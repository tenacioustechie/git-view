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

fn stash_fixture() -> TempDir {
    let dir = repo();
    std::fs::write(dir.path().join("file"), "base\n").unwrap();
    commit(dir.path(), "base");
    dir
}
async fn open(engine: &GitEngine, path: &Path) -> String {
    engine.open_repository(path).await.unwrap().repository_id
}
async fn create(
    engine: &GitEngine,
    id: &str,
    message: &str,
    include_untracked: bool,
) -> AppResult<()> {
    let status = engine.get_status(id).await?;
    let plan = engine
        .prepare_action(
            id,
            RepositoryAction::CreateStash {
                status_snapshot_id: status.snapshot_id,
                message: message.into(),
                include_untracked,
            },
        )
        .await?;
    engine.execute(&plan).await?;
    Ok(())
}
async fn apply(engine: &GitEngine, id: &str, restore_index: bool) -> AppResult<()> {
    let list = engine.list_stashes(id).await?;
    let status = engine.get_status(id).await?;
    let plan = engine
        .prepare_action(
            id,
            RepositoryAction::ApplyStash {
                status_snapshot_id: status.snapshot_id,
                stash_snapshot_id: list.snapshot_id,
                entry_id: list.entries[0].entry_id.clone(),
                restore_index,
            },
        )
        .await?;
    engine.execute(&plan).await?;
    Ok(())
}
#[tokio::test]
async fn save_inspect_apply_index_and_worktree_versions_retains_entry() {
    let dir = stash_fixture();
    let p = dir.path();
    let engine = GitEngine::isolated();
    let id = open(&engine, p).await;
    std::fs::write(p.join("file"), "staged\n").unwrap();
    git(p, &["add", "file"]);
    std::fs::write(p.join("file"), "working\n").unwrap();
    std::fs::write(p.join("new"), "untracked\n").unwrap();
    create(&engine, &id, "named stash", true).await.unwrap();
    assert!(!p.join("new").exists());
    assert_eq!(std::fs::read_to_string(p.join("file")).unwrap(), "base\n");
    let list = engine.list_stashes(&id).await.unwrap();
    assert_eq!(list.entries.len(), 1);
    assert!(list.entries[0].has_untracked);
    assert!(list.entries[0].message.contains("named stash"));
    for section in [
        StashSection::Worktree,
        StashSection::Index,
        StashSection::Untracked,
    ] {
        let details = engine
            .get_stash_details(&id, &list.snapshot_id, &list.entries[0].entry_id, section)
            .await
            .unwrap();
        assert_eq!(details.files.len(), 1);
    }
    apply(&engine, &id, true).await.unwrap();
    assert_eq!(git(p, &["show", ":file"]), "staged");
    assert_eq!(
        std::fs::read_to_string(p.join("file")).unwrap(),
        "working\n"
    );
    assert_eq!(
        std::fs::read_to_string(p.join("new")).unwrap(),
        "untracked\n"
    );
    assert_eq!(engine.list_stashes(&id).await.unwrap().entries.len(), 1);
}
#[tokio::test]
async fn unchecked_untracked_and_default_apply_index_behavior() {
    let dir = stash_fixture();
    let p = dir.path();
    let engine = GitEngine::isolated();
    let id = open(&engine, p).await;
    std::fs::write(p.join("file"), "saved\n").unwrap();
    std::fs::write(p.join("keep"), "untracked\n").unwrap();
    create(&engine, &id, "tracked only", false).await.unwrap();
    assert!(p.join("keep").exists());
    assert!(!engine.list_stashes(&id).await.unwrap().entries[0].has_untracked);
    assert!(apply(&engine, &id, false).await.is_err());
    std::fs::remove_file(p.join("keep")).unwrap();
    git(p, &["config", "stash.index", "true"]);
    apply(&engine, &id, false).await.unwrap();
    assert_eq!(git(p, &["show", ":file"]), "base");
    assert_eq!(std::fs::read_to_string(p.join("file")).unwrap(), "saved\n");
}
#[tokio::test]
async fn ignored_file_directory_collision_and_conflict_preserve_stash() {
    let dir = stash_fixture();
    let p = dir.path();
    let engine = GitEngine::isolated();
    let id = open(&engine, p).await;
    std::fs::write(p.join("secret"), "saved\n").unwrap();
    git(p, &["add", "secret"]);
    create(&engine, &id, "secret addition", false)
        .await
        .unwrap();
    std::fs::write(p.join(".git/info/exclude"), "secret\n").unwrap();
    std::fs::create_dir(p.join("secret")).unwrap();
    std::fs::write(p.join("secret/private"), "private\n").unwrap();
    assert_eq!(
        apply(&engine, &id, false).await.unwrap_err().code,
        "local_path_collision"
    );
    assert_eq!(
        std::fs::read_to_string(p.join("secret/private")).unwrap(),
        "private\n"
    );
    let dir = stash_fixture();
    let p = dir.path();
    let engine = GitEngine::isolated();
    let id = open(&engine, p).await;
    std::fs::write(p.join("file"), "saved\n").unwrap();
    create(&engine, &id, "conflict", false).await.unwrap();
    std::fs::write(p.join("file"), "other\n").unwrap();
    commit(p, "other");
    assert!(apply(&engine, &id, false).await.is_err());
    let status = engine.get_status(&id).await.unwrap();
    assert!(status.counts.conflicted > 0);
    assert_eq!(status.git_state, GitState::Idle);
    assert_eq!(engine.list_stashes(&id).await.unwrap().entries.len(), 1);
    assert!(!p.join(".git/MERGE_HEAD").exists());
}
#[tokio::test]
async fn occurrence_identity_stale_lists_and_cancellation() {
    let dir = stash_fixture();
    let p = dir.path();
    let engine = GitEngine::isolated();
    let id = open(&engine, p).await;
    std::fs::write(p.join("file"), "one\n").unwrap();
    create(&engine, &id, "first", false).await.unwrap();
    let first = engine.list_stashes(&id).await.unwrap();
    let status = engine.get_status(&id).await.unwrap();
    let plan = engine
        .prepare_action(
            &id,
            RepositoryAction::ApplyStash {
                status_snapshot_id: status.snapshot_id,
                stash_snapshot_id: first.snapshot_id.clone(),
                entry_id: first.entries[0].entry_id.clone(),
                restore_index: false,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .execute_cancelable(
                &plan,
                std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true))
            )
            .await
            .err()
            .unwrap()
            .code,
        "canceled"
    );
    std::fs::write(p.join("file"), "two\n").unwrap();
    create(&engine, &id, "second", false).await.unwrap();
    assert_eq!(
        engine.execute(&plan).await.err().unwrap().code,
        "precondition_changed"
    );
    assert!(engine
        .get_stash_details(
            &id,
            &first.snapshot_id,
            &first.entries[0].entry_id,
            StashSection::Worktree
        )
        .await
        .is_err());
    assert!(git(p, &["status", "--porcelain"]).is_empty());
}
#[cfg(any(target_os = "macos", windows))]
#[tokio::test]
async fn delete_selected_occurrence_preserves_files_index_and_packed_refs() {
    for selection in 0..3 {
        let dir = stash_fixture();
        let p = dir.path();
        let engine = GitEngine::isolated();
        let id = open(&engine, p).await;
        for message in ["first", "second", "third"] {
            std::fs::write(p.join("file"), message).unwrap();
            create(&engine, &id, message, false).await.unwrap();
        }
        git(p, &["pack-refs", "--all", "--prune"]);
        std::fs::write(p.join("file"), "local staged").unwrap();
        git(p, &["add", "file"]);
        std::fs::write(p.join("file"), "local working").unwrap();
        let index = std::fs::read(p.join(".git/index")).unwrap();
        let list = engine.list_stashes(&id).await.unwrap();
        let selected = list.entries[selection].clone();
        let expected: Vec<_> = list
            .entries
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != selection)
            .map(|(_, e)| e.oid.clone())
            .collect();
        let status = engine.get_status(&id).await.unwrap();
        let plan = engine
            .prepare_action(
                &id,
                RepositoryAction::DeleteStash {
                    status_snapshot_id: status.snapshot_id,
                    stash_snapshot_id: list.snapshot_id,
                    entry_id: selected.entry_id,
                },
            )
            .await
            .unwrap();
        engine.execute(&plan).await.unwrap();
        let after = engine.list_stashes(&id).await.unwrap();
        assert_eq!(
            after
                .entries
                .iter()
                .map(|e| e.oid.clone())
                .collect::<Vec<_>>(),
            expected
        );
        assert!(after.recovery.is_none());
        assert_eq!(std::fs::read(p.join(".git/index")).unwrap(), index);
        assert_eq!(
            std::fs::read_to_string(p.join("file")).unwrap(),
            "local working"
        );
        assert_eq!(
            git(
                p,
                &[
                    "for-each-ref",
                    "--format=%(objectname)",
                    "refs/git-view/stash-deletions"
                ]
            ),
            selected.oid
        );
    }
}
fn hook(path: &Path, name: &str, body: &str) {
    let directory = path.join(".git/test-hooks");
    std::fs::create_dir_all(&directory).unwrap();
    let file = directory.join(name);
    std::fs::write(&file, format!("#!/bin/sh\n{body}\n")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    git(path, &["config", "core.hooksPath", ".git/test-hooks"]);
}
#[tokio::test]
async fn create_ref_hook_rejection_leaves_actual_changes_and_no_false_success() {
    let dir = stash_fixture();
    let p = dir.path();
    let engine = GitEngine::isolated();
    let id = open(&engine, p).await;
    std::fs::write(p.join("file"), "not saved\n").unwrap();
    hook(
        p,
        "reference-transaction",
        "[ \"$1\" != prepared ] || exit 1\nexit 0",
    );
    assert!(create(&engine, &id, "rejected", false).await.is_err());
    assert_eq!(
        std::fs::read_to_string(p.join("file")).unwrap(),
        "not saved\n"
    );
    assert!(engine.list_stashes(&id).await.unwrap().entries.is_empty());
}
#[tokio::test]
async fn directory_rename_and_untracked_collisions_are_blocked_before_partial_apply() {
    for setting in [None, Some("true")] {
        let dir = stash_fixture();
        let p = dir.path();
        std::fs::create_dir(p.join("old")).unwrap();
        std::fs::write(p.join("old/existing"), "existing\n").unwrap();
        commit(p, "directory base");
        let engine = GitEngine::isolated();
        let id = open(&engine, p).await;
        std::fs::write(p.join("old/new"), "incoming\n").unwrap();
        git(p, &["add", "old/new"]);
        create(&engine, &id, "addition", false).await.unwrap();
        git(p, &["mv", "old", "renamed"]);
        commit(p, "rename dir");
        std::fs::write(p.join(".git/info/exclude"), "renamed/new\n").unwrap();
        std::fs::write(p.join("renamed/new"), "private\n").unwrap();
        if let Some(value) = setting {
            git(p, &["config", "merge.directoryRenames", value]);
        }
        assert_eq!(
            apply(&engine, &id, false).await.unwrap_err().code,
            "ignored_directory_rename"
        );
        assert_eq!(
            std::fs::read_to_string(p.join("renamed/new")).unwrap(),
            "private\n"
        );
        assert!(git(p, &["status", "--porcelain"]).is_empty());
    }
    let dir = stash_fixture();
    let p = dir.path();
    let engine = GitEngine::isolated();
    let id = open(&engine, p).await;
    std::fs::write(p.join("file"), "tracked saved\n").unwrap();
    std::fs::write(p.join("a"), "a\n").unwrap();
    std::fs::write(p.join("b"), "b\n").unwrap();
    create(&engine, &id, "with untracked", true).await.unwrap();
    std::fs::write(p.join(".git/info/exclude"), "b\n").unwrap();
    std::fs::write(p.join("b"), "private\n").unwrap();
    assert_eq!(
        apply(&engine, &id, false).await.unwrap_err().code,
        "local_path_collision"
    );
    assert_eq!(std::fs::read_to_string(p.join("file")).unwrap(), "base\n");
    assert!(!p.join("a").exists());
    assert_eq!(std::fs::read_to_string(p.join("b")).unwrap(), "private\n");
}
#[cfg(any(target_os = "macos", windows))]
#[tokio::test]
async fn identical_raw_occurrences_and_linked_worktrees_have_exact_selection() {
    let dir = stash_fixture();
    let p = dir.path();
    let engine = GitEngine::isolated();
    let id = open(&engine, p).await;
    std::fs::write(p.join("file"), "saved\n").unwrap();
    create(&engine, &id, "original", false).await.unwrap();
    let oid = git(p, &["rev-parse", "refs/stash"]);
    for _ in 0..2 {
        let output = fixture_command(p)
            .env("GIT_COMMITTER_DATE", "1700000000 +0000")
            .args([
                "reflog",
                "write",
                "refs/stash",
                &oid,
                &oid,
                "duplicate occurrence",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let raw = std::fs::read(p.join(".git/logs/refs/stash")).unwrap();
    let rows: Vec<_> = raw
        .split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
        .collect();
    assert_eq!(rows[1], rows[2]);
    let linked = tempfile::tempdir().unwrap();
    let worktree = linked.path().join("linked");
    git(
        p,
        &[
            "worktree",
            "add",
            "--detach",
            worktree.to_str().unwrap(),
            "HEAD",
        ],
    );
    let linked_id = open(&engine, &worktree).await;
    let list = engine.list_stashes(&id).await.unwrap();
    assert_eq!(list.entries.len(), 3);
    assert_ne!(list.entries[0].entry_id, list.entries[1].entry_id);
    assert_eq!(list.entries[0].created_at, "2023-11-14T22:13:20+00:00");
    assert_eq!(list.entries[0].message, "duplicate occurrence");
    let refreshed = engine.list_stashes(&linked_id).await.unwrap();
    assert_ne!(refreshed.snapshot_id, list.snapshot_id);
    assert_eq!(
        refreshed
            .entries
            .iter()
            .map(|e| &e.entry_id)
            .collect::<Vec<_>>(),
        list.entries.iter().map(|e| &e.entry_id).collect::<Vec<_>>()
    );
    let original_ids: Vec<_> = list.entries.iter().map(|e| e.entry_id.clone()).collect();
    let status = engine.get_status(&linked_id).await.unwrap();
    let plan = engine
        .prepare_action(
            &linked_id,
            RepositoryAction::DeleteStash {
                status_snapshot_id: status.snapshot_id,
                stash_snapshot_id: list.snapshot_id,
                entry_id: list.entries[1].entry_id.clone(),
            },
        )
        .await
        .unwrap();
    engine.execute(&plan).await.unwrap();
    let after = engine.list_stashes(&id).await.unwrap();
    assert_eq!(after.entries.len(), 2);
    assert_eq!(after.entries[0].message, "duplicate occurrence");
    assert_eq!(after.entries[1].message, "On main: original");
    assert!(after
        .entries
        .iter()
        .all(|e| !original_ids.contains(&e.entry_id)));
    assert_eq!(git(p, &["rev-parse", "refs/stash"]), oid);
}
#[tokio::test]
async fn changed_submodule_gitlinks_are_rejected_even_when_nested_checkout_matches_index() {
    let child = stash_fixture();
    let dir = stash_fixture();
    let p = dir.path();
    git(
        p,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            child.path().to_str().unwrap(),
            "module",
        ],
    );
    commit(p, "add submodule");
    let module = p.join("module");
    git(&module, &["config", "user.name", "Fixture"]);
    git(
        &module,
        &["config", "user.email", "fixture@example.invalid"],
    );
    git(&module, &["config", "commit.gpgsign", "false"]);
    git(&module, &["config", "core.hooksPath", ".disabled-hooks"]);
    std::fs::write(module.join("file"), "new submodule\n").unwrap();
    commit(&module, "child next");
    git(p, &["add", "module"]);
    std::fs::write(p.join("file"), "normal local\n").unwrap();
    let engine = GitEngine::isolated();
    let id = open(&engine, p).await;
    assert!(!engine.get_status(&id).await.unwrap().submodule_dirty);
    assert!(create(&engine, &id, "unsupported gitlink", false)
        .await
        .unwrap_err()
        .message
        .contains("Submodule"));
    assert_eq!(
        std::fs::read_to_string(p.join("file")).unwrap(),
        "normal local\n"
    );
    assert!(engine.list_stashes(&id).await.unwrap().entries.is_empty());
    // An external stash with a changed gitlink must also remain inspectable but not applied.
    git(p, &["stash", "push", "-m", "external gitlink"]);
    git(p, &["reset", "--hard", "HEAD"]);
    git(
        p,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "update",
            "--init",
        ],
    );
    assert!(apply(&engine, &id, true)
        .await
        .unwrap_err()
        .message
        .contains("gitlink"));
}
#[cfg(unix)]
#[tokio::test]
async fn ignored_symlink_collision_keeps_outside_bytes() {
    let dir = stash_fixture();
    let p = dir.path();
    let engine = GitEngine::isolated();
    let id = open(&engine, p).await;
    std::fs::write(p.join("secret"), "saved").unwrap();
    git(p, &["add", "secret"]);
    create(&engine, &id, "secret", false).await.unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("private"), "private").unwrap();
    std::os::unix::fs::symlink(outside.path().join("private"), p.join("secret")).unwrap();
    std::fs::write(p.join(".git/info/exclude"), "secret\n").unwrap();
    assert_eq!(
        apply(&engine, &id, false).await.unwrap_err().code,
        "local_path_collision"
    );
    assert_eq!(
        std::fs::read_to_string(outside.path().join("private")).unwrap(),
        "private"
    );
}
#[cfg(not(any(target_os = "macos", windows)))]
#[tokio::test]
async fn deletion_reports_unsupported_conditional_replace_platform() {
    let dir = stash_fixture();
    let engine = GitEngine::isolated();
    let id = open(&engine, dir.path()).await;
    assert!(engine
        .list_stashes(&id)
        .await
        .unwrap()
        .delete_unavailable_reason
        .is_some());
}
#[cfg(any(target_os = "macos", windows))]
#[tokio::test]
async fn prepared_stash_guard_blocks_external_writers_and_honors_hook_rejection() {
    let dir = stash_fixture();
    let p = dir.path();
    let engine = GitEngine::isolated();
    let id = open(&engine, p).await;
    for message in ["one", "two"] {
        std::fs::write(p.join("file"), message).unwrap();
        create(&engine, &id, message, false).await.unwrap();
    }
    hook(
        p,
        "reference-transaction",
        r#"
[ "$1" = prepared ] || exit 0
while read -r old new ref; do
 if [ "$ref" = refs/stash ]; then
  if git update-ref refs/stash "$old" "$old" 2>/dev/null; then exit 1; fi
  printf 'blocked\n' > "$(git rev-parse --git-path writer-blocked)"
 fi
done
exit 0
"#,
    );
    let list = engine.list_stashes(&id).await.unwrap();
    let status = engine.get_status(&id).await.unwrap();
    let plan = engine
        .prepare_action(
            &id,
            RepositoryAction::DeleteStash {
                status_snapshot_id: status.snapshot_id,
                stash_snapshot_id: list.snapshot_id,
                entry_id: list.entries[0].entry_id.clone(),
            },
        )
        .await
        .unwrap();
    engine.execute(&plan).await.unwrap();
    assert_eq!(
        std::fs::read_to_string(p.join(".git/writer-blocked")).unwrap(),
        "blocked\n"
    );
    assert_eq!(engine.list_stashes(&id).await.unwrap().entries.len(), 1);
    hook(
        p,
        "reference-transaction",
        "[ \"$1\" != prepared ] || exit 1\nexit 0",
    );
    let before = std::fs::read(p.join(".git/logs/refs/stash")).unwrap();
    let list = engine.list_stashes(&id).await.unwrap();
    let status = engine.get_status(&id).await.unwrap();
    let plan = engine
        .prepare_action(
            &id,
            RepositoryAction::DeleteStash {
                status_snapshot_id: status.snapshot_id,
                stash_snapshot_id: list.snapshot_id,
                entry_id: list.entries[0].entry_id.clone(),
            },
        )
        .await
        .unwrap();
    assert!(engine.execute(&plan).await.is_err());
    assert_eq!(
        std::fs::read(p.join(".git/logs/refs/stash")).unwrap(),
        before
    );
    assert!(!p.join(".git/git-view-stash-delete.json").exists());
}
#[cfg(target_os = "macos")]
#[tokio::test]
async fn delete_preserves_private_reflog_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let dir = stash_fixture();
    let p = dir.path();
    let engine = GitEngine::isolated();
    let id = open(&engine, p).await;
    for message in ["one", "two"] {
        std::fs::write(p.join("file"), message).unwrap();
        create(&engine, &id, message, false).await.unwrap();
    }
    let log = p.join(".git/logs/refs/stash");
    std::fs::set_permissions(&log, std::fs::Permissions::from_mode(0o600)).unwrap();
    let list = engine.list_stashes(&id).await.unwrap();
    let status = engine.get_status(&id).await.unwrap();
    let plan = engine
        .prepare_action(
            &id,
            RepositoryAction::DeleteStash {
                status_snapshot_id: status.snapshot_id,
                stash_snapshot_id: list.snapshot_id,
                entry_id: list.entries[0].entry_id.clone(),
            },
        )
        .await
        .unwrap();
    engine.execute(&plan).await.unwrap();
    assert_eq!(
        std::fs::metadata(&log).unwrap().permissions().mode() & 0o777,
        0o600
    );
}
