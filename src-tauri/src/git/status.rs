use super::*;

type ParsedStatus = (
    Head,
    Option<String>,
    Option<u64>,
    Option<u64>,
    Vec<FileEntry>,
    bool,
);
pub(crate) fn parse_status(bytes: &[u8]) -> AppResult<ParsedStatus> {
    let mut oid = None;
    let mut branch = None;
    let mut upstream = None;
    let mut ahead = None;
    let mut behind = None;
    let mut files = Vec::new();
    let mut submodule_dirty = false;
    let mut records = bytes.split(|b| *b == 0);
    while let Some(record) = records.next() {
        if record.is_empty() {
            continue;
        }
        if record.starts_with(b"# ") {
            let line = String::from_utf8_lossy(record);
            if let Some(value) = line.strip_prefix("# branch.oid ") {
                oid = Some(value.to_string());
            }
            if let Some(value) = line.strip_prefix("# branch.head ") {
                branch = Some(value.to_string());
            }
            if let Some(value) = line.strip_prefix("# branch.upstream ") {
                upstream = Some(value.to_string());
            }
            if let Some(value) = line.strip_prefix("# branch.ab ") {
                let mut words = value.split_whitespace();
                ahead = words
                    .next()
                    .and_then(|s| s.strip_prefix('+'))
                    .and_then(|s| s.parse().ok());
                behind = words
                    .next()
                    .and_then(|s| s.strip_prefix('-'))
                    .and_then(|s| s.parse().ok());
            }
            continue;
        }
        let (path, old_path, index, worktree, untracked, conflicted, submodule) = match record[0] {
            b'?' => (&record[2..], None, None, None, true, false, false),
            b'1' | b'2' | b'u' => {
                let count = match record[0] {
                    b'1' => 9,
                    b'2' => 10,
                    _ => 11,
                };
                let fields = record.splitn(count, |b| *b == b' ').collect::<Vec<_>>();
                if fields.len() != count || fields[1].len() != 2 {
                    return Err(AppError::new(
                        "invalid_git_output",
                        "Malformed status record",
                    ));
                }
                let sub = fields[2];
                let module = sub.first() == Some(&b'S');
                if module && sub.get(1..).is_some_and(|s| s.iter().any(|b| *b != b'.')) {
                    submodule_dirty = true;
                }
                let old = if record[0] == b'2' {
                    Some(records.next().ok_or_else(|| {
                        AppError::new("invalid_git_output", "Missing rename path")
                    })?)
                } else {
                    None
                };
                (
                    fields[count - 1],
                    old,
                    Some((fields[1][0] as char).to_string()),
                    Some((fields[1][1] as char).to_string()),
                    false,
                    record[0] == b'u',
                    module,
                )
            }
            b'!' => continue,
            _ => {
                return Err(AppError::new(
                    "invalid_git_output",
                    "Unknown Git status record",
                ))
            }
        };
        let kind = if untracked {
            "untracked"
        } else if conflicted {
            "unmerged"
        } else {
            match index
                .as_deref()
                .filter(|s| *s != ".")
                .or(worktree.as_deref())
                .unwrap_or(".")
            {
                "A" => "added",
                "D" => "deleted",
                "R" => "renamed",
                "C" => "copied",
                "T" => "typeChanged",
                _ => "modified",
            }
        };
        files.push(FileEntry {
            wire: ChangedFile {
                file_id: id(),
                display_path: String::from_utf8_lossy(path).into_owned(),
                old_display_path: old_path.map(|p| String::from_utf8_lossy(p).into_owned()),
                change_kind: Some(kind.into()),
                index_status: index,
                worktree_status: worktree,
                untracked,
                conflicted,
                submodule,
            },
            path: path_bytes(path),
            old_path: old_path.map(path_bytes),
        });
    }
    let oid =
        oid.ok_or_else(|| AppError::new("invalid_git_output", "Status did not contain HEAD"))?;
    let branch = branch.unwrap_or_default();
    let head = if oid == "(initial)" {
        Head::Unborn { name: branch }
    } else if branch == "(detached)" {
        Head::Detached { oid }
    } else {
        Head::Branch { name: branch, oid }
    };
    Ok((head, upstream, ahead, behind, files, submodule_dirty))
}
impl GitEngine {
    pub(crate) async fn observe(&self, repo: &Repository) -> AppResult<Snapshot> {
        let bytes = self
            .runner
            .read(
                &repo.root,
                &[
                    "status",
                    "--porcelain=v2",
                    "--branch",
                    "-z",
                    "--untracked-files=all",
                    "--ignore-submodules=none",
                ],
            )
            .await?;
        let (head, upstream_ref, ahead, behind, files, submodule_dirty) = parse_status(&bytes)?;
        let mut evidence = bytes.clone();
        for file in &files {
            for path in std::iter::once(&file.path).chain(file.old_path.iter()) {
                if let Ok(meta) = std::fs::symlink_metadata(repo.root.join(path)) {
                    evidence.extend_from_slice(
                        format!("{}{:?}", meta.len(), meta.modified()).as_bytes(),
                    );
                }
            }
        }
        let mut git_state = if repo.git_dir.join("rebase-merge").exists()
            || repo.git_dir.join("rebase-apply").exists()
        {
            GitState::Rebase
        } else if repo.git_dir.join("MERGE_HEAD").exists() {
            GitState::Merge
        } else if repo.git_dir.join("CHERRY_PICK_HEAD").exists() {
            GitState::CherryPick
        } else if repo.git_dir.join("REVERT_HEAD").exists() {
            GitState::Revert
        } else if repo.git_dir.join("sequencer").exists() {
            GitState::Other
        } else {
            GitState::Idle
        };
        let journal = merge::read_squash(repo)?;
        if let Some(journal) = &journal {
            if journal.phase == merge::SquashPhase::Running
                && !repo
                    .squash_running
                    .load(std::sync::atomic::Ordering::Acquire)
            {
                return Err(AppError::new("squash_manual_review","The recorded squash attempt was interrupted before its outcome was recorded. Journal and working changes were preserved; review git-view-squash.json in the worktree Git directory manually before removing its marker."));
            }
            merge::validate_context(journal, &head)?;
            if git_state != GitState::Idle {
                return Err(AppError::new("squash_manual_review","A squash review journal coexists with another Git operation. Journal and working changes were preserved; manual review is required."));
            }
            git_state = GitState::SquashMerge;
            evidence.extend_from_slice(
                serde_json::to_string(journal)
                    .expect("serializing squash journal")
                    .as_bytes(),
            );
        }
        let squash_merge = journal.map(|journal| journal.state);
        // These files define ordinary merge continuation/abort semantics. Existence
        // alone is insufficient: external edits must invalidate a prepared action.
        for name in [
            "MERGE_HEAD",
            "MERGE_MSG",
            "MERGE_MODE",
            "ORIG_HEAD",
            "MERGE_AUTOSTASH",
            "AUTO_MERGE",
            "CHERRY_PICK_HEAD",
            "REVERT_HEAD",
            "sequencer/head",
            "sequencer/todo",
            "sequencer/abort-safety",
            "sequencer/opts",
            "git-view-cherry-pick.json",
            "git-view-rebase.json",
            "REBASE_HEAD",
            "rebase-merge/head-name",
            "rebase-merge/orig-head",
            "rebase-merge/onto",
            "rebase-merge/stopped-sha",
            "rebase-merge/msgnum",
            "rebase-merge/end",
            "rebase-merge/done",
            "rebase-merge/git-rebase-todo",
            "rebase-apply/head-name",
            "rebase-apply/orig-head",
            "rebase-apply/onto",
        ] {
            evidence.extend_from_slice(name.as_bytes());
            evidence.push(0);
            if let Some(bytes) = merge::read_metadata(&repo.git_dir.join(name), 512 * 1024)? {
                evidence.push(1);
                evidence.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
                evidence.extend_from_slice(&bytes);
            } else {
                evidence.push(0);
            }
        }
        evidence.extend_from_slice(format!("{git_state:?}").as_bytes());
        let remote = match &head {
            Head::Branch { name, .. } => {
                let key = format!("branch.{name}.remote");
                let out = self
                    .runner
                    .run(
                        &repo.root,
                        &runner::args(&["config", "--get", &key]),
                        None,
                        false,
                        runner::STRUCTURED_LIMIT,
                    )
                    .await?;
                if out.code == 0 {
                    text(&out.stdout)
                } else {
                    String::new()
                }
            }
            _ => String::new(),
        };
        evidence.extend_from_slice(remote.as_bytes());
        let upstream = upstream_ref.map(|reference| Upstream {
            reference,
            remote,
            ahead,
            behind,
        });
        let mut counts = Counts::default();
        for file in &files {
            counts.staged +=
                usize::from(file.wire.index_status.as_deref().is_some_and(|s| s != "."));
            counts.unstaged += usize::from(
                file.wire
                    .worktree_status
                    .as_deref()
                    .is_some_and(|s| s != "."),
            );
            counts.untracked += usize::from(file.wire.untracked);
            counts.conflicted += usize::from(file.wire.conflicted);
        }
        let mut status = RepositoryStatus {
            repository_id: repo.id.clone(),
            snapshot_id: String::new(),
            observed_at: now(),
            head,
            git_state,
            squash_merge,
            dirty: !files.is_empty() || submodule_dirty,
            counts,
            submodule_dirty,
            upstream,
            last_successful_fetch_at: self.state.lock().await.fetch_times.get(&repo.id).cloned(),
            active_operation_id: None,
            operation_context: None,
        };
        self.operation_context(repo, &mut status).await?;
        evidence.extend_from_slice(
            serde_json::to_string(&status.operation_context)
                .unwrap()
                .as_bytes(),
        );
        Ok(Snapshot {
            repository_id: repo.id.clone(),
            fingerprint: digest(&evidence),
            status: Some(status),
            files,
            commit: None,
        })
    }
    pub async fn get_status(&self, repository_id: &str) -> AppResult<RepositoryStatus> {
        let repo = self.repository(repository_id).await?;
        // Acquire before observing. A clean snapshot captured before a concurrent
        // squash must never authorize deleting that newer operation's journal.
        let reconciliation_guard = repo.lock.try_lock().ok();
        let mut snapshot = self.observe(&repo).await?;
        if reconciliation_guard.is_some()
            && snapshot
                .status
                .as_ref()
                .is_some_and(|status| status.git_state == GitState::SquashMerge && !status.dirty)
        {
            merge::clear_squash(&repo)?;
            snapshot = self.observe(&repo).await?;
        }
        let mut status = snapshot.status.clone().unwrap();
        status.snapshot_id = self.save_snapshot(snapshot).await;
        Ok(status)
    }
    pub async fn get_changes(&self, repository_id: &str) -> AppResult<Changes> {
        let status = self.get_status(repository_id).await?;
        let snapshot = self.snapshot(repository_id, &status.snapshot_id).await?;
        Ok(Changes {
            snapshot_id: status.snapshot_id,
            files: snapshot.files.into_iter().map(|file| file.wire).collect(),
        })
    }
    pub async fn list_refs(&self, repository_id: &str) -> AppResult<Vec<GitRef>> {
        let repo = self.repository(repository_id).await?;
        let bytes = self
            .runner
            .read(
                &repo.root,
                &[
                    "for-each-ref",
                    "--format=%(refname)%00%(objectname)%00%(*objectname)",
                    "refs/heads/",
                    "refs/remotes/",
                    "refs/tags/",
                ],
            )
            .await?;
        let mut refs = Vec::new();
        for line in bytes.split(|b| *b == b'\n').filter(|l| !l.is_empty()) {
            let fields = line.split(|b| *b == 0).collect::<Vec<_>>();
            if fields.len() != 3 {
                return Err(AppError::new("invalid_git_output", "Malformed ref record"));
            }
            let full_name = text(fields[0]);
            let (kind, label) = if let Some(label) = full_name.strip_prefix("refs/heads/") {
                ("local", label)
            } else if let Some(label) = full_name.strip_prefix("refs/remotes/") {
                ("remote", label)
            } else {
                (
                    "tag",
                    full_name.strip_prefix("refs/tags/").unwrap_or(&full_name),
                )
            };
            refs.push(GitRef {
                label: label.to_string(),
                kind: kind.into(),
                oid: text(if fields[2].is_empty() {
                    fields[1]
                } else {
                    fields[2]
                }),
                full_name,
            });
        }
        Ok(refs)
    }
    pub(crate) async fn config(&self, repo: &Repository, key: &str) -> AppResult<Option<String>> {
        let out = self
            .runner
            .run(
                &repo.root,
                &runner::args(&["config", "--get", key]),
                None,
                false,
                runner::STRUCTURED_LIMIT,
            )
            .await?;
        if out.code == 1 {
            Ok(None)
        } else {
            Ok(Some(text(&runner::checked(out)?.stdout)))
        }
    }
    pub async fn list_remotes(&self, repository_id: &str) -> AppResult<Vec<Remote>> {
        let repo = self.repository(repository_id).await?;
        let status = self.get_status(repository_id).await?;
        let names = text(&self.runner.read(&repo.root, &["remote"]).await?)
            .lines()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let branch = match &status.head {
            Head::Branch { name, .. } | Head::Unborn { name } => Some(name),
            _ => None,
        };
        let upstream_remote = if let Some(branch) = branch {
            self.config(&repo, &format!("branch.{branch}.remote"))
                .await?
        } else {
            None
        };
        let fetch = upstream_remote
            .clone()
            .filter(|n| names.contains(n))
            .or_else(|| names.iter().find(|n| n.as_str() == "origin").cloned())
            .or_else(|| {
                if names.len() == 1 {
                    Some(names[0].clone())
                } else {
                    None
                }
            });
        let push = if let Some(branch) = branch {
            self.config(&repo, &format!("branch.{branch}.pushRemote"))
                .await?
        } else {
            None
        }
        .or(self.config(&repo, "remote.pushDefault").await?)
        .or(upstream_remote)
        .or(fetch.clone());
        let mode = self
            .config(&repo, "push.default")
            .await?
            .unwrap_or_else(|| "simple".into());
        let mut remotes = Vec::new();
        for name in names {
            let destination = if let Some(branch) = branch {
                if let Some(spec) = self.config(&repo, &format!("remote.{name}.push")).await? {
                    spec.split_once(':')
                        .filter(|(source, target)| {
                            !source.starts_with('+')
                                && !source.contains('*')
                                && !target.contains('*')
                                && ["HEAD", branch.as_str(), &format!("refs/heads/{branch}")]
                                    .contains(source)
                        })
                        .map(|(_, target)| target.to_owned())
                } else if mode == "upstream" {
                    self.config(&repo, &format!("branch.{branch}.merge"))
                        .await?
                } else if mode == "nothing" || mode == "matching" {
                    None
                } else {
                    Some(format!("refs/heads/{branch}"))
                }
            } else {
                None
            };
            remotes.push(Remote {
                id: name.clone(),
                default_fetch: fetch.as_ref() == Some(&name),
                default_push: push.as_ref() == Some(&name),
                destination_ref: destination,
                name,
            });
        }
        Ok(remotes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raw_non_utf8_and_rename_paths_are_not_execution_strings() {
        let bytes = b"# branch.oid (initial)\0# branch.head main\0? raw-\xff\0? tabs\tand\nlines\0";
        let (_, _, ahead, behind, files, _) = parse_status(bytes).unwrap();
        assert!(ahead.is_none() && behind.is_none());
        assert_eq!(files.len(), 2);
        #[cfg(unix)]
        assert_eq!(os_bytes(&files[0].path), b"raw-\xff");
        assert_eq!(files[1].wire.display_path, "tabs\tand\nlines");
    }
    #[tokio::test]
    async fn observation_cannot_cleanup_journal_using_a_stale_clean_read() {
        let dir = tempfile::tempdir().unwrap();
        let engine = GitEngine::isolated();
        for args in [
            vec!["init", "-b", "main"],
            vec!["config", "user.name", "Fixture"],
            vec!["config", "user.email", "fixture@example.invalid"],
            vec!["config", "commit.gpgsign", "false"],
            vec!["config", "core.hooksPath", ".disabled-hooks"],
            vec!["commit", "--allow-empty", "-m", "base"],
        ] {
            runner::checked(
                engine
                    .runner
                    .run(
                        dir.path(),
                        &runner::args(&args),
                        None,
                        true,
                        runner::DIAGNOSTIC_LIMIT,
                    )
                    .await
                    .unwrap(),
            )
            .unwrap();
        }
        let opened = engine.open_repository(dir.path()).await.unwrap();
        let repo = engine.repository(&opened.repository_id).await.unwrap();
        let oid = match opened.status.head {
            Head::Branch { oid, .. } => oid,
            _ => panic!("attached fixture"),
        };
        let journal = merge::SquashJournal {
            version: 1,
            operation_id: id(),
            phase: merge::SquashPhase::Review,
            original_branch: "refs/heads/main".into(),
            state: SquashMergeState {
                original_head: oid.clone(),
                source_oid: oid,
                source_ref: "refs/heads/source".into(),
            },
        };
        merge::write_squash(&repo, &journal).unwrap();
        // An observer may see clean bytes while a squash is only just starting.
        // Merely observing must never clear the journal, even if no lock is held.
        let stale = engine.observe(&repo).await.unwrap();
        assert!(!stale.status.unwrap().dirty);
        assert!(merge::read_squash(&repo).unwrap().is_some());
        std::fs::write(dir.path().join("new squash result"), "staged").unwrap();
        runner::checked(
            engine
                .runner
                .run(
                    dir.path(),
                    &runner::args(&["add", "--", "new squash result"]),
                    None,
                    true,
                    runner::DIAGNOSTIC_LIMIT,
                )
                .await
                .unwrap(),
        )
        .unwrap();
        // Reconciliation must acquire the lock and read the current state afresh.
        let fresh = engine.get_status(&opened.repository_id).await.unwrap();
        assert_eq!(fresh.git_state, GitState::SquashMerge);
        assert_eq!(fresh.counts.staged, 1);
        assert!(merge::read_squash(&repo).unwrap().is_some());
    }
}
