use super::*;
use std::collections::BTreeMap;
#[derive(Clone)]
pub(crate) struct FrozenConfig {
    pub values: BTreeMap<String, Vec<OsString>>,
    pub hooks: OsString,
}
fn frozen_key(key: &str) -> bool {
    key.starts_with("user.")
        || key.starts_with("author.")
        || key.starts_with("committer.")
        || key.starts_with("gpg.")
        || key.starts_with("commit.")
        || key.starts_with("i18n.")
        || matches!(
            key,
            "core.hookspath" | "core.commentchar" | "core.commentstring"
        )
}
impl GitEngine {
    async fn config_values(&self, root: &Path) -> AppResult<BTreeMap<String, Vec<OsString>>> {
        let bytes = self
            .runner
            .read(root, &["config", "--null", "--list"])
            .await?;
        let mut values = BTreeMap::new();
        for record in bytes
            .split(|byte| *byte == 0)
            .filter(|part| !part.is_empty())
        {
            let end = record
                .iter()
                .position(|byte| *byte == b'\n')
                .unwrap_or(record.len());
            let key = String::from_utf8_lossy(&record[..end]).to_lowercase();
            let value = if end < record.len() {
                path_bytes(&record[end + 1..])
            } else {
                OsString::from("true")
            };
            values.entry(key).or_insert_with(Vec::new).push(value);
        }
        Ok(values)
    }
    pub(crate) async fn freeze_config(&self, repo: &Repository) -> AppResult<FrozenConfig> {
        let values = self.config_values(&repo.root).await?;
        let sparse = self
            .runner
            .run(
                &repo.root,
                &runner::args(&["config", "--bool", "--get", "core.sparseCheckout"]),
                None,
                false,
                runner::STRUCTURED_LIMIT,
            )
            .await?;
        if sparse.code != 1 && text(&runner::checked(sparse)?.stdout) == "true" {
            return Err(AppError::new(
                "unsupported_configuration",
                "Combining commits in a sparse worktree is not supported yet.",
            ));
        }
        if values
            .get("gpg.format")
            .and_then(|v| v.last())
            .is_some_and(|v| v == "ssh")
            && values.contains_key("gpg.ssh.defaultkeycommand")
            && !values.contains_key("user.signingkey")
        {
            return Err(AppError::new("unsupported_configuration", "SSH defaultKeyCommand may select a different identity in a detached worktree. Configure an explicit user.signingKey before combining commits."));
        }
        let hooks = self
            .runner
            .read(
                &repo.root,
                &["rev-parse", "--path-format=absolute", "--git-path", "hooks"],
            )
            .await?;
        let hooks = path_bytes(hooks.strip_suffix(b"\n").unwrap_or(&hooks));
        Ok(FrozenConfig { values, hooks })
    }
    pub(crate) async fn candidate_config(
        &self,
        repo: &Repository,
        path: &Path,
        frozen: &FrozenConfig,
    ) -> AppResult<Vec<OsString>> {
        let candidate = self.config_values(path).await?;
        for key in frozen.values.keys().chain(candidate.keys()) {
            if frozen.values.get(key) != candidate.get(key)
                && !frozen_key(key)
                && !matches!(key.as_str(), "core.worktree" | "core.bare")
            {
                return Err(AppError::new("unsupported_configuration",format!("Git configuration {key} differs in the detached candidate worktree. The original branch was not rewritten.")));
            }
        }
        let mut args = Vec::new();
        for (key, values) in &frozen.values {
            if frozen_key(key) && key != "core.hookspath" {
                // Effective Git configuration is last-value-wins for these commit policy keys.
                if let Some(value) = values.last() {
                    let mut value = value.clone();
                    if (key.ends_with(".program") || key == "gpg.program")
                        && Path::new(&value).components().count() > 1
                        && !Path::new(&value).is_absolute()
                    {
                        value = repo.root.join(value).into_os_string();
                    }
                    if matches!(
                        key.as_str(),
                        "gpg.ssh.allowedsignersfile" | "gpg.ssh.revocationfile"
                    ) {
                        let expanded = self
                            .runner
                            .read(&repo.root, &["config", "--path", "--get", key])
                            .await?;
                        let expanded =
                            path_bytes(expanded.strip_suffix(b"\n").unwrap_or(&expanded));
                        value = if Path::new(&expanded).is_absolute() {
                            expanded
                        } else {
                            repo.root.join(expanded).into_os_string()
                        };
                    }
                    if key == "user.signingkey"
                        && frozen
                            .values
                            .get("gpg.format")
                            .and_then(|v| v.last())
                            .is_some_and(|v| v == "ssh")
                        && !value.to_string_lossy().starts_with("key::")
                    {
                        let expanded = self
                            .runner
                            .read(&repo.root, &["config", "--path", "--get", key])
                            .await?;
                        let expanded =
                            path_bytes(expanded.strip_suffix(b"\n").unwrap_or(&expanded));
                        value = if Path::new(&expanded).is_absolute() {
                            expanded
                        } else {
                            repo.root.join(expanded).into_os_string()
                        };
                    }
                    let mut pair = OsString::from(format!("{key}="));
                    pair.push(value);
                    args.push("-c".into());
                    args.push(pair);
                }
            }
        }
        // A detached conditional include may enable a policy absent in the original.
        // Explicitly reject that context rather than guessing an inverse override.
        for key in candidate.keys() {
            if frozen_key(key) && !frozen.values.contains_key(key) && key != "core.hookspath" {
                return Err(AppError::new(
                    "unsupported_configuration",
                    format!("Candidate-only Git policy {key} requires manual review."),
                ));
            }
        }
        let mut hooks = OsString::from("core.hooksPath=");
        hooks.push(&frozen.hooks);
        args.push("-c".into());
        args.push(hooks);
        if args.iter().map(|v| os_bytes(v).len() + 1).sum::<usize>() > 16 * 1024 {
            return Err(AppError::new(
                "unsupported_configuration",
                "Effective commit settings exceed the safe cross-platform command length.",
            ));
        }
        Ok(args)
    }
}
