use git_view::{contracts::*, git::GitEngine};
use std::{path::Path, process::Command};
use tempfile::TempDir;

fn git(path: &Path, args: &[&str]) -> String {
    let null = "/dev/null";
    let output = Command::new("git")
        .current_dir(path)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", null)
        .env("GIT_CONFIG_SYSTEM", null)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().into()
}
fn repo(conflict: bool) -> (TempDir, String, String) {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    git(p, &["init", "-b", "main"]);
    git(p, &["config", "user.name", "Fixture"]);
    git(p, &["config", "user.email", "fixture@example.invalid"]);
    git(p, &["config", "commit.gpgsign", "false"]);
    git(p, &["config", "core.hooksPath", ".disabled-hooks"]);
    std::fs::write(p.join("file"), "base\n").unwrap();
    git(p, &["add", "--all"]);
    git(p, &["commit", "-m", "base"]);
    git(p, &["switch", "-c", "feature"]);
    std::fs::write(
        p.join(if conflict { "file" } else { "feature-one" }),
        "feature\n",
    )
    .unwrap();
    git(p, &["add", "--all"]);
    git(p, &["commit", "-m", "feature one"]);
    if !conflict {
        std::fs::write(p.join("feature-two"), "second\n").unwrap();
        git(p, &["add", "--all"]);
        git(p, &["commit", "-m", "feature two"]);
    }
    let source = git(p, &["rev-parse", "HEAD"]);
    git(p, &["switch", "main"]);
    std::fs::write(p.join(if conflict { "file" } else { "target" }), "target\n").unwrap();
    git(p, &["add", "--all"]);
    git(p, &["commit", "-m", "target"]);
    let target = git(p, &["rev-parse", "HEAD"]);
    git(p, &["switch", "feature"]);
    (dir, source, target)
}
async fn plan(
    engine: &GitEngine,
    id: &str,
    step: &str,
) -> AppResult<git_view::git::PreparedAction> {
    let status = engine.get_status(id).await?;
    let action = match step {
        "start" => RepositoryAction::StartRebase {
            status_snapshot_id: status.snapshot_id,
            onto_ref: "refs/heads/main".into(),
        },
        "continue" => RepositoryAction::ContinueRebase {
            status_snapshot_id: status.snapshot_id,
        },
        "skip" => RepositoryAction::SkipRebase {
            status_snapshot_id: status.snapshot_id,
        },
        "abort" => RepositoryAction::AbortRebase {
            status_snapshot_id: status.snapshot_id,
        },
        _ => unreachable!(),
    };
    engine.prepare_action(id, action).await
}
#[tokio::test]
async fn clean_replay_keeps_target_and_backup_and_reparents_two_commits() {
    let (dir, original, target) = repo(false);
    let p = dir.path();
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    let prepared = plan(&engine, &id, "start").await.unwrap();
    assert!(prepared.summary.contains("2 linear commits"));
    engine.execute(&prepared).await.unwrap();
    assert_eq!(git(p, &["branch", "--show-current"]), "feature");
    assert_eq!(git(p, &["rev-parse", "main"]), target);
    assert_eq!(git(p, &["rev-parse", "HEAD~2"]), target);
    assert_ne!(git(p, &["rev-parse", "HEAD"]), original);
    assert_eq!(
        git(
            p,
            &[
                "for-each-ref",
                "--format=%(objectname)",
                "refs/git-view/rebase-backups/"
            ]
        ),
        original
    );
    assert!(!p.join(".git/git-view-rebase.json").exists());
    assert_eq!(
        engine.get_status(&id).await.unwrap().git_state,
        GitState::Idle
    );
}
#[tokio::test]
async fn conflict_can_be_staged_and_continued_after_engine_reload() {
    let (dir, _, target) = repo(true);
    let p = dir.path();
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    let stopped = engine
        .execute(&plan(&engine, &id, "start").await.unwrap())
        .await
        .err()
        .unwrap();
    assert_eq!(stopped.code, "rebase_stopped");
    let reloaded = GitEngine::isolated();
    let id = reloaded.open_repository(p).await.unwrap().repository_id;
    let status = reloaded.get_status(&id).await.unwrap();
    assert_eq!(status.git_state, GitState::Rebase);
    assert_eq!(
        status.operation_context.as_ref().unwrap().stop_reason,
        OperationStopReason::Conflicts
    );
    assert!(status.operation_context.as_ref().unwrap().can_skip);
    std::fs::write(p.join("file"), "resolved\n").unwrap();
    let changes = reloaded.get_changes(&id).await.unwrap();
    let action = RepositoryAction::Stage {
        snapshot_id: changes.snapshot_id,
        file_ids: changes
            .files
            .iter()
            .filter(|f| f.conflicted)
            .map(|f| f.file_id.clone())
            .collect(),
    };
    reloaded
        .execute(&reloaded.prepare_action(&id, action).await.unwrap())
        .await
        .unwrap();
    assert!(
        reloaded
            .get_status(&id)
            .await
            .unwrap()
            .operation_context
            .unwrap()
            .can_continue
    );
    reloaded
        .execute(&plan(&reloaded, &id, "continue").await.unwrap())
        .await
        .unwrap();
    assert_eq!(git(p, &["branch", "--show-current"]), "feature");
    assert_eq!(git(p, &["rev-parse", "HEAD^"]), target);
    assert_eq!(
        std::fs::read_to_string(p.join("file")).unwrap(),
        "resolved\n"
    );
}
#[tokio::test]
async fn reviewed_skip_omits_conflicting_commit_and_abort_restores_original() {
    for step in ["skip", "abort"] {
        let (dir, original, target) = repo(true);
        let p = dir.path();
        let engine = GitEngine::isolated();
        let id = engine.open_repository(p).await.unwrap().repository_id;
        engine
            .execute(&plan(&engine, &id, "start").await.unwrap())
            .await
            .err()
            .unwrap();
        std::fs::write(p.join("file"), "resolution edit\n").unwrap();
        engine
            .execute(&plan(&engine, &id, step).await.unwrap())
            .await
            .unwrap();
        assert_eq!(git(p, &["branch", "--show-current"]), "feature");
        assert_eq!(
            git(p, &["rev-parse", "HEAD"]),
            if step == "skip" { target } else { original }
        );
        assert_eq!(
            std::fs::read_to_string(p.join("file")).unwrap(),
            if step == "skip" {
                "target\n"
            } else {
                "feature\n"
            }
        );
        assert!(!p.join(".git/git-view-rebase.json").exists());
    }
}

#[tokio::test]
async fn moved_target_and_dirty_source_are_rejected_before_replay() {
    let (dir, original, _) = repo(false);
    let p = dir.path();
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    let prepared = plan(&engine, &id, "start").await.unwrap();
    git(p, &["branch", "-f", "main", &original]);
    assert!(engine.execute(&prepared).await.is_err());
    assert_eq!(git(p, &["rev-parse", "HEAD"]), original);
    assert!(!p.join(".git/git-view-rebase.json").exists());
    std::fs::write(p.join("untracked"), "keep\n").unwrap();
    assert!(plan(&engine, &id, "start").await.is_err());
    assert_eq!(
        std::fs::read_to_string(p.join("untracked")).unwrap(),
        "keep\n"
    );
}

#[tokio::test]
async fn merge_commit_in_source_range_is_rejected() {
    let (dir, original, _) = repo(false);
    let p = dir.path();
    git(p, &["switch", "-c", "side", &original]);
    std::fs::write(p.join("side"), "side\n").unwrap();
    git(p, &["add", "--all"]);
    git(p, &["commit", "-m", "side"]);
    git(p, &["switch", "feature"]);
    std::fs::write(p.join("more"), "more\n").unwrap();
    git(p, &["add", "--all"]);
    git(p, &["commit", "-m", "more"]);
    git(p, &["merge", "--no-ff", "side", "-m", "merge side"]);
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    let result = plan(&engine, &id, "start").await;
    assert!(result.is_err());
    assert!(result.err().unwrap().message.contains("linear commits"));
}

#[cfg(unix)]
#[tokio::test]
async fn pre_rebase_hook_rejection_preserves_original_and_clears_app_record() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, original, _) = repo(false);
    let p = dir.path();
    let hooks = p.join(".git/test-hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    let hook = hooks.join("pre-rebase");
    std::fs::write(&hook, "#!/bin/sh\necho blocked >&2\nexit 1\n").unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    git(p, &["config", "core.hooksPath", ".git/test-hooks"]);
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    let failure = engine
        .execute(&plan(&engine, &id, "start").await.unwrap())
        .await
        .err()
        .unwrap();
    assert_ne!(failure.code, "outcome_uncertain");
    assert_eq!(git(p, &["rev-parse", "HEAD"]), original);
    assert_eq!(
        engine.get_status(&id).await.unwrap().git_state,
        GitState::Idle
    );
    assert!(!p.join(".git/git-view-rebase.json").exists());
}

#[tokio::test]
async fn externally_changed_rebase_metadata_disables_app_controls() {
    let (dir, _, _) = repo(true);
    let p = dir.path();
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    engine
        .execute(&plan(&engine, &id, "start").await.unwrap())
        .await
        .err()
        .unwrap();
    std::fs::write(
        p.join(".git/rebase-merge/onto"),
        format!("{}\n", "f".repeat(40)),
    )
    .unwrap();
    let status = engine.get_status(&id).await.unwrap();
    let context = status.operation_context.unwrap();
    assert_eq!(context.stop_reason, OperationStopReason::Unknown);
    assert!(!context.can_continue && !context.can_skip && !context.can_abort);
    assert!(plan(&engine, &id, "abort").await.is_err());
}

#[tokio::test]
async fn commit_that_becomes_empty_requires_reviewed_skip() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    git(p, &["init", "-b", "main"]);
    git(p, &["config", "user.name", "Fixture"]);
    git(p, &["config", "user.email", "fixture@example.invalid"]);
    git(p, &["config", "commit.gpgsign", "false"]);
    std::fs::write(p.join("base"), "base\n").unwrap();
    git(p, &["add", "--all"]);
    git(p, &["commit", "-m", "base"]);
    git(p, &["switch", "-c", "feature"]);
    std::fs::write(p.join("duplicate"), "same\n").unwrap();
    git(p, &["add", "--all"]);
    git(p, &["commit", "-m", "feature adds duplicate"]);
    git(p, &["switch", "main"]);
    std::fs::write(p.join("duplicate"), "same\n").unwrap();
    git(p, &["add", "--all"]);
    git(p, &["commit", "-m", "main adds duplicate"]);
    let target = git(p, &["rev-parse", "HEAD"]);
    git(p, &["switch", "feature"]);
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    let result = engine
        .execute(&plan(&engine, &id, "start").await.unwrap())
        .await
        .err()
        .unwrap();
    assert_eq!(result.code, "rebase_stopped");
    let context = engine
        .get_status(&id)
        .await
        .unwrap()
        .operation_context
        .unwrap();
    assert_eq!(context.stop_reason, OperationStopReason::Empty);
    assert!(!context.can_continue && context.can_skip);
    engine
        .execute(&plan(&engine, &id, "skip").await.unwrap())
        .await
        .unwrap();
    assert_eq!(git(p, &["rev-parse", "HEAD"]), target);
}
