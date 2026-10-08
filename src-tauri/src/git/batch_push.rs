use super::*;

impl GitEngine {
    // Deliberately separate from repository-screen defaults: a group never guesses
    // a destination, configures an upstream, or silently uses a triangular remote.
    pub(crate) async fn prepare_upstream_push(
        &self,
        repository_id: &str,
        status: &RepositoryStatus,
    ) -> AppResult<PreparedAction> {
        let repo = self.repository(repository_id).await?;
        let _guard = repo.lock.try_lock().map_err(|_| {
            AppError::new(
                "repository_busy",
                "Another operation is using this repository.",
            )
        })?;
        let config = self.config_fingerprint(&repo).await?;
        let (branch, oid) = match &status.head {
            Head::Branch { name, oid } => (name, oid),
            _ => {
                return Err(AppError::new(
                    "invalid_head",
                    "Group push requires an attached branch with a commit.",
                ))
            }
        };
        if status.upstream.is_none() {
            return Err(AppError::new("missing_upstream", "The configured upstream is unavailable locally. Fetch and inspect this repository before group push."));
        }
        let remote = self
            .single_push_config(&repo, &format!("branch.{branch}.remote"))
            .await?
            .filter(|value| !value.is_empty() && value != ".")
            .ok_or_else(|| {
                AppError::new(
                    "missing_upstream",
                    "Configure one named-remote upstream in the repository before group push.",
                )
            })?;
        let destination = self
            .single_push_config(&repo, &format!("branch.{branch}.merge"))
            .await?
            .filter(|value| value.starts_with("refs/heads/"))
            .ok_or_else(|| {
                AppError::new(
                    "missing_upstream",
                    "Group push requires one configured upstream branch.",
                )
            })?;
        let push_remote = self
            .single_push_config(&repo, &format!("branch.{branch}.pushRemote"))
            .await?
            .or(self.single_push_config(&repo, "remote.pushDefault").await?);
        if push_remote.as_ref().is_some_and(|value| value != &remote) {
            return Err(AppError::new("unsupported_push", "The configured push remote differs from the upstream remote. Review this triangular workflow in the repository screen."));
        }
        let mut plan = self
            .prepare_action(
                repository_id,
                RepositoryAction::Push {
                    status_snapshot_id: status.snapshot_id.clone(),
                    remote_id: remote.clone(),
                    destination_ref: destination.clone(),
                    set_upstream: false,
                },
            )
            .await?;
        if self.config_fingerprint(&repo).await? != config || plan.config_fingerprint != config {
            return Err(AppError::new(
                "precondition_changed",
                "Git configuration changed while resolving the upstream. Preview again.",
            ));
        }
        // A hook or external process can move the branch after preflight. Pin the
        // sent object to the reviewed commit, rather than resolving the branch again.
        *plan.args.last_mut().expect("push has an explicit refspec") =
            format!("{oid}:{destination}").into();
        plan.args
            .splice(0..0, runner::args(&["-c", "push.autoSetupRemote=false"]));
        plan.summary = format!("Push refs/heads/{branch} at {oid} to {remote} {destination}. Committed content only; uncommitted changes stay local. No force push or upstream changes.");
        Ok(plan)
    }

    async fn single_push_config(&self, repo: &Repository, key: &str) -> AppResult<Option<String>> {
        let output = self
            .runner
            .run(
                &repo.root,
                &runner::args(&["config", "--null", "--get-all", key]),
                None,
                false,
                runner::STRUCTURED_LIMIT,
            )
            .await?;
        if output.code == 1 {
            return Ok(None);
        }
        let output = runner::checked(output)?;
        let bytes = output.stdout.strip_suffix(&[0]).unwrap_or(&output.stdout);
        let values: Vec<_> = bytes.split(|b| *b == 0).collect();
        if values.len() != 1
            || values[0].is_empty()
            || values[0].iter().any(|b| b.is_ascii_control())
        {
            return Err(AppError::new(
                "unsupported_push",
                "Group push requires one unambiguous upstream and push-remote configuration.",
            ));
        }
        String::from_utf8(values[0].to_vec())
            .map(Some)
            .map_err(|_| {
                AppError::new(
                    "unsupported_push",
                    "Upstream configuration is not valid UTF-8.",
                )
            })
    }
}
