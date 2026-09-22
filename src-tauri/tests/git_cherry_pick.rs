use git_view::{contracts::*, git::GitEngine};
use std::{path::Path, process::Command};
use tempfile::TempDir;
fn fixture_command(path: &Path) -> Command {
    let mut command = Command::new("git");
    let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
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

fn commit(path: &Path, message: &str) -> String {
    git(path, &["add", "--all"]);
    git(path, &["commit", "--allow-empty", "-m", message]);
    git(path, &["rev-parse", "HEAD"])
}

fn fixture(conflict: bool) -> (TempDir, String, String) {
    let dir = repo();
    let p = dir.path();
    std::fs::write(p.join("file"), "base\n").unwrap();
    commit(p, "base");
    git(p, &["switch", "-c", "source"]);
    std::fs::write(p.join(if conflict { "file" } else { "added" }), "source\n").unwrap();
    git(p, &["config", "user.name", "Source Author"]);
    git(p, &["config", "user.email", "source@example.invalid"]);
    let source = commit(p, "Source subject\n\nFull source body");
    git(p, &["switch", "main"]);
    git(p, &["config", "user.name", "Target Committer"]);
    git(p, &["config", "user.email", "target@example.invalid"]);
    std::fs::write(
        p.join(if conflict { "file" } else { "unrelated" }),
        "target\n",
    )
    .unwrap();
    let target = commit(p, "target independent change");
    (dir, source, target)
}
async fn prepare(
    engine: &GitEngine,
    id: &str,
    kind: &str,
    source: &str,
) -> AppResult<git_view::git::PreparedAction> {
    let snapshot = engine.get_status(id).await?.snapshot_id;
    let action = match kind {
        "start" => RepositoryAction::CherryPick {
            status_snapshot_id: snapshot,
            source_oid: source.into(),
        },
        "continue" => RepositoryAction::ContinueCherryPick {
            status_snapshot_id: snapshot,
        },
        "abort" => RepositoryAction::AbortCherryPick {
            status_snapshot_id: snapshot,
        },
        "skip" => RepositoryAction::SkipCherryPick {
            status_snapshot_id: snapshot,
        },
        _ => panic!(),
    };
    engine.prepare_action(id, action).await
}
async fn stage(engine: &GitEngine, id: &str) {
    let changes = engine.get_changes(id).await.unwrap();
    let action = RepositoryAction::Stage {
        snapshot_id: changes.snapshot_id,
        file_ids: changes.files.into_iter().map(|f| f.file_id).collect(),
    };
    let plan = engine.prepare_action(id, action).await.unwrap();
    engine.execute(&plan).await.unwrap();
}
fn hook(path: &Path, name: &str, body: &str) {
    let hooks = path.join(".git/test-hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    let file = hooks.join(name);
    std::fs::write(&file, body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    git(path, &["config", "core.hooksPath", ".git/test-hooks"]);
}
#[tokio::test]
async fn success_preserves_source_message_author_and_unrelated_target_content() {
    let (dir, source, target) = fixture(false);
    let p = dir.path();
    for name in ["pre-commit", "commit-msg"] {
        hook(p, name, "#!/bin/sh\nexit 1\n");
    }
    hook(
        p,
        "prepare-commit-msg",
        "#!/bin/sh\necho prepare >> .git/hook-observed\n",
    );
    hook(
        p,
        "post-commit",
        "#!/bin/sh\necho post >> .git/hook-observed\n",
    );
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    let plan = prepare(&engine, &id, "start", &source).await.unwrap();
    assert!(plan.summary.contains(&source) && plan.summary.contains(&target));
    engine.execute(&plan).await.unwrap();
    assert_eq!(git(p, &["show", "-s", "--format=%P", "HEAD"]), target);
    for format in ["%B", "%an%n%ae%n%aI"] {
        assert_eq!(
            git(p, &["show", "-s", &format!("--format={format}"), "HEAD"]),
            git(p, &["show", "-s", &format!("--format={format}"), &source])
        );
    }
    assert_eq!(
        std::fs::read_to_string(p.join("unrelated")).unwrap(),
        "target\n"
    );
    assert_eq!(
        std::fs::read_to_string(p.join(".git/hook-observed")).unwrap(),
        "prepare\npost\n"
    );
    assert_eq!(
        engine.get_status(&id).await.unwrap().git_state,
        GitState::Idle
    );
    assert!(!p.join(".git/git-view-cherry-pick.json").exists());
}
#[tokio::test]
async fn conflict_reload_stage_continue_and_normal_commit_hooks() {
    let (dir, source, target) = fixture(true);
    let p = dir.path();
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    let plan = prepare(&engine, &id, "start", &source).await.unwrap();
    assert!(engine.execute(&plan).await.is_err());
    let restarted = GitEngine::isolated();
    let id = restarted.open_repository(p).await.unwrap().repository_id;
    let context = restarted
        .get_status(&id)
        .await
        .unwrap()
        .operation_context
        .unwrap();
    assert_eq!(context.stop_reason, OperationStopReason::Conflicts);
    assert_eq!(context.original_head, Some(target.clone()));
    assert!(!context.can_continue);
    std::fs::write(p.join("file"), "resolved\n").unwrap();
    stage(&restarted, &id).await;
    hook(
        p,
        "pre-commit",
        "#!/bin/sh\necho continued >> .git/continued-hook\n",
    );
    let plan = prepare(&restarted, &id, "continue", "").await.unwrap();
    restarted.execute(&plan).await.unwrap();
    assert_eq!(git(p, &["show", "-s", "--format=%P", "HEAD"]), target);
    assert_eq!(
        std::fs::read_to_string(p.join(".git/continued-hook")).unwrap(),
        "continued\n"
    );
    assert_eq!(
        restarted.get_status(&id).await.unwrap().git_state,
        GitState::Idle
    );
}
#[tokio::test]
async fn abort_restores_reviewed_target_and_removes_owned_marker() {
    let (dir, source, target) = fixture(true);
    let p = dir.path();
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    assert!(engine
        .execute(&prepare(&engine, &id, "start", &source).await.unwrap())
        .await
        .is_err());
    std::fs::write(p.join("file"), "resolution to discard\n").unwrap();
    stage(&engine, &id).await;
    engine
        .execute(&prepare(&engine, &id, "abort", "").await.unwrap())
        .await
        .unwrap();
    assert_eq!(git(p, &["rev-parse", "HEAD"]), target);
    assert_eq!(std::fs::read_to_string(p.join("file")).unwrap(), "target\n");
    assert!(git(p, &["status", "--porcelain"]).is_empty());
}
#[tokio::test]
async fn initially_empty_rejected_and_becomes_empty_stops_until_explicit_skip() {
    let (dir, source, _) = fixture(false);
    let p = dir.path();
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    let empty = commit(p, "initially empty");
    assert_eq!(
        prepare(&engine, &id, "start", &empty)
            .await
            .err()
            .unwrap()
            .code,
        "initially_empty_commit"
    );
    assert!(!p.join(".git/CHERRY_PICK_HEAD").exists());
    engine
        .execute(&prepare(&engine, &id, "start", &source).await.unwrap())
        .await
        .unwrap();
    let original = git(p, &["rev-parse", "HEAD"]);
    assert!(engine
        .execute(&prepare(&engine, &id, "start", &source).await.unwrap())
        .await
        .is_err());
    let context = engine
        .get_status(&id)
        .await
        .unwrap()
        .operation_context
        .unwrap();
    assert_eq!(context.stop_reason, OperationStopReason::Empty);
    assert!(context.can_skip && !context.can_continue);
    engine
        .execute(&prepare(&engine, &id, "skip", "").await.unwrap())
        .await
        .unwrap();
    assert_eq!(git(p, &["rev-parse", "HEAD"]), original);
    assert_eq!(
        engine.get_status(&id).await.unwrap().git_state,
        GitState::Idle
    );
}
#[tokio::test]
async fn hook_and_signing_failures_leave_ready_stopped_pick() {
    for signing in [false, true] {
        let (dir, source, target) = fixture(false);
        let p = dir.path();
        if signing {
            git(p, &["config", "commit.gpgSign", "true"]);
            git(p, &["config", "gpg.program", "git-view-missing-signer"]);
        } else {
            hook(p, "prepare-commit-msg", "#!/bin/sh\nexit 1\n");
        }
        let engine = GitEngine::isolated();
        let id = engine.open_repository(p).await.unwrap().repository_id;
        assert!(engine
            .execute(&prepare(&engine, &id, "start", &source).await.unwrap())
            .await
            .is_err());
        let context = engine
            .get_status(&id)
            .await
            .unwrap()
            .operation_context
            .unwrap();
        assert_eq!(context.stop_reason, OperationStopReason::Ready);
        assert!(context.can_continue && context.can_abort);
        assert_eq!(git(p, &["rev-parse", "HEAD"]), target);
        git(p, &["config", "commit.gpgSign", "false"]);
        hook(p, "prepare-commit-msg", "#!/bin/sh\nexit 0\n");
        engine
            .execute(&prepare(&engine, &id, "continue", "").await.unwrap())
            .await
            .unwrap();
    }
}
#[tokio::test]
async fn ignored_destination_and_directory_rename_preserve_private_bytes() {
    for renamed in [false, true] {
        for configured in [false, true] {
            let dir = repo();
            let p = dir.path();
            std::fs::create_dir(p.join("old")).unwrap();
            std::fs::write(p.join("old/existing"), "base").unwrap();
            commit(p, "base");
            git(p, &["switch", "-c", "source"]);
            std::fs::write(p.join("old/new"), "source").unwrap();
            let source = commit(p, "add");
            git(p, &["switch", "main"]);
            let destination = if renamed {
                git(p, &["mv", "old", "renamed"]);
                commit(p, "rename directory");
                "renamed/new"
            } else {
                "old/new"
            };
            if configured {
                git(p, &["config", "merge.directoryRenames", "true"]);
            }
            std::fs::write(p.join(".git/info/exclude"), format!("{destination}\n")).unwrap();
            std::fs::write(p.join(destination), "private bytes").unwrap();
            let original = git(p, &["rev-parse", "HEAD"]);
            let engine = GitEngine::isolated();
            let id = engine.open_repository(p).await.unwrap().repository_id;
            assert!(prepare(&engine, &id, "start", &source).await.is_err());
            assert_eq!(
                std::fs::read_to_string(p.join(destination)).unwrap(),
                "private bytes"
            );
            assert_eq!(git(p, &["rev-parse", "HEAD"]), original);
            assert!(!p.join(".git/git-view-cherry-pick.json").exists());
        }
    }
}
#[tokio::test]
async fn stale_preview_metadata_and_external_target_drift_are_rejected() {
    let (dir, source, target) = fixture(true);
    let p = dir.path();
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    let stale = prepare(&engine, &id, "start", &source).await.unwrap();
    git(p, &["config", "test.drift", "yes"]);
    assert_eq!(
        engine.execute(&stale).await.err().unwrap().code,
        "precondition_changed"
    );
    assert!(engine
        .execute(&prepare(&engine, &id, "start", &source).await.unwrap())
        .await
        .is_err());
    let abort = prepare(&engine, &id, "abort", "").await.unwrap();
    std::fs::write(p.join(".git/MERGE_MSG"), "external message").unwrap();
    assert_eq!(
        engine.execute(&abort).await.err().unwrap().code,
        "precondition_changed"
    );
    git(p, &["update-ref", "refs/heads/main", &source, &target]);
    let context = engine
        .get_status(&id)
        .await
        .unwrap()
        .operation_context
        .unwrap();
    assert_eq!(context.stop_reason, OperationStopReason::Unknown);
    assert!(!context.can_abort && !context.can_continue && !context.can_skip);
    assert!(p.join(".git/git-view-cherry-pick.json").exists());
}
#[tokio::test]
async fn external_single_pick_context_is_honest_and_sequencer_is_unavailable() {
    let (dir, source, target) = fixture(true);
    let p = dir.path();
    let output = fixture_command(p)
        .args(["cherry-pick", &source])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    let context = engine
        .get_status(&id)
        .await
        .unwrap()
        .operation_context
        .unwrap();
    assert!(context.original_head.is_none());
    assert!(context.reason.contains("externally"));
    std::fs::create_dir(p.join(".git/sequencer")).unwrap();
    std::fs::write(
        p.join(".git/sequencer/todo"),
        format!("pick {source} source\n"),
    )
    .unwrap();
    let context = engine
        .get_status(&id)
        .await
        .unwrap()
        .operation_context
        .unwrap();
    assert_eq!(context.stop_reason, OperationStopReason::Unknown);
    assert!(!context.can_abort && !context.can_continue);
    std::fs::remove_file(p.join(".git/sequencer/todo")).unwrap();
    std::fs::remove_dir(p.join(".git/sequencer")).unwrap();
    engine
        .execute(&prepare(&engine, &id, "abort", "").await.unwrap())
        .await
        .unwrap();
    assert_eq!(git(p, &["rev-parse", "HEAD"]), target);
}
#[tokio::test]
async fn abort_does_not_recurse_into_dirty_submodules_for_cherry_or_merge() {
    for cherry in [true, false] {
        let child = repo();
        std::fs::write(child.path().join("f"), "submodule base").unwrap();
        commit(child.path(), "submodule base");
        let (dir, source, _) = fixture(true);
        let p = dir.path();
        git(
            p,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                child.path().to_str().unwrap(),
                "s",
            ],
        );
        let target = commit(p, "add submodule");
        git(p, &["config", "submodule.recurse", "true"]);
        let engine = GitEngine::isolated();
        let id = engine.open_repository(p).await.unwrap().repository_id;
        let plan = if cherry {
            prepare(&engine, &id, "start", &source).await.unwrap()
        } else {
            let status = engine.get_status(&id).await.unwrap();
            engine
                .prepare_action(
                    &id,
                    RepositoryAction::Merge {
                        status_snapshot_id: status.snapshot_id,
                        source_ref: "refs/heads/source".into(),
                    },
                )
                .await
                .unwrap()
        };
        assert!(engine.execute(&plan).await.is_err());
        std::fs::write(p.join("s/f"), "private nested edit").unwrap();
        let plan = if cherry {
            prepare(&engine, &id, "abort", "").await.unwrap()
        } else {
            let status = engine.get_status(&id).await.unwrap();
            engine
                .prepare_action(
                    &id,
                    RepositoryAction::AbortMerge {
                        status_snapshot_id: status.snapshot_id,
                    },
                )
                .await
                .unwrap()
        };
        engine.execute(&plan).await.unwrap();
        assert_eq!(
            std::fs::read_to_string(p.join("s/f")).unwrap(),
            "private nested edit"
        );
        assert_eq!(git(p, &["rev-parse", "HEAD"]), target);
        assert!(engine.get_status(&id).await.unwrap().submodule_dirty);
    }
}
#[tokio::test]
async fn directory_to_file_and_symlink_ancestors_are_protected() {
    let dir = repo();
    let p = dir.path();
    std::fs::create_dir(p.join("folder")).unwrap();
    std::fs::write(p.join("folder/tracked"), "base").unwrap();
    commit(p, "base");
    git(p, &["switch", "-c", "source"]);
    git(p, &["rm", "folder/tracked"]);
    std::fs::write(p.join("folder"), "source file").unwrap();
    let source = commit(p, "directory to file");
    git(p, &["switch", "main"]);
    std::fs::write(p.join(".git/info/exclude"), "folder/private\n").unwrap();
    std::fs::write(p.join("folder/private"), "private").unwrap();
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    assert_eq!(
        prepare(&engine, &id, "start", &source)
            .await
            .err()
            .unwrap()
            .code,
        "local_path_collision"
    );
    assert_eq!(
        std::fs::read_to_string(p.join("folder/private")).unwrap(),
        "private"
    );
    #[cfg(unix)]
    {
        let (other, source, _) = fixture(false);
        let p = other.path();
        std::fs::write(p.join(".git/info/exclude"), "added\n").unwrap();
        let outside = tempfile::tempdir().unwrap();
        let file = outside.path().join("secret");
        std::fs::write(&file, "outside bytes").unwrap();
        std::os::unix::fs::symlink(&file, p.join("added")).unwrap();
        let engine = GitEngine::isolated();
        let id = engine.open_repository(p).await.unwrap().repository_id;
        assert_eq!(
            prepare(&engine, &id, "start", &source)
                .await
                .err()
                .unwrap()
                .code,
            "local_path_collision"
        );
        assert_eq!(std::fs::read_to_string(file).unwrap(), "outside bytes");
    }
}
#[tokio::test]
async fn dirty_detached_merge_selection_and_cancellation_do_not_start_a_pick() {
    let (dir, source, target) = fixture(false);
    let p = dir.path();
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    for invalid in ["HEAD", "HEAD..source", "-x", "abcd"] {
        assert!(prepare(&engine, &id, "start", invalid).await.is_err());
    }
    let plan = prepare(&engine, &id, "start", &source).await.unwrap();
    let canceled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    assert_eq!(
        engine
            .execute_cancelable(&plan, canceled)
            .await
            .err()
            .unwrap()
            .code,
        "canceled"
    );
    assert!(!p.join(".git/git-view-cherry-pick.json").exists());
    std::fs::write(p.join("untracked"), "retain").unwrap();
    assert!(prepare(&engine, &id, "start", &source).await.is_err());
    std::fs::remove_file(p.join("untracked")).unwrap();
    git(p, &["switch", "--detach"]);
    assert!(prepare(&engine, &id, "start", &source).await.is_err());
    git(p, &["switch", "main"]);
    git(p, &["merge", "--no-ff", "source", "-m", "merge"]);
    let merge = git(p, &["rev-parse", "HEAD"]);
    assert!(prepare(&engine, &id, "start", &merge).await.is_err());
    assert_eq!(
        git(p, &["show", "-s", "--format=%P", &merge])
            .split_whitespace()
            .next(),
        Some(target.as_str())
    );
}
#[tokio::test]
async fn interrupted_marker_without_git_state_requires_reviewed_clean_clear() {
    let (dir, source, target) = fixture(false);
    let p = dir.path();
    let journal = serde_json::json!({"version":1,"operationId":"00000000-0000-4000-8000-000000000001","branch":"refs/heads/main","originalHead":target,"sourceOid":source});
    std::fs::write(
        p.join(".git/git-view-cherry-pick.json"),
        serde_json::to_vec(&journal).unwrap(),
    )
    .unwrap();
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    let status = engine.get_status(&id).await.unwrap();
    assert_eq!(status.git_state, GitState::Other);
    let context = status.operation_context.unwrap();
    assert_eq!(context.stop_reason, OperationStopReason::Unknown);
    assert!(context.can_abort && !context.can_continue);
    engine
        .execute(&prepare(&engine, &id, "abort", "").await.unwrap())
        .await
        .unwrap();
    assert_eq!(
        engine.get_status(&id).await.unwrap().git_state,
        GitState::Idle
    );
    assert_eq!(git(p, &["rev-parse", "HEAD"]), target);
}
#[tokio::test]
async fn root_source_and_sparse_safety_have_explicit_outcomes() {
    let dir = repo();
    let p = dir.path();
    std::fs::write(p.join("root-file"), "root").unwrap();
    let root = commit(p, "root source");
    git(p, &["switch", "--orphan", "target"]);
    std::fs::write(p.join("target"), "target").unwrap();
    commit(p, "target root");
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    engine
        .execute(&prepare(&engine, &id, "start", &root).await.unwrap())
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(p.join("root-file")).unwrap(),
        "root"
    );
    let (dir, source, _) = fixture(true);
    let p = dir.path();
    git(p, &["update-index", "--skip-worktree", "file"]);
    std::fs::write(p.join("file"), "private hidden tracked bytes").unwrap();
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    let plan = prepare(&engine, &id, "start", &source).await.unwrap();
    assert!(engine.execute(&plan).await.is_err());
    assert_eq!(
        std::fs::read_to_string(p.join("file")).unwrap(),
        "private hidden tracked bytes"
    );
}
#[tokio::test]
async fn unavailable_external_source_keeps_working_changes_inspectable() {
    let (dir, source, _) = fixture(true);
    let p = dir.path();
    let result = fixture_command(p)
        .args(["cherry-pick", &source])
        .output()
        .unwrap();
    assert!(!result.status.success());
    std::fs::write(
        p.join(".git/CHERRY_PICK_HEAD"),
        "1111111111111111111111111111111111111111\n",
    )
    .unwrap();
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    let status = engine.get_status(&id).await.unwrap();
    let context = status.operation_context.unwrap();
    assert_eq!(context.stop_reason, OperationStopReason::Unknown);
    assert!(!context.can_abort && !context.can_continue && !context.can_skip);
    assert!(status.counts.conflicted > 0);
    assert!(!engine.get_changes(&id).await.unwrap().files.is_empty());
}
#[tokio::test]
async fn stale_owned_journal_cannot_enable_another_operations_mutations() {
    let (dir, source, target) = fixture(true);
    let p = dir.path();
    let output = fixture_command(p)
        .args(["merge", "source", "-m", "external merge"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let journal = serde_json::json!({"version":1,"operationId":"00000000-0000-4000-8000-000000000001","branch":"refs/heads/main","originalHead":target,"sourceOid":source});
    std::fs::write(
        p.join(".git/git-view-cherry-pick.json"),
        serde_json::to_vec(&journal).unwrap(),
    )
    .unwrap();
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    let status = engine.get_status(&id).await.unwrap();
    assert_eq!(status.git_state, GitState::Merge);
    assert_eq!(
        status.operation_context.as_ref().unwrap().stop_reason,
        OperationStopReason::Unknown
    );
    assert_eq!(
        engine
            .prepare_action(
                &id,
                RepositoryAction::AbortMerge {
                    status_snapshot_id: status.snapshot_id.clone()
                }
            )
            .await
            .err()
            .unwrap()
            .code,
        "cherry_pick_manual_review"
    );
    assert_eq!(
        engine
            .prepare_action(
                &id,
                RepositoryAction::ContinueMerge {
                    status_snapshot_id: status.snapshot_id
                }
            )
            .await
            .err()
            .unwrap()
            .code,
        "cherry_pick_manual_review"
    );
    let changes = engine.get_changes(&id).await.unwrap();
    let before = std::fs::read(p.join(".git/index")).unwrap();
    assert_eq!(
        engine
            .prepare_action(
                &id,
                RepositoryAction::Stage {
                    snapshot_id: changes.snapshot_id,
                    file_ids: changes.files.into_iter().map(|f| f.file_id).collect()
                }
            )
            .await
            .err()
            .unwrap()
            .code,
        "cherry_pick_manual_review"
    );
    assert_eq!(std::fs::read(p.join(".git/index")).unwrap(), before);
}
