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
fn tracking(path: &Path, remote: &str, source: &str, oid: &str, spec: &str) {
    git(
        path,
        &["config", &format!("remote.{remote}.url"), "unused.invalid"],
    );
    git(
        path,
        &["config", "--add", &format!("remote.{remote}.fetch"), spec],
    );
    git(path, &["update-ref", source, oid]);
}
async fn plan(
    engine: &GitEngine,
    id: &str,
    remote: &str,
    source: &str,
    name: &str,
) -> AppResult<git_view::git::PreparedAction> {
    let status = engine.get_status(id).await?;
    engine
        .prepare_action(
            id,
            RepositoryAction::CheckoutRemoteBranch {
                status_snapshot_id: status.snapshot_id,
                remote_id: remote.into(),
                source_ref: source.into(),
                name: name.into(),
            },
        )
        .await
}
fn fixture() -> (TempDir, String) {
    let repo = repo();
    std::fs::write(repo.path().join("file"), "original").unwrap();
    let oid = commit(repo.path(), "initial");
    tracking(
        repo.path(),
        "team/remote",
        "refs/cache/team/feature/name",
        &oid,
        "+refs/heads/*:refs/cache/team/*",
    );
    (repo, oid)
}
#[tokio::test]
async fn custom_mapping_slashes_explicit_tracking_and_clean_checkout() {
    let (repo, oid) = fixture();
    git(repo.path(), &["config", "branch.autoSetupMerge", "false"]);
    git(repo.path(), &["config", "branch.autoSetupRebase", "always"]);
    let engine = GitEngine::isolated();
    let opened = engine.open_repository(repo.path()).await.unwrap();
    let id = &opened.repository_id;
    let listing = engine.list_remote_branches(id).await.unwrap();
    assert_eq!(listing.targets.len(), 1);
    let target = &listing.targets[0];
    assert_eq!(target.source_oid, oid);
    assert_eq!(target.remote_name, "team/remote");
    assert_eq!(target.upstream_ref, "refs/heads/feature/name");
    assert_eq!(target.suggested_name, "feature/name");
    let prepared = plan(
        &engine,
        id,
        "team/remote",
        &target.source_ref,
        "local/custom",
    )
    .await
    .unwrap();
    assert!(prepared.summary.contains(&oid));
    engine.execute(&prepared).await.unwrap();
    assert_eq!(
        git(repo.path(), &["symbolic-ref", "HEAD"]),
        "refs/heads/local/custom"
    );
    assert_eq!(
        git(repo.path(), &["config", "branch.local/custom.remote"]),
        "team/remote"
    );
    assert_eq!(
        git(repo.path(), &["config", "branch.local/custom.merge"]),
        "refs/heads/feature/name"
    );
    assert_eq!(git(repo.path(), &["rev-parse", "@{upstream}"]), oid);
}
#[tokio::test]
async fn mappings_exclude_symbolic_head_negative_and_ambiguous_ownership() {
    let (repo, oid) = fixture();
    git(
        repo.path(),
        &[
            "symbolic-ref",
            "refs/cache/team/HEAD",
            "refs/cache/team/feature/name",
        ],
    );
    tracking(
        repo.path(),
        "other",
        "refs/remotes/other/feature/name",
        &oid,
        "+refs/heads/*:refs/remotes/other/*",
    );
    let engine = GitEngine::isolated();
    let id = engine
        .open_repository(repo.path())
        .await
        .unwrap()
        .repository_id;
    assert_eq!(
        engine
            .list_remote_branches(&id)
            .await
            .unwrap()
            .targets
            .len(),
        2
    );
    git(
        repo.path(),
        &[
            "config",
            "--add",
            "remote.other.fetch",
            "+refs/heads/*:refs/cache/team/*",
        ],
    );
    let ambiguous = engine.list_remote_branches(&id).await.unwrap();
    assert!(ambiguous.targets.is_empty());
    assert!(ambiguous
        .issues
        .iter()
        .any(|issue| issue.source_ref == "refs/cache/team/feature/name"));
    git(
        repo.path(),
        &[
            "config",
            "--add",
            "remote.other.fetch",
            "^refs/heads/feature/name",
        ],
    );
    let excluded = engine.list_remote_branches(&id).await.unwrap();
    assert_eq!(excluded.targets.len(), 1);
    assert_eq!(excluded.targets[0].remote_id, "team/remote");
}
#[tokio::test]
async fn dirty_names_collisions_and_orphan_config_are_rejected() {
    let (repo, _) = fixture();
    let engine = GitEngine::isolated();
    let id = engine
        .open_repository(repo.path())
        .await
        .unwrap()
        .repository_id;
    for name in ["main", "main/child", "-bad", "@{-1}"] {
        assert!(
            plan(
                &engine,
                &id,
                "team/remote",
                "refs/cache/team/feature/name",
                name
            )
            .await
            .is_err(),
            "{name}"
        );
    }
    git(repo.path(), &["config", "branch.orphan.remote", "retained"]);
    assert_eq!(
        plan(
            &engine,
            &id,
            "team/remote",
            "refs/cache/team/feature/name",
            "orphan"
        )
        .await
        .err()
        .unwrap()
        .code,
        "branch_exists"
    );
    std::fs::write(repo.path().join("untracked"), "retain").unwrap();
    assert_eq!(
        plan(
            &engine,
            &id,
            "team/remote",
            "refs/cache/team/feature/name",
            "new"
        )
        .await
        .err()
        .unwrap()
        .code,
        "dirty"
    );
    assert_eq!(
        git(repo.path(), &["config", "branch.orphan.remote"]),
        "retained"
    );
}
#[tokio::test]
async fn stale_ref_config_and_symbolic_swap_cannot_execute() {
    for drift in ["oid", "config", "symref", "collision"] {
        let (repo, oid) = fixture();
        let engine = GitEngine::isolated();
        let id = engine
            .open_repository(repo.path())
            .await
            .unwrap()
            .repository_id;
        let prepared = plan(
            &engine,
            &id,
            "team/remote",
            "refs/cache/team/feature/name",
            "new",
        )
        .await
        .unwrap();
        match drift {
            "oid" => {
                let next = commit(repo.path(), "next");
                git(
                    repo.path(),
                    &["update-ref", "refs/cache/team/feature/name", &next],
                );
            }
            "config" => {
                git(
                    repo.path(),
                    &[
                        "config",
                        "remote.team/remote.fetch",
                        "+refs/heads/*:refs/else/*",
                    ],
                );
            }
            "symref" => {
                git(
                    repo.path(),
                    &[
                        "symbolic-ref",
                        "refs/cache/team/feature/name",
                        "refs/heads/main",
                    ],
                );
            }
            "collision" => {
                git(repo.path(), &["branch", "new", &oid]);
            }
            _ => unreachable!(),
        }
        assert!(engine.execute(&prepared).await.is_err(), "{drift}");
        assert_eq!(
            git(repo.path(), &["symbolic-ref", "HEAD"]),
            "refs/heads/main"
        );
    }
}
#[tokio::test]
async fn ignored_collision_preserves_original_head_and_bytes() {
    let (repo, original) = fixture();
    git(repo.path(), &["switch", "-c", "source"]);
    std::fs::write(repo.path().join("secret"), "remote").unwrap();
    let remote = commit(repo.path(), "remote secret");
    git(repo.path(), &["switch", "main"]);
    git(
        repo.path(),
        &["update-ref", "refs/cache/team/feature/name", &remote],
    );
    std::fs::write(repo.path().join(".git/info/exclude"), "secret\n").unwrap();
    std::fs::write(repo.path().join("secret"), "local ignored bytes").unwrap();
    let engine = GitEngine::isolated();
    let id = engine
        .open_repository(repo.path())
        .await
        .unwrap()
        .repository_id;
    let prepared = plan(
        &engine,
        &id,
        "team/remote",
        "refs/cache/team/feature/name",
        "new",
    )
    .await
    .unwrap();
    assert!(engine.execute(&prepared).await.is_err());
    assert_eq!(git(repo.path(), &["rev-parse", "HEAD"]), original);
    assert_eq!(
        std::fs::read_to_string(repo.path().join("secret")).unwrap(),
        "local ignored bytes"
    );
}
fn hook(path: &Path, body: &str) {
    let hooks = path.join(".git/test-hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    std::fs::write(hooks.join("post-checkout"), body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            hooks.join("post-checkout"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
    git(path, &["config", "core.hooksPath", ".git/test-hooks"]);
}
#[tokio::test]
async fn failing_or_successful_drifting_hook_reports_partial_state_without_rollback() {
    for body in [
        "#!/bin/sh\necho hook-rejected >&2\nexit 1\n",
        "#!/bin/sh\ngit config branch.new.merge refs/heads/wrong\nexit 0\n",
    ] {
        let (repo, oid) = fixture();
        hook(repo.path(), body);
        let engine = GitEngine::isolated();
        let id = engine
            .open_repository(repo.path())
            .await
            .unwrap()
            .repository_id;
        let prepared = plan(
            &engine,
            &id,
            "team/remote",
            "refs/cache/team/feature/name",
            "new",
        )
        .await
        .unwrap();
        let error = engine.execute(&prepared).await.err().unwrap();
        assert!(error.message.contains("Observed HEAD"), "{error}");
        assert_eq!(
            git(repo.path(), &["symbolic-ref", "HEAD"]),
            "refs/heads/new"
        );
        assert_eq!(git(repo.path(), &["rev-parse", "HEAD"]), oid);
    }
}
#[tokio::test]
async fn unborn_and_detached_heads_can_checkout_an_explicit_remote_branch() {
    for unborn in [true, false] {
        let (repo, oid) = fixture();
        if unborn {
            git(repo.path(), &["switch", "--orphan", "unborn"]);
        } else {
            git(repo.path(), &["switch", "--detach"]);
        }
        let engine = GitEngine::isolated();
        let id = engine
            .open_repository(repo.path())
            .await
            .unwrap()
            .repository_id;
        let prepared = plan(
            &engine,
            &id,
            "team/remote",
            "refs/cache/team/feature/name",
            "new",
        )
        .await
        .unwrap();
        engine.execute(&prepared).await.unwrap();
        assert_eq!(git(repo.path(), &["rev-parse", "HEAD"]), oid);
    }
}
#[tokio::test]
async fn resulting_tracking_is_used_by_existing_pull_and_push() {
    let (repo, _) = fixture();
    let server = tempfile::tempdir().unwrap();
    git(server.path(), &["init", "--bare"]);
    git(
        repo.path(),
        &[
            "remote",
            "set-url",
            "team/remote",
            server.path().to_str().unwrap(),
        ],
    );
    git(
        repo.path(),
        &["push", "team/remote", "main:refs/heads/feature/name"],
    );
    let engine = GitEngine::isolated();
    let id = engine
        .open_repository(repo.path())
        .await
        .unwrap()
        .repository_id;
    let checkout = plan(
        &engine,
        &id,
        "team/remote",
        "refs/cache/team/feature/name",
        "local",
    )
    .await
    .unwrap();
    engine.execute(&checkout).await.unwrap();
    std::fs::write(repo.path().join("file"), "pushed").unwrap();
    let pushed = commit(repo.path(), "local push");
    let status = engine.get_status(&id).await.unwrap();
    let push = engine
        .prepare_action(
            &id,
            RepositoryAction::Push {
                status_snapshot_id: status.snapshot_id,
                remote_id: "team/remote".into(),
                destination_ref: "refs/heads/feature/name".into(),
                set_upstream: false,
            },
        )
        .await
        .unwrap();
    engine.execute(&push).await.unwrap();
    assert_eq!(
        git(server.path(), &["rev-parse", "refs/heads/feature/name"]),
        pushed
    );
    git(repo.path(), &["switch", "main"]);
    git(repo.path(), &["merge", "--ff-only", "local"]);
    std::fs::write(repo.path().join("file"), "upstream").unwrap();
    let upstream = commit(repo.path(), "upstream");
    git(
        repo.path(),
        &["push", "team/remote", "main:refs/heads/feature/name"],
    );
    git(repo.path(), &["switch", "local"]);
    let status = engine.get_status(&id).await.unwrap();
    let pull = engine
        .prepare_action(
            &id,
            RepositoryAction::Pull {
                status_snapshot_id: status.snapshot_id,
            },
        )
        .await
        .unwrap();
    engine.execute(&pull).await.unwrap();
    assert_eq!(git(repo.path(), &["rev-parse", "HEAD"]), upstream);
}
#[tokio::test]
async fn source_is_locked_through_hook_and_released_after_checkout() {
    let (repo, oid) = fixture();
    hook(repo.path(), "#!/bin/sh\nif git update-ref refs/cache/team/feature/name HEAD 2>/dev/null; then\n  echo unexpected-source-update >&2\n  exit 1\nfi\nexit 0\n");
    let engine = GitEngine::isolated();
    let id = engine
        .open_repository(repo.path())
        .await
        .unwrap()
        .repository_id;
    let prepared = plan(
        &engine,
        &id,
        "team/remote",
        "refs/cache/team/feature/name",
        "new",
    )
    .await
    .unwrap();
    engine.execute(&prepared).await.unwrap();
    assert_eq!(
        git(repo.path(), &["rev-parse", "refs/cache/team/feature/name"]),
        oid
    );
    git(
        repo.path(),
        &["update-ref", "refs/cache/team/feature/name", &oid],
    );
    assert!(!repo
        .path()
        .join(".git/refs/cache/team/feature/name.lock")
        .exists());
}
#[tokio::test]
async fn prepared_reference_hook_drift_is_rejected_before_switch() {
    let (repo, _) = fixture();
    hook(repo.path(), "#!/bin/sh\nexit 0\n");
    let reference_hook = repo.path().join(".git/test-hooks/reference-transaction");
    std::fs::write(&reference_hook, "#!/bin/sh\n[ \"$1\" = prepared ] || exit 0\nwhile read -r old new ref; do\n  if [ \"$ref\" = refs/cache/team/feature/name ]; then\n    git config test.checkoutDrift changed || exit 1\n  fi\ndone\nexit 0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(reference_hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let engine = GitEngine::isolated();
    let id = engine
        .open_repository(repo.path())
        .await
        .unwrap()
        .repository_id;
    let prepared = plan(
        &engine,
        &id,
        "team/remote",
        "refs/cache/team/feature/name",
        "new",
    )
    .await
    .unwrap();
    assert_eq!(
        engine.execute(&prepared).await.err().unwrap().code,
        "precondition_changed"
    );
    assert_eq!(
        git(repo.path(), &["symbolic-ref", "HEAD"]),
        "refs/heads/main"
    );
}
#[tokio::test]
async fn ongoing_merge_and_linked_unborn_occupancy_are_rejected() {
    let (repo, oid) = fixture();
    let engine = GitEngine::isolated();
    let id = engine
        .open_repository(repo.path())
        .await
        .unwrap()
        .repository_id;
    std::fs::write(repo.path().join(".git/MERGE_HEAD"), format!("{oid}\n")).unwrap();
    std::fs::write(repo.path().join(".git/MERGE_MSG"), "unfinished merge").unwrap();
    assert_eq!(
        plan(
            &engine,
            &id,
            "team/remote",
            "refs/cache/team/feature/name",
            "new"
        )
        .await
        .err()
        .unwrap()
        .code,
        "operation_in_progress"
    );
    std::fs::remove_file(repo.path().join(".git/MERGE_HEAD")).unwrap();
    std::fs::remove_file(repo.path().join(".git/MERGE_MSG")).unwrap();
    let linked = tempfile::tempdir().unwrap();
    let path = linked.path().join("linked");
    git(
        repo.path(),
        &[
            "worktree",
            "add",
            "--orphan",
            "-b",
            "busy",
            path.to_str().unwrap(),
        ],
    );
    assert_eq!(
        plan(
            &engine,
            &id,
            "team/remote",
            "refs/cache/team/feature/name",
            "busy"
        )
        .await
        .err()
        .unwrap()
        .code,
        "branch_exists"
    );
}
#[tokio::test]
async fn canceled_checkout_preserves_state_and_does_not_create_branch() {
    let (repo, oid) = fixture();
    let engine = GitEngine::isolated();
    let id = engine
        .open_repository(repo.path())
        .await
        .unwrap()
        .repository_id;
    let prepared = plan(
        &engine,
        &id,
        "team/remote",
        "refs/cache/team/feature/name",
        "new",
    )
    .await
    .unwrap();
    let canceled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    assert_eq!(
        engine
            .execute_cancelable(&prepared, canceled)
            .await
            .err()
            .unwrap()
            .code,
        "canceled"
    );
    assert_eq!(git(repo.path(), &["rev-parse", "HEAD"]), oid);
    assert!(git(repo.path(), &["for-each-ref", "refs/heads/new"]).is_empty());
}
#[tokio::test]
async fn exact_partial_glob_and_unmapped_reference_behaviors_are_explicit() {
    let (repo, oid) = fixture();
    git(
        repo.path(),
        &["config", "--unset-all", "remote.team/remote.fetch"],
    );
    git(
        repo.path(),
        &[
            "config",
            "--add",
            "remote.team/remote.fetch",
            "+refs/heads/feature/name:refs/cache/team/feature/name",
        ],
    );
    tracking(
        repo.path(),
        "partial",
        "refs/custom/topic-tail",
        &oid,
        "+refs/heads/release-*:refs/custom/*-tail",
    );
    git(
        repo.path(),
        &["update-ref", "refs/remotes/orphan/topic", &oid],
    );
    let engine = GitEngine::isolated();
    let id = engine
        .open_repository(repo.path())
        .await
        .unwrap()
        .repository_id;
    let listing = engine.list_remote_branches(&id).await.unwrap();
    assert_eq!(listing.targets.len(), 2);
    assert!(listing
        .targets
        .iter()
        .any(|target| target.upstream_ref == "refs/heads/release-topic"));
    assert!(listing
        .issues
        .iter()
        .any(|issue| issue.source_ref == "refs/remotes/orphan/topic"));
    let checkout = plan(
        &engine,
        &id,
        "partial",
        "refs/custom/topic-tail",
        "release.topic",
    )
    .await
    .unwrap();
    engine.execute(&checkout).await.unwrap();
    assert_eq!(
        git(repo.path(), &["config", "branch.release.topic.merge"]),
        "refs/heads/release-topic"
    );
}
#[tokio::test]
async fn multiple_destinations_for_one_upstream_are_not_silently_selected() {
    let (repo, oid) = fixture();
    tracking(
        repo.path(),
        "team/remote",
        "refs/cache/duplicate/feature/name",
        &oid,
        "+refs/heads/*:refs/cache/duplicate/*",
    );
    let engine = GitEngine::isolated();
    let id = engine
        .open_repository(repo.path())
        .await
        .unwrap()
        .repository_id;
    let listing = engine.list_remote_branches(&id).await.unwrap();
    assert!(listing.targets.is_empty());
    assert_eq!(listing.issues.len(), 2);
    assert!(listing
        .issues
        .iter()
        .all(|issue| issue.reason.contains("multiple local")));
}
#[tokio::test]
async fn squash_review_state_is_preserved_when_checkout_is_rejected() {
    let (repo, _) = fixture();
    git(repo.path(), &["switch", "-c", "source"]);
    std::fs::write(repo.path().join("file"), "squash review").unwrap();
    commit(repo.path(), "source change");
    git(repo.path(), &["switch", "main"]);
    let engine = GitEngine::isolated();
    let id = engine
        .open_repository(repo.path())
        .await
        .unwrap()
        .repository_id;
    let status = engine.get_status(&id).await.unwrap();
    let squash = engine
        .prepare_action(
            &id,
            RepositoryAction::SquashMerge {
                status_snapshot_id: status.snapshot_id,
                source_ref: "refs/heads/source".into(),
            },
        )
        .await
        .unwrap();
    engine.execute(&squash).await.unwrap();
    let before = std::fs::read(repo.path().join(".git/index")).unwrap();
    assert_eq!(
        plan(
            &engine,
            &id,
            "team/remote",
            "refs/cache/team/feature/name",
            "new"
        )
        .await
        .err()
        .unwrap()
        .code,
        "dirty"
    );
    assert_eq!(
        engine.get_status(&id).await.unwrap().git_state,
        GitState::SquashMerge
    );
    assert_eq!(
        std::fs::read(repo.path().join(".git/index")).unwrap(),
        before
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("file")).unwrap(),
        "squash review"
    );
}
