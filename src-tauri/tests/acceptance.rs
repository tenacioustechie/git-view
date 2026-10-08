use git_view::{
    catalog::{read_project, Catalog},
    contracts::*,
    git::GitEngine,
    operations::Operations,
};
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
fn git(path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(path)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
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
    ] {
        git(path, &["config", key, value]);
    }
}
fn init(path: &Path) {
    std::fs::create_dir_all(path).unwrap();
    git(path, &["init", "-b", "main"]);
    config(path);
}
fn commit(path: &Path, file: &str, content: &str) {
    std::fs::write(path.join(file), content).unwrap();
    git(path, &["add", "--all"]);
    git(path, &["commit", "-m", content]);
}
async fn finish(service: &Operations, batch: &Batch) -> Vec<OperationResult> {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let ops = service.get(Some(&batch.batch_id)).await;
            if ops.len() == batch.operation_ids.len()
                && ops
                    .iter()
                    .all(|o| !matches!(o.state, JobState::Queued | JobState::Running))
            {
                break ops;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}
fn request(project: &Project, action: &str, branch: Option<&str>) -> BatchRequest {
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
        branch_filter: branch.map(str::to_owned),
        target_branch: None,
    }
}

fn switch_request(project: &Project, branch: Option<&str>) -> BatchRequest {
    let mut request = request(project, "switchBranch", branch);
    request.target_branch = Some("team/topic".into());
    request
}

fn push_repo(root: &Path, name: &str) -> (PathBuf, PathBuf, String) {
    let repo = root.join(name);
    init(&repo);
    commit(&repo, "file", "base\n");
    let remote = root.join(format!("{name}.git"));
    git(root, &["init", "--bare", remote.to_str().unwrap()]);
    git(
        &repo,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    git(&repo, &["push", "-u", "origin", "main:upstream-name"]);
    let base = git(&repo, &["rev-parse", "HEAD"]);
    commit(&repo, "file", "publish\n");
    (repo, remote, base)
}

#[tokio::test]
async fn group_push_cancel_stops_queued_jobs_but_reports_already_running_pushes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let names = ["first", "second", "third", "queued"];
    let mut remotes = Vec::new();
    for name in names {
        let (repo, remote, base) = push_repo(root, name);
        remotes.push((remote, base));
        let hooks = repo.join(".git/hooks");
        std::fs::create_dir_all(&hooks).unwrap();
        let hook = hooks.join("pre-push");
        std::fs::write(&hook, "#!/bin/sh\ntouch .git/push-started\nsleep 2\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        git(
            &repo,
            &["config", "core.hooksPath", hooks.to_str().unwrap()],
        );
    }
    let (engine, catalog, project) = switch_project(root, &names).await;
    let service = Operations::new(engine, catalog);
    let plan = service
        .plan_batch(request(&project, "push", None))
        .await
        .unwrap();
    let batch = service.run_batch(&plan.plan_id).await.unwrap();
    tokio::time::timeout(Duration::from_secs(20), async {
        while names
            .iter()
            .filter(|name| root.join(name).join(".git/push-started").exists())
            .count()
            < 3
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    service.cancel(&batch.batch_id).await.unwrap();
    let results = finish(&service, &batch).await;
    assert_eq!(
        results
            .iter()
            .filter(|r| r.state == JobState::Succeeded)
            .count(),
        3
    );
    assert_eq!(
        results
            .iter()
            .filter(|r| r.state == JobState::Canceled)
            .count(),
        1
    );
    for (result, (remote, base)) in results.iter().zip(remotes) {
        let tip = git(&remote, &["rev-parse", "refs/heads/upstream-name"]);
        if result.state == JobState::Canceled {
            assert_eq!(tip, base);
        } else {
            assert_ne!(tip, base);
            assert!(result.reason.as_ref().unwrap().contains("action completed"));
        }
    }
}

#[tokio::test]
async fn group_push_reviews_exact_upstreams_preserves_dirty_bytes_and_shares_alias_results() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let (repo, remote, base) = push_repo(root, "ready");
    let (missing, _, _) = push_repo(root, "missing-upstream");
    git(&missing, &["branch", "--unset-upstream"]);
    let (detached, _, _) = push_repo(root, "detached");
    git(&detached, &["switch", "--detach"]);
    init(&root.join("unborn"));
    std::fs::write(repo.join("file"), "keep dirty bytes\n").unwrap();
    git(&repo, &["config", "push.followTags", "true"]);
    git(&repo, &["config", "push.autoSetupRemote", "true"]);
    git(
        &repo,
        &["tag", "-a", "unreviewed-tag", "-m", "never push this"],
    );
    let source = git(&repo, &["rev-parse", "HEAD"]);
    let (engine, catalog, project) = switch_project(
        root,
        &[
            "ready",
            "ready",
            "missing-upstream",
            "detached",
            "unborn",
            "absent",
        ],
    )
    .await;
    let service = Operations::new(engine, catalog);
    let preview = service
        .plan_batch(request(&project, "push", None))
        .await
        .unwrap();
    assert_eq!(preview.candidates.iter().filter(|c| c.eligible).count(), 2);
    assert_eq!(
        preview.candidates[0].destination.as_deref(),
        Some("origin refs/heads/upstream-name")
    );
    assert!(preview.candidates[0]
        .summary
        .as_ref()
        .unwrap()
        .contains(&source));
    assert_eq!(
        git(&remote, &["rev-parse", "refs/heads/upstream-name"]),
        base
    );
    let batch = service.run_batch(&preview.plan_id).await.unwrap();
    assert_eq!(
        service.run_batch(&preview.plan_id).await.unwrap().batch_id,
        batch.batch_id
    );
    let results = finish(&service, &batch).await;
    assert!(results[..2].iter().all(|r| r.state == JobState::Succeeded));
    assert!(results[2..].iter().all(|r| r.state == JobState::Skipped));
    assert_eq!(results[0].destination, preview.candidates[0].destination);
    assert_eq!(results[1].summary, preview.candidates[1].summary);
    assert!(results[1]
        .reason
        .as_ref()
        .unwrap()
        .contains("Same repository"));
    assert_eq!(
        git(&remote, &["rev-parse", "refs/heads/upstream-name"]),
        source
    );
    assert!(git(&remote, &["tag", "--list"]).is_empty());
    assert_eq!(
        git(
            &remote,
            &["for-each-ref", "--format=%(refname)", "refs/heads"]
        ),
        "refs/heads/upstream-name"
    );
    assert_eq!(
        git(&repo, &["config", "branch.main.merge"]),
        "refs/heads/upstream-name"
    );
    assert_eq!(
        std::fs::read_to_string(repo.join("file")).unwrap(),
        "keep dirty bytes\n"
    );
}

#[tokio::test]
async fn group_push_rejects_ambiguous_config_and_rechecks_preview_state() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let (repo, remote, base) = push_repo(root, "one");
    let (engine, catalog, project) = switch_project(root, &["one"]).await;
    let service = Operations::new(engine, catalog.clone());
    for (key, value) in [
        ("branch.main.pushRemote", "fork"),
        ("remote.pushDefault", "fork"),
        ("remote.origin.mirror", "true"),
        ("remote.origin.push", "+main:refs/heads/upstream-name"),
        ("remote.origin.push", "main:refs/heads/other"),
    ] {
        git(&repo, &["config", key, value]);
        let plan = service
            .plan_batch(request(&project, "push", None))
            .await
            .unwrap();
        assert!(!plan.candidates[0].eligible, "{key}={value}");
        git(&repo, &["config", "--unset-all", key]);
    }
    git(
        &repo,
        &["config", "--add", "branch.main.merge", "refs/heads/other"],
    );
    assert!(
        !service
            .plan_batch(request(&project, "push", None))
            .await
            .unwrap()
            .candidates[0]
            .eligible
    );
    git(
        &repo,
        &[
            "config",
            "--replace-all",
            "branch.main.merge",
            "refs/heads/upstream-name",
        ],
    );
    git(
        &repo,
        &[
            "config",
            "--add",
            "remote.origin.pushurl",
            remote.to_str().unwrap(),
        ],
    );
    git(
        &repo,
        &[
            "config",
            "--add",
            "remote.origin.pushurl",
            root.join("elsewhere.git").to_str().unwrap(),
        ],
    );
    assert!(
        !service
            .plan_batch(request(&project, "push", None))
            .await
            .unwrap()
            .candidates[0]
            .eligible
    );
    git(&repo, &["config", "--unset-all", "remote.origin.pushurl"]);
    assert!(
        !service
            .plan_batch(request(&project, "push", Some("other")))
            .await
            .unwrap()
            .candidates[0]
            .eligible
    );
    let preview = service
        .plan_batch(request(&project, "push", Some("main")))
        .await
        .unwrap();
    git(
        &repo,
        &[
            "config",
            "branch.main.merge",
            "refs/heads/changed-after-preview",
        ],
    );
    let result = finish(
        &service,
        &service.run_batch(&preview.plan_id).await.unwrap(),
    )
    .await;
    assert_eq!(result[0].state, JobState::Skipped);
    git(
        &repo,
        &["config", "branch.main.merge", "refs/heads/upstream-name"],
    );
    let preview = service
        .plan_batch(request(&project, "push", None))
        .await
        .unwrap();
    commit(&repo, "file", "new unreviewed commit\n");
    let result = finish(
        &service,
        &service.run_batch(&preview.plan_id).await.unwrap(),
    )
    .await;
    assert_eq!(result[0].state, JobState::Skipped);
    let preview = service
        .plan_batch(request(&project, "push", None))
        .await
        .unwrap();
    std::fs::write(&project.display_path, b"{}").unwrap();
    assert!(service.run_batch(&preview.plan_id).await.is_err());
    assert_eq!(
        git(&remote, &["rev-parse", "refs/heads/upstream-name"]),
        base
    );
}

#[tokio::test]
async fn group_push_partial_failure_cancellation_and_fresh_retry_are_independent() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let (_, good_remote, _) = push_repo(root, "good");
    let (bad, bad_remote, base) = push_repo(root, "bad");
    let other = root.join("other");
    git(
        root,
        &[
            "clone",
            "-b",
            "upstream-name",
            bad_remote.to_str().unwrap(),
            other.to_str().unwrap(),
        ],
    );
    config(&other);
    commit(&other, "different", "remote ahead\n");
    git(&other, &["push", "origin", "HEAD:upstream-name"]);
    let remote_tip = git(&bad_remote, &["rev-parse", "refs/heads/upstream-name"]);
    let (engine, catalog, project) = switch_project(root, &["good", "bad"]).await;
    let service = Operations::new(engine, catalog);
    let preview = service
        .plan_batch(request(&project, "push", None))
        .await
        .unwrap();
    assert!(preview.candidates.iter().all(|c| c.eligible)); // Preview does not contact remote.
    let results = finish(
        &service,
        &service.run_batch(&preview.plan_id).await.unwrap(),
    )
    .await;
    assert_eq!(results[0].state, JobState::Succeeded);
    assert_eq!(results[1].state, JobState::Failed);
    assert!(results[1].outcome_uncertain);
    assert_eq!(
        git(&bad_remote, &["rev-parse", "refs/heads/upstream-name"]),
        remote_tip
    );
    assert_ne!(
        git(&good_remote, &["rev-parse", "refs/heads/upstream-name"]),
        base
    );
    // New retry resolves a changed configured upstream, but never runs without review.
    git(&bad, &["fetch", "origin"]);
    git(&bad, &["merge", "--no-edit", "origin/upstream-name"]);
    let mut retry = request(&project, "push", None);
    retry.member_ids = vec![preview.candidates[1].member_id.clone()];
    let retry = service.plan_batch(retry).await.unwrap();
    assert!(retry.candidates[0].eligible);
    assert_eq!(
        git(&bad_remote, &["rev-parse", "refs/heads/upstream-name"]),
        remote_tip
    );
    let batch = service.run_batch(&retry.plan_id).await.unwrap();
    service.cancel(&batch.batch_id).await.unwrap();
    assert_eq!(finish(&service, &batch).await[0].state, JobState::Canceled);
    assert_eq!(
        git(&bad_remote, &["rev-parse", "refs/heads/upstream-name"]),
        remote_tip
    );
}

#[tokio::test]
async fn group_push_pins_the_reviewed_commit_even_if_a_hook_moves_the_local_branch() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let (repo, remote, _) = push_repo(root, "one");
    let reviewed = git(&repo, &["rev-parse", "HEAD"]);
    git(&repo, &["switch", "-c", "other"]);
    commit(&repo, "later", "not reviewed\n");
    let unreviewed = git(&repo, &["rev-parse", "HEAD"]);
    git(&repo, &["switch", "main"]);
    let hooks = repo.join(".git/hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    let hook = hooks.join("pre-push");
    std::fs::write(
        &hook,
        format!("#!/bin/sh\ngit update-ref refs/heads/main {unreviewed}\n"),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    git(
        &repo,
        &["config", "core.hooksPath", hooks.to_str().unwrap()],
    );
    let (engine, catalog, project) = switch_project(root, &["one"]).await;
    let service = Operations::new(engine, catalog);
    let preview = service
        .plan_batch(request(&project, "push", None))
        .await
        .unwrap();
    let results = finish(
        &service,
        &service.run_batch(&preview.plan_id).await.unwrap(),
    )
    .await;
    assert_eq!(results[0].state, JobState::Succeeded);
    assert_eq!(
        git(&remote, &["rev-parse", "refs/heads/upstream-name"]),
        reviewed
    );
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), unreviewed);
}
async fn switch_project(root: &Path, names: &[&str]) -> (GitEngine, Catalog, Project) {
    let engine = GitEngine::isolated();
    let catalog = Catalog::new();
    let content = ProjectContent {
        version: 1,
        name: "Switch group".into(),
        repositories: names
            .iter()
            .map(|name| ProjectMember {
                path: (*name).into(),
                label: Some((*name).into()),
            })
            .collect(),
    };
    let file = root.join("switch.git-view.json");
    std::fs::write(&file, serde_json::to_vec(&content).unwrap()).unwrap();
    let project = catalog.open(&engine, &file).await.unwrap();
    (engine, catalog, project)
}
fn switch_repo(root: &Path, name: &str) -> PathBuf {
    let p = root.join(name);
    init(&p);
    commit(&p, "base", "base\n");
    git(&p, &["switch", "-c", "team/topic"]);
    commit(&p, "topic", "topic\n");
    git(&p, &["switch", "main"]);
    p
}

#[tokio::test]
async fn group_switch_does_not_recurse_into_submodules_with_recursive_user_config() {
    let temp = tempfile::tempdir().unwrap();
    let child = temp.path().join("child");
    init(&child);
    commit(&child, "file", "old child\n");
    let old = git(&child, &["rev-parse", "HEAD"]);
    commit(&child, "file", "new child\n");
    let new = git(&child, &["rev-parse", "HEAD"]);
    let parent = temp.path().join("parent");
    init(&parent);
    git(
        &parent,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            child.to_str().unwrap(),
            "nested",
        ],
    );
    let nested = parent.join("nested");
    git(&nested, &["checkout", &old]);
    commit(&parent, "base", "main\n");
    git(&parent, &["switch", "-c", "team/topic"]);
    git(&nested, &["checkout", &new]);
    commit(&parent, "base", "topic\n");
    git(&parent, &["switch", "main"]);
    git(&nested, &["checkout", &old]);
    git(&parent, &["config", "submodule.recurse", "true"]);
    let (engine, catalog, project) = switch_project(temp.path(), &["parent"]).await;
    let service = Operations::new(engine, catalog);
    let plan = service
        .plan_batch(switch_request(&project, None))
        .await
        .unwrap();
    assert!(plan.candidates[0].eligible, "{:?}", plan.candidates[0]);
    let batch = service.run_batch(&plan.plan_id).await.unwrap();
    let results = finish(&service, &batch).await;
    assert_eq!(results[0].state, JobState::Succeeded, "{results:?}");
    assert_eq!(git(&parent, &["branch", "--show-current"]), "team/topic");
    assert_eq!(git(&nested, &["rev-parse", "HEAD"]), old);
    assert_eq!(
        std::fs::read_to_string(nested.join("file")).unwrap(),
        "old child\n"
    );
}

#[tokio::test]
async fn group_switch_reviews_mixed_members_and_shares_alias_execution() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    for name in [
        "clean",
        "dirty",
        "missing-target",
        "busy",
        "detached",
        "already",
        "occupied",
    ] {
        switch_repo(root, name);
    }
    std::fs::write(root.join("dirty/base"), "private edits\n").unwrap();
    git(
        &root.join("missing-target"),
        &["branch", "-D", "team/topic"],
    );
    std::fs::write(
        root.join("busy/.git/MERGE_HEAD"),
        format!(
            "{}\n",
            git(&root.join("busy"), &["rev-parse", "team/topic"])
        ),
    )
    .unwrap();
    git(&root.join("detached"), &["switch", "--detach"]);
    git(&root.join("already"), &["switch", "team/topic"]);
    git(
        &root.join("occupied"),
        &[
            "worktree",
            "add",
            root.join("linked").to_str().unwrap(),
            "team/topic",
        ],
    );
    let (engine, catalog, project) = switch_project(
        root,
        &[
            "clean",
            "dirty",
            "missing-target",
            "busy",
            "detached",
            "already",
            "occupied",
            "absent",
            "./clean",
        ],
    )
    .await;
    let service = Operations::new(engine, catalog);
    let plan = service
        .plan_batch(switch_request(&project, None))
        .await
        .unwrap();
    assert_eq!(plan.target_branch.as_deref(), Some("team/topic"));
    assert_eq!(plan.candidates.iter().filter(|c| c.eligible).count(), 2);
    assert!(plan.candidates[0]
        .summary
        .as_ref()
        .unwrap()
        .contains(&git(&root.join("clean"), &["rev-parse", "team/topic"])));
    assert!(plan.candidates[5]
        .reason
        .as_ref()
        .unwrap()
        .contains("Already"));
    assert!(plan.candidates[6]
        .reason
        .as_ref()
        .unwrap()
        .contains("worktree"));
    assert_eq!(
        git(&root.join("clean"), &["branch", "--show-current"]),
        "main"
    );
    let batch = service.run_batch(&plan.plan_id).await.unwrap();
    assert_eq!(
        service.run_batch(&plan.plan_id).await.unwrap().batch_id,
        batch.batch_id
    );
    let ops = finish(&service, &batch).await;
    assert_eq!(
        ops.iter()
            .filter(|o| o.state == JobState::Succeeded)
            .count(),
        2
    );
    assert_eq!(
        ops.iter().filter(|o| o.state == JobState::Skipped).count(),
        7
    );
    assert!(ops
        .iter()
        .all(|o| o.action == "switchBranch" && o.target_branch.as_deref() == Some("team/topic")));
    assert!(ops[8].reason.as_ref().unwrap().contains("Same repository"));
    assert_eq!(
        git(&root.join("clean"), &["branch", "--show-current"]),
        "team/topic"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("dirty/base")).unwrap(),
        "private edits\n"
    );
    assert_eq!(
        git(&root.join("dirty"), &["branch", "--show-current"]),
        "main"
    );
}

#[tokio::test]
async fn group_switch_rechecks_refs_dirty_state_filters_and_project_revision() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let moved = switch_repo(root, "moved");
    let dirty = switch_repo(root, "dirty");
    let symbolic = switch_repo(root, "symbolic");
    let (engine, catalog, project) = switch_project(root, &["moved", "dirty", "symbolic"]).await;
    let service = Operations::new(engine, catalog);
    let excluded = service
        .plan_batch(switch_request(&project, Some("other")))
        .await
        .unwrap();
    assert!(excluded.candidates.iter().all(|c| !c.eligible));
    let plan = service
        .plan_batch(switch_request(&project, Some("main")))
        .await
        .unwrap();
    git(&moved, &["update-ref", "refs/heads/team/topic", "main"]);
    std::fs::write(dirty.join("local"), "keep\n").unwrap();
    git(
        &symbolic,
        &["symbolic-ref", "refs/heads/team/topic", "refs/heads/main"],
    );
    let batch = service.run_batch(&plan.plan_id).await.unwrap();
    assert!(finish(&service, &batch)
        .await
        .iter()
        .all(|o| o.state == JobState::Skipped));
    for p in [&moved, &dirty, &symbolic] {
        assert_eq!(git(p, &["branch", "--show-current"]), "main");
    }
    let symbolic_plan = service
        .plan_batch(switch_request(&project, None))
        .await
        .unwrap();
    assert!(!symbolic_plan.candidates[2].eligible);
    std::fs::write(root.join("switch.git-view.json"), "changed externally").unwrap();
    assert!(service.run_batch(&symbolic_plan.plan_id).await.is_err());
}

#[tokio::test]
async fn group_switch_preserves_ignored_files_and_reports_post_checkout_failure_without_rollback() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    switch_repo(root, "clean");
    let ignored = switch_repo(root, "ignored");
    let hooked = switch_repo(root, "hooked");
    commit(&ignored, ".gitignore", "secret\n");
    git(&ignored, &["switch", "team/topic"]);
    std::fs::write(ignored.join("secret"), "tracked target\n").unwrap();
    git(&ignored, &["add", "-f", "secret"]);
    git(&ignored, &["commit", "-m", "target secret"]);
    git(&ignored, &["switch", "main"]);
    std::fs::write(ignored.join("secret"), "private ignored\n").unwrap();
    let hooks = hooked.join(".git/hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    let hook = hooks.join("post-checkout");
    std::fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    git(
        &hooked,
        &["config", "core.hooksPath", hooks.to_str().unwrap()],
    );
    let (engine, catalog, project) = switch_project(root, &["clean", "ignored", "hooked"]).await;
    let service = Operations::new(engine, catalog);
    let plan = service
        .plan_batch(switch_request(&project, None))
        .await
        .unwrap();
    assert!(plan.candidates.iter().all(|c| c.eligible));
    let batch = service.run_batch(&plan.plan_id).await.unwrap();
    let ops = finish(&service, &batch).await;
    assert_eq!(ops[0].state, JobState::Succeeded);
    assert_eq!(ops[1].state, JobState::Failed);
    assert_eq!(ops[2].state, JobState::Failed);
    assert!(ops[2].outcome_uncertain);
    assert_eq!(
        std::fs::read_to_string(ignored.join("secret")).unwrap(),
        "private ignored\n"
    );
    assert_eq!(git(&ignored, &["branch", "--show-current"]), "main");
    assert_eq!(git(&hooked, &["branch", "--show-current"]), "team/topic");
    assert_eq!(
        git(&root.join("clean"), &["branch", "--show-current"]),
        "team/topic"
    );
}

#[tokio::test]
async fn group_switch_cancellation_and_literal_target_validation() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    switch_repo(root, "one");
    let (engine, catalog, project) = switch_project(root, &["one"]).await;
    let service = Operations::new(engine, catalog);
    for name in ["", "-", "@{-1}", "refs/heads/main", "main\n", "name space"] {
        let mut req = switch_request(&project, None);
        req.target_branch = Some(name.into());
        assert!(service.plan_batch(req).await.is_err(), "{name}");
    }
    let mut req = switch_request(&project, None);
    req.action = "fetch".into();
    assert!(service.plan_batch(req).await.is_err());
    let plan = service
        .plan_batch(switch_request(&project, None))
        .await
        .unwrap();
    let batch = service.run_batch(&plan.plan_id).await.unwrap();
    service.cancel(&batch.batch_id).await.unwrap();
    assert_eq!(finish(&service, &batch).await[0].state, JobState::Canceled);
    assert_eq!(
        git(&root.join("one"), &["branch", "--show-current"]),
        "main"
    );
}
#[tokio::test]
async fn mixed_group_pull_preserves_dirty_and_diverged_and_reports_aliases() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let seed = root.join("seed");
    init(&seed);
    commit(&seed, "base", "base\n");
    let remote = root.join("remote.git");
    git(
        root,
        &[
            "clone",
            "--bare",
            seed.to_str().unwrap(),
            remote.to_str().unwrap(),
        ],
    );
    for name in ["clean space", "dirty", "diverged", "other"] {
        let path = root.join(name);
        git(
            root,
            &["clone", remote.to_str().unwrap(), path.to_str().unwrap()],
        );
        config(&path);
    }
    let dirty = root.join("dirty");
    std::fs::write(dirty.join("base"), "staged\n").unwrap();
    git(&dirty, &["add", "base"]);
    std::fs::write(dirty.join("base"), "unstaged\n").unwrap();
    std::fs::write(dirty.join("untracked"), "keep\n").unwrap();
    let old_head = git(&dirty, &["rev-parse", "HEAD"]);
    let old_index = git(&dirty, &["write-tree"]);
    commit(&root.join("diverged"), "local", "local\n");
    let diverged_head = git(&root.join("diverged"), &["rev-parse", "HEAD"]);
    git(&root.join("other"), &["switch", "-c", "feature"]);
    commit(&seed, "remote-change", "remote\n");
    git(&seed, &["push", remote.to_str().unwrap(), "main"]);
    let new_head = git(&seed, &["rev-parse", "HEAD"]);
    let file = root.join("project.git-view.json");
    std::fs::write(&file,r#"{"version":1,"name":"Mixed","repositories":[{"path":"clean space"},{"path":"dirty"},{"path":"diverged"},{"path":"missing"},{"path":"other"},{"path":"./clean space","label":"Alias"}]}"#).unwrap();
    let engine = GitEngine::isolated();
    let catalog = Catalog::new();
    let project = catalog.open(&engine, &file).await.unwrap();
    assert_eq!(project.members.len(), 6);
    let service = Operations::new(engine.clone(), catalog.clone());
    let plan = service
        .plan_batch(request(&project, "pull", Some("main")))
        .await
        .unwrap();
    assert_eq!(plan.candidates.len(), 6);
    assert!(!plan.candidates[1].eligible);
    assert!(!plan.candidates[3].eligible);
    assert!(!plan.candidates[4].eligible);
    let batch = service.run_batch(&plan.plan_id).await.unwrap();
    assert_eq!(
        service.run_batch(&plan.plan_id).await.unwrap().batch_id,
        batch.batch_id
    );
    let ops = finish(&service, &batch).await;
    assert_eq!(
        ops.iter()
            .filter(|o| o.state == JobState::Succeeded)
            .count(),
        2
    );
    assert_eq!(
        ops.iter().filter(|o| o.state == JobState::Skipped).count(),
        3
    );
    assert_eq!(
        ops.iter().filter(|o| o.state == JobState::Failed).count(),
        1
    );
    assert_eq!(
        git(&root.join("clean space"), &["rev-parse", "HEAD"]),
        new_head
    );
    assert_eq!(git(&dirty, &["rev-parse", "HEAD"]), old_head);
    assert_eq!(git(&dirty, &["write-tree"]), old_index);
    assert_eq!(
        std::fs::read_to_string(dirty.join("base")).unwrap(),
        "unstaged\n"
    );
    assert_eq!(
        std::fs::read_to_string(dirty.join("untracked")).unwrap(),
        "keep\n"
    );
    assert_eq!(git(&dirty, &["branch", "--show-current"]), "main");
    assert_eq!(
        git(&root.join("diverged"), &["rev-parse", "HEAD"]),
        diverged_head
    );
    // Retry only the previously skipped dirty member after manual fixture cleanup.
    git(&dirty, &["restore", "--staged", "base"]);
    git(&dirty, &["restore", "base"]);
    std::fs::remove_file(dirty.join("untracked")).unwrap();
    let refreshed = catalog.refresh(&engine, &project.project_id).await.unwrap();
    let mut req = request(&refreshed, "pull", None);
    req.member_ids = vec![plan.candidates[1].member_id.clone()];
    let retry = service.plan_batch(req).await.unwrap();
    assert_eq!(retry.candidates.len(), 1);
    let batch = service.run_batch(&retry.plan_id).await.unwrap();
    assert_eq!(finish(&service, &batch).await[0].state, JobState::Succeeded);
}
#[tokio::test]
async fn stale_status_and_project_preview_never_mutate() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    init(&repo);
    commit(&repo, "base", "base");
    let remote = temp.path().join("remote.git");
    git(
        temp.path(),
        &[
            "clone",
            "--bare",
            repo.to_str().unwrap(),
            remote.to_str().unwrap(),
        ],
    );
    git(
        &repo,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    git(&repo, &["fetch", "origin"]);
    git(&repo, &["branch", "--set-upstream-to=origin/main"]);
    let engine = GitEngine::isolated();
    let catalog = Catalog::new();
    let file = temp.path().join("p.json");
    let content = ProjectContent {
        version: 1,
        name: "P".into(),
        repositories: vec![ProjectMember {
            path: "repo".into(),
            label: None,
        }],
    };
    let project = catalog.create(&engine, &file, content).await.unwrap();
    let service = Operations::new(engine.clone(), catalog.clone());
    let plan = service
        .plan_batch(request(&project, "pull", None))
        .await
        .unwrap();
    std::fs::write(repo.join("new"), "outside edit").unwrap();
    let batch = service.run_batch(&plan.plan_id).await.unwrap();
    assert_eq!(finish(&service, &batch).await[0].state, JobState::Skipped);
    let plan = service
        .plan_batch(request(&project, "fetch", None))
        .await
        .unwrap();
    std::fs::write(&file, "{ corrupt").unwrap();
    assert!(service.run_batch(&plan.plan_id).await.is_err());
    let stale = catalog.refresh(&engine, &project.project_id).await.unwrap();
    assert!(stale.stale);
    assert_eq!(stale.members.len(), 1);
    assert!(service
        .plan_batch(request(&stale, "fetch", None))
        .await
        .is_err());
}
#[tokio::test]
async fn project_validation_and_atomic_revision_save() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("project.json");
    for json in [
        r#"{"version":2,"name":"x","repositories":[]}"#,
        r#"{"version":1,"name":" ","repositories":[]}"#,
        r#"{"version":1,"name":"x","repositories":[],"typo":1}"#,
        r#"{"version":1,"name":"x","repositories":[{"path":" "}]}"#,
        r#"{"version":1,"name":"x","repositories":[{"path":"x","label":" "}]}"#,
    ] {
        std::fs::write(&file, json).unwrap();
        assert!(read_project(&file).is_err());
    }
    std::fs::write(&file, vec![b' '; 1024 * 1024 + 1]).unwrap();
    assert!(read_project(&file).is_err());
    std::fs::remove_file(&file).unwrap();
    let engine = GitEngine::isolated();
    let catalog = Catalog::new();
    let content = ProjectContent {
        version: 1,
        name: "Duplicate labels allowed".into(),
        repositories: vec![
            ProjectMember {
                path: temp.path().join("missing").to_str().unwrap().into(),
                label: Some("same".into()),
            },
            ProjectMember {
                path: "other".into(),
                label: Some("same".into()),
            },
        ],
    };
    let p = catalog
        .create(&engine, &file, content.clone())
        .await
        .unwrap();
    assert!(!PathBuf::from(&p.content.repositories[0].path).is_absolute());
    assert!(
        matches!(&p.members[0],RepositoryRow::Unavailable{display_path,..} if Path::new(display_path).is_absolute())
    );
    assert!(catalog
        .create(&engine, &file, content.clone())
        .await
        .is_err());
    let mut next = content.clone();
    next.name = "Edited".into();
    let p2 = catalog
        .save(&engine, &p.project_id, &p.revision, next)
        .await
        .unwrap();
    assert_eq!(p2.name, "Edited");
    assert!(catalog
        .save(&engine, &p.project_id, &p.revision, content.clone())
        .await
        .is_err());
    std::fs::write(&file, "external content").unwrap();
    assert!(catalog
        .save(&engine, &p.project_id, &p2.revision, content)
        .await
        .is_err());
    assert_eq!(std::fs::read_to_string(file).unwrap(), "external content");
}
#[tokio::test]
async fn single_action_plan_is_idempotent_and_retains_results() {
    let temp = tempfile::tempdir().unwrap();
    init(temp.path());
    std::fs::write(temp.path().join("one"), "one").unwrap();
    let engine = GitEngine::isolated();
    let repo = engine.open_repository(temp.path()).await.unwrap();
    let changes = engine.get_changes(&repo.repository_id).await.unwrap();
    let service = Operations::new(engine.clone(), Catalog::new());
    let plan = service
        .plan_action(
            &repo.repository_id,
            RepositoryAction::Stage {
                snapshot_id: changes.snapshot_id,
                file_ids: vec![changes.files[0].file_id.clone()],
            },
        )
        .await;
    let RepositoryActionPlan::Eligible { plan_id, .. } = plan else {
        panic!("stage blocked")
    };
    let first = service.run_action(&plan_id).await.unwrap();
    let second = service.run_action(&plan_id).await.unwrap();
    assert_eq!(first.operation_id, second.operation_id);
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let ops = service.get(None).await;
            if ops[0].state == JobState::Succeeded {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        git(temp.path(), &["diff", "--cached", "--name-only"]),
        "one"
    );
}
#[tokio::test]
async fn hundred_members_finish_with_bounded_cancelable_scheduling() {
    let temp = tempfile::tempdir().unwrap();
    let seed = temp.path().join("seed");
    init(&seed);
    commit(&seed, "base", "base");
    let remote = temp.path().join("remote.git");
    git(
        temp.path(),
        &[
            "clone",
            "--bare",
            seed.to_str().unwrap(),
            remote.to_str().unwrap(),
        ],
    );
    let mut members = Vec::new();
    for i in 0..100 {
        let name = format!("repo-{i}");
        let path = temp.path().join(&name);
        git(
            temp.path(),
            &[
                "clone",
                "--shared",
                remote.to_str().unwrap(),
                path.to_str().unwrap(),
            ],
        );
        config(&path);
        members.push(ProjectMember {
            path: name,
            label: None,
        });
    }
    let engine = GitEngine::isolated();
    let catalog = Catalog::new();
    let project = catalog
        .create(
            &engine,
            &temp.path().join("p.json"),
            ProjectContent {
                version: 1,
                name: "Hundred".into(),
                repositories: members,
            },
        )
        .await
        .unwrap();
    let service = Operations::new(engine, catalog);
    let plan = service
        .plan_batch(request(&project, "fetch", None))
        .await
        .unwrap();
    let batch = service.run_batch(&plan.plan_id).await.unwrap();
    service.cancel(&batch.batch_id).await.unwrap();
    let ops = finish(&service, &batch).await;
    assert_eq!(ops.len(), 100);
    assert!(ops.iter().any(|o| o.state == JobState::Canceled));
    assert!(ops
        .iter()
        .all(|o| matches!(o.state, JobState::Canceled | JobState::Succeeded)));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_project_saves_accept_only_one_revision() {
    let temp = tempfile::tempdir().unwrap();
    let engine = GitEngine::isolated();
    let catalog = Catalog::new();
    let project = catalog
        .create(
            &engine,
            &temp.path().join("p.json"),
            ProjectContent {
                version: 1,
                name: "Original".into(),
                repositories: vec![],
            },
        )
        .await
        .unwrap();
    let mut first = project.content.clone();
    first.name = "First".into();
    let mut second = project.content.clone();
    second.name = "Second".into();
    let a = catalog.save(&engine, &project.project_id, &project.revision, first);
    let b = catalog.save(&engine, &project.project_id, &project.revision, second);
    let (a, b) = tokio::join!(a, b);
    assert_ne!(a.is_ok(), b.is_ok());
    let saved = read_project(Path::new(&project.display_path)).unwrap().0;
    assert!(saved.name == "First" || saved.name == "Second");
}

#[cfg(unix)]
#[tokio::test]
async fn unix_backslash_paths_survive_project_save() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo\\literal");
    init(&path);
    let engine = GitEngine::isolated();
    let catalog = Catalog::new();
    let project = catalog
        .create(
            &engine,
            &temp.path().join("p.json"),
            ProjectContent {
                version: 1,
                name: "Paths".into(),
                repositories: vec![ProjectMember {
                    path: path.to_str().unwrap().into(),
                    label: None,
                }],
            },
        )
        .await
        .unwrap();
    assert_eq!(project.content.repositories[0].path, "repo\\literal");
    assert!(matches!(project.members[0], RepositoryRow::Ready { .. }));
}
