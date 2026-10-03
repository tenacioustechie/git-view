use super::*;
use std::collections::HashSet;

const PAGE: usize = 200;
#[derive(Clone)]
pub(crate) struct FileHistorySnapshot {
    repository_id: String,
    source: FileHistorySource,
    path: OsString,
    start_oid: Option<String>,
    shallow: Vec<String>,
    shallow_fingerprint: String,
    cursors: HashMap<String, usize>,
    members: HashSet<String>,
}

pub(super) fn ancestry(repo: &Repository) -> AppResult<Vec<u8>> {
    if repo.common_dir.join("info/grafts").exists() {
        return Err(AppError::new(
            "unsupported_history",
            "Legacy grafts change ancestry; remove or migrate them before inspecting file history.",
        ));
    }
    Ok(merge::read_metadata(
        &repo.common_dir.join("shallow"),
        runner::STRUCTURED_LIMIT as u64,
    )?
    .unwrap_or_default())
}

impl GitEngine {
    pub async fn get_file_history(
        &self,
        request: FileHistoryRequest,
    ) -> AppResult<FileHistoryPage> {
        tokio::time::timeout(std::time::Duration::from_secs(30), self.file_history_page(request))
            .await.map_err(|_| AppError::new("history_timeout", "File history exceeded 30 seconds. No partial page was returned; try an earlier starting commit."))?
    }

    async fn file_history_page(&self, request: FileHistoryRequest) -> AppResult<FileHistoryPage> {
        let repo = self.repository(&request.repository_id).await?;
        let shallow_bytes = ancestry(&repo)?;
        let fingerprint = digest(&shallow_bytes);
        let (key, mut snapshot, offset) = if let Some(key) = request.history_snapshot_id {
            let snapshot = self.state.lock().await.file_history.get(&key)
                .filter(|s| s.repository_id == request.repository_id && s.source == request.source)
                .cloned().ok_or_else(|| AppError::new("stale_snapshot", "File history expired or belongs to another selection. Close it and reopen from the file."))?;
            if snapshot.shallow_fingerprint != fingerprint {
                return Err(AppError::new(
                    "stale_snapshot",
                    "Shallow history changed. Reopen file history.",
                ));
            }
            let offset = match request.cursor {
                Some(cursor) => *snapshot.cursors.get(&cursor).ok_or_else(|| {
                    AppError::new(
                        "invalid_cursor",
                        "Cursor is outside this file-history snapshot.",
                    )
                })?,
                None => 0,
            };
            (key, snapshot, offset)
        } else {
            if request.cursor.is_some() {
                return Err(AppError::new(
                    "invalid_cursor",
                    "A cursor requires a file-history snapshot.",
                ));
            }
            let source = self
                .snapshot(&request.repository_id, &request.source.snapshot_id)
                .await?;
            let file = source
                .files
                .iter()
                .find(|file| file.wire.file_id == request.source.file_id)
                .ok_or_else(|| {
                    AppError::new(
                        "stale_snapshot",
                        "File is outside the selected snapshot. Refresh and select it again.",
                    )
                })?;
            let path = match request.source.path {
                FileHistoryPath::Current => file.path.clone(),
                FileHistoryPath::Previous => file.old_path.clone().ok_or_else(|| {
                    AppError::new("invalid_path", "This file has no recorded previous path.")
                })?,
            };
            let start_oid = if let Some((oid, _)) = source.commit {
                Some(oid)
            } else if let Some(status) = source.status {
                match status.head {
                    Head::Branch { oid, .. } | Head::Detached { oid } => Some(oid),
                    Head::Unborn { .. } => None,
                }
            } else {
                return Err(AppError::new(
                    "invalid_context",
                    "Select a working or historical file first.",
                ));
            };
            (
                id(),
                FileHistorySnapshot {
                    repository_id: request.repository_id.clone(),
                    source: request.source,
                    path,
                    start_oid,
                    shallow: text(&shallow_bytes).lines().map(str::to_owned).collect(),
                    shallow_fingerprint: fingerprint,
                    cursors: HashMap::new(),
                    members: HashSet::new(),
                },
                0,
            )
        };
        let mut oids = Vec::new();
        if let Some(tip) = &snapshot.start_oid {
            // Raw path bytes originate only in a backend snapshot, never display text.
            // Explicit full-history traversal: no automatic rename following or fake DAG.
            let mut args = runner::args(&[
                "--no-replace-objects",
                "--literal-pathspecs",
                "rev-list",
                "--full-history",
                "--topo-order",
                &format!("--max-count={}", PAGE + 1),
                &format!("--skip={offset}"),
                tip,
                "--",
            ]);
            args.push(snapshot.path.clone());
            let output = runner::checked(
                self.runner
                    .run(&repo.root, &args, None, false, runner::STRUCTURED_LIMIT)
                    .await?,
            )?;
            oids = text(&output.stdout).lines().map(str::to_owned).collect();
        }
        let more = oids.len() > PAGE;
        oids.truncate(PAGE);
        let commits = history::commits(&self.runner, &repo, &oids)
            .await?
            .into_iter()
            .map(|commit| CommitSummary {
                boundary: snapshot.shallow.contains(&commit.oid),
                oid: commit.oid,
                parents: commit.parents,
                subject: commit.message.lines().next().unwrap_or_default().into(),
                author_name: commit.author.name,
                authored_at: commit.author.at,
                refs: Vec::new(),
            })
            .collect();
        snapshot.members.extend(oids);
        let next_cursor = more.then(|| {
            let cursor = id();
            snapshot.cursors.insert(cursor.clone(), offset + PAGE);
            cursor
        });
        if digest(&ancestry(&repo)?) != snapshot.shallow_fingerprint {
            return Err(AppError::new(
                "stale_snapshot",
                "History ancestry changed during the read. Reopen file history.",
            ));
        }
        let result = FileHistoryPage {
            display_path: snapshot.path.to_string_lossy().into_owned(),
            start_oid: snapshot.start_oid.clone(),
            shallow: !snapshot.shallow.is_empty(),
            history: HistoryPage {
                snapshot_id: key.clone(),
                commits,
                next_cursor,
            },
        };
        let mut state = self.state.lock().await;
        if let Some(existing) = state.file_history.get(&key) {
            snapshot.cursors.extend(existing.cursors.clone());
            snapshot.members.extend(existing.members.clone());
        }
        if state.file_history.len() >= 32 && !state.file_history.contains_key(&key) {
            let old = state.file_history.keys().next().cloned().unwrap();
            state.file_history.remove(&old);
        }
        state.file_history.insert(key, snapshot);
        Ok(result)
    }

    pub async fn get_file_history_commit(
        &self,
        repository_id: &str,
        history_snapshot_id: &str,
        oid: &str,
        parent_oid: Option<String>,
    ) -> AppResult<CommitDetails> {
        let snapshot = self
            .state
            .lock()
            .await
            .file_history
            .get(history_snapshot_id)
            .filter(|s| s.repository_id == repository_id)
            .cloned()
            .ok_or_else(|| {
                AppError::new(
                    "stale_snapshot",
                    "File history expired. Reopen it from the file.",
                )
            })?;
        if !snapshot.members.contains(oid) {
            return Err(AppError::new(
                "invalid_commit",
                "Select a commit returned by this file history.",
            ));
        }
        let repo = self.repository(repository_id).await?;
        if digest(&ancestry(&repo)?) != snapshot.shallow_fingerprint {
            return Err(AppError::new(
                "stale_snapshot",
                "History ancestry changed. Reopen file history.",
            ));
        }
        let mut details = self.get_commit(repository_id, oid, parent_oid).await?;
        let files = self.snapshot(repository_id, &details.snapshot_id).await?;
        // Compare raw paths, including the old side of a rename/deletion. Display
        // strings can be lossy and must not become identity or command arguments.
        details.files.retain(|file| {
            files.files.iter().any(|entry| {
                entry.wire.file_id == file.file_id
                    && (entry.path == snapshot.path
                        || entry.old_path.as_ref() == Some(&snapshot.path))
            })
        });
        Ok(details)
    }
}
