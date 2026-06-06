use chrono::Local;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::State;

use crate::db::{
    DevicePackageImportResult, ExternalDataset, GitHubSyncConnectionTestResult,
    GitHubSyncRemoteDevice, GitHubSyncRunResult, GitHubSyncSettings, GitHubSyncSettingsInput,
    LlmCallFilters,
};
use crate::device_packages;
use crate::github_sync;
use crate::AppState;

#[tauri::command]
pub async fn export_calls_csv(
    state: State<'_, AppState>,
    filters: Option<LlmCallFilters>,
) -> Result<String, String> {
    let filters = filters.unwrap_or(LlmCallFilters {
        from: None,
        to: None,
        provider: None,
        agent_id: None,
        model: None,
        status: None,
        workflow_id: None,
        project_id: None,
        session_id: None,
        limit: 100,
        offset: 0,
    });
    let csv = state
        .repository
        .export_llm_calls_csv(&filters)
        .await
        .map_err(|err| err.to_string())?;

    let export_dir = default_export_dir();
    std::fs::create_dir_all(&export_dir).map_err(|err| err.to_string())?;

    let filename = format!(
        "tokenscope-calls-{}.csv",
        Local::now().format("%Y%m%d-%H%M%S")
    );
    let path = export_dir.join(filename);
    std::fs::write(&path, csv).map_err(|err| err.to_string())?;

    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn export_device_dataset_package(
    state: State<'_, AppState>,
    export_dir: Option<String>,
) -> Result<String, String> {
    let export_dir = export_dir
        .map(PathBuf::from)
        .unwrap_or_else(default_export_dir);

    device_packages::export_device_dataset_package(&state.repository, &export_dir).await
}

#[tauri::command]
pub async fn open_export_folder(path: Option<String>) -> Result<String, String> {
    let export_dir = path.map(PathBuf::from).unwrap_or_else(default_export_dir);
    std::fs::create_dir_all(&export_dir).map_err(|err| err.to_string())?;
    open_path(&export_dir)?;

    Ok(export_dir.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn import_device_dataset_package(
    state: State<'_, AppState>,
    path: String,
) -> Result<DevicePackageImportResult, String> {
    let path = std::path::PathBuf::from(path);
    device_packages::import_device_dataset_package(&state.repository, &path).await
}

#[tauri::command]
pub async fn list_external_datasets(
    state: State<'_, AppState>,
) -> Result<Vec<ExternalDataset>, String> {
    state
        .repository
        .list_external_datasets()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn remove_external_dataset(
    state: State<'_, AppState>,
    dataset_id: String,
) -> Result<i64, String> {
    if dataset_id.trim().is_empty() {
        return Err("dataset id is required".to_string());
    }

    state
        .repository
        .remove_external_dataset(&dataset_id)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn get_github_sync_settings(
    state: State<'_, AppState>,
) -> Result<GitHubSyncSettings, String> {
    state
        .repository
        .get_github_sync_settings()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn list_github_sync_remote_devices(
    state: State<'_, AppState>,
) -> Result<Vec<GitHubSyncRemoteDevice>, String> {
    state
        .repository
        .list_github_sync_remote_devices()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn save_github_sync_settings(
    state: State<'_, AppState>,
    input: GitHubSyncSettingsInput,
) -> Result<GitHubSyncSettings, String> {
    state
        .repository
        .save_github_sync_settings(&input)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn test_github_sync_connection(
    state: State<'_, AppState>,
) -> Result<GitHubSyncConnectionTestResult, String> {
    github_sync::engine::test_connection(&state.repository).await
}

#[tauri::command]
pub async fn run_github_sync_once(
    state: State<'_, AppState>,
) -> Result<GitHubSyncRunResult, String> {
    github_sync::engine::run_once(&state.repository, false).await
}

#[tauri::command]
pub async fn force_github_sync_bootstrap_upload(
    state: State<'_, AppState>,
) -> Result<GitHubSyncRunResult, String> {
    github_sync::engine::run_once(&state.repository, true).await
}

fn default_export_dir() -> PathBuf {
    std::env::temp_dir()
        .join("tokenscope-desktop")
        .join("exports")
}

fn open_path(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("explorer");
        command.arg(path);
        command
    };

    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(path);
        command
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(path);
        command
    };

    command.spawn().map_err(|err| err.to_string())?;
    Ok(())
}
