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
    }
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
