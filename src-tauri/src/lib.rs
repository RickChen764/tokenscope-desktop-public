mod background_sync;
mod commands;
mod db;
mod device_packages;
mod github_sync;
mod importers;
pub mod pricing;
mod proxy;
mod security;
mod tray_status;
pub mod usage;

use db::TokenScopeRepository;
use tauri::Manager;

pub struct AppState {
    pub repository: TokenScopeRepository,
    pub sync_runtime: background_sync::BackgroundSyncRuntime,
    pub github_sync_runtime: github_sync::engine::GitHubSyncRuntime,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .on_menu_event(|app, event| tray_status::handle_token_pulse_menu_event(app, event))
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data_dir)?;
            let db_path = app_data_dir.join("tokenscope.sqlite3");

            let repository = tauri::async_runtime::block_on(async {
                let repository = TokenScopeRepository::connect(&db_path).await?;
                repository.migrate().await?;
                Ok::<TokenScopeRepository, Box<dyn std::error::Error + Send + Sync>>(repository)
            })
            .map_err(|err| {
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("failed to initialize database: {err}"),
                )
            })?;

            let sync_runtime = background_sync::BackgroundSyncRuntime::default();
            let github_sync_runtime = github_sync::engine::GitHubSyncRuntime::default();
            app.manage(AppState {
                repository: repository.clone(),
                sync_runtime: sync_runtime.clone(),
                github_sync_runtime: github_sync_runtime.clone(),
            });
            tray_status::setup_token_pulse_tray(app, repository.clone())?;
            background_sync::spawn_background_sync_loop(
                repository.clone(),
                sync_runtime.clone(),
                github_sync_runtime.clone(),
            );
            background_sync::spawn_launch_sync_if_enabled(
                repository,
                sync_runtime,
                github_sync_runtime,
            );
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::dashboard::get_dashboard_summary,
            commands::dashboard::get_dashboard_summary_for_dates,
            commands::dashboard::get_daily_usage_series,
            commands::dashboard::get_token_pulse,
            tray_status::set_token_pulse_detail_hovered,
            tray_status::set_token_pulse_dragging,
            tray_status::show_token_pulse_context_menu,
            tray_status::open_token_pulse_home,
            tray_status::hide_token_pulse_window,
            tray_status::get_token_pulse_position_locked,
            tray_status::set_token_pulse_position_locked,
            commands::dashboard::get_dimension_summary,
            commands::dashboard::get_dimension_daily_series,
            commands::dashboard::get_top_agents,
            commands::dashboard::get_top_models,
            commands::dashboard::get_top_providers,
            commands::dashboard::get_top_workflows,
            commands::dashboard::get_top_projects,
            commands::dashboard::get_top_sessions,
            commands::dashboard::list_recent_calls,
            commands::dashboard::list_llm_calls,
            commands::dashboard::get_call_filter_options,
            commands::dashboard::get_data_health_summary,
            commands::dashboard::list_data_health_issues,
            commands::dashboard::list_custom_importer_profiles,
            commands::dashboard::upsert_custom_importer_profile,
            commands::dashboard::delete_custom_importer_profile,
            commands::dashboard::preview_custom_importer,
            commands::dashboard::run_custom_importer,
            commands::dashboard::list_agent_sources,
            commands::dashboard::seed_demo_data,
            commands::dashboard::clear_demo_data,
            commands::dashboard::import_codex_threads,
            commands::dashboard::get_codex_usage_limits,
            commands::dashboard::detect_local_agents,
            commands::dashboard::import_detected_agents,
            commands::dashboard::get_sync_settings,
            commands::dashboard::save_sync_settings,
            commands::dashboard::run_background_sync_once,
            commands::settings::export_calls_csv,
            commands::settings::export_device_dataset_package,
            commands::settings::import_device_dataset_package,
            commands::settings::list_external_datasets,
            commands::settings::open_export_folder,
            commands::settings::remove_external_dataset,
            commands::settings::get_github_sync_settings,
            commands::settings::get_github_sync_runtime_status,
            commands::settings::list_github_sync_remote_devices,
            commands::settings::save_github_sync_settings,
            commands::settings::test_github_sync_connection,
            commands::settings::run_github_sync_once,
            commands::settings::force_reimport_github_sync_remote_device,
            commands::settings::force_github_sync_bootstrap_upload
        ])
        .run(tauri::generate_context!())
        .expect("error while running TokenScope Desktop");
}
