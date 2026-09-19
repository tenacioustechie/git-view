mod actions;
mod history;
mod runner;
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
    pub refs: Vec<GitRef>,
    pub cursors: HashMap<String, usize>,
}
#[derive(Default)]
pub(crate) struct State {
    pub repositories: HashMap<String, Repository>,
    pub locks: HashMap<PathBuf, Arc<Mutex<()>>>,
    pub snapshots: HashMap<String, Snapshot>,
    pub history: HashMap<String, HistorySnapshot>,
    pub fetch_times: HashMap<String, String>,
}
#[derive(Clone, Default)]
pub struct GitEngine {
    pub(crate) state: Arc<Mutex<State>>,
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
    pub(crate) async fn repository(&self, id: &str) -> AppResult<Repository> {
        self.state
            .lock()
            .await
            .repositories
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::new("repository_unavailable", "Open the repository again."))
    }
    pub async fn open_repository(&self, path: &Path) -> AppResult<RepositoryOpen> {
        let root = PathBuf::from(path_bytes(
            runner::read(
                path,
                &["rev-parse", "--path-format=absolute", "--show-toplevel"],
            )
            .await?
            .strip_suffix(b"\n")
            .unwrap_or_default(),
        ));
        let root = std::fs::canonicalize(root)
            .map_err(|e| AppError::new("repository_unavailable", e.to_string()))?;
        let git_bytes = runner::read(&root, &["rev-parse", "--absolute-git-dir"]).await?;
        let git_dir = PathBuf::from(path_bytes(
            git_bytes.strip_suffix(b"\n").unwrap_or(&git_bytes),
        ));
        let common_bytes = runner::read(
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
                    },
                );
                repository_id
            };
        drop(state);
        let status = self.get_status(&repository_id).await?;
        Ok(RepositoryOpen {
            repository_id,
            display_path: root.to_string_lossy().into_owned(),
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
