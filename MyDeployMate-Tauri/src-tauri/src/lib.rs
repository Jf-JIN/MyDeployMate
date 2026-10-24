//! MyDeployMate Tauri shell: wires the store and the run coordinator into commands and enforces
//! a single instance.

mod commands;
mod connection;
mod credentials;
mod error;
mod exchange;
mod history;
mod run;
mod state;
mod trust;
mod update;
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
        // App updates from GitHub Releases, verified against the key in tauri.conf.json (NFR-04).
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(state::AppState::init())
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                run::close_requested(window, api);
            }
        })
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
            commands::pipeline_templates,
            commands::wsl_distros,
            commands::shell_programs,
            commands::preview_run,
            exchange::export_workspace,
            exchange::import_preview,
            exchange::import_apply,
            history::list_runs,
            history::run_choices,
            history::latest_runs,
            history::delete_run,
            history::expired_run_count,
            history::purge_history,
            history::plan_changes,
            history::write_export,
            run::start_run,
            run::open_run,
            run::active_run,
            run::cancel_run,
            run::answer_confirmation,
            run::answer_run_secret,
            run::answer_run_trust,
            run::active_run_servers,
            run::cancel_and_quit,
            connection::connection_tests,
            connection::test_connection,
            connection::cancel_connection_test,
            connection::answer_host_trust,
            trust::trusted_hosts,
            trust::remove_trusted_host,
            trust::fetch_host_key,
            trust::replace_trusted_host,
            credentials::submit_secret,
            credentials::saved_credentials,
            credentials::forget_credential,
            credentials::set_variable_secret,
            credentials::variable_secret_present,
            update::check_update,
            update::install_update,
            update::cancel_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running MyDeployMate");
}
