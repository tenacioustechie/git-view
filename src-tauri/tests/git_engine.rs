use contracts::*;
use git_view::contracts;
#[path = "../src/git/mod.rs"]
mod git;
use git::GitEngine;
use std::{path::Path, process::Command};
use tempfile::TempDir;
fn git(path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(path)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().into()
}
fn repo() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-b", "main"]);
    git(dir.path(), &["config", "user.name", "Fixture"]);
    git(
        dir.path(),
        &["config", "user.email", "fixture@example.invalid"],
    );
    git(dir.path(), &["config", "commit.gpgsign", "false"]);
    git(dir.path(), &["config", "core.hooksPath", ".disabled-hooks"]);
    dir
}
fn write(path: &Path, name: &str, body: &str) {
    std::fs::write(path.join(name), body).unwrap();
}
fn commit(path: &Path, message: &str) {
    git(path, &["add", "--all"]);
    git(path, &["commit", "-m", message]);
}
#[tokio::test]
async fn status_identity_and_literal_staging() {
    let dir = repo();
    let engine = GitEngine::new();
    let opened = engine.open_repository(dir.path()).await.unwrap();
    assert!(matches!(opened.status.head, Head::Unborn { .. }));
    for name in ["space name", "unicodé", "-dash", "[abc]", "a"] {
        write(dir.path(), name, "first\n");
    }
    let changes = engine.get_changes(&opened.repository_id).await.unwrap();
    assert_eq!(changes.files.len(), 5);
    let selected = changes
        .files
        .iter()
        .find(|f| f.display_path == "[abc]")
        .unwrap();
    let plan = engine
        .prepare_action(
            &opened.repository_id,
            RepositoryAction::Stage {
                snapshot_id: changes.snapshot_id,
                file_ids: vec![selected.file_id.clone()],
            },
        )
        .await
        .unwrap();
    assert!(!plan.summary.is_empty());
    let _destination = &plan.destination;
    let outcome = engine.execute(&plan).await.unwrap();
    assert_eq!(outcome.exit_code, 0);
    assert!(!outcome.output_truncated);
    assert_eq!(
        git(dir.path(), &["diff", "--cached", "--name-only"]),
        "[abc]"
    );
    let changes = engine.get_changes(&opened.repository_id).await.unwrap();
    let selected = changes
        .files
        .iter()
        .find(|f| f.display_path == "[abc]")
        .unwrap();
    let plan = engine
        .prepare_action(
            &opened.repository_id,
            RepositoryAction::Unstage {
                snapshot_id: changes.snapshot_id,
                file_ids: vec![selected.file_id.clone()],
            },
        )
        .await
        .unwrap();
    engine.execute(&plan).await.unwrap();
    assert!(git(dir.path(), &["ls-files"]).is_empty());
    assert!(dir.path().join("[abc]").exists());
    commit(dir.path(), "root");
    write(dir.path(), "a", "staged\n");
    git(dir.path(), &["add", "a"]);
    write(dir.path(), "a", "unstaged\n");
    let status = engine.get_status(&opened.repository_id).await.unwrap();
    assert_eq!(status.counts.staged, 1);
    assert_eq!(status.counts.unstaged, 1);
    let plan = engine
        .prepare_action(
            &opened.repository_id,
            RepositoryAction::Commit {
                status_snapshot_id: status.snapshot_id,
                message: "only staged".into(),
            },
        )
        .await
        .unwrap();
    engine.execute(&plan).await.unwrap();
    assert_eq!(git(dir.path(), &["show", "HEAD:a"]), "staged");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a")).unwrap(),
        "unstaged\n"
    );
    std::fs::create_dir(dir.path().join("nested")).unwrap();
    assert_eq!(
        engine
            .open_repository(&dir.path().join("nested"))
            .await
            .unwrap()
            .repository_id,
        opened.repository_id
    );
    git(dir.path(), &["checkout", "--detach"]);
    assert!(matches!(
        engine.get_status(&opened.repository_id).await.unwrap().head,
        Head::Detached { .. }
    ));
}
#[tokio::test]
async fn pull_policy_dirty_and_stale() {
    let source = repo();
    write(source.path(), "a", "one");
    commit(source.path(), "one");
    let bare = tempfile::tempdir().unwrap();
    git(bare.path(), &["init", "--bare"]);
    git(
        source.path(),
        &["remote", "add", "origin", bare.path().to_str().unwrap()],
    );
    git(source.path(), &["push", "-u", "origin", "main"]);
    let clone = tempfile::tempdir().unwrap();
    git(
        clone.path(),
        &["clone", "-b", "main", bare.path().to_str().unwrap(), "."],
    );
    git(clone.path(), &["config", "pull.rebase", "true"]);
    git(clone.path(), &["config", "rebase.autoStash", "true"]);
    let engine = GitEngine::new();
    let opened = engine.open_repository(clone.path()).await.unwrap();
    let plan = engine
        .prepare_action(
            &opened.repository_id,
            RepositoryAction::Pull {
                status_snapshot_id: opened.status.snapshot_id.clone(),
            },
        )
        .await
        .unwrap();
    write(source.path(), "a", "two");
    commit(source.path(), "two");
    git(source.path(), &["push"]);
    engine.execute(&plan).await.unwrap();
    assert_eq!(
        git(clone.path(), &["rev-parse", "HEAD"]),
        git(source.path(), &["rev-parse", "HEAD"])
    );
    let status = engine.get_status(&opened.repository_id).await.unwrap();
    let plan = engine
        .prepare_action(
            &opened.repository_id,
            RepositoryAction::Pull {
                status_snapshot_id: status.snapshot_id,
            },
        )
        .await
        .unwrap();
    write(clone.path(), "dirty", "mine");
    assert_eq!(
        engine.execute(&plan).await.err().unwrap().code,
        "precondition_changed"
    );
    let status = engine.get_status(&opened.repository_id).await.unwrap();
    assert_eq!(
        engine
            .prepare_action(
                &opened.repository_id,
                RepositoryAction::Pull {
                    status_snapshot_id: status.snapshot_id
                }
            )
            .await
            .err()
            .unwrap()
            .code,
        "dirty"
    );
    assert_eq!(
        std::fs::read_to_string(clone.path().join("dirty")).unwrap(),
        "mine"
    );
}
#[cfg(unix)]
#[tokio::test]
async fn byte_paths_round_trip() {
    use std::os::unix::ffi::OsStringExt;
    let dir = repo();
    let names = [
        std::ffi::OsString::from("tab\tnewline\n"),
        std::ffi::OsString::from_vec(b"raw-\xff".to_vec()),
    ];
    let mut created = 0;
    for name in &names {
        match std::fs::write(dir.path().join(name), "bytes") {
            Ok(()) => created += 1,
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                eprintln!("Filesystem cannot create non-UTF-8 filename: {e}")
            }
            Err(e) => panic!("{e}"),
        }
    }
    let engine = GitEngine::new();
    let opened = engine.open_repository(dir.path()).await.unwrap();
    let changes = engine.get_changes(&opened.repository_id).await.unwrap();
    let plan = engine
        .prepare_action(
            &opened.repository_id,
            RepositoryAction::Stage {
                snapshot_id: changes.snapshot_id,
                file_ids: changes.files.iter().map(|f| f.file_id.clone()).collect(),
            },
        )
        .await
        .unwrap();
    engine.execute(&plan).await.unwrap();
    assert_eq!(
        engine
            .get_status(&opened.repository_id)
            .await
            .unwrap()
            .counts
            .staged,
        created
    );
}
#[tokio::test]
async fn history_root_merge_parent_binary_and_snapshot() {
    let dir = repo();
    write(dir.path(), "a", "root\n");
    commit(dir.path(), "root");
    let root = git(dir.path(), &["rev-parse", "HEAD"]);
    git(dir.path(), &["switch", "-c", "side"]);
    write(dir.path(), "side", "side\n");
    commit(dir.path(), "side");
    let side = git(dir.path(), &["rev-parse", "HEAD"]);
    git(dir.path(), &["switch", "main"]);
    write(dir.path(), "main", "main\n");
    commit(dir.path(), "main");
    git(dir.path(), &["merge", "--no-ff", "side", "-m", "merge"]);
    let engine = GitEngine::new();
    let opened = engine.open_repository(dir.path()).await.unwrap();
    let details = engine
        .get_commit(&opened.repository_id, "HEAD", Some(side.clone()))
        .await
        .unwrap();
    assert_eq!(details.parents.len(), 2);
    assert_eq!(details.selected_parent.as_deref(), Some(side.as_str()));
    assert_eq!(details.files[0].display_path, "main");
    let diff = engine
        .get_diff(DiffRequest {
            repository_id: opened.repository_id.clone(),
            snapshot_id: details.snapshot_id,
            file_id: details.files[0].file_id.clone(),
            context: "commit".into(),
        })
        .await
        .unwrap();
    assert!(matches!(diff,DiffResult::Text{text,..} if text.contains("+main")));
    let details = engine
        .get_commit(&opened.repository_id, &root, None)
        .await
        .unwrap();
    assert!(details.selected_parent.is_none());
    assert_eq!(details.files.len(), 1);
    let page = engine
        .get_history(&opened.repository_id, None, None, None)
        .await
        .unwrap();
    assert_eq!(page.commits.len(), 4);
    write(dir.path(), "later", "later");
    commit(dir.path(), "later");
    let frozen = engine
        .get_history(&opened.repository_id, None, Some(page.snapshot_id), None)
        .await
        .unwrap();
    assert_eq!(frozen.commits.len(), 4);
    assert_eq!(frozen.commits[0].oid, page.commits[0].oid);
    std::fs::write(dir.path().join("binary"), [0, 1, 2, 3]).unwrap();
    let changes = engine.get_changes(&opened.repository_id).await.unwrap();
    let file = changes
        .files
        .iter()
        .find(|f| f.display_path == "binary")
        .unwrap();
    assert!(matches!(
        engine
            .get_diff(DiffRequest {
                repository_id: opened.repository_id,
                snapshot_id: changes.snapshot_id,
                file_id: file.file_id.clone(),
                context: "unstaged".into()
            })
            .await
            .unwrap(),
        DiffResult::Binary { .. }
    ));
}
#[tokio::test]
async fn branches_push_and_worktree_identity() {
    let dir = repo();
    write(dir.path(), "a", "root");
    commit(dir.path(), "root");
    let bare = tempfile::tempdir().unwrap();
    git(bare.path(), &["init", "--bare"]);
    git(
        dir.path(),
        &["remote", "add", "origin", bare.path().to_str().unwrap()],
    );
    git(dir.path(), &["config", "push.followTags", "true"]);
    git(dir.path(), &["tag", "-a", "private-tag", "-m", "private"]);
    let engine = GitEngine::new();
    let opened = engine.open_repository(dir.path()).await.unwrap();
    let oid = git(dir.path(), &["rev-parse", "HEAD"]);
    let plan = engine
        .prepare_action(
            &opened.repository_id,
            RepositoryAction::CreateBranch {
                status_snapshot_id: opened.status.snapshot_id,
                name: "topic".into(),
                start_oid: oid,
                switch_after_create: true,
            },
        )
        .await
        .unwrap();
    engine.execute(&plan).await.unwrap();
    assert_eq!(git(dir.path(), &["branch", "--show-current"]), "topic");
    let status = engine.get_status(&opened.repository_id).await.unwrap();
    let plan = engine
        .prepare_action(
            &opened.repository_id,
            RepositoryAction::Push {
                status_snapshot_id: status.snapshot_id,
                remote_id: "origin".into(),
                destination_ref: "refs/heads/topic".into(),
                set_upstream: true,
            },
        )
        .await
        .unwrap();
    engine.execute(&plan).await.unwrap();
    assert_eq!(
        git(bare.path(), &["for-each-ref", "--format=%(refname)"]),
        "refs/heads/topic"
    );
    let worktree = tempfile::tempdir().unwrap();
    git(
        dir.path(),
        &["worktree", "add", worktree.path().to_str().unwrap(), "main"],
    );
    let linked = engine.open_repository(worktree.path()).await.unwrap();
    assert_ne!(linked.repository_id, opened.repository_id);
    assert!(matches!(linked.status.head,Head::Branch{name,..} if name=="main"));
}
#[tokio::test]
async fn divergence_and_branch_changes_never_reinterpreted() {
    let source = repo();
    write(source.path(), "a", "root");
    commit(source.path(), "root");
    let bare = tempfile::tempdir().unwrap();
    git(bare.path(), &["init", "--bare"]);
    git(
        source.path(),
        &["remote", "add", "origin", bare.path().to_str().unwrap()],
    );
    git(source.path(), &["push", "-u", "origin", "main"]);
    let clone = repo();
    git(
        clone.path(),
        &["remote", "add", "origin", bare.path().to_str().unwrap()],
    );
    git(clone.path(), &["fetch", "origin"]);
    git(clone.path(), &["reset", "--hard", "origin/main"]);
    git(clone.path(), &["branch", "--set-upstream-to=origin/main"]);
    write(clone.path(), "local", "local");
    commit(clone.path(), "local");
    let local = git(clone.path(), &["rev-parse", "HEAD"]);
    write(source.path(), "remote", "remote");
    commit(source.path(), "remote");
    git(source.path(), &["push"]);
    let engine = GitEngine::new();
    let opened = engine.open_repository(clone.path()).await.unwrap();
    let plan = engine
        .prepare_action(
            &opened.repository_id,
            RepositoryAction::Pull {
                status_snapshot_id: opened.status.snapshot_id,
            },
        )
        .await
        .unwrap();
    assert!(engine.execute(&plan).await.is_err());
    assert_eq!(git(clone.path(), &["rev-parse", "HEAD"]), local);
    assert!(git(clone.path(), &["stash", "list"]).is_empty());
    let status = engine.get_status(&opened.repository_id).await.unwrap();
    let plan = engine
        .prepare_action(
            &opened.repository_id,
            RepositoryAction::Pull {
                status_snapshot_id: status.snapshot_id,
            },
        )
        .await
        .unwrap();
    git(clone.path(), &["switch", "-c", "changed"]);
    assert_eq!(
        engine.execute(&plan).await.err().unwrap().code,
        "precondition_changed"
    );
    assert_eq!(git(clone.path(), &["branch", "--show-current"]), "changed");
}
#[tokio::test]
async fn rename_deletion_and_stale_file_selection() {
    let dir = repo();
    write(dir.path(), "before name", "root");
    write(dir.path(), "delete", "delete");
    commit(dir.path(), "root");
    git(dir.path(), &["mv", "before name", "after name"]);
    std::fs::remove_file(dir.path().join("delete")).unwrap();
    let engine = GitEngine::new();
    let opened = engine.open_repository(dir.path()).await.unwrap();
    let changes = engine.get_changes(&opened.repository_id).await.unwrap();
    let renamed = changes
        .files
        .iter()
        .find(|f| f.display_path == "after name")
        .unwrap();
    assert_eq!(renamed.old_display_path.as_deref(), Some("before name"));
    let plan = engine
        .prepare_action(
            &opened.repository_id,
            RepositoryAction::Unstage {
                snapshot_id: changes.snapshot_id,
                file_ids: vec![renamed.file_id.clone()],
            },
        )
        .await
        .unwrap();
    engine.execute(&plan).await.unwrap();
    assert!(git(dir.path(), &["diff", "--cached", "--name-only"]).is_empty());
    let changes = engine.get_changes(&opened.repository_id).await.unwrap();
    let deletion = changes
        .files
        .iter()
        .find(|f| f.display_path == "delete")
        .unwrap();
    let plan = engine
        .prepare_action(
            &opened.repository_id,
            RepositoryAction::Stage {
                snapshot_id: changes.snapshot_id.clone(),
                file_ids: vec![deletion.file_id.clone()],
            },
        )
        .await
        .unwrap();
    engine.execute(&plan).await.unwrap();
    assert_eq!(
        git(dir.path(), &["diff", "--cached", "--name-only"]),
        "delete"
    );
    assert_eq!(
        engine
            .prepare_action(
                &opened.repository_id,
                RepositoryAction::Stage {
                    snapshot_id: changes.snapshot_id,
                    file_ids: vec![deletion.file_id.clone()]
                }
            )
            .await
            .err()
            .unwrap()
            .code,
        "precondition_changed"
    );
}
#[cfg(unix)]
#[tokio::test]
async fn rejected_commit_hook_preserves_head_and_index() {
    use std::os::unix::fs::PermissionsExt;
    let dir = repo();
    write(dir.path(), "a", "root");
    commit(dir.path(), "root");
    let before = git(dir.path(), &["rev-parse", "HEAD"]);
    write(dir.path(), "a", "next");
    git(dir.path(), &["add", "a"]);
    std::fs::create_dir(dir.path().join(".disabled-hooks")).unwrap();
    let hook = dir.path().join(".disabled-hooks/pre-commit");
    std::fs::write(&hook, "#!/bin/sh\necho rejected >&2\nexit 1\n").unwrap();
    std::fs::set_permissions(hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    let engine = GitEngine::new();
    let opened = engine.open_repository(dir.path()).await.unwrap();
    let plan = engine
        .prepare_action(
            &opened.repository_id,
            RepositoryAction::Commit {
                status_snapshot_id: opened.status.snapshot_id,
                message: "draft retained by caller".into(),
            },
        )
        .await
        .unwrap();
    assert!(engine.execute(&plan).await.is_err());
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), before);
    assert_eq!(
        engine
            .get_status(&opened.repository_id)
            .await
            .unwrap()
            .counts
            .staged,
        1
    );
}

#[tokio::test]
async fn shallow_boundary_and_octopus_parents() {
    let dir = repo();
    write(dir.path(), "root", "root");
    commit(dir.path(), "root");
    for name in ["one", "two", "three"] {
        git(dir.path(), &["switch", "-c", name, "main"]);
        write(dir.path(), name, name);
        commit(dir.path(), name);
    }
    git(dir.path(), &["switch", "main"]);
    git(
        dir.path(),
        &["merge", "--no-ff", "one", "two", "three", "-m", "octopus"],
    );
    let engine = GitEngine::new();
    let opened = engine.open_repository(dir.path()).await.unwrap();
    let details = engine
        .get_commit(&opened.repository_id, "HEAD", None)
        .await
        .unwrap();
    assert!(details.parents.len() >= 3);
    for parent in &details.parents {
        let comparison = engine
            .get_commit(&opened.repository_id, &details.oid, Some(parent.clone()))
            .await
            .unwrap();
        assert_eq!(comparison.selected_parent.as_ref(), Some(parent));
    }
    let shallow = tempfile::tempdir().unwrap();
    let url = format!("file://{}", dir.path().to_string_lossy());
    git(
        shallow.path(),
        &["clone", "--depth=1", "-b", "main", &url, "."],
    );
    let opened = engine.open_repository(shallow.path()).await.unwrap();
    let page = engine
        .get_history(&opened.repository_id, Some("HEAD".into()), None, None)
        .await
        .unwrap();
    assert_eq!(page.commits.len(), 1);
    assert!(page.commits[0].boundary);
    assert!(page.commits[0].parents.len() >= 3);
}
#[tokio::test]
async fn fetch_does_not_prune_and_unsupported_push_rejected() {
    let dir = repo();
    write(dir.path(), "a", "root");
    commit(dir.path(), "root");
    let bare = tempfile::tempdir().unwrap();
    git(bare.path(), &["init", "--bare"]);
    git(
        dir.path(),
        &["remote", "add", "origin", bare.path().to_str().unwrap()],
    );
    git(dir.path(), &["push", "origin", "main", "main:old"]);
    git(dir.path(), &["fetch", "origin"]);
    git(bare.path(), &["update-ref", "-d", "refs/heads/old"]);
    git(dir.path(), &["config", "fetch.prune", "true"]);
    let engine = GitEngine::new();
    let opened = engine.open_repository(dir.path()).await.unwrap();
    let plan = engine
        .prepare_action(
            &opened.repository_id,
            RepositoryAction::Fetch {
                remote_id: "origin".into(),
            },
        )
        .await
        .unwrap();
    engine.execute(&plan).await.unwrap();
    assert!(!git(dir.path(), &["rev-parse", "refs/remotes/origin/old"]).is_empty());
    git(dir.path(), &["config", "remote.origin.mirror", "true"]);
    let status = engine.get_status(&opened.repository_id).await.unwrap();
    assert_eq!(
        engine
            .prepare_action(
                &opened.repository_id,
                RepositoryAction::Push {
                    status_snapshot_id: status.snapshot_id,
                    remote_id: "origin".into(),
                    destination_ref: "refs/heads/main".into(),
                    set_upstream: false
                }
            )
            .await
            .err()
            .unwrap()
            .code,
        "unsupported_push"
    );
    git(dir.path(), &["config", "remote.origin.mirror", "false"]);
    git(
        dir.path(),
        &[
            "config",
            "--add",
            "remote.origin.pushurl",
            bare.path().to_str().unwrap(),
        ],
    );
    git(
        dir.path(),
        &[
            "config",
            "--add",
            "remote.origin.pushurl",
            bare.path().to_str().unwrap(),
        ],
    );
    let status = engine.get_status(&opened.repository_id).await.unwrap();
    assert_eq!(
        engine
            .prepare_action(
                &opened.repository_id,
                RepositoryAction::Push {
                    status_snapshot_id: status.snapshot_id,
                    remote_id: "origin".into(),
                    destination_ref: "refs/heads/main".into(),
                    set_upstream: false
                }
            )
            .await
            .err()
            .unwrap()
            .code,
        "unsupported_push"
    );
}
#[tokio::test]
async fn immutable_history_paging() {
    use std::io::Write;
    let dir = repo();
    let mut child = Command::new("git")
        .current_dir(dir.path())
        .arg("fast-import")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    for n in 0..205 {
        let message = format!("commit {n}\n");
        write!(input,"commit refs/heads/main\ncommitter Fixture <fixture@example.invalid> {} +0000\ndata {}\n{}\n",1_700_000_000+n,message.len(),message).unwrap();
    }
    drop(input);
    assert!(child.wait_with_output().unwrap().status.success());
    let engine = GitEngine::new();
    let opened = engine.open_repository(dir.path()).await.unwrap();
    let first = engine
        .get_history(&opened.repository_id, None, None, None)
        .await
        .unwrap();
    assert_eq!(first.commits.len(), 200);
    assert!(first.next_cursor.is_some());
    write(dir.path(), "new", "new");
    commit(dir.path(), "outside snapshot");
    let second = engine
        .get_history(
            &opened.repository_id,
            None,
            Some(first.snapshot_id),
            first.next_cursor,
        )
        .await
        .unwrap();
    assert_eq!(second.commits.len(), 5);
    assert!(second.next_cursor.is_none());
    assert_eq!(
        first.commits.last().unwrap().parents[0],
        second.commits[0].oid
    );
}

#[tokio::test]
#[ignore = "Acceptance measurement; run explicitly with --ignored --nocapture"]
async fn acceptance_performance_measurements() {
    use std::{io::Write, time::Instant};
    let dir = repo();
    let mut child = Command::new("git")
        .current_dir(dir.path())
        .arg("fast-import")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    for n in 0..10_000 {
        let message = format!("commit {n}\n");
        write!(input,"commit refs/heads/main\ncommitter Fixture <fixture@example.invalid> {} +0000\ndata {}\n{}\n",1_700_000_000+n,message.len(),message).unwrap();
    }
    drop(input);
    assert!(child.wait_with_output().unwrap().status.success());
    let engine = GitEngine::new();
    let opened = engine.open_repository(dir.path()).await.unwrap();
    let start = Instant::now();
    let history = engine
        .get_history(&opened.repository_id, None, None, None)
        .await
        .unwrap();
    let history_ms = start.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(history.commits.len(), 200);
    let mut fixtures = Vec::new();
    let mut ids = Vec::new();
    for _ in 0..25 {
        let dir = repo();
        write(dir.path(), "a", "root");
        commit(dir.path(), "root");
        ids.push(
            engine
                .open_repository(dir.path())
                .await
                .unwrap()
                .repository_id,
        );
        fixtures.push(dir);
    }
    let start = Instant::now();
    for id in &ids {
        assert!(!engine.get_status(id).await.unwrap().dirty);
    }
    let sweep_ms = start.elapsed().as_secs_f64() * 1000.0;
    println!("PERF OS={} ARCH={} MODE=debug HISTORY_10000_FIRST200_MS={history_ms:.2} STATUS25_SEQUENTIAL_MS={sweep_ms:.2}",std::env::consts::OS,std::env::consts::ARCH);
}

#[tokio::test]
async fn dirty_submodule_and_existing_operation_are_visible() {
    let child = repo();
    write(child.path(), "a", "child");
    commit(child.path(), "child");
    let dir = repo();
    write(dir.path(), "base", "root");
    commit(dir.path(), "root");
    git(
        dir.path(),
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            child.path().to_str().unwrap(),
            "module",
        ],
    );
    commit(dir.path(), "module");
    write(&dir.path().join("module"), "a", "dirty child");
    let engine = GitEngine::new();
    let opened = engine.open_repository(dir.path()).await.unwrap();
    assert!(opened.status.dirty && opened.status.submodule_dirty);
    assert_eq!(
        engine
            .prepare_action(
                &opened.repository_id,
                RepositoryAction::Pull {
                    status_snapshot_id: opened.status.snapshot_id
                }
            )
            .await
            .err()
            .unwrap()
            .code,
        "dirty"
    );
    std::fs::write(
        dir.path().join(".git/MERGE_HEAD"),
        git(dir.path(), &["rev-parse", "HEAD"]),
    )
    .unwrap();
    let status = engine.get_status(&opened.repository_id).await.unwrap();
    assert_eq!(status.git_state, GitState::Merge);
}
#[tokio::test]
async fn lockfile_is_retained_and_large_preview_is_bounded() {
    let dir = repo();
    write(dir.path(), "a", "root");
    let engine = GitEngine::new();
    let opened = engine.open_repository(dir.path()).await.unwrap();
    let changes = engine.get_changes(&opened.repository_id).await.unwrap();
    let plan = engine
        .prepare_action(
            &opened.repository_id,
            RepositoryAction::Stage {
                snapshot_id: changes.snapshot_id,
                file_ids: vec![changes.files[0].file_id.clone()],
            },
        )
        .await
        .unwrap();
    std::fs::write(dir.path().join(".git/index.lock"), "external").unwrap();
    assert_eq!(
        engine.execute(&plan).await.err().unwrap().code,
        "lock_conflict"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join(".git/index.lock")).unwrap(),
        "external"
    );
    std::fs::write(dir.path().join("big"), vec![b'a'; 2 * 1024 * 1024 + 1]).unwrap();
    let changes = engine.get_changes(&opened.repository_id).await.unwrap();
    let file = changes
        .files
        .iter()
        .find(|f| f.display_path == "big")
        .unwrap();
    let diff = engine
        .get_diff(DiffRequest {
            repository_id: opened.repository_id,
            snapshot_id: changes.snapshot_id,
            file_id: file.file_id.clone(),
            context: "unstaged".into(),
        })
        .await
        .unwrap();
    assert!(matches!(diff,DiffResult::Text{text,truncated:true} if text.len()==2*1024*1024));
}
