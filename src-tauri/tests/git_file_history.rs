use git_view::{contracts::*, git::GitEngine};
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};
use tempfile::TempDir;

fn command(path: &Path) -> Command {
    let mut cmd = Command::new("git");
    let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
    cmd.current_dir(path)
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
        cmd.env_remove(key);
    }
    cmd
}
fn git(path: &Path, args: &[&str]) -> String {
    let output = command(path).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().into()
}
fn repo() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    git(p, &["init", "-b", "main"]);
    git(p, &["config", "user.name", "Fixture"]);
    git(p, &["config", "user.email", "fixture@example.invalid"]);
    git(p, &["config", "commit.gpgsign", "false"]);
    git(p, &["config", "core.hooksPath", ".disabled-hooks"]);
    dir
}
fn commit(p: &Path, name: &str, content: &str, message: &str) -> String {
    std::fs::write(p.join(name), content).unwrap();
    git(p, &["add", "--all"]);
    git(p, &["commit", "-m", message]);
    git(p, &["rev-parse", "HEAD"])
}
async fn source(engine: &GitEngine, id: &str, oid: &str, name: &str) -> FileHistoryRequest {
    let details = engine.get_commit(id, oid, None).await.unwrap();
    let file = details
        .files
        .iter()
        .find(|file| file.display_path == name)
        .unwrap();
    FileHistoryRequest {
        repository_id: id.into(),
        source: FileHistorySource {
            snapshot_id: details.snapshot_id,
            file_id: file.file_id.clone(),
            path: FileHistoryPath::Current,
        },
        history_snapshot_id: None,
        cursor: None,
    }
}
fn subjects(page: &FileHistoryPage) -> Vec<&str> {
    page.history
        .commits
        .iter()
        .map(|c| c.subject.as_str())
        .collect()
}

#[tokio::test]
async fn literal_deleted_path_history_has_exact_scope_and_read_only_previews() {
    let dir = repo();
    let p = dir.path();
    let name = "-file[1] β.txt";
    let root = commit(p, name, "one\n", "created");
    commit(p, "-file1 β.txt", "unrelated\n", "not a pattern match");
    git(p, &["checkout", "-b", "side"]);
    commit(p, name, "side\n", "excluded side");
    git(p, &["checkout", "main"]);
    commit(p, name, "two\n", "edited");
    git(p, &["rm", "--", name]);
    git(p, &["commit", "-m", "deleted"]);
    let deleted = git(p, &["rev-parse", "HEAD"]);
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    let request = source(&engine, &id, &deleted, name).await;
    let before = std::fs::read(p.join(".git/index")).unwrap();
    let page = engine.get_file_history(request).await.unwrap();
    assert_eq!(subjects(&page), ["deleted", "edited", "created"]);
    assert_eq!(page.start_oid, Some(deleted.clone()));
    let details = engine
        .get_file_history_commit(&id, &page.history.snapshot_id, &deleted, None)
        .await
        .unwrap();
    assert_eq!(details.files.len(), 1);
    assert_eq!(details.files[0].change_kind.as_deref(), Some("deleted"));
    let diff = engine
        .get_diff(DiffRequest {
            repository_id: id.clone(),
            snapshot_id: details.snapshot_id,
            file_id: details.files[0].file_id.clone(),
            context: "commit".into(),
        })
        .await
        .unwrap();
    assert!(
        matches!(diff, DiffResult::Text { text, hunk_actions: None, .. } if text.contains("-two"))
    );
    assert_eq!(std::fs::read(p.join(".git/index")).unwrap(), before);
    assert_eq!(git(p, &["rev-parse", "HEAD"]), deleted);
    assert!(!p.join(name).exists());
    let root_page = engine
        .get_file_history(source(&engine, &id, &root, name).await)
        .await
        .unwrap();
    assert_eq!(subjects(&root_page), ["created"]);
    assert!(engine
        .get_file_history_commit(&id, &root_page.history.snapshot_id, &deleted, None)
        .await
        .is_err());
}

#[tokio::test]
async fn renamed_paths_are_explicit_and_previous_path_finds_the_rename_preview() {
    let dir = repo();
    let p = dir.path();
    commit(p, "old.txt", "keep exactly the same content\n", "original");
    git(p, &["mv", "old.txt", "new.txt"]);
    git(p, &["commit", "-m", "renamed"]);
    let renamed = git(p, &["rev-parse", "HEAD"]);
    git(p, &["config", "log.follow", "true"]);
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    let mut request = source(&engine, &id, &renamed, "new.txt").await;
    assert_eq!(
        subjects(&engine.get_file_history(request.clone()).await.unwrap()),
        ["renamed"]
    );
    request.source.path = FileHistoryPath::Previous;
    let page = engine.get_file_history(request.clone()).await.unwrap();
    assert_eq!(subjects(&page), ["renamed", "original"]);
    assert_eq!(page.display_path, "old.txt");
    let details = engine
        .get_file_history_commit(&id, &page.history.snapshot_id, &renamed, None)
        .await
        .unwrap();
    assert_eq!(
        details.files[0].old_display_path.as_deref(),
        Some("old.txt")
    );
    request.history_snapshot_id = Some(page.history.snapshot_id);
    request.source.path = FileHistoryPath::Current;
    assert!(engine.get_file_history(request).await.is_err());
}

#[tokio::test]
async fn pages_are_frozen_to_the_original_tip_and_validate_cursors_and_membership() {
    let dir = repo();
    let p = dir.path();
    let mut child = command(p)
        .arg("fast-import")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    for n in 0..205 {
        let message = format!("change {n}\n");
        let contents = format!("line {n}\n");
        write!(input, "commit refs/heads/main\ncommitter Fixture <fixture@example.invalid> {} +0000\ndata {}\n{}M 100644 inline file.txt\ndata {}\n{}\n", 1700000000+n, message.len(), message, contents.len(), contents).unwrap();
    }
    drop(input);
    assert!(child.wait_with_output().unwrap().status.success());
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    let original = git(p, &["rev-parse", "HEAD"]);
    let mut request = source(&engine, &id, &original, "file.txt").await;
    let page = engine.get_file_history(request.clone()).await.unwrap();
    assert_eq!(page.history.commits.len(), 200);
    let tree = git(p, &["rev-parse", "HEAD^{tree}"]);
    let newer = git(p, &["commit-tree", &tree, "-p", &original, "-m", "later"]);
    git(p, &["update-ref", "refs/heads/main", &newer]);
    request.history_snapshot_id = Some(page.history.snapshot_id.clone());
    request.cursor = page.history.next_cursor.clone();
    let next = engine.get_file_history(request.clone()).await.unwrap();
    assert_eq!(next.history.commits.len(), 5);
    assert!(next.history.next_cursor.is_none());
    assert_eq!(next.start_oid, Some(original));
    assert!(next.history.commits.iter().all(|c| !page
        .history
        .commits
        .iter()
        .any(|p| p.oid == c.oid)));
    assert_eq!(
        subjects(&engine.get_file_history(request.clone()).await.unwrap()),
        subjects(&next)
    );
    request.cursor = Some("invented".into());
    assert!(engine.get_file_history(request.clone()).await.is_err());
    request.cursor = None;
    request.source.file_id = "other file".into();
    assert!(engine.get_file_history(request).await.is_err());
    assert!(engine
        .get_file_history_commit(&id, &page.history.snapshot_id, &newer, None)
        .await
        .is_err());
}

#[tokio::test]
async fn working_unborn_shallow_and_invalid_sources_are_explicit() {
    let dir = repo();
    let p = dir.path();
    std::fs::write(p.join("untracked.txt"), "local draft").unwrap();
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    let changes = engine.get_changes(&id).await.unwrap();
    let mut request = FileHistoryRequest {
        repository_id: id.clone(),
        source: FileHistorySource {
            snapshot_id: changes.snapshot_id,
            file_id: changes.files[0].file_id.clone(),
            path: FileHistoryPath::Current,
        },
        history_snapshot_id: None,
        cursor: None,
    };
    let page = engine.get_file_history(request.clone()).await.unwrap();
    assert!(page.history.commits.is_empty());
    assert!(page.start_oid.is_none());
    request.source.path = FileHistoryPath::Previous;
    assert!(engine.get_file_history(request).await.is_err());
    let root = commit(p, "untracked.txt", "committed\n", "root");
    let tip = commit(p, "untracked.txt", "new\n", "tip");
    std::fs::write(p.join(".git/shallow"), format!("{tip}\n")).unwrap();
    let request = source(&engine, &id, &tip, "untracked.txt").await;
    let page = engine.get_file_history(request.clone()).await.unwrap();
    assert!(page.shallow);
    assert_eq!(page.history.commits.len(), 1);
    assert!(page.history.commits[0].boundary);
    std::fs::write(p.join(".git/shallow"), format!("{root}\n")).unwrap();
    let mut stale = request;
    stale.history_snapshot_id = Some(page.history.snapshot_id);
    assert!(engine.get_file_history(stale).await.is_err());
    std::fs::create_dir_all(p.join(".git/info")).unwrap();
    std::fs::write(p.join(".git/info/grafts"), "").unwrap();
    let request = source(&engine, &id, &tip, "untracked.txt").await;
    assert!(engine.get_file_history(request).await.is_err());
}

#[tokio::test]
async fn merge_history_uses_real_parents_and_displays_only_the_selected_path() {
    let dir = repo();
    let p = dir.path();
    commit(p, "file.txt", "base\n", "base");
    git(p, &["checkout", "-b", "side"]);
    let side = commit(p, "file.txt", "side\n", "side edit");
    git(p, &["checkout", "main"]);
    let main = commit(p, "other.txt", "main\n", "main other file");
    git(p, &["merge", "--no-ff", "side", "-m", "merge"]);
    let tip = git(p, &["rev-parse", "HEAD"]);
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    let page = engine
        .get_file_history(source(&engine, &id, &tip, "file.txt").await)
        .await
        .unwrap();
    assert!(subjects(&page).contains(&"side edit"));
    assert!(!subjects(&page).contains(&"main other file"));
    let merge = page.history.commits.iter().find(|c| c.oid == tip).unwrap();
    assert_eq!(merge.parents, [main.clone(), side.clone()]);
    let first = engine
        .get_file_history_commit(&id, &page.history.snapshot_id, &tip, Some(main))
        .await
        .unwrap();
    assert_eq!(first.files.len(), 1);
    let second = engine
        .get_file_history_commit(&id, &page.history.snapshot_id, &tip, Some(side))
        .await
        .unwrap();
    assert!(second.files.is_empty());
    assert!(engine
        .get_file_history_commit(
            &id,
            &page.history.snapshot_id,
            &tip,
            Some("not-a-parent".into())
        )
        .await
        .is_err());
}
