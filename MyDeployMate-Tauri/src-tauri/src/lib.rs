//! MyDeployMate Tauri shell: wires the store and the run coordinator into commands and enforces
//! a single instance.

mod commands;
mod error;
mod run;
mod state;
mod values;

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
        // Native file picker for key files and paths (UI-14).
        .plugin(tauri_plugin_dialog::init())
        .manage(state::AppState::init())
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::load_workspace,
            commands::validate_workspace_draft,
            commands::save_workspace,
            commands::open_data_dir,
            commands::new_server,
            commands::new_environment,
            commands::new_variable,
            commands::environment_references,
            commands::server_references,
            commands::put_server,
            commands::remove_server,
            commands::inspect_key_file,
            commands::env_var_present,
            commands::new_project,
            commands::copy_project,
            commands::variable_views,
            commands::local_dir_exists,
            commands::new_pipeline,
            commands::copy_pipeline,
            commands::new_step,
            commands::copy_step,
            commands::builtin_variables,
            commands::wsl_distros,
            commands::shell_programs,
            commands::preview_run,
            run::start_run,
            run::active_run,
            run::cancel_run,
            run::answer_confirmation,
        ])
        .run(tauri::generate_context!())
        .expect("error while running MyDeployMate");
}
