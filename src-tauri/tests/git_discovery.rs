use git_view::git::GitEngine;
use std::{path::Path, process::Command};

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
fn make_repo(path: &Path) {
    std::fs::create_dir_all(path).unwrap();
    git(path, &["init", "-b", "main", "--template="]);
    git(path, &["config", "user.name", "Fixture"]);
    git(path, &["config", "user.email", "fixture@example.invalid"]);
    git(path, &["config", "commit.gpgSign", "false"]);
    git(path, &["config", "core.hooksPath", ".git/test-hooks"]);
}
#[tokio::test]
async fn depth_zero_one_and_boundary_are_exact_and_existing_identity_is_reused() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    make_repo(&root.join("near"));
    make_repo(&root.join("outer/deep"));
    let engine = GitEngine::isolated();
    let known = engine.open_repository(&root.join("near")).await.unwrap();
    let zero = engine.discover_repositories(root, 0).await.unwrap();
    assert!(zero.candidates.is_empty());
    assert_eq!(zero.visited_directories, 1);
    assert!(!zero.truncated);
    let one = engine.discover_repositories(root, 1).await.unwrap();
    assert_eq!(one.candidates.len(), 1);
    assert_eq!(one.candidates[0].repository_id, known.repository_id);
    let two = engine.discover_repositories(root, 2).await.unwrap();
    assert_eq!(two.candidates.len(), 2);
    assert!(!two.truncated);
    assert!(two.issues.is_empty());
    assert_eq!(
        engine
            .discover_repositories(root, 11)
            .await
            .err()
            .unwrap()
            .code,
        "invalid_depth"
    );
}
#[tokio::test]
async fn recognized_root_stops_before_nested_repositories() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    make_repo(root);
    make_repo(&root.join("nested"));
    let engine = GitEngine::isolated();
    let result = engine.discover_repositories(root, 10).await.unwrap();
    assert_eq!(result.candidates.len(), 1);
    assert_eq!(result.visited_directories, 1);
    let zero = engine.discover_repositories(root, 0).await.unwrap();
    assert_eq!(
        zero.candidates[0].repository_id,
        result.candidates[0].repository_id
    );
}
#[tokio::test]
async fn linked_worktree_git_files_are_found_as_distinct_worktrees() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let main = root.join("main");
    make_repo(&main);
    git(&main, &["commit", "--allow-empty", "-m", "base"]);
    let linked = root.join("linked");
    git(
        &main,
        &["worktree", "add", "-b", "linked", linked.to_str().unwrap()],
    );
    assert!(linked.join(".git").is_file());
    let engine = GitEngine::isolated();
    let result = engine.discover_repositories(root, 1).await.unwrap();
    assert_eq!(result.candidates.len(), 2);
    assert_ne!(
        result.candidates[0].repository_id,
        result.candidates[1].repository_id
    );
    assert!(result.issues.is_empty());
}
#[tokio::test]
async fn malformed_markers_are_reported_without_hiding_nested_repositories() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir(root.join("broken")).unwrap();
    std::fs::write(root.join("broken/.git"), "gitdir: missing-location\n").unwrap();
    make_repo(&root.join("broken/valid"));
    let engine = GitEngine::isolated();
    let result = engine.discover_repositories(root, 2).await.unwrap();
    assert_eq!(result.candidates.len(), 1);
    assert_eq!(result.candidates[0].label, "valid");
    assert_eq!(result.issues.len(), 1);
    assert!(result.issues[0]
        .reason
        .contains("Cannot open Git repository"));
    assert!(!result.truncated);
}
#[tokio::test]
async fn dependency_build_and_internal_folders_are_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for name in [
        "node_modules",
        "target",
        "dist",
        "build",
        ".venv",
        "venv",
        ".cache",
        ".git",
    ] {
        std::fs::create_dir(root.join(name)).unwrap();
        make_repo(&root.join(name).join("hidden"));
    }
    make_repo(&root.join("visible"));
    let engine = GitEngine::isolated();
    let result = engine.discover_repositories(root, 3).await.unwrap();
    assert_eq!(result.candidates.len(), 1);
    assert_eq!(result.candidates[0].label, "visible");
    assert_eq!(result.visited_directories, 2);
    // An invalid root .git directory is reported even though traversal never enters it.
    assert!(result
        .issues
        .iter()
        .all(|item| item.path == root.to_string_lossy()));
}
#[tokio::test]
async fn missing_paths_and_plain_files_are_reported_without_mutations() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let missing = root.join("missing");
    let file = root.join("file");
    std::fs::write(&file, "retain").unwrap();
    let engine = GitEngine::isolated();
    for path in [&missing, &file] {
        let result = engine.discover_repositories(path, 3).await.unwrap();
        assert!(result.candidates.is_empty());
        assert_eq!(result.issues.len(), 1);
        assert!(!result.truncated);
    }
    assert!(!missing.exists());
    assert_eq!(std::fs::read_to_string(file).unwrap(), "retain");
}
#[cfg(unix)]
#[tokio::test]
async fn symlink_escape_cycle_and_root_link_are_not_followed() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("scan");
    std::fs::create_dir(&root).unwrap();
    let outside = dir.path().join("outside");
    make_repo(&outside);
    symlink(&outside, root.join("escape")).unwrap();
    symlink(&root, root.join("cycle")).unwrap();
    make_repo(&root.join("real"));
    let engine = GitEngine::isolated();
    let result = engine.discover_repositories(&root, 10).await.unwrap();
    assert_eq!(result.candidates.len(), 1);
    assert_eq!(result.candidates[0].label, "real");
    assert_eq!(result.visited_directories, 2);
    assert_eq!(result.issues.len(), 2);
    let linked = engine
        .discover_repositories(&root.join("escape"), 3)
        .await
        .unwrap();
    assert!(linked.candidates.is_empty());
    assert_eq!(linked.issues.len(), 1);
}
#[cfg(unix)]
#[tokio::test]
async fn inaccessible_directory_is_reported() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let blocked = dir.path().join("blocked");
    std::fs::create_dir(&blocked).unwrap();
    std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o0)).unwrap();
    let unavailable = std::fs::read_dir(&blocked).is_err();
    let engine = GitEngine::isolated();
    let result = engine.discover_repositories(dir.path(), 1).await.unwrap();
    std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o700)).unwrap();
    if unavailable {
        assert!(result
            .issues
            .iter()
            .any(|item| item.path.ends_with("blocked")));
    } else {
        eprintln!("SKIP inaccessible-folder assertion: process can read mode-000 directories");
    }
}

#[tokio::test]
async fn discovery_preserves_completed_squash_journal_and_index_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    make_repo(root);
    std::fs::write(root.join("a"), "base\n").unwrap();
    git(root, &["add", "a"]);
    git(root, &["commit", "-m", "base"]);
    let head = git(root, &["rev-parse", "HEAD"]);
    let bytes=serde_json::to_vec(&serde_json::json!({"version":1,"operationId":"retained-fixture","phase":"review","originalBranch":"refs/heads/main","state":{"originalHead":head,"sourceOid":head,"sourceRef":"refs/heads/source"}})).unwrap();
    let marker = root.join(".git/git-view-squash.json");
    std::fs::write(&marker, &bytes).unwrap();
    let index = std::fs::read(root.join(".git/index")).unwrap();
    let engine = GitEngine::isolated();
    let result = engine.discover_repositories(root, 3).await.unwrap();
    assert_eq!(result.candidates.len(), 1);
    assert!(result.issues.is_empty());
    assert_eq!(std::fs::read(marker).unwrap(), bytes);
    assert_eq!(std::fs::read(root.join(".git/index")).unwrap(), index);
}
#[cfg(windows)]
#[tokio::test]
async fn windows_junction_escape_and_cycle_are_not_followed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("scan");
    std::fs::create_dir(&root).unwrap();
    let outside = dir.path().join("outside");
    make_repo(&outside);
    make_repo(&root.join("real"));
    for (name, target) in [("escape", outside.as_path()), ("cycle", root.as_path())] {
        let output = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(root.join(name))
            .arg(target)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "Cannot create fixture junction: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let engine = GitEngine::isolated();
    let result = engine.discover_repositories(&root, 10).await.unwrap();
    assert_eq!(result.candidates.len(), 1);
    assert_eq!(result.candidates[0].label, "real");
    assert_eq!(result.visited_directories, 2);
    assert_eq!(result.issues.len(), 2);
    let linked = engine
        .discover_repositories(&root.join("escape"), 3)
        .await
        .unwrap();
    assert!(linked.candidates.is_empty());
    assert_eq!(linked.issues.len(), 1);
}

#[cfg(unix)]
#[tokio::test]
async fn non_utf8_repository_paths_are_never_offered_as_lossy_project_paths() {
    use std::os::unix::ffi::OsStringExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir
        .path()
        .join(std::ffi::OsString::from_vec(b"repository-\xff".to_vec()));
    match std::fs::create_dir(&path) {
        Ok(()) => {}
        Err(error)
            if error.kind() == std::io::ErrorKind::PermissionDenied
                || (cfg!(target_os = "macos") && error.raw_os_error() == Some(92)) =>
        {
            eprintln!("SKIP invalid-byte directory fixture: filesystem rejects creation: {error}");
            return;
        }
        Err(error) => panic!("{error}"),
    }
    make_repo(&path);
    let engine = GitEngine::isolated();
    let result = engine.discover_repositories(dir.path(), 1).await.unwrap();
    assert!(result.candidates.is_empty());
    assert!(result
        .issues
        .iter()
        .any(|item| item.reason.contains("not UTF-8")));
    assert!(!result.truncated);
}
