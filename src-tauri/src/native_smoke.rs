//! Compiled only for disposable, real-WebView acceptance runs.
use crate::{commands::AppState, contracts::*};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    process::Command,
};
use tauri::State;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Context {
    pub project_path: String,
    pub clean_path: String,
    pub dirty_path: String,
    pub merge_path: String,
}
pub struct Fixture {
    pub context: Context,
    pub report: PathBuf,
    pub _root: tempfile::TempDir,
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
        },
        report,
        _root: root,
    }
}
fn fixture(state: &AppState) -> AppResult<&Fixture> {
    state
        .smoke
        .as_ref()
        .ok_or_else(|| AppError::new("smoke_disabled", "No native fixture active."))
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
