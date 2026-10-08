use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
impl GitEngine {
    pub(crate) async fn config_fingerprint(&self, repo: &Repository) -> AppResult<String> {
        Ok(digest(
            &self
                .runner
                .read(&repo.root, &["config", "--null", "--list"])
                .await?,
        ))
    }
    pub async fn prepare_action(
        &self,
        repository_id: &str,
        action: RepositoryAction,
    ) -> AppResult<PreparedAction> {
        let repo = self.repository(repository_id).await?;
        let initial_config = self.config_fingerprint(&repo).await?;
        let observed = self.observe(&repo).await?;
        let status = observed.status.as_ref().unwrap();
        if status.operation_context.as_ref().is_some_and(|context| {
            context.kind == GitState::CherryPick
                && context.stop_reason == OperationStopReason::Unknown
        }) && cherry_pick::read_journal(&repo)?.is_some()
            && !matches!(
                action,
                RepositoryAction::Fetch { .. } | RepositoryAction::AbortCherryPick { .. }
            )
        {
            return Err(AppError::new("cherry_pick_manual_review","The recorded cherry-pick disagrees with the repository state. Inspect the operation context and journal before changing the index, files, or history."));
        }
        if status.operation_context.as_ref().is_some_and(|context| {
            context.kind == GitState::Rebase
                && context.stop_reason == OperationStopReason::Unknown
                && context
                    .rebase
                    .as_ref()
                    .is_some_and(|rebase| rebase.app_owned)
        }) && !matches!(action, RepositoryAction::Fetch { .. })
        {
            return Err(AppError::new("rebase_manual_review", "The rebase state cannot be verified. Inspect Git metadata, working files, and any saved backup before changing the repository."));
        }
        let args;
        let mut input = None;
        let mut destination = None;
        let mut target_ref = None;
        let mut summary = None;
        let mut squash_start = None;
        let mut rewrite = None;
        let mut remote_checkout = None;
        let mut cherry = None;
        let mut rebase = None;
        let mut stash = None;
        let mut hunk = None;
        let finish_squash = matches!(action, RepositoryAction::Commit { .. })
            && status.git_state == GitState::SquashMerge;
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
            | RepositoryAction::CheckoutRemoteBranch {
                status_snapshot_id, ..
            }
            | RepositoryAction::Merge {
                status_snapshot_id, ..
            }
            | RepositoryAction::SquashMerge {
                status_snapshot_id, ..
            }
            | RepositoryAction::CherryPick {
                status_snapshot_id, ..
            }
            | RepositoryAction::ContinueCherryPick { status_snapshot_id }
            | RepositoryAction::AbortCherryPick { status_snapshot_id }
            | RepositoryAction::SkipCherryPick { status_snapshot_id }
            | RepositoryAction::StartRebase {
                status_snapshot_id, ..
            }
            | RepositoryAction::ContinueRebase { status_snapshot_id }
            | RepositoryAction::SkipRebase { status_snapshot_id }
            | RepositoryAction::AbortRebase { status_snapshot_id }
            | RepositoryAction::CreateStash {
                status_snapshot_id, ..
            }
            | RepositoryAction::ApplyStash {
                status_snapshot_id, ..
            }
            | RepositoryAction::DeleteStash {
                status_snapshot_id, ..
            }
            | RepositoryAction::FinalizeStashDelete {
                status_snapshot_id, ..
            }
            | RepositoryAction::ContinueMerge { status_snapshot_id }
            | RepositoryAction::AbortMerge { status_snapshot_id }
            | RepositoryAction::CombineCommits {
                status_snapshot_id, ..
            }
            | RepositoryAction::RecoverCombine {
                status_snapshot_id, ..
            } => Some(status_snapshot_id),
            RepositoryAction::Fetch { .. }
            | RepositoryAction::StageHunk { .. }
            | RepositoryAction::UnstageHunk { .. } => None,
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
            RepositoryAction::StageHunk { .. } | RepositoryAction::UnstageHunk { .. } => {
                let (prepared, description) = self.prepare_hunk(&repo, &action).await?;
                hunk = Some(prepared);
                summary = Some(description);
                args = vec![];
            }
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
                    &self
                        .runner
                        .read(
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
                    &format!("branch.{branch}.mergeOptions="),
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
                if !matches!(
                    status.git_state,
                    GitState::Idle | GitState::Merge | GitState::SquashMerge
                ) && !(status.git_state == GitState::CherryPick
                    && status.operation_context.as_ref().is_some_and(|context| {
                        context.can_abort && context.stop_reason != OperationStopReason::Unknown
                    }))
                    && !(status.git_state == GitState::Rebase
                        && status.operation_context.as_ref().is_some_and(|context| {
                            context.can_abort && context.stop_reason != OperationStopReason::Unknown
                        }))
                {
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
                                "--force",
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
                if !matches!(status.git_state, GitState::Idle | GitState::SquashMerge)
                    || status.counts.conflicted > 0
                {
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
                        &self
                            .runner
                            .read(&repo.root, &["check-ref-format", "--branch", name])
                            .await?,
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
            RepositoryAction::CheckoutRemoteBranch {
                remote_id,
                source_ref,
                name,
                ..
            } => {
                self.require_clean(status)?;
                let checkout = self
                    .prepare_remote_checkout(&repo, remote_id, source_ref, name)
                    .await?;
                args = runner::args(&[
                    "--no-replace-objects",
                    "switch",
                    "--no-guess",
                    "--no-recurse-submodules",
                    "--no-overwrite-ignore",
                    "--track=direct",
                    "-c",
                    name,
                    source_ref,
                ]);
                summary = Some(format!("Create and switch to local branch {name} from {} at {}. Track remote {} branch {}. Current HEAD: {:?}.", source_ref, checkout.target.source_oid, checkout.target.remote_name, checkout.target.upstream_ref, status.head));
                destination = Some(format!(
                    "{name} → {} {}",
                    checkout.target.remote_name, checkout.target.upstream_ref
                ));
                remote_checkout = Some(checkout);
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
                let symbolic = self
                    .runner
                    .run(
                        &repo.root,
                        &runner::args(&["symbolic-ref", "-q", &selected.full_name]),
                        None,
                        false,
                        runner::STRUCTURED_LIMIT,
                    )
                    .await?;
                if symbolic.code == 0 {
                    return Err(AppError::new(
                        "invalid_ref",
                        "Switching to a symbolic local branch is not supported.",
                    ));
                }
                if symbolic.code != 1 {
                    runner::checked(symbolic)?;
                }
                if !matches!(&status.head, Head::Branch { name, .. } if name == &selected.label) {
                    let worktrees = self
                        .runner
                        .read(&repo.root, &["worktree", "list", "--porcelain", "-z"])
                        .await?;
                    let branch = format!("branch {}", selected.full_name);
                    if worktrees
                        .split(|b| *b == 0)
                        .any(|field| field == branch.as_bytes())
                    {
                        return Err(AppError::new(
                            "branch_in_use",
                            "The target branch is checked out in another worktree.",
                        ));
                    }
                }
                args = runner::args(&[
                    "switch",
                    "--no-guess",
                    "--no-recurse-submodules",
                    "--no-overwrite-ignore",
                    "--",
                    &selected.label,
                ]);
                let source = match &status.head {
                    Head::Branch { name, oid } => format!("{name} at {oid}"),
                    Head::Detached { oid } => format!("detached HEAD at {oid}"),
                    Head::Unborn { name } => format!("unborn {name}"),
                };
                summary = Some(format!("Switch from {source} to {} at {}. No fetch, pull, branch creation or automatic stash.", selected.full_name, selected.oid));
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
                let mirror = self
                    .runner
                    .run(
                        &repo.root,
                        &runner::args(&[
                            "config",
                            "--type=bool",
                            "--get",
                            &format!("remote.{}.mirror", remote.name),
                        ]),
                        None,
                        false,
                        runner::STRUCTURED_LIMIT,
                    )
                    .await?;
                if mirror.code != 1 && text(&runner::checked(mirror)?.stdout) == "true" {
                    return Err(AppError::new(
                        "unsupported_push",
                        "Mirror remotes are not supported.",
                    ));
                }
                let urls = self
                    .runner
                    .read(
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
                let push_config = self
                    .runner
                    .run(
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
                self.runner
                    .read(&repo.root, &["check-ref-format", destination_ref])
                    .await?;
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
            RepositoryAction::Merge { source_ref, .. }
            | RepositoryAction::SquashMerge { source_ref, .. } => {
                self.require_clean(status)?;
                let (branch, original_head) = match &status.head {
                    Head::Branch { name, oid } => (name, oid),
                    _ => {
                        return Err(AppError::new(
                            "invalid_head",
                            "Merge requires an attached, existing branch.",
                        ))
                    }
                };
                let references = self.list_refs(repository_id).await?;
                let source = references
                    .iter()
                    .find(|reference| reference.full_name == *source_ref)
                    .ok_or_else(|| {
                        AppError::new("invalid_ref", "Select an available source reference.")
                    })?;
                let oid = self.resolve_commit(&repo, &source.oid).await?;
                target_ref = Some((source.full_name.clone(), oid.clone()));
                let squash = matches!(action, RepositoryAction::SquashMerge { .. });
                if squash {
                    let ancestor = self
                        .runner
                        .run(
                            &repo.root,
                            &runner::args(&["merge-base", "--is-ancestor", &oid, original_head]),
                            None,
                            false,
                            runner::STRUCTURED_LIMIT,
                        )
                        .await?;
                    if ancestor.code == 0 {
                        return Err(AppError::new(
                            "already_up_to_date",
                            "The source is already included in the current branch.",
                        ));
                    }
                    if ancestor.code != 1 {
                        runner::checked(ancestor)?;
                    }
                    squash_start = Some(merge::SquashJournal {
                        version: 1,
                        operation_id: id(),
                        phase: merge::SquashPhase::Running,
                        original_branch: format!("refs/heads/{branch}"),
                        state: SquashMergeState {
                            original_head: original_head.clone(),
                            source_oid: oid.clone(),
                            source_ref: source.full_name.clone(),
                        },
                    });
                }
                args = runner::args(&[
                    "-c",
                    &format!("branch.{branch}.mergeOptions="),
                    "-c",
                    "merge.autoStash=false",
                    "merge",
                    "--no-edit",
                    "--no-autostash",
                    "--ff",
                    if squash { "--squash" } else { "--no-squash" },
                    if squash { "--no-commit" } else { "--commit" },
                    "--",
                    &oid,
                ]);
                destination = Some(format!(
                    "{} ({oid}) → {branch} ({original_head})",
                    source.full_name
                ));
                summary = Some(if squash {
                    format!("Squash-merge {} ({oid}) into {branch} at {original_head}. Leave the result staged for review and a separate normal commit; no ordinary merge abort is available.",source.full_name)
                } else {
                    format!("Merge {} ({oid}) into {branch} at {original_head}; allow fast-forward or create a normal merge commit.",source.full_name)
                });
            }
            RepositoryAction::CombineCommits {
                commit_oids,
                message,
                ..
            } => {
                rewrite = Some(
                    Box::pin(self.prepare_combine(
                        &repo,
                        status,
                        &observed.fingerprint,
                        commit_oids,
                        message,
                    ))
                    .await?,
                );
                args = Vec::new();
                summary=Some(format!("Replace these {} tip commits with one commit: {}. Original HEAD is retained in a backup ref. This rewrites history; remote-tracking refs cannot prove these commits are unpublished. Message: {}",commit_oids.len(),commit_oids.join(", "),message));
                destination = Some(match &status.head {
                    Head::Branch { name, oid } => format!("{name} at {oid}"),
                    _ => String::new(),
                });
            }
            RepositoryAction::RecoverCombine { recovery_id, .. } => {
                rewrite = Some(Box::pin(self.prepare_recovery(&repo, status, recovery_id)).await?);
                args = Vec::new();
                summary=Some("Restore the recorded original branch tip using verified backup refs. The index and working files are left byte-for-byte unchanged; later commits or a changed branch block recovery.".into());
                destination = Some(recovery_id.clone());
            }
            RepositoryAction::CherryPick { .. }
            | RepositoryAction::ContinueCherryPick { .. }
            | RepositoryAction::AbortCherryPick { .. }
            | RepositoryAction::SkipCherryPick { .. } => {
                let (prepared, command, description) =
                    self.prepare_cherry(&repo, status, &action).await?;
                cherry = Some(prepared);
                args = command;
                summary = Some(description);
            }
            RepositoryAction::StartRebase { .. }
            | RepositoryAction::ContinueRebase { .. }
            | RepositoryAction::SkipRebase { .. }
            | RepositoryAction::AbortRebase { .. } => {
                let (prepared, command, description) =
                    self.prepare_rebase(&repo, status, &action).await?;
                rebase = Some(prepared);
                args = command;
                summary = Some(description);
            }
            RepositoryAction::CreateStash { .. }
            | RepositoryAction::ApplyStash { .. }
            | RepositoryAction::DeleteStash { .. }
            | RepositoryAction::FinalizeStashDelete { .. } => {
                let (prepared, command, description) =
                    self.prepare_stash(&repo, status, &action).await?;
                stash = Some(prepared);
                args = command;
                summary = Some(description);
            }
            RepositoryAction::ContinueMerge { .. } => {
                if status.git_state != GitState::Merge {
                    return Err(AppError::new(
                        "invalid_operation",
                        "There is no ordinary merge to continue.",
                    ));
                }
                if status.counts.conflicted > 0 {
                    return Err(AppError::new(
                        "unresolved_conflicts",
                        "Resolve and stage every conflict before continuing the merge.",
                    ));
                }
                args = runner::args(&["commit", "--no-edit"]);
                summary=Some("Complete the ordinary merge with its recorded message, normal hooks, and signing configuration.".into());
            }
            RepositoryAction::AbortMerge { .. } => {
                if status.git_state != GitState::Merge {
                    return Err(AppError::new("invalid_operation","Only an ordinary merge with MERGE_HEAD can be aborted. Squash changes require manual review and cleanup."));
                }
                args = runner::args(&["-c", "submodule.recurse=false", "merge", "--abort"]);
                summary=Some("Abort the ordinary merge. Git will try to restore its pre-merge state; uncommitted resolution edits may be discarded.".into());
            }
        }
        let config_fingerprint = self.config_fingerprint(&repo).await?;
        if (remote_checkout.is_some()
            || cherry.is_some()
            || rebase.is_some()
            || stash.is_some()
            || hunk.is_some())
            && config_fingerprint != initial_config
        {
            return Err(AppError::new(
                "precondition_changed",
                "Git configuration changed while preparing the action.",
            ));
        }
        Ok(PreparedAction {
            summary: summary
                .unwrap_or_else(|| format!("{} in {}", action.kind(), repo.root.to_string_lossy())),
            destination,
            repository: repo.clone(),
            fingerprint: snapshot_id.map(|_| observed.fingerprint),
            config_fingerprint,
            target_ref,
            head_guard: None,
            squash_start,
            finish_squash,
            rewrite,
            remote_checkout,
            cherry,
            rebase,
            stash,
            hunk,
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
        self.execute_cancelable(plan, Arc::new(AtomicBool::new(false)))
            .await
    }
    pub async fn execute_cancelable(
        &self,
        plan: &PreparedAction,
        canceled: Arc<AtomicBool>,
    ) -> AppResult<ActionOutcome> {
        let check_canceled = || {
            if canceled.load(Ordering::Acquire) {
                Err(AppError::new(
                    "canceled",
                    "Operation was canceled before Git started.",
                ))
            } else {
                Ok(())
            }
        };
        check_canceled()?;
        let _guard = plan.repository.lock.lock().await;
        check_canceled()?;
        if let Some(head) = &plan.head_guard {
            if self
                .observe(&plan.repository)
                .await?
                .status
                .as_ref()
                .unwrap()
                .head
                != *head
            {
                return Err(AppError::new(
                    "precondition_changed",
                    "The filtered repository branch or HEAD changed after preview.",
                ));
            }
        }
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
            if matches!(plan.action, RepositoryAction::SwitchBranch { .. }) {
                let symbolic = self
                    .runner
                    .run(
                        &plan.repository.root,
                        &runner::args(&["symbolic-ref", "-q", reference]),
                        None,
                        false,
                        runner::STRUCTURED_LIMIT,
                    )
                    .await?;
                if symbolic.code != 1 {
                    return Err(AppError::new("precondition_changed", "The target branch is no longer a verifiable direct local ref. Preview again."));
                }
            }
            if self.resolve_commit(&plan.repository, reference).await? != *oid {
                return Err(AppError::new(
                    "precondition_changed",
                    "The selected branch changed after preview.",
                ));
            }
        }
        check_canceled()?;
        if let Some(rewrite) = &plan.rewrite {
            return Box::pin(self.execute_rewrite(&plan.repository, rewrite, &canceled)).await;
        }
        if let Some(checkout) = &plan.remote_checkout {
            return Box::pin(self.execute_remote_checkout(plan, checkout, &canceled)).await;
        }
        if let Some(cherry) = &plan.cherry {
            return Box::pin(self.execute_cherry(plan, cherry, &canceled)).await;
        }
        if let Some(rebase) = &plan.rebase {
            return Box::pin(self.execute_rebase(plan, rebase, &canceled)).await;
        }
        if let Some(hunk) = &plan.hunk {
            return Box::pin(self.execute_hunk(plan, hunk, &canceled)).await;
        }
        if let Some(stash) = &plan.stash {
            return Box::pin(self.execute_stash(plan, stash, &canceled)).await;
        }
        let _squash_activity = plan
            .squash_start
            .as_ref()
            .map(|_| merge::SquashActivity::begin(&plan.repository));
        if let Some(state) = &plan.squash_start {
            merge::write_squash(&plan.repository, state)?;
        }
        let output = self
            .runner
            .run(
                &plan.repository.root,
                &plan.args,
                plan.input.clone(),
                true,
                runner::DIAGNOSTIC_LIMIT,
            )
            .await?;
        if let Some(journal) = &plan.squash_start {
            merge::finish_attempt(&plan.repository, &journal.operation_id)?;
        }
        let output=runner::checked(output).map_err(|mut error| {
            if matches!(plan.action, RepositoryAction::Push { .. }) {
                error.code="outcome_uncertain".into();
                error.message=format!("Push returned an error. Inspect the remote branch before retrying; a connection or hook failure does not prove that the remote was unchanged. {}", error.message);
            }
            if matches!(plan.action, RepositoryAction::SwitchBranch { .. }) {
                error.code="outcome_uncertain".into();
                error.message=format!("Switch returned an error and may already have changed HEAD or files (for example, before a hook failed). Refresh and inspect before retrying. {}", error.message);
            }
            if !matches!(plan.action,RepositoryAction::Fetch{..}|RepositoryAction::Push{..}) && plan.repository.git_dir.join("index.lock").exists() {
                error.code="lock_conflict".into();
                error.message="Git could not acquire the repository index lock. Another Git process may be active; no lock was removed.".into();
                error.retryable=true;
            }
            error
        })?;
        if let RepositoryAction::SwitchBranch { local_ref, .. } = &plan.action {
            let observed = self.observe(&plan.repository).await.map_err(|error| AppError::new("outcome_uncertain", format!("Switch completed, but the resulting repository state could not be verified. Refresh and inspect before retrying. {}", error.message)))?;
            let expected = plan.target_ref.as_ref().map(|(_, oid)| oid);
            if !matches!(&observed.status.as_ref().unwrap().head, Head::Branch { name, oid } if format!("refs/heads/{name}") == *local_ref && Some(oid) == expected)
            {
                return Err(AppError::new("outcome_uncertain", "The switch command completed, but HEAD differs from the reviewed target. A hook or external process may have changed it; inspect this repository before retrying."));
            }
        }
        if plan.finish_squash {
            self.finish_squash_commit(&plan.repository).await?;
        }
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
