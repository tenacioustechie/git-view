use git_view::{contracts::*, git::GitEngine};
use std::{path::Path, process::Command};
use tempfile::TempDir;
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
fn hook(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
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
async fn combine(
    engine: &GitEngine,
    id: &str,
    oids: Vec<String>,
) -> AppResult<git_view::git::ActionOutcome> {
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
async fn recover(
    engine: &GitEngine,
    id: &str,
    recovery_id: String,
) -> AppResult<git_view::git::ActionOutcome> {
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
async fn combines_exact_suffix_and_recovers_preserving_dirty_bytes() {
    let dir = linear();
    let path = dir.path();
    let oids = suffix(path);
    let old = oids[0].clone();
    let base = git(path, &["rev-parse", "HEAD~2"]);
    let tree = git(path, &["rev-parse", "HEAD^{tree}"]);
    let original_index = std::fs::read(path.join(".git/index")).unwrap();
    let engine = GitEngine::isolated();
    let id = engine.open_repository(path).await.unwrap().repository_id;
    combine(&engine, &id, oids).await.unwrap();
    assert_ne!(git(path, &["rev-parse", "HEAD"]), old);
    assert_eq!(git(path, &["show", "-s", "--format=%P", "HEAD"]), base);
    assert_eq!(git(path, &["rev-parse", "HEAD^{tree}"]), tree);
    assert_eq!(
        git(path, &["show", "-s", "--format=%B", "HEAD"]),
        "Combined message\n\nDetails"
    );
    assert_eq!(
        std::fs::read(path.join(".git/index")).unwrap(),
        original_index
    );
    let record = engine.list_rewrites(&id).await.unwrap().remove(0);
    assert_eq!(record.phase, "installed");
    assert!(record.can_recover);
    assert_eq!(git(path, &["rev-parse", &record.backup_ref]), old);
    write(path, "a", "staged\n");
    git(path, &["add", "a"]);
    write(path, "a", "unstaged\n");
    write(path, "[literal]", "untracked\n");
    let index = std::fs::read(path.join(".git/index")).unwrap();
    recover(&engine, &id, record.recovery_id).await.unwrap();
    assert_eq!(git(path, &["rev-parse", "HEAD"]), old);
    assert_eq!(std::fs::read(path.join(".git/index")).unwrap(), index);
    assert_eq!(
        std::fs::read_to_string(path.join("a")).unwrap(),
        "unstaged\n"
    );
    assert_eq!(
        std::fs::read_to_string(path.join("[literal]")).unwrap(),
        "untracked\n"
    );
    assert_eq!(
        engine.list_rewrites(&id).await.unwrap()[0].phase,
        "recovered"
    );
}
#[tokio::test]
async fn net_empty_suffix_still_creates_one_commit() {
    let dir = repo();
    write(dir.path(), "a", "second\n");
    commit(dir.path(), "second");
    write(dir.path(), "a", "base\n");
    commit(dir.path(), "revert");
    let engine = GitEngine::isolated();
    let id = engine
        .open_repository(dir.path())
        .await
        .unwrap()
        .repository_id;
    combine(&engine, &id, suffix(dir.path())).await.unwrap();
    assert_eq!(git(dir.path(), &["rev-list", "--count", "HEAD"]), "2");
    assert_eq!(git(dir.path(), &["diff", "HEAD^", "HEAD"]), "");
}
#[tokio::test]
async fn rejecting_relative_hook_preserves_original_history_and_index() {
    let dir = linear();
    let path = dir.path();
    let old = git(path, &["rev-parse", "HEAD"]);
    let index = std::fs::read(path.join(".git/index")).unwrap();
    hook(
        &path.join(".git/test-hooks/pre-commit"),
        "#!/bin/sh\necho rejecting >&2\nexit 1\n",
    );
    let engine = GitEngine::isolated();
    let id = engine.open_repository(path).await.unwrap().repository_id;
    assert!(combine(&engine, &id, suffix(path)).await.is_err());
    assert_eq!(git(path, &["rev-parse", "HEAD"]), old);
    assert_eq!(std::fs::read(path.join(".git/index")).unwrap(), index);
    let record = engine.list_rewrites(&id).await.unwrap().remove(0);
    assert_eq!(record.phase, "failed");
    assert_eq!(git(path, &["rev-parse", &record.backup_ref]), old);
}

#[tokio::test]
async fn invalid_selections_fail_before_backup_creation() {
    let dir = linear();
    let path = dir.path();
    let oids = suffix(path);
    let base = git(path, &["rev-parse", "HEAD~2"]);
    let engine = GitEngine::isolated();
    let id = engine.open_repository(path).await.unwrap().repository_id;
    for invalid in [
        vec![oids[0].clone()],
        vec![oids[0].clone(), oids[0].clone()],
        vec![oids[1].clone(), oids[0].clone()],
        vec![oids[0].clone(), base.clone()],
        vec![oids[0].clone(), oids[1].clone(), base],
    ] {
        assert_eq!(
            combine(&engine, &id, invalid).await.err().unwrap().code,
            "invalid_selection"
        );
        assert!(engine.list_rewrites(&id).await.unwrap().is_empty());
    }
    write(path, ".git/shallow", &format!("{}\n", oids[1]));
    assert_eq!(
        combine(&engine, &id, oids).await.err().unwrap().code,
        "invalid_selection"
    );
}
#[tokio::test]
async fn dirty_and_detached_contexts_fail_before_backup_creation() {
    let dir = linear();
    let path = dir.path();
    let engine = GitEngine::isolated();
    let id = engine.open_repository(path).await.unwrap().repository_id;
    write(path, "untracked", "bytes");
    assert_eq!(
        combine(&engine, &id, suffix(path))
            .await
            .err()
            .unwrap()
            .code,
        "dirty"
    );
    std::fs::remove_file(path.join("untracked")).unwrap();
    git(path, &["switch", "--detach"]);
    assert_eq!(
        combine(&engine, &id, suffix(path))
            .await
            .err()
            .unwrap()
            .code,
        "invalid_head"
    );
    assert!(engine.list_rewrites(&id).await.unwrap().is_empty());
}
#[tokio::test]
async fn hook_altered_candidate_tree_is_rejected() {
    let dir = linear();
    let path = dir.path();
    let old = git(path, &["rev-parse", "HEAD"]);
    hook(
        &path.join(".git/test-hooks/pre-commit"),
        "#!/bin/sh\nprintf altered > hook-file\ngit add hook-file\n",
    );
    let engine = GitEngine::isolated();
    let id = engine.open_repository(path).await.unwrap().repository_id;
    assert_eq!(
        combine(&engine, &id, suffix(path))
            .await
            .err()
            .unwrap()
            .code,
        "rewrite_manual_review"
    );
    assert_eq!(git(path, &["rev-parse", "HEAD"]), old);
    assert!(!path.join("hook-file").exists());
}
#[tokio::test]
async fn conditional_signing_policy_is_honored_in_detached_candidate() {
    let dir = linear();
    let path = dir.path();
    let old = git(path, &["rev-parse", "HEAD"]);
    write(
        path,
        ".git/signing-policy",
        "[commit]\n gpgSign = true\n[gpg]\n program = git-view-fixture-missing-signer\n",
    );
    git(
        path,
        &["config", "includeIf.onbranch:main.path", "signing-policy"],
    );
    let engine = GitEngine::isolated();
    let id = engine.open_repository(path).await.unwrap().repository_id;
    assert!(combine(&engine, &id, suffix(path)).await.is_err());
    assert_eq!(git(path, &["rev-parse", "HEAD"]), old);
    assert_eq!(engine.list_rewrites(&id).await.unwrap()[0].phase, "failed");
}
#[tokio::test]
async fn worktree_identity_configuration_is_preserved() {
    let dir = linear();
    let path = dir.path();
    git(path, &["config", "extensions.worktreeConfig", "true"]);
    git(
        path,
        &["config", "--worktree", "user.name", "Worktree Identity"],
    );
    let engine = GitEngine::isolated();
    let id = engine.open_repository(path).await.unwrap().repository_id;
    combine(&engine, &id, suffix(path)).await.unwrap();
    assert_eq!(
        git(path, &["show", "-s", "--format=%an", "HEAD"]),
        "Worktree Identity"
    );
}
#[tokio::test]
async fn later_commit_blocks_recovery_and_retains_both_histories() {
    let dir = linear();
    let path = dir.path();
    let engine = GitEngine::isolated();
    let id = engine.open_repository(path).await.unwrap().repository_id;
    combine(&engine, &id, suffix(path)).await.unwrap();
    let record = engine.list_rewrites(&id).await.unwrap().remove(0);
    write(path, "later", "later");
    commit(path, "later");
    let later = git(path, &["rev-parse", "HEAD"]);
    assert!(recover(&engine, &id, record.recovery_id).await.is_err());
    assert_eq!(git(path, &["rev-parse", "HEAD"]), later);
    assert_eq!(
        git(path, &["rev-parse", &record.backup_ref]),
        record.original_head
    );
    assert_eq!(
        git(
            path,
            &["rev-parse", record.replacement_ref.as_ref().unwrap()]
        ),
        record.replacement_head.unwrap()
    );
    assert!(!engine.list_rewrites(&id).await.unwrap()[0].can_recover);
}
#[tokio::test]
async fn changed_or_corrupt_journal_after_preview_is_preserved_exactly() {
    for corrupt in [false, true] {
        let dir = linear();
        let path = dir.path();
        let engine = GitEngine::isolated();
        let id = engine.open_repository(path).await.unwrap().repository_id;
        combine(&engine, &id, suffix(path)).await.unwrap();
        let record = engine.list_rewrites(&id).await.unwrap().remove(0);
        let replacement = record.replacement_head.unwrap();
        let status = engine.get_status(&id).await.unwrap();
        let plan = engine
            .prepare_action(
                &id,
                RepositoryAction::RecoverCombine {
                    status_snapshot_id: status.snapshot_id,
                    recovery_id: record.recovery_id.clone(),
                },
            )
            .await
            .unwrap();
        let journal = path.join(format!(
            ".git/git-view/rewrites/{}.json",
            record.recovery_id
        ));
        let bytes = std::fs::read(&journal).unwrap();
        let changed = if corrupt {
            b"{ corrupt".to_vec()
        } else {
            let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            value["record"]["message"] = "external edit".into();
            serde_json::to_vec(&value).unwrap()
        };
        std::fs::write(&journal, &changed).unwrap();
        assert!(engine.execute(&plan).await.is_err());
        assert_eq!(std::fs::read(&journal).unwrap(), changed);
        assert_eq!(git(path, &["rev-parse", "HEAD"]), replacement);
    }
}
fn drift_hook(path: &Path) {
    git(
        path,
        &["config", "test.gitViewOriginalRoot", path.to_str().unwrap()],
    );
    hook(&path.join(".git/test-hooks/reference-transaction"),"#!/bin/sh\n[ \"$1\" = prepared ] || exit 0\noriginal=$(git config --get test.gitViewOriginalRoot) || exit 1\nwhile read -r old new ref; do\n if [ \"$ref\" = refs/heads/main ] && [ \"$old\" != \"$new\" ]; then\n  git -C \"$original\" config test.gitViewDrift changed || exit 1\n fi\ndone\nexit 0\n");
}
#[tokio::test]
async fn prepared_reference_hook_config_drift_blocks_install_and_recovery() {
    for recovering in [false, true] {
        let dir = linear();
        let path = dir.path();
        let engine = GitEngine::isolated();
        let id = engine.open_repository(path).await.unwrap().repository_id;
        if recovering {
            combine(&engine, &id, suffix(path)).await.unwrap();
        }
        let before = git(path, &["rev-parse", "HEAD"]);
        drift_hook(path);
        let error = if recovering {
            let record = engine.list_rewrites(&id).await.unwrap().remove(0);
            recover(&engine, &id, record.recovery_id)
                .await
                .err()
                .unwrap()
        } else {
            combine(&engine, &id, suffix(path)).await.err().unwrap()
        };
        assert_eq!(error.code, "precondition_changed", "{error:?}");
        assert_eq!(git(path, &["rev-parse", "HEAD"]), before);
    }
}
#[tokio::test]
async fn symbolic_backup_is_never_accepted_as_durable_anchor() {
    let dir = linear();
    let path = dir.path();
    let engine = GitEngine::isolated();
    let id = engine.open_repository(path).await.unwrap().repository_id;
    combine(&engine, &id, suffix(path)).await.unwrap();
    let record = engine.list_rewrites(&id).await.unwrap().remove(0);
    let before = git(path, &["rev-parse", "HEAD"]);
    git(
        path,
        &["branch", "retained-original", &record.original_head],
    );
    git(
        path,
        &[
            "symbolic-ref",
            &record.backup_ref,
            "refs/heads/retained-original",
        ],
    );
    assert!(recover(&engine, &id, record.recovery_id).await.is_err());
    assert_eq!(git(path, &["rev-parse", "HEAD"]), before);
}

#[tokio::test]
async fn raw_candidate_parent_cannot_be_spoofed_by_replacement_refs() {
    let dir = linear();
    let path = dir.path();
    let old = git(path, &["rev-parse", "HEAD"]);
    hook(&path.join(".git/test-hooks/post-commit"),"#!/bin/sh\nfirst=$(git rev-parse HEAD)\ngit -c core.hooksPath=/nonexistent-git-view-fixture-hooks commit --allow-empty -m extra || exit 1\nsecond=$(git rev-parse HEAD)\ngit replace \"$second\" \"$first\"\n");
    let engine = GitEngine::isolated();
    let id = engine.open_repository(path).await.unwrap().repository_id;
    assert_eq!(
        combine(&engine, &id, suffix(path))
            .await
            .err()
            .unwrap()
            .code,
        "rewrite_manual_review"
    );
    assert_eq!(git(path, &["rev-parse", "HEAD"]), old);
}
#[tokio::test]
async fn hook_changes_to_original_branch_or_files_block_install_without_discarding_them() {
    for branch_switch in [false, true] {
        let dir = linear();
        let path = dir.path();
        let old = git(path, &["rev-parse", "HEAD"]);
        git(path, &["branch", "other"]);
        git(
            path,
            &["config", "test.gitViewOriginalRoot", path.to_str().unwrap()],
        );
        let command = if branch_switch {
            "git -C \"$original\" switch other"
        } else {
            "printf external > \"$original/a\"; git -C \"$original\" add a"
        };
        hook(&path.join(".git/test-hooks/pre-commit"),&format!("#!/bin/sh\noriginal=$(git config --get test.gitViewOriginalRoot) || exit 1\nunset GIT_INDEX_FILE GIT_DIR GIT_WORK_TREE GIT_PREFIX\n{command}\n"));
        let engine = GitEngine::isolated();
        let id = engine.open_repository(path).await.unwrap().repository_id;
        assert_eq!(
            combine(&engine, &id, suffix(path))
                .await
                .err()
                .unwrap()
                .code,
            "precondition_changed"
        );
        assert_eq!(git(path, &["rev-parse", "main"]), old);
        if branch_switch {
            assert_eq!(git(path, &["symbolic-ref", "--short", "HEAD"]), "other");
        } else {
            assert_eq!(std::fs::read_to_string(path.join("a")).unwrap(), "external");
            assert_eq!(git(path, &["show", ":a"]), "external");
        }
    }
}
#[tokio::test]
async fn reference_transaction_rejection_keeps_original_tip() {
    let dir = linear();
    let path = dir.path();
    let old = git(path, &["rev-parse", "HEAD"]);
    hook(&path.join(".git/test-hooks/reference-transaction"),"#!/bin/sh\n[ \"$1\" = prepared ] || exit 0\nwhile read -r old new ref; do\n if [ \"$ref\" = refs/heads/main ] && [ \"$old\" != \"$new\" ]; then exit 1; fi\ndone\nexit 0\n");
    let engine = GitEngine::isolated();
    let id = engine.open_repository(path).await.unwrap().repository_id;
    assert!(combine(&engine, &id, suffix(path)).await.is_err());
    assert_eq!(git(path, &["rev-parse", "HEAD"]), old);
    assert!(!path.join(".git/HEAD.lock").exists());
}
#[tokio::test]
async fn sparse_boolean_spellings_are_rejected() {
    let dir = linear();
    let path = dir.path();
    git(path, &["config", "core.sparseCheckout", "yes"]);
    let engine = GitEngine::isolated();
    let id = engine.open_repository(path).await.unwrap().repository_id;
    assert_eq!(
        combine(&engine, &id, suffix(path))
            .await
            .err()
            .unwrap()
            .code,
        "unsupported_configuration"
    );
    assert!(engine.list_rewrites(&id).await.unwrap().is_empty());
}

#[tokio::test]
async fn relative_ssh_signing_key_and_program_keep_original_context() {
    let dir = linear();
    let path = dir.path();
    let old = git(path, &["rev-parse", "HEAD"]);
    write(path, ".git/signing.key", "fixture key\n");
    git(path, &["config", "commit.gpgSign", "true"]);
    git(path, &["config", "gpg.format", "ssh"]);
    git(path, &["config", "user.signingKey", ".git/signing.key"]);
    git(path, &["config", "gpg.ssh.program", ".git/fake-signer"]);
    git(
        path,
        &["config", "test.gitViewOriginalRoot", path.to_str().unwrap()],
    );
    hook(&path.join(".git/fake-signer"),"#!/bin/sh\noriginal=$(git config --get test.gitViewOriginalRoot) || exit 1\nprintf '%s\\n' \"$@\" > \"$original/.git/signer-arguments\"\nexit 1\n");
    let engine = GitEngine::isolated();
    let id = engine.open_repository(path).await.unwrap().repository_id;
    assert!(combine(&engine, &id, suffix(path)).await.is_err());
    assert_eq!(git(path, &["rev-parse", "HEAD"]), old);
    let arguments = std::fs::read_to_string(path.join(".git/signer-arguments")).unwrap();
    let expected = std::fs::canonicalize(path)
        .unwrap()
        .join(".git/signing.key");
    assert!(
        arguments
            .lines()
            .any(|line| std::path::Path::new(line) == expected),
        "{arguments}"
    );
}
#[tokio::test]
async fn changed_branch_and_detached_head_disable_recovery() {
    let dir = linear();
    let path = dir.path();
    let engine = GitEngine::isolated();
    let id = engine.open_repository(path).await.unwrap().repository_id;
    combine(&engine, &id, suffix(path)).await.unwrap();
    let record = engine.list_rewrites(&id).await.unwrap().remove(0);
    git(path, &["switch", "-c", "other"]);
    assert!(!engine.list_rewrites(&id).await.unwrap()[0].can_recover);
    assert!(recover(&engine, &id, record.recovery_id.clone())
        .await
        .is_err());
    git(path, &["switch", "--detach"]);
    assert!(!engine.list_rewrites(&id).await.unwrap()[0].can_recover);
    assert!(recover(&engine, &id, record.recovery_id).await.is_err());
}
#[tokio::test]
async fn stale_preview_rejects_config_index_and_head_changes() {
    for changed in ["config", "index", "head"] {
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
        match changed {
            "config" => {
                git(path, &["config", "user.name", "Changed"]);
            }
            "index" => {
                write(path, "a", "staged");
                git(path, &["add", "a"]);
            }
            _ => {
                git(path, &["commit", "--allow-empty", "-m", "later"]);
            }
        }
        assert_eq!(
            engine.execute(&plan).await.err().unwrap().code,
            "precondition_changed"
        );
        assert!(engine.list_rewrites(&id).await.unwrap().is_empty());
    }
}

#[tokio::test]
async fn cwd_dependent_ssh_default_key_command_requires_explicit_key() {
    let dir = linear();
    let path = dir.path();
    git(path, &["config", "gpg.format", "ssh"]);
    git(
        path,
        &["config", "gpg.ssh.defaultKeyCommand", "cat relative.pub"],
    );
    let engine = GitEngine::isolated();
    let id = engine.open_repository(path).await.unwrap().repository_id;
    assert_eq!(
        combine(&engine, &id, suffix(path))
            .await
            .err()
            .unwrap()
            .code,
        "unsupported_configuration"
    );
    assert!(engine.list_rewrites(&id).await.unwrap().is_empty());
}
