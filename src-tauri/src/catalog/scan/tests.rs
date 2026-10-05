use super::*;
use crate::operations::Operations;
use std::{process::Command, time::Instant};

fn fixture(count: usize) -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let mut repositories = Vec::new();
    for index in 0..count {
        let name = format!("repo-{index}");
        let path = temp.path().join(&name);
        std::fs::create_dir(&path).unwrap();
        let result = Command::new("git")
            .args(["init", "-b", "main"])
            .arg(&path)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env(
                "GIT_CONFIG_GLOBAL",
                if cfg!(windows) { "NUL" } else { "/dev/null" },
            )
            .output()
            .unwrap();
        assert!(result.status.success());
        repositories.push(ProjectMember {
            path: name,
            label: None,
        });
    }
    let path = temp.path().join("project.json");
    std::fs::write(
        &path,
        serde_json::to_vec(&ProjectContent {
            version: 1,
            name: "Scan fixture".into(),
            repositories,
        })
        .unwrap(),
    )
    .unwrap();
    (temp, path)
}

async fn until(catalog: &Catalog, id: &str, condition: impl Fn(&Project) -> bool) -> Project {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let current = catalog.get(id).await.unwrap();
            if condition(&current) {
                return current;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn fifteen_members_publish_around_a_slow_row_and_keep_unavailable_errors() {
    let (_temp, path) = fixture(15);
    // A missing member must be reported independently of the deliberately slow first member.
    let (mut content, _) = read_project(&path).unwrap();
    content.repositories[14].path = "missing".into();
    std::fs::write(&path, serde_json::to_vec(&content).unwrap()).unwrap();
    let catalog = Catalog::new();
    let engine = GitEngine::isolated();
    let gate = Arc::new(Semaphore::new(0));
    let root = dunce::canonicalize(path.parent().unwrap()).unwrap();
    catalog.read_gates.lock().await.insert(
        root.join("repo-0").to_string_lossy().into_owned(),
        gate.clone(),
    );
    let initial = catalog.start_open(&engine, &path).await.unwrap();
    assert!(initial
        .members
        .iter()
        .all(|row| matches!(row, RepositoryRow::Loading { .. })));
    let partial = until(&catalog, &initial.project_id, |p| {
        p.status_scan.as_ref().unwrap().pending_member_ids.len() == 1
    })
    .await;
    assert!(matches!(partial.members[0], RepositoryRow::Loading { .. }));
    assert!(matches!(partial.members[1], RepositoryRow::Ready { .. }));
    assert!(matches!(
        partial.members[14],
        RepositoryRow::Unavailable { .. }
    ));
    let service = Operations::new(engine.clone(), catalog.clone());
    let rejected = service
        .plan_batch(BatchRequest {
            project_id: initial.project_id.clone(),
            revision: initial.revision.clone(),
            member_ids: vec![],
            action: "fetch".into(),
            branch_filter: None,
            target_branch: None,
        })
        .await
        .unwrap_err();
    assert_eq!(rejected.code, "status_incomplete");
    gate.add_permits(1);
    let complete = until(&catalog, &initial.project_id, |p| {
        p.status_scan.as_ref().unwrap().state == "complete"
    })
    .await;
    for (before, after) in initial.members.iter().zip(&complete.members) {
        let RepositoryRow::Loading {
            member_id,
            label,
            display_path,
        } = before
        else {
            panic!()
        };
        let (next_id, next_label, next_path) = match after {
            RepositoryRow::Ready {
                member_id,
                label,
                display_path,
                ..
            }
            | RepositoryRow::Unavailable {
                member_id,
                label,
                display_path,
                ..
            } => (member_id, label, display_path),
            _ => panic!("all rows must be resolved"),
        };
        assert_eq!(
            (member_id, label, display_path),
            (next_id, next_label, next_path)
        );
    }
}

#[tokio::test]
async fn cancel_retains_rows_releases_reads_and_old_tokens_cannot_cancel_a_new_scan() {
    let (_temp, path) = fixture(5);
    let catalog = Catalog::new();
    let engine = GitEngine::isolated();
    let complete = catalog.open(&engine, &path).await.unwrap();
    let permits = catalog
        .read_permits
        .clone()
        .acquire_many_owned(4)
        .await
        .unwrap();
    let first = catalog
        .start_refresh(&engine, &complete.project_id)
        .await
        .unwrap();
    assert!(first
        .members
        .iter()
        .all(|row| matches!(row, RepositoryRow::Ready { .. })));
    let first_id = &first.status_scan.as_ref().unwrap().scan_id;
    let canceled = catalog
        .cancel_scan(&first.project_id, first_id)
        .await
        .unwrap();
    assert_eq!(canceled.status_scan.as_ref().unwrap().state, "canceled");
    assert_eq!(
        canceled
            .status_scan
            .as_ref()
            .unwrap()
            .pending_member_ids
            .len(),
        5
    );
    let next = catalog
        .start_refresh(&engine, &first.project_id)
        .await
        .unwrap();
    let next_id = next.status_scan.as_ref().unwrap().scan_id.clone();
    assert_ne!(first_id, &next_id);
    let still_running = catalog
        .cancel_scan(&first.project_id, first_id)
        .await
        .unwrap();
    assert_eq!(still_running.status_scan.as_ref().unwrap().scan_id, next_id);
    assert_eq!(still_running.status_scan.as_ref().unwrap().state, "running");
    drop(permits);
    let final_state = until(&catalog, &first.project_id, |p| {
        p.status_scan.as_ref().unwrap().state == "complete"
    })
    .await;
    assert_eq!(final_state.status_scan.as_ref().unwrap().scan_id, next_id);
    let _permit =
        tokio::time::timeout(Duration::from_secs(2), catalog.read_permits.acquire_many(4))
            .await
            .unwrap()
            .unwrap();
}

#[tokio::test]
async fn reload_and_save_win_over_old_scans_even_with_the_same_revision() {
    let (_temp, path) = fixture(2);
    let catalog = Catalog::new();
    let engine = GitEngine::isolated();
    let permits = catalog
        .read_permits
        .clone()
        .acquire_many_owned(4)
        .await
        .unwrap();
    let old = catalog.start_open(&engine, &path).await.unwrap();
    let newer = catalog.start_open(&engine, &path).await.unwrap();
    let ignored = catalog
        .start_scan(
            &engine,
            old.clone(),
            Some(old.status_scan.as_ref().map(|s| s.scan_id.clone())),
        )
        .await
        .unwrap();
    assert_eq!(
        ignored.status_scan.as_ref().unwrap().scan_id,
        newer.status_scan.as_ref().unwrap().scan_id
    );
    let mut content = old.content.clone();
    content.name = "External edit".into();
    content.repositories.truncate(1);
    std::fs::write(&path, serde_json::to_vec(&content).unwrap()).unwrap();
    let stale = catalog
        .start_refresh(&engine, &old.project_id)
        .await
        .unwrap();
    assert!(stale.stale);
    assert_eq!(stale.members.len(), 2);
    let reloaded = catalog.start_open(&engine, &path).await.unwrap();
    assert_eq!(reloaded.members.len(), 1);
    drop(permits);
    let completed = until(&catalog, &old.project_id, |p| {
        p.status_scan.as_ref().unwrap().state == "complete"
    })
    .await;
    assert_eq!(completed.name, "External edit");
    assert_eq!(completed.members.len(), 1);
    let mut saved_content = completed.content.clone();
    saved_content.name = "Saved".into();
    catalog
        .start_refresh(&engine, &completed.project_id)
        .await
        .unwrap();
    let saved = catalog
        .save(
            &engine,
            &completed.project_id,
            &completed.revision,
            saved_content,
        )
        .await
        .unwrap();
    assert_eq!(saved.name, "Saved");
    assert!(!saved.stale);
    assert_eq!(
        catalog.get(&saved.project_id).await.unwrap().revision,
        saved.revision
    );
}

#[tokio::test]
async fn empty_project_finishes_without_a_worker() {
    let (_temp, path) = fixture(0);
    let catalog = Catalog::new();
    let project = catalog
        .start_open(&GitEngine::isolated(), &path)
        .await
        .unwrap();
    assert_eq!(project.status_scan.unwrap().state, "complete");
    assert!(catalog.scans.lock().await.is_empty());
}

#[tokio::test]
#[ignore = "Explicit B2 25-repository measurement and separate 100-repository stress run"]
async fn progressive_group_measurement() {
    for count in [15, 25, 100] {
        let (_temp, path) = fixture(count);
        let catalog = Catalog::new();
        let engine = GitEngine::isolated();
        let start = Instant::now();
        let initial = catalog.start_open(&engine, &path).await.unwrap();
        let shell = start.elapsed();
        until(&catalog, &initial.project_id, |p| {
            p.status_scan.as_ref().unwrap().pending_member_ids.len() < count
        })
        .await;
        let first = start.elapsed();
        until(&catalog, &initial.project_id, |p| {
            p.status_scan.as_ref().unwrap().state == "complete"
        })
        .await;
        println!(
            "B2 OS={} ARCH={} COUNT={count} INITIAL_MS={:.2} FIRST_ROW_MS={:.2} COMPLETE_MS={:.2}",
            std::env::consts::OS,
            std::env::consts::ARCH,
            shell.as_secs_f64() * 1000.0,
            first.as_secs_f64() * 1000.0,
            start.elapsed().as_secs_f64() * 1000.0
        );
    }
}
