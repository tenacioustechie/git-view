use super::*;
use std::{path::Path, process::Command};
use tempfile::TempDir;
fn git(path: &Path, args: &[&str]) -> String {
    let mut command = Command::new("git");
    let null = "/dev/null";
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
fn suffix(path: &Path) -> Vec<String> {
    git(path, &["rev-list", "--max-count=2", "HEAD"])
        .lines()
        .map(str::to_owned)
        .collect()
}
fn linear() -> TempDir {
    let dir = repo();
    write(dir.path(), "a", "second\n");
    commit(dir.path(), "second");
    write(dir.path(), "a", "third\n");
    commit(dir.path(), "third");
    dir
}
async fn combine(engine: &GitEngine, id: &str, oids: Vec<String>) -> AppResult<ActionOutcome> {
    let status = engine.get_status(id).await?;
    let plan = engine
        .prepare_action(
            id,
            RepositoryAction::CombineCommits {
                status_snapshot_id: status.snapshot_id,
                commit_oids: oids,
                message: "Combined message\n\nDetails".into(),
            },
        )
        .await?;
    engine.execute(&plan).await
}
async fn recover(engine: &GitEngine, id: &str, recovery_id: String) -> AppResult<ActionOutcome> {
    let status = engine.get_status(id).await?;
    let plan = engine
        .prepare_action(
            id,
            RepositoryAction::RecoverCombine {
                status_snapshot_id: status.snapshot_id,
                recovery_id,
            },
        )
        .await?;
    engine.execute(&plan).await
}
#[tokio::test]
async fn every_durable_phase_and_side_effect_reconciles_without_retrying_commit() {
    for point in [
        "prepared",
        "backupEffect",
        "backupCreated",
        "worktreeEffect",
        "worktreeCreated",
        "commitEffect",
        "candidateCommitted",
        "replacementEffect",
        "candidateReady",
        "installing",
        "installEffect",
        "installed",
    ] {
        let dir = linear();
        let path = dir.path();
        let old = git(path, &["rev-parse", "HEAD"]);
        let index = std::fs::read(path.join(".git/index")).unwrap();
        let engine = GitEngine::isolated();
        let id = engine.open_repository(path).await.unwrap().repository_id;
        *engine.rewrite_failure.lock().unwrap() = Some(point.into());
        let error = combine(&engine, &id, suffix(path)).await.err().unwrap();
        assert!(
            ["simulated_interruption", "outcome_uncertain"].contains(&error.code.as_str()),
            "{point}: {error:?}"
        );
        drop(engine);
        let restarted = GitEngine::isolated();
        let id = restarted.open_repository(path).await.unwrap().repository_id;
        let record = restarted.list_rewrites(&id).await.unwrap().remove(0);
        let installed = ["installEffect", "installed"].contains(&point);
        assert_eq!(
            record.phase,
            if installed {
                "installed"
            } else if point == "candidateCommitted" {
                "manualReview"
            } else {
                "notInstalled"
            },
            "{point}"
        );
        if installed {
            assert_eq!(
                Some(git(path, &["rev-parse", "HEAD"])),
                record.replacement_head
            );
            assert!(record.can_recover);
        } else {
            assert_eq!(git(path, &["rev-parse", "HEAD"]), old);
            assert!(!record.can_recover);
        }
        assert_eq!(
            std::fs::read(path.join(".git/index")).unwrap(),
            index,
            "{point}"
        );
        if point != "prepared" {
            assert_eq!(git(path, &["rev-parse", &record.backup_ref]), old);
        }
    }
}
#[tokio::test]
async fn recovery_interruptions_reconcile_actual_refs_and_keep_dirty_bytes() {
    for point in ["recovering", "recoverEffect", "recovered"] {
        let dir = linear();
        let path = dir.path();
        let old = git(path, &["rev-parse", "HEAD"]);
        let engine = GitEngine::isolated();
        let id = engine.open_repository(path).await.unwrap().repository_id;
        combine(&engine, &id, suffix(path)).await.unwrap();
        let record = engine.list_rewrites(&id).await.unwrap().remove(0);
        let replacement = record.replacement_head.unwrap();
        write(path, "a", "staged");
        git(path, &["add", "a"]);
        write(path, "a", "working");
        write(path, "[literal]", "untracked");
        let index = std::fs::read(path.join(".git/index")).unwrap();
        *engine.rewrite_failure.lock().unwrap() = Some(point.into());
        assert!(recover(&engine, &id, record.recovery_id).await.is_err());
        drop(engine);
        let restarted = GitEngine::isolated();
        let id = restarted.open_repository(path).await.unwrap().repository_id;
        let record = restarted.list_rewrites(&id).await.unwrap().remove(0);
        assert_eq!(
            record.phase,
            if point == "recovering" {
                "installed"
            } else {
                "recovered"
            }
        );
        assert_eq!(
            git(path, &["rev-parse", "HEAD"]),
            if point == "recovering" {
                replacement
            } else {
                old
            }
        );
        assert_eq!(std::fs::read(path.join(".git/index")).unwrap(), index);
        assert_eq!(std::fs::read_to_string(path.join("a")).unwrap(), "working");
        assert_eq!(
            std::fs::read_to_string(path.join("[literal]")).unwrap(),
            "untracked"
        );
    }
}

#[tokio::test]
async fn backup_collision_never_reuses_an_existing_reference() {
    let dir = linear();
    let path = dir.path();
    let engine = GitEngine::isolated();
    let id = engine.open_repository(path).await.unwrap().repository_id;
    let status = engine.get_status(&id).await.unwrap();
    let plan = engine
        .prepare_action(
            &id,
            RepositoryAction::CombineCommits {
                status_snapshot_id: status.snapshot_id,
                commit_oids: suffix(path),
                message: "Combined".into(),
            },
        )
        .await
        .unwrap();
    let Some(rewrite::PreparedRewrite::Combine { journal, .. }) = &plan.rewrite else {
        panic!("missing combine plan")
    };
    git(
        path,
        &[
            "update-ref",
            &journal.record.backup_ref,
            &journal.record.base_oid,
        ],
    );
    let old = git(path, &["rev-parse", "HEAD"]);
    assert!(engine.execute(&plan).await.is_err());
    assert_eq!(git(path, &["rev-parse", "HEAD"]), old);
    assert_eq!(
        git(path, &["rev-parse", &journal.record.backup_ref]),
        journal.record.base_oid
    );
    assert_eq!(
        engine.list_rewrites(&id).await.unwrap()[0].phase,
        "manualReview"
    );
}

#[test]
fn rewrite_pipeline_works_with_windows_sized_thread_stack() {
    std::thread::Builder::new()
        .stack_size(1024 * 1024)
        .spawn(recovery_interruptions_reconcile_actual_refs_and_keep_dirty_bytes)
        .unwrap()
        .join()
        .unwrap();
}
