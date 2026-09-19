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
fn write(path: &Path, name: &str, body: &str) {
    std::fs::write(path.join(name), body).unwrap();
}
fn commit(path: &Path, message: &str) {
    git(path, &["add", "--all"]);
    git(path, &["commit", "--allow-empty", "-m", message]);
}
fn query(field: HistorySearchField, text: &str) -> HistorySearch {
    HistorySearch {
        field,
        text: text.into(),
    }
}
async fn search(
    engine: &GitEngine,
    id: &str,
    field: HistorySearchField,
    text: &str,
) -> AppResult<HistoryPage> {
    engine
        .search_history(id, query(field, text), None, None, None)
        .await
}
fn many(path: &Path, count: usize) {
    use std::io::Write;
    let mut child = fixture_command(path)
        .arg("fast-import")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    for n in 0..count {
        let message = format!("Match item {n}\n\nSearch body\n");
        write!(input,"commit refs/heads/main\ncommitter Fixture <fixture@example.invalid> {} +0000\ndata {}\n{}\n",1700000000+n,message.len(),message).unwrap();
    }
    drop(input);
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[tokio::test]
async fn message_and_author_search_are_literal_case_insensitive_and_include_body_email_unicode() {
    let dir = repo();
    let p = dir.path();
    git(p, &["config", "user.name", "Äuthor Ωmega"]);
    git(p, &["config", "user.email", "Alpha@example.invalid"]);
    commit(
        p,
        "Subject [a.b] -leading\n\nBody CAFÉ ΩMEGA literal .* $(unsafe)",
    );
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    for text in [
        "[a.b]",
        "-leading",
        "café",
        "ωmega",
        "literal .*",
        "$(unsafe)",
        "body",
    ] {
        assert_eq!(
            search(&engine, &id, HistorySearchField::Message, text)
                .await
                .unwrap()
                .commits
                .len(),
            1,
            "{text}"
        );
    }
    assert!(search(&engine, &id, HistorySearchField::Message, "[a.c]")
        .await
        .unwrap()
        .commits
        .is_empty());
    for text in ["äuthor", "ωMEGA", "ALPHA@EXAMPLE.INVALID"] {
        assert_eq!(
            search(&engine, &id, HistorySearchField::Author, text)
                .await
                .unwrap()
                .commits
                .len(),
            1,
            "{text}"
        );
    }
    assert!(search(&engine, &id, HistorySearchField::Author, "+0000")
        .await
        .unwrap()
        .commits
        .is_empty());
}
#[tokio::test]
async fn filtered_pages_keep_frozen_tips_query_scope_and_all_issued_cursors() {
    let dir = repo();
    let p = dir.path();
    many(p, 405);
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    let q = query(HistorySearchField::Message, "match");
    let first = engine
        .search_history(&id, q.clone(), Some("head".into()), None, None)
        .await
        .unwrap();
    assert_eq!(first.commits.len(), 200);
    let (a, b) = tokio::join!(
        engine.search_history(
            &id,
            q.clone(),
            Some("HEAD".into()),
            Some(first.snapshot_id.clone()),
            first.next_cursor.clone()
        ),
        engine.search_history(
            &id,
            q.clone(),
            Some("head".into()),
            Some(first.snapshot_id.clone()),
            first.next_cursor.clone()
        )
    );
    let a = a.unwrap();
    let b = b.unwrap();
    assert_eq!(a.commits.len(), 200);
    assert_eq!(a.commits[0].oid, b.commits[0].oid);
    commit(p, "Match outside frozen snapshot");
    for cursor in [a.next_cursor, b.next_cursor] {
        let page = engine
            .search_history(
                &id,
                q.clone(),
                Some("head".into()),
                Some(first.snapshot_id.clone()),
                cursor,
            )
            .await
            .unwrap();
        assert_eq!(page.commits.len(), 5);
        assert!(page.next_cursor.is_none());
    }
    assert_eq!(
        engine
            .search_history(
                &id,
                query(HistorySearchField::Message, "different"),
                Some("head".into()),
                Some(first.snapshot_id.clone()),
                None
            )
            .await
            .err()
            .unwrap()
            .code,
        "incompatible_snapshot"
    );
    assert_eq!(
        engine
            .search_history(&id, q.clone(), None, Some(first.snapshot_id.clone()), None)
            .await
            .err()
            .unwrap()
            .code,
        "incompatible_snapshot"
    );
    assert_eq!(
        engine
            .get_history(
                &id,
                Some("head".into()),
                Some(first.snapshot_id.clone()),
                None
            )
            .await
            .err()
            .unwrap()
            .code,
        "incompatible_snapshot"
    );
    assert_eq!(
        engine
            .search_history(
                &id,
                q.clone(),
                Some("head".into()),
                Some(first.snapshot_id.clone()),
                Some("invented".into())
            )
            .await
            .err()
            .unwrap()
            .code,
        "invalid_cursor"
    );
    let another = search(&engine, &id, HistorySearchField::Message, "match")
        .await
        .unwrap();
    assert_eq!(
        engine
            .search_history(
                &id,
                q.clone(),
                None,
                Some(another.snapshot_id),
                first.next_cursor
            )
            .await
            .err()
            .unwrap()
            .code,
        "invalid_cursor"
    );
    assert_eq!(
        engine
            .search_history(&id, q, None, Some("expired".into()), None)
            .await
            .err()
            .unwrap()
            .code,
        "stale_snapshot"
    );
}
#[tokio::test]
async fn oid_search_is_scoped_reachable_and_reports_real_prefix_ambiguity() {
    let dir = repo();
    let p = dir.path();
    many(p, 2048);
    let oids = git(p, &["rev-list", "HEAD"]);
    let mut prefixes = std::collections::HashMap::<String, Vec<String>>::new();
    for oid in oids.lines() {
        prefixes
            .entry(oid[..4].into())
            .or_default()
            .push(oid.into());
    }
    let (prefix, pair) = prefixes
        .into_iter()
        .find(|(_, oids)| oids.len() == 2)
        .expect("Deterministic fixture has a four-digit collision");
    git(p, &["branch", "limited", &pair[1]]);
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    assert_eq!(
        search(&engine, &id, HistorySearchField::Oid, &prefix)
            .await
            .err()
            .unwrap()
            .code,
        "ambiguous_oid"
    );
    let scoped = engine
        .search_history(
            &id,
            query(HistorySearchField::Oid, &prefix),
            Some("refs/heads/limited".into()),
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(scoped.commits.len(), 1);
    assert_eq!(scoped.commits[0].oid, pair[1]);
    let full = search(
        &engine,
        &id,
        HistorySearchField::Oid,
        &pair[0].to_uppercase(),
    )
    .await
    .unwrap();
    assert_eq!(full.commits[0].oid, pair[0]);
    assert!(
        search(&engine, &id, HistorySearchField::Oid, &"f".repeat(64))
            .await
            .unwrap()
            .commits
            .is_empty()
    );
}
#[tokio::test]
async fn all_scope_excludes_backup_stash_and_unreachable_objects() {
    let dir = repo();
    let p = dir.path();
    commit(p, "Visible");
    let tree = git(p, &["rev-parse", "HEAD^{tree}"]);
    let hidden = git(p, &["commit-tree", &tree, "-m", "Hidden needle"]);
    git(p, &["update-ref", "refs/git-view/backups/fixture", &hidden]);
    git(p, &["update-ref", "refs/stash", &hidden]);
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    assert!(search(&engine, &id, HistorySearchField::Message, "hidden")
        .await
        .unwrap()
        .commits
        .is_empty());
    assert!(search(&engine, &id, HistorySearchField::Oid, &hidden)
        .await
        .unwrap()
        .commits
        .is_empty());
    git(p, &["branch", "topic", &hidden]);
    assert_eq!(
        search(&engine, &id, HistorySearchField::Message, "hidden")
            .await
            .unwrap()
            .commits
            .len(),
        1
    );
    assert!(engine
        .search_history(
            &id,
            query(HistorySearchField::Oid, &hidden),
            Some("head".into()),
            None,
            None
        )
        .await
        .unwrap()
        .commits
        .is_empty());
}
#[tokio::test]
async fn search_keeps_actual_ordered_merge_parents_and_filtered_gaps() {
    let dir = repo();
    let p = dir.path();
    commit(p, "needle root");
    let root = git(p, &["rev-parse", "HEAD"]);
    commit(p, "middle omitted");
    let middle = git(p, &["rev-parse", "HEAD"]);
    git(p, &["switch", "-c", "side"]);
    write(p, "side", "side");
    commit(p, "side omitted");
    let side = git(p, &["rev-parse", "HEAD"]);
    git(p, &["switch", "main"]);
    write(p, "main", "main");
    commit(p, "main omitted");
    let main = git(p, &["rev-parse", "HEAD"]);
    git(p, &["merge", "--no-ff", "side", "-m", "needle merge"]);
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    let page = search(&engine, &id, HistorySearchField::Message, "needle")
        .await
        .unwrap();
    assert_eq!(page.commits.len(), 2);
    assert_eq!(page.commits[0].parents, vec![main, side]);
    assert_eq!(page.commits[1].oid, root);
    assert!(page.commits[1].parents.is_empty());
    let filtered = search(&engine, &id, HistorySearchField::Message, "side omitted")
        .await
        .unwrap();
    assert_eq!(filtered.commits[0].parents, vec![middle]);
}
#[tokio::test]
async fn query_validation_and_unborn_history_are_explicit() {
    let dir = repo();
    let engine = GitEngine::isolated();
    let id = engine
        .open_repository(dir.path())
        .await
        .unwrap()
        .repository_id;
    for text in [
        "",
        " ",
        "nul\0byte",
        "line\nbreak",
        "return\rbreak",
        &"é".repeat(257),
    ] {
        assert_eq!(
            search(&engine, &id, HistorySearchField::Message, text)
                .await
                .err()
                .unwrap()
                .code,
            "invalid_query"
        );
    }
    for text in [
        "abc",
        "HEAD",
        "--all",
        "abcd^",
        "abcd:filename",
        &"a".repeat(65),
    ] {
        assert_eq!(
            search(&engine, &id, HistorySearchField::Oid, text)
                .await
                .err()
                .unwrap()
                .code,
            "invalid_query"
        );
    }
    for (field, text) in [
        (HistorySearchField::Message, "anything"),
        (HistorySearchField::Author, "name"),
        (HistorySearchField::Oid, "1234"),
    ] {
        assert!(search(&engine, &id, field, text)
            .await
            .unwrap()
            .commits
            .is_empty());
    }
    assert_eq!(
        engine
            .search_history(
                &id,
                query(HistorySearchField::Message, "a"),
                None,
                None,
                Some("cursor".into())
            )
            .await
            .err()
            .unwrap()
            .code,
        "invalid_cursor"
    );
}

#[tokio::test]
async fn author_paging_excludes_date_only_matches_and_preserves_exact_results() {
    use std::io::Write;
    let dir = repo();
    let p = dir.path();
    let mut child = fixture_command(p)
        .arg("fast-import")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    for n in 0..610 {
        let author = if n % 3 == 0 {
            "1700000 Person"
        } else {
            "Other Person"
        };
        let message = format!("Item {n}\n");
        write!(input,"commit refs/heads/main\nauthor {author} <fixture@example.invalid> {} +0000\ncommitter Fixture <fixture@example.invalid> {} +0000\ndata {}\n{}\n",1700000000+n,1700000000+n,message.len(),message).unwrap();
    }
    drop(input);
    assert!(child.wait_with_output().unwrap().status.success());
    let expected = git(p, &["log", "--format=%H %an", "HEAD"])
        .lines()
        .filter(|line| line.contains("1700000 Person"))
        .map(|line| line.split_whitespace().next().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(expected.len(), 204);
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    let first = search(&engine, &id, HistorySearchField::Author, "1700000")
        .await
        .unwrap();
    assert_eq!(first.commits.len(), 200);
    let second = engine
        .search_history(
            &id,
            query(HistorySearchField::Author, "1700000"),
            None,
            Some(first.snapshot_id),
            first.next_cursor,
        )
        .await
        .unwrap();
    assert_eq!(second.commits.len(), 4);
    assert!(second.next_cursor.is_none());
    let actual = first
        .commits
        .into_iter()
        .chain(second.commits)
        .map(|c| c.oid)
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}
#[tokio::test]
async fn search_details_and_patch_ignore_replacements_consistently() {
    let dir = repo();
    let p = dir.path();
    write(p, "original.txt", "Original needle bytes\n");
    commit(p, "Original needle message");
    let original = git(p, &["rev-parse", "HEAD"]);
    git(p, &["switch", "--orphan", "replacement"]);
    write(p, "spoof.txt", "Spoofed content\n");
    commit(p, "Spoofed message");
    let replacement = git(p, &["rev-parse", "HEAD"]);
    git(p, &["switch", "main"]);
    git(p, &["replace", &original, &replacement]);
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    let page = engine
        .search_history(
            &id,
            query(HistorySearchField::Message, "original needle"),
            Some("head".into()),
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(page.commits.len(), 1);
    assert_eq!(page.commits[0].oid, original);
    assert_eq!(page.commits[0].subject, "Original needle message");
    let details = engine.get_commit(&id, &original, None).await.unwrap();
    assert!(details.parents.is_empty());
    assert_eq!(details.files.len(), 1);
    assert_eq!(details.files[0].display_path, "original.txt");
    assert!(details.message.contains("Original needle"));
    let patch = engine
        .get_diff(DiffRequest {
            repository_id: id,
            snapshot_id: details.snapshot_id,
            context: "commit".into(),
            file_id: details.files[0].file_id.clone(),
        })
        .await
        .unwrap();
    assert!(
        matches!(patch,DiffResult::Text{text,..} if text.contains("Original needle bytes") && !text.contains("Spoofed"))
    );
}
#[tokio::test]
async fn shallow_boundaries_and_depth_drift_are_honest() {
    let source = repo();
    commit(source.path(), "needle root");
    commit(source.path(), "needle tip");
    let root = git(source.path(), &["rev-parse", "HEAD^"]);
    let shallow = tempfile::tempdir().unwrap();
    git(
        shallow.path(),
        &[
            "clone",
            "--depth=1",
            "-b",
            "main",
            &format!("file://{}", source.path().to_string_lossy()),
            ".",
        ],
    );
    let engine = GitEngine::isolated();
    let id = engine
        .open_repository(shallow.path())
        .await
        .unwrap()
        .repository_id;
    let page = search(&engine, &id, HistorySearchField::Message, "needle")
        .await
        .unwrap();
    assert_eq!(page.commits.len(), 1);
    assert!(page.commits[0].boundary);
    assert_eq!(page.commits[0].parents, vec![root]);
    git(shallow.path(), &["fetch", "--unshallow"]);
    assert_eq!(
        engine
            .search_history(
                &id,
                query(HistorySearchField::Message, "needle"),
                None,
                Some(page.snapshot_id),
                None
            )
            .await
            .err()
            .unwrap()
            .code,
        "stale_snapshot"
    );
    assert_eq!(
        search(&engine, &id, HistorySearchField::Message, "needle")
            .await
            .unwrap()
            .commits
            .len(),
        2
    );
}
#[tokio::test]
async fn missing_snapshot_objects_return_error_instead_of_empty_results() {
    let dir = repo();
    let p = dir.path();
    commit(p, "needle root");
    let root = git(p, &["rev-parse", "HEAD"]);
    commit(p, "needle tip");
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    let page = search(&engine, &id, HistorySearchField::Message, "needle")
        .await
        .unwrap();
    let object = p.join(".git/objects").join(&root[..2]).join(&root[2..]);
    #[cfg(windows)]
    {
        let mut permissions = std::fs::metadata(&object).unwrap().permissions();
        permissions.set_readonly(false);
        std::fs::set_permissions(&object, permissions).unwrap();
    }
    std::fs::remove_file(object).unwrap();
    assert!(engine
        .search_history(
            &id,
            query(HistorySearchField::Message, "needle"),
            None,
            Some(page.snapshot_id),
            None
        )
        .await
        .is_err());
}
#[tokio::test]
async fn all_history_reads_preserve_index_and_completed_squash_journal() {
    let dir = repo();
    let p = dir.path();
    write(p, "a", "base\n");
    commit(p, "needle base");
    let head = git(p, &["rev-parse", "HEAD"]);
    let engine = GitEngine::isolated();
    let id = engine.open_repository(p).await.unwrap().repository_id;
    let bytes=serde_json::to_vec(&serde_json::json!({"version":1,"operationId":"history-fixture","phase":"review","originalBranch":"refs/heads/main","state":{"originalHead":head,"sourceOid":head,"sourceRef":"refs/heads/source"}})).unwrap();
    let marker = p.join(".git/git-view-squash.json");
    std::fs::write(&marker, &bytes).unwrap();
    let index = std::fs::read(p.join(".git/index")).unwrap();
    assert_eq!(
        search(&engine, &id, HistorySearchField::Message, "needle")
            .await
            .unwrap()
            .commits
            .len(),
        1
    );
    assert_eq!(
        engine
            .get_history(&id, None, None, None)
            .await
            .unwrap()
            .commits
            .len(),
        1
    );
    assert_eq!(std::fs::read(marker).unwrap(), bytes);
    assert_eq!(std::fs::read(p.join(".git/index")).unwrap(), index);
    assert_eq!(git(p, &["rev-parse", "HEAD"]), head);
}
