use super::*;
use std::{
    collections::HashSet,
    io::Write,
    sync::atomic::{AtomicBool, Ordering},
};
const JOURNAL: &str = "git-view-cherry-pick.json";
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CherryJournal {
    version: u32,
    operation_id: String,
    branch: String,
    original_head: String,
    source_oid: String,
}
#[derive(Clone)]
pub(crate) struct CherryPlan {
    journal: Option<CherryJournal>,
    branch: String,
    head: String,
    source: String,
    start: bool,
    commit: bool,
    clear_only: bool,
}
struct Source {
    tree: String,
    parents: Vec<String>,
    subject: String,
}
fn error(message: &str) -> AppError {
    AppError::new("cherry_pick_unavailable", message)
}
fn valid_oid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64) && oid.bytes().all(|b| b.is_ascii_hexdigit())
}
pub(crate) fn read_journal(repo: &Repository) -> AppResult<Option<CherryJournal>> {
    let Some(bytes) = merge::read_metadata(&repo.git_dir.join(JOURNAL), 64 * 1024)? else {
        return Ok(None);
    };
    let journal: CherryJournal = serde_json::from_slice(&bytes).map_err(|_| {
        error("The cherry-pick journal is corrupt; preserve it and review the repository manually.")
    })?;
    if journal.version != 1
        || uuid::Uuid::parse_str(&journal.operation_id).is_err()
        || !journal.branch.starts_with("refs/heads/")
        || !valid_oid(&journal.original_head)
        || !valid_oid(&journal.source_oid)
    {
        return Err(error(
            "The cherry-pick journal is unsupported; manual review is required.",
        ));
    }
    Ok(Some(journal))
}
fn write_journal(repo: &Repository, journal: &CherryJournal) -> AppResult<()> {
    let mut file =
        tempfile::NamedTempFile::new_in(&repo.git_dir).map_err(|e| error(&e.to_string()))?;
    file.write_all(&serde_json::to_vec(journal).unwrap())
        .and_then(|()| file.as_file().sync_all())
        .map_err(|e| error(&e.to_string()))?;
    file.persist_noclobber(repo.git_dir.join(JOURNAL))
        .map_err(|e| error(&e.to_string()))?;
    #[cfg(unix)]
    std::fs::File::open(&repo.git_dir)
        .and_then(|directory| directory.sync_all())
        .map_err(|e| error(&e.to_string()))?;
    Ok(())
}
fn clear_journal(repo: &Repository, journal: &CherryJournal) -> AppResult<()> {
    let actual = read_journal(repo)?
        .ok_or_else(|| error("The cherry-pick journal disappeared; inspect the actual outcome."))?;
    if serde_json::to_vec(&actual).unwrap() != serde_json::to_vec(journal).unwrap() {
        return Err(error(
            "The cherry-pick journal changed; it was preserved for manual review.",
        ));
    }
    std::fs::remove_file(repo.git_dir.join(JOURNAL)).map_err(|e| error(&e.to_string()))
}
fn marker_oids(repo: &Repository, name: &str) -> AppResult<Vec<String>> {
    let Some(bytes) = merge::read_metadata(&repo.git_dir.join(name), 512 * 1024)? else {
        return Ok(Vec::new());
    };
    let value = std::str::from_utf8(&bytes)
        .map_err(|_| error("Git operation metadata is not valid text."))?;
    let oids: Vec<_> = value.lines().map(str::trim).map(str::to_owned).collect();
    if oids.is_empty() || oids.iter().any(|oid| !valid_oid(oid)) {
        return Err(error(
            "Git operation metadata contains an invalid commit ID.",
        ));
    }
    Ok(oids)
}
impl GitEngine {
    async fn cherry_source(&self, repo: &Repository, oid: &str) -> AppResult<Source> {
        if !valid_oid(oid) {
            return Err(error(
                "Select one full commit ID; revision expressions and ranges are not accepted.",
            ));
        }
        let bytes = self
            .runner
            .read(
                &repo.root,
                &["--no-replace-objects", "cat-file", "commit", oid],
            )
            .await?;
        let (headers, message) = bytes.split_at(
            bytes
                .windows(2)
                .position(|w| w == b"\n\n")
                .ok_or_else(|| error("Invalid source commit object."))?,
        );
        let mut tree = None;
        let mut parents = Vec::new();
        for line in headers.split(|b| *b == b'\n') {
            if let Some(value) = line.strip_prefix(b"tree ") {
                tree = Some(text(value));
            }
            if let Some(value) = line.strip_prefix(b"parent ") {
                parents.push(text(value));
            }
        }
        Ok(Source {
            tree: tree.ok_or_else(|| error("Source commit has no tree."))?,
            parents,
            subject: text(&message[2..])
                .lines()
                .next()
                .unwrap_or_default()
                .into(),
        })
    }
    pub(crate) async fn tree_paths(
        &self,
        repo: &Repository,
        oid: &str,
    ) -> AppResult<HashSet<Vec<u8>>> {
        Ok(self
            .runner
            .read(
                &repo.root,
                &[
                    "--no-replace-objects",
                    "ls-tree",
                    "-r",
                    "--name-only",
                    "-z",
                    oid,
                ],
            )
            .await?
            .split(|b| *b == 0)
            .filter(|p| !p.is_empty())
            .map(<[u8]>::to_vec)
            .collect())
    }
    pub(crate) async fn protect_materialized_paths(
        &self,
        repo: &Repository,
        paths: &HashSet<Vec<u8>>,
    ) -> AppResult<()> {
        let tracked = self.runner.read(&repo.root, &["ls-files", "-z"]).await?;
        let tracked: HashSet<_> = tracked
            .split(|b| *b == 0)
            .filter(|p| !p.is_empty())
            .collect();
        for bytes in paths {
            let relative = PathBuf::from(path_bytes(bytes));
            let full = repo.root.join(&relative);
            if !tracked.contains(bytes.as_slice()) && std::fs::symlink_metadata(&full).is_ok() {
                return Err(AppError::new("local_path_collision",format!("This action could overwrite an ignored or untracked local path: {}. Move that content outside the repository first.",relative.to_string_lossy())));
            }
            let mut parent = relative.parent();
            while let Some(path) = parent.filter(|p| !p.as_os_str().is_empty()) {
                if let Ok(meta) = std::fs::symlink_metadata(repo.root.join(path)) {
                    if meta.file_type().is_symlink() || !meta.is_dir() {
                        return Err(AppError::new("local_path_collision",format!("An action destination has a file or symlink ancestor: {}. Review it before proceeding.",path.to_string_lossy())));
                    }
                }
                parent = path.parent();
            }
        }
        Ok(())
    }
    async fn cherry_collisions(
        &self,
        repo: &Repository,
        source_oid: &str,
        target: &str,
    ) -> AppResult<()> {
        let source = self.cherry_source(repo, source_oid).await?;
        let base = if let Some(parent) = source.parents.first() {
            self.tree_paths(repo, parent).await?
        } else {
            HashSet::new()
        };
        let incoming = self.tree_paths(repo, source_oid).await?;
        let target_paths = self.tree_paths(repo, target).await?;
        let changed = self
            .runner
            .read(
                &repo.root,
                &[
                    "--no-replace-objects",
                    "diff-tree",
                    "--root",
                    "--no-commit-id",
                    "--name-only",
                    "--no-renames",
                    "-r",
                    "-z",
                    source_oid,
                ],
            )
            .await?;
        let paths = changed
            .split(|b| *b == 0)
            .filter(|p| !p.is_empty())
            .map(<[u8]>::to_vec)
            .collect();
        self.protect_materialized_paths(repo, &paths).await?;
        // Directory-rename placement depends on merge-ort heuristics. A conservative
        // path-set test catches possible moves regardless of rename limits/similarity.
        let directory = |path: &Vec<u8>| {
            path.rsplitn(2, |b| *b == b'/')
                .nth(1)
                .unwrap_or_default()
                .to_vec()
        };
        let removed: HashSet<_> = base.difference(&target_paths).map(directory).collect();
        let added: HashSet<_> = target_paths.difference(&base).map(directory).collect();
        let possible_move = incoming.difference(&base).next().is_some()
            && removed.iter().any(|old| added.iter().any(|new| old != new));
        if possible_move {
            let ignored = self
                .runner
                .read(
                    &repo.root,
                    &[
                        "ls-files",
                        "--others",
                        "--ignored",
                        "--exclude-standard",
                        "--directory",
                        "-z",
                    ],
                )
                .await?;
            if !ignored.is_empty() {
                return Err(AppError::new("ignored_directory_rename","This pick may relocate additions through a directory rename while ignored content exists. Git can overwrite that content even when it stops with a conflict. Move ignored content outside the checkout or handle this pick manually."));
            }
        }
        Ok(())
    }
    pub(crate) async fn operation_context(
        &self,
        repo: &Repository,
        status: &mut RepositoryStatus,
    ) -> AppResult<()> {
        if self.rebase_context(repo, status).await? {
            return Ok(());
        }
        let journal = read_journal(repo)?;
        if journal.is_some() && status.git_state == GitState::Idle {
            status.git_state = GitState::Other;
        }
        let is_cherry = status.git_state == GitState::CherryPick || journal.is_some();
        if !is_cherry && !matches!(status.git_state, GitState::Merge | GitState::SquashMerge) {
            return Ok(());
        }
        let branch = if status.git_state == GitState::SquashMerge {
            match &status.head {
                Head::Branch { name, .. } => Some(name.clone()),
                _ => None,
            }
        } else {
            None
        };
        let source_result = if is_cherry {
            marker_oids(repo, "CHERRY_PICK_HEAD")
        } else if status.git_state == GitState::Merge {
            marker_oids(repo, "MERGE_HEAD")
        } else {
            Ok(status
                .squash_merge
                .as_ref()
                .map(|s| vec![s.source_oid.clone()])
                .unwrap_or_default())
        };
        let (sources, source_error) = match source_result {
            Ok(sources) => (sources, None),
            Err(error) => (Vec::new(), Some(error)),
        };
        let mut context = OperationContext {
            kind: if is_cherry {
                GitState::CherryPick
            } else {
                status.git_state.clone()
            },
            source_oids: sources,
            source_ref: status.squash_merge.as_ref().map(|s| s.source_ref.clone()),
            original_head: status
                .squash_merge
                .as_ref()
                .map(|s| s.original_head.clone()),
            target_branch: branch,
            stop_reason: OperationStopReason::Unknown,
            can_continue: false,
            can_abort: false,
            can_skip: false,
            reason: String::new(),
            rebase: None,
        };
        if let Some(error) = source_error {
            context.reason=format!("Operation source metadata could not be read reliably: {}. Inspect the working changes and recover the Git operation manually.",error.message);
            status.operation_context = Some(context);
            return Ok(());
        }
        if is_cherry {
            if let Some(journal) = &journal {
                context.original_head = Some(journal.original_head.clone());
                context.target_branch =
                    Some(journal.branch.trim_start_matches("refs/heads/").into());
                if context.source_oids.is_empty() {
                    context.source_oids.push(journal.source_oid.clone());
                }
                if !matches!(&status.head,Head::Branch{name,oid} if format!("refs/heads/{name}")==journal.branch && *oid==journal.original_head)
                    || status.git_state != GitState::CherryPick
                    || context.source_oids != vec![journal.source_oid.clone()]
                {
                    context.can_abort = status.git_state == GitState::Other
                        && !status.dirty
                        && matches!(&status.head,Head::Branch{name,oid} if format!("refs/heads/{name}")==journal.branch && *oid==journal.original_head)
                        && !repo.git_dir.join("sequencer").exists();
                    context.reason="The recorded cherry-pick no longer matches Git's markers or original target. Inspect actual history and files; no completion or rollback was inferred. A clean unchanged target can explicitly clear the app marker with Abort; otherwise review git-view-cherry-pick.json manually.".into();
                    status.operation_context = Some(context);
                    return Ok(());
                }
            }
            if repo.git_dir.join("sequencer").exists()
                || context.source_oids.len() != 1
                || !matches!(status.head, Head::Branch { .. })
                || !self
                    .cherry_source(repo, &context.source_oids[0])
                    .await
                    .is_ok_and(|source| source.parents.len() <= 1)
            {
                context.reason="Only a single non-merge cherry-pick on an attached branch is supported. This operation requires manual review.".into();
                status.operation_context = Some(context);
                return Ok(());
            }
            context.can_abort = true;
        }
        if status.counts.conflicted > 0 {
            context.stop_reason = OperationStopReason::Conflicts;
            context.reason="Resolve the listed conflicts in an external editor, then stage each resolution and refresh.".into();
        } else if is_cherry {
            let out = self
                .runner
                .run(
                    &repo.root,
                    &runner::args(&[
                        "--no-replace-objects",
                        "diff",
                        "--cached",
                        "--quiet",
                        "--no-ext-diff",
                        "--no-textconv",
                        "HEAD",
                        "--",
                    ]),
                    None,
                    false,
                    runner::STRUCTURED_LIMIT,
                )
                .await?;
            if out.code == 0 {
                context.stop_reason = OperationStopReason::Empty;
                context.can_skip = status.counts.unstaged == 0 && !status.submodule_dirty;
                context.reason="This stopped pick has no staged change relative to HEAD. Skip explicitly to exit without creating an empty commit, or Abort. Unstaged changes must be resolved before Skip.".into();
            } else if out.code == 1 {
                context.stop_reason = OperationStopReason::Ready;
                context.can_continue = true;
                context.reason="The pick has staged changes and no unresolved conflicts. Continue retries its normal Git commit, hooks, and signing; Abort discards the picked/staged resolution changes.".into();
            } else {
                runner::checked(out)?;
            }
        } else {
            context.stop_reason = OperationStopReason::Ready;
            context.can_continue = status.git_state == GitState::Merge;
            context.reason=if status.git_state==GitState::SquashMerge{"Squash changes remain staged for a separate normal commit. There is no ordinary merge abort; inspect and clean up manually if abandoning them."}else{"All conflicts are staged. Continue creates the ordinary merge commit using its recorded message and normal Git hooks/signing."}.into();
        }
        if status.git_state == GitState::Merge {
            context.can_abort = true;
        }
        if is_cherry && journal.is_none() {
            context.reason.push_str(" This operation was started externally; its original target is unknown. Abort returns tracked files/index toward the reviewed current HEAD and can discard unrelated staged edits.");
        }
        status.operation_context = Some(context);
        Ok(())
    }
    pub(crate) async fn prepare_cherry(
        &self,
        repo: &Repository,
        status: &RepositoryStatus,
        action: &RepositoryAction,
    ) -> AppResult<(CherryPlan, Vec<OsString>, String)> {
        let (branch, head) = match &status.head {
            Head::Branch { name, oid } => (format!("refs/heads/{name}"), oid.clone()),
            _ => {
                return Err(error(
                    "Cherry-pick requires an attached local branch with an existing commit.",
                ))
            }
        };
        if let RepositoryAction::CherryPick { source_oid, .. } = action {
            if status.git_state != GitState::Idle || status.dirty {
                return Err(error("Start a cherry-pick only with a clean working tree/index and no operation in progress."));
            }
            let source_oid = source_oid.to_ascii_lowercase();
            let source = self.cherry_source(repo, &source_oid).await?;
            if source.parents.len() > 1 {
                return Err(error("Merge commits require a mainline choice and are outside single-commit cherry-pick."));
            }
            let empty = if let Some(parent) = source.parents.first() {
                self.cherry_source(repo, parent).await?.tree == source.tree
            } else {
                self.tree_paths(repo, &source_oid).await?.is_empty()
            };
            if empty {
                return Err(AppError::new("initially_empty_commit","The selected commit was originally empty. This action does not create empty commits."));
            }
            if !self
                .runner
                .read(
                    &repo.root,
                    &["for-each-ref", "--format=%(refname)", "refs/replace/"],
                )
                .await?
                .is_empty()
                || merge::read_metadata(&repo.common_dir.join("info/grafts"), 512 * 1024)?
                    .is_some_and(|b| !b.is_empty())
            {
                return Err(error(
                    "Cherry-pick with replacement refs or legacy grafts requires manual review.",
                ));
            }
            self.cherry_collisions(repo, &source_oid, &head).await?;
            let journal = CherryJournal {
                version: 1,
                operation_id: id(),
                branch: branch.clone(),
                original_head: head.clone(),
                source_oid: source_oid.clone(),
            };
            let args = runner::args(&[
                "--no-replace-objects",
                "-c",
                "submodule.recurse=false",
                "cherry-pick",
                "--no-edit",
                "--",
                &source_oid,
            ]);
            let summary=format!("Cherry-pick {source_oid} ({}) onto {branch} at {head}. Preserve the source message and author with normal Git cherry-pick hooks/signing. Conflicts or an empty result stop for review; no push is performed.",source.subject);
            Ok((
                CherryPlan {
                    journal: Some(journal),
                    branch,
                    head,
                    source: source_oid,
                    start: true,
                    commit: true,
                    clear_only: false,
                },
                args,
                summary,
            ))
        } else {
            let context = status
                .operation_context
                .as_ref()
                .filter(|c| c.kind == GitState::CherryPick)
                .ok_or_else(|| error("There is no supported cherry-pick to continue or exit."))?;
            let (flag, allowed, commit) = match action {
                RepositoryAction::ContinueCherryPick { .. } => {
                    ("--continue", context.can_continue, true)
                }
                RepositoryAction::AbortCherryPick { .. } => ("--abort", context.can_abort, false),
                RepositoryAction::SkipCherryPick { .. } => ("--skip", context.can_skip, false),
                _ => unreachable!(),
            };
            if !allowed {
                return Err(error(&context.reason));
            }
            let journal = read_journal(repo)?;
            let clear_only =
                flag == "--abort" && status.git_state == GitState::Other && journal.is_some();
            if !commit && !clear_only {
                self.protect_materialized_paths(repo, &self.tree_paths(repo, &head).await?)
                    .await?;
            }
            let summary = if commit {
                format!("Continue cherry-pick {} on {branch} at {head}; commit staged resolutions with normal Git hooks/signing.",context.source_oids.join(", "))
            } else if clear_only {
                "Clear the interrupted app cherry-pick marker only. The reviewed branch, index, and working files remain unchanged.".into()
            } else {
                format!("Cherry-pick {flag} on {branch} at {head}. Discard the current picked/staged resolution changes, including unrelated staged edits, and exit this one pick. Git may refuse if later working edits cannot be preserved.")
            };
            Ok((
                CherryPlan {
                    journal,
                    branch,
                    head,
                    source: context.source_oids[0].clone(),
                    start: false,
                    commit,
                    clear_only,
                },
                runner::args(&[
                    "--no-replace-objects",
                    "-c",
                    "submodule.recurse=false",
                    "cherry-pick",
                    flag,
                ]),
                summary,
            ))
        }
    }
    pub(crate) async fn execute_cherry(
        &self,
        plan: &PreparedAction,
        cherry: &CherryPlan,
        canceled: &AtomicBool,
    ) -> AppResult<ActionOutcome> {
        let repo = &plan.repository;
        if canceled.load(Ordering::Acquire) {
            return Err(AppError::new("canceled", "Canceled before Git started."));
        }
        let observed = self.observe(repo).await?;
        let (fresh, _, _) = self
            .prepare_cherry(repo, observed.status.as_ref().unwrap(), &plan.action)
            .await?;
        if fresh.branch != cherry.branch
            || fresh.head != cherry.head
            || fresh.source != cherry.source
            || observed.fingerprint != *plan.fingerprint.as_ref().unwrap()
            || self.config_fingerprint(repo).await? != plan.config_fingerprint
        {
            return Err(AppError::new(
                "precondition_changed",
                "Cherry-pick context changed after preview.",
            ));
        }
        if canceled.load(Ordering::Acquire) {
            return Err(AppError::new("canceled", "Canceled before Git started."));
        }
        if cherry.clear_only {
            clear_journal(repo, cherry.journal.as_ref().unwrap())?;
            return Ok(ActionOutcome {
                exit_code: 0,
                output_truncated: false,
            });
        }
        if cherry.start {
            write_journal(repo, cherry.journal.as_ref().unwrap())?;
        }
        let result = self
            .runner
            .run(&repo.root, &plan.args, None, true, runner::DIAGNOSTIC_LIMIT)
            .await
            .and_then(runner::checked);
        if let Err(mut failure) = result {
            match self.observe(repo).await {
                Ok(snapshot) => {
                    let status = snapshot.status.unwrap();
                    failure.message.push_str(&format!(" Observed HEAD: {:?}; Git state: {:?}. Refresh and inspect before retrying; a stopped pick may need Continue, Skip, or Abort.",status.head,status.git_state));
                }
                Err(error) => {
                    failure.code = "outcome_uncertain".into();
                    failure.message.push_str(&format!(
                        " Final state could not be inspected: {}",
                        error.message
                    ));
                }
            }
            return Err(failure);
        }
        let output = result.unwrap();
        let verification = async {
        let head = self
            .runner
            .read(&repo.root, &["symbolic-ref", "--quiet", "HEAD"])
            .await?;
        let oid = self
            .runner
            .read(
                &repo.root,
                &["--no-replace-objects", "rev-parse", "--verify", "HEAD"],
            )
            .await?;
        let actual = text(&oid);
        let valid = if cherry.commit {
            self.cherry_source(repo, &actual).await?.parents == vec![cherry.head.clone()]
        } else {
            actual == cherry.head
        };
        if text(&head) != cherry.branch
            || !valid
            || repo.git_dir.join("CHERRY_PICK_HEAD").exists()
            || repo.git_dir.join("sequencer").exists()
        {
            return Err(AppError::new("outcome_uncertain","Git returned success but the resulting branch, parent, or operation markers differ from the reviewed cherry-pick. No rollback was attempted; inspect actual history and files."));
        }
        if let Some(journal) = &cherry.journal {
            clear_journal(repo, journal).map_err(|mut e| {
                e.code = "outcome_uncertain".into();
                e
            })?;
        }
            Ok::<(),AppError>(())
        }.await;
        verification.map_err(|mut error| { error.code="outcome_uncertain".into(); error.message=format!("Git completed but verification failed: {}. Inspect actual history and files; no rollback was attempted.",error.message);error.exit_code=Some(output.code);error.output_truncated=Some(output.truncated);error })?;
        Ok(ActionOutcome {
            exit_code: output.code,
            output_truncated: output.truncated,
        })
    }
}
