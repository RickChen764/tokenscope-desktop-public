use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use chrono::{Local, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::db::{
    GitHubSyncConnectionTestResult, GitHubSyncRunResult, GitHubSyncSettings,
    GitHubSyncShardStateInput, TokenScopeRepository, GITHUB_SYNC_DATA_MODE_AGGREGATE_V3,
    GITHUB_SYNC_DATA_MODE_DETAIL_V2,
};
use crate::github_sync::crypto::{
    content_hash_hex, decrypt_sync_payload_bytes, encrypt_sync_payload_bytes,
};
use crate::github_sync::github::{GitHubContentFile, GitHubContentsClient, GitHubSyncLayout};
use crate::github_sync::packages::{
    decode_github_sync_package, export_github_sync_aggregate_package, export_github_sync_package,
    import_github_sync_package, serialize_github_sync_package, GitHubSyncAggregatePackage,
    GitHubSyncCompactPackage, GitHubSyncShardSelector,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncRunMode {
    Normal,
    ForceBootstrap,
    ForceRemoteReimport,
    Today,
}

impl SyncRunMode {
    fn as_status(self) -> &'static str {
        match self {
            SyncRunMode::Normal => "normal",
            SyncRunMode::ForceBootstrap => "force_bootstrap",
            SyncRunMode::ForceRemoteReimport => "force_remote_reimport",
            SyncRunMode::Today => "today",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GitHubSyncRuntimeStatus {
    pub running: bool,
    pub mode: Option<String>,
    pub phase: Option<String>,
    pub message: Option<String>,
    pub started_at: Option<String>,
    pub updated_at: Option<String>,
    pub last_status: Option<String>,
    pub current_step: i64,
    pub total_steps: i64,
    pub uploaded_shards: i64,
    pub downloaded_shards: i64,
    pub imported: i64,
    pub skipped: i64,
}

#[derive(Debug, Default)]
struct GitHubSyncRuntimeState {
    next_run_id: u64,
    active_run_id: Option<u64>,
    status: GitHubSyncRuntimeStatus,
}

#[derive(Clone, Debug, Default)]
pub struct GitHubSyncRuntime {
    state: Arc<Mutex<GitHubSyncRuntimeState>>,
}

#[derive(Debug, Clone, Copy)]
pub struct GitHubSyncRunGuard {
    run_id: u64,
}

impl GitHubSyncRuntime {
    pub fn try_start(&self, mode: SyncRunMode) -> Option<GitHubSyncRunGuard> {
        let mut state = self.state.lock().expect("github sync runtime lock");
        if state.status.running {
            return None;
        }

        state.next_run_id += 1;
        let run_id = state.next_run_id;
        let now = Local::now().to_rfc3339();
        state.active_run_id = Some(run_id);
        state.status = GitHubSyncRuntimeStatus {
            running: true,
            mode: Some(mode.as_status().to_string()),
            phase: Some("starting".to_string()),
            message: Some("正在启动 GitHub 同步...".to_string()),
            started_at: Some(now.clone()),
            updated_at: Some(now),
            last_status: None,
            current_step: 0,
            total_steps: 0,
            uploaded_shards: 0,
            downloaded_shards: 0,
            imported: 0,
            skipped: 0,
        };

        Some(GitHubSyncRunGuard { run_id })
    }

    pub fn status(&self) -> GitHubSyncRuntimeStatus {
        self.state
            .lock()
            .expect("github sync runtime lock")
            .status
            .clone()
    }

    pub fn update_phase(&self, phase: &str, message: impl Into<String>) {
        let mut state = self.state.lock().expect("github sync runtime lock");
        if !state.status.running {
            return;
        }
        state.status.phase = Some(phase.to_string());
        state.status.message = Some(message.into());
        state.status.updated_at = Some(Local::now().to_rfc3339());
    }

    fn update_progress(
        &self,
        phase: &str,
        message: impl Into<String>,
        current_step: i64,
        total_steps: i64,
    ) {
        let mut state = self.state.lock().expect("github sync runtime lock");
        if !state.status.running {
            return;
        }
        state.status.phase = Some(phase.to_string());
        state.status.message = Some(message.into());
        state.status.current_step = current_step.max(0);
        state.status.total_steps = total_steps.max(0);
        state.status.updated_at = Some(Local::now().to_rfc3339());
    }

    fn update_counts(&self, uploaded_shards: i64, summary: &GitHubSyncDownloadSummary) {
        let mut state = self.state.lock().expect("github sync runtime lock");
        if !state.status.running {
            return;
        }
        state.status.uploaded_shards = uploaded_shards;
        state.status.downloaded_shards = summary.downloaded_shards;
        state.status.imported = summary.imported;
        state.status.skipped = summary.skipped;
        state.status.updated_at = Some(Local::now().to_rfc3339());
    }

    pub fn finish(&self, guard: &GitHubSyncRunGuard, status: &str, message: impl Into<String>) {
        let mut state = self.state.lock().expect("github sync runtime lock");
        if state.active_run_id != Some(guard.run_id) {
            return;
        }
        state.active_run_id = None;
        state.status.running = false;
        state.status.phase = Some("finished".to_string());
        state.status.message = Some(message.into());
        state.status.last_status = Some(status.to_string());
        state.status.updated_at = Some(Local::now().to_rfc3339());
    }
}

#[allow(async_fn_in_trait)]
pub trait GitHubSyncTransport {
    async fn list_device_dirs(&self, layout: &GitHubSyncLayout) -> Result<Vec<String>, String>;

    async fn list_day_files(
        &self,
        layout: &GitHubSyncLayout,
        device_id: &str,
    ) -> Result<Vec<GitHubContentFile>, String>;

    async fn get_file(&self, path: &str) -> Result<Option<GitHubContentFile>, String>;

    async fn get_file_metadata(&self, path: &str) -> Result<Option<GitHubContentFile>, String>;

    async fn put_file(
        &self,
        path: &str,
        content: Vec<u8>,
        sha: Option<String>,
        message: &str,
    ) -> Result<GitHubContentFile, String>;
}

impl GitHubSyncTransport for GitHubContentsClient {
    async fn list_device_dirs(&self, layout: &GitHubSyncLayout) -> Result<Vec<String>, String> {
        GitHubContentsClient::list_device_dirs(self, layout).await
    }

    async fn list_day_files(
        &self,
        layout: &GitHubSyncLayout,
        device_id: &str,
    ) -> Result<Vec<GitHubContentFile>, String> {
        GitHubContentsClient::list_day_files(self, layout, device_id).await
    }

    async fn get_file(&self, path: &str) -> Result<Option<GitHubContentFile>, String> {
        GitHubContentsClient::get_file(self, path).await
    }

    async fn get_file_metadata(&self, path: &str) -> Result<Option<GitHubContentFile>, String> {
        GitHubContentsClient::get_file_metadata(self, path).await
    }

    async fn put_file(
        &self,
        path: &str,
        content: Vec<u8>,
        sha: Option<String>,
        message: &str,
    ) -> Result<GitHubContentFile, String> {
        GitHubContentsClient::put_file(self, path, content, sha, message).await
    }
}

pub async fn test_connection(
    repository: &TokenScopeRepository,
) -> Result<GitHubSyncConnectionTestResult, String> {
    let settings = repository
        .get_github_sync_settings()
        .await
        .map_err(|err| err.to_string())?;
    if settings.owner.trim().is_empty() || settings.repo.trim().is_empty() {
        return Ok(GitHubSyncConnectionTestResult {
            status: "error".to_string(),
            message: "请先填写 GitHub owner 和 repo。".to_string(),
        });
    }
    if !settings.token_configured {
        return Ok(GitHubSyncConnectionTestResult {
            status: "error".to_string(),
            message: "请先填写 personal access token。".to_string(),
        });
    }

    Ok(GitHubSyncConnectionTestResult {
        status: "ready".to_string(),
        message: "GitHub 同步配置已保存，网络连接会在同步时验证。".to_string(),
    })
}

pub async fn run_once_with_runtime(
    repository: &TokenScopeRepository,
    runtime: &GitHubSyncRuntime,
    force_bootstrap: bool,
) -> Result<GitHubSyncRunResult, String> {
    let mode = if force_bootstrap {
        SyncRunMode::ForceBootstrap
    } else {
        SyncRunMode::Normal
    };
    let Some(guard) = runtime.try_start(mode) else {
        return Ok(busy_result(runtime.status()));
    };

    let result = run_once_inner(repository, force_bootstrap, Some(runtime)).await;
    record_error_result(repository, &result).await;
    match &result {
        Ok(result) => runtime.finish(&guard, &result.status, &result.message),
        Err(err) => runtime.finish(&guard, "error", err),
    }
    result
}

pub async fn force_reimport_remote_device_with_runtime(
    repository: &TokenScopeRepository,
    runtime: &GitHubSyncRuntime,
    remote_device_id: &str,
) -> Result<GitHubSyncRunResult, String> {
    let Some(guard) = runtime.try_start(SyncRunMode::ForceRemoteReimport) else {
        return Ok(busy_result(runtime.status()));
    };

    let result =
        force_reimport_remote_device_inner(repository, remote_device_id, Some(runtime)).await;
    record_error_result(repository, &result).await;
    match &result {
        Ok(result) => runtime.finish(&guard, &result.status, &result.message),
        Err(err) => runtime.finish(&guard, "error", err),
    }
    result
}

pub async fn sync_today_with_runtime(
    repository: &TokenScopeRepository,
    runtime: &GitHubSyncRuntime,
    date_local: &str,
) -> Result<GitHubSyncRunResult, String> {
    let Some(guard) = runtime.try_start(SyncRunMode::Today) else {
        return Ok(busy_result(runtime.status()));
    };

    let result = sync_today_inner(repository, date_local, Some(runtime)).await;
    record_error_result(repository, &result).await;
    match &result {
        Ok(result) => runtime.finish(&guard, &result.status, &result.message),
        Err(err) => runtime.finish(&guard, "error", err),
    }
    result
}

async fn run_once_inner(
    repository: &TokenScopeRepository,
    force_bootstrap: bool,
    runtime: Option<&GitHubSyncRuntime>,
) -> Result<GitHubSyncRunResult, String> {
    if let Some(runtime) = runtime {
        runtime.update_phase("prepare", "读取 GitHub 同步配置");
    }
    let settings = repository
        .get_github_sync_settings()
        .await
        .map_err(|err| err.to_string())?;
    if !settings.enabled {
        return Ok(disabled_result());
    }
    let token = repository
        .github_sync_secret("token")
        .await
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "GitHub personal access token 未配置。".to_string())?;
    let transport = GitHubContentsClient::new(
        settings.owner.clone(),
        settings.repo.clone(),
        settings.branch.clone(),
        token,
    );
    run_with_settings(
        repository,
        &transport,
        if force_bootstrap {
            SyncRunMode::ForceBootstrap
        } else {
            SyncRunMode::Normal
        },
        settings,
        runtime,
    )
    .await
}

async fn sync_today_inner(
    repository: &TokenScopeRepository,
    date_local: &str,
    runtime: Option<&GitHubSyncRuntime>,
) -> Result<GitHubSyncRunResult, String> {
    if let Some(runtime) = runtime {
        runtime.update_phase("prepare", "读取 GitHub 同步配置");
    }
    let settings = repository
        .get_github_sync_settings()
        .await
        .map_err(|err| err.to_string())?;
    if !settings.enabled {
        return Ok(disabled_result());
    }
    let token = repository
        .github_sync_secret("token")
        .await
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "GitHub personal access token 未配置。".to_string())?;
    let transport = GitHubContentsClient::new(
        settings.owner.clone(),
        settings.repo.clone(),
        settings.branch.clone(),
        token,
    );

    sync_today_with_settings(repository, &transport, settings, date_local, runtime).await
}

async fn force_reimport_remote_device_inner(
    repository: &TokenScopeRepository,
    remote_device_id: &str,
    runtime: Option<&GitHubSyncRuntime>,
) -> Result<GitHubSyncRunResult, String> {
    if let Some(runtime) = runtime {
        runtime.update_phase("prepare", "读取 GitHub 同步配置");
    }
    let settings = repository
        .get_github_sync_settings()
        .await
        .map_err(|err| err.to_string())?;
    if !settings.enabled {
        return Ok(disabled_result());
    }
    let token = repository
        .github_sync_secret("token")
        .await
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "GitHub personal access token 未配置。".to_string())?;
    let transport = GitHubContentsClient::new(
        settings.owner.clone(),
        settings.repo.clone(),
        settings.branch.clone(),
        token,
    );

    force_reimport_remote_device_with_settings(
        repository,
        &transport,
        settings,
        remote_device_id,
        runtime,
    )
    .await
}

#[cfg(test)]
pub async fn run_github_sync_once_with_transport<T: GitHubSyncTransport>(
    repository: &TokenScopeRepository,
    transport: &T,
    mode: SyncRunMode,
) -> Result<GitHubSyncRunResult, String> {
    let settings = repository
        .get_github_sync_settings()
        .await
        .map_err(|err| err.to_string())?;
    let result = run_with_settings(repository, transport, mode, settings, None).await;
    record_error_result(repository, &result).await;
    result
}

#[cfg(test)]
pub async fn sync_today_with_transport<T: GitHubSyncTransport>(
    repository: &TokenScopeRepository,
    transport: &T,
    date_local: &str,
) -> Result<GitHubSyncRunResult, String> {
    let settings = repository
        .get_github_sync_settings()
        .await
        .map_err(|err| err.to_string())?;
    let result = sync_today_with_settings(repository, transport, settings, date_local, None).await;
    record_error_result(repository, &result).await;
    result
}

#[cfg(test)]
pub async fn force_reimport_remote_device_with_transport<T: GitHubSyncTransport>(
    repository: &TokenScopeRepository,
    transport: &T,
    remote_device_id: &str,
) -> Result<GitHubSyncRunResult, String> {
    let settings = repository
        .get_github_sync_settings()
        .await
        .map_err(|err| err.to_string())?;
    let result = force_reimport_remote_device_with_settings(
        repository,
        transport,
        settings,
        remote_device_id,
        None,
    )
    .await;
    record_error_result(repository, &result).await;
    result
}

async fn force_reimport_remote_device_with_settings<T: GitHubSyncTransport>(
    repository: &TokenScopeRepository,
    transport: &T,
    settings: GitHubSyncSettings,
    remote_device_id: &str,
    runtime: Option<&GitHubSyncRuntime>,
) -> Result<GitHubSyncRunResult, String> {
    let started_at = Local::now().to_rfc3339();
    if !settings.enabled {
        return Ok(disabled_result_with_started_at(started_at));
    }
    let remote_device_id = remote_device_id.trim();
    if remote_device_id.is_empty() {
        return Err("远端设备 ID 不能为空。".to_string());
    }
    let sync_password = repository
        .github_sync_secret("sync_password")
        .await
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "GitHub 同步密码未配置。".to_string())?;
    let local_device_id = repository
        .get_or_create_local_device_id()
        .await
        .map_err(|err| err.to_string())?;
    if remote_device_id == local_device_id {
        return Err("不能将当前设备作为远端设备重新导入。".to_string());
    }

    let layout = GitHubSyncLayout::new(settings.path_prefix.clone());
    let mut summary = GitHubSyncDownloadSummary::default();
    if let Some(runtime) = runtime {
        runtime.update_phase("download", format!("重新导入远端设备 {remote_device_id}"));
    }
    download_remote_device_updates(
        repository,
        transport,
        DownloadRemoteDeviceRequest {
            layout: &layout,
            remote_device_id,
            sync_password: &sync_password,
            uploaded_shards: 0,
            runtime,
            force_reimport: true,
        },
        &mut summary,
    )
    .await?;

    if summary.downloaded_shards == 0 {
        return Err(format!("未找到远端设备 {remote_device_id} 的可导入分片。"));
    }

    let finished_at = Local::now().to_rfc3339();
    let message = format!(
        "GitHub 远端设备重新导入完成：设备 {remote_device_id}，下载 {} 个分片，导入 {} 条记录。",
        summary.downloaded_shards, summary.imported
    );
    repository
        .record_github_sync_run("success", &message, None, Some(&finished_at))
        .await
        .map_err(|err| err.to_string())?;

    Ok(GitHubSyncRunResult {
        status: "success".to_string(),
        message,
        uploaded_shards: 0,
        downloaded_shards: summary.downloaded_shards,
        imported: summary.imported,
        skipped: summary.skipped,
        started_at,
        finished_at,
    })
}

async fn run_with_settings<T: GitHubSyncTransport>(
    repository: &TokenScopeRepository,
    transport: &T,
    mode: SyncRunMode,
    settings: GitHubSyncSettings,
    runtime: Option<&GitHubSyncRuntime>,
) -> Result<GitHubSyncRunResult, String> {
    let started_at = Local::now().to_rfc3339();
    if !settings.enabled {
        return Ok(disabled_result_with_started_at(started_at));
    }
    let sync_password = repository
        .github_sync_secret("sync_password")
        .await
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "GitHub 同步密码未配置。".to_string())?;
    let device_id = repository
        .get_or_create_local_device_id()
        .await
        .map_err(|err| err.to_string())?;
    let layout = GitHubSyncLayout::new(settings.path_prefix.clone());
    if let Some(runtime) = runtime {
        runtime.update_phase("export", "导出本机 GitHub 同步分片");
    }
    let bootstrap_package = export_local_github_sync_package(
        repository,
        GitHubSyncShardSelector::Bootstrap,
        &settings.data_mode,
    )
    .await?;
    let mut local_dates = bootstrap_package.date_locals();
    for uploaded_date in repository
        .github_sync_uploaded_day_dates(&device_id)
        .await
        .map_err(|err| err.to_string())?
    {
        local_dates.insert(uploaded_date);
    }

    let mut uploaded_shards = 0;
    let should_upload_bootstrap =
        mode == SyncRunMode::ForceBootstrap || !settings.bootstrap_uploaded;
    if should_upload_bootstrap {
        let path = layout.bootstrap_path(&device_id);
        let hash = package_content_hash(&bootstrap_package)?;
        if let Some(runtime) = runtime {
            runtime.update_progress("upload", "上传本机 bootstrap 分片", 1, 1);
        }
        let uploaded_file = upload_package(
            transport,
            &path,
            &bootstrap_package,
            &sync_password,
            "sync TokenScope bootstrap",
        )
        .await?;
        repository
            .record_github_sync_shard(&GitHubSyncShardStateInput {
                device_id: device_id.clone(),
                shard_kind: "bootstrap".to_string(),
                shard_date: None,
                content_hash: hash,
                github_blob_sha: Some(uploaded_file.sha),
                github_path: path,
                imported_at: None,
            })
            .await
            .map_err(|err| err.to_string())?;
        repository
            .set_github_sync_bootstrap_uploaded(true)
            .await
            .map_err(|err| err.to_string())?;
        uploaded_shards += 1;
    } else {
        let total_dates = local_dates.len() as i64;
        for (index, date) in local_dates.into_iter().enumerate() {
            if let Some(runtime) = runtime {
                runtime.update_progress(
                    "upload",
                    format!("检查本机 day 分片 {}/{}", index + 1, total_dates),
                    (index + 1) as i64,
                    total_dates,
                );
            }
            let package = export_local_github_sync_package(
                repository,
                GitHubSyncShardSelector::Day(date.clone()),
                &settings.data_mode,
            )
            .await?;
            let plaintext = package_bytes(&package)?;
            let hash = package_content_hash(&package)?;
            let existing = repository
                .github_sync_shard(&device_id, "day", Some(&date))
                .await
                .map_err(|err| err.to_string())?;
            if existing
                .as_ref()
                .map(|state| state.content_hash.as_str() == hash)
                .unwrap_or(false)
            {
                continue;
            }
            let path = layout.day_path(&device_id, &date);
            if let Some(runtime) = runtime {
                runtime.update_progress(
                    "upload",
                    format!("上传本机 day 分片 {date}"),
                    (index + 1) as i64,
                    total_dates,
                );
            }
            let uploaded_file = upload_plaintext(
                transport,
                &path,
                &plaintext,
                &sync_password,
                &format!("sync TokenScope day {date}"),
            )
            .await?;
            repository
                .record_github_sync_shard(&GitHubSyncShardStateInput {
                    device_id: device_id.clone(),
                    shard_kind: "day".to_string(),
                    shard_date: Some(date),
                    content_hash: hash,
                    github_blob_sha: Some(uploaded_file.sha),
                    github_path: path,
                    imported_at: None,
                })
                .await
                .map_err(|err| err.to_string())?;
            uploaded_shards += 1;
            if let Some(runtime) = runtime {
                runtime.update_counts(uploaded_shards, &GitHubSyncDownloadSummary::default());
            }
        }
    }

    if uploaded_shards > 0 {
        if let Some(runtime) = runtime {
            runtime.update_phase("manifest", "上传本机设备清单");
        }
        upload_manifest(
            transport,
            &layout,
            &device_id,
            &sync_password,
            &settings.data_mode,
        )
        .await?;
    }
    if let Some(runtime) = runtime {
        runtime.update_phase("download", "读取远端设备分片");
    }
    let download_summary = download_remote_updates(
        repository,
        transport,
        &layout,
        &device_id,
        &sync_password,
        uploaded_shards,
        runtime,
    )
    .await?;
    if let Some(runtime) = runtime {
        runtime.update_counts(uploaded_shards, &download_summary);
    }
    let finished_at = Local::now().to_rfc3339();
    let message = format!(
        "GitHub 同步完成：上传 {uploaded_shards} 个本机分片，下载 {} 个远端分片，导入 {} 条记录。",
        download_summary.downloaded_shards, download_summary.imported
    );
    repository
        .record_github_sync_run(
            "success",
            &message,
            if uploaded_shards > 0 {
                Some(&finished_at)
            } else {
                None
            },
            if download_summary.downloaded_shards > 0 {
                Some(&finished_at)
            } else {
                None
            },
        )
        .await
        .map_err(|err| err.to_string())?;

    Ok(GitHubSyncRunResult {
        status: "success".to_string(),
        message,
        uploaded_shards,
        downloaded_shards: download_summary.downloaded_shards,
        imported: download_summary.imported,
        skipped: download_summary.skipped,
        started_at,
        finished_at,
    })
}

async fn sync_today_with_settings<T: GitHubSyncTransport>(
    repository: &TokenScopeRepository,
    transport: &T,
    settings: GitHubSyncSettings,
    date_local: &str,
    runtime: Option<&GitHubSyncRuntime>,
) -> Result<GitHubSyncRunResult, String> {
    let started_at = Local::now().to_rfc3339();
    let date_local = normalize_day_date(date_local)?;
    if !settings.enabled {
        return Ok(disabled_result_with_started_at(started_at));
    }
    let sync_password = repository
        .github_sync_secret("sync_password")
        .await
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "GitHub 同步密码未配置。".to_string())?;
    let device_id = repository
        .get_or_create_local_device_id()
        .await
        .map_err(|err| err.to_string())?;
    let layout = GitHubSyncLayout::new(settings.path_prefix.clone());

    if let Some(runtime) = runtime {
        runtime.update_progress(
            "upload",
            format!("检查本机今日 day 分片 {date_local}"),
            1,
            1,
        );
    }
    let package = export_local_github_sync_package(
        repository,
        GitHubSyncShardSelector::Day(date_local.clone()),
        &settings.data_mode,
    )
    .await?;
    let plaintext = package_bytes(&package)?;
    let hash = package_content_hash(&package)?;
    let existing = repository
        .github_sync_shard(&device_id, "day", Some(&date_local))
        .await
        .map_err(|err| err.to_string())?;
    let has_local_data = package.date_locals().contains(&date_local);
    let mut uploaded_shards = 0;
    if has_local_data || existing.is_some() {
        if existing
            .as_ref()
            .map(|state| state.content_hash.as_str() == hash)
            .unwrap_or(false)
        {
            // The local day shard is unchanged; keep the recorded remote blob.
        } else {
            let path = layout.day_path(&device_id, &date_local);
            if let Some(runtime) = runtime {
                runtime.update_progress(
                    "upload",
                    format!("上传本机今日 day 分片 {date_local}"),
                    1,
                    1,
                );
            }
            let uploaded_file = upload_plaintext(
                transport,
                &path,
                &plaintext,
                &sync_password,
                &format!("sync TokenScope day {date_local}"),
            )
            .await?;
            repository
                .record_github_sync_shard(&GitHubSyncShardStateInput {
                    device_id: device_id.clone(),
                    shard_kind: "day".to_string(),
                    shard_date: Some(date_local.clone()),
                    content_hash: hash,
                    github_blob_sha: Some(uploaded_file.sha),
                    github_path: path,
                    imported_at: None,
                })
                .await
                .map_err(|err| err.to_string())?;
            uploaded_shards += 1;
        }
    }

    if let Some(runtime) = runtime {
        runtime.update_phase("download", format!("读取远端今日 day 分片 {date_local}"));
    }
    let download_summary = download_remote_today_updates(
        repository,
        transport,
        DownloadRemoteTodayRequest {
            layout: &layout,
            local_device_id: &device_id,
            date_local: &date_local,
            sync_password: &sync_password,
            uploaded_shards,
            runtime,
        },
    )
    .await?;
    if let Some(runtime) = runtime {
        runtime.update_counts(uploaded_shards, &download_summary);
    }

    let finished_at = Local::now().to_rfc3339();
    let message = format!(
        "GitHub 今日同步完成：上传 {uploaded_shards} 个本机分片，下载 {} 个远端分片，导入 {} 条记录。",
        download_summary.downloaded_shards, download_summary.imported
    );
    repository
        .record_github_sync_run(
            "success",
            &message,
            if uploaded_shards > 0 {
                Some(&finished_at)
            } else {
                None
            },
            if download_summary.downloaded_shards > 0 {
                Some(&finished_at)
            } else {
                None
            },
        )
        .await
        .map_err(|err| err.to_string())?;

    Ok(GitHubSyncRunResult {
        status: "success".to_string(),
        message,
        uploaded_shards,
        downloaded_shards: download_summary.downloaded_shards,
        imported: download_summary.imported,
        skipped: download_summary.skipped,
        started_at,
        finished_at,
    })
}

async fn download_remote_updates<T: GitHubSyncTransport>(
    repository: &TokenScopeRepository,
    transport: &T,
    layout: &GitHubSyncLayout,
    local_device_id: &str,
    sync_password: &str,
    uploaded_shards: i64,
    runtime: Option<&GitHubSyncRuntime>,
) -> Result<GitHubSyncDownloadSummary, String> {
    let mut summary = GitHubSyncDownloadSummary::default();
    let mut device_ids = transport.list_device_dirs(layout).await?;
    device_ids.sort();
    device_ids.dedup();

    let total_devices = device_ids.len() as i64;
    for (device_index, remote_device_id) in device_ids.into_iter().enumerate() {
        if remote_device_id == local_device_id {
            continue;
        }
        if let Some(runtime) = runtime {
            runtime.update_progress(
                "download",
                format!(
                    "读取远端设备 {} ({}/{})",
                    remote_device_id,
                    device_index + 1,
                    total_devices
                ),
                (device_index + 1) as i64,
                total_devices,
            );
        }

        let remote_data_mode =
            remote_manifest_data_mode(transport, layout, &remote_device_id, sync_password).await?;
        if let Some(remote_data_mode) = remote_data_mode.as_deref() {
            repository
                .update_external_dataset_sync_data_mode_for_device(
                    &remote_device_id,
                    remote_data_mode,
                )
                .await
                .map_err(|err| err.to_string())?;
        }
        let bootstrap_path = layout.bootstrap_path(&remote_device_id);
        import_remote_shard(
            repository,
            transport,
            RemoteShardImportRequest {
                path: &bootstrap_path,
                shard_hint: None,
                sync_password,
                force_reimport: false,
                expected_data_mode: remote_data_mode.as_deref(),
            },
            &mut summary,
        )
        .await?;
        if let Some(runtime) = runtime {
            runtime.update_counts(uploaded_shards, &summary);
        }

        let mut day_files = transport.list_day_files(layout, &remote_device_id).await?;
        day_files.sort_by(|left, right| left.path.cmp(&right.path));
        let total_day_files = day_files.len() as i64;
        for (file_index, day_file) in day_files.into_iter().enumerate() {
            let Some(day_file_date) = parse_day_file_date(&day_file.name) else {
                summary.skipped += 1;
                continue;
            };
            if let Some(runtime) = runtime {
                runtime.update_progress(
                    "download",
                    format!(
                        "导入远端 day 分片 {} ({}/{})",
                        day_file.name,
                        file_index + 1,
                        total_day_files
                    ),
                    (file_index + 1) as i64,
                    total_day_files,
                );
            }
            import_remote_shard(
                repository,
                transport,
                RemoteShardImportRequest {
                    path: &day_file.path,
                    shard_hint: Some(RemoteShardHint {
                        device_id: &remote_device_id,
                        shard_kind: "day",
                        shard_date: Some(day_file_date.as_str()),
                        github_blob_sha: day_file.sha.as_str(),
                    }),
                    sync_password,
                    force_reimport: false,
                    expected_data_mode: remote_data_mode.as_deref(),
                },
                &mut summary,
            )
            .await?;
            if let Some(runtime) = runtime {
                runtime.update_counts(uploaded_shards, &summary);
            }
        }
    }

    Ok(summary)
}

struct DownloadRemoteDeviceRequest<'a> {
    layout: &'a GitHubSyncLayout,
    remote_device_id: &'a str,
    sync_password: &'a str,
    uploaded_shards: i64,
    runtime: Option<&'a GitHubSyncRuntime>,
    force_reimport: bool,
}

struct DownloadRemoteTodayRequest<'a> {
    layout: &'a GitHubSyncLayout,
    local_device_id: &'a str,
    date_local: &'a str,
    sync_password: &'a str,
    uploaded_shards: i64,
    runtime: Option<&'a GitHubSyncRuntime>,
}

struct RemoteShardImportRequest<'a> {
    path: &'a str,
    shard_hint: Option<RemoteShardHint<'a>>,
    sync_password: &'a str,
    force_reimport: bool,
    expected_data_mode: Option<&'a str>,
}

async fn download_remote_device_updates<T: GitHubSyncTransport>(
    repository: &TokenScopeRepository,
    transport: &T,
    request: DownloadRemoteDeviceRequest<'_>,
    summary: &mut GitHubSyncDownloadSummary,
) -> Result<(), String> {
    let bootstrap_path = request.layout.bootstrap_path(request.remote_device_id);
    let remote_data_mode = remote_manifest_data_mode(
        transport,
        request.layout,
        request.remote_device_id,
        request.sync_password,
    )
    .await?;
    if let Some(remote_data_mode) = remote_data_mode.as_deref() {
        repository
            .update_external_dataset_sync_data_mode_for_device(
                request.remote_device_id,
                remote_data_mode,
            )
            .await
            .map_err(|err| err.to_string())?;
    }
    import_remote_shard(
        repository,
        transport,
        RemoteShardImportRequest {
            path: &bootstrap_path,
            shard_hint: None,
            sync_password: request.sync_password,
            force_reimport: request.force_reimport,
            expected_data_mode: remote_data_mode.as_deref(),
        },
        summary,
    )
    .await?;
    if let Some(runtime) = request.runtime {
        runtime.update_counts(request.uploaded_shards, summary);
    }

    let mut day_files = transport
        .list_day_files(request.layout, request.remote_device_id)
        .await?;
    day_files.sort_by(|left, right| left.path.cmp(&right.path));
    let total_day_files = day_files.len() as i64;
    for (file_index, day_file) in day_files.into_iter().enumerate() {
        let Some(day_file_date) = parse_day_file_date(&day_file.name) else {
            summary.skipped += 1;
            continue;
        };
        if let Some(runtime) = request.runtime {
            runtime.update_progress(
                "download",
                format!(
                    "导入远端 day 分片 {} ({}/{})",
                    day_file.name,
                    file_index + 1,
                    total_day_files
                ),
                (file_index + 1) as i64,
                total_day_files,
            );
        }
        import_remote_shard(
            repository,
            transport,
            RemoteShardImportRequest {
                path: &day_file.path,
                shard_hint: Some(RemoteShardHint {
                    device_id: request.remote_device_id,
                    shard_kind: "day",
                    shard_date: Some(day_file_date.as_str()),
                    github_blob_sha: day_file.sha.as_str(),
                }),
                sync_password: request.sync_password,
                force_reimport: request.force_reimport,
                expected_data_mode: remote_data_mode.as_deref(),
            },
            summary,
        )
        .await?;
        if let Some(runtime) = request.runtime {
            runtime.update_counts(request.uploaded_shards, summary);
        }
    }

    Ok(())
}

async fn download_remote_today_updates<T: GitHubSyncTransport>(
    repository: &TokenScopeRepository,
    transport: &T,
    request: DownloadRemoteTodayRequest<'_>,
) -> Result<GitHubSyncDownloadSummary, String> {
    let mut summary = GitHubSyncDownloadSummary::default();
    let mut device_ids = transport.list_device_dirs(request.layout).await?;
    device_ids.sort();
    device_ids.dedup();

    let total_devices = device_ids.len() as i64;
    for (device_index, remote_device_id) in device_ids.into_iter().enumerate() {
        if remote_device_id == request.local_device_id {
            continue;
        }
        if let Some(runtime) = request.runtime {
            runtime.update_progress(
                "download",
                format!(
                    "导入远端今日 day 分片 {} ({}/{})",
                    remote_device_id,
                    device_index + 1,
                    total_devices
                ),
                (device_index + 1) as i64,
                total_devices,
            );
        }

        let remote_data_mode = remote_manifest_data_mode(
            transport,
            request.layout,
            &remote_device_id,
            request.sync_password,
        )
        .await?;
        if let Some(remote_data_mode) = remote_data_mode.as_deref() {
            repository
                .update_external_dataset_sync_data_mode_for_device(
                    &remote_device_id,
                    remote_data_mode,
                )
                .await
                .map_err(|err| err.to_string())?;
        }
        let day_path = request
            .layout
            .day_path(&remote_device_id, request.date_local);
        let Some(day_metadata) = transport.get_file_metadata(&day_path).await? else {
            continue;
        };
        import_remote_shard(
            repository,
            transport,
            RemoteShardImportRequest {
                path: &day_path,
                shard_hint: Some(RemoteShardHint {
                    device_id: &remote_device_id,
                    shard_kind: "day",
                    shard_date: Some(request.date_local),
                    github_blob_sha: day_metadata.sha.as_str(),
                }),
                sync_password: request.sync_password,
                force_reimport: false,
                expected_data_mode: remote_data_mode.as_deref(),
            },
            &mut summary,
        )
        .await?;
        if let Some(runtime) = request.runtime {
            runtime.update_counts(request.uploaded_shards, &summary);
        }
    }

    Ok(summary)
}

async fn import_remote_shard<T: GitHubSyncTransport>(
    repository: &TokenScopeRepository,
    transport: &T,
    request: RemoteShardImportRequest<'_>,
    summary: &mut GitHubSyncDownloadSummary,
) -> Result<(), String> {
    if !request.force_reimport {
        if let Some(hint) = request.shard_hint.as_ref() {
            if repository
                .github_sync_shard(hint.device_id, hint.shard_kind, hint.shard_date)
                .await
                .map_err(|err| err.to_string())?
                .as_ref()
                .and_then(|state| state.github_blob_sha.as_deref())
                == Some(hint.github_blob_sha)
            {
                summary.skipped += 1;
                return Ok(());
            }
        }
    }

    let Some(file) = transport.get_file(request.path).await? else {
        return Ok(());
    };
    let plaintext = decrypt_remote_file_bytes(
        request.path,
        &file.content,
        request.sync_password,
        "GitHub 同步远端分片读取失败",
    )?;
    let content_hash = content_hash_hex(&plaintext);
    let package = decode_github_sync_package(&plaintext)
        .map_err(|err| format!("GitHub 同步远端分片解析失败：{}：{err}", request.path))?;
    if request
        .expected_data_mode
        .map(|mode| package.data_mode() != mode)
        .unwrap_or(false)
    {
        summary.skipped += 1;
        return Ok(());
    }
    let remote_device_id = package.device_id().to_string();
    let shard_kind = package.shard_kind().to_string();
    let shard_date = package.shard_date().map(ToString::to_string);

    if !request.force_reimport
        && repository
            .github_sync_shard(&remote_device_id, &shard_kind, shard_date.as_deref())
            .await
            .map_err(|err| err.to_string())?
            .as_ref()
            .map(|state| state.content_hash.as_str() == content_hash)
            .unwrap_or(false)
    {
        summary.skipped += 1;
        return Ok(());
    }

    let import_result = import_github_sync_package(repository, package).await?;
    let imported_at = Local::now().to_rfc3339();
    repository
        .record_github_sync_shard(&GitHubSyncShardStateInput {
            device_id: remote_device_id,
            shard_kind,
            shard_date,
            content_hash,
            github_blob_sha: Some(file.sha),
            github_path: request.path.to_string(),
            imported_at: Some(imported_at),
        })
        .await
        .map_err(|err| err.to_string())?;
    summary.downloaded_shards += 1;
    summary.imported += import_result.imported;
    summary.skipped += import_result.skipped;

    Ok(())
}

fn parse_day_file_date(name: &str) -> Option<String> {
    let date = name.strip_suffix(".tokenscope.zst.enc")?;
    let bytes = date.as_bytes();
    let is_iso_date_name = bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[..4].iter().all(u8::is_ascii_digit)
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[8..].iter().all(u8::is_ascii_digit);

    is_iso_date_name.then(|| date.to_string())
}

fn normalize_day_date(date_local: &str) -> Result<String, String> {
    NaiveDate::parse_from_str(date_local, "%Y-%m-%d")
        .map(|date| date.to_string())
        .map_err(|_| format!("GitHub 同步日期无效：{date_local}"))
}

async fn upload_package<T: GitHubSyncTransport>(
    transport: &T,
    path: &str,
    package: &GitHubSyncLocalPackage,
    sync_password: &str,
    message: &str,
) -> Result<GitHubContentFile, String> {
    let plaintext = package_bytes(package)?;
    upload_plaintext(transport, path, &plaintext, sync_password, message).await
}

async fn upload_manifest<T: GitHubSyncTransport>(
    transport: &T,
    layout: &GitHubSyncLayout,
    device_id: &str,
    sync_password: &str,
    data_mode: &str,
) -> Result<(), String> {
    let manifest = GitHubSyncManifest {
        version: 1,
        device_id: device_id.to_string(),
        updated_at: Local::now().to_rfc3339(),
        active_data_mode: normalize_github_sync_data_mode(data_mode).to_string(),
    };
    let plaintext = serde_json::to_vec(&manifest)
        .map_err(|err| format!("GitHub manifest 序列化失败：{err}"))?;
    upload_plaintext(
        transport,
        &layout.manifest_path(device_id),
        &plaintext,
        sync_password,
        "sync TokenScope manifest",
    )
    .await?;
    Ok(())
}

async fn upload_plaintext<T: GitHubSyncTransport>(
    transport: &T,
    path: &str,
    plaintext: &[u8],
    sync_password: &str,
    message: &str,
) -> Result<GitHubContentFile, String> {
    let encrypted = encrypt_sync_payload_bytes(plaintext, sync_password)?;
    let sha = transport.get_file(path).await?.map(|file| file.sha);
    transport.put_file(path, encrypted, sha, message).await
}

fn decrypt_remote_file_bytes(
    path: &str,
    content: &[u8],
    sync_password: &str,
    context: &str,
) -> Result<Vec<u8>, String> {
    decrypt_sync_payload_bytes(content, sync_password)
        .map_err(|err| format!("{context}：{path}：{err}"))
}

async fn remote_manifest_data_mode<T: GitHubSyncTransport>(
    transport: &T,
    layout: &GitHubSyncLayout,
    device_id: &str,
    sync_password: &str,
) -> Result<Option<String>, String> {
    let manifest_path = layout.manifest_path(device_id);
    let Some(file) = transport.get_file(&manifest_path).await? else {
        return Ok(Some(GITHUB_SYNC_DATA_MODE_DETAIL_V2.to_string()));
    };
    let plaintext = match decrypt_sync_payload_bytes(&file.content, sync_password) {
        Ok(plaintext) => plaintext,
        Err(err) if is_optional_manifest_error(&err) => return Ok(None),
        Err(err) => {
            return Err(format!(
                "GitHub 同步远端 manifest 读取失败：{manifest_path}：{err}"
            ))
        }
    };
    let manifest = match serde_json::from_slice::<GitHubSyncManifest>(&plaintext) {
        Ok(manifest) => manifest,
        Err(_) => return Ok(None),
    };
    Ok(Some(
        normalize_github_sync_data_mode(&manifest.active_data_mode).to_string(),
    ))
}

fn is_optional_manifest_error(err: &str) -> bool {
    !err.contains("解密失败")
        && !err.contains("同步密码不能为空")
        && !err.contains("派生同步密钥失败")
}

async fn export_local_github_sync_package(
    repository: &TokenScopeRepository,
    selector: GitHubSyncShardSelector,
    data_mode: &str,
) -> Result<GitHubSyncLocalPackage, String> {
    match normalize_github_sync_data_mode(data_mode) {
        GITHUB_SYNC_DATA_MODE_DETAIL_V2 => export_github_sync_package(repository, selector)
            .await
            .map(GitHubSyncLocalPackage::Compact),
        _ => export_github_sync_aggregate_package(repository, selector)
            .await
            .map(GitHubSyncLocalPackage::Aggregate),
    }
}

fn package_bytes<T: Serialize>(package: &T) -> Result<Vec<u8>, String> {
    serialize_github_sync_package(package)
}

fn package_content_hash(package: &GitHubSyncLocalPackage) -> Result<String, String> {
    let stable_package = package.stable_for_hash();
    package_bytes(&stable_package).map(|bytes| content_hash_hex(&bytes))
}

fn normalize_github_sync_data_mode(value: &str) -> &str {
    match value.trim() {
        GITHUB_SYNC_DATA_MODE_DETAIL_V2 => GITHUB_SYNC_DATA_MODE_DETAIL_V2,
        GITHUB_SYNC_DATA_MODE_AGGREGATE_V3 => GITHUB_SYNC_DATA_MODE_AGGREGATE_V3,
        _ => GITHUB_SYNC_DATA_MODE_AGGREGATE_V3,
    }
}

async fn record_error_result(
    repository: &TokenScopeRepository,
    result: &Result<GitHubSyncRunResult, String>,
) {
    if let Err(err) = result {
        let _ = repository
            .record_github_sync_run("error", err, None, None)
            .await;
    }
}

#[derive(Debug, Default)]
struct GitHubSyncDownloadSummary {
    downloaded_shards: i64,
    imported: i64,
    skipped: i64,
}

fn disabled_result() -> GitHubSyncRunResult {
    disabled_result_with_started_at(Local::now().to_rfc3339())
}

fn busy_result(status: GitHubSyncRuntimeStatus) -> GitHubSyncRunResult {
    let now = Local::now().to_rfc3339();
    GitHubSyncRunResult {
        status: "busy".to_string(),
        message: "已有 GitHub 同步任务正在执行。".to_string(),
        uploaded_shards: status.uploaded_shards,
        downloaded_shards: status.downloaded_shards,
        imported: status.imported,
        skipped: status.skipped,
        started_at: status.started_at.unwrap_or_else(|| now.clone()),
        finished_at: now,
    }
}

fn disabled_result_with_started_at(started_at: String) -> GitHubSyncRunResult {
    let finished_at = Local::now().to_rfc3339();
    GitHubSyncRunResult {
        status: "disabled".to_string(),
        message: "GitHub 同步未启用。".to_string(),
        uploaded_shards: 0,
        downloaded_shards: 0,
        imported: 0,
        skipped: 0,
        started_at,
        finished_at,
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct GitHubSyncManifest {
    version: i64,
    device_id: String,
    updated_at: String,
    #[serde(default = "default_manifest_data_mode")]
    active_data_mode: String,
}

fn default_manifest_data_mode() -> String {
    GITHUB_SYNC_DATA_MODE_DETAIL_V2.to_string()
}

#[derive(Clone, Serialize)]
#[serde(untagged)]
enum GitHubSyncLocalPackage {
    Compact(GitHubSyncCompactPackage),
    Aggregate(GitHubSyncAggregatePackage),
}

impl GitHubSyncLocalPackage {
    fn date_locals(&self) -> BTreeSet<String> {
        match self {
            Self::Compact(package) => package
                .rows
                .iter()
                .map(|call| call.date_local().to_string())
                .collect(),
            Self::Aggregate(package) => {
                package.daily_rows.iter().map(|row| row.0.clone()).collect()
            }
        }
    }

    fn stable_for_hash(&self) -> Self {
        let mut stable_package = self.clone();
        match &mut stable_package {
            Self::Compact(package) => package.exported_at.clear(),
            Self::Aggregate(package) => package.exported_at.clear(),
        }
        stable_package
    }
}

struct RemoteShardHint<'a> {
    device_id: &'a str,
    shard_kind: &'static str,
    shard_date: Option<&'a str>,
    github_blob_sha: &'a str,
}

#[cfg(test)]
#[derive(Default)]
pub struct FakeGitHubTransport {
    uploaded: std::sync::Mutex<Vec<FakeUploadedFile>>,
    get_file_paths: std::sync::Mutex<Vec<String>>,
}

#[cfg(test)]
#[derive(Clone)]
struct FakeUploadedFile {
    path: String,
    sha: String,
    content: Vec<u8>,
}

#[cfg(test)]
impl FakeGitHubTransport {
    pub fn uploaded_path(&self, needle: &str) -> String {
        self.uploaded
            .lock()
            .expect("fake transport lock")
            .iter()
            .filter(|file| file.path.contains(needle))
            .map(|file| file.path.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn seed_file(&self, path: &str, content: Vec<u8>) {
        let mut uploaded = self.uploaded.lock().expect("fake transport lock");
        let sha = format!("fake-seed-sha-{}", uploaded.len() + 1);
        uploaded.push(FakeUploadedFile {
            path: path.to_string(),
            sha,
            content,
        });
    }

    fn get_file_count_for(&self, path: &str) -> usize {
        self.get_file_paths
            .lock()
            .expect("fake transport get file lock")
            .iter()
            .filter(|current| current.as_str() == path)
            .count()
    }
}

#[cfg(test)]
impl GitHubSyncTransport for FakeGitHubTransport {
    async fn list_device_dirs(&self, layout: &GitHubSyncLayout) -> Result<Vec<String>, String> {
        let prefix = format!("{}/", layout.devices_path());
        let mut devices = self
            .uploaded
            .lock()
            .expect("fake transport lock")
            .iter()
            .filter_map(|file| file.path.strip_prefix(&prefix))
            .filter_map(|suffix| suffix.split('/').next())
            .filter(|device_id| !device_id.is_empty())
            .map(|device_id| device_id.to_string())
            .collect::<Vec<_>>();
        devices.sort();
        devices.dedup();
        Ok(devices)
    }

    async fn list_day_files(
        &self,
        layout: &GitHubSyncLayout,
        device_id: &str,
    ) -> Result<Vec<GitHubContentFile>, String> {
        let prefix = format!("{}/", layout.days_path(device_id));
        Ok(self
            .uploaded
            .lock()
            .expect("fake transport lock")
            .iter()
            .filter(|file| file.path.starts_with(&prefix))
            .filter(|file| !file.path[prefix.len()..].contains('/'))
            .map(|file| GitHubContentFile {
                name: file
                    .path
                    .rsplit('/')
                    .next()
                    .unwrap_or(&file.path)
                    .to_string(),
                path: file.path.clone(),
                sha: file.sha.clone(),
                content: Vec::new(),
            })
            .collect())
    }

    async fn get_file(&self, path: &str) -> Result<Option<GitHubContentFile>, String> {
        self.get_file_paths
            .lock()
            .expect("fake transport get file lock")
            .push(path.to_string());
        Ok(self
            .uploaded
            .lock()
            .expect("fake transport lock")
            .iter()
            .rev()
            .find(|file| file.path == path)
            .map(|file| GitHubContentFile {
                name: path.rsplit('/').next().unwrap_or(path).to_string(),
                path: path.to_string(),
                sha: file.sha.clone(),
                content: file.content.clone(),
            }))
    }

    async fn get_file_metadata(&self, path: &str) -> Result<Option<GitHubContentFile>, String> {
        Ok(self
            .uploaded
            .lock()
            .expect("fake transport lock")
            .iter()
            .rev()
            .find(|file| file.path == path)
            .map(|file| GitHubContentFile {
                name: path.rsplit('/').next().unwrap_or(path).to_string(),
                path: path.to_string(),
                sha: file.sha.clone(),
                content: Vec::new(),
            }))
    }

    async fn put_file(
        &self,
        path: &str,
        content: Vec<u8>,
        _sha: Option<String>,
        _message: &str,
    ) -> Result<GitHubContentFile, String> {
        let mut uploaded = self.uploaded.lock().expect("fake transport lock");
        let sha = format!("fake-sha-{}", uploaded.len() + 1);
        uploaded.push(FakeUploadedFile {
            path: path.to_string(),
            sha: sha.clone(),
            content,
        });
        Ok(GitHubContentFile {
            name: path.rsplit('/').next().unwrap_or(path).to_string(),
            path: path.to_string(),
            sha,
            content: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use sqlx::{query, Row};

    use super::*;
    use crate::db::{GitHubSyncSettingsInput, TokenScopeRepository};
    use crate::github_sync::crypto::encrypt_sync_payload;
    use crate::github_sync::packages::{GitHubSyncDevice, GitHubSyncPackage, GitHubSyncShard};

    #[tokio::test]
    async fn first_sync_uploads_bootstrap_then_manifest() {
        let repository = TokenScopeRepository::connect_in_memory().await.unwrap();
        repository.migrate().await.unwrap();
        insert_test_call(&repository, "local-a", "2026-06-05", 100).await;
        repository
            .save_github_sync_settings(&valid_settings())
            .await
            .unwrap();

        let transport = FakeGitHubTransport::default();
        let result =
            run_github_sync_once_with_transport(&repository, &transport, SyncRunMode::Normal)
                .await
                .expect("sync succeeds");

        assert_eq!(result.status, "success");
        assert!(transport
            .uploaded_path("tokenscope-sync/v1/devices/")
            .contains("bootstrap.tokenscope.zst.enc"));
        assert!(transport
            .uploaded_path("tokenscope-sync/v1/devices/")
            .contains("manifest.enc"));
    }

    #[tokio::test]
    async fn sync_uploads_binary_encrypted_envelopes() {
        let repository = TokenScopeRepository::connect_in_memory().await.unwrap();
        repository.migrate().await.unwrap();
        insert_test_call(&repository, "local-a", "2026-06-05", 100).await;
        repository
            .save_github_sync_settings(&valid_settings())
            .await
            .unwrap();

        let transport = FakeGitHubTransport::default();
        run_github_sync_once_with_transport(&repository, &transport, SyncRunMode::Normal)
            .await
            .expect("sync succeeds");
        let uploaded = transport.uploaded.lock().expect("fake transport lock");
        let bootstrap = uploaded
            .iter()
            .find(|file| file.path.contains("bootstrap.tokenscope.zst.enc"))
            .expect("bootstrap uploaded");

        assert!(bootstrap.content.starts_with(b"TSGS2"));
        assert!(!bootstrap.content.starts_with(b"{"));
    }

    #[tokio::test]
    async fn today_sync_uploads_and_downloads_only_selected_day() {
        let repository = TokenScopeRepository::connect_in_memory().await.unwrap();
        repository.migrate().await.unwrap();
        repository
            .save_github_sync_settings(&valid_settings())
            .await
            .unwrap();
        insert_test_call(&repository, "local-today", "2026-06-05", 100).await;
        insert_test_call(&repository, "local-yesterday", "2026-06-04", 200).await;

        let layout = GitHubSyncLayout::new("tokenscope-sync".to_string());
        let transport = FakeGitHubTransport::default();
        let remote_today_path = layout.day_path("remote-a", "2026-06-05");
        let remote_yesterday_path = layout.day_path("remote-a", "2026-06-04");
        transport.seed_file(
            &remote_today_path,
            encrypted_package_bytes(
                &remote_day_package("remote-a", "2026-06-05", 300),
                "sync-password",
            ),
        );
        transport.seed_file(
            &remote_yesterday_path,
            encrypted_package_bytes(
                &remote_day_package("remote-a", "2026-06-04", 400),
                "sync-password",
            ),
        );

        let result = sync_today_with_transport(&repository, &transport, "2026-06-05")
            .await
            .expect("today sync succeeds");

        assert_eq!(result.uploaded_shards, 1);
        assert!(transport
            .uploaded_path("days/2026-06-05.tokenscope.zst.enc")
            .contains("2026-06-05"));
        let local_device_id = repository
            .get_or_create_local_device_id()
            .await
            .expect("local device id exists");
        assert!(transport
            .uploaded_path(&layout.day_path(&local_device_id, "2026-06-04"))
            .is_empty());
        assert_eq!(
            imported_token_sum(&repository, "device-remote-a", "2026-06-05").await,
            300
        );
        assert_eq!(
            imported_token_sum(&repository, "device-remote-a", "2026-06-04").await,
            0
        );
        assert_eq!(transport.get_file_count_for(&remote_today_path), 1);
        assert_eq!(transport.get_file_count_for(&remote_yesterday_path), 0);
    }

    #[tokio::test]
    async fn second_sync_uploads_changed_day_shards_only() {
        let repository = TokenScopeRepository::connect_in_memory().await.unwrap();
        repository.migrate().await.unwrap();
        repository
            .save_github_sync_settings(&valid_settings())
            .await
            .unwrap();
        repository
            .set_github_sync_bootstrap_uploaded(true)
            .await
            .unwrap();
        insert_test_call(&repository, "local-a", "2026-06-05", 100).await;

        let transport = FakeGitHubTransport::default();
        let result =
            run_github_sync_once_with_transport(&repository, &transport, SyncRunMode::Normal)
                .await
                .expect("sync succeeds");

        assert_eq!(result.uploaded_shards, 1);
        assert!(transport
            .uploaded_path("days/2026-06-05.tokenscope.zst.enc")
            .contains("2026-06-05"));
    }

    #[tokio::test]
    async fn sync_skips_all_uploads_when_local_shards_are_unchanged() {
        let repository = TokenScopeRepository::connect_in_memory().await.unwrap();
        repository.migrate().await.unwrap();
        repository
            .save_github_sync_settings(&valid_settings())
            .await
            .unwrap();
        repository
            .set_github_sync_bootstrap_uploaded(true)
            .await
            .unwrap();
        insert_test_call(&repository, "local-a", "2026-06-05", 100).await;

        let transport = FakeGitHubTransport::default();
        run_github_sync_once_with_transport(&repository, &transport, SyncRunMode::Normal)
            .await
            .expect("first sync succeeds");
        let first_upload_at = app_setting_value(&repository, "github_sync_last_upload_at").await;
        let uploaded_before_second_sync = transport
            .uploaded
            .lock()
            .expect("fake transport lock")
            .len();

        let result =
            run_github_sync_once_with_transport(&repository, &transport, SyncRunMode::Normal)
                .await
                .expect("second sync succeeds");
        let uploaded_after_second_sync = transport
            .uploaded
            .lock()
            .expect("fake transport lock")
            .len();

        assert_eq!(result.uploaded_shards, 0);
        assert_eq!(uploaded_after_second_sync, uploaded_before_second_sync);
        assert_eq!(
            app_setting_value(&repository, "github_sync_last_upload_at").await,
            first_upload_at
        );
    }

    #[tokio::test]
    async fn sync_uploads_empty_day_shard_when_previously_uploaded_date_is_cleared() {
        let repository = TokenScopeRepository::connect_in_memory().await.unwrap();
        repository.migrate().await.unwrap();
        repository
            .save_github_sync_settings(&valid_settings())
            .await
            .unwrap();
        repository
            .set_github_sync_bootstrap_uploaded(true)
            .await
            .unwrap();
        insert_test_call(&repository, "local-a", "2026-06-05", 100).await;

        let transport = FakeGitHubTransport::default();
        run_github_sync_once_with_transport(&repository, &transport, SyncRunMode::Normal)
            .await
            .expect("first sync succeeds");
        query("DELETE FROM llm_call WHERE id = 'local-a'")
            .execute(repository.pool())
            .await
            .expect("test call deleted");

        let result =
            run_github_sync_once_with_transport(&repository, &transport, SyncRunMode::Normal)
                .await
                .expect("second sync succeeds");

        assert_eq!(result.uploaded_shards, 1);
        assert!(transport
            .uploaded_path("days/2026-06-05.tokenscope.zst.enc")
            .contains("2026-06-05"));
    }

    #[tokio::test]
    async fn sync_downloads_and_imports_remote_device_bootstrap() {
        let repository = TokenScopeRepository::connect_in_memory().await.unwrap();
        repository.migrate().await.unwrap();
        repository
            .save_github_sync_settings(&valid_settings())
            .await
            .unwrap();
        let layout = GitHubSyncLayout::new("tokenscope-sync".to_string());
        let transport = FakeGitHubTransport::default();
        let remote_package = remote_bootstrap_package("remote-a", "2026-06-05", 250);
        transport.seed_file(
            &layout.bootstrap_path("remote-a"),
            encrypted_package_bytes(&remote_package, "sync-password"),
        );

        let result =
            run_github_sync_once_with_transport(&repository, &transport, SyncRunMode::Normal)
                .await
                .expect("sync succeeds");

        assert_eq!(result.downloaded_shards, 1);
        assert_eq!(result.imported, 1);
        assert_eq!(
            imported_token_sum(&repository, "device-remote-a", "2026-06-05").await,
            250
        );
    }

    #[tokio::test]
    async fn sync_downloads_and_imports_remote_device_day_shards() {
        let repository = TokenScopeRepository::connect_in_memory().await.unwrap();
        repository.migrate().await.unwrap();
        repository
            .save_github_sync_settings(&valid_settings())
            .await
            .unwrap();
        repository
            .set_github_sync_bootstrap_uploaded(true)
            .await
            .unwrap();
        let layout = GitHubSyncLayout::new("tokenscope-sync".to_string());
        let transport = FakeGitHubTransport::default();
        let remote_package = remote_day_package("remote-a", "2026-06-05", 300);
        transport.seed_file(
            &layout.day_path("remote-a", "2026-06-05"),
            encrypted_package_bytes(&remote_package, "sync-password"),
        );

        let result =
            run_github_sync_once_with_transport(&repository, &transport, SyncRunMode::Normal)
                .await
                .expect("sync succeeds");

        assert_eq!(result.downloaded_shards, 1);
        assert_eq!(result.imported, 1);
        assert_eq!(
            imported_token_sum(&repository, "device-remote-a", "2026-06-05").await,
            300
        );
    }

    #[tokio::test]
    async fn sync_skips_remote_day_download_when_github_sha_is_unchanged() {
        let repository = TokenScopeRepository::connect_in_memory().await.unwrap();
        repository.migrate().await.unwrap();
        repository
            .save_github_sync_settings(&valid_settings())
            .await
            .unwrap();
        repository
            .set_github_sync_bootstrap_uploaded(true)
            .await
            .unwrap();
        let layout = GitHubSyncLayout::new("tokenscope-sync".to_string());
        let day_path = layout.day_path("remote-a", "2026-06-05");
        let transport = FakeGitHubTransport::default();
        let remote_package = remote_day_package("remote-a", "2026-06-05", 300);
        transport.seed_file(
            &day_path,
            encrypted_package_bytes(&remote_package, "sync-password"),
        );

        run_github_sync_once_with_transport(&repository, &transport, SyncRunMode::Normal)
            .await
            .expect("first sync succeeds");
        assert_eq!(transport.get_file_count_for(&day_path), 1);

        let result =
            run_github_sync_once_with_transport(&repository, &transport, SyncRunMode::Normal)
                .await
                .expect("second sync succeeds");

        assert_eq!(result.downloaded_shards, 0);
        assert_eq!(transport.get_file_count_for(&day_path), 1);
    }

    #[tokio::test]
    async fn today_sync_skips_remote_day_download_when_github_sha_is_unchanged() {
        let repository = TokenScopeRepository::connect_in_memory().await.unwrap();
        repository.migrate().await.unwrap();
        repository
            .save_github_sync_settings(&valid_settings())
            .await
            .unwrap();
        repository
            .set_github_sync_bootstrap_uploaded(true)
            .await
            .unwrap();
        let layout = GitHubSyncLayout::new("tokenscope-sync".to_string());
        let day_path = layout.day_path("remote-a", "2026-06-05");
        let transport = FakeGitHubTransport::default();
        let remote_package = remote_day_package("remote-a", "2026-06-05", 300);
        transport.seed_file(
            &day_path,
            encrypted_package_bytes(&remote_package, "sync-password"),
        );

        sync_today_with_transport(&repository, &transport, "2026-06-05")
            .await
            .expect("first today sync succeeds");
        assert_eq!(transport.get_file_count_for(&day_path), 1);

        let result = sync_today_with_transport(&repository, &transport, "2026-06-05")
            .await
            .expect("second today sync succeeds");

        assert_eq!(result.downloaded_shards, 0);
        assert_eq!(transport.get_file_count_for(&day_path), 1);
    }

    #[tokio::test]
    async fn force_reimport_remote_device_ignores_recorded_hash_and_repairs_imported_rows() {
        let repository = TokenScopeRepository::connect_in_memory().await.unwrap();
        repository.migrate().await.unwrap();
        repository
            .save_github_sync_settings(&valid_settings())
            .await
            .unwrap();
        repository
            .set_github_sync_bootstrap_uploaded(true)
            .await
            .unwrap();
        let layout = GitHubSyncLayout::new("tokenscope-sync".to_string());
        let transport = FakeGitHubTransport::default();
        let remote_package = remote_day_package("remote-a", "2026-06-05", 300);
        let plaintext = package_bytes(&remote_package).expect("package serializes");
        let content_hash = content_hash_hex(&plaintext);
        transport.seed_file(
            &layout.day_path("remote-a", "2026-06-05"),
            encrypted_package_bytes(&remote_package, "sync-password"),
        );

        repository
            .record_github_sync_shard(&GitHubSyncShardStateInput {
                device_id: "remote-a".to_string(),
                shard_kind: "day".to_string(),
                shard_date: Some("2026-06-05".to_string()),
                content_hash,
                github_blob_sha: None,
                github_path: layout.day_path("remote-a", "2026-06-05"),
                imported_at: Some("2026-06-06T10:00:00+08:00".to_string()),
            })
            .await
            .unwrap();
        let stale_package = remote_day_package("remote-a", "2026-06-05", 100);
        crate::github_sync::packages::import_github_sync_package(&repository, stale_package)
            .await
            .unwrap();
        assert_eq!(
            imported_token_sum(&repository, "device-remote-a", "2026-06-05").await,
            100
        );

        let result =
            force_reimport_remote_device_with_transport(&repository, &transport, "remote-a")
                .await
                .expect("force reimport succeeds");

        assert_eq!(result.downloaded_shards, 1);
        assert_eq!(result.imported, 1);
        assert_eq!(
            imported_token_sum(&repository, "device-remote-a", "2026-06-05").await,
            300
        );
    }

    #[tokio::test]
    async fn sync_records_remote_download_errors_for_status_ui() {
        let repository = TokenScopeRepository::connect_in_memory().await.unwrap();
        repository.migrate().await.unwrap();
        repository
            .save_github_sync_settings(&valid_settings())
            .await
            .unwrap();
        repository
            .set_github_sync_bootstrap_uploaded(true)
            .await
            .unwrap();
        let layout = GitHubSyncLayout::new("tokenscope-sync".to_string());
        let transport = FakeGitHubTransport::default();
        transport.seed_file(&layout.bootstrap_path("remote-a"), Vec::new());

        let err = run_github_sync_once_with_transport(&repository, &transport, SyncRunMode::Normal)
            .await
            .expect_err("corrupt remote shard fails");

        assert!(err.contains("GitHub 同步分片加密信封解析失败"));
        assert!(err.contains("tokenscope-sync/v1/devices/remote-a/bootstrap.tokenscope.zst.enc"));
        assert_eq!(
            app_setting_value(&repository, "github_sync_last_status").await,
            Some("error".to_string())
        );
        assert_eq!(
            app_setting_value(&repository, "github_sync_last_message").await,
            Some(err)
        );
    }

    #[tokio::test]
    async fn sync_reports_remote_manifest_path_when_manifest_password_is_wrong() {
        let repository = TokenScopeRepository::connect_in_memory().await.unwrap();
        repository.migrate().await.unwrap();
        repository
            .save_github_sync_settings(&valid_settings())
            .await
            .unwrap();
        repository
            .set_github_sync_bootstrap_uploaded(true)
            .await
            .unwrap();
        let layout = GitHubSyncLayout::new("tokenscope-sync".to_string());
        let transport = FakeGitHubTransport::default();
        transport.seed_file(
            &layout.manifest_path("remote-a"),
            encrypted_manifest_bytes(
                "remote-a",
                GITHUB_SYNC_DATA_MODE_DETAIL_V2,
                "wrong-password",
            ),
        );

        let err = run_github_sync_once_with_transport(&repository, &transport, SyncRunMode::Normal)
            .await
            .expect_err("wrong manifest password fails");

        assert!(err.contains("GitHub 同步远端 manifest 读取失败"));
        assert!(err.contains("解密失败"));
        assert!(err.contains("tokenscope-sync/v1/devices/remote-a/manifest.enc"));
    }

    #[tokio::test]
    async fn sync_continues_when_remote_manifest_is_not_an_encrypted_envelope() {
        let repository = TokenScopeRepository::connect_in_memory().await.unwrap();
        repository.migrate().await.unwrap();
        repository
            .save_github_sync_settings(&valid_settings())
            .await
            .unwrap();
        repository
            .set_github_sync_bootstrap_uploaded(true)
            .await
            .unwrap();
        let layout = GitHubSyncLayout::new("tokenscope-sync".to_string());
        let transport = FakeGitHubTransport::default();
        transport.seed_file(
            &layout.manifest_path("remote-a"),
            b"not an envelope".to_vec(),
        );
        let remote_package = remote_day_package("remote-a", "2026-06-05", 300);
        transport.seed_file(
            &layout.day_path("remote-a", "2026-06-05"),
            encrypted_package_bytes(&remote_package, "sync-password"),
        );

        let result =
            run_github_sync_once_with_transport(&repository, &transport, SyncRunMode::Normal)
                .await
                .expect("sync ignores corrupt optional manifest");

        assert_eq!(result.downloaded_shards, 1);
        assert_eq!(result.imported, 1);
        assert_eq!(
            imported_token_sum(&repository, "device-remote-a", "2026-06-05").await,
            300
        );
    }

    #[test]
    fn github_sync_runtime_exposes_running_status_and_rejects_overlap() {
        let runtime = GitHubSyncRuntime::default();

        assert!(!runtime.status().running);
        let guard = runtime
            .try_start(SyncRunMode::Normal)
            .expect("first sync can start");
        let running_status = runtime.status();

        assert!(running_status.running);
        assert_eq!(running_status.mode.as_deref(), Some("normal"));
        assert!(running_status.started_at.is_some());
        assert!(runtime.try_start(SyncRunMode::ForceBootstrap).is_none());

        runtime.update_phase("upload", "上传本机分片");
        let phase_status = runtime.status();
        assert_eq!(phase_status.phase.as_deref(), Some("upload"));
        assert_eq!(phase_status.message.as_deref(), Some("上传本机分片"));

        runtime.finish(&guard, "success", "GitHub 同步完成");
        let finished_status = runtime.status();
        assert!(!finished_status.running);
        assert_eq!(finished_status.last_status.as_deref(), Some("success"));
        assert_eq!(finished_status.message.as_deref(), Some("GitHub 同步完成"));
    }

    #[tokio::test]
    async fn github_sync_runtime_returns_busy_when_another_sync_is_running() {
        let repository = TokenScopeRepository::connect_in_memory().await.unwrap();
        repository.migrate().await.unwrap();
        let runtime = GitHubSyncRuntime::default();
        let _guard = runtime
            .try_start(SyncRunMode::Normal)
            .expect("first sync can start");

        let result = run_once_with_runtime(&repository, &runtime, false)
            .await
            .expect("busy result is returned");

        assert_eq!(result.status, "busy");
        assert_eq!(result.message, "已有 GitHub 同步任务正在执行。");
    }

    #[test]
    fn day_file_dates_only_accept_iso_date_shards() {
        assert_eq!(
            parse_day_file_date("2026-06-05.tokenscope.zst.enc").as_deref(),
            Some("2026-06-05")
        );
        assert!(parse_day_file_date("notes.tokenscope.zst.enc").is_none());
        assert!(parse_day_file_date("2026-6-5.tokenscope.zst.enc").is_none());
        assert!(parse_day_file_date("2026-06-05.json").is_none());
    }

    fn valid_settings() -> GitHubSyncSettingsInput {
        GitHubSyncSettingsInput {
            enabled: true,
            owner: "rick".to_string(),
            repo: "tokenscope-sync".to_string(),
            branch: "main".to_string(),
            path_prefix: "tokenscope-sync".to_string(),
            data_mode: None,
            token: Some("ghp_test_token".to_string()),
            sync_password: Some("sync-password".to_string()),
        }
    }

    async fn insert_test_call(
        repository: &TokenScopeRepository,
        id: &str,
        date_local: &str,
        total_tokens: i64,
    ) {
        query(
            r#"
      INSERT INTO llm_call (
        id,
        started_at,
        date_local,
        provider,
        model_requested,
        total_tokens,
        total_billable_tokens,
        estimated_cost_usd,
        status,
        created_at
      ) VALUES (?1, ?2, ?3, 'codex', 'gpt-5', ?4, ?4, 0.0, 'success', ?2)
      "#,
        )
        .bind(id)
        .bind(format!("{date_local}T10:00:00+08:00"))
        .bind(date_local)
        .bind(total_tokens)
        .execute(repository.pool())
        .await
        .expect("test call inserted");
    }

    fn remote_bootstrap_package(
        device_id: &str,
        date_local: &str,
        total_tokens: i64,
    ) -> GitHubSyncPackage {
        remote_package(device_id, "bootstrap", None, date_local, total_tokens)
    }

    fn remote_day_package(
        device_id: &str,
        date_local: &str,
        total_tokens: i64,
    ) -> GitHubSyncPackage {
        remote_package(
            device_id,
            "day",
            Some(date_local.to_string()),
            date_local,
            total_tokens,
        )
    }

    fn remote_package(
        device_id: &str,
        shard_kind: &str,
        shard_date: Option<String>,
        date_local: &str,
        total_tokens: i64,
    ) -> GitHubSyncPackage {
        GitHubSyncPackage {
            package_type: "tokenscope.github_sync".to_string(),
            version: 1,
            exported_at: format!("{date_local}T10:00:00+08:00"),
            device: GitHubSyncDevice {
                id: device_id.to_string(),
                name: device_id.to_string(),
            },
            shard: GitHubSyncShard {
                kind: shard_kind.to_string(),
                date: shard_date,
            },
            calls: vec![crate::device_packages::DevicePackageCall {
                source_key: "github-sync-test".to_string(),
                external_id: format!("{device_id}-{date_local}-{total_tokens}"),
                id: format!("{device_id}-{date_local}-{total_tokens}"),
                started_at: format!("{date_local}T10:00:00+08:00"),
                ended_at: None,
                date_local: date_local.to_string(),
                provider: "codex".to_string(),
                provider_config_id: None,
                api_type: None,
                model_requested: Some("gpt-5".to_string()),
                model_response: None,
                agent_id: None,
                agent_name: None,
                agent_run_id: None,
                workflow_id: None,
                workflow_step: None,
                session_id: None,
                trace_id: None,
                span_id: None,
                parent_span_id: None,
                project_id: None,
                user_id: None,
                environment: None,
                feature: None,
                input_tokens: 0,
                output_tokens: total_tokens,
                cached_input_tokens: 0,
                cache_write_input_tokens: 0,
                reasoning_output_tokens: 0,
                audio_input_tokens: 0,
                audio_output_tokens: 0,
                image_input_tokens: 0,
                image_output_tokens: 0,
                total_tokens,
                total_billable_tokens: total_tokens,
                request_count: 1,
                tool_call_count: 0,
                retry_count: 0,
                latency_ms: None,
                http_status: None,
                status: "success".to_string(),
                error_type: None,
                error_message: None,
                estimated_cost_usd: 0.0,
                cost_currency: "USD".to_string(),
                provider_reported_cost_usd: None,
                reconciled_cost_usd: None,
                cost_source: None,
                usage_source: None,
                request_hash: None,
                response_hash: None,
                prompt_template_id: None,
                created_at: format!("{date_local}T10:00:00+08:00"),
            }],
            total_tokens,
        }
    }

    fn encrypted_package_bytes(package: &GitHubSyncPackage, sync_password: &str) -> Vec<u8> {
        let plaintext = package_bytes(package).expect("package serializes");
        let envelope = encrypt_sync_payload(&plaintext, sync_password).expect("package encrypts");
        serde_json::to_vec(&envelope).expect("envelope serializes")
    }

    fn encrypted_manifest_bytes(device_id: &str, data_mode: &str, sync_password: &str) -> Vec<u8> {
        let manifest = GitHubSyncManifest {
            version: 1,
            device_id: device_id.to_string(),
            updated_at: "2026-06-05T10:00:00+08:00".to_string(),
            active_data_mode: data_mode.to_string(),
        };
        let plaintext = serde_json::to_vec(&manifest).expect("manifest serializes");
        let envelope = encrypt_sync_payload(&plaintext, sync_password).expect("manifest encrypts");
        serde_json::to_vec(&envelope).expect("envelope serializes")
    }

    async fn imported_token_sum(
        repository: &TokenScopeRepository,
        dataset_id: &str,
        date_local: &str,
    ) -> i64 {
        query(
            "SELECT COALESCE(SUM(total_tokens), 0) AS total FROM llm_call WHERE origin_dataset_id = ?1 AND date_local = ?2",
        )
        .bind(dataset_id)
        .bind(date_local)
        .fetch_one(repository.pool())
        .await
        .expect("sum reads")
        .get("total")
    }

    async fn app_setting_value(repository: &TokenScopeRepository, key: &str) -> Option<String> {
        query("SELECT value FROM app_setting WHERE key = ?1")
            .bind(key)
            .fetch_optional(repository.pool())
            .await
            .expect("setting reads")
            .map(|row| row.get("value"))
    }
}
