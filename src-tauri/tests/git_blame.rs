use git_view::{contracts::*, git::GitEngine};
use std::{path::Path, process::Command};
use tempfile::TempDir;

fn git(path: &Path, args: &[&str]) -> String {
    let mut cmd = Command::new("git");
    let null = "/dev/null";
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
    let output = cmd.args(args).output().unwrap();
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
    git(p, &["config", "core.autocrlf", "false"]);
    git(p, &["config", "core.hooksPath", ".disabled-hooks"]);
    dir
}
fn commit(p: &Path, message: &str) -> String {
    git(p, &["add", "--all"]);
    git(p, &["commit", "-m", message]);
    git(p, &["rev-parse", "HEAD"])
}
async fn source(engine: &GitEngine, id: &str, oid: &str, path: &str) -> BlameRequest {
    let details = engine.get_commit(id, oid, None).await.unwrap();
    BlameRequest {
        repository_id: id.into(),
        snapshot_id: details.snapshot_id,
        file_id: details
            .files
            .iter()
            .find(|f| f.display_path == path)
            .unwrap()
            .file_id
            .clone(),
        side: FileHistoryPath::Current,
    }
}

#[tokio::test]
async fn exact_revision_lines_raw_identities_rename_origin_and_read_only_navigation() {
    let dir = repo();
    let p = dir.path();
    let old = "-old[1] β.txt";
    let new = "new β.txt";
    std::fs::write(p.join(old), "keep\r\nold\nno newline").unwrap();
    let root = commit(p, "original");
    std::fs::write(p.join(old), "keep\r\nnew\nno newline").unwrap();
    let edited = commit(p, "edited");
    git(p, &["mv", "--", old, new]);
    let renamed = commit(p, "renamed");
    // A configured ignore file, mailmap and replacement must not alter attribution.
    std::fs::write(p.join("ignore-revs"), format!("{edited}\n")).unwrap();
    git(p, &["config", "blame.ignoreRevsFile", "ignore-revs"]);
    std::fs::write(
        p.join(".mailmap"),
        "Mapped <mapped@example.invalid> Fixture <fixture@example.invalid>\n",
    )
    .unwrap();
    git(p, &["replace", &root, &edited]);
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    let request = source(&engine, &id, &renamed, new).await;
    std::fs::write(p.join(new), "local edits\n").unwrap();
    let index = std::fs::read(p.join(".git/index")).unwrap();
    let result = engine.get_blame(request.clone()).await.unwrap();
    assert!(result.unavailable_reason.is_none());
    assert_eq!(result.revision_oid.as_deref(), Some(renamed.as_str()));
    assert_eq!(
        result
            .lines
            .iter()
            .map(|l| l.content.as_str())
            .collect::<Vec<_>>(),
        ["keep\r", "new", "no newline"]
    );
    assert_eq!(
        result
            .lines
            .iter()
            .map(|l| l.oid.as_str())
            .collect::<Vec<_>>(),
        [&root, &edited, &root]
    );
    assert!(result
        .lines
        .iter()
        .all(|l| l.author.name == "Fixture" && l.original_path == old && !l.boundary));
    let details = engine
        .get_blame_commit(&id, &result.snapshot_id, &edited, None)
        .await
        .unwrap();
    assert_eq!(details.message.trim(), "edited");
    assert!(engine
        .get_blame_commit(&id, &result.snapshot_id, &renamed, None)
        .await
        .is_err());
    assert!(engine
        .get_blame_commit("wrong", &result.snapshot_id, &edited, None)
        .await
        .is_err());
    assert!(engine
        .get_blame_commit(&id, &result.snapshot_id, &edited, Some(renamed.clone()))
        .await
        .is_err());
    let mut before = request;
    before.side = FileHistoryPath::Previous;
    let before = engine.get_blame(before).await.unwrap();
    assert_eq!(before.revision_oid, Some(edited));
    assert_eq!(before.display_path, old);
    assert_eq!(std::fs::read(p.join(".git/index")).unwrap(), index);
    assert_eq!(
        std::fs::read_to_string(p.join(new)).unwrap(),
        "local edits\n"
    );
    assert_eq!(git(p, &["rev-parse", "HEAD"]), renamed);
}

#[tokio::test]
async fn working_snapshot_is_frozen_and_unborn_untracked_deleted_root_are_explicit() {
    let dir = repo();
    let p = dir.path();
    std::fs::write(p.join("file"), "draft\n").unwrap();
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    let changes = engine.get_changes(&id).await.unwrap();
    let mut request = BlameRequest {
        repository_id: id.clone(),
        snapshot_id: changes.snapshot_id,
        file_id: changes.files[0].file_id.clone(),
        side: FileHistoryPath::Current,
    };
    assert!(engine
        .get_blame(request)
        .await
        .unwrap()
        .revision_oid
        .is_none());
    let root = commit(p, "root");
    std::fs::write(p.join("file"), "later\n").unwrap();
    let changes = engine.get_changes(&id).await.unwrap();
    request = BlameRequest {
        repository_id: id.clone(),
        snapshot_id: changes.snapshot_id,
        file_id: changes.files[0].file_id.clone(),
        side: FileHistoryPath::Current,
    };
    let later = commit(p, "later");
    let result = engine.get_blame(request.clone()).await.unwrap();
    assert!(result.working);
    assert_eq!(result.revision_oid, Some(root.clone()));
    assert_eq!(result.lines[0].content, "draft");
    request.side = FileHistoryPath::Previous;
    assert!(engine.get_blame(request).await.is_err());
    git(p, &["rm", "file"]);
    let deleted = commit(p, "deleted");
    let mut request = source(&engine, &id, &deleted, "file").await;
    assert!(engine
        .get_blame(request.clone())
        .await
        .unwrap()
        .unavailable_reason
        .unwrap()
        .contains("does not exist"));
    request.side = FileHistoryPath::Previous;
    assert_eq!(
        engine.get_blame(request).await.unwrap().revision_oid,
        Some(later)
    );
    let mut request = source(&engine, &id, &root, "file").await;
    request.side = FileHistoryPath::Previous;
    assert!(engine
        .get_blame(request)
        .await
        .unwrap()
        .revision_oid
        .is_none());
}

#[tokio::test]
async fn binary_encoding_empty_and_large_files_are_not_partial_attributions() {
    let dir = repo();
    let p = dir.path();
    for (name, bytes) in [
        ("binary", vec![0, 1]),
        ("encoding", vec![255]),
        ("empty", vec![]),
        ("large", vec![b'x'; 2 * 1024 * 1024 + 1]),
        ("lines", "x\n".repeat(10_001).into_bytes()),
    ] {
        std::fs::write(p.join(name), bytes).unwrap();
    }
    let oid = commit(p, "limits");
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    for name in ["binary", "encoding", "large", "lines"] {
        let result = engine
            .get_blame(source(&engine, &id, &oid, name).await)
            .await
            .unwrap();
        assert!(result.unavailable_reason.is_some(), "{name}");
        assert!(result.lines.is_empty());
    }
    let result = engine
        .get_blame(source(&engine, &id, &oid, "empty").await)
        .await
        .unwrap();
    assert!(result.unavailable_reason.is_none());
    assert!(result.lines.is_empty());
    let mut request = source(&engine, &id, &oid, "empty").await;
    request.file_id = "invented".into();
    assert!(engine.get_blame(request).await.is_err());
}

#[tokio::test]
async fn shallow_boundary_drift_and_grafts_are_not_invented_origins() {
    let dir = repo();
    let p = dir.path();
    std::fs::write(p.join("file"), "original\n").unwrap();
    let root = commit(p, "root");
    std::fs::write(p.join("file"), "original\nnew\n").unwrap();
    let tip = commit(p, "tip");
    std::fs::write(p.join(".git/shallow"), format!("{tip}\n")).unwrap();
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    let request = source(&engine, &id, &tip, "file").await;
    let result = engine.get_blame(request.clone()).await.unwrap();
    assert!(result.shallow);
    assert!(result.lines.iter().all(|l| l.boundary && l.oid == tip));
    std::fs::write(p.join(".git/shallow"), format!("{root}\n")).unwrap();
    assert!(engine
        .get_blame_commit(&id, &result.snapshot_id, &tip, None)
        .await
        .is_err());
    std::fs::create_dir_all(p.join(".git/info")).unwrap();
    std::fs::write(p.join(".git/info/grafts"), "").unwrap();
    assert!(engine.get_blame(request).await.is_err());
}

#[tokio::test]
async fn merge_comparison_parent_is_the_selected_parent() {
    let dir = repo();
    let p = dir.path();
    std::fs::write(p.join("file"), "base\n").unwrap();
    commit(p, "base");
    git(p, &["checkout", "-b", "side"]);
    std::fs::write(p.join("file"), "side\n").unwrap();
    let side = commit(p, "side");
    git(p, &["checkout", "main"]);
    std::fs::write(p.join("other"), "main\n").unwrap();
    let main = commit(p, "main");
    git(p, &["merge", "--no-ff", "side", "-m", "merge"]);
    let tip = git(p, &["rev-parse", "HEAD"]);
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    let mut request = source(&engine, &id, &tip, "file").await;
    let result = engine.get_blame(request.clone()).await.unwrap();
    assert_eq!(result.lines[0].oid, side);
    request.side = FileHistoryPath::Previous;
    let result = engine.get_blame(request).await.unwrap();
    assert_eq!(result.revision_oid, Some(main));
    assert_eq!(result.lines[0].content, "base");
    let details = engine
        .get_commit(&id, &tip, Some(side.clone()))
        .await
        .unwrap();
    let result = engine
        .get_blame(BlameRequest {
            repository_id: id,
            snapshot_id: details.snapshot_id,
            file_id: details.files[0].file_id.clone(),
            side: FileHistoryPath::Previous,
        })
        .await
        .unwrap();
    assert_eq!(result.revision_oid, Some(side));
    assert!(result.unavailable_reason.is_some());
}
