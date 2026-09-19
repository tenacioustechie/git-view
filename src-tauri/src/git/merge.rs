use super::*;
use std::io::{Read, Write};
const JOURNAL: &str = "git-view-squash.json";
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SquashJournal {
    pub version: u32,
    pub operation_id: String,
    pub phase: SquashPhase,
    pub original_branch: String,
    pub state: SquashMergeState,
}
#[derive(Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SquashPhase {
    Running,
    Review,
}
pub(crate) struct SquashActivity(Arc<std::sync::atomic::AtomicBool>);
impl SquashActivity {
    pub fn begin(repo: &Repository) -> Self {
        repo.squash_running
            .store(true, std::sync::atomic::Ordering::Release);
        Self(repo.squash_running.clone())
    }
}
impl Drop for SquashActivity {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::Release);
    }
}
pub(crate) fn read_metadata(path: &Path, limit: u64) -> AppResult<Option<Vec<u8>>> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(AppError::new(
                "operation_metadata_unavailable",
                error.to_string(),
            ))
        }
    };
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| AppError::new("operation_metadata_unavailable", error.to_string()))?;
    if bytes.len() as u64 > limit {
        return Err(AppError::new(
            "output_limit",
            "Git operation metadata exceeds its size limit; no partial observation was used.",
        ));
    }
    Ok(Some(bytes))
}
pub(crate) fn validate_context(journal: &SquashJournal, head: &Head) -> AppResult<()> {
    let matches = matches!(head,Head::Branch{name,oid} if format!("refs/heads/{name}")==journal.original_branch && *oid==journal.state.original_head);
    if !matches {
        return Err(AppError::new("squash_manual_review",format!("Squash review started on {} at {}, but the branch or HEAD changed externally. Its journal and all working changes were preserved. Review git-view-squash.json in the worktree Git directory; remove that application marker only after confirming the squash was completed or abandoned.",journal.original_branch,journal.state.original_head)));
    }
    Ok(())
}

pub(crate) fn read_squash(repo: &Repository) -> AppResult<Option<SquashJournal>> {
    let bytes = match read_metadata(&repo.git_dir.join(JOURNAL), 64 * 1024).map_err(|error| {
        AppError::new(
            "squash_journal_unavailable",
            format!(
                "Squash journal could not be read; manual review is required: {}",
                error.message
            ),
        )
    })? {
        Some(bytes) => bytes,
        None => return Ok(None),
    };
    let journal: SquashJournal = serde_json::from_slice(&bytes).map_err(|error| {
        AppError::new(
            "squash_journal_invalid",
            format!("Squash operation journal needs manual review: {error}"),
        )
    })?;
    if journal.version != 1
        || journal.operation_id.is_empty()
        || !journal.original_branch.starts_with("refs/heads/")
    {
        return Err(AppError::new(
            "squash_journal_invalid",
            "Unknown or incomplete squash journal; manual review is required.",
        ));
    }
    Ok(Some(journal))
}
pub(crate) fn write_squash(repo: &Repository, state: &SquashJournal) -> AppResult<()> {
    let mut file = tempfile::NamedTempFile::new_in(&repo.git_dir)
        .map_err(|error| AppError::new("squash_journal_unavailable", error.to_string()))?;
    let bytes = serde_json::to_vec(state)
        .map_err(|error| AppError::new("squash_journal_unavailable", error.to_string()))?;
    file.write_all(&bytes)
        .and_then(|()| file.as_file().sync_all())
        .map_err(|error| AppError::new("squash_journal_unavailable", error.to_string()))?;
    file.persist_noclobber(repo.git_dir.join(JOURNAL))
        .map_err(|error| {
            AppError::new(
                "squash_journal_unavailable",
                format!(
                    "Cannot record squash operation without replacing an existing journal: {error}"
                ),
            )
        })?;
    #[cfg(unix)]
    std::fs::File::open(&repo.git_dir)
        .and_then(|dir| dir.sync_all())
        .map_err(|error| AppError::new("squash_journal_unavailable", error.to_string()))?;
    Ok(())
}
pub(crate) fn finish_attempt(repo: &Repository, operation_id: &str) -> AppResult<()> {
    let mut journal = read_squash(repo)?.ok_or_else(|| {
        AppError::new(
            "squash_manual_review",
            "Squash journal disappeared during Git execution; manual review is required.",
        )
    })?;
    if journal.operation_id != operation_id || journal.phase != SquashPhase::Running {
        return Err(AppError::new(
            "squash_manual_review",
            "Squash journal changed during execution; manual review is required.",
        ));
    }
    journal.phase = SquashPhase::Review;
    let mut file = tempfile::NamedTempFile::new_in(&repo.git_dir)
        .map_err(|error| AppError::new("squash_journal_unavailable", error.to_string()))?;
    let bytes = serde_json::to_vec(&journal)
        .map_err(|error| AppError::new("squash_journal_unavailable", error.to_string()))?;
    file.write_all(&bytes)
        .and_then(|()| file.as_file().sync_all())
        .map_err(|error| AppError::new("squash_journal_unavailable", error.to_string()))?;
    file.persist(repo.git_dir.join(JOURNAL))
        .map_err(|error| AppError::new("squash_journal_unavailable", error.to_string()))?;
    #[cfg(unix)]
    std::fs::File::open(&repo.git_dir)
        .and_then(|dir| dir.sync_all())
        .map_err(|error| AppError::new("squash_journal_unavailable", error.to_string()))?;
    Ok(())
}
pub(crate) fn clear_squash(repo: &Repository) -> AppResult<()> {
    match std::fs::remove_file(repo.git_dir.join(JOURNAL)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(AppError::new(
            "squash_journal_unavailable",
            format!("Git completed, but its squash review marker could not be cleared: {error}"),
        )),
    }
}

impl GitEngine {
    pub(crate) async fn finish_squash_commit(&self, repo: &Repository) -> AppResult<()> {
        let journal=read_squash(repo)?.ok_or_else(||AppError::new("squash_manual_review","Git completed the commit, but its squash journal disappeared; manual review is required."))?;
        let branch=self.runner.read(&repo.root,&["symbolic-ref","--quiet","HEAD"]).await.map_err(|_|AppError::new("squash_manual_review","Git completed the commit, but the current branch cannot be verified. Its squash journal was preserved for manual review."))?;
        let commit = self
            .runner
            .read(&repo.root, &["rev-list", "--parents", "-n", "1", "HEAD"])
            .await?;
        let commit = text(&commit);
        let fields = commit.split_whitespace().collect::<Vec<_>>();
        if journal.phase != SquashPhase::Review
            || text(&branch) != journal.original_branch
            || fields.len() != 2
            || fields[1] != journal.state.original_head
        {
            return Err(AppError::new("squash_manual_review","Git completed the commit, but its branch or parent differs from the recorded squash context. The journal was preserved for manual review."));
        }
        clear_squash(repo)
    }
}
