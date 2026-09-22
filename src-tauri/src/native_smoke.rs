//! Compiled only for disposable, real-WebView acceptance runs.
use crate::{commands::AppState, contracts::*};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    process::Command,
};
use tauri::State;

#[tauri::command]
pub async fn native_smoke_progress(
    state: State<'_, AppState>,
    checks: Vec<String>,
) -> AppResult<()> {
    let f = fixture(&state)?;
    std::fs::write(
        f.report.with_extension("progress.json"),
        serde_json::to_vec_pretty(&checks).unwrap(),
    )
    .map_err(|e| AppError::new("report_failed", e.to_string()))
}

#[tauri::command]
pub async fn native_smoke_focus(window: tauri::WebviewWindow) -> AppResult<()> {
    window
        .set_focus()
        .map_err(|e| AppError::new("fixture", e.to_string()))
}

#[tauri::command]
pub async fn native_smoke_appearance(
    window: tauri::WebviewWindow,
    theme: String,
    width: f64,
    height: f64,
) -> AppResult<()> {
    let theme = match theme.as_str() {
        "light" => tauri::Theme::Light,
        "dark" => tauri::Theme::Dark,
        _ => return Err(AppError::new("fixture", "Unsupported theme")),
    };
    window
        .set_theme(Some(theme))
        .map_err(|e| AppError::new("fixture", e.to_string()))?;
    window
        .set_size(tauri::LogicalSize::new(width, height))
        .map_err(|e| AppError::new("fixture", e.to_string()))?;
    window
        .set_focus()
        .map_err(|e| AppError::new("fixture", e.to_string()))?;
    Ok(())
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Context {
    pub project_path: String,
    pub clean_path: String,
    pub dirty_path: String,
    pub merge_path: String,
    pub m2: bool,
    pub m3: bool,
    pub performance: bool,
    pub discovery: bool,
    pub history_search: bool,
    pub remote_checkout: bool,
    pub cherry_pick: bool,
    pub stash: bool,
}
pub struct Fixture {
    pub context: Context,
    pub report: PathBuf,
    pub _root: tempfile::TempDir,
    pub cases: std::sync::Mutex<std::collections::HashMap<String, PathBuf>>,
    pub rewrites: std::sync::Mutex<std::collections::HashMap<String, serde_json::Value>>,
    pub slow_commit: tokio::sync::Mutex<Option<serde_json::Value>>,
    pub remote_checkouts: std::sync::Mutex<std::collections::HashMap<String, serde_json::Value>>,
    pub cherry_picks: std::sync::Mutex<std::collections::HashMap<String, serde_json::Value>>,
    pub stashes: std::sync::Mutex<std::collections::HashMap<String, serde_json::Value>>,
    pub slow_stash: tokio::sync::Mutex<Option<serde_json::Value>>,
}
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
        .expect("fixture Git");
    assert!(
        output.status.success(),
        "fixture git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().into()
}
fn configure(path: &Path) {
    for (k, v) in [
        ("user.name", "Native Fixture"),
        ("user.email", "native@example.invalid"),
        ("commit.gpgsign", "false"),
        ("core.hooksPath", ".disabled-hooks"),
        ("core.autocrlf", "false"),
    ] {
        git(path, &["config", k, v]);
    }
}
fn commit(path: &Path, message: &str) {
    git(path, &["add", "--all"]);
    git(path, &["commit", "-m", message]);
}
pub fn setup(report: PathBuf) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let seed = root.path().join("seed");
    std::fs::create_dir(&seed).unwrap();
    git(&seed, &["init", "-b", "main"]);
    configure(&seed);
    std::fs::write(seed.join("first.txt"), "first base\n").unwrap();
    std::fs::write(seed.join("second.txt"), "second base\n").unwrap();
    commit(&seed, "Initial native fixture");
    let remote = root.path().join("remote.git");
    git(
        root.path(),
        &[
            "clone",
            "--bare",
            seed.to_str().unwrap(),
            remote.to_str().unwrap(),
        ],
    );
    for name in ["Clean", "Dirty", "Diverged"] {
        let p = root.path().join(name);
        git(
            root.path(),
            &["clone", remote.to_str().unwrap(), p.to_str().unwrap()],
        );
        configure(&p);
    }
    std::fs::write(
        root.path().join("Dirty/first.txt"),
        "dirty unchanged by batch\n",
    )
    .unwrap();
    std::fs::write(root.path().join("Diverged/local.txt"), "local\n").unwrap();
    commit(&root.path().join("Diverged"), "Local divergence");
    std::fs::write(seed.join("remote.txt"), "remote\n").unwrap();
    commit(&seed, "Remote update");
    git(&seed, &["push", remote.to_str().unwrap(), "main"]);
    let merge = root.path().join("Merge");
    git(
        root.path(),
        &["clone", remote.to_str().unwrap(), merge.to_str().unwrap()],
    );
    configure(&merge);
    git(&merge, &["switch", "-c", "feature"]);
    std::fs::write(merge.join("feature.txt"), "feature\n").unwrap();
    commit(&merge, "Feature change");
    git(&merge, &["switch", "main"]);
    std::fs::write(merge.join("main.txt"), "main\n").unwrap();
    commit(&merge, "Main change");
    git(&merge, &["merge", "--no-edit", "feature"]);
    let project = root.path().join("native.git-view.json");
    std::fs::write(&project,r#"{"version":1,"name":"Native acceptance","repositories":[{"path":"Clean","label":"Clean"},{"path":"Dirty","label":"Dirty"},{"path":"Diverged","label":"Diverged"},{"path":"Missing","label":"Missing"}]}"#).unwrap();
    Fixture {
        context: Context {
            project_path: project.to_str().unwrap().into(),
            clean_path: root.path().join("Clean").to_str().unwrap().into(),
            dirty_path: root.path().join("Dirty").to_str().unwrap().into(),
            merge_path: merge.to_str().unwrap().into(),
            m2: true,
            m3: true,
            performance: true,
            discovery: true,
            history_search: true,
            remote_checkout: true,
            cherry_pick: true,
            stash: true,
        },
        report,
        _root: root,
        cases: Default::default(),
        rewrites: Default::default(),
        slow_commit: Default::default(),
        remote_checkouts: Default::default(),
        cherry_picks: Default::default(),
        stashes: Default::default(),
        slow_stash: Default::default(),
    }
}

#[tauri::command]
pub async fn native_smoke_remote_checkout_case(
    state: State<'_, AppState>,
    scenario: String,
) -> AppResult<serde_json::Value> {
    if !matches!(scenario.as_str(), "success" | "dirty" | "collision") {
        return Err(AppError::new("fixture", "Unknown remote checkout scenario"));
    }
    let f = fixture(&state)?;
    let root = f._root.path().join(format!("remote-checkout-{scenario}"));
    std::fs::create_dir(&root).map_err(|e| AppError::new("fixture", e.to_string()))?;
    let seed = root.join("seed");
    std::fs::create_dir(&seed).unwrap();
    git(&seed, &["init", "--template=", "-b", "main"]);
    configure(&seed);
    std::fs::write(seed.join("checkout.txt"), "base\n").unwrap();
    commit(&seed, "Checkout main");
    let original_head = git(&seed, &["rev-parse", "HEAD"]);
    git(&seed, &["switch", "-c", "feature/topic"]);
    std::fs::write(seed.join("checkout.txt"), "origin feature\n").unwrap();
    commit(&seed, "Origin topic");
    let origin_oid = git(&seed, &["rev-parse", "HEAD"]);
    let origin = root.join("origin.git");
    git(
        &root,
        &[
            "clone",
            "--bare",
            seed.to_str().unwrap(),
            origin.to_str().unwrap(),
        ],
    );
    std::fs::write(seed.join("checkout.txt"), "review feature\n").unwrap();
    commit(&seed, "Review topic");
    let source_oid = git(&seed, &["rev-parse", "HEAD"]);
    let remote = root.join("review.git");
    git(
        &root,
        &[
            "clone",
            "--bare",
            seed.to_str().unwrap(),
            remote.to_str().unwrap(),
        ],
    );
    let path = root.join("checkout");
    git(
        &root,
        &[
            "clone",
            "--branch",
            "main",
            origin.to_str().unwrap(),
            path.to_str().unwrap(),
        ],
    );
    configure(&path);
    git(
        &path,
        &["remote", "add", "review", remote.to_str().unwrap()],
    );
    git(&path, &["fetch", "review"]);
    if scenario == "dirty" {
        std::fs::write(path.join("keep.txt"), "untracked work\n").unwrap();
    } else if scenario == "collision" {
        git(&path, &["branch", "custom/topic"]);
        git(
            &path,
            &["branch", "--set-upstream-to=origin/main", "custom/topic"],
        );
    }
    let opened = state.engine.open_repository(&path).await?;
    let result = serde_json::json!({
        "repositoryId": opened.repository_id, "path": opened.display_path,
        "remoteId": "review", "sourceRef": "refs/remotes/review/feature/topic",
        "sourceOid": source_oid, "upstreamRef": "refs/heads/feature/topic",
        "suggestedName": "feature/topic", "localName": "custom/topic"
    });
    f.remote_checkouts.lock().unwrap().insert(
        opened.repository_id.clone(),
        serde_json::json!({
            "scenario": scenario, "path": opened.display_path, "originalHead": original_head,
            "sourceOid": source_oid, "originOid": origin_oid,
            "originPath": origin, "remotePath": remote
        }),
    );
    state.launches.push(LaunchRequest {
        request_id: uuid::Uuid::new_v4().to_string(),
        kind: "repository".into(),
        path: Some(opened.display_path),
        error: None,
        scan_depth: None,
    });
    Ok(result)
}

#[tauri::command]
pub async fn native_smoke_remote_checkout_verify(
    state: State<'_, AppState>,
    repository_id: String,
    scenario: String,
    local_name: String,
) -> AppResult<serde_json::Value> {
    let f = fixture(&state)?;
    let record = f
        .remote_checkouts
        .lock()
        .unwrap()
        .get(&repository_id)
        .cloned()
        .ok_or_else(|| AppError::new("fixture", "Unknown checkout fixture"))?;
    if record["scenario"] != scenario || local_name != "custom/topic" {
        return Err(AppError::new(
            "fixture",
            "Checkout fixture arguments differ",
        ));
    }
    let path = Path::new(record["path"].as_str().unwrap());
    let expected_head = if scenario == "success" {
        "sourceOid"
    } else {
        "originalHead"
    };
    let branch = if scenario == "success" {
        "custom/topic"
    } else {
        "main"
    };
    let content = if scenario == "success" {
        "review feature\n"
    } else {
        "base\n"
    };
    let mut verified = git(path, &["rev-parse", "HEAD"]) == record[expected_head].as_str().unwrap()
        && git(path, &["rev-parse", "refs/heads/main"]) == record["originalHead"].as_str().unwrap()
        && git(path, &["symbolic-ref", "--short", "HEAD"]) == branch
        && std::fs::read_to_string(path.join("checkout.txt")).unwrap() == content
        && git(path, &["diff", "--cached"]).is_empty()
        && git(path, &["diff"]).is_empty()
        && git(
            Path::new(record["originPath"].as_str().unwrap()),
            &["rev-parse", "refs/heads/feature/topic"],
        ) == record["originOid"].as_str().unwrap()
        && git(
            Path::new(record["remotePath"].as_str().unwrap()),
            &["rev-parse", "refs/heads/feature/topic"],
        ) == record["sourceOid"].as_str().unwrap();
    if scenario == "success" {
        verified &= git(path, &["config", "--get", "branch.custom/topic.remote"]) == "review"
            && git(path, &["config", "--get", "branch.custom/topic.merge"])
                == "refs/heads/feature/topic";
    } else if scenario == "collision" {
        verified &= git(path, &["rev-parse", "refs/heads/custom/topic"])
            == record["originalHead"].as_str().unwrap()
            && git(path, &["config", "--get", "branch.custom/topic.remote"]) == "origin"
            && git(path, &["config", "--get", "branch.custom/topic.merge"]) == "refs/heads/main";
    } else {
        verified &= std::fs::read_to_string(path.join("keep.txt")).unwrap() == "untracked work\n"
            && !git(path, &["config", "--name-only", "--list"])
                .lines()
                .any(|key| key.starts_with("branch.custom/topic."))
            && git(
                path,
                &[
                    "for-each-ref",
                    "--format=%(refname)",
                    "refs/heads/custom/topic",
                ],
            )
            .is_empty();
    }
    if !verified {
        return Err(AppError::new(
            "fixture",
            "Remote checkout changed unexpected state",
        ));
    }
    Ok(serde_json::json!({ "verified": true }))
}

#[tauri::command]
pub async fn native_smoke_cherry_pick_case(
    state: State<'_, AppState>,
    scenario: String,
) -> AppResult<serde_json::Value> {
    if !matches!(
        scenario.as_str(),
        "success" | "conflictContinue" | "conflictAbort" | "empty"
    ) {
        return Err(AppError::new("fixture", "Unknown cherry-pick scenario"));
    }
    let f = fixture(&state)?;
    let existing = f.cherry_picks.lock().unwrap().get(&scenario).cloned();
    let result = if let Some(record) = existing {
        record
    } else {
        let path = f._root.path().join(format!("cherry-pick-{scenario}"));
        std::fs::create_dir(&path).unwrap();
        git(&path, &["init", "--template=", "-b", "main"]);
        configure(&path);
        std::fs::write(path.join("base.txt"), "base\n").unwrap();
        if scenario == "conflictAbort" {
            std::fs::write(path.join("conflict.txt"), "base\n").unwrap();
        }
        commit(&path, "Cherry-pick base");
        git(&path, &["switch", "-c", "feature"]);
        let conflict = scenario.starts_with("conflict");
        let changed_path = if conflict {
            "conflict.txt"
        } else {
            "feature.txt"
        };
        std::fs::write(path.join(changed_path), "feature\n").unwrap();
        git(&path, &["add", "--all"]);
        git(
            &path,
            &[
                "commit",
                "--author=Cherry Source <source@example.invalid>",
                "-m",
                "Cherry-pick source\n\nPreserve this source message.",
            ],
        );
        let source_oid = git(&path, &["rev-parse", "HEAD"]);
        git(&path, &["switch", "main"]);
        std::fs::write(path.join("main.txt"), "main\n").unwrap();
        if scenario == "conflictContinue" {
            std::fs::write(path.join("conflict.txt"), "main\n").unwrap();
        } else if scenario == "conflictAbort" {
            std::fs::remove_file(path.join("conflict.txt")).unwrap();
        } else if scenario == "empty" {
            std::fs::write(path.join("feature.txt"), "feature\n").unwrap();
        }
        commit(&path, "Cherry-pick target");
        let target_oid = git(&path, &["rev-parse", "HEAD"]);
        let target_tree = git(&path, &["rev-parse", "HEAD^{tree}"]);
        let opened = state.engine.open_repository(&path).await?;
        f.cases
            .lock()
            .unwrap()
            .insert(opened.repository_id.clone(), path);
        let record = serde_json::json!({
            "repositoryId": opened.repository_id, "path": opened.display_path,
            "sourceOid": source_oid, "targetOid": target_oid, "targetTree": target_tree,
            "sourceRef": "refs/heads/feature",
            "conflictPaths": if conflict { vec!["conflict.txt"] } else { vec![] },
            "scenario": scenario
        });
        f.cherry_picks
            .lock()
            .unwrap()
            .insert(scenario.clone(), record.clone());
        record
    };
    state.launches.push(LaunchRequest {
        request_id: uuid::Uuid::new_v4().to_string(),
        kind: "repository".into(),
        path: Some(result["path"].as_str().unwrap().into()),
        error: None,
        scan_depth: None,
    });
    Ok(result)
}

#[tauri::command]
pub async fn native_smoke_cherry_pick_verify(
    state: State<'_, AppState>,
    repository_id: String,
    scenario: String,
) -> AppResult<serde_json::Value> {
    let record = fixture(&state)?
        .cherry_picks
        .lock()
        .unwrap()
        .get(&scenario)
        .cloned()
        .ok_or_else(|| AppError::new("fixture", "Unknown cherry-pick fixture"))?;
    if record["repositoryId"] != repository_id {
        return Err(AppError::new(
            "fixture",
            "Cherry-pick fixture arguments differ",
        ));
    }
    let path = Path::new(record["path"].as_str().unwrap());
    let source = record["sourceOid"].as_str().unwrap();
    let target = record["targetOid"].as_str().unwrap();
    let status = state.engine.get_status(&repository_id).await?;
    let mut verified = status.git_state == GitState::Idle
        && status.operation_context.is_none()
        && !status.dirty
        && git(path, &["status", "--porcelain"]).is_empty()
        && git(path, &["symbolic-ref", "--short", "HEAD"]) == "main"
        && git(path, &["rev-parse", "refs/heads/feature"]) == source
        && std::fs::read_to_string(path.join("main.txt")).unwrap() == "main\n";
    if matches!(scenario.as_str(), "success" | "conflictContinue") {
        let head = git(path, &["rev-parse", "HEAD"]);
        verified &= head != source
            && head != target
            && git(path, &["show", "-s", "--format=%P", "HEAD"]) == target
            && git(path, &["show", "-s", "--format=%an%n%ae%n%aI%n%B", "HEAD"])
                == git(path, &["show", "-s", "--format=%an%n%ae%n%aI%n%B", source]);
        let (file, expected) = if scenario == "success" {
            ("feature.txt", "feature\n")
        } else {
            ("conflict.txt", "resolved both sides\n")
        };
        verified &= std::fs::read_to_string(path.join(file)).unwrap() == expected;
    } else {
        verified &= git(path, &["rev-parse", "HEAD"]) == target
            && git(path, &["rev-parse", "HEAD^{tree}"]) == record["targetTree"].as_str().unwrap();
        if scenario == "conflictAbort" {
            verified &= !path.join("conflict.txt").exists();
        }
    }
    if !verified {
        return Err(AppError::new(
            "fixture",
            "Cherry-pick changed unexpected state or metadata",
        ));
    }
    Ok(serde_json::json!({ "verified": true }))
}

#[tauri::command]
pub async fn native_smoke_slow_commit(state: State<'_, AppState>) -> AppResult<serde_json::Value> {
    let f = fixture(&state)?;
    let mut cached = f.slow_commit.lock().await;
    if cached.is_none() {
        let path = f._root.path().join("reload-operation");
        std::fs::create_dir(&path).unwrap();
        git(&path, &["init", "-b", "main"]);
        configure(&path);
        std::fs::write(path.join("reload.txt"), "before\n").unwrap();
        commit(&path, "Before reload");
        std::fs::write(path.join("reload.txt"), "after\n").unwrap();
        git(&path, &["add", "reload.txt"]);
        let hooks = path.join(".git/reload-hooks");
        std::fs::create_dir(&hooks).unwrap();
        let hook = hooks.join("pre-commit");
        std::fs::write(&hook, "#!/bin/sh\nsleep 8\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        git(
            &path,
            &["config", "core.hooksPath", hooks.to_str().unwrap()],
        );
        let opened = state.engine.open_repository(&path).await?;
        let status = state.engine.get_status(&opened.repository_id).await?;
        let plan = state
            .operations
            .plan_action(
                &opened.repository_id,
                RepositoryAction::Commit {
                    status_snapshot_id: status.snapshot_id,
                    message: "Commit surviving WebView reload".into(),
                },
            )
            .await;
        let RepositoryActionPlan::Eligible { plan_id, .. } = plan else {
            return Err(AppError::new("fixture", "Slow commit plan was blocked"));
        };
        let operation = state.operations.run_action(&plan_id).await?;
        *cached = Some(serde_json::json!({ "repositoryId": opened.repository_id,
            "operationId": operation.operation_id, "path": opened.display_path }));
    }
    let response = cached.as_ref().unwrap().clone();
    state.launches.push(LaunchRequest {
        scan_depth: None,
        request_id: uuid::Uuid::new_v4().to_string(),
        kind: "repository".into(),
        path: Some(response["path"].as_str().unwrap().into()),
        error: None,
    });
    Ok(response)
}
fn fixture(state: &AppState) -> AppResult<&Fixture> {
    state
        .smoke
        .as_ref()
        .ok_or_else(|| AppError::new("smoke_disabled", "No native fixture active."))
}

#[tauri::command]
pub async fn native_smoke_m2_case(
    state: State<'_, AppState>,
    scenario: String,
) -> AppResult<serde_json::Value> {
    if ![
        "merge",
        "squash",
        "mergeContinue",
        "mergeAbort",
        "squashConflict",
    ]
    .contains(&scenario.as_str())
    {
        return Err(AppError::new("fixture", "Unsupported merge case"));
    }
    let f = fixture(&state)?;
    let path = f
        ._root
        .path()
        .join(format!("{scenario}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&path).unwrap();
    git(&path, &["init", "-b", "main"]);
    configure(&path);
    std::fs::write(path.join("conflict.txt"), "base\n").unwrap();
    commit(&path, "Base");
    git(&path, &["switch", "-c", "feature"]);
    let conflict = !["merge", "squash"].contains(&scenario.as_str());
    let file = if conflict {
        "conflict.txt"
    } else {
        "feature.txt"
    };
    std::fs::write(path.join(file), "feature\n").unwrap();
    commit(&path, "Feature");
    git(&path, &["switch", "main"]);
    std::fs::write(
        path.join(if conflict { "conflict.txt" } else { "main.txt" }),
        "main\n",
    )
    .unwrap();
    commit(&path, "Main");
    let opened = state.engine.open_repository(&path).await?;
    f.cases
        .lock()
        .unwrap()
        .insert(opened.repository_id.clone(), path);
    state.launches.push(LaunchRequest {
        scan_depth: None,
        request_id: uuid::Uuid::new_v4().to_string(),
        kind: "repository".into(),
        path: Some(opened.display_path.clone()),
        error: None,
    });
    Ok(
        serde_json::json!({"repositoryId":opened.repository_id,"path":opened.display_path,"sourceRef":"refs/heads/feature"}),
    )
}

#[tauri::command]
pub async fn native_smoke_resolve_conflicts(
    state: State<'_, AppState>,
    repository_id: String,
) -> AppResult<()> {
    let path = fixture(&state)?
        .cases
        .lock()
        .unwrap()
        .get(&repository_id)
        .cloned()
        .ok_or_else(|| AppError::new("fixture", "Unknown test-owned repository"))?;
    std::fs::write(path.join("conflict.txt"), "resolved both sides\n")
        .map_err(|e| AppError::new("fixture", e.to_string()))?;
    Ok(())
}

#[tauri::command]
pub async fn native_smoke_forward_launch(
    state: State<'_, AppState>,
    mode: String,
) -> AppResult<serde_json::Value> {
    let fixture = fixture(&state)?;
    let executable =
        std::env::current_exe().map_err(|e| AppError::new("test_launch", e.to_string()))?;
    let root = fixture._root.path();
    let (cwd, args) = match mode.as_str() {
        "repository" => (Path::new(&fixture.context.clean_path), vec!["repo", "."]),
        "welcome" => (root, vec![]),
        "invalid" => (root, vec!["repo", "missing-native-repository"]),
        "project" => (root, vec!["group", "native.git-view.json"]),
        _ => return Err(AppError::new("test_launch", "Unknown test mode")),
    };
    let output = tokio::process::Command::new(executable)
        .current_dir(cwd)
        .env_remove("GIT_VIEW_NATIVE_SMOKE_REPORT")
        .args(args)
        .output()
        .await
        .map_err(|e| AppError::new("test_launch", e.to_string()))?;
    Ok(
        serde_json::json!({"exitCode":output.status.code(),"stdout":String::from_utf8_lossy(&output.stdout),"stderr":String::from_utf8_lossy(&output.stderr)}),
    )
}
#[tauri::command]
pub fn native_smoke_context(state: State<'_, AppState>) -> AppResult<Context> {
    Ok(fixture(&state)?.context.clone())
}
#[tauri::command]
pub async fn native_smoke_prepare_changes(
    state: State<'_, AppState>,
    repository_id: String,
) -> AppResult<serde_json::Value> {
    let f = fixture(&state)?;
    let path = Path::new(&f.context.clean_path);
    let opened = state.engine.open_repository(path).await?;
    if opened.repository_id != repository_id {
        return Err(AppError::new(
            "wrong_fixture",
            "Only test-owned clean repository may be changed.",
        ));
    }
    std::fs::write(path.join("first.txt"), "first native change\n").unwrap();
    std::fs::write(path.join("second.txt"), "second native change\n").unwrap();
    Ok(serde_json::json!({"firstFile":"first.txt","secondFile":"second.txt"}))
}
#[tauri::command]
pub async fn native_smoke_fail_hook(
    state: State<'_, AppState>,
    repository_id: String,
) -> AppResult<()> {
    let f = fixture(&state)?;
    let path = Path::new(&f.context.clean_path);
    if state.engine.open_repository(path).await?.repository_id != repository_id {
        return Err(AppError::new("wrong_fixture", "Wrong fixture"));
    }
    let hooks = path.join(".git/native-hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    let hook = hooks.join("pre-commit");
    std::fs::write(
        &hook,
        "#!/bin/sh\necho 'Native fixture hook rejected commit' >&2\nexit 1\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    git(path, &["config", "core.hooksPath", hooks.to_str().unwrap()]);
    Ok(())
}
#[tauri::command]
pub fn native_smoke_report(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    result: serde_json::Value,
) -> AppResult<()> {
    let f = fixture(&state)?;
    let mut result = result;
    let path = Path::new(&f.context.clean_path);
    result["cleanHead"] = git(path, &["rev-parse", "HEAD"]).into();
    result["dirtyContents"] =
        std::fs::read_to_string(Path::new(&f.context.dirty_path).join("first.txt"))
            .unwrap()
            .into();
    std::fs::write(&f.report, serde_json::to_vec_pretty(&result).unwrap())
        .map_err(|e| AppError::new("report_failed", e.to_string()))?;
    let exit = if result["passed"] == true { 0 } else { 1 };
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(200));
        app.exit(exit);
    });
    Ok(())
}

#[tauri::command]
pub async fn native_smoke_m3_case(
    state: State<'_, AppState>,
    scenario: String,
) -> AppResult<serde_json::Value> {
    if !["combine", "hookFailure", "staleRecovery"].contains(&scenario.as_str()) {
        return Err(AppError::new("fixture", "Unknown rewrite case"));
    }
    let f = fixture(&state)?;
    let path = f
        ._root
        .path()
        .join(format!("rewrite-{scenario}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&path).unwrap();
    git(&path, &["init", "-b", "main"]);
    configure(&path);
    std::fs::write(path.join("first.txt"), "root\n").unwrap();
    std::fs::write(path.join("second.txt"), "root second\n").unwrap();
    commit(&path, "Root");
    for (message, body) in [
        ("First linear change", "first\n"),
        ("Second linear change", "second\n"),
        ("Third linear change", "third\n"),
    ] {
        std::fs::write(path.join("first.txt"), body).unwrap();
        commit(&path, message);
    }
    if scenario == "hookFailure" {
        let hooks = path.join(".git/native-rewrite-hooks");
        std::fs::create_dir(&hooks).unwrap();
        let hook = hooks.join("pre-commit");
        std::fs::write(
            &hook,
            "#!/bin/sh\necho 'Rewrite fixture hook rejected commit' >&2\nexit 1\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        git(
            &path,
            &["config", "core.hooksPath", hooks.to_str().unwrap()],
        );
    }
    let opened = state.engine.open_repository(&path).await?;
    let oids = git(&path, &["rev-list", "--max-count=2", "HEAD"])
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let base = git(&path, &["rev-parse", "HEAD~2"]);
    let tree = git(&path, &["rev-parse", "HEAD^{tree}"]);
    let original = git(&path, &["rev-parse", "HEAD"]);
    f.cases
        .lock()
        .unwrap()
        .insert(opened.repository_id.clone(), path);
    f.rewrites.lock().unwrap().insert(opened.repository_id.clone(),serde_json::json!({"originalHead":original,"baseOid":base,"treeOid":tree,"scenario":scenario}));
    state.launches.push(LaunchRequest {
        scan_depth: None,
        request_id: uuid::Uuid::new_v4().to_string(),
        kind: "repository".into(),
        path: Some(opened.display_path.clone()),
        error: None,
    });
    Ok(
        serde_json::json!({"repositoryId":opened.repository_id,"path":opened.display_path,"commitOids":oids,"baseOid":base,"treeOid":tree}),
    )
}

#[tauri::command]
pub async fn native_smoke_m3_edit(
    state: State<'_, AppState>,
    repository_id: String,
    commit: bool,
) -> AppResult<()> {
    let f = fixture(&state)?;
    let path = f
        .cases
        .lock()
        .unwrap()
        .get(&repository_id)
        .cloned()
        .ok_or_else(|| AppError::new("fixture", "Unknown rewrite fixture"))?;
    std::fs::write(path.join("first.txt"), "later staged content\n").unwrap();
    git(&path, &["add", "first.txt"]);
    std::fs::write(path.join("first.txt"), "later unstaged content\n").unwrap();
    std::fs::write(path.join("untracked.txt"), "later untracked content\n").unwrap();
    if commit {
        git(&path, &["add", "--all"]);
        git(&path, &["commit", "-m", "Later user commit"]);
    }
    let mut records = f.rewrites.lock().unwrap();
    let record = records
        .get_mut(&repository_id)
        .ok_or_else(|| AppError::new("fixture", "Missing fixture expectation"))?;
    record["laterHead"] = git(&path, &["rev-parse", "HEAD"]).into();
    record["indexTree"] = git(&path, &["write-tree"]).into();
    record["firstBytes"] = std::fs::read_to_string(path.join("first.txt"))
        .unwrap()
        .into();
    record["untrackedBytes"] = std::fs::read_to_string(path.join("untracked.txt"))
        .unwrap()
        .into();
    Ok(())
}

#[tauri::command]
pub async fn native_smoke_m3_verify(
    state: State<'_, AppState>,
    repository_id: String,
    recovery_id: String,
    expected: String,
) -> AppResult<serde_json::Value> {
    let f = fixture(&state)?;
    let path = f
        .cases
        .lock()
        .unwrap()
        .get(&repository_id)
        .cloned()
        .ok_or_else(|| AppError::new("fixture", "Unknown fixture"))?;
    let expected_state = f
        .rewrites
        .lock()
        .unwrap()
        .get(&repository_id)
        .cloned()
        .ok_or_else(|| AppError::new("fixture", "Missing expected state"))?;
    let records = state.engine.list_rewrites(&repository_id).await?;
    let record = records
        .iter()
        .find(|r| r.recovery_id == recovery_id)
        .ok_or_else(|| AppError::new("fixture", "Recovery journal absent"))?;
    let fail = |message: &str| AppError::new("native_assertion", message);
    let original = expected_state["originalHead"].as_str().unwrap();
    let head = git(&path, &["rev-parse", "HEAD"]);
    if git(&path, &["rev-parse", &record.backup_ref]) != original {
        return Err(fail("Backup must preserve original tip"));
    }
    match expected.as_str() {
        "combined" => {
            if Some(head.as_str()) != record.replacement_head.as_deref() || head == original {
                return Err(fail("Replacement HEAD is incorrect"));
            }
            if git(&path, &["rev-parse", "HEAD^{tree}"])
                != expected_state["treeOid"].as_str().unwrap()
            {
                return Err(fail("Replacement tree changed"));
            }
            if git(&path, &["rev-parse", "HEAD^"]) != expected_state["baseOid"].as_str().unwrap() {
                return Err(fail("Replacement parent incorrect"));
            }
            if git(&path, &["rev-list", "--count", "HEAD"]) != "3" {
                return Err(fail("Two selected commits were not combined into one"));
            }
        }
        "recovered" | "hookFailure" => {
            if head != original {
                return Err(fail("Original history was not preserved/restored"));
            }
        }
        "stale" => {
            if head != expected_state["laterHead"].as_str().unwrap() {
                return Err(fail("Later user commit changed"));
            }
        }
        _ => return Err(fail("Unknown verification mode")),
    }
    if expected_state.get("indexTree").is_some()
        && (git(&path, &["write-tree"]) != expected_state["indexTree"].as_str().unwrap()
            || std::fs::read_to_string(path.join("first.txt")).unwrap()
                != expected_state["firstBytes"].as_str().unwrap()
            || std::fs::read_to_string(path.join("untracked.txt")).unwrap()
                != expected_state["untrackedBytes"].as_str().unwrap())
    {
        return Err(fail("Recovery altered later index or working files"));
    }
    Ok(serde_json::json!({"verified":true}))
}

#[tauri::command]
pub async fn native_smoke_performance(state: State<'_, AppState>) -> AppResult<serde_json::Value> {
    use std::io::Write;
    let f = fixture(&state)?;
    let path = f._root.path().join("history-10000");
    std::fs::create_dir(&path).unwrap();
    git(&path, &["init", "-b", "main"]);
    configure(&path);
    let mut child = Command::new("git")
        .current_dir(&path)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .arg("fast-import")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    for n in 0..10000 {
        let message = format!("History fixture {n}\n");
        write!(input,"commit refs/heads/main\ncommitter Fixture <fixture@example.invalid> {} +0000\ndata {}\n{}\n",1700000000+n,message.len(),message).unwrap();
    }
    drop(input);
    if !child.wait_with_output().unwrap().status.success() {
        return Err(AppError::new("fixture", "fast-import failed"));
    }
    let opened = state.engine.open_repository(&path).await?;
    state.launches.push(LaunchRequest {
        scan_depth: None,
        request_id: uuid::Uuid::new_v4().to_string(),
        kind: "repository".into(),
        path: Some(opened.display_path.clone()),
        error: None,
    });
    Ok(serde_json::json!({"repositoryId":opened.repository_id,"path":opened.display_path}))
}

#[tauri::command]
pub async fn native_smoke_discovery_case(
    state: State<'_, AppState>,
    action: String,
) -> AppResult<serde_json::Value> {
    let f = fixture(&state)?;
    let base = dunce::canonicalize(f._root.path())
        .map_err(|error| AppError::new("fixture", error.to_string()))?;
    let root = base.join("discovery-root");
    let project = base.join("discovery.git-view.json");
    let create = |name: &str| -> AppResult<()> {
        let path = root.join(name);
        if path.join(".git").exists() {
            return Ok(());
        }
        std::fs::create_dir_all(&path)
            .map_err(|error| AppError::new("fixture", error.to_string()))?;
        git(&path, &["init", "--template=", "-b", "main"]);
        configure(&path);
        std::fs::write(
            path.join("discovery.txt"),
            format!("Discovery fixture {name}\n"),
        )
        .map_err(|error| AppError::new("fixture", error.to_string()))?;
        commit(&path, "Discovery fixture commit");
        Ok(())
    };
    match action.as_str() {
        "setup" => {
            create("Alpha")?;
            create("nested/βeta")?;
        }
        "add" => {
            if !root.is_dir() {
                return Err(AppError::new(
                    "fixture",
                    "Set up discovery before adding its fixture.",
                ));
            }
            create("Gamma")?;
        }
        "launch" | "openProject" => {
            if !root.is_dir() {
                return Err(AppError::new(
                    "fixture",
                    "Set up discovery before launching its fixture.",
                ));
            }
            let executable = std::env::current_exe()
                .map_err(|error| AppError::new("test_launch", error.to_string()))?;
            let mut command = tokio::process::Command::new(executable);
            command
                .current_dir(&root)
                .env_remove("GIT_VIEW_NATIVE_SMOKE_REPORT")
                .kill_on_drop(true);
            if action == "launch" {
                command.args(["group", "--scan", ".", "--depth", "2"]);
            } else {
                command.arg("group").arg(&project);
            }
            let output = tokio::time::timeout(std::time::Duration::from_secs(15), command.output())
                .await
                .map_err(|_| AppError::new("test_launch", "Discovery fixture launch timed out."))?
                .map_err(|error| AppError::new("test_launch", error.to_string()))?;
            if !output.status.success() {
                return Err(AppError::new(
                    "test_launch",
                    format!(
                        "Discovery fixture child failed with {:?}: {}",
                        output.status.code(),
                        String::from_utf8_lossy(&output.stderr)
                    ),
                ));
            }
        }
        "inspect" => {}
        _ => {
            return Err(AppError::new(
                "fixture",
                "Unknown discovery fixture action.",
            ))
        }
    }
    let mut candidates = Vec::new();
    for name in ["Alpha", "nested/βeta", "Gamma"] {
        let path = root.join(name);
        if path.join(".git").exists() {
            candidates.push(
                dunce::canonicalize(path)
                    .map_err(|error| AppError::new("fixture", error.to_string()))?
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    candidates.sort();
    Ok(
        serde_json::json!({"rootPath":root.to_string_lossy(),"projectPath":project.to_string_lossy(),"candidatePaths":candidates,"exists":project.is_file()}),
    )
}

#[tauri::command]
pub async fn native_smoke_history_search_case(
    state: State<'_, AppState>,
) -> AppResult<serde_json::Value> {
    let f = fixture(&state)?;
    let path = f._root.path().join("history-search");
    std::fs::create_dir(&path).map_err(|error| AppError::new("fixture", error.to_string()))?;
    git(&path, &["init", "--template=", "-b", "main"]);
    configure(&path);
    std::fs::write(path.join("search.txt"), "baseline\n").unwrap();
    commit(&path, "Search baseline");
    let base = git(&path, &["rev-parse", "HEAD"]);
    git(&path, &["config", "user.name", "Search Author"]);
    git(&path, &["config", "user.email", "search@example.invalid"]);
    std::fs::write(path.join("search.txt"), "older match\n").unwrap();
    commit(&path, "Literal [a.b] search older\n\nBodyOnlyNeedle");
    let older = git(&path, &["rev-parse", "HEAD"]);
    configure(&path);
    std::fs::write(path.join("search.txt"), "unmatched middle\n").unwrap();
    commit(&path, "Unmatched middle");
    git(&path, &["config", "user.name", "Search Author"]);
    git(&path, &["config", "user.email", "search@example.invalid"]);
    std::fs::write(path.join("search.txt"), "newer match\n").unwrap();
    commit(&path, "Literal [a.b] search newer");
    let newer = git(&path, &["rev-parse", "HEAD"]);
    git(&path, &["switch", "-c", "side", &base]);
    configure(&path);
    std::fs::write(path.join("search.txt"), "outside scope\n").unwrap();
    commit(&path, "Literal [a.b] search outside scope");
    let other = git(&path, &["rev-parse", "HEAD"]);
    git(&path, &["switch", "main"]);
    let opened = state.engine.open_repository(&path).await?;
    state.launches.push(LaunchRequest {
        scan_depth: None,
        request_id: uuid::Uuid::new_v4().to_string(),
        kind: "repository".into(),
        path: Some(opened.display_path.clone()),
        error: None,
    });
    Ok(serde_json::json!({
        "repositoryId": opened.repository_id, "path": opened.display_path,
        "message": "Literal [a.b] search", "author": "Search Author", "body": "BodyOnlyNeedle",
        "matchOids": [newer, older], "bodyOid": older, "otherOid": other,
        "scopeRef": "refs/heads/main"
    }))
}

async fn stash_fixture(state: &AppState, scenario: &str) -> AppResult<serde_json::Value> {
    if !matches!(
        scenario,
        "default" | "includeUntracked" | "conflict" | "delete" | "slow"
    ) {
        return Err(AppError::new("fixture", "Unknown stash scenario"));
    }
    let f = fixture(state)?;
    let existing = f.stashes.lock().unwrap().get(scenario).cloned();
    if let Some(record) = existing {
        return Ok(record);
    }
    let path = f._root.path().join(format!("stash-{scenario}"));
    std::fs::create_dir(&path).unwrap();
    git(&path, &["init", "--template=", "-b", "main"]);
    configure(&path);
    let tracked = if scenario == "conflict" {
        "conflict.txt"
    } else {
        "tracked.txt"
    };
    std::fs::write(path.join(tracked), "base\n").unwrap();
    std::fs::write(path.join(".gitignore"), "ignored.txt\n").unwrap();
    commit(&path, "Stash base");
    std::fs::write(path.join("ignored.txt"), "private ignored bytes\n").unwrap();
    if scenario == "conflict" {
        std::fs::write(path.join(tracked), "saved stash side\n").unwrap();
        git(&path, &["stash", "push", "-m", "Native conflicting stash"]);
        std::fs::write(path.join(tracked), "current target side\n").unwrap();
        commit(&path, "Diverged stash target");
    } else if scenario == "delete" {
        for (message, content) in [
            ("Native first stash", "first\n"),
            ("Native middle stash", "middle\n"),
            ("Native newest stash", "newest\n"),
        ] {
            std::fs::write(path.join(tracked), content).unwrap();
            git(&path, &["stash", "push", "-m", message]);
        }
        std::fs::write(path.join(tracked), "keep staged bytes\n").unwrap();
        git(&path, &["add", "--", tracked]);
        std::fs::write(path.join(tracked), "keep working bytes\n").unwrap();
        std::fs::write(path.join("new.txt"), "keep untracked bytes\n").unwrap();
    } else {
        std::fs::write(path.join(tracked), "saved index bytes\n").unwrap();
        git(&path, &["add", "--", tracked]);
        std::fs::write(path.join(tracked), "saved working bytes\n").unwrap();
        if scenario != "slow" {
            std::fs::write(path.join("new.txt"), "saved untracked bytes\n").unwrap();
        }
    }
    let oids: Vec<String> = git(&path, &["stash", "list", "--format=%H"])
        .lines()
        .map(str::to_owned)
        .collect();
    let opened = state.engine.open_repository(&path).await?;
    let result = serde_json::json!({
        "repositoryId": opened.repository_id, "path": opened.display_path,
        "trackedPath": tracked, "untrackedPath": "new.txt", "scenario": scenario,
        "stashOid": oids.first(), "selectedStashOid": if scenario == "delete" { oids.get(1) } else { None },
        "originalOids": oids, "head": git(&path, &["rev-parse", "HEAD"]),
        "originalIndex": git(&path, &["write-tree"]),
    });
    f.cases.lock().unwrap().insert(opened.repository_id, path);
    f.stashes
        .lock()
        .unwrap()
        .insert(scenario.into(), result.clone());
    Ok(result)
}

#[tauri::command]
pub async fn native_smoke_stash_case(
    state: State<'_, AppState>,
    scenario: String,
) -> AppResult<serde_json::Value> {
    let result = stash_fixture(&state, &scenario).await?;
    state.launches.push(LaunchRequest {
        request_id: uuid::Uuid::new_v4().to_string(),
        kind: "repository".into(),
        path: Some(result["path"].as_str().unwrap().into()),
        error: None,
        scan_depth: None,
    });
    Ok(result)
}

#[tauri::command]
pub async fn native_smoke_stash_verify(
    state: State<'_, AppState>,
    repository_id: String,
    scenario: String,
    stage: String,
) -> AppResult<serde_json::Value> {
    let record = fixture(&state)?
        .stashes
        .lock()
        .unwrap()
        .get(&scenario)
        .cloned()
        .ok_or_else(|| AppError::new("fixture", "Unknown stash fixture"))?;
    if record["repositoryId"] != repository_id {
        return Err(AppError::new("fixture", "Stash fixture arguments differ"));
    }
    let path = Path::new(record["path"].as_str().unwrap());
    let tracked = record["trackedPath"].as_str().unwrap();
    let oids: Vec<_> = git(path, &["stash", "list", "--format=%H"])
        .lines()
        .map(str::to_owned)
        .collect();
    let status = state.engine.get_status(&repository_id).await?;
    let mut verified = git(path, &["rev-parse", "HEAD"]) == record["head"].as_str().unwrap()
        && git(path, &["symbolic-ref", "--short", "HEAD"]) == "main"
        && std::fs::read_to_string(path.join("ignored.txt")).unwrap() == "private ignored bytes\n";
    match (scenario.as_str(), stage.as_str()) {
        ("default" | "includeUntracked" | "slow", "created") => {
            verified &= oids.len() == 1
                && status.git_state == GitState::Idle
                && status.counts.staged == 0
                && status.counts.unstaged == 0;
            if let Some(oid) = oids.first() {
                verified &= git(path, &["show", &format!("{oid}:{tracked}")])
                    == "saved working bytes"
                    && git(path, &["show", &format!("{oid}^2:{tracked}")]) == "saved index bytes"
                    && std::fs::read_to_string(path.join(tracked)).unwrap() == "base\n"
                    && git(path, &["show", &format!(":{tracked}")]) == "base";
                let parents = git(path, &["show", "-s", "--format=%P", oid]);
                verified &= parents.split_whitespace().count()
                    == if scenario == "includeUntracked" { 3 } else { 2 };
                if scenario == "includeUntracked" {
                    verified &= !path.join("new.txt").exists()
                        && !status.dirty
                        && git(path, &["show", &format!("{oid}^3:new.txt")])
                            == "saved untracked bytes";
                } else if scenario == "default" {
                    verified &= std::fs::read_to_string(path.join("new.txt")).unwrap()
                        == "saved untracked bytes\n"
                        && status.counts.untracked == 1;
                } else {
                    verified &= !status.dirty;
                }
                fixture(&state)?
                    .stashes
                    .lock()
                    .unwrap()
                    .get_mut(&scenario)
                    .unwrap()["createdOid"] = oid.clone().into();
            }
        }
        ("includeUntracked", "applied") => {
            verified &= oids.len() == 1
                && oids.first().map(String::as_str) == record["createdOid"].as_str()
                && status.git_state == GitState::Idle
                && status.counts.conflicted == 0
                && std::fs::read_to_string(path.join(tracked)).unwrap() == "saved working bytes\n"
                && git(path, &["show", &format!(":{tracked}")]) == "saved index bytes"
                && std::fs::read_to_string(path.join("new.txt")).unwrap()
                    == "saved untracked bytes\n";
        }
        ("delete", "deleted") => {
            let original: Vec<String> =
                serde_json::from_value(record["originalOids"].clone()).unwrap();
            let expected = vec![original[0].clone(), original[2].clone()];
            let backups = git(
                path,
                &["for-each-ref", "--format=%(objectname)", "refs/git-view/"],
            );
            verified &= oids == expected
                && backups.lines().any(|oid| oid == original[1])
                && git(path, &["write-tree"]) == record["originalIndex"].as_str().unwrap()
                && std::fs::read_to_string(path.join(tracked)).unwrap() == "keep working bytes\n"
                && std::fs::read_to_string(path.join("new.txt")).unwrap()
                    == "keep untracked bytes\n"
                && state
                    .engine
                    .list_stashes(&repository_id)
                    .await?
                    .recovery
                    .is_none();
        }
        ("conflict", "conflicted") => {
            verified &= oids.len() == 1
                && oids.first().map(String::as_str) == record["stashOid"].as_str()
                && status.git_state == GitState::Idle
                && status.counts.conflicted > 0
                && !path.join(".git/MERGE_HEAD").exists()
                && !path.join(".git/CHERRY_PICK_HEAD").exists()
                && !path.join(".git/sequencer").exists();
        }
        _ => {
            return Err(AppError::new(
                "fixture",
                "Unsupported stash verification stage",
            ))
        }
    }
    if !verified {
        return Err(AppError::new(
            "fixture",
            "Stash changed unexpected saved, index, or working state",
        ));
    }
    Ok(serde_json::json!({"verified": true}))
}

#[tauri::command]
pub async fn native_smoke_slow_stash(state: State<'_, AppState>) -> AppResult<serde_json::Value> {
    let f = fixture(&state)?;
    let mut cached = f.slow_stash.lock().await;
    if cached.is_none() {
        let record = stash_fixture(&state, "slow").await?;
        let path = Path::new(record["path"].as_str().unwrap());
        let hooks = path.join(".git/reload-stash-hooks");
        std::fs::create_dir(&hooks).unwrap();
        let hook = hooks.join("reference-transaction");
        std::fs::write(&hook, "#!/bin/sh\nif [ \"$1\" = prepared ]; then\n  while read old new ref; do\n    if [ \"$ref\" = refs/stash ]; then sleep 8; fi\n  done\nfi\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        git(path, &["config", "core.hooksPath", hooks.to_str().unwrap()]);
        let repository_id = record["repositoryId"].as_str().unwrap();
        let status = state.engine.get_status(repository_id).await?;
        let plan = state
            .operations
            .plan_action(
                repository_id,
                RepositoryAction::CreateStash {
                    status_snapshot_id: status.snapshot_id,
                    message: "Stash surviving WebView reload".into(),
                    include_untracked: false,
                },
            )
            .await;
        let RepositoryActionPlan::Eligible { plan_id, .. } = plan else {
            return Err(AppError::new("fixture", "Slow stash plan was blocked"));
        };
        let operation = state.operations.run_action(&plan_id).await?;
        *cached = Some(
            serde_json::json!({"repositoryId": repository_id, "path": record["path"], "operationId": operation.operation_id}),
        );
    }
    let response = cached.as_ref().unwrap().clone();
    state.launches.push(LaunchRequest {
        request_id: uuid::Uuid::new_v4().to_string(),
        kind: "repository".into(),
        path: Some(response["path"].as_str().unwrap().into()),
        error: None,
        scan_depth: None,
    });
    Ok(response)
}
