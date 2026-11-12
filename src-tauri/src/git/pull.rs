//! Single-repository pull with a chosen integration strategy.
//!
//! Preview fetches the upstream remote, then plans integration of the exact
//! fetched upstream commit: fast-forward when the branch is only behind, or the
//! chosen merge/rebase when it has diverged. Execution never fetches again, so
//! the reviewed upstream commit is the one integrated.
use super::*;
use std::collections::HashSet;

pub(crate) struct PullTarget {
    pub branch: String,
    pub head_oid: String,
    pub remote: String,
    /// Full remote-tracking ref, for example `refs/remotes/origin/main`.
    pub tracking_ref: String,
}

pub(crate) struct PullIntegration {
    pub preview: PullPreview,
    pub args: Vec<OsString>,
    pub summary: String,
    pub destination: String,
}

/// Untracked files never block pull; Git and the touched-path check protect them.
pub(crate) fn tracked_dirty(status: &RepositoryStatus) -> bool {
    status.counts.staged + status.counts.unstaged + status.counts.conflicted > 0
        || status.submodule_dirty
}

impl GitEngine {
    /// Validates the pull preconditions shared by every strategy.
    pub(crate) async fn pull_target(
        &self,
        repo: &Repository,
        status: &RepositoryStatus,
    ) -> AppResult<PullTarget> {
        if tracked_dirty(status) {
            return Err(AppError::new(
                "dirty",
                "Working tree has staged, modified or conflicted files; resolve them before pulling. Untracked files alone do not block pull.",
            ));
        }
        if status.git_state != GitState::Idle {
            return Err(AppError::new(
                "operation_in_progress",
                "Finish the ongoing Git operation first.",
            ));
        }
        let Head::Branch { name: branch, oid } = &status.head else {
            return Err(AppError::new(
                "invalid_head",
                "Pull requires an attached, existing branch.",
            ));
        };
        let upstream = status.upstream.as_ref().ok_or_else(|| {
            AppError::new(
                "missing_upstream",
                "The current branch has no available upstream.",
            )
        })?;
        let merge = text(
            &self
                .runner
                .read(
                    &repo.root,
                    &["config", "--get-all", &format!("branch.{branch}.merge")],
                )
                .await?,
        );
        if merge.lines().count() != 1
            || upstream.remote.is_empty()
            || !merge.starts_with("refs/heads/")
        {
            return Err(AppError::new(
                "unsupported",
                "Pull requires exactly one configured upstream branch on a named remote.",
            ));
        }
        let tracking_ref = text(
            &self
                .runner
                .read(
                    &repo.root,
                    &[
                        "for-each-ref",
                        "--format=%(upstream)",
                        &format!("refs/heads/{branch}"),
                    ],
                )
                .await?,
        );
        if !tracking_ref.starts_with("refs/remotes/") || tracking_ref.ends_with("/HEAD") {
            return Err(AppError::new(
                "unsupported",
                "Pull requires the upstream to be a remote-tracking branch.",
            ));
        }
        Ok(PullTarget {
            branch: branch.clone(),
            head_oid: oid.clone(),
            remote: upstream.remote.clone(),
            tracking_ref,
        })
    }

    /// Fetches the upstream remote for a pull preview and returns a fresh status
    /// snapshot. The branch and HEAD must be unchanged by the fetch.
    pub(crate) async fn fetch_for_pull(
        &self,
        repository_id: &str,
        status_snapshot_id: &str,
    ) -> AppResult<String> {
        let repo = self.repository(repository_id).await?;
        let observed = self.observe(&repo).await?;
        let snapshot = self.snapshot(repository_id, status_snapshot_id).await?;
        if snapshot.status.is_none() || snapshot.fingerprint != observed.fingerprint {
            return Err(AppError::new(
                "precondition_changed",
                "Repository changed since preview; refresh and try again.",
            ));
        }
        let status = observed.status.as_ref().unwrap();
        let target = self.pull_target(&repo, status).await?;
        {
            let _guard = repo.lock.lock().await;
            runner::checked(
                self.runner
                    .run(
                        &repo.root,
                        &runner::args(&[
                            "fetch",
                            "--no-prune",
                            "--no-recurse-submodules",
                            "--",
                            &target.remote,
                        ]),
                        None,
                        true,
                        runner::DIAGNOSTIC_LIMIT,
                    )
                    .await?,
            )
            .map_err(|mut error| {
                error.message = format!(
                    "Could not fetch {} before pulling. {}",
                    target.remote, error.message
                );
                error
            })?;
            self.state
                .lock()
                .await
                .fetch_times
                .insert(repo.id.clone(), now());
        }
        let fresh = self.get_status(repository_id).await?;
        if fresh.head != status.head {
            return Err(AppError::new(
                "precondition_changed",
                "The current branch changed while fetching. Review the repository and pull again.",
            ));
        }
        Ok(fresh.snapshot_id)
    }

    async fn is_ancestor(
        &self,
        repo: &Repository,
        ancestor: &str,
        descendant: &str,
    ) -> AppResult<bool> {
        let output = self
            .runner
            .run(
                &repo.root,
                &runner::args(&["merge-base", "--is-ancestor", ancestor, descendant]),
                None,
                false,
                runner::STRUCTURED_LIMIT,
            )
            .await?;
        match output.code {
            0 => Ok(true),
            1 => Ok(false),
            _ => runner::checked(output).map(|_| false),
        }
    }

    async fn count(&self, repo: &Repository, range: &str) -> AppResult<u32> {
        text(
            &self
                .runner
                .read(&repo.root, &["rev-list", "--count", range])
                .await?,
        )
        .parse()
        .map_err(|_| AppError::new("git_failed", "Git returned an invalid commit count."))
    }

    /// Paths a fast-forward or merge from `base` to `upstream` can write.
    async fn changed_paths(
        &self,
        repo: &Repository,
        base: &str,
        upstream: &str,
    ) -> AppResult<HashSet<Vec<u8>>> {
        Ok(self
            .runner
            .read(
                &repo.root,
                &[
                    "--no-replace-objects",
                    "diff",
                    "--name-only",
                    "--no-renames",
                    "-z",
                    base,
                    upstream,
                ],
            )
            .await?
            .split(|b| *b == 0)
            .filter(|p| !p.is_empty())
            .map(<[u8]>::to_vec)
            .collect())
    }

    /// Plans how the fetched upstream is integrated. Returns `None` for args
    /// when the guided rebase engine must prepare the rebase.
    pub(crate) async fn prepare_pull_integration(
        &self,
        repo: &Repository,
        status: &RepositoryStatus,
        mode: PullMode,
    ) -> AppResult<PullIntegration> {
        let target = self.pull_target(repo, status).await?;
        let upstream_oid = self.resolve_commit(repo, &target.tracking_ref).await?;
        let short = target
            .tracking_ref
            .strip_prefix("refs/remotes/")
            .unwrap_or(&target.tracking_ref)
            .to_owned();
        let head = &target.head_oid;
        if self.is_ancestor(repo, &upstream_oid, head).await? {
            return Err(AppError::new(
                "already_up_to_date",
                format!(
                    "{} already includes {short}. Nothing to pull.",
                    target.branch
                ),
            ));
        }
        let incoming = self.count(repo, &format!("{head}..{upstream_oid}")).await?;
        let local = self.count(repo, &format!("{upstream_oid}..{head}")).await?;
        let fast_forward = self.is_ancestor(repo, head, &upstream_oid).await?;
        let strategy = if fast_forward {
            PullMode::FastForward
        } else if mode == PullMode::FastForward {
            return Err(AppError::new(
                "diverged",
                format!(
                    "{} has {local} local commit(s) not on {short}, which has {incoming} new commit(s). Choose Pull with rebase or Pull with merge to combine them.",
                    target.branch
                ),
            ));
        } else {
            mode
        };
        let preview = PullPreview {
            mode,
            strategy,
            branch: target.branch.clone(),
            head_oid: head.clone(),
            upstream_ref: target.tracking_ref.clone(),
            upstream_oid: upstream_oid.clone(),
            remote: target.remote.clone(),
            incoming,
            local,
            fetched_at: self
                .state
                .lock()
                .await
                .fetch_times
                .get(&repo.id)
                .cloned()
                .unwrap_or_else(now),
        };
        let destination = format!("{short} → {}", target.branch);
        let branch = &target.branch;
        let common = [
            "-c".to_owned(),
            format!("branch.{branch}.mergeOptions="),
            "-c".into(),
            "merge.autoStash=false".into(),
            "-c".into(),
            "merge.directoryRenames=false".into(),
            "-c".into(),
            "submodule.recurse=false".into(),
        ];
        let (args, summary) = match strategy {
            PullMode::FastForward => {
                let paths = self.changed_paths(repo, head, &upstream_oid).await?;
                self.protect_materialized_paths(repo, &paths).await?;
                let mut args: Vec<OsString> = common.iter().map(Into::into).collect();
                args.extend(runner::args(&[
                    "merge",
                    "--ff-only",
                    "--no-autostash",
                    "--",
                    &upstream_oid,
                ]));
                (args, format!("Fast-forward {branch} from {head} to {short} at {upstream_oid} ({incoming} new commit(s)). No local commits are rewritten."))
            }
            PullMode::Merge => {
                let base = text(
                    &self
                        .runner
                        .read(&repo.root, &["merge-base", head, &upstream_oid])
                        .await?,
                );
                let paths = self.changed_paths(repo, &base, &upstream_oid).await?;
                self.protect_materialized_paths(repo, &paths).await?;
                let mut args: Vec<OsString> = common.iter().map(Into::into).collect();
                args.extend(runner::args(&[
                    "merge",
                    "--no-edit",
                    "--no-autostash",
                    "--no-ff",
                    "--no-squash",
                    "--commit",
                    "-m",
                    &format!("Merge remote-tracking branch '{short}'"),
                    "--",
                    &upstream_oid,
                ]));
                (args, format!("Merge {short} at {upstream_oid} ({incoming} new commit(s)) into {branch} at {head} ({local} local commit(s)) with a merge commit. Conflicts stop for resolution; Abort merge restores {branch}."))
            }
            // The guided rebase engine builds its own arguments and preview.
            PullMode::Rebase => (Vec::new(), String::new()),
        };
        Ok(PullIntegration {
            preview,
            args,
            summary,
            destination,
        })
    }
}
