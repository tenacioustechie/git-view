//! Group pull with rebase, merge and fast-forward-only modes.
use git_view::{catalog::Catalog, contracts::*, git::GitEngine, operations::Operations};
use std::{path::Path, process::Command, time::Duration};

fn git(path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(path)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().into()
}
fn config(path: &Path) {
    for (key, value) in [
        ("user.name", "Fixture"),
        ("user.email", "fixture@example.invalid"),
        ("commit.gpgsign", "false"),
        ("core.hooksPath", ".disabled-hooks"),
        ("core.autocrlf", "false"),
        // Hostile user pull settings must not change the chosen strategy.
        ("pull.rebase", "true"),
        ("pull.ff", "only"),
        ("rebase.autoStash", "true"),
    ] {
        git(path, &["config", key, value]);
    }
}
fn commit(path: &Path, file: &str, content: &str) {
    std::fs::write(path.join(file), content).unwrap();
    git(path, &["add", "--all"]);
    git(path, &["commit", "-m", &format!("{file}: {content}")]);
}
fn head(path: &Path) -> String {
    git(path, &["rev-parse", "HEAD"])
}

struct Group {
    _temp: tempfile::TempDir,
    root: std::path::PathBuf,
    remote_head: String,
    service: Operations,
    project: Project,
}

/// Members: `behind` (only behind), `diverged` (unrelated local commit),
/// `conflict` (local edit of the file the remote also changed), `dirty`
/// (modified tracked file) and `ahead` (nothing to pull).
async fn group() -> Group {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().to_path_buf();
    let seed = root.join("seed");
    std::fs::create_dir_all(&seed).unwrap();
    git(&seed, &["init", "-b", "main"]);
    config(&seed);
    commit(&seed, "shared", "base\n");
    let remote = root.join("remote.git");
    git(
        &root,
        &[
            "clone",
            "--bare",
            seed.to_str().unwrap(),
            remote.to_str().unwrap(),
        ],
    );
    for name in ["behind", "diverged", "conflict", "dirty"] {
        git(
            &root,
            &[
                "clone",
                remote.to_str().unwrap(),
                root.join(name).to_str().unwrap(),
            ],
        );
        config(&root.join(name));
    }
    commit(&root.join("diverged"), "local", "local\n");
    commit(&root.join("conflict"), "shared", "local edit\n");
    std::fs::write(root.join("dirty/shared"), "uncommitted\n").unwrap();
    commit(&seed, "shared", "remote edit\n");
    git(&seed, &["push", remote.to_str().unwrap(), "main"]);
    git(
        &root,
        &[
            "clone",
            remote.to_str().unwrap(),
            root.join("ahead").to_str().unwrap(),
        ],
    );
    config(&root.join("ahead"));
    commit(&root.join("ahead"), "mine", "ahead\n");
    let names = ["behind", "diverged", "conflict", "dirty", "ahead"];
    let content = ProjectContent {
        version: 1,
        name: "Pull modes".into(),
        repositories: names
            .iter()
            .map(|name| ProjectMember {
                path: (*name).into(),
                label: Some((*name).into()),
            })
            .collect(),
    };
    let file = root.join("pull.git-view.json");
    std::fs::write(&file, serde_json::to_vec(&content).unwrap()).unwrap();
    let engine = GitEngine::isolated();
    let catalog = Catalog::new();
    let project = catalog.open(&engine, &file).await.unwrap();
    Group {
        remote_head: head(&seed),
        service: Operations::new(engine, catalog),
        project,
        root,
        _temp: temp,
    }
}
fn request(project: &Project, action: &str, pull_mode: Option<PullMode>) -> BatchRequest {
    BatchRequest {
        project_id: project.project_id.clone(),
        revision: project.revision.clone(),
        member_ids: project
            .members
            .iter()
            .map(|r| match r {
                RepositoryRow::Ready { member_id, .. }
                | RepositoryRow::Unavailable { member_id, .. }
                | RepositoryRow::Loading { member_id, .. } => member_id.clone(),
            })
            .collect(),
        action: action.into(),
        branch_filter: None,
        target_branch: None,
        pull_mode,
    }
}
async fn run(g: &Group, plan: &BatchPlan) -> Vec<(String, OperationResult)> {
    let batch = g.service.run_batch(&plan.plan_id).await.unwrap();
    let ops = tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let ops = g.service.get(Some(&batch.batch_id)).await;
            if ops.len() == batch.operation_ids.len()
                && ops
                    .iter()
                    .all(|o| !matches!(o.state, JobState::Queued | JobState::Running))
            {
                break ops;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    plan.candidates
        .iter()
        .map(|c| {
            let op = ops
                .iter()
                .find(|o| o.member_id.as_deref() == Some(&c.member_id))
                .unwrap()
                .clone();
            (c.label.clone(), op)
        })
        .collect()
}
fn result<'a>(results: &'a [(String, OperationResult)], label: &str) -> &'a OperationResult {
    &results.iter().find(|(l, _)| l == label).unwrap().1
}
fn candidate<'a>(plan: &'a BatchPlan, label: &str) -> &'a BatchCandidate {
    plan.candidates.iter().find(|c| c.label == label).unwrap()
}
fn assert_untouched_after_abort(path: &Path, original: &str) {
    assert_eq!(head(path), original);
    assert_eq!(
        std::fs::read_to_string(path.join("shared")).unwrap(),
        "local edit\n"
    );
    assert!(!path.join(".git/rebase-merge").exists());
    assert!(!path.join(".git/MERGE_HEAD").exists());
    assert!(!path.join(".git/git-view-rebase.json").exists());
    assert_eq!(git(path, &["status", "--porcelain"]), "");
}

#[tokio::test]
async fn rebase_group_pull_completes_clean_members_and_aborts_the_conflicting_one() {
    let g = group().await;
    let conflict = g.root.join("conflict");
    let conflict_head = head(&conflict);
    let dirty_head = head(&g.root.join("dirty"));
    let plan = g
        .service
        .plan_batch(request(&g.project, "pull", Some(PullMode::Rebase)))
        .await
        .unwrap();
    assert_eq!(plan.pull_mode, Some(PullMode::Rebase));
    assert!(candidate(&plan, "behind")
        .summary
        .as_deref()
        .unwrap()
        .starts_with("Fast-forward to origin/main"));
    assert!(candidate(&plan, "diverged")
        .summary
        .as_deref()
        .unwrap()
        .starts_with("Rebase 1 local commit(s) onto origin/main"));
    assert!(candidate(&plan, "conflict").eligible);
    assert!(!candidate(&plan, "dirty").eligible);
    let ahead = candidate(&plan, "ahead");
    assert!(!ahead.eligible && ahead.reason.as_deref().unwrap().contains("Nothing to pull"));

    let results = run(&g, &plan).await;
    assert_eq!(result(&results, "behind").state, JobState::Succeeded);
    assert_eq!(head(&g.root.join("behind")), g.remote_head);
    assert_eq!(result(&results, "diverged").state, JobState::Succeeded);
    assert_eq!(
        git(&g.root.join("diverged"), &["rev-parse", "HEAD^"]),
        g.remote_head
    );
    let aborted = result(&results, "conflict");
    assert_eq!(aborted.state, JobState::Failed);
    let error = aborted.error.as_ref().unwrap();
    assert_eq!(error.code, "pull_conflicts_aborted", "{}", error.message);
    assert!(
        error.message.starts_with(
            "Needs attention: the rebase onto origin/main stopped (1 conflicted file(s))"
        ),
        "{}",
        error.message
    );
    assert!(!aborted.outcome_uncertain);
    assert_untouched_after_abort(&conflict, &conflict_head);
    // The backup ref from the aborted attempt is retained.
    assert!(git(
        &conflict,
        &["for-each-ref", "refs/git-view/rebase-backups/"]
    )
    .contains(&conflict_head));
    assert_eq!(result(&results, "dirty").state, JobState::Skipped);
    assert_eq!(head(&g.root.join("dirty")), dirty_head);
    assert_eq!(
        std::fs::read_to_string(g.root.join("dirty/shared")).unwrap(),
        "uncommitted\n"
    );
}

#[tokio::test]
async fn merge_group_pull_merges_diverged_members_and_aborts_the_conflicting_one() {
    let g = group().await;
    let conflict = g.root.join("conflict");
    let conflict_head = head(&conflict);
    let diverged_head = head(&g.root.join("diverged"));
    let plan = g
        .service
        .plan_batch(request(&g.project, "pull", Some(PullMode::Merge)))
        .await
        .unwrap();
    assert!(candidate(&plan, "diverged")
        .summary
        .as_deref()
        .unwrap()
        .starts_with("Merge origin/main"));
    let results = run(&g, &plan).await;
    assert_eq!(result(&results, "diverged").state, JobState::Succeeded);
    assert_eq!(
        git(&g.root.join("diverged"), &["log", "-1", "--format=%P"]),
        format!("{diverged_head} {}", g.remote_head)
    );
    let error = result(&results, "conflict").error.clone().unwrap();
    assert_eq!(error.code, "pull_conflicts_aborted", "{}", error.message);
    assert!(
        error.message.contains("the merge of origin/main stopped"),
        "{}",
        error.message
    );
    assert_untouched_after_abort(&conflict, &conflict_head);
}

#[tokio::test]
async fn fast_forward_only_group_pull_skips_diverged_members_at_preview() {
    let g = group().await;
    let plan = g
        .service
        .plan_batch(request(&g.project, "pull", Some(PullMode::FastForward)))
        .await
        .unwrap();
    for label in ["diverged", "conflict"] {
        let c = candidate(&plan, label);
        assert!(!c.eligible);
        assert!(c
            .reason
            .as_deref()
            .unwrap()
            .contains("Choose Pull with rebase or Pull with merge"));
    }
    let results = run(&g, &plan).await;
    assert_eq!(result(&results, "behind").state, JobState::Succeeded);
    assert_eq!(head(&g.root.join("behind")), g.remote_head);
}

#[tokio::test]
async fn pull_mode_is_rejected_for_other_batch_actions() {
    let g = group().await;
    let error = g
        .service
        .plan_batch(request(&g.project, "fetch", Some(PullMode::Rebase)))
        .await
        .err()
        .unwrap();
    assert_eq!(error.code, "invalid_action");
}
