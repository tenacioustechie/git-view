use super::*;
use std::{
    io::Write,
    sync::atomic::{AtomicBool, Ordering},
};

const JOURNAL: &str = "git-view-rebase.json";
const MAX_COMMITS: usize = 100;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RebaseJournal {
    version: u32,
    operation_id: String,
    branch: String,
    original_head: String,
    onto_ref: String,
    onto_oid: String,
    base_oid: String,
    commits: Vec<String>,
    backup_ref: String,
}

#[derive(Clone, PartialEq, Eq)]
enum Step {
    Start,
    Continue,
    Skip,
    Abort,
}

#[derive(Clone)]
pub(crate) struct RebasePlan {
    journal: RebaseJournal,
    step: Step,
    preview: Option<RebasePreview>,
}

impl RebasePlan {
    pub(crate) fn preview(&self) -> Option<RebasePreview> {
        self.preview.clone()
    }
}

fn error(message: impl Into<String>) -> AppError {
    AppError::new("rebase_unavailable", message)
}
fn oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn read_journal(repo: &Repository) -> AppResult<Option<RebaseJournal>> {
    let Some(bytes) = merge::read_metadata(&repo.git_dir.join(JOURNAL), 64 * 1024)? else {
        return Ok(None);
    };
    let value: RebaseJournal = serde_json::from_slice(&bytes).map_err(|_| {
        error("The rebase record is corrupt. Preserve it and review the repository manually.")
    })?;
    if value.version != 1
        || uuid::Uuid::parse_str(&value.operation_id).is_err()
        || !value.branch.starts_with("refs/heads/")
        || !value.onto_ref.starts_with("refs/")
        || !oid(&value.original_head)
        || !oid(&value.onto_oid)
        || !oid(&value.base_oid)
        || value.commits.is_empty()
        || value.commits.len() > MAX_COMMITS
        || value.commits.iter().any(|v| !oid(v))
        || value.backup_ref != format!("refs/git-view/rebase-backups/{}", value.operation_id)
    {
        return Err(error(
            "The rebase record is unsupported. Preserve it for manual review.",
        ));
    }
    Ok(Some(value))
}
fn write_journal(repo: &Repository, value: &RebaseJournal) -> AppResult<()> {
    let mut file =
        tempfile::NamedTempFile::new_in(&repo.git_dir).map_err(|e| error(e.to_string()))?;
    file.write_all(&serde_json::to_vec(value).unwrap())
        .and_then(|()| file.as_file().sync_all())
        .map_err(|e| error(e.to_string()))?;
    file.persist_noclobber(repo.git_dir.join(JOURNAL))
        .map_err(|e| error(e.to_string()))?;
    #[cfg(unix)]
    std::fs::File::open(&repo.git_dir)
        .and_then(|f| f.sync_all())
        .map_err(|e| error(e.to_string()))?;
    Ok(())
}
fn clear_journal(repo: &Repository, value: &RebaseJournal) -> AppResult<()> {
    let actual = read_journal(repo)?.ok_or_else(|| error("The rebase record disappeared."))?;
    if serde_json::to_vec(&actual).unwrap() != serde_json::to_vec(value).unwrap() {
        return Err(error(
            "The rebase record changed. Preserve it for manual review.",
        ));
    }
    std::fs::remove_file(repo.git_dir.join(JOURNAL)).map_err(|e| error(e.to_string()))
}
fn marker(repo: &Repository, name: &str) -> AppResult<Option<String>> {
    let Some(bytes) = merge::read_metadata(&repo.git_dir.join(name), 64 * 1024)? else {
        return Ok(None);
    };
    let value = std::str::from_utf8(&bytes)
        .map_err(|_| error("Invalid Git rebase metadata."))?
        .trim();
    if value.len() > 4096 || value.bytes().any(|b| matches!(b, b'\n' | b'\r' | 0)) {
        return Err(error("Invalid Git rebase metadata."));
    }
    Ok(Some(value.to_owned()))
}
fn metadata_dir(repo: &Repository) -> Option<&str> {
    if repo.git_dir.join("rebase-merge").is_dir() {
        Some("rebase-merge")
    } else if repo.git_dir.join("rebase-apply").is_dir() {
        Some("rebase-apply")
    } else {
        None
    }
}

impl GitEngine {
    pub(crate) async fn rebase_context(
        &self,
        repo: &Repository,
        status: &mut RepositoryStatus,
    ) -> AppResult<bool> {
        let journal = read_journal(repo)?;
        if journal.is_none() && status.git_state != GitState::Rebase {
            return Ok(false);
        }
        if journal.is_some() && status.git_state == GitState::Idle {
            status.git_state = GitState::Other
        }
        let dir = metadata_dir(repo);
        let current = marker(repo, "REBASE_HEAD")?;
        let head_name = if let Some(dir) = dir {
            marker(repo, &format!("{dir}/head-name"))?
        } else {
            None
        };
        let original = if let Some(dir) = dir {
            marker(repo, &format!("{dir}/orig-head"))?
        } else {
            None
        };
        let onto = if let Some(dir) = dir {
            marker(repo, &format!("{dir}/onto"))?
        } else {
            None
        };
        let msgnum = if let Some(dir) = dir {
            marker(repo, &format!("{dir}/msgnum"))?.and_then(|s| s.parse::<usize>().ok())
        } else {
            None
        };
        let end = if let Some(dir) = dir {
            marker(repo, &format!("{dir}/end"))?.and_then(|s| s.parse::<usize>().ok())
        } else {
            None
        };
        let mut context = OperationContext {
            kind: GitState::Rebase,
            source_oids: current.clone().into_iter().collect(),
            source_ref: journal.as_ref().map(|j| j.onto_ref.clone()),
            original_head: journal
                .as_ref()
                .map(|j| j.original_head.clone())
                .or(original.clone()),
            target_branch: journal
                .as_ref()
                .map(|j| j.branch.trim_start_matches("refs/heads/").to_owned())
                .or_else(|| {
                    head_name
                        .as_deref()
                        .and_then(|v| v.strip_prefix("refs/heads/"))
                        .map(str::to_owned)
                }),
            stop_reason: OperationStopReason::Unknown,
            can_continue: false,
            can_abort: false,
            can_skip: false,
            reason: String::new(),
            rebase: Some(RebaseProgress {
                onto_ref: journal.as_ref().map(|j| j.onto_ref.clone()),
                onto_oid: journal
                    .as_ref()
                    .map(|j| j.onto_oid.clone())
                    .or(onto.clone()),
                current_oid: current.clone(),
                completed: msgnum.map(|n| n.saturating_sub(1)),
                total: end,
                app_owned: journal.is_some(),
            }),
        };
        let valid = journal.as_ref().is_some_and(|j| {
            status.git_state == GitState::Rebase
                && dir == Some("rebase-merge")
                && head_name.as_deref() == Some(&j.branch)
                && original.as_deref() == Some(&j.original_head)
                && onto.as_deref() == Some(&j.onto_oid)
                && current.as_ref().is_some_and(|v| j.commits.contains(v))
                && matches!(status.head, Head::Detached { .. })
                && msgnum.is_some_and(|n| n > 0 && n <= j.commits.len())
                && end == Some(j.commits.len())
        });
        if !valid {
            context.reason = if journal.is_some() {
                "The recorded rebase does not match Git's current metadata. Inspect the branch, files, and saved backup; use Git externally for recovery.".into()
            } else {
                "This rebase was started outside Git View. Inspect its state and use Git externally to continue, skip, or abort.".into()
            };
        } else if status.counts.conflicted > 0 {
            context.stop_reason = OperationStopReason::Conflicts;
            context.can_abort = true;
            context.can_skip = status.counts.untracked == 0 && !status.submodule_dirty;
            context.reason = "Resolve files externally, Refresh, and Stage resolution. During rebase, ours is the chosen base plus replayed commits; theirs is the commit being replayed. Reviewed Skip omits that commit and may discard resolution edits.".into();
        } else {
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
            context.can_abort = true;
            context.can_skip = status.counts.untracked == 0 && !status.submodule_dirty;
            if out.code == 0 {
                context.stop_reason = OperationStopReason::Empty;
                context.reason = "This commit has no staged change. Continue may drop it. Review Skip to omit it explicitly, or Abort; preserve an empty commit with Git externally.".into();
            } else if out.code == 1 {
                context.stop_reason = OperationStopReason::Ready;
                context.can_continue = status.counts.unstaged == 0
                    && status.counts.untracked == 0
                    && !status.submodule_dirty;
                context.reason = "Review staged resolution and the current commit message, then Continue. Normal Git hooks and signing still apply. Reviewed Skip omits this commit.".into();
            } else {
                runner::checked(out)?;
            }
        }
        status.operation_context = Some(context);
        Ok(true)
    }

    pub(crate) async fn prepare_rebase(
        &self,
        repo: &Repository,
        status: &RepositoryStatus,
        action: &RepositoryAction,
    ) -> AppResult<(RebasePlan, Vec<OsString>, String)> {
        if let RepositoryAction::StartRebase { onto_ref, .. } = action {
            let (branch, head) = match &status.head {
                Head::Branch { name, oid } => (format!("refs/heads/{name}"), oid.clone()),
                _ => {
                    return Err(error(
                        "Rebase requires an attached local branch with an existing commit.",
                    ))
                }
            };
            if status.git_state != GitState::Idle || status.dirty || status.submodule_dirty {
                return Err(error("Start rebase with a clean index and worktree, including untracked files and submodules, and no Git operation in progress."));
            }
            if !((onto_ref.starts_with("refs/heads/") || onto_ref.starts_with("refs/remotes/"))
                && !onto_ref.ends_with("/HEAD")
                && onto_ref != &branch)
            {
                return Err(error(
                    "Choose a different existing local or fetched remote branch.",
                ));
            }
            let reference = self
                .list_refs(&repo.id)
                .await?
                .into_iter()
                .find(|r| r.full_name == *onto_ref && (r.kind == "local" || r.kind == "remote"))
                .ok_or_else(|| {
                    error("The chosen target is unavailable. Fetch or refresh branches.")
                })?;
            if !oid(&reference.oid) {
                return Err(error("The chosen target does not resolve to a commit."));
            }
            let onto_oid = self.resolve_commit(repo, onto_ref).await?;
            if onto_oid != reference.oid {
                return Err(error("The chosen target must point directly to a commit."));
            }
            if text(
                &self
                    .runner
                    .read(&repo.root, &["rev-parse", "--is-shallow-repository"])
                    .await?,
            ) != "false"
            {
                return Err(error(
                    "Shallow ancestry cannot be fully reviewed for rebase.",
                ));
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
                    .is_some_and(|v| !v.is_empty())
            {
                return Err(error(
                    "Replacement refs or grafts require manual review before rebase.",
                ));
            }
            let bases = text(
                &self
                    .runner
                    .read(
                        &repo.root,
                        &[
                            "--no-replace-objects",
                            "merge-base",
                            "--all",
                            &head,
                            &onto_oid,
                        ],
                    )
                    .await?,
            );
            let parts: Vec<_> = bases.lines().collect();
            if parts.len() != 1 || !oid(parts[0]) {
                return Err(error("Rebase requires one shared merge base."));
            }
            let base_oid = parts[0].to_owned();
            if base_oid == head || base_oid == onto_oid {
                return Err(error("The selected branches have no divergent commits to replay onto a newer target."));
            }
            let range = format!("{base_oid}..{head}");
            let lines = text(
                &self
                    .runner
                    .read(
                        &repo.root,
                        &[
                            "--no-replace-objects",
                            "rev-list",
                            "--reverse",
                            "--topo-order",
                            "--parents",
                            &range,
                        ],
                    )
                    .await?,
            );
            let mut commits = Vec::new();
            for line in lines.lines() {
                let fields: Vec<_> = line.split_whitespace().collect();
                if fields.len() != 2 || !fields.iter().all(|v| oid(v)) {
                    return Err(error("The source range contains a merge or incomplete ancestry; only linear commits are supported."));
                }
                commits.push(fields[0].to_owned());
                if commits.len() > MAX_COMMITS {
                    return Err(error("The first rebase workflow supports at most 100 commits; no commits were omitted from the preview."));
                }
            }
            if commits.is_empty() {
                return Err(error("No source commits are available to replay."));
            }
            let mut descriptions = Vec::new();
            let mut paths = self.tree_paths(repo, &onto_oid).await?;
            for commit in &commits {
                let subject = text(
                    &self
                        .runner
                        .read(
                            &repo.root,
                            &["--no-replace-objects", "log", "-1", "--format=%s", commit],
                        )
                        .await?,
                );
                descriptions.push(RebaseCommit {
                    oid: commit.clone(),
                    subject,
                });
                paths.extend(self.tree_paths(repo, commit).await?);
            }
            self.protect_materialized_paths(repo, &paths).await?;
            // Directory rename detection can relocate files beyond the named tree paths.
            // Reject ignored material conservatively until that case has a full path model.
            let ignored = self
                .runner
                .read(
                    &repo.root,
                    &[
                        "ls-files",
                        "--others",
                        "--ignored",
                        "--exclude-standard",
                        "-z",
                    ],
                )
                .await?;
            if !ignored.is_empty() {
                return Err(error("Move ignored files outside the worktree before rebasing; directory renames could overwrite them."));
            }
            let operation_id = id();
            let journal = RebaseJournal {
                version: 1,
                backup_ref: format!("refs/git-view/rebase-backups/{operation_id}"),
                operation_id,
                branch: branch.clone(),
                original_head: head.clone(),
                onto_ref: onto_ref.clone(),
                onto_oid: onto_oid.clone(),
                base_oid: base_oid.clone(),
                commits,
            };
            let commit_count = descriptions.len();
            let preview = RebasePreview {
                branch: branch.clone(),
                original_head: head,
                onto_ref: onto_ref.clone(),
                onto_oid: onto_oid.clone(),
                base_oid: base_oid.clone(),
                commits: descriptions,
            };
            let args = runner::args(&[
                "--no-replace-objects",
                "-c",
                "submodule.recurse=false",
                "rebase",
                "--merge",
                "--no-fork-point",
                "--no-rebase-merges",
                "--no-update-refs",
                "--no-autostash",
                "--no-rerere-autoupdate",
                "--reapply-cherry-picks",
                "--keep-empty",
                "--empty=stop",
                "--onto",
                &onto_oid,
                &base_oid,
            ]);
            Ok((RebasePlan { journal, step: Step::Start, preview: Some(preview) }, args,
                format!("Rebase {} onto {} at {}. Replay {} linear commits; their IDs will change. A normal push may reject previously published commits. A backup ref is created. No fetch or push is performed.", branch, onto_ref, onto_oid, commit_count)))
        } else {
            let context = status
                .operation_context
                .as_ref()
                .filter(|c| c.kind == GitState::Rebase)
                .ok_or_else(|| error("No supported rebase is in progress."))?;
            let (step, allowed, flag) = match action {
                RepositoryAction::ContinueRebase { .. } => {
                    (Step::Continue, context.can_continue, "--continue")
                }
                RepositoryAction::SkipRebase { .. } => (Step::Skip, context.can_skip, "--skip"),
                RepositoryAction::AbortRebase { .. } => (Step::Abort, context.can_abort, "--abort"),
                _ => unreachable!(),
            };
            if !allowed {
                return Err(error(&context.reason));
            }
            let journal = read_journal(repo)?.ok_or_else(|| {
                error("Only a rebase started by Git View can use these controls.")
            })?;
            if step == Step::Abort {
                self.protect_materialized_paths(
                    repo,
                    &self.tree_paths(repo, &journal.original_head).await?,
                )
                .await?;
            } else {
                let mut paths = self.tree_paths(repo, &journal.onto_oid).await?;
                for commit in &journal.commits {
                    paths.extend(self.tree_paths(repo, commit).await?);
                }
                self.protect_materialized_paths(repo, &paths).await?;
                if !self
                    .runner
                    .read(
                        &repo.root,
                        &[
                            "ls-files",
                            "--others",
                            "--ignored",
                            "--exclude-standard",
                            "-z",
                        ],
                    )
                    .await?
                    .is_empty()
                {
                    return Err(error("Move ignored files outside the worktree before continuing or skipping; a later replay could overwrite them."));
                }
            }
            let source = context.source_oids.first().cloned().unwrap_or_default();
            let description = match step {
                Step::Continue => format!("Continue replaying {source} on {}. Use Git's current message, hooks, and signing. The rebase may stop again.", journal.branch),
                Step::Skip => format!("Skip source commit {source} during rebase of {}. Its change is omitted from the resulting branch; current resolution edits may be discarded. The rebase may stop again.", journal.branch),
                Step::Abort => format!("Abort rebase of {} and ask Git to restore original HEAD {}. Current resolution edits may be discarded. Inspect actual files and refs afterward.", journal.branch, journal.original_head),
                Step::Start => unreachable!(),
            };
            Ok((
                RebasePlan {
                    journal,
                    step,
                    preview: None,
                },
                runner::args(&[
                    "--no-replace-objects",
                    "-c",
                    "submodule.recurse=false",
                    "rebase",
                    flag,
                ]),
                description,
            ))
        }
    }

    pub(crate) async fn execute_rebase(
        &self,
        plan: &PreparedAction,
        rebase: &RebasePlan,
        canceled: &AtomicBool,
    ) -> AppResult<ActionOutcome> {
        let repo = &plan.repository;
        if canceled.load(Ordering::Acquire) {
            return Err(AppError::new("canceled", "Canceled before Git started."));
        }
        let observed = self.observe(repo).await?;
        let (fresh, _, _) = self
            .prepare_rebase(repo, observed.status.as_ref().unwrap(), &plan.action)
            .await?;
        if observed.fingerprint != *plan.fingerprint.as_ref().unwrap()
            || fresh.journal.branch != rebase.journal.branch
            || fresh.journal.original_head != rebase.journal.original_head
            || fresh.journal.onto_oid != rebase.journal.onto_oid
            || fresh.journal.base_oid != rebase.journal.base_oid
            || fresh.journal.commits != rebase.journal.commits
            || self.config_fingerprint(repo).await? != plan.config_fingerprint
        {
            return Err(AppError::new(
                "precondition_changed",
                "Rebase context changed after preview.",
            ));
        }
        if canceled.load(Ordering::Acquire) {
            return Err(AppError::new("canceled", "Canceled before Git started."));
        }
        if rebase.step == Step::Start {
            let backup = runner::checked(
                self.runner
                    .run(
                        &repo.root,
                        &runner::args(&[
                            "update-ref",
                            "--create-reflog",
                            &rebase.journal.backup_ref,
                            &rebase.journal.original_head,
                        ]),
                        None,
                        true,
                        runner::DIAGNOSTIC_LIMIT,
                    )
                    .await?,
            )?;
            let _ = backup;
            write_journal(repo, &rebase.journal)?;
        }
        let output = self
            .runner
            .run(&repo.root, &plan.args, None, true, runner::DIAGNOSTIC_LIMIT)
            .await?;
        let after = self.observe(repo).await?;
        let status = after.status.unwrap();
        if output.code != 0 {
            let mut failure = match runner::checked(output) {
                Err(error) => error,
                Ok(_) => unreachable!(),
            };
            if rebase.step == Step::Start
                && metadata_dir(repo).is_none()
                && !status.dirty
                && matches!(&status.head, Head::Branch { name, oid }
                    if format!("refs/heads/{name}") == rebase.journal.branch
                    && oid == &rebase.journal.original_head)
                && self
                    .resolve_commit(repo, &rebase.journal.backup_ref)
                    .await
                    .ok()
                    .as_deref()
                    == Some(&rebase.journal.original_head)
                && self
                    .resolve_commit(repo, &rebase.journal.onto_ref)
                    .await
                    .ok()
                    .as_deref()
                    == Some(&rebase.journal.onto_oid)
            {
                clear_journal(repo, &rebase.journal)?;
                failure.message.push_str(" No active rebase remains; the branch and working tree are at the reviewed original state. The backup ref is retained.");
                return Err(failure);
            }
            if status.git_state == GitState::Rebase
                && status.operation_context.as_ref().is_some_and(|c| {
                    c.kind == GitState::Rebase && c.stop_reason != OperationStopReason::Unknown
                })
            {
                failure.code = "rebase_stopped".into();
                failure.message = format!(
                    "Rebase stopped at {}. {}",
                    status
                        .operation_context
                        .as_ref()
                        .unwrap()
                        .source_oids
                        .join(", "),
                    status.operation_context.as_ref().unwrap().reason
                );
            } else {
                failure.code = "outcome_uncertain".into();
                failure.message.push_str(" Inspect actual refs, working files, backup, and rebase record before retrying.");
            }
            return Err(failure);
        }
        let branch = text(
            &self
                .runner
                .read(&repo.root, &["symbolic-ref", "--quiet", "HEAD"])
                .await?,
        );
        let head = text(
            &self
                .runner
                .read(
                    &repo.root,
                    &["--no-replace-objects", "rev-parse", "--verify", "HEAD"],
                )
                .await?,
        );
        let expected_head = if rebase.step == Step::Abort {
            Some(rebase.journal.original_head.as_str())
        } else {
            None
        };
        if status.git_state == GitState::Rebase
            || branch != rebase.journal.branch
            || expected_head.is_some_and(|v| v != head)
        {
            return Err(AppError::new("outcome_uncertain", "Git reported rebase completion but the branch, HEAD, or operation state differs. Inspect the saved backup and files."));
        }
        if rebase.step != Step::Abort
            && self.resolve_commit(repo, &rebase.journal.onto_ref).await? != rebase.journal.onto_oid
        {
            return Err(AppError::new("outcome_uncertain", "The selected target changed during rebase. Inspect final history and saved backup."));
        }
        clear_journal(repo, &rebase.journal).map_err(|mut e| {
            e.code = "outcome_uncertain".into();
            e
        })?;
        Ok(ActionOutcome {
            exit_code: output.code,
            output_truncated: output.truncated,
        })
    }
}
