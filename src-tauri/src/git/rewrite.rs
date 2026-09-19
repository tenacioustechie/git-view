use super::*;
use super::{ref_guard::HeadGuard, rewrite_config::FrozenConfig};
use std::{
    io::{Read, Write},
    sync::atomic::{AtomicBool, Ordering},
};
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Journal {
    #[serde(skip)]
    disk_digest: Option<String>,
    version: u32,
    root_hash: String,
    config_fingerprint: String,
    status_fingerprint: String,
    index_digest: String,
    pub record: RewriteRecord,
}
#[derive(Clone)]
pub(crate) enum PreparedRewrite {
    Combine {
        journal: Journal,
        config: FrozenConfig,
    },
    Recover {
        journal: Journal,
    },
}
fn directory(repo: &Repository) -> PathBuf {
    repo.git_dir.join("git-view/rewrites")
}
fn journal_path(repo: &Repository, id: &str) -> PathBuf {
    directory(repo).join(format!("{id}.json"))
}
fn workspace(repo: &Repository, id: &str) -> PathBuf {
    directory(repo).join(format!("{id}.worktree"))
}
fn invalid(message: impl Into<String>) -> AppError {
    AppError::new("rewrite_manual_review", message)
}
fn oid_valid(oid: &str) -> bool {
    !oid.is_empty() && oid.bytes().all(|byte| byte.is_ascii_hexdigit())
}
fn index_digest(repo: &Repository) -> AppResult<String> {
    Ok(
        match merge::read_metadata(&repo.git_dir.join("index"), 32 * 1024 * 1024)? {
            Some(bytes) => digest(&bytes),
            None => "missing".into(),
        },
    )
}
fn save(repo: &Repository, journal: &mut Journal, create: bool) -> AppResult<()> {
    if !create {
        check_journal(repo, journal)?;
    }
    let dir = directory(repo);
    std::fs::create_dir_all(&dir).map_err(|e| invalid(e.to_string()))?;
    let bytes = serde_json::to_vec(journal).map_err(|e| invalid(e.to_string()))?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err(invalid("Rewrite journal exceeds its bounded size."));
    }
    let mut file = tempfile::NamedTempFile::new_in(&dir).map_err(|e| invalid(e.to_string()))?;
    file.write_all(&bytes)
        .and_then(|()| file.as_file().sync_all())
        .map_err(|e| invalid(e.to_string()))?;
    let path = journal_path(repo, &journal.record.recovery_id);
    if create {
        file.persist_noclobber(path)
            .map_err(|e| invalid(e.to_string()))?;
    } else {
        file.persist(path).map_err(|e| invalid(e.to_string()))?;
    }
    #[cfg(unix)]
    std::fs::File::open(&dir)
        .and_then(|file| file.sync_all())
        .map_err(|e| invalid(e.to_string()))?;
    journal.disk_digest = Some(digest(&bytes));
    Ok(())
}
fn check_journal(repo: &Repository, journal: &Journal) -> AppResult<()> {
    let bytes = merge::read_metadata(
        &journal_path(repo, &journal.record.recovery_id),
        2 * 1024 * 1024,
    )?;
    if bytes.as_ref().map(|bytes| digest(bytes)) != journal.disk_digest {
        return Err(AppError::new(
            "precondition_changed",
            "Recovery journal changed outside this operation; its exact contents were preserved.",
        ));
    }
    Ok(())
}
fn load(repo: &Repository, id: &str) -> AppResult<Journal> {
    if uuid::Uuid::parse_str(id)
        .map(|value| value.to_string())
        .ok()
        .as_deref()
        != Some(id)
    {
        return Err(invalid("Invalid recovery identifier."));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(journal_path(repo, id))
        .and_then(|file| file.take(2 * 1024 * 1024 + 1).read_to_end(&mut bytes))
        .map_err(|e| invalid(e.to_string()))?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err(invalid("Rewrite journal exceeds its bounded size."));
    }
    let mut journal: Journal = serde_json::from_slice(&bytes)
        .map_err(|e| invalid(format!("Rewrite journal is corrupt: {e}")))?;
    journal.disk_digest = Some(digest(&bytes));
    let record = &journal.record;
    if journal.version != 1
        || record.recovery_id != id
        || journal.root_hash != digest(&os_bytes(repo.root.as_os_str()))
        || record.backup_ref != format!("refs/git-view/backups/{id}")
        || record
            .replacement_ref
            .as_ref()
            .is_some_and(|reference| reference != &format!("refs/git-view/replacements/{id}"))
        || !record.branch.starts_with("refs/heads/")
        || record.branch.chars().any(char::is_whitespace)
        || !oid_valid(&record.original_head)
        || !oid_valid(&record.base_oid)
        || !oid_valid(&record.tree_oid)
        || record
            .replacement_head
            .as_ref()
            .is_some_and(|oid| !oid_valid(oid))
        || record.commit_oids.len() < 2
        || record.commit_oids.len() > 1000
        || record.commit_oids.first() != Some(&record.original_head)
        || record.commit_oids.iter().any(|oid| !oid_valid(oid))
    {
        return Err(invalid(
            "Rewrite journal identity, paths or references failed validation; nothing was changed.",
        ));
    }
    if ![
        "prepared",
        "backupCreated",
        "worktreeCreated",
        "candidateCommitted",
        "candidateReady",
        "installing",
        "installed",
        "recovering",
        "recovered",
        "failed",
    ]
    .contains(&record.phase.as_str())
    {
        return Err(invalid(
            "Unknown rewrite journal phase; manual review is required.",
        ));
    }
    Ok(journal)
}
impl GitEngine {
    fn rewrite_checkpoint(&self, _point: &str) -> AppResult<()> {
        #[cfg(test)]
        if self.rewrite_failure.lock().unwrap().as_deref() == Some(_point) {
            return Err(AppError::new(
                "simulated_interruption",
                format!("Interrupted at {_point}"),
            ));
        }
        Ok(())
    }
    async fn direct_ref(&self, repo: &Repository, reference: &str) -> AppResult<Option<String>> {
        let symbolic = self
            .runner
            .run(
                &repo.root,
                &runner::args(&["symbolic-ref", "--quiet", reference]),
                None,
                false,
                runner::STRUCTURED_LIMIT,
            )
            .await?;
        if symbolic.code == 0 {
            return Err(invalid(format!(
                "Reference {reference} became symbolic; manual review is required."
            )));
        }
        if symbolic.code != 1 {
            runner::checked(symbolic)?;
        }
        let value = self
            .runner
            .run(
                &repo.root,
                &runner::args(&[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    "--end-of-options",
                    reference,
                ]),
                None,
                false,
                runner::STRUCTURED_LIMIT,
            )
            .await?;
        if value.code == 1 {
            return Ok(None);
        }
        Ok(Some(text(&runner::checked(value)?.stdout)))
    }
    async fn transaction(
        &self,
        path: &Path,
        prefix: &[OsString],
        body: String,
    ) -> AppResult<runner::Output> {
        let mut args = prefix.to_vec();
        args.extend(runner::args(&["update-ref", "--stdin"]));
        runner::checked(
            self.runner
                .run(
                    path,
                    &args,
                    Some(body.into_bytes()),
                    true,
                    runner::DIAGNOSTIC_LIMIT,
                )
                .await?,
        )
    }
    async fn guard_capability(&self, repo: &Repository) -> AppResult<()> {
        let body=format!("start\noption no-deref\nsymref-verify refs/git-view/capability-probe/{}\nprepare\nabort\n",id());
        self.transaction(&repo.root,&[],body).await.map_err(|error|{let mut result=AppError::new("unsupported_git","Combining commits requires Git symbolic-reference transactions and successful reference hooks. Upgrade Git or review its hook failure before retrying.");result.details=error.details;result})?;
        Ok(())
    }
    async fn tree_parent(&self, repo: &Repository, oid: &str) -> AppResult<(String, Vec<String>)> {
        let bytes = self
            .runner
            .read(
                &repo.root,
                &["--no-replace-objects", "cat-file", "commit", oid],
            )
            .await?;
        let mut tree = None;
        let mut parents = Vec::new();
        for line in bytes.split(|byte| *byte == b'\n') {
            if line.is_empty() {
                break;
            }
            if let Some(value) = line.strip_prefix(b"tree ") {
                tree = Some(text(value));
            }
            if let Some(value) = line.strip_prefix(b"parent ") {
                parents.push(text(value));
            }
        }
        Ok((
            tree.ok_or_else(|| invalid("Raw commit object has no tree"))?,
            parents,
        ))
    }
    async fn raw_context(&self, repo: &Repository) -> AppResult<()> {
        let replacements = self
            .runner
            .read(
                &repo.root,
                &["for-each-ref", "--format=%(refname)", "refs/replace/"],
            )
            .await?;
        if !replacements.is_empty() || repo.common_dir.join("info/grafts").exists() {
            return Err(AppError::new("unsupported_configuration","Commit replacement refs or grafts require manual history review before combining or recovering."));
        }
        Ok(())
    }
    async fn validate_suffix(
        &self,
        repo: &Repository,
        head: &str,
        oids: &[String],
    ) -> AppResult<(String, String)> {
        if oids.len() < 2 || oids.len() > 1000 || oids.first().map(String::as_str) != Some(head) {
            return Err(AppError::new(
                "invalid_selection",
                "Select 2–1000 consecutive commits, newest first, ending at the current HEAD.",
            ));
        }
        self.raw_context(repo).await?;
        let shallow = std::fs::read_to_string(repo.common_dir.join("shallow")).unwrap_or_default();
        let mut seen = std::collections::HashSet::new();
        let mut base = String::new();
        let mut tree = String::new();
        for (index, oid) in oids.iter().enumerate() {
            if !oid_valid(oid) || !seen.insert(oid) || shallow.lines().any(|line| line == oid) {
                return Err(AppError::new(
                    "invalid_selection",
                    "Duplicate, invalid or shallow-boundary commits cannot be combined.",
                ));
            }
            let (current_tree, parents) = self.tree_parent(repo, oid).await?;
            if parents.len() != 1 {
                return Err(AppError::new(
                    "invalid_selection",
                    "Root and merge commits cannot be combined.",
                ));
            }
            if index == 0 {
                tree = current_tree;
            }
            if let Some(next) = oids.get(index + 1) {
                if &parents[0] != next {
                    return Err(AppError::new(
                        "invalid_selection",
                        "Selected commits must form an exact linear suffix.",
                    ));
                }
            } else {
                base = parents[0].clone();
                self.resolve_commit(repo, &base).await?;
            }
        }
        Ok((base, tree))
    }
    pub(crate) async fn prepare_combine(
        &self,
        repo: &Repository,
        status: &RepositoryStatus,
        fingerprint: &str,
        oids: &[String],
        message: &str,
    ) -> AppResult<PreparedRewrite> {
        if status.dirty || status.git_state != GitState::Idle {
            return Err(AppError::new(
                "dirty",
                "Combining commits requires a clean index/worktree and no ongoing operation.",
            ));
        }
        let (name, head) = match &status.head {
            Head::Branch { name, oid } => (name, oid),
            _ => {
                return Err(AppError::new(
                    "invalid_head",
                    "Combining commits requires an attached branch.",
                ))
            }
        };
        if message.trim().is_empty() || message.len() > 1024 * 1024 {
            return Err(AppError::new(
                "invalid_message",
                "Enter a nonblank combined message of at most 1 MiB.",
            ));
        }
        let branch = format!("refs/heads/{name}");
        if self.direct_ref(repo, &branch).await?.as_deref() != Some(head) {
            return Err(invalid(
                "The attached branch is not a direct reference to HEAD.",
            ));
        }
        let (base, tree) = self.validate_suffix(repo, head, oids).await?;
        self.guard_capability(repo).await?;
        let recovery_id = id();
        let record = RewriteRecord {
            recovery_id: recovery_id.clone(),
            phase: "prepared".into(),
            branch,
            original_head: head.clone(),
            replacement_head: None,
            base_oid: base,
            tree_oid: tree,
            backup_ref: format!("refs/git-view/backups/{recovery_id}"),
            replacement_ref: None,
            commit_oids: oids.to_vec(),
            message: message.into(),
            created_at: now(),
            error: None,
            can_recover: false,
        };
        Ok(PreparedRewrite::Combine {
            journal: Journal {
                disk_digest: None,
                version: 1,
                root_hash: digest(&os_bytes(repo.root.as_os_str())),
                config_fingerprint: self.config_fingerprint(repo).await?,
                status_fingerprint: fingerprint.into(),
                index_digest: index_digest(repo)?,
                record,
            },
            config: self.freeze_config(repo).await?,
        })
    }
    pub(crate) async fn prepare_recovery(
        &self,
        repo: &Repository,
        status: &RepositoryStatus,
        recovery_id: &str,
    ) -> AppResult<PreparedRewrite> {
        if status.git_state != GitState::Idle {
            return Err(invalid(
                "Finish the current Git operation before recovering combined history.",
            ));
        }
        self.raw_context(repo).await?;
        let journal = load(repo, recovery_id)?;
        let record = self.reconcile(repo, &journal).await?;
        if !record.can_recover {
            return Err(invalid(record.error.unwrap_or_else(|| {
                "This rewrite cannot be recovered automatically from the current branch state."
                    .into()
            })));
        }
        self.guard_capability(repo).await?;
        Ok(PreparedRewrite::Recover { journal })
    }
    async fn verify_candidate(
        &self,
        repo: &Repository,
        journal: &Journal,
        replacement: &str,
    ) -> AppResult<()> {
        let (tree, parents) = self.tree_parent(repo, replacement).await?;
        if tree != journal.record.tree_oid || parents != [journal.record.base_oid.clone()] {
            return Err(invalid("A hook or external change altered the replacement tree/parent. The original branch was not rewritten."));
        }
        let (tree, _) = self
            .tree_parent(repo, &journal.record.original_head)
            .await?;
        if tree != journal.record.tree_oid {
            return Err(invalid(
                "Original tree no longer matches the recorded object.",
            ));
        }
        Ok(())
    }
    async fn reconcile(&self, repo: &Repository, journal: &Journal) -> AppResult<RewriteRecord> {
        self.raw_context(repo).await?;
        let (base, tree) = self
            .validate_suffix(
                repo,
                &journal.record.original_head,
                &journal.record.commit_oids,
            )
            .await?;
        if base != journal.record.base_oid || tree != journal.record.tree_oid {
            return Err(invalid(
                "Journal base/tree do not match the original raw history.",
            ));
        }
        let mut record = journal.record.clone();
        record.can_recover = false;
        let backup = self.direct_ref(repo, &record.backup_ref).await?;
        let branch = self.direct_ref(repo, &record.branch).await?;
        if backup
            .as_deref()
            .is_some_and(|value| value != record.original_head)
        {
            record.phase = "manualReview".into();
            record.error =
                Some("The backup ref changed; it was preserved for manual review.".into());
            return Ok(record);
        }
        if let Some(replacement) = &record.replacement_head {
            let reference = record
                .replacement_ref
                .as_ref()
                .ok_or_else(|| invalid("Replacement anchor missing from journal."))?;
            if self.direct_ref(repo, reference).await?.as_deref() != Some(replacement) {
                record.phase = "manualReview".into();
                record.error = Some("Replacement anchor is missing or changed.".into());
                return Ok(record);
            }
            self.verify_candidate(repo, journal, replacement).await?;
            if branch.as_deref() == Some(replacement)
                && backup.as_deref() == Some(&record.original_head)
            {
                let head = self
                    .runner
                    .run(
                        &repo.root,
                        &runner::args(&["symbolic-ref", "--quiet", "HEAD"]),
                        None,
                        false,
                        runner::STRUCTURED_LIMIT,
                    )
                    .await?;
                let attached = if head.code == 1 {
                    false
                } else {
                    text(&runner::checked(head)?.stdout) == record.branch
                };
                record.phase = "installed".into();
                record.can_recover = attached
                    && workspace(repo, &record.recovery_id).is_dir()
                    && self
                        .observe(repo)
                        .await?
                        .status
                        .is_some_and(|status| status.git_state == GitState::Idle);
                record.error = if record.can_recover {
                    None
                } else {
                    Some("The worktree switched branches, is in an ongoing operation, or its retained candidate is missing; automatic recovery is blocked.".into())
                };
                return Ok(record);
            }
        }
        if branch.as_deref() == Some(&record.original_head) {
            if matches!(record.phase.as_str(), "recovering" | "recovered") {
                record.phase = "recovered".into();
                record.error = None;
            } else if record.phase != "failed" {
                record.phase = "notInstalled".into();
                record.error=Some("Preparation stopped without replacing the original branch. Backup/candidate state is retained; no hook or commit will be retried automatically.".into());
            }
        } else {
            record.phase = "manualReview".into();
            record.error=Some("The original branch contains later or different history; automatic recovery would overwrite it.".into());
        }
        Ok(record)
    }
    pub async fn list_rewrites(&self, repository_id: &str) -> AppResult<Vec<RewriteRecord>> {
        let repo = self.repository(repository_id).await?;
        let entries = match std::fs::read_dir(directory(&repo)) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(invalid(error.to_string())),
        };
        let mut records = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| invalid(e.to_string()))?;
            if entry.path().extension().and_then(|v| v.to_str()) != Some("json") {
                continue;
            }
            if records.len() >= 256 {
                return Err(invalid("Too many recovery journals to inspect safely."));
            }
            let path = entry.path();
            let id = path
                .file_stem()
                .and_then(|v| v.to_str())
                .ok_or_else(|| invalid("Invalid journal filename"))?;
            let journal = load(&repo, id)?;
            records.push(self.reconcile(&repo, &journal).await?);
        }
        records.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(records)
    }
    async fn persist_phase(
        &self,
        repo: &Repository,
        journal: &mut Journal,
        phase: &str,
    ) -> AppResult<()> {
        journal.record.phase = phase.into();
        save(repo, journal, false)?;
        self.rewrite_checkpoint(phase)
    }
    async fn revalidate_original(&self, repo: &Repository, journal: &Journal) -> AppResult<()> {
        self.raw_context(repo).await?;
        let current = self.observe(repo).await?;
        if current.fingerprint != journal.status_fingerprint
            || self.config_fingerprint(repo).await? != journal.config_fingerprint
            || index_digest(repo)? != journal.index_digest
        {
            return Err(AppError::new("precondition_changed","Original branch, index/worktree or configuration changed during replacement preparation."));
        }
        Ok(())
    }
    async fn guarded_cas(
        &self,
        repo: &Repository,
        path: &Path,
        prefix: &[OsString],
        journal: &Journal,
        old: &str,
        new: &str,
    ) -> AppResult<()> {
        let detached = self
            .runner
            .run(
                path,
                &runner::args(&["symbolic-ref", "--quiet", "HEAD"]),
                None,
                false,
                runner::STRUCTURED_LIMIT,
            )
            .await?;
        if detached.code != 1 {
            return Err(invalid(
                "Candidate worktree is no longer detached; original history was preserved.",
            ));
        }
        let before = self.observe(repo).await?.fingerprint;
        let config_before = self.config_fingerprint(repo).await?;
        let index_before = index_digest(repo)?;
        let guard = HeadGuard::acquire(&self.runner, repo, &journal.record.branch).await?;
        let result=async {
            let body=format!("option no-deref\nverify {} {}\noption no-deref\nverify {} {}\noption no-deref\nupdate {} {new} {old}\n",journal.record.backup_ref,journal.record.original_head,journal.record.replacement_ref.as_ref().unwrap(),journal.record.replacement_head.as_ref().unwrap(),journal.record.branch);
            let transaction=HeadGuard::prepare(&self.runner,path,prefix,body).await?;
            // The transaction now holds all ref locks. Validate direct identity while
            // no cooperating Git process can replace an anchor with a symbolic ref.
            if self.direct_ref(repo,&journal.record.backup_ref).await?.as_deref()!=Some(&journal.record.original_head)||self.direct_ref(repo,journal.record.replacement_ref.as_ref().unwrap()).await?.as_deref()!=journal.record.replacement_head.as_deref()||self.direct_ref(repo,&journal.record.branch).await?.as_deref()!=Some(old){return Err(invalid("Locked rewrite references differ from their recorded direct values."));}
            self.raw_context(repo).await?;
            if old==journal.record.original_head {self.revalidate_original(repo,journal).await?;}
            if self.observe(repo).await?.fingerprint != before || self.config_fingerprint(repo).await? != config_before || index_digest(repo)? != index_before { return Err(AppError::new("precondition_changed", "Original worktree or configuration changed while reference hooks ran.")); }
            check_journal(repo, journal)?;
            transaction.commit().await
        }.await;
        let release = guard.release().await;
        result?;
        release?;
        Ok(())
    }

    pub(crate) async fn execute_rewrite(
        &self,
        repo: &Repository,
        prepared: &PreparedRewrite,
        canceled: &Arc<AtomicBool>,
    ) -> AppResult<ActionOutcome> {
        let mut journal = match prepared {
            PreparedRewrite::Combine { journal, .. } | PreparedRewrite::Recover { journal } => {
                journal.clone()
            }
        };
        let result = match prepared {
            PreparedRewrite::Combine { config, .. } => {
                Box::pin(self.combine(repo, &mut journal, config, canceled)).await
            }
            PreparedRewrite::Recover { .. } => {
                Box::pin(self.recover(repo, &mut journal, canceled)).await
            }
        };
        if let Err(mut error) = result {
            let branch = self
                .direct_ref(repo, &journal.record.branch)
                .await
                .ok()
                .flatten();
            let mutated = match prepared {
                PreparedRewrite::Combine { .. } => {
                    branch.as_ref() == journal.record.replacement_head.as_ref()
                        && journal.record.replacement_head.is_some()
                }
                PreparedRewrite::Recover { .. } => {
                    branch.as_deref() == Some(&journal.record.original_head)
                }
            };
            if mutated {
                error.code = "outcome_uncertain".into();
                error.message=format!("History may have changed before reporting completed. Refresh recovery records and actual refs: {}",error.message);
            }
            if !mutated
                && error.code != "simulated_interruption"
                && error.code != "outcome_uncertain"
                && journal_path(repo, &journal.record.recovery_id).exists()
            {
                journal.record.phase = "failed".into();
                journal.record.error = Some(error.message.clone());
                let _ = save(repo, &mut journal, false);
            }
            return Err(error);
        }
        Ok(ActionOutcome {
            exit_code: 0,
            output_truncated: false,
        })
    }
    async fn combine(
        &self,
        repo: &Repository,
        journal: &mut Journal,
        config: &FrozenConfig,
        canceled: &Arc<AtomicBool>,
    ) -> AppResult<()> {
        save(repo, journal, true)?;
        self.rewrite_checkpoint("prepared")?;
        self.transaction(
            &repo.root,
            &[],
            format!(
                "start\noption no-deref\ncreate {} {}\nprepare\ncommit\n",
                journal.record.backup_ref, journal.record.original_head
            ),
        )
        .await?;
        self.rewrite_checkpoint("backupEffect")?;
        self.persist_phase(repo, journal, "backupCreated").await?;
        let path = workspace(repo, &journal.record.recovery_id);
        let mut args = runner::args(&["worktree", "add", "--detach", "--no-checkout"]);
        args.push(path.clone().into_os_string());
        args.push(journal.record.base_oid.clone().into());
        runner::checked(
            self.runner
                .run(&repo.root, &args, None, true, runner::DIAGNOSTIC_LIMIT)
                .await?,
        )?;
        self.rewrite_checkpoint("worktreeEffect")?;
        self.persist_phase(repo, journal, "worktreeCreated").await?;
        let prefix = self.candidate_config(repo, &path, config).await?;
        let mut args = prefix.clone();
        args.extend(runner::args(&[
            "read-tree",
            "--reset",
            "-u",
            &journal.record.original_head,
        ]));
        runner::checked(
            self.runner
                .run(&path, &args, None, true, runner::DIAGNOSTIC_LIMIT)
                .await?,
        )?;
        if canceled.load(Ordering::Acquire) {
            return Err(AppError::new(
                "canceled",
                "Combine preparation canceled; original history is unchanged.",
            ));
        }
        let mut args = prefix.clone();
        args.extend(runner::args(&["commit", "--allow-empty", "--file=-"]));
        runner::checked(
            self.runner
                .run(
                    &path,
                    &args,
                    Some(journal.record.message.as_bytes().to_vec()),
                    true,
                    runner::DIAGNOSTIC_LIMIT,
                )
                .await?,
        )?;
        self.rewrite_checkpoint("commitEffect")?;
        let replacement = text(&self.runner.read(&path, &["rev-parse", "HEAD"]).await?);
        self.verify_candidate(repo, journal, &replacement).await?;
        journal.record.replacement_head = Some(replacement.clone());
        journal.record.replacement_ref = Some(format!(
            "refs/git-view/replacements/{}",
            journal.record.recovery_id
        ));
        self.persist_phase(repo, journal, "candidateCommitted")
            .await?;
        self.transaction(
            &path,
            &prefix,
            format!(
                "start\noption no-deref\ncreate {} {replacement}\nprepare\ncommit\n",
                journal.record.replacement_ref.as_ref().unwrap()
            ),
        )
        .await?;
        self.rewrite_checkpoint("replacementEffect")?;
        self.persist_phase(repo, journal, "candidateReady").await?;
        self.revalidate_original(repo, journal).await?;
        if self
            .direct_ref(repo, &journal.record.backup_ref)
            .await?
            .as_deref()
            != Some(&journal.record.original_head)
        {
            return Err(invalid("Backup ref changed during preparation."));
        }
        if canceled.load(Ordering::Acquire) {
            return Err(AppError::new(
                "canceled",
                "Combine canceled before installing history; backup/candidate retained.",
            ));
        }
        self.persist_phase(repo, journal, "installing").await?;
        self.guarded_cas(
            repo,
            &path,
            &prefix,
            journal,
            &journal.record.original_head,
            &replacement,
        )
        .await?;
        self.rewrite_checkpoint("installEffect")?;
        if self
            .direct_ref(repo, &journal.record.branch)
            .await?
            .as_deref()
            != Some(&replacement)
        {
            return Err(invalid("Installed branch does not match its replacement."));
        }
        self.persist_phase(repo, journal, "installed").await?;
        Ok(())
    }
    async fn recover(
        &self,
        repo: &Repository,
        journal: &mut Journal,
        canceled: &Arc<AtomicBool>,
    ) -> AppResult<()> {
        let current = load(repo, &journal.record.recovery_id)?;
        if serde_json::to_vec(&current).ok() != serde_json::to_vec(journal).ok() {
            return Err(AppError::new(
                "precondition_changed",
                "Recovery journal changed after preview.",
            ));
        }
        if !self.reconcile(repo, journal).await?.can_recover {
            return Err(invalid("Branch or recovery anchors changed after preview."));
        }
        let path = workspace(repo, &journal.record.recovery_id);
        if !path.is_dir() {
            return Err(invalid("The retained candidate worktree is missing. Backup refs were preserved for manual recovery."));
        }
        let frozen = self.freeze_config(repo).await?;
        let prefix = self.candidate_config(repo, &path, &frozen).await?;
        if canceled.load(Ordering::Acquire) {
            return Err(AppError::new(
                "canceled",
                "Recovery canceled before changing refs.",
            ));
        }
        self.persist_phase(repo, journal, "recovering").await?;
        self.guarded_cas(
            repo,
            &path,
            &prefix,
            journal,
            journal.record.replacement_head.as_ref().unwrap(),
            &journal.record.original_head,
        )
        .await?;
        self.rewrite_checkpoint("recoverEffect")?;
        if self
            .direct_ref(repo, &journal.record.branch)
            .await?
            .as_deref()
            != Some(&journal.record.original_head)
        {
            return Err(invalid(
                "Recovery branch differs from the retained original tip.",
            ));
        }
        self.persist_phase(repo, journal, "recovered").await?;
        Ok(())
    }
}
