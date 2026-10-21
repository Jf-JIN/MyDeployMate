//! MyDeployMate Tauri shell: wires the store into commands and enforces a single instance.

mod commands;
mod error;
mod state;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be registered first: a second launch focuses the existing window and exits (DATA-04).
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .manage(state::AppState::init())
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::load_workspace,
            commands::validate_workspace_draft,
            commands::save_workspace,
        ])
        .run(tauri::generate_context!())
        .expect("error while running MyDeployMate");
}
