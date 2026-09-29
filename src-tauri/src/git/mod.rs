mod actions;
mod cherry_pick;
mod discovery;
mod file_history;
mod history;
mod hunks;
mod merge;
mod rebase;
mod ref_guard;
mod remote_checkout;
mod rewrite;
mod rewrite_config;
mod runner;
mod stash;
mod stash_atomic;
mod status;
use crate::contracts::*;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    ffi::OsString,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::Mutex;

#[derive(Clone)]
pub(crate) struct Repository {
    pub id: String,
    pub root: PathBuf,
    pub git_dir: PathBuf,
    pub common_dir: PathBuf,
    pub lock: Arc<Mutex<()>>,
    pub squash_running: Arc<std::sync::atomic::AtomicBool>,
}
#[derive(Clone)]
pub(crate) struct FileEntry {
    pub wire: ChangedFile,
    pub path: OsString,
    pub old_path: Option<OsString>,
}
#[derive(Clone)]
pub(crate) struct Snapshot {
    pub repository_id: String,
    pub fingerprint: String,
    pub status: Option<RepositoryStatus>,
    pub files: Vec<FileEntry>,
    pub commit: Option<(String, Option<String>)>,
}
#[derive(Clone)]
pub(crate) struct HistorySnapshot {
    pub repository_id: String,
    pub tips: Vec<String>,
    pub scope: String,
    pub query: Option<HistorySearch>,
    pub shallow: Vec<String>,
    pub shallow_fingerprint: String,
    pub oid_matches: Option<Vec<String>>,
    pub refs: Vec<GitRef>,
    pub cursors: HashMap<String, usize>,
}
#[derive(Default)]
pub(crate) struct State {
    pub repositories: HashMap<String, Repository>,
    pub locks: HashMap<PathBuf, Arc<Mutex<()>>>,
    pub snapshots: HashMap<String, Snapshot>,
    pub history: HashMap<String, HistorySnapshot>,
    pub file_history: HashMap<String, file_history::FileHistorySnapshot>,
    pub hunks: std::collections::VecDeque<(String, Arc<hunks::HunkSnapshot>)>,
    pub stashes: HashMap<String, stash::StashSnapshot>,
    pub fetch_times: HashMap<String, String>,
}
#[derive(Clone, Default)]
pub struct GitEngine {
    pub(crate) state: Arc<Mutex<State>>,
    pub(crate) runner: runner::Runner,
    #[cfg(test)]
    pub(crate) rewrite_failure: Arc<std::sync::Mutex<Option<String>>>,
    #[cfg(test)]
    pub(crate) stash_failure: Arc<std::sync::Mutex<Option<String>>>,
}
#[derive(Clone)]
pub struct PreparedAction {
    pub summary: String,
    pub destination: Option<String>,
    pub(crate) repository: Repository,
    pub(crate) action: RepositoryAction,
    pub(crate) fingerprint: Option<String>,
    pub(crate) args: Vec<OsString>,
    pub(crate) input: Option<Vec<u8>>,
    pub(crate) config_fingerprint: String,
    pub(crate) target_ref: Option<(String, String)>,
    pub(crate) head_guard: Option<Head>,
    pub(crate) squash_start: Option<merge::SquashJournal>,
    pub(crate) finish_squash: bool,
    pub(crate) rewrite: Option<rewrite::PreparedRewrite>,
    pub(crate) remote_checkout: Option<remote_checkout::Checkout>,
    pub(crate) cherry: Option<cherry_pick::CherryPlan>,
    pub(crate) rebase: Option<rebase::RebasePlan>,
    pub(crate) stash: Option<stash::StashPlan>,
    pub(crate) hunk: Option<hunks::HunkPlan>,
}
impl PreparedAction {
    /// Restrict a filtered batch fetch to the HEAD observed in its preview.
    /// Working-tree changes alone do not invalidate this fetch guard.
    pub fn guard_head(&mut self, head: Head) {
        self.head_guard = Some(head);
    }
    /// Identical fetches across aliases/linked worktrees may share one execution.
    /// Other actions remain separate and are serialized by the common-directory lock.
    pub fn fetch_coalesce_key(&self) -> Option<String> {
        if !matches!(self.action, RepositoryAction::Fetch { .. }) {
            return None;
        }
        let mut bytes = os_bytes(self.repository.common_dir.as_os_str());
        bytes.push(0);
        bytes.extend_from_slice(self.config_fingerprint.as_bytes());
        if let Some(head) = &self.head_guard {
            // A guard belongs to one worktree. Coalescing across worktrees would skip
            // revalidating another candidate even if their original HEADs were equal.
            bytes.push(0);
            bytes.extend_from_slice(self.repository.id.as_bytes());
            bytes.push(0);
            bytes.extend_from_slice(
                serde_json::to_string(head)
                    .expect("serializing Head")
                    .as_bytes(),
            );
        }
        for arg in &self.args {
            bytes.push(0);
            bytes.extend_from_slice(&os_bytes(arg));
        }
        Some(digest(&bytes))
    }
}
pub struct ActionOutcome {
    pub exit_code: i32,
    pub output_truncated: bool,
}
pub(crate) fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
pub(crate) fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
pub(crate) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(crate) fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .trim_end_matches(['\r', '\n'])
        .to_owned()
}
#[cfg(unix)]
pub(crate) fn path_bytes(bytes: &[u8]) -> OsString {
    use std::os::unix::ffi::OsStringExt;
    OsString::from_vec(bytes.to_vec())
}
#[cfg(not(unix))]
pub(crate) fn path_bytes(bytes: &[u8]) -> OsString {
    OsString::from(String::from_utf8_lossy(bytes).into_owned())
}
#[cfg(unix)]
pub(crate) fn os_bytes(path: &std::ffi::OsStr) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    path.as_bytes().to_vec()
}
#[cfg(not(unix))]
pub(crate) fn os_bytes(path: &std::ffi::OsStr) -> Vec<u8> {
    path.to_string_lossy().as_bytes().to_vec()
}
impl GitEngine {
    pub fn new() -> Self {
        Self::default()
    }
    /// For disposable fixture repositories only: ignore system/global Git config and inherited identities.
    /// The desktop application uses `new()` and preserves the user's normal Git configuration.
    pub fn isolated() -> Self {
        Self {
            runner: runner::Runner {
                isolated: true,
                ..runner::Runner::default()
            },
            ..Self::default()
        }
    }

    pub(crate) async fn repository(&self, id: &str) -> AppResult<Repository> {
        self.state
            .lock()
            .await
            .repositories
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::new("repository_unavailable", "Open the repository again."))
    }
    pub(crate) async fn register_repository(&self, path: &Path) -> AppResult<Repository> {
        let root = PathBuf::from(path_bytes(
            self.runner
                .read(
                    path,
                    &["rev-parse", "--path-format=absolute", "--show-toplevel"],
                )
                .await?
                .strip_suffix(b"\n")
                .unwrap_or_default(),
        ));
        let root = std::fs::canonicalize(root)
            .map_err(|e| AppError::new("repository_unavailable", e.to_string()))?;
        let git_bytes = self
            .runner
            .read(&root, &["rev-parse", "--absolute-git-dir"])
            .await?;
        let git_dir = PathBuf::from(path_bytes(
            git_bytes.strip_suffix(b"\n").unwrap_or(&git_bytes),
        ));
        let common_bytes = self
            .runner
            .read(
                &root,
                &["rev-parse", "--path-format=absolute", "--git-common-dir"],
            )
            .await?;
        let common_dir = std::fs::canonicalize(PathBuf::from(path_bytes(
            common_bytes.strip_suffix(b"\n").unwrap_or(&common_bytes),
        )))
        .map_err(|e| AppError::new("repository_unavailable", e.to_string()))?;
        let mut state = self.state.lock().await;
        let repository_id =
            if let Some(repo) = state.repositories.values().find(|repo| repo.root == root) {
                repo.id.clone()
            } else {
                let repository_id = id();
                let lock = state.locks.entry(common_dir.clone()).or_default().clone();
                state.repositories.insert(
                    repository_id.clone(),
                    Repository {
                        id: repository_id.clone(),
                        root: root.clone(),
                        git_dir,
                        common_dir,
                        lock,
                        squash_running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                    },
                );
                repository_id
            };
        Ok(state.repositories.get(&repository_id).unwrap().clone())
    }
    pub async fn open_repository(&self, path: &Path) -> AppResult<RepositoryOpen> {
        let repository = self.register_repository(path).await?;
        let status = self.get_status(&repository.id).await?;
        Ok(RepositoryOpen {
            repository_id: repository.id,
            display_path: repository.root.to_string_lossy().into_owned(),
            status,
        })
    }
    pub(crate) async fn save_snapshot(&self, snapshot: Snapshot) -> String {
        let mut state = self.state.lock().await;
        if snapshot.status.is_some() {
            if let Some((key, _)) = state.snapshots.iter().find(|(_, s)| {
                s.repository_id == snapshot.repository_id
                    && s.status.is_some()
                    && s.fingerprint == snapshot.fingerprint
            }) {
                return key.clone();
            }
        }
        if state.snapshots.len() >= 512 {
            let key = state.snapshots.keys().next().cloned().unwrap();
            state.snapshots.remove(&key);
        }
        let key = id();
        state.snapshots.insert(key.clone(), snapshot);
        key
    }
    pub(crate) async fn snapshot(
        &self,
        repository_id: &str,
        snapshot_id: &str,
    ) -> AppResult<Snapshot> {
        self.state
            .lock()
            .await
            .snapshots
            .get(snapshot_id)
            .filter(|s| s.repository_id == repository_id)
            .cloned()
            .ok_or_else(|| AppError::new("stale_snapshot", "Refresh the repository selection."))
    }
}

#[cfg(test)]
mod rewrite_tests;
