use git_view::{contracts::*, git::GitEngine};
use std::{path::Path, process::Command};
use tempfile::TempDir;
fn fixture_command(path: &Path) -> Command {
    let mut command = Command::new("git");
    let null = "/dev/null";
    command
        .current_dir(path)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", null)
        .env("GIT_CONFIG_SYSTEM", null);
    for key in [
        "GIT_TEMPLATE_DIR",
        "GIT_EXEC_PATH",
        "GIT_CONFIG",
        "GIT_NAMESPACE",
        "GIT_SHALLOW_FILE",
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_CONFIG_COUNT",
        "GIT_CONFIG_PARAMETERS",
        "GIT_AUTHOR_NAME",
        "GIT_AUTHOR_EMAIL",
        "GIT_AUTHOR_DATE",
        "GIT_COMMITTER_NAME",
        "GIT_COMMITTER_EMAIL",
        "GIT_COMMITTER_DATE",
        "EMAIL",
    ] {
        command.env_remove(key);
    }
    command
}
fn git(path: &Path, args: &[&str]) -> String {
    let output = fixture_command(path).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().into()
}
fn hook(path: &Path, script: &str) {
    std::fs::write(path, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
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
    let engine = GitEngine::isolated();
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
    git(
        clone.path(),
        &["config", "branch.main.mergeOptions", "--squash"],
    );
    let engine = GitEngine::isolated();
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
            Err(e)
                if name.to_str().is_none()
                    && (e.kind() == std::io::ErrorKind::PermissionDenied
                        || (cfg!(target_os = "macos") && e.raw_os_error() == Some(92))) =>
            {
                eprintln!("SKIP raw non-UTF-8 filename only: filesystem rejects its creation ({e}); tab/newline coverage remains mandatory.")
            }
            Err(e) => panic!("{e}"),
        }
    }
    let engine = GitEngine::isolated();
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
    let engine = GitEngine::isolated();
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
    let engine = GitEngine::isolated();
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
    let engine = GitEngine::isolated();
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
    let engine = GitEngine::isolated();
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
#[tokio::test]
async fn rejected_commit_hook_preserves_head_and_index() {
    let dir = repo();
    write(dir.path(), "a", "root");
    commit(dir.path(), "root");
    let before = git(dir.path(), &["rev-parse", "HEAD"]);
    write(dir.path(), "a", "next");
    git(dir.path(), &["add", "a"]);
    std::fs::create_dir(dir.path().join(".disabled-hooks")).unwrap();
    hook(
        &dir.path().join(".disabled-hooks/pre-commit"),
        "#!/bin/sh\necho rejected >&2\nexit 1\n",
    );
    let engine = GitEngine::isolated();
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
    let engine = GitEngine::isolated();
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
    let engine = GitEngine::isolated();
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
    let mut child = fixture_command(dir.path())
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
    let engine = GitEngine::isolated();
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
    let mut child = fixture_command(dir.path())
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
    let engine = GitEngine::isolated();
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
    println!("PERF OS={} ARCH={} MODE={} HISTORY_10000_FIRST200_MS={history_ms:.2} STATUS25_SEQUENTIAL_MS={sweep_ms:.2}",std::env::consts::OS,std::env::consts::ARCH,if cfg!(debug_assertions){"debug"}else{"release"});
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
    let engine = GitEngine::isolated();
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
    let engine = GitEngine::isolated();
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
    assert!(matches!(diff,DiffResult::Text{text,truncated:true,..} if text.len()==2*1024*1024));
}

#[tokio::test]
async fn unborn_unstage_after_additional_edits_retains_worktree() {
    let dir = repo();
    write(dir.path(), "a", "staged");
    git(dir.path(), &["add", "a"]);
    write(dir.path(), "a", "later worktree");
    let engine = GitEngine::isolated();
    let opened = engine.open_repository(dir.path()).await.unwrap();
    let changes = engine.get_changes(&opened.repository_id).await.unwrap();
    let plan = engine
        .prepare_action(
            &opened.repository_id,
            RepositoryAction::Unstage {
                snapshot_id: changes.snapshot_id,
                file_ids: vec![changes.files[0].file_id.clone()],
            },
        )
        .await
        .unwrap();
    engine.execute(&plan).await.unwrap();
    assert!(git(dir.path(), &["ls-files"]).is_empty());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a")).unwrap(),
        "later worktree"
    );
}
#[tokio::test]
async fn successful_hook_altering_index_is_preserved_and_refreshed() {
    let dir = repo();
    write(dir.path(), "a", "root");
    commit(dir.path(), "root");
    write(dir.path(), "a", "next");
    git(dir.path(), &["add", "a"]);
    std::fs::create_dir(dir.path().join(".disabled-hooks")).unwrap();
    hook(
        &dir.path().join(".disabled-hooks/pre-commit"),
        "#!/bin/sh\nprintf hook-added > hook-result\ngit add -- hook-result\n",
    );
    let engine = GitEngine::isolated();
    let opened = engine.open_repository(dir.path()).await.unwrap();
    let plan = engine
        .prepare_action(
            &opened.repository_id,
            RepositoryAction::Commit {
                status_snapshot_id: opened.status.snapshot_id,
                message: "hook changes are authoritative".into(),
            },
        )
        .await
        .unwrap();
    engine.execute(&plan).await.unwrap();
    assert_eq!(git(dir.path(), &["show", "HEAD:hook-result"]), "hook-added");
    assert_eq!(
        engine
            .get_status(&opened.repository_id)
            .await
            .unwrap()
            .counts
            .staged,
        0
    );
}

#[tokio::test]
async fn shared_worktree_mutations_are_serialized() {
    let dir = repo();
    write(dir.path(), "a", "root");
    commit(dir.path(), "root");
    let linked = tempfile::tempdir().unwrap();
    git(
        dir.path(),
        &[
            "worktree",
            "add",
            "-b",
            "side",
            linked.path().to_str().unwrap(),
        ],
    );
    let hooks = dir.path().join(".git/fixture-hooks");
    std::fs::create_dir(&hooks).unwrap();
    git(
        dir.path(),
        &["config", "core.hooksPath", hooks.to_str().unwrap()],
    );
    hook(&hooks.join("pre-commit"),"#!/bin/sh\ncommon=$(git rev-parse --path-format=absolute --git-common-dir)\nmkdir \"$common/git-view-test-lock\" || exit 1\ntrap 'rmdir \"$common/git-view-test-lock\"' 0\nsleep 0.15\n");
    for (path, body) in [(dir.path(), "main change"), (linked.path(), "side change")] {
        write(path, "a", body);
        git(path, &["add", "a"]);
    }
    let engine = GitEngine::isolated();
    let main = engine.open_repository(dir.path()).await.unwrap();
    let side = engine.open_repository(linked.path()).await.unwrap();
    let first = engine
        .prepare_action(
            &main.repository_id,
            RepositoryAction::Commit {
                status_snapshot_id: main.status.snapshot_id,
                message: "main change".into(),
            },
        )
        .await
        .unwrap();
    let second = engine
        .prepare_action(
            &side.repository_id,
            RepositoryAction::Commit {
                status_snapshot_id: side.status.snapshot_id,
                message: "side change".into(),
            },
        )
        .await
        .unwrap();
    let (first, second) = tokio::join!(engine.execute(&first), engine.execute(&second));
    assert!(first.is_ok(), "{:?}", first.err());
    assert!(second.is_ok(), "{:?}", second.err());
    assert_eq!(git(dir.path(), &["show", "HEAD:a"]), "main change");
    assert_eq!(git(linked.path(), &["show", "HEAD:a"]), "side change");
}
#[tokio::test]
async fn push_remote_and_configured_destination_match_preview() {
    let dir = repo();
    write(dir.path(), "a", "root");
    commit(dir.path(), "root");
    let origin = tempfile::tempdir().unwrap();
    let fork = tempfile::tempdir().unwrap();
    for remote in [&origin, &fork] {
        git(remote.path(), &["init", "--bare"]);
    }
    git(
        dir.path(),
        &["remote", "add", "origin", origin.path().to_str().unwrap()],
    );
    git(
        dir.path(),
        &["remote", "add", "fork", fork.path().to_str().unwrap()],
    );
    git(dir.path(), &["config", "branch.main.remote", "origin"]);
    git(
        dir.path(),
        &["config", "branch.main.merge", "refs/heads/main"],
    );
    git(dir.path(), &["config", "remote.pushDefault", "origin"]);
    git(dir.path(), &["config", "branch.main.pushRemote", "fork"]);
    git(
        dir.path(),
        &[
            "config",
            "remote.fork.push",
            "refs/heads/main:refs/heads/review",
        ],
    );
    git(
        dir.path(),
        &["config", "push.recurseSubmodules", "on-demand"],
    );
    let engine = GitEngine::isolated();
    let opened = engine.open_repository(dir.path()).await.unwrap();
    let remotes = engine.list_remotes(&opened.repository_id).await.unwrap();
    let selected = remotes.iter().find(|r| r.default_push).unwrap();
    assert_eq!(selected.name, "fork");
    assert_eq!(
        selected.destination_ref.as_deref(),
        Some("refs/heads/review")
    );
    let plan = engine
        .prepare_action(
            &opened.repository_id,
            RepositoryAction::Push {
                status_snapshot_id: opened.status.snapshot_id,
                remote_id: selected.id.clone(),
                destination_ref: selected.destination_ref.clone().unwrap(),
                set_upstream: false,
            },
        )
        .await
        .unwrap();
    assert_eq!(plan.destination.as_deref(), Some("fork refs/heads/review"));
    engine.execute(&plan).await.unwrap();
    assert!(git(origin.path(), &["for-each-ref"]).is_empty());
    assert_eq!(
        git(fork.path(), &["for-each-ref", "--format=%(refname)"]),
        "refs/heads/review"
    );
}
#[tokio::test]
async fn in_progress_states_block_conflicting_actions() {
    let dir = repo();
    write(dir.path(), "a", "root");
    commit(dir.path(), "root");
    let engine = GitEngine::isolated();
    let opened = engine.open_repository(dir.path()).await.unwrap();
    for (name, state, is_dir) in [
        ("MERGE_HEAD", GitState::Merge, false),
        ("rebase-merge", GitState::Rebase, true),
        ("CHERRY_PICK_HEAD", GitState::CherryPick, false),
        ("REVERT_HEAD", GitState::Revert, false),
        ("sequencer", GitState::Other, true),
    ] {
        let path = dir.path().join(".git").join(name);
        if is_dir {
            std::fs::create_dir(&path).unwrap();
        } else {
            std::fs::write(&path, git(dir.path(), &["rev-parse", "HEAD"])).unwrap();
        }
        let status = engine.get_status(&opened.repository_id).await.unwrap();
        assert_eq!(status.git_state, state);
        let error = engine
            .prepare_action(
                &opened.repository_id,
                RepositoryAction::CreateBranch {
                    status_snapshot_id: status.snapshot_id,
                    name: "blocked".into(),
                    start_oid: git(dir.path(), &["rev-parse", "HEAD"]),
                    switch_after_create: false,
                },
            )
            .await
            .err()
            .unwrap();
        assert_eq!(error.code, "operation_in_progress");
        if is_dir {
            std::fs::remove_dir(&path).unwrap();
        } else {
            std::fs::remove_file(&path).unwrap();
        }
    }
}

#[tokio::test]
async fn signing_rejection_does_not_bypass_signing_or_reset_index() {
    let dir = repo();
    write(dir.path(), "a", "root");
    commit(dir.path(), "root");
    let before = git(dir.path(), &["rev-parse", "HEAD"]);
    let signer = dir.path().join(".git/reject-signing");
    hook(
        &signer,
        "#!/bin/sh\necho fixture-signing-rejection >&2\nexit 1\n",
    );
    git(
        dir.path(),
        &["config", "gpg.program", signer.to_str().unwrap()],
    );
    git(dir.path(), &["config", "gpg.format", "openpgp"]);
    git(
        dir.path(),
        &["config", "user.signingkey", "fixture-rejected"],
    );
    git(dir.path(), &["config", "commit.gpgsign", "true"]);
    write(dir.path(), "a", "staged");
    git(dir.path(), &["add", "a"]);
    let engine = GitEngine::isolated();
    let opened = engine.open_repository(dir.path()).await.unwrap();
    let plan = engine
        .prepare_action(
            &opened.repository_id,
            RepositoryAction::Commit {
                status_snapshot_id: opened.status.snapshot_id,
                message: "must remain signed".into(),
            },
        )
        .await
        .unwrap();
    assert!(engine.execute(&plan).await.is_err());
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), before);
    assert_eq!(git(dir.path(), &["show", ":a"]), "staged");
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
async fn failed_noisy_hook_reports_exit_and_truncation() {
    let dir = repo();
    write(dir.path(), "a", "root");
    commit(dir.path(), "root");
    let before = git(dir.path(), &["rev-parse", "HEAD"]);
    write(dir.path(), "a", "next");
    git(dir.path(), &["add", "a"]);
    std::fs::create_dir(dir.path().join(".disabled-hooks")).unwrap();
    hook(&dir.path().join(".disabled-hooks/pre-commit"),"#!/bin/sh\ni=0\nwhile test $i -lt 9000; do\nprintf 0123456789012345678901234567890123456789012345678901234567890123\nprintf 0123456789012345678901234567890123456789012345678901234567890123 >&2\ni=$((i+1))\ndone\nexit 1\n");
    let engine = GitEngine::isolated();
    let opened = engine.open_repository(dir.path()).await.unwrap();
    let plan = engine
        .prepare_action(
            &opened.repository_id,
            RepositoryAction::Commit {
                status_snapshot_id: opened.status.snapshot_id,
                message: "noisy failure".into(),
            },
        )
        .await
        .unwrap();
    let error = engine.execute(&plan).await.err().unwrap();
    assert_eq!(error.exit_code, Some(1));
    assert_eq!(error.output_truncated, Some(true));
    assert!(error.details.unwrap().len() <= 512 * 1024);
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), before);
}

#[tokio::test]
async fn missing_repository_is_not_reported_as_missing_git() {
    let dir = tempfile::tempdir().unwrap();
    let engine = GitEngine::isolated();
    let error = engine
        .open_repository(&dir.path().join("missing"))
        .await
        .err()
        .unwrap();
    assert_eq!(error.code, "repository_unavailable");
}

#[tokio::test]
async fn cancellation_after_waiting_for_shared_lock_never_starts_git() {
    use std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        time::{Duration, Instant},
    };
    let dir = repo();
    write(dir.path(), "a", "root");
    commit(dir.path(), "root");
    write(dir.path(), "a", "next");
    git(dir.path(), &["add", "a"]);
    std::fs::create_dir(dir.path().join(".disabled-hooks")).unwrap();
    hook(
        &dir.path().join(".disabled-hooks/pre-commit"),
        "#!/bin/sh\nprintf running > .git/fixture-hook-running\nsleep 0.4\n",
    );
    let engine = GitEngine::isolated();
    let opened = engine.open_repository(dir.path()).await.unwrap();
    let first = engine
        .prepare_action(
            &opened.repository_id,
            RepositoryAction::Commit {
                status_snapshot_id: opened.status.snapshot_id.clone(),
                message: "first only".into(),
            },
        )
        .await
        .unwrap();
    let queued = engine
        .prepare_action(
            &opened.repository_id,
            RepositoryAction::Commit {
                status_snapshot_id: opened.status.snapshot_id,
                message: "must not start".into(),
            },
        )
        .await
        .unwrap();
    let first_engine = engine.clone();
    let running = tokio::spawn(async move { first_engine.execute(&first).await });
    let started = Instant::now();
    while !dir.path().join(".git/fixture-hook-running").exists() {
        assert!(started.elapsed() < Duration::from_secs(5));
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let canceled = Arc::new(AtomicBool::new(false));
    let waiting_engine = engine.clone();
    let flag = canceled.clone();
    let waiting =
        tokio::spawn(async move { waiting_engine.execute_cancelable(&queued, flag).await });
    tokio::time::sleep(Duration::from_millis(20)).await;
    canceled.store(true, Ordering::Release);
    assert!(running.await.unwrap().is_ok());
    let error = waiting.await.unwrap().err().unwrap();
    assert_eq!(error.code, "canceled");
    assert_eq!(git(dir.path(), &["rev-list", "--count", "HEAD"]), "2");
    assert_eq!(git(dir.path(), &["log", "-1", "--format=%s"]), "first only");
}
#[tokio::test]
async fn filtered_fetch_guard_rechecks_head_but_allows_worktree_changes() {
    let dir = repo();
    write(dir.path(), "a", "root");
    commit(dir.path(), "root");
    let bare = tempfile::tempdir().unwrap();
    git(bare.path(), &["init", "--bare"]);
    git(
        dir.path(),
        &["remote", "add", "origin", bare.path().to_str().unwrap()],
    );
    git(dir.path(), &["push", "origin", "main"]);
    let linked = tempfile::tempdir().unwrap();
    git(
        dir.path(),
        &[
            "worktree",
            "add",
            "-b",
            "side",
            linked.path().to_str().unwrap(),
        ],
    );
    let engine = GitEngine::isolated();
    let main = engine.open_repository(dir.path()).await.unwrap();
    let side = engine.open_repository(linked.path()).await.unwrap();
    let mut main_fetch = engine
        .prepare_action(
            &main.repository_id,
            RepositoryAction::Fetch {
                remote_id: "origin".into(),
            },
        )
        .await
        .unwrap();
    let mut side_fetch = engine
        .prepare_action(
            &side.repository_id,
            RepositoryAction::Fetch {
                remote_id: "origin".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        main_fetch.fetch_coalesce_key(),
        side_fetch.fetch_coalesce_key()
    );
    main_fetch.guard_head(main.status.head.clone());
    side_fetch.guard_head(side.status.head);
    assert_ne!(
        main_fetch.fetch_coalesce_key(),
        side_fetch.fetch_coalesce_key()
    );
    let mut alias = engine
        .prepare_action(
            &main.repository_id,
            RepositoryAction::Fetch {
                remote_id: "origin".into(),
            },
        )
        .await
        .unwrap();
    alias.guard_head(main.status.head);
    assert_eq!(main_fetch.fetch_coalesce_key(), alias.fetch_coalesce_key());
    write(dir.path(), "dirty", "keep mine");
    engine.execute(&main_fetch).await.unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("dirty")).unwrap(),
        "keep mine"
    );
    git(dir.path(), &["switch", "-c", "changed-after-preview"]);
    assert_eq!(
        engine.execute(&alias).await.err().unwrap().code,
        "precondition_changed"
    );
}
