use super::*;
impl GitEngine {
    pub(crate) async fn config_fingerprint(&self, repo: &Repository) -> AppResult<String> {
        Ok(digest(
            &runner::read(&repo.root, &["config", "--null", "--list"]).await?,
        ))
    }
    pub async fn prepare_action(
        &self,
        repository_id: &str,
        action: RepositoryAction,
    ) -> AppResult<PreparedAction> {
        let repo = self.repository(repository_id).await?;
        let observed = self.observe(&repo).await?;
        let status = observed.status.as_ref().unwrap();
        let args;
        let mut input = None;
        let mut destination = None;
        let mut target_ref = None;
        let snapshot_id = match &action {
            RepositoryAction::Stage { snapshot_id, .. }
            | RepositoryAction::Unstage { snapshot_id, .. } => Some(snapshot_id),
            RepositoryAction::Commit {
                status_snapshot_id, ..
            }
            | RepositoryAction::Pull { status_snapshot_id }
            | RepositoryAction::Push {
                status_snapshot_id, ..
            }
            | RepositoryAction::CreateBranch {
                status_snapshot_id, ..
            }
            | RepositoryAction::SwitchBranch {
                status_snapshot_id, ..
            }
            | RepositoryAction::Merge {
                status_snapshot_id, ..
            }
            | RepositoryAction::SquashMerge {
                status_snapshot_id, ..
            }
            | RepositoryAction::ContinueMerge { status_snapshot_id }
            | RepositoryAction::AbortMerge { status_snapshot_id } => Some(status_snapshot_id),
            RepositoryAction::Fetch { .. } => None,
        };
        if let Some(snapshot_id) = snapshot_id {
            let snapshot = self.snapshot(repository_id, snapshot_id).await?;
            if snapshot.status.is_none() || snapshot.fingerprint != observed.fingerprint {
                return Err(AppError::new(
                    "precondition_changed",
                    "Repository changed since preview; refresh and try again.",
                ));
            }
        }
        match &action {
            RepositoryAction::Fetch { remote_id } => {
                let remotes = self.list_remotes(repository_id).await?;
                let remote = remotes
                    .iter()
                    .find(|r| r.id == *remote_id)
                    .ok_or_else(|| AppError::new("invalid_remote", "Select an existing remote."))?;
                args = runner::args(&[
                    "fetch",
                    "--no-prune",
                    "--no-recurse-submodules",
                    "--",
                    &remote.name,
                ]);
                destination = Some(remote.name.clone());
            }
            RepositoryAction::Pull { .. } => {
                if status.dirty {
                    return Err(AppError::new(
                        "dirty",
                        "Working tree has changes; resolve them before pulling.",
                    ));
                }
                if status.git_state != GitState::Idle {
                    return Err(AppError::new(
                        "operation_in_progress",
                        "Finish the ongoing Git operation first.",
                    ));
                }
                let branch = if let Head::Branch { name, .. } = &status.head {
                    name
                } else {
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
                    &runner::read(
                        &repo.root,
                        &["config", "--get-all", &format!("branch.{branch}.merge")],
                    )
                    .await?,
                );
                if merge.lines().count() != 1 {
                    return Err(AppError::new(
                        "unsupported",
                        "Pull requires exactly one configured upstream branch.",
                    ));
                }
                if upstream.remote.is_empty() || !merge.starts_with("refs/heads/") {
                    return Err(AppError::new(
                        "unsupported",
                        "Pull requires one branch upstream.",
                    ));
                }
                destination = Some(format!("{} → {branch}", upstream.reference));
                args = runner::args(&[
                    "-c",
                    "pull.rebase=false",
                    "-c",
                    "rebase.autoStash=false",
                    "-c",
                    "merge.autoStash=false",
                    "pull",
                    "--ff-only",
                    "--no-rebase",
                    "--no-autostash",
                    "--no-recurse-submodules",
                    "--",
                    &upstream.remote,
                    &merge,
                ]);
            }
            RepositoryAction::Stage {
                snapshot_id,
                file_ids,
            }
            | RepositoryAction::Unstage {
                snapshot_id,
                file_ids,
            } => {
                if status.git_state != GitState::Idle {
                    return Err(AppError::new(
                        "operation_in_progress",
                        "This operation is not available during an ongoing Git operation.",
                    ));
                }
                let snapshot = self.snapshot(repository_id, snapshot_id).await?;
                if file_ids.is_empty() {
                    return Err(AppError::new(
                        "invalid_selection",
                        "Select at least one file.",
                    ));
                }
                let mut paths = Vec::new();
                for file_id in file_ids {
                    let file = snapshot
                        .files
                        .iter()
                        .find(|f| f.wire.file_id == *file_id)
                        .ok_or_else(|| {
                            AppError::new(
                                "stale_snapshot",
                                "The selected file is no longer available.",
                            )
                        })?;
                    for path in std::iter::once(&file.path).chain(file.old_path.iter()) {
                        paths.extend_from_slice(&os_bytes(path));
                        paths.push(0);
                    }
                }
                args = match &action {
                    RepositoryAction::Stage { .. } => runner::args(&[
                        "--literal-pathspecs",
                        "add",
                        "--all",
                        "--pathspec-from-file=-",
                        "--pathspec-file-nul",
                    ]),
                    _ => {
                        if matches!(status.head, Head::Unborn { .. }) {
                            runner::args(&[
                                "--literal-pathspecs",
                                "rm",
                                "--cached",
                                "-r",
                                "--ignore-unmatch",
                                "--pathspec-from-file=-",
                                "--pathspec-file-nul",
                            ])
                        } else {
                            runner::args(&[
                                "--literal-pathspecs",
                                "restore",
                                "--staged",
                                "--source=HEAD",
                                "--pathspec-from-file=-",
                                "--pathspec-file-nul",
                            ])
                        }
                    }
                };
                input = Some(paths);
            }
            RepositoryAction::Commit { message, .. } => {
                if status.git_state != GitState::Idle || status.counts.conflicted > 0 {
                    return Err(AppError::new(
                        "operation_in_progress",
                        "Resolve the ongoing Git operation first.",
                    ));
                }
                if message.trim().is_empty() {
                    return Err(AppError::new(
                        "invalid_message",
                        "A nonblank commit message is required.",
                    ));
                }
                if status.counts.staged == 0 {
                    return Err(AppError::new(
                        "nothing_staged",
                        "Stage changes before committing.",
                    ));
                }
                args = runner::args(&["commit", "--file=-"]);
                input = Some(message.as_bytes().to_vec());
            }
            RepositoryAction::CreateBranch {
                name,
                start_oid,
                switch_after_create,
                ..
            } => {
                self.require_clean(status)?;
                if name.starts_with('-')
                    || text(
                        &runner::read(&repo.root, &["check-ref-format", "--branch", name]).await?,
                    ) != *name
                {
                    return Err(AppError::new(
                        "invalid_branch",
                        "Enter a literal valid branch name.",
                    ));
                }
                let oid = self.resolve_commit(&repo, start_oid).await?;
                args = if *switch_after_create {
                    runner::args(&["switch", "--no-guess", "-c", name, &oid])
                } else {
                    runner::args(&["branch", "--", name, &oid])
                };
                destination = Some(format!("{name} at {oid}"));
            }
            RepositoryAction::SwitchBranch { local_ref, .. } => {
                self.require_clean(status)?;
                let refs = self.list_refs(repository_id).await?;
                let selected = refs
                    .iter()
                    .find(|r| r.kind == "local" && r.full_name == *local_ref)
                    .ok_or_else(|| {
                        AppError::new("invalid_ref", "Select an existing local branch.")
                    })?;
                args = runner::args(&["switch", "--no-guess", "--", &selected.label]);
                destination = Some(selected.full_name.clone());
                target_ref = Some((selected.full_name.clone(), selected.oid.clone()));
            }
            RepositoryAction::Push {
                remote_id,
                destination_ref,
                set_upstream,
                ..
            } => {
                if status.git_state != GitState::Idle {
                    return Err(AppError::new(
                        "operation_in_progress",
                        "Finish the ongoing Git operation first.",
                    ));
                }
                let branch = if let Head::Branch { name, .. } = &status.head {
                    name
                } else {
                    return Err(AppError::new(
                        "invalid_head",
                        "Push requires an attached, existing branch.",
                    ));
                };
                let remotes = self.list_remotes(repository_id).await?;
                let remote = remotes
                    .iter()
                    .find(|r| r.id == *remote_id)
                    .ok_or_else(|| AppError::new("invalid_remote", "Select an existing remote."))?;
                if self
                    .config(&repo, &format!("remote.{}.mirror", remote.name))
                    .await?
                    .is_some_and(|v| v == "true" || v == "1" || v == "yes")
                {
                    return Err(AppError::new(
                        "unsupported_push",
                        "Mirror remotes are not supported.",
                    ));
                }
                let urls = runner::read(
                    &repo.root,
                    &["remote", "get-url", "--push", "--all", &remote.name],
                )
                .await?;
                if text(&urls).lines().count() != 1 {
                    return Err(AppError::new(
                        "unsupported_push",
                        "Multiple effective push URLs are not supported.",
                    ));
                }
                let push_config = runner::run(
                    &repo.root,
                    &runner::args(&[
                        "config",
                        "--get-all",
                        &format!("remote.{}.push", remote.name),
                    ]),
                    None,
                    false,
                    runner::STRUCTURED_LIMIT,
                )
                .await?;
                let configured = if push_config.code == 1 {
                    None
                } else {
                    let values = text(&runner::checked(push_config)?.stdout);
                    if values.lines().count() != 1 {
                        return Err(AppError::new(
                            "unsupported_push",
                            "Multiple configured push refspecs are not supported.",
                        ));
                    }
                    Some(values)
                };
                if let Some(spec) = configured {
                    if spec.contains('*') || spec.starts_with('+') || !spec.contains(':') {
                        return Err(AppError::new("unsupported_push","Wildcard, forced, or implicit configured push refspecs require manual review."));
                    }
                    let (source, target) = spec.split_once(':').unwrap();
                    if !["HEAD", branch.as_str(), &format!("refs/heads/{branch}")].contains(&source)
                        || target != destination_ref
                    {
                        return Err(AppError::new("unsupported_push","The selected destination must match the configured single-branch push refspec."));
                    }
                }
                if !destination_ref.starts_with("refs/heads/") {
                    return Err(AppError::new(
                        "invalid_ref",
                        "Push destination must be a full branch reference.",
                    ));
                }
                runner::read(&repo.root, &["check-ref-format", destination_ref]).await?;
                let mut push =
                    runner::args(&["push", "--no-follow-tags", "--recurse-submodules=no"]);
                if *set_upstream {
                    push.push("--set-upstream".into());
                }
                push.push("--".into());
                push.push(remote.name.clone().into());
                push.push(format!("refs/heads/{branch}:{destination_ref}").into());
                args = push;
                destination = Some(format!("{} {destination_ref}", remote.name));
            }
            _ => return Err(AppError::new(
                "unsupported",
                "Merge and history rewriting are not enabled until the M1 acceptance gate passes.",
            )),
        }
        Ok(PreparedAction {
            summary: format!("{} in {}", action.kind(), repo.root.to_string_lossy()),
            destination,
            repository: repo.clone(),
            fingerprint: snapshot_id.map(|_| observed.fingerprint),
            config_fingerprint: self.config_fingerprint(&repo).await?,
            target_ref,
            action,
            args,
            input,
        })
    }
    fn require_clean(&self, status: &RepositoryStatus) -> AppResult<()> {
        if status.dirty {
            return Err(AppError::new(
                "dirty",
                "This operation requires a clean working tree and index.",
            ));
        }
        if status.git_state != GitState::Idle {
            return Err(AppError::new(
                "operation_in_progress",
                "Finish the ongoing Git operation first.",
            ));
        }
        Ok(())
    }
    pub async fn execute(&self, plan: &PreparedAction) -> AppResult<ActionOutcome> {
        let _guard = plan.repository.lock.lock().await;
        if self.config_fingerprint(&plan.repository).await? != plan.config_fingerprint {
            return Err(AppError::new(
                "precondition_changed",
                "Git configuration changed after preview.",
            ));
        }
        if let Some(fingerprint) = &plan.fingerprint {
            if self.observe(&plan.repository).await?.fingerprint != *fingerprint {
                return Err(AppError::new(
                    "precondition_changed",
                    "Repository changed after preview; refresh before retrying.",
                ));
            }
        }
        if let Some((reference, oid)) = &plan.target_ref {
            if self.resolve_commit(&plan.repository, reference).await? != *oid {
                return Err(AppError::new(
                    "precondition_changed",
                    "The selected branch changed after preview.",
                ));
            }
        }
        let output = runner::checked(
            runner::run(
                &plan.repository.root,
                &plan.args,
                plan.input.clone(),
                true,
                runner::DIAGNOSTIC_LIMIT,
            )
            .await?,
        ).map_err(|mut error| {
            if plan.repository.git_dir.join("index.lock").exists() {
                error.code="lock_conflict".into();
                error.message="Git could not acquire the repository index lock. Another Git process may be active; no lock was removed.".into();
                error.retryable=true;
            }
            error
        })?;
        if matches!(
            plan.action,
            RepositoryAction::Fetch { .. } | RepositoryAction::Pull { .. }
        ) {
            self.state
                .lock()
                .await
                .fetch_times
                .insert(plan.repository.id.clone(), now());
        }
        Ok(ActionOutcome {
            exit_code: output.code,
            output_truncated: output.truncated,
        })
    }
}
