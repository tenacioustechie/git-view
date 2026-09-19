pub mod catalog;
mod commands;
pub mod contracts;
pub mod git;
pub mod launch;
#[cfg(feature = "native-smoke")]
mod native_smoke;
pub mod operations;
use tauri::{Emitter, Manager};
pub fn run() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    #[cfg(feature = "native-smoke")]
    let smoke =
        std::env::var_os("GIT_VIEW_NATIVE_SMOKE_REPORT").map(|p| native_smoke::setup(p.into()));
    #[cfg(feature = "native-smoke")]
    let args = smoke
        .as_ref()
        .map(|f| vec!["group".into(), f.context.project_path.clone()])
        .unwrap_or(args);
    if args == ["--help"] || args == ["-h"] {
        print!("{}", launch::HELP);
        return;
    }
    if args == ["--version"] {
        println!("Git View {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let cwd = std::env::current_dir().expect("Cannot read invoking working directory");
    let engine = git::GitEngine::new();
    #[cfg(feature = "native-smoke")]
    let engine = if smoke.is_some() {
        git::GitEngine::isolated()
    } else {
        engine
    };
    let catalog = catalog::Catalog::new();
    let mut initial = match launch::parse(&args, &cwd) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2)
        }
    };
    let validity = tauri::async_runtime::block_on(async {
        let path = std::path::Path::new(initial.path.as_deref().unwrap());
        if initial.kind == "project" {
            catalog::read_project(path).map(|_| ())
        } else {
            engine.open_repository(path).await.map(|_| ())
        }
    });
    if let Err(e) = validity {
        if args.is_empty() {
            initial.kind = "welcome".into();
            initial.path = None;
        } else {
            eprintln!("{e}");
            std::process::exit(2);
        }
    }
    let operations = operations::Operations::new(engine.clone(), catalog.clone());
    let launches = launch::LaunchQueue::default();
    launches.push(initial);
    let state = commands::AppState {
        engine,
        catalog,
        operations,
        launches,
        #[cfg(feature = "native-smoke")]
        smoke,
    };
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            let argv_empty = argv.len() <= 1;
            let request = match launch::parse(
                &argv.into_iter().skip(1).collect::<Vec<_>>(),
                std::path::Path::new(&cwd),
            ) {
                Ok(r) => r,
                Err(error) => contracts::LaunchRequest {
                    request_id: uuid::Uuid::new_v4().to_string(),
                    kind: "welcome".into(),
                    path: None,
                    error: Some(error),
                },
            };
            let app_handle = app.clone();
            let engine = app.state::<commands::AppState>().engine.clone();
            tauri::async_runtime::spawn(async move {
                let mut request = request;
                if argv_empty
                    && engine
                        .open_repository(std::path::Path::new(&cwd))
                        .await
                        .is_err()
                {
                    request.kind = "welcome".into();
                    request.path = None;
                }
                app_handle
                    .state::<commands::AppState>()
                    .launches
                    .push(request);
                let _ = app_handle.emit("launch-request", ());
            });
            let _ = app.emit("launch-request", ());
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .setup(|app| {
            app.state::<commands::AppState>()
                .operations
                .set_app(app.handle().clone());
            Ok(())
        });
    #[cfg(not(feature = "native-smoke"))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        commands::open_repository,
        commands::open_project,
        commands::refresh_project,
        commands::create_project,
        commands::save_project,
        commands::get_status,
        commands::get_changes,
        commands::list_refs,
        commands::list_remotes,
        commands::get_history,
        commands::get_commit,
        commands::get_diff,
        commands::plan_repository_action,
        commands::run_repository_action,
        commands::plan_batch,
        commands::run_batch,
        commands::cancel_batch,
        commands::get_operations,
        commands::get_launch_request,
        commands::ack_launch_request,
        commands::list_rewrites
    ]);
    #[cfg(feature = "native-smoke")]
    let builder = builder.invoke_handler(tauri::generate_handler![
        commands::open_repository,
        commands::open_project,
        commands::refresh_project,
        commands::create_project,
        commands::save_project,
        commands::get_status,
        commands::get_changes,
        commands::list_refs,
        commands::list_remotes,
        commands::get_history,
        commands::get_commit,
        commands::get_diff,
        commands::plan_repository_action,
        commands::run_repository_action,
        commands::plan_batch,
        commands::run_batch,
        commands::cancel_batch,
        commands::get_operations,
        commands::get_launch_request,
        commands::ack_launch_request,
        commands::list_rewrites,
        native_smoke::native_smoke_context,
        native_smoke::native_smoke_prepare_changes,
        native_smoke::native_smoke_fail_hook,
        native_smoke::native_smoke_forward_launch,
        native_smoke::native_smoke_m2_case,
        native_smoke::native_smoke_resolve_conflicts,
        native_smoke::native_smoke_m3_case,
        native_smoke::native_smoke_m3_edit,
        native_smoke::native_smoke_m3_verify,
        native_smoke::native_smoke_performance,
        native_smoke::native_smoke_appearance,
        native_smoke::native_smoke_slow_commit,
        native_smoke::native_smoke_progress,
        native_smoke::native_smoke_report
    ]);
    builder
        .run(tauri::generate_context!())
        .expect("Git View could not start");
}
