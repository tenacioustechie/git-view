//! Single-repository pull with rebase, merge and fast-forward-only strategies.
use git_view::{
    contracts::*,
    git::{GitEngine, PreparedAction},
};
use std::{path::Path, process::Command};
use tempfile::TempDir;

fn git(path: &Path, args: &[&str]) -> String {
    let mut command = Command::new("git");
    command
        .current_dir(path)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null");
    for key in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_CONFIG_COUNT",
        "GIT_CONFIG_PARAMETERS",
        "GIT_AUTHOR_NAME",
        "GIT_AUTHOR_EMAIL",
        "GIT_COMMITTER_NAME",
        "GIT_COMMITTER_EMAIL",
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
fn identity(path: &Path) {
    git(path, &["config", "user.name", "Fixture"]);
    git(path, &["config", "user.email", "fixture@example.invalid"]);
    git(path, &["config", "commit.gpgsign", "false"]);
    git(path, &["config", "core.hooksPath", ".disabled-hooks"]);
}
fn write(path: &Path, name: &str, body: &str) {
    if let Some(parent) = path.join(name).parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path.join(name), body).unwrap();
}
fn commit(path: &Path, message: &str) {
    git(path, &["add", "--all"]);
    git(path, &["commit", "-m", message]);
}
fn head(path: &Path) -> String {
    git(path, &["rev-parse", "HEAD"])
}

struct Fixture {
    source: TempDir,
    _bare: TempDir,
    clone: TempDir,
    engine: GitEngine,
    id: String,
}

/// `source` pushes to `bare`; `clone` tracks `origin/main`. The clone has not
/// fetched the commits `source` pushes afterwards, so a pull must fetch first.
async fn fixture() -> Fixture {
    let source = tempfile::tempdir().unwrap();
    git(source.path(), &["init", "-b", "main"]);
    identity(source.path());
    write(source.path(), "a", "one\n");
    write(source.path(), ".gitignore", "build/\n");
    commit(source.path(), "one");
    let bare = tempfile::tempdir().unwrap();
    git(bare.path(), &["init", "--bare", "-b", "main"]);
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
    identity(clone.path());
    // User pull settings must not change the app's chosen strategy.
    git(clone.path(), &["config", "pull.rebase", "true"]);
    git(clone.path(), &["config", "pull.ff", "only"]);
    git(clone.path(), &["config", "rebase.autoStash", "true"]);
    git(
        clone.path(),
        &["config", "branch.main.mergeOptions", "--squash"],
    );
    let engine = GitEngine::isolated();
    let id = engine
        .open_repository(clone.path())
        .await
        .unwrap()
        .repository_id;
    Fixture {
        source,
        _bare: bare,
        clone,
        engine,
        id,
    }
}
fn upstream_commit(f: &Fixture, name: &str, body: &str) {
    write(f.source.path(), name, body);
    commit(f.source.path(), &format!("upstream {name}"));
    git(f.source.path(), &["push"]);
}
fn local_commit(f: &Fixture, name: &str, body: &str) {
    write(f.clone.path(), name, body);
    commit(f.clone.path(), &format!("local {name}"));
}
async fn plan(f: &Fixture, mode: PullMode) -> AppResult<PreparedAction> {
    let status = f.engine.get_status(&f.id).await.unwrap();
    f.engine
        .prepare_action(
            &f.id,
            RepositoryAction::Pull {
                status_snapshot_id: status.snapshot_id,
                mode: Some(mode),
            },
        )
        .await
}
async fn control(f: &Fixture, make: impl Fn(String) -> RepositoryAction) -> AppResult<()> {
    let status = f.engine.get_status(&f.id).await.unwrap();
    let plan = f
        .engine
        .prepare_action(&f.id, make(status.snapshot_id))
        .await?;
    f.engine.execute(&plan).await.map(|_| ())
}

#[tokio::test]
async fn behind_only_fast_forwards_in_every_mode_after_fetching_in_preview() {
    for mode in [PullMode::Rebase, PullMode::Merge, PullMode::FastForward] {
        let f = fixture().await;
        upstream_commit(&f, "incoming", "remote\n");
        write(f.clone.path(), "notes.txt", "untracked\n");
        write(f.clone.path(), "build/out", "ignored\n");
        let plan = plan(&f, mode).await.unwrap();
        let preview = plan.pull_preview().unwrap();
        assert_eq!(preview.mode, mode);
        assert_eq!(preview.strategy, PullMode::FastForward);
        assert_eq!((preview.incoming, preview.local), (1, 0));
        assert_eq!(preview.upstream_ref, "refs/remotes/origin/main");
        assert_eq!(preview.upstream_oid, head(f.source.path()));
        f.engine.execute(&plan).await.unwrap();
        assert_eq!(head(f.clone.path()), head(f.source.path()), "{mode:?}");
        assert_eq!(
            std::fs::read_to_string(f.clone.path().join("notes.txt")).unwrap(),
            "untracked\n"
        );
        assert!(f.clone.path().join("build/out").exists());
    }
}

#[tokio::test]
async fn nothing_to_pull_and_fast_forward_only_divergence_are_blocked() {
    let f = fixture().await;
    assert_eq!(
        plan(&f, PullMode::Rebase).await.err().unwrap().code,
        "already_up_to_date"
    );
    local_commit(&f, "mine", "local\n");
    assert_eq!(
        plan(&f, PullMode::Merge).await.err().unwrap().code,
        "already_up_to_date"
    );
    upstream_commit(&f, "incoming", "remote\n");
    let error = plan(&f, PullMode::FastForward).await.err().unwrap();
    assert_eq!(error.code, "diverged");
    assert!(
        error.message.contains("Pull with rebase"),
        "{}",
        error.message
    );
}

#[tokio::test]
async fn diverged_rebase_replays_local_commits_onto_fetched_upstream() {
    let f = fixture().await;
    local_commit(&f, "mine", "local\n");
    let original = head(f.clone.path());
    upstream_commit(&f, "incoming", "remote\n");
    // Neither an unrelated untracked file nor ignored build output blocks pull.
    write(f.clone.path(), "notes.txt", "untracked\n");
    write(f.clone.path(), "build/out", "ignored\n");
    let plan = plan(&f, PullMode::Rebase).await.unwrap();
    let preview = plan.pull_preview().unwrap();
    assert_eq!(preview.strategy, PullMode::Rebase);
    assert_eq!((preview.incoming, preview.local), (1, 1));
    assert!(
        plan.summary.starts_with("Pull with rebase"),
        "{}",
        plan.summary
    );
    f.engine.execute(&plan).await.unwrap();
    let path = f.clone.path();
    assert_eq!(git(path, &["rev-parse", "HEAD^"]), head(f.source.path()));
    assert_eq!(git(path, &["log", "-1", "--format=%s"]), "local mine");
    assert_eq!(git(path, &["rev-list", "--merges", "--count", "HEAD"]), "0");
    assert!(git(path, &["for-each-ref", "refs/git-view/rebase-backups/"]).contains(&original));
    assert!(!path.join(".git/git-view-rebase.json").exists());
    assert_eq!(
        std::fs::read_to_string(path.join("notes.txt")).unwrap(),
        "untracked\n"
    );
    assert!(path.join("build/out").exists());
}

#[tokio::test]
async fn diverged_merge_creates_merge_commit_despite_user_pull_settings() {
    let f = fixture().await;
    local_commit(&f, "mine", "local\n");
    let original = head(f.clone.path());
    upstream_commit(&f, "incoming", "remote\n");
    let plan = plan(&f, PullMode::Merge).await.unwrap();
    assert_eq!(plan.pull_preview().unwrap().strategy, PullMode::Merge);
    f.engine.execute(&plan).await.unwrap();
    let path = f.clone.path();
    assert_eq!(
        git(path, &["log", "-1", "--format=%P"]),
        format!("{original} {}", head(f.source.path()))
    );
    assert_eq!(
        git(path, &["log", "-1", "--format=%s"]),
        "Merge remote-tracking branch 'origin/main'"
    );
    assert!(path.join("incoming").exists() && path.join("mine").exists());
}

#[tokio::test]
async fn rebase_conflict_stops_and_continue_finishes_with_ignored_files_present() {
    let f = fixture().await;
    write(f.clone.path(), "a", "local\n");
    commit(f.clone.path(), "local a");
    upstream_commit(&f, "a", "remote\n");
    write(f.clone.path(), "build/out", "ignored\n");
    let plan = plan(&f, PullMode::Rebase).await.unwrap();
    let error = f.engine.execute(&plan).await.err().unwrap();
    assert_eq!(error.code, "rebase_stopped", "{}", error.message);
    let status = f.engine.get_status(&f.id).await.unwrap();
    assert_eq!(status.git_state, GitState::Rebase);
    write(f.clone.path(), "a", "resolved\n");
    git(f.clone.path(), &["add", "a"]);
    control(&f, |status_snapshot_id| RepositoryAction::ContinueRebase {
        status_snapshot_id,
    })
    .await
    .unwrap();
    let path = f.clone.path();
    assert_eq!(git(path, &["rev-parse", "HEAD^"]), head(f.source.path()));
    assert_eq!(
        std::fs::read_to_string(path.join("a")).unwrap(),
        "resolved\n"
    );
    assert!(!path.join(".git/git-view-rebase.json").exists());
}

#[tokio::test]
async fn rebase_conflict_abort_restores_original_branch() {
    let f = fixture().await;
    write(f.clone.path(), "a", "local\n");
    commit(f.clone.path(), "local a");
    let original = head(f.clone.path());
    upstream_commit(&f, "a", "remote\n");
    let plan = plan(&f, PullMode::Rebase).await.unwrap();
    assert_eq!(
        f.engine.execute(&plan).await.err().unwrap().code,
        "rebase_stopped"
    );
    control(&f, |status_snapshot_id| RepositoryAction::AbortRebase {
        status_snapshot_id,
    })
    .await
    .unwrap();
    assert_eq!(head(f.clone.path()), original);
    assert_eq!(
        f.engine.get_status(&f.id).await.unwrap().git_state,
        GitState::Idle
    );
}

#[tokio::test]
async fn merge_conflict_stops_and_abort_restores_original_branch() {
    let f = fixture().await;
    write(f.clone.path(), "a", "local\n");
    commit(f.clone.path(), "local a");
    let original = head(f.clone.path());
    upstream_commit(&f, "a", "remote\n");
    let plan = plan(&f, PullMode::Merge).await.unwrap();
    let error = f.engine.execute(&plan).await.err().unwrap();
    assert_eq!(error.code, "merge_stopped", "{}", error.message);
    let status = f.engine.get_status(&f.id).await.unwrap();
    assert_eq!(status.git_state, GitState::Merge);
    assert_eq!(status.counts.conflicted, 1);
    control(&f, |status_snapshot_id| RepositoryAction::AbortMerge {
        status_snapshot_id,
    })
    .await
    .unwrap();
    assert_eq!(head(f.clone.path()), original);
}

#[tokio::test]
async fn untracked_or_ignored_file_at_incoming_path_is_protected() {
    for (name, body) in [("incoming", "mine\n"), ("build/incoming", "ignored\n")] {
        let f = fixture().await;
        // Force-add so the upstream tracks a path that is ignored locally.
        write(f.source.path(), name, "remote\n");
        git(f.source.path(), &["add", "--force", name]);
        git(f.source.path(), &["commit", "-m", "upstream"]);
        git(f.source.path(), &["push"]);
        write(f.clone.path(), name, body);
        for mode in [PullMode::FastForward, PullMode::Rebase] {
            let error = plan(&f, mode).await.err().unwrap();
            assert_eq!(
                error.code, "local_path_collision",
                "{name}: {}",
                error.message
            );
        }
        assert_eq!(
            std::fs::read_to_string(f.clone.path().join(name)).unwrap(),
            body
        );
    }
}

#[tokio::test]
async fn tracked_changes_block_and_upstream_moving_after_preview_is_rejected() {
    let f = fixture().await;
    upstream_commit(&f, "incoming", "remote\n");
    write(f.clone.path(), "a", "edited\n");
    assert_eq!(
        plan(&f, PullMode::Rebase).await.err().unwrap().code,
        "dirty"
    );
    git(f.clone.path(), &["checkout", "--", "a"]);
    let plan = plan(&f, PullMode::Rebase).await.unwrap();
    let reviewed = plan.pull_preview().unwrap().upstream_oid;
    upstream_commit(&f, "later", "remote\n");
    git(f.clone.path(), &["fetch"]);
    assert_eq!(
        f.engine.execute(&plan).await.err().unwrap().code,
        "precondition_changed"
    );
    assert_ne!(head(f.clone.path()), reviewed);
}
