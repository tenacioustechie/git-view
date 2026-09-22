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

fn content() -> Vec<u8> {
    (1..=40)
        .map(|i| format!("line {i}\n"))
        .collect::<String>()
        .into_bytes()
}
fn fixture(name: &str) -> TempDir {
    let dir = repo();
    std::fs::write(dir.path().join(name), content()).unwrap();
    commit(dir.path(), "base");
    dir
}
async fn open(engine: &GitEngine, path: &Path) -> String {
    engine.open_repository(path).await.unwrap().repository_id
}
async fn diff(engine: &GitEngine, id: &str, path: &str, context: &str) -> (String, DiffResult) {
    let changes = engine.get_changes(id).await.unwrap();
    let file = changes
        .files
        .iter()
        .find(|f| f.display_path == path)
        .unwrap();
    let result = engine
        .get_diff(DiffRequest {
            repository_id: id.into(),
            snapshot_id: changes.snapshot_id,
            file_id: file.file_id.clone(),
            context: context.into(),
        })
        .await
        .unwrap();
    (file.file_id.clone(), result)
}
async fn hunks(
    engine: &GitEngine,
    id: &str,
    path: &str,
    context: &str,
) -> (String, String, Vec<DiffHunk>) {
    let (file, result) = diff(engine, id, path, context).await;
    match result {
        DiffResult::Text {
            hunk_actions:
                Some(HunkActions::Available {
                    diff_snapshot_id,
                    hunks,
                    ..
                }),
            ..
        } => (diff_snapshot_id, file, hunks),
        other => panic!("Expected hunk actions: {other:?}"),
    }
}
async fn plan(
    engine: &GitEngine,
    id: &str,
    path: &str,
    context: &str,
    index: usize,
) -> git_view::git::PreparedAction {
    let (snapshot, file, hunks) = hunks(engine, id, path, context).await;
    let action = if context == "unstaged" {
        RepositoryAction::StageHunk {
            diff_snapshot_id: snapshot,
            file_id: file,
            hunk_id: hunks[index].hunk_id.clone(),
        }
    } else {
        RepositoryAction::UnstageHunk {
            diff_snapshot_id: snapshot,
            file_id: file,
            hunk_id: hunks[index].hunk_id.clone(),
        }
    };
    engine.prepare_action(id, action).await.unwrap()
}
fn unavailable(result: DiffResult) -> String {
    match result {
        DiffResult::Text {
            hunk_actions: Some(HunkActions::Unavailable { reason }),
            ..
        }
        | DiffResult::Unavailable { reason } => reason,
        DiffResult::Binary { description } => description,
        other => panic!("Expected unavailable: {other:?}"),
    }
}
fn hook(path: &Path, body: &str) {
    let dir = path.join(".git/test-hooks");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("post-index-change");
    std::fs::write(&file, format!("#!/bin/sh\n{body}\n")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    git(path, &["config", "core.hooksPath", ".git/test-hooks"]);
}
#[tokio::test]
async fn stage_one_hunk_commit_and_unstage_preserve_other_edits() {
    let dir = fixture("file");
    let p = dir.path();
    let engine = GitEngine::isolated();
    let id = open(&engine, p).await;
    let working = String::from_utf8(content())
        .unwrap()
        .replace("line 4\n", "FOUR\n")
        .replace("line 30\n", "THIRTY\n");
    std::fs::write(p.join("file"), &working).unwrap();
    let action = plan(&engine, &id, "file", "unstaged", 1).await;
    engine.execute(&action).await.unwrap();
    let indexed = git(p, &["show", ":file"]);
    assert!(indexed.contains("line 4\n"));
    assert!(indexed.contains("THIRTY\n"));
    assert_eq!(std::fs::read_to_string(p.join("file")).unwrap(), working);
    git(p, &["commit", "-m", "only second hunk"]);
    assert!(git(p, &["show", "HEAD:file"]).contains("line 4\n"));
    assert!(git(p, &["diff", "--", "file"]).contains("+FOUR"));
    let stage = plan(&engine, &id, "file", "unstaged", 0).await;
    engine.execute(&stage).await.unwrap();
    std::fs::write(p.join("file"), format!("{working}unstaged tail\n")).unwrap();
    let unstage = plan(&engine, &id, "file", "staged", 0).await;
    engine.execute(&unstage).await.unwrap();
    assert_eq!(git(p, &["show", ":file"]), indexed);
    assert_eq!(
        std::fs::read_to_string(p.join("file")).unwrap(),
        format!("{working}unstaged tail\n")
    );
}
#[tokio::test]
async fn identical_context_hunks_use_exact_occurrence_and_stale_content_is_rejected() {
    let dir = repo();
    let p = dir.path();
    let block =
        "context0\ncontext1\ncontext2\nvalue\ncontext3\ncontext4\ncontext5\nseparator\nmore\nend\n";
    let base = block.repeat(3);
    std::fs::write(p.join("file"), &base).unwrap();
    commit(p, "repeated base");
    let engine = GitEngine::isolated();
    let id = open(&engine, p).await;
    let working = base.replacen("value", "VALUE", 2);
    std::fs::write(p.join("file"), &working).unwrap();
    let (snapshot, file, hs) = hunks(&engine, &id, "file", "unstaged").await;
    assert_eq!(hs.len(), 2);
    assert_ne!(hs[0].hunk_id, hs[1].hunk_id);
    assert_eq!(
        hs[0].text.split_once('\n').unwrap().1,
        hs[1].text.split_once('\n').unwrap().1
    );
    let action = engine
        .prepare_action(
            &id,
            RepositoryAction::StageHunk {
                diff_snapshot_id: snapshot.clone(),
                file_id: file.clone(),
                hunk_id: hs[1].hunk_id.clone(),
            },
        )
        .await
        .unwrap();
    engine.execute(&action).await.unwrap();
    assert_eq!(
        git(p, &["show", ":file"]),
        format!("{block}{}{block}", block.replace("value", "VALUE")).trim()
    );
    assert_eq!(
        engine
            .prepare_action(
                &id,
                RepositoryAction::StageHunk {
                    diff_snapshot_id: snapshot,
                    file_id: file,
                    hunk_id: hs[0].hunk_id.clone()
                }
            )
            .await
            .err()
            .unwrap()
            .code,
        "precondition_changed"
    );
    let action = plan(&engine, &id, "file", "unstaged", 0).await;
    std::fs::write(p.join("file"), working.replace("context0", "changed")).unwrap();
    let before = git(p, &["show", ":file"]);
    assert_eq!(
        engine.execute(&action).await.err().unwrap().code,
        "precondition_changed"
    );
    assert_eq!(git(p, &["show", ":file"]), before);
}
#[tokio::test]
async fn crlf_unicode_and_missing_newline_round_trip() {
    for (crlf, normalize, missing) in [
        (false, false, true),
        (true, false, false),
        (true, true, false),
    ] {
        let dir = repo();
        let p = dir.path();
        let name = "βeta tab\tfile";
        git(
            p,
            &[
                "config",
                "core.autocrlf",
                if normalize { "true" } else { "false" },
            ],
        );
        let mut base = String::from_utf8(content()).unwrap();
        if crlf {
            base = base.replace('\n', "\r\n");
        }
        if missing {
            base = base.trim_end_matches('\n').into();
        }
        std::fs::write(p.join(name), &base).unwrap();
        commit(p, "line endings");
        let original = fixture_command(p)
            .args(["show", &format!(":{name}")])
            .output()
            .unwrap()
            .stdout;
        let engine = GitEngine::isolated();
        let id = open(&engine, p).await;
        let working = base.replace("line 4", "FOUR").replace("line 30", "THIRTY");
        std::fs::write(p.join(name), &working).unwrap();
        let action = plan(&engine, &id, name, "unstaged", 1).await;
        engine.execute(&action).await.unwrap();
        assert_eq!(std::fs::read(p.join(name)).unwrap(), working.as_bytes());
        let unstage = plan(&engine, &id, name, "staged", 0).await;
        engine.execute(&unstage).await.unwrap();
        assert_eq!(
            fixture_command(p)
                .args(["show", &format!(":{name}")])
                .output()
                .unwrap()
                .stdout,
            original
        );
        assert_eq!(std::fs::read(p.join(name)).unwrap(), working.as_bytes());
    }
}
#[tokio::test]
async fn tracked_line_deletions_empty_files_and_insertions_are_supported() {
    for initial in [content(), vec![]] {
        let dir = repo();
        let p = dir.path();
        std::fs::write(p.join("file"), &initial).unwrap();
        commit(p, "base");
        let engine = GitEngine::isolated();
        let id = open(&engine, p).await;
        let working = if initial.is_empty() {
            b"inserted\n".to_vec()
        } else {
            vec![]
        };
        std::fs::write(p.join("file"), &working).unwrap();
        let action = plan(&engine, &id, "file", "unstaged", 0).await;
        engine.execute(&action).await.unwrap();
        assert_eq!(
            fixture_command(p)
                .args(["show", ":file"])
                .output()
                .unwrap()
                .stdout,
            working
        );
        assert!(p.join("file").exists());
        let action = plan(&engine, &id, "file", "staged", 0).await;
        engine.execute(&action).await.unwrap();
        assert_eq!(
            fixture_command(p)
                .args(["show", ":file"])
                .output()
                .unwrap()
                .stdout,
            initial
        );
        assert_eq!(std::fs::read(p.join("file")).unwrap(), working);
    }
}
#[tokio::test]
async fn user_diff_prefix_indicators_and_context_do_not_change_target() {
    let dir = fixture("file");
    let p = dir.path();
    std::fs::create_dir(p.join("dir")).unwrap();
    std::fs::write(p.join("dir/file"), content()).unwrap();
    commit(p, "nested same blob");
    git(p, &["config", "diff.noprefix", "true"]);
    git(p, &["config", "diff.mnemonicPrefix", "true"]);
    git(p, &["config", "diff.context", "100"]);
    git(p, &["config", "diff.interHunkContext", "100"]);
    git(p, &["config", "diff.suppressBlankEmpty", "true"]);
    git(p, &["config", "diff.outputIndicatorNew", "!"]);
    git(p, &["config", "diff.outputIndicatorOld", "?"]);
    git(p, &["config", "diff.relative", "true"]);
    let engine = GitEngine::isolated();
    let id = open(&engine, p).await;
    let working = String::from_utf8(content())
        .unwrap()
        .replace("line 4\n", "FOUR\n")
        .replace("line 30\n", "THIRTY\n");
    std::fs::write(p.join("dir/file"), &working).unwrap();
    let action = plan(&engine, &id, "dir/file", "unstaged", 1).await;
    engine.execute(&action).await.unwrap();
    assert_eq!(
        git(p, &["show", ":file"]),
        String::from_utf8(content()).unwrap().trim()
    );
    let indexed = git(p, &["show", ":dir/file"]);
    assert!(indexed.contains("THIRTY"));
    assert!(!indexed.contains("FOUR"));
    assert_eq!(
        std::fs::read_to_string(p.join("dir/file")).unwrap(),
        working
    );
}
#[tokio::test]
async fn hooks_run_only_on_real_mutation_and_unexpected_working_changes_are_reported() {
    let dir = fixture("file");
    let p = dir.path();
    let engine = GitEngine::isolated();
    let id = open(&engine, p).await;
    std::fs::write(
        p.join("file"),
        String::from_utf8(content())
            .unwrap()
            .replace("line 4", "FOUR"),
    )
    .unwrap();
    hook(
        p,
        "printf 'hook bytes' > file\nprintf 'ran' > .git/hunk-hook-ran",
    );
    let action = plan(&engine, &id, "file", "unstaged", 0).await;
    assert!(!p.join(".git/hunk-hook-ran").exists());
    let failure = engine.execute(&action).await.err().unwrap();
    assert_eq!(failure.code, "outcome_uncertain");
    assert_eq!(
        std::fs::read_to_string(p.join("file")).unwrap(),
        "hook bytes"
    );
    assert_eq!(
        std::fs::read_to_string(p.join(".git/hunk-hook-ran")).unwrap(),
        "ran"
    );
    assert!(git(p, &["show", ":file"]).contains("FOUR"));
}
#[tokio::test]
async fn unsupported_file_types_and_attributes_have_explicit_reasons() {
    let dir = fixture("file");
    let p = dir.path();
    let engine = GitEngine::isolated();
    let id = open(&engine, p).await;
    std::fs::write(p.join("new"), "new\n").unwrap();
    assert!(unavailable(diff(&engine, &id, "new", "unstaged").await.1).contains("whole-file"));
    git(p, &["add", "new"]);
    assert!(unavailable(diff(&engine, &id, "new", "staged").await.1).contains("whole-file"));
    std::fs::remove_file(p.join("file")).unwrap();
    assert!(unavailable(diff(&engine, &id, "file", "unstaged").await.1).contains("whole-file"));
    std::fs::write(p.join("file"), b"binary\0changed").unwrap();
    assert!(unavailable(diff(&engine, &id, "file", "unstaged").await.1)
        .to_lowercase()
        .contains("binary"));
    std::fs::write(p.join("file"), b"changed\n").unwrap();
    for attribute in [
        "filter=custom",
        "ident",
        "working-tree-encoding=UTF-8",
        "diff=custom",
    ] {
        std::fs::write(
            p.join(".git/info/attributes"),
            format!("file {attribute}\n"),
        )
        .unwrap();
        assert!(
            unavailable(diff(&engine, &id, "file", "unstaged").await.1).contains("attribute"),
            "{attribute}"
        );
    }
}
#[tokio::test]
async fn attributes_index_flags_config_and_context_are_revalidated() {
    let dir = fixture("file");
    let p = dir.path();
    let engine = GitEngine::isolated();
    let id = open(&engine, p).await;
    std::fs::write(p.join("file"), b"changed\n").unwrap();
    let action = plan(&engine, &id, "file", "unstaged", 0).await;
    std::fs::write(p.join(".git/info/attributes"), "file text eol=lf\n").unwrap();
    assert_eq!(
        engine.execute(&action).await.err().unwrap().code,
        "precondition_changed"
    );
    let (snapshot, file, hs) = hunks(&engine, &id, "file", "unstaged").await;
    assert!(engine
        .prepare_action(
            &id,
            RepositoryAction::UnstageHunk {
                diff_snapshot_id: snapshot,
                file_id: file,
                hunk_id: hs[0].hunk_id.clone()
            }
        )
        .await
        .is_err());
    let action = plan(&engine, &id, "file", "unstaged", 0).await;
    git(p, &["config", "apply.whitespace", "fix"]);
    assert_eq!(
        engine.execute(&action).await.err().unwrap().code,
        "precondition_changed"
    );
    let action = plan(&engine, &id, "file", "unstaged", 0).await;
    git(p, &["update-index", "--assume-unchanged", "file"]);
    assert_eq!(
        engine.execute(&action).await.err().unwrap().code,
        "precondition_changed"
    );
}
