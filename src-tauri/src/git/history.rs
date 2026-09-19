use super::*;
use std::io::Read;
const PAGE: usize = 200;
const DIFF_LIMIT: usize = 2 * 1024 * 1024;
#[derive(Clone)]
struct Commit {
    oid: String,
    parents: Vec<String>,
    message: String,
    author: Identity,
    committer: Identity,
}
fn identity(value: &str) -> Identity {
    let (name, tail) = value.rsplit_once(" <").unwrap_or((value, ""));
    let (email, time) = tail.split_once("> ").unwrap_or((tail, ""));
    let timestamp = time
        .split_whitespace()
        .next()
        .and_then(|n| n.parse::<i64>().ok())
        .and_then(|n| chrono::DateTime::from_timestamp(n, 0))
        .map(|date| date.to_rfc3339())
        .unwrap_or_default();
    Identity {
        name: name.to_string(),
        email: email.to_string(),
        at: timestamp,
    }
}
fn parse_commit(oid: String, body: &[u8]) -> AppResult<Commit> {
    let body = String::from_utf8_lossy(body);
    let (headers, message) = body
        .split_once("\n\n")
        .ok_or_else(|| AppError::new("invalid_git_output", "Commit has no message separator"))?;
    let mut parents = Vec::new();
    let mut author = None;
    let mut committer = None;
    for line in headers.lines() {
        if let Some(value) = line.strip_prefix("parent ") {
            parents.push(value.to_string());
        }
        if let Some(value) = line.strip_prefix("author ") {
            author = Some(identity(value));
        }
        if let Some(value) = line.strip_prefix("committer ") {
            committer = Some(identity(value));
        }
    }
    Ok(Commit {
        oid,
        parents,
        message: message.to_string(),
        author: author.ok_or_else(|| AppError::new("invalid_git_output", "Commit lacks author"))?,
        committer: committer
            .ok_or_else(|| AppError::new("invalid_git_output", "Commit lacks committer"))?,
    })
}
async fn commits(repo: &Repository, oids: &[String]) -> AppResult<Vec<Commit>> {
    if oids.is_empty() {
        return Ok(Vec::new());
    }
    let input = format!("{}\n", oids.join("\n")).into_bytes();
    let bytes = runner::checked(
        runner::run(
            &repo.root,
            &runner::args(&["cat-file", "--batch"]),
            Some(input),
            false,
            runner::STRUCTURED_LIMIT,
        )
        .await?,
    )?
    .stdout;
    let mut remaining = bytes.as_slice();
    let mut output = Vec::new();
    for _ in oids {
        let end = remaining
            .iter()
            .position(|b| *b == b'\n')
            .ok_or_else(|| AppError::new("invalid_git_output", "Truncated object header"))?;
        let header = String::from_utf8_lossy(&remaining[..end]);
        let fields = header.split_whitespace().collect::<Vec<_>>();
        if fields.len() != 3 || fields[1] != "commit" {
            return Err(AppError::new(
                "missing_object",
                "A commit is unavailable in this history snapshot.",
            ));
        }
        let size = fields[2]
            .parse::<usize>()
            .map_err(|_| AppError::new("invalid_git_output", "Invalid object size"))?;
        remaining = &remaining[end + 1..];
        if size > runner::STRUCTURED_LIMIT || remaining.len() <= size || remaining[size] != b'\n' {
            return Err(AppError::new(
                "invalid_git_output",
                "Truncated or oversized commit object",
            ));
        }
        output.push(parse_commit(fields[0].to_string(), &remaining[..size])?);
        remaining = &remaining[size + 1..];
    }
    if !remaining.is_empty() {
        return Err(AppError::new(
            "invalid_git_output",
            "Unexpected trailing object data",
        ));
    }
    Ok(output)
}
impl GitEngine {
    pub(crate) async fn resolve_commit(
        &self,
        repo: &Repository,
        reference: &str,
    ) -> AppResult<String> {
        let expression = format!("{reference}^{{commit}}");
        Ok(text(
            &runner::read(
                &repo.root,
                &["rev-parse", "--verify", "--end-of-options", &expression],
            )
            .await?,
        ))
    }
    pub async fn get_history(
        &self,
        repository_id: &str,
        scope: Option<String>,
        snapshot_id: Option<String>,
        cursor: Option<String>,
    ) -> AppResult<HistoryPage> {
        let repo = self.repository(repository_id).await?;
        let (key, mut snapshot, offset) = if let Some(key) = snapshot_id {
            let state = self.state.lock().await;
            let snapshot = state
                .history
                .get(&key)
                .filter(|s| s.repository_id == repository_id)
                .cloned()
                .ok_or_else(|| {
                    AppError::new("stale_snapshot", "History expired; refresh history.")
                })?;
            let offset = match cursor {
                Some(cursor) => *snapshot.cursors.get(&cursor).ok_or_else(|| {
                    AppError::new(
                        "invalid_cursor",
                        "History cursor is not part of this snapshot.",
                    )
                })?,
                None => 0,
            };
            (key, snapshot, offset)
        } else {
            if cursor.is_some() {
                return Err(AppError::new(
                    "invalid_cursor",
                    "History cursor requires a snapshot.",
                ));
            }
            let refs = self.list_refs(repository_id).await?;
            let status = self.get_status(repository_id).await?;
            let mut tips = Vec::new();
            match scope.as_deref().unwrap_or("all") {
                "all" => {
                    for reference in &refs {
                        match self.resolve_commit(&repo, &reference.oid).await {
                            Ok(tip) => tips.push(tip),
                            Err(error) => {
                                if reference.kind != "tag" {
                                    return Err(error);
                                }
                                let kind =
                                    runner::read(&repo.root, &["cat-file", "-t", &reference.oid])
                                        .await?;
                                if !["blob", "tree"].contains(&text(&kind).as_str()) {
                                    return Err(error);
                                }
                            }
                        }
                    }
                    if let Head::Branch { oid, .. } | Head::Detached { oid } = &status.head {
                        tips.push(oid.clone());
                    }
                }
                "head" | "HEAD" => {
                    if let Head::Branch { oid, .. } | Head::Detached { oid } = &status.head {
                        tips.push(oid.clone());
                    }
                }
                reference => {
                    let selected =
                        refs.iter()
                            .find(|r| r.full_name == reference)
                            .ok_or_else(|| {
                                AppError::new("invalid_ref", "Select an available full reference.")
                            })?;
                    tips.push(self.resolve_commit(&repo, &selected.oid).await?);
                }
            }
            tips.sort();
            tips.dedup();
            (
                id(),
                HistorySnapshot {
                    repository_id: repository_id.into(),
                    tips,
                    refs,
                    cursors: HashMap::new(),
                },
                0,
            )
        };
        let args = runner::args(&[
            "rev-list",
            "--stdin",
            "--topo-order",
            "--boundary",
            &format!("--max-count={}", PAGE + 1),
            &format!("--skip={offset}"),
        ]);

        let lines = if snapshot.tips.is_empty() {
            Vec::new()
        } else {
            let bytes = runner::checked(
                runner::run(
                    &repo.root,
                    &args,
                    Some(format!("{}\n", snapshot.tips.join("\n")).into_bytes()),
                    false,
                    runner::STRUCTURED_LIMIT,
                )
                .await?,
            )?
            .stdout;
            text(&bytes).lines().map(str::to_owned).collect::<Vec<_>>()
        };
        let has_more = lines.len() > PAGE;
        let oids = lines
            .iter()
            .take(PAGE)
            .map(|line| line.trim_start_matches('-').to_owned())
            .collect::<Vec<_>>();
        let objects = commits(&repo, &oids).await?;
        let shallow = std::fs::read_to_string(repo.common_dir.join("shallow")).unwrap_or_default();
        let commits = objects
            .into_iter()
            .map(|commit| CommitSummary {
                refs: snapshot
                    .refs
                    .iter()
                    .filter(|r| r.oid == commit.oid)
                    .map(|r| RefDecoration {
                        full_name: r.full_name.clone(),
                        label: r.label.clone(),
                        kind: r.kind.clone(),
                    })
                    .collect(),
                boundary: shallow.lines().any(|oid| oid == commit.oid)
                    || lines.iter().any(|line| line == &format!("-{}", commit.oid)),
                oid: commit.oid,
                parents: commit.parents,
                subject: commit.message.lines().next().unwrap_or_default().to_owned(),
                author_name: commit.author.name,
                authored_at: commit.author.at,
            })
            .collect();
        let next_cursor = if has_more {
            let cursor = id();
            snapshot.cursors.insert(cursor.clone(), offset + PAGE);
            Some(cursor)
        } else {
            None
        };
        let mut state = self.state.lock().await;
        if state.history.len() >= 32 && !state.history.contains_key(&key) {
            let old = state.history.keys().next().cloned().unwrap();
            state.history.remove(&old);
        }
        state.history.insert(key.clone(), snapshot);
        Ok(HistoryPage {
            snapshot_id: key,
            commits,
            next_cursor,
        })
    }
    pub async fn get_commit(
        &self,
        repository_id: &str,
        oid: &str,
        parent_oid: Option<String>,
    ) -> AppResult<CommitDetails> {
        let repo = self.repository(repository_id).await?;
        let oid = self.resolve_commit(&repo, oid).await?;
        let commit = commits(&repo, std::slice::from_ref(&oid)).await?.remove(0);
        let parent = match parent_oid {
            Some(parent) => {
                if !commit.parents.contains(&parent) {
                    return Err(AppError::new(
                        "invalid_parent",
                        "The selected object is not a parent of this commit.",
                    ));
                }
                Some(parent)
            }
            None => commit.parents.first().cloned(),
        };
        let args = if let Some(parent) = &parent {
            runner::args(&[
                "diff",
                "--name-status",
                "-z",
                "--find-renames",
                parent,
                &oid,
                "--",
            ])
        } else {
            runner::args(&[
                "diff-tree",
                "--root",
                "--no-commit-id",
                "-r",
                "--name-status",
                "-z",
                "--find-renames",
                &oid,
                "--",
            ])
        };
        let bytes = runner::checked(
            runner::run(&repo.root, &args, None, false, runner::STRUCTURED_LIMIT).await?,
        )?
        .stdout;
        let mut records = bytes.split(|b| *b == 0);
        let mut files = Vec::new();
        while let Some(status) = records.next() {
            if status.is_empty() {
                continue;
            }
            let first = records
                .next()
                .ok_or_else(|| AppError::new("invalid_git_output", "Missing historical path"))?;
            let (path, old_path) = if status[0] == b'R' || status[0] == b'C' {
                (
                    records.next().ok_or_else(|| {
                        AppError::new("invalid_git_output", "Missing renamed path")
                    })?,
                    Some(first),
                )
            } else {
                (first, None)
            };
            let kind = match status[0] {
                b'A' => "added",
                b'D' => "deleted",
                b'R' => "renamed",
                b'C' => "copied",
                b'T' => "typeChanged",
                _ => "modified",
            };
            files.push(FileEntry {
                wire: ChangedFile {
                    file_id: id(),
                    display_path: String::from_utf8_lossy(path).into_owned(),
                    old_display_path: old_path.map(|p| String::from_utf8_lossy(p).into_owned()),
                    change_kind: Some(kind.into()),
                    index_status: None,
                    worktree_status: None,
                    untracked: false,
                    conflicted: false,
                    submodule: false,
                },
                path: path_bytes(path),
                old_path: old_path.map(path_bytes),
            });
        }
        let snapshot_id = self
            .save_snapshot(Snapshot {
                repository_id: repository_id.into(),
                fingerprint: String::new(),
                status: None,
                files: files.clone(),
                commit: Some((oid.clone(), parent.clone())),
            })
            .await;
        Ok(CommitDetails {
            snapshot_id,
            oid,
            parents: commit.parents,
            selected_parent: parent,
            message: commit.message,
            author: commit.author,
            committer: commit.committer,
            files: files.into_iter().map(|f| f.wire).collect(),
        })
    }
    pub async fn get_diff(&self, request: DiffRequest) -> AppResult<DiffResult> {
        let repo = self.repository(&request.repository_id).await?;
        let snapshot = self
            .snapshot(&request.repository_id, &request.snapshot_id)
            .await?;
        let file = snapshot
            .files
            .iter()
            .find(|file| file.wire.file_id == request.file_id)
            .ok_or_else(|| {
                AppError::new("stale_snapshot", "File is outside the selected snapshot.")
            })?;
        let mut args = runner::args(&[
            "--literal-pathspecs",
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
        ]);
        match request.context.as_str() {
            "commit" => {
                let (oid, parent) = snapshot.commit.as_ref().ok_or_else(|| {
                    AppError::new("invalid_context", "Select a historical file first.")
                })?;
                if let Some(parent) = parent {
                    args.push(parent.into());
                    args.push(oid.into());
                } else {
                    args = runner::args(&[
                        "--literal-pathspecs",
                        "show",
                        "--format=",
                        "--root",
                        "--no-ext-diff",
                        "--no-textconv",
                        "--no-color",
                        oid,
                    ]);
                }
            }
            "staged" | "unstaged" => {
                if snapshot.status.is_none() {
                    return Err(AppError::new(
                        "invalid_context",
                        "Select a working file first.",
                    ));
                }
                if self.observe(&repo).await?.fingerprint != snapshot.fingerprint {
                    return Err(AppError::new(
                        "stale_snapshot",
                        "Working changes were updated; refresh first.",
                    ));
                }
                if file.wire.untracked && request.context == "unstaged" {
                    let path = repo.root.join(&file.path);
                    let meta = std::fs::symlink_metadata(&path)
                        .map_err(|e| AppError::new("file_unavailable", e.to_string()))?;
                    let bytes = if meta.file_type().is_symlink() {
                        os_bytes(
                            std::fs::read_link(&path)
                                .map_err(|e| AppError::new("file_unavailable", e.to_string()))?
                                .as_os_str(),
                        )
                    } else if meta.is_file() {
                        let mut bytes = Vec::new();
                        std::fs::File::open(path)
                            .and_then(|file| {
                                file.take((DIFF_LIMIT + 1) as u64).read_to_end(&mut bytes)
                            })
                            .map_err(|e| AppError::new("file_unavailable", e.to_string()))?;
                        bytes
                    } else {
                        return Ok(DiffResult::Unavailable {
                            reason: "This path is not a regular file.".into(),
                        });
                    };
                    if bytes.contains(&0) {
                        return Ok(DiffResult::Binary {
                            description: "Binary untracked file".into(),
                        });
                    }
                    return Ok(DiffResult::Text {
                        text: String::from_utf8_lossy(&bytes[..bytes.len().min(DIFF_LIMIT)])
                            .into_owned(),
                        truncated: bytes.len() > DIFF_LIMIT,
                    });
                }
                if request.context == "staged" {
                    args.push("--cached".into());
                }
            }
            _ => return Err(AppError::new("invalid_context", "Unknown diff context")),
        }
        args.push("--".into());
        args.push(file.path.clone());
        if let Some(old) = &file.old_path {
            args.push(old.clone());
        }
        let output = match runner::run(&repo.root, &args, None, false, DIFF_LIMIT).await {
            Err(error) if error.code == "output_limit" => {
                return Ok(DiffResult::Unavailable {
                    reason: "Diff exceeds the 2 MiB preview limit.".into(),
                })
            }
            result => runner::checked(result?)?,
        };
        if output.stdout.windows(13).any(|w| w == b"Binary files ")
            || output.stdout.windows(16).any(|w| w == b"GIT binary patch")
        {
            Ok(DiffResult::Binary {
                description: "Git reports a binary file change.".into(),
            })
        } else {
            Ok(DiffResult::Text {
                text: String::from_utf8_lossy(&output.stdout).into_owned(),
                truncated: false,
            })
        }
    }
}
