use super::*;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone)]
pub(crate) struct Checkout {
    pub target: RemoteCheckoutTarget,
    pub name: String,
}
#[derive(Default)]
struct Mapping {
    positive: Vec<(String, String)>,
    negative: Vec<String>,
}
fn substitute(pattern: &str, value: &str) -> Option<String> {
    match pattern.split_once('*') {
        Some((prefix, suffix)) => value
            .strip_prefix(prefix)?
            .strip_suffix(suffix)
            .map(str::to_owned),
        None => (pattern == value).then(String::new),
    }
}
fn configuration(bytes: &[u8]) -> AppResult<BTreeMap<String, Mapping>> {
    let mut remotes: BTreeMap<String, Mapping> = BTreeMap::new();
    for record in bytes.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let Some(split) = record.iter().position(|b| *b == b'\n') else {
            continue;
        };
        let Ok(key) = std::str::from_utf8(&record[..split]) else {
            continue;
        };
        let Some(key) = key.strip_prefix("remote.") else {
            continue;
        };
        let Some(name) = key.strip_suffix(".fetch") else {
            continue;
        };
        let spec = std::str::from_utf8(&record[split + 1..]).map_err(|_| {
            AppError::new("unsupported_refspec", "A fetch refspec is not valid UTF-8.")
        })?;
        let mapping = remotes.entry(name.to_owned()).or_default();
        if let Some(source) = spec.strip_prefix('^') {
            mapping.negative.push(source.into());
        } else if let Some((source, destination)) = spec.trim_start_matches('+').split_once(':') {
            if source.matches('*').count() > 1
                || destination.matches('*').count() > 1
                || source.contains('*') != destination.contains('*')
            {
                return Err(AppError::new(
                    "unsupported_refspec",
                    "The configured fetch refspec cannot be mapped unambiguously.",
                ));
            }
            mapping.positive.push((source.into(), destination.into()));
        }
    }
    Ok(remotes)
}
impl GitEngine {
    pub async fn list_remote_branches(
        &self,
        repository_id: &str,
    ) -> AppResult<RemoteBranchListing> {
        let repo = self.repository(repository_id).await?;
        self.remote_branches(&repo).await
    }
    async fn remote_branches(&self, repo: &Repository) -> AppResult<RemoteBranchListing> {
        let config = self
            .runner
            .read(&repo.root, &["config", "--null", "--list"])
            .await?;
        // Ask Git to reject malformed configured refspecs before interpreting mappings.
        self.runner.read(&repo.root, &["remote"]).await?;
        let mappings = configuration(&config)?;
        let bytes = self
            .runner
            .read(
                &repo.root,
                &[
                    "--no-replace-objects",
                    "for-each-ref",
                    "--format=%(refname)%00%(objectname)%00%(symref)%00%(objecttype)",
                    "refs/",
                ],
            )
            .await?;
        let mut result = RemoteBranchListing {
            targets: Vec::new(),
            issues: Vec::new(),
        };
        for row in bytes.split(|b| *b == b'\n').filter(|r| !r.is_empty()) {
            let fields: Vec<_> = row.split(|b| *b == 0).collect();
            if fields.len() != 4 {
                return Err(AppError::new(
                    "invalid_git_output",
                    "Git returned an invalid reference record.",
                ));
            }
            let Ok(reference) = std::str::from_utf8(fields[0]) else {
                result.issues.push(RemoteBranchIssue {
                    source_ref: text(fields[0]),
                    reason: "This reference cannot be represented safely as UTF-8.".into(),
                });
                continue;
            };
            if !fields[2].is_empty() {
                continue;
            }
            let mut owners = Vec::new();
            for (remote, mapping) in &mappings {
                for (source, destination) in &mapping.positive {
                    if let Some(capture) = substitute(destination, reference) {
                        let upstream = source.replace('*', &capture);
                        if !mapping
                            .negative
                            .iter()
                            .any(|negative| substitute(negative, &upstream).is_some())
                        {
                            owners.push((remote.clone(), upstream));
                        }
                    }
                }
            }
            owners.sort();
            owners.dedup();
            if owners.is_empty() && !reference.starts_with("refs/remotes/") {
                continue;
            }
            let mut destinations = Vec::new();
            if let [(remote, upstream)] = owners.as_slice() {
                for (source, destination) in &mappings[remote].positive {
                    if let Some(capture) = substitute(source, upstream) {
                        destinations.push(destination.replace('*', &capture));
                    }
                }
                destinations.sort();
                destinations.dedup();
            }
            let reason = if owners.len() != 1 {
                Some("No unique configured remote fetch mapping owns this reference.")
            } else if destinations.len() != 1 {
                Some("The remote branch maps to multiple local tracking references; its upstream is ambiguous.")
            } else if !owners[0].1.starts_with("refs/heads/")
                || reference.starts_with("refs/heads/")
                || reference.starts_with("refs/tags/")
                || reference.starts_with("refs/git-view/")
            {
                Some("This fetch destination is not a supported remote branch reference.")
            } else if fields[3] != b"commit" {
                Some("The reference does not point directly to a commit.")
            } else {
                None
            };
            if let Some(reason) = reason {
                result.issues.push(RemoteBranchIssue {
                    source_ref: reference.into(),
                    reason: reason.into(),
                });
            } else {
                let (remote, upstream) = owners.pop().unwrap();
                result.targets.push(RemoteCheckoutTarget {
                    remote_id: remote.clone(),
                    remote_name: remote,
                    source_ref: reference.into(),
                    source_oid: text(fields[1]),
                    suggested_name: upstream.strip_prefix("refs/heads/").unwrap().into(),
                    upstream_ref: upstream,
                });
            }
        }
        if digest(&config) != self.config_fingerprint(repo).await? {
            return Err(changed(
                "Git fetch configuration changed during enumeration.",
            ));
        }
        Ok(result)
    }
    pub(crate) async fn prepare_remote_checkout(
        &self,
        repo: &Repository,
        remote_id: &str,
        source_ref: &str,
        name: &str,
    ) -> AppResult<Checkout> {
        if name.starts_with('-') || name.is_empty() || name.contains(['\0', '\n', '\r']) {
            return Err(AppError::new(
                "invalid_branch",
                "Enter a literal valid local branch name.",
            ));
        }
        let checked = self
            .runner
            .read(&repo.root, &["check-ref-format", "--branch", name])
            .await?;
        if text(&checked) != name {
            return Err(AppError::new(
                "invalid_branch",
                "Branch expansion expressions are not accepted.",
            ));
        }
        self.check_destination(repo, name).await?;
        let listing = self.remote_branches(repo).await?;
        let target = listing
            .targets
            .into_iter()
            .find(|target| target.remote_id == remote_id && target.source_ref == source_ref)
            .ok_or_else(|| {
                AppError::new(
                    "invalid_remote_branch",
                    "Select a direct remote branch with a unique effective fetch mapping.",
                )
            })?;
        Ok(Checkout {
            target,
            name: name.into(),
        })
    }
    async fn check_destination(&self, repo: &Repository, name: &str) -> AppResult<()> {
        let destination = format!("refs/heads/{name}");
        let refs = self
            .runner
            .read(
                &repo.root,
                &["for-each-ref", "--format=%(refname)", "refs/heads/"],
            )
            .await?;
        let collision = text(&refs).lines().any(|reference| {
            reference == destination
                || reference.starts_with(&format!("{destination}/"))
                || destination.starts_with(&format!("{reference}/"))
        });
        let worktrees = self
            .runner
            .read(&repo.root, &["worktree", "list", "--porcelain", "-z"])
            .await?;
        let occupied = worktrees
            .split(|b| *b == 0)
            .any(|field| field == format!("branch {destination}").as_bytes());
        let config = self
            .runner
            .read(&repo.root, &["config", "--null", "--list"])
            .await?;
        let section = format!("branch.{name}");
        let configured = config.split(|b| *b == 0).any(|row| {
            let key = row.split(|b| *b == b'\n').next().unwrap_or_default();
            std::str::from_utf8(key)
                .ok()
                .and_then(|key| key.rsplit_once('.'))
                .is_some_and(|(subsection, _)| subsection == section)
        });
        if collision || occupied || configured {
            return Err(AppError::new("branch_exists", "The local branch name, its namespace, or its tracking configuration already exists. Choose another name or switch to the existing branch."));
        }
        Ok(())
    }
    pub(crate) async fn execute_remote_checkout(
        &self,
        plan: &PreparedAction,
        checkout: &Checkout,
        canceled: &AtomicBool,
    ) -> AppResult<ActionOutcome> {
        let repo = &plan.repository;
        let guard = ref_guard::HeadGuard::prepare(&self.runner, &repo.root, &[], format!("option no-deref\nverify {} {}\n", checkout.target.source_ref, checkout.target.source_oid)).await.map_err(|_| changed("The source reference could not be locked at its reviewed commit. Refresh before retrying."))?;
        let preflight = async {
            let current = self
                .prepare_remote_checkout(
                    repo,
                    &checkout.target.remote_id,
                    &checkout.target.source_ref,
                    &checkout.name,
                )
                .await?;
            if current.target.source_oid != checkout.target.source_oid
                || current.target.upstream_ref != checkout.target.upstream_ref
                || self.config_fingerprint(repo).await? != plan.config_fingerprint
                || Some(self.observe(repo).await?.fingerprint) != plan.fingerprint
            {
                return Err(changed(
                    "The source, configuration, or working tree changed after preview.",
                ));
            }
            if canceled.load(Ordering::Acquire) {
                return Err(AppError::new(
                    "canceled",
                    "Operation canceled before Git started.",
                ));
            }
            Ok(())
        }
        .await;
        if let Err(error) = preflight {
            guard.release().await?;
            return Err(error);
        }
        let outcome = self
            .runner
            .run(&repo.root, &plan.args, None, true, runner::DIAGNOSTIC_LIMIT)
            .await;
        let release = guard.release().await;
        // A post-checkout hook can fail after switching, or change the resulting branch.
        // Inspect every started outcome; never delete or reset the partially created branch.
        let inspection = self.inspect_checkout(repo, checkout).await;
        let observed = match &inspection {
            Ok(description) => description.clone(),
            Err(error) => format!("Final state could not be verified: {}", error.message),
        };
        let output = outcome.and_then(runner::checked).map_err(|mut error| {
            error.message = format!(
                "{} Checkout may have partially completed; refresh before retrying. {observed}",
                error.message
            );
            if let Err(release_error) = &release {
                error.code = "outcome_uncertain".into();
                error.message.push_str(&format!(
                    " Source reference guard also failed to release: {}",
                    release_error.message
                ));
            }
            error
        })?;
        release.map_err(|mut error| {
            error.message = format!("{} {observed}", error.message);
            error
        })?;
        inspection.map_err(|mut error| {
            error.exit_code = Some(output.code);
            error.output_truncated = Some(output.truncated);
            error
        })?;
        Ok(ActionOutcome {
            exit_code: output.code,
            output_truncated: output.truncated,
        })
    }
    async fn inspect_checkout(&self, repo: &Repository, checkout: &Checkout) -> AppResult<String> {
        let observed = self.observe(repo).await?;
        let status = observed.status.unwrap();
        let local = self
            .runner
            .read(
                &repo.root,
                &[
                    "--no-replace-objects",
                    "for-each-ref",
                    "--format=%(refname)%00%(objectname)%00%(symref)%00%(upstream)",
                    &format!("refs/heads/{}", checkout.name),
                ],
            )
            .await?;
        let expected = format!(
            "refs/heads/{}\0{}\0\0{}",
            checkout.name, checkout.target.source_oid, checkout.target.source_ref
        );
        let config = self
            .runner
            .read(&repo.root, &["config", "--null", "--list"])
            .await?;
        let values = |key: String| -> Vec<Vec<u8>> {
            config
                .split(|b| *b == 0)
                .filter_map(|row| {
                    row.strip_prefix(format!("{key}\n").as_bytes())
                        .map(<[u8]>::to_vec)
                })
                .collect()
        };
        let tracking = values(format!("branch.{}.remote", checkout.name))
            == vec![checkout.target.remote_name.as_bytes().to_vec()]
            && values(format!("branch.{}.merge", checkout.name))
                == vec![checkout.target.upstream_ref.as_bytes().to_vec()];
        let description = format!(
            "Observed HEAD: {:?}; local reference: {}.",
            status.head,
            text(&local).replace('\0', " ")
        );
        if status.head
            != (Head::Branch {
                name: checkout.name.clone(),
                oid: checkout.target.source_oid.clone(),
            })
            || text(&local) != expected
            || !tracking
            || status.dirty
            || status.git_state != GitState::Idle
        {
            return Err(AppError::new("checkout_incomplete", format!("Checkout did not leave the reviewed branch, tracking configuration, and clean working tree. No rollback was attempted. {description}")));
        }
        Ok(description)
    }
}
fn changed(message: &str) -> AppError {
    AppError::new("precondition_changed", message)
}
