use git_view::{contracts::*, git::GitEngine};
use std::{path::Path, process::Command};
use tempfile::TempDir;
fn git(path: &Path, args: &[&str]) -> String {
    let mut command = Command::new("git");
    let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
    command
        .current_dir(path)
        .env("GIT_CONFIG_GLOBAL", null)
        .env("GIT_CONFIG_SYSTEM", null)
        .env("GIT_CONFIG_NOSYSTEM", "1");
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
    let output = command.args(args).output().unwrap();
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
    for (key, value) in [
        ("user.name", "Fixture"),
        ("user.email", "fixture@example.invalid"),
        ("commit.gpgsign", "false"),
        ("core.hooksPath", ".git/test-hooks"),
    ] {
        git(dir.path(), &["config", key, value]);
    }
    write(dir.path(), "a", "base\n");
    commit(dir.path(), "base");
    dir
}
fn write(path: &Path, name: &str, body: &str) {
    std::fs::write(path.join(name), body).unwrap();
}
fn commit(path: &Path, message: &str) {
    git(path, &["add", "--all"]);
    git(path, &["commit", "-m", message]);
}
fn divergent(conflict: bool) -> TempDir {
    let dir = repo();
    git(dir.path(), &["switch", "-c", "source"]);
    write(
        dir.path(),
        if conflict { "a" } else { "source" },
        "source\n",
    );
    commit(dir.path(), "source");
    git(dir.path(), &["switch", "main"]);
    write(dir.path(), if conflict { "a" } else { "main" }, "main\n");
    commit(dir.path(), "main");
    dir
}
fn hook(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}
async fn merge(
    engine: &GitEngine,
    id: &str,
    squash: bool,
) -> AppResult<git_view::git::ActionOutcome> {
    let status = engine.get_status(id).await?;
    let action = if squash {
        RepositoryAction::SquashMerge {
            status_snapshot_id: status.snapshot_id,
            source_ref: "refs/heads/source".into(),
        }
    } else {
        RepositoryAction::Merge {
            status_snapshot_id: status.snapshot_id,
            source_ref: "refs/heads/source".into(),
        }
    };
    let plan = engine.prepare_action(id, action).await?;
    engine.execute(&plan).await
}
async fn stage(engine: &GitEngine, id: &str) {
    let changes = engine.get_changes(id).await.unwrap();
    let plan = engine
        .prepare_action(
            id,
            RepositoryAction::Stage {
                snapshot_id: changes.snapshot_id,
                file_ids: changes.files.into_iter().map(|f| f.file_id).collect(),
            },
        )
        .await
        .unwrap();
    engine.execute(&plan).await.unwrap();
}

#[tokio::test]
async fn normal_merge_allows_ff_and_overrides_conflicting_user_defaults() {
    let dir = repo();
    git(dir.path(), &["switch", "-c", "source"]);
    write(dir.path(), "source", "source");
    commit(dir.path(), "source");
    let source = git(dir.path(), &["rev-parse", "HEAD"]);
    git(dir.path(), &["switch", "main"]);
    git(dir.path(), &["config", "merge.ff", "false"]);
    git(
        dir.path(),
        &["config", "branch.main.mergeOptions", "--squash --no-commit"],
    );
    git(dir.path(), &["config", "merge.autoStash", "true"]);
    let engine = GitEngine::isolated();
    let opened = engine.open_repository(dir.path()).await.unwrap();
    merge(&engine, &opened.repository_id, false).await.unwrap();
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), source);
    assert!(
        !engine
            .get_status(&opened.repository_id)
            .await
            .unwrap()
            .dirty
    );
    assert!(!dir.path().join(".git/MERGE_HEAD").exists());
}
#[tokio::test]
async fn divergent_normal_merge_creates_two_parents_and_runs_hook() {
    let dir = divergent(false);
    git(
        dir.path(),
        &["config", "branch.main.mergeOptions", "--ff-only"],
    );
    hook(
        &dir.path().join(".git/test-hooks/pre-merge-commit"),
        "#!/bin/sh\nprintf ran > .git/merge-hook-ran\n",
    );
    let before = git(dir.path(), &["rev-parse", "HEAD"]);
    let source = git(dir.path(), &["rev-parse", "source"]);
    let engine = GitEngine::isolated();
    let opened = engine.open_repository(dir.path()).await.unwrap();
    merge(&engine, &opened.repository_id, false).await.unwrap();
    assert_eq!(
        git(dir.path(), &["show", "-s", "--format=%P", "HEAD"]),
        format!("{before} {source}")
    );
    assert!(dir.path().join(".git/merge-hook-ran").exists());
    assert_eq!(
        engine
            .get_status(&opened.repository_id)
            .await
            .unwrap()
            .git_state,
        GitState::Idle
    );
}
#[tokio::test]
async fn ordinary_conflict_survives_restart_resolves_and_continues() {
    let dir = divergent(true);
    let engine = GitEngine::isolated();
    let opened = engine.open_repository(dir.path()).await.unwrap();
    assert!(merge(&engine, &opened.repository_id, false).await.is_err());
    let restarted = GitEngine::isolated();
    let opened = restarted.open_repository(dir.path()).await.unwrap();
    assert_eq!(opened.status.git_state, GitState::Merge);
    assert_eq!(opened.status.counts.conflicted, 1);
    assert_eq!(
        restarted
            .prepare_action(
                &opened.repository_id,
                RepositoryAction::ContinueMerge {
                    status_snapshot_id: opened.status.snapshot_id
                }
            )
            .await
            .err()
            .unwrap()
            .code,
        "unresolved_conflicts"
    );
    write(dir.path(), "a", "resolved\n");
    stage(&restarted, &opened.repository_id).await;
    let status = restarted.get_status(&opened.repository_id).await.unwrap();
    let plan = restarted
        .prepare_action(
            &opened.repository_id,
            RepositoryAction::ContinueMerge {
                status_snapshot_id: status.snapshot_id,
            },
        )
        .await
        .unwrap();
    restarted.execute(&plan).await.unwrap();
    assert_eq!(git(dir.path(), &["show", "HEAD:a"]), "resolved");
    assert_eq!(
        git(dir.path(), &["show", "-s", "--format=%P", "HEAD"])
            .split_whitespace()
            .count(),
        2
    );
    assert_eq!(
        restarted
            .get_status(&opened.repository_id)
            .await
            .unwrap()
            .git_state,
        GitState::Idle
    );
}
#[tokio::test]
async fn ordinary_conflict_abort_restores_original_state() {
    let dir = divergent(true);
    let before = git(dir.path(), &["rev-parse", "HEAD"]);
    let engine = GitEngine::isolated();
    let opened = engine.open_repository(dir.path()).await.unwrap();
    assert!(merge(&engine, &opened.repository_id, false).await.is_err());
    let status = engine.get_status(&opened.repository_id).await.unwrap();
    let plan = engine
        .prepare_action(
            &opened.repository_id,
            RepositoryAction::AbortMerge {
                status_snapshot_id: status.snapshot_id,
            },
        )
        .await
        .unwrap();
    engine.execute(&plan).await.unwrap();
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), before);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a")).unwrap(),
        "main\n"
    );
    assert!(
        !engine
            .get_status(&opened.repository_id)
            .await
            .unwrap()
            .dirty
    );
}
#[tokio::test]
async fn normal_merge_hook_failure_retains_merge_for_continue() {
    let dir = divergent(false);
    let before = git(dir.path(), &["rev-parse", "HEAD"]);
    hook(
        &dir.path().join(".git/test-hooks/pre-merge-commit"),
        "#!/bin/sh\nexit 1\n",
    );
    let engine = GitEngine::isolated();
    let opened = engine.open_repository(dir.path()).await.unwrap();
    assert!(merge(&engine, &opened.repository_id, false).await.is_err());
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), before);
    let status = engine.get_status(&opened.repository_id).await.unwrap();
    assert_eq!(status.git_state, GitState::Merge);
    assert_eq!(status.counts.conflicted, 0);
    hook(
        &dir.path().join(".git/test-hooks/pre-commit"),
        "#!/bin/sh\nexit 1\n",
    );
    let plan = engine
        .prepare_action(
            &opened.repository_id,
            RepositoryAction::ContinueMerge {
                status_snapshot_id: status.snapshot_id,
            },
        )
        .await
        .unwrap();
    assert!(engine.execute(&plan).await.is_err());
    assert!(dir.path().join(".git/MERGE_HEAD").exists());
    std::fs::remove_file(dir.path().join(".git/test-hooks/pre-commit")).unwrap();
    let status = engine.get_status(&opened.repository_id).await.unwrap();
    let plan = engine
        .prepare_action(
            &opened.repository_id,
            RepositoryAction::ContinueMerge {
                status_snapshot_id: status.snapshot_id,
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
            .git_state,
        GitState::Idle
    );
}
#[tokio::test]
async fn squash_review_is_durable_and_requires_explicit_single_parent_commit() {
    let dir = divergent(false);
    let before = git(dir.path(), &["rev-parse", "HEAD"]);
    let source = git(dir.path(), &["rev-parse", "source"]);
    git(
        dir.path(),
        &["config", "branch.main.mergeOptions", "--no-squash --commit"],
    );
    let engine = GitEngine::isolated();
    let opened = engine.open_repository(dir.path()).await.unwrap();
    merge(&engine, &opened.repository_id, true).await.unwrap();
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), before);
    assert!(!dir.path().join(".git/MERGE_HEAD").exists());
    let restarted = GitEngine::isolated();
    let opened = restarted.open_repository(dir.path()).await.unwrap();
    assert_eq!(opened.status.git_state, GitState::SquashMerge);
    assert_eq!(opened.status.counts.staged, 1);
    let journal = opened.status.squash_merge.unwrap();
    assert_eq!(journal.original_head, before);
    assert_eq!(journal.source_oid, source);
    assert_eq!(journal.source_ref, "refs/heads/source");
    let plan = restarted
        .prepare_action(
            &opened.repository_id,
            RepositoryAction::Commit {
                status_snapshot_id: opened.status.snapshot_id,
                message: "reviewed squash".into(),
            },
        )
        .await
        .unwrap();
    restarted.execute(&plan).await.unwrap();
    assert_eq!(
        git(dir.path(), &["show", "-s", "--format=%P", "HEAD"]),
        before
    );
    assert_eq!(
        git(dir.path(), &["show", "-s", "--format=%s", "HEAD"]),
        "reviewed squash"
    );
    assert_eq!(git(dir.path(), &["show", "HEAD:source"]), "source");
    let status = restarted.get_status(&opened.repository_id).await.unwrap();
    assert_eq!(status.git_state, GitState::Idle);
    assert!(status.squash_merge.is_none());
    assert!(!dir.path().join(".git/git-view-squash.json").exists());
}
#[tokio::test]
async fn squash_conflict_has_no_merge_head_or_destructive_abort() {
    let dir = divergent(true);
    let before = git(dir.path(), &["rev-parse", "HEAD"]);
    let engine = GitEngine::isolated();
    let opened = engine.open_repository(dir.path()).await.unwrap();
    assert!(merge(&engine, &opened.repository_id, true).await.is_err());
    assert!(!dir.path().join(".git/MERGE_HEAD").exists());
    let restarted = GitEngine::isolated();
    let opened = restarted.open_repository(dir.path()).await.unwrap();
    assert_eq!(opened.status.git_state, GitState::SquashMerge);
    assert_eq!(opened.status.counts.conflicted, 1);
    assert_eq!(
        restarted
            .prepare_action(
                &opened.repository_id,
                RepositoryAction::AbortMerge {
                    status_snapshot_id: opened.status.snapshot_id
                }
            )
            .await
            .err()
            .unwrap()
            .code,
        "invalid_operation"
    );
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), before);
    write(dir.path(), "a", "squash resolution\n");
    stage(&restarted, &opened.repository_id).await;
    let status = restarted.get_status(&opened.repository_id).await.unwrap();
    let plan = restarted
        .prepare_action(
            &opened.repository_id,
            RepositoryAction::Commit {
                status_snapshot_id: status.snapshot_id,
                message: "resolved squash".into(),
            },
        )
        .await
        .unwrap();
    restarted.execute(&plan).await.unwrap();
    assert_eq!(
        git(dir.path(), &["show", "-s", "--format=%P", "HEAD"]),
        before
    );
    assert_eq!(git(dir.path(), &["show", "HEAD:a"]), "squash resolution");
}
#[tokio::test]
async fn squash_manual_cleanup_only_removes_application_marker() {
    let dir = divergent(false);
    let before = git(dir.path(), &["rev-parse", "HEAD"]);
    let engine = GitEngine::isolated();
    let opened = engine.open_repository(dir.path()).await.unwrap();
    merge(&engine, &opened.repository_id, true).await.unwrap();
    git(dir.path(), &["reset", "--hard", &before]);
    let status = engine.get_status(&opened.repository_id).await.unwrap();
    assert_eq!(status.git_state, GitState::Idle);
    assert!(status.squash_merge.is_none());
    assert!(!dir.path().join(".git/git-view-squash.json").exists());
}

#[tokio::test]
async fn stale_merge_source_and_dirty_start_are_rejected_without_mutation() {
    let dir = divergent(false);
    let before = git(dir.path(), &["rev-parse", "HEAD"]);
    let engine = GitEngine::isolated();
    let opened = engine.open_repository(dir.path()).await.unwrap();
    let plan = engine
        .prepare_action(
            &opened.repository_id,
            RepositoryAction::SquashMerge {
                status_snapshot_id: opened.status.snapshot_id,
                source_ref: "refs/heads/source".into(),
            },
        )
        .await
        .unwrap();
    git(dir.path(), &["branch", "-f", "source", "HEAD"]);
    assert_eq!(
        engine.execute(&plan).await.err().unwrap().code,
        "precondition_changed"
    );
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), before);
    assert!(!dir.path().join(".git/git-view-squash.json").exists());
    write(dir.path(), "dirty", "mine");
    assert_eq!(
        merge(&engine, &opened.repository_id, false)
            .await
            .err()
            .unwrap()
            .code,
        "dirty"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("dirty")).unwrap(),
        "mine"
    );
}
#[tokio::test]
async fn squash_commit_hook_failure_retains_review_and_original_head() {
    let dir = divergent(false);
    let before = git(dir.path(), &["rev-parse", "HEAD"]);
    let engine = GitEngine::isolated();
    let opened = engine.open_repository(dir.path()).await.unwrap();
    merge(&engine, &opened.repository_id, true).await.unwrap();
    hook(
        &dir.path().join(".git/test-hooks/pre-commit"),
        "#!/bin/sh\nexit 1\n",
    );
    let status = engine.get_status(&opened.repository_id).await.unwrap();
    let plan = engine
        .prepare_action(
            &opened.repository_id,
            RepositoryAction::Commit {
                status_snapshot_id: status.snapshot_id,
                message: "retained squash draft".into(),
            },
        )
        .await
        .unwrap();
    assert!(engine.execute(&plan).await.is_err());
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), before);
    let restarted = GitEngine::isolated();
    let opened = restarted.open_repository(dir.path()).await.unwrap();
    assert_eq!(opened.status.git_state, GitState::SquashMerge);
    assert_eq!(opened.status.counts.staged, 1);
    assert_eq!(opened.status.squash_merge.unwrap().original_head, before);
}
#[tokio::test]
async fn empty_squash_result_does_not_leave_false_in_progress_state() {
    let dir = repo();
    let before = git(dir.path(), &["rev-parse", "HEAD"]);
    git(dir.path(), &["switch", "-c", "source"]);
    git(
        dir.path(),
        &["commit", "--allow-empty", "-m", "empty source"],
    );
    git(dir.path(), &["switch", "main"]);
    let engine = GitEngine::isolated();
    let opened = engine.open_repository(dir.path()).await.unwrap();
    merge(&engine, &opened.repository_id, true).await.unwrap();
    let status = engine.get_status(&opened.repository_id).await.unwrap();
    assert_eq!(status.git_state, GitState::Idle);
    assert!(!status.dirty);
    assert!(status.squash_merge.is_none());
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), before);
}
#[tokio::test]
async fn malformed_squash_journal_is_reported_and_preserved() {
    let dir = repo();
    let engine = GitEngine::isolated();
    let opened = engine.open_repository(dir.path()).await.unwrap();
    std::fs::write(dir.path().join(".git/git-view-squash.json"), "corrupt").unwrap();
    assert_eq!(
        engine
            .get_status(&opened.repository_id)
            .await
            .err()
            .unwrap()
            .code,
        "squash_journal_invalid"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join(".git/git-view-squash.json")).unwrap(),
        "corrupt"
    );
}

#[tokio::test]
async fn changing_merge_metadata_invalidates_continue_and_abort_plans() {
    for name in ["MERGE_HEAD", "MERGE_MSG", "ORIG_HEAD"] {
        let dir = divergent(false);
        let before = git(dir.path(), &["rev-parse", "HEAD"]);
        hook(
            &dir.path().join(".git/test-hooks/pre-merge-commit"),
            "#!/bin/sh\nexit 1\n",
        );
        let engine = GitEngine::isolated();
        let opened = engine.open_repository(dir.path()).await.unwrap();
        assert!(merge(&engine, &opened.repository_id, false).await.is_err());
        let status = engine.get_status(&opened.repository_id).await.unwrap();
        let action = if name == "ORIG_HEAD" {
            RepositoryAction::AbortMerge {
                status_snapshot_id: status.snapshot_id,
            }
        } else {
            RepositoryAction::ContinueMerge {
                status_snapshot_id: status.snapshot_id,
            }
        };
        let plan = engine
            .prepare_action(&opened.repository_id, action)
            .await
            .unwrap();
        let replacement = if name == "MERGE_MSG" {
            "externally replaced merge message\n".to_owned()
        } else {
            format!("{}\n", git(dir.path(), &["rev-parse", "HEAD^"]))
        };
        std::fs::write(dir.path().join(".git").join(name), &replacement).unwrap();
        assert_eq!(
            engine.execute(&plan).await.err().unwrap().code,
            "precondition_changed",
            "{name}"
        );
        assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), before);
        assert_eq!(
            std::fs::read_to_string(dir.path().join(".git").join(name)).unwrap(),
            replacement
        );
    }
}
#[tokio::test]
async fn external_squash_commit_with_later_edits_requires_manual_review() {
    let dir = divergent(false);
    let engine = GitEngine::isolated();
    let opened = engine.open_repository(dir.path()).await.unwrap();
    merge(&engine, &opened.repository_id, true).await.unwrap();
    git(dir.path(), &["commit", "-m", "completed outside Git View"]);
    let external = git(dir.path(), &["rev-parse", "HEAD"]);
    write(dir.path(), "later edits", "must remain");
    let journal_before = std::fs::read(dir.path().join(".git/git-view-squash.json")).unwrap();
    let error = engine
        .get_status(&opened.repository_id)
        .await
        .err()
        .unwrap();
    assert_eq!(error.code, "squash_manual_review");
    assert!(error.message.contains("changed externally"));
    assert_eq!(
        engine
            .prepare_action(
                &opened.repository_id,
                RepositoryAction::Fetch {
                    remote_id: "origin".into()
                }
            )
            .await
            .err()
            .unwrap()
            .code,
        "squash_manual_review"
    );
    assert_eq!(
        std::fs::read(dir.path().join(".git/git-view-squash.json")).unwrap(),
        journal_before
    );
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), external);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("later edits")).unwrap(),
        "must remain"
    );
}
#[tokio::test]
async fn external_branch_switch_cannot_reuse_an_old_squash_journal() {
    let dir = divergent(false);
    let engine = GitEngine::isolated();
    let opened = engine.open_repository(dir.path()).await.unwrap();
    merge(&engine, &opened.repository_id, true).await.unwrap();
    let before = git(dir.path(), &["rev-parse", "HEAD"]);
    git(dir.path(), &["switch", "-c", "external-branch"]);
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), before);
    assert_eq!(
        engine
            .get_status(&opened.repository_id)
            .await
            .err()
            .unwrap()
            .code,
        "squash_manual_review"
    );
    assert_eq!(
        git(dir.path(), &["diff", "--cached", "--name-only"]),
        "source"
    );
    assert!(dir.path().join(".git/git-view-squash.json").exists());
}

#[tokio::test]
async fn interrupted_clean_squash_journal_is_preserved_for_manual_review() {
    let dir = repo();
    git(dir.path(), &["switch", "-c", "source"]);
    git(
        dir.path(),
        &["commit", "--allow-empty", "-m", "empty source"],
    );
    git(dir.path(), &["switch", "main"]);
    let engine = GitEngine::isolated();
    let opened = engine.open_repository(dir.path()).await.unwrap();
    merge(&engine, &opened.repository_id, true).await.unwrap();
    let path = dir.path().join(".git/git-view-squash.json");
    let mut journal: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    journal["phase"] = "running".into();
    let bytes = serde_json::to_vec(&journal).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    let restarted = GitEngine::isolated();
    let error = restarted.open_repository(dir.path()).await.err().unwrap();
    assert_eq!(error.code, "squash_manual_review");
    assert!(error.message.contains("interrupted"));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert!(git(dir.path(), &["status", "--porcelain"]).is_empty());
}
