pub mod contracts;
pub mod git;
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|_, _, _| {}))
        .plugin(tauri_plugin_dialog::init())
        .run(tauri::generate_context!())
        .expect("Git View could not start");
}
