use std::collections::BTreeSet;

use chrono::Local;
use serde::Serialize;

use crate::db::{
    GitHubSyncConnectionTestResult, GitHubSyncRunResult, GitHubSyncSettings,
    GitHubSyncShardStateInput, TokenScopeRepository,
};
use crate::github_sync::crypto::{
    content_hash_hex, decrypt_sync_payload, encrypt_sync_payload, SyncEncryptedEnvelope,
};
use crate::github_sync::github::{GitHubContentFile, GitHubContentsClient, GitHubSyncLayout};
use crate::github_sync::packages::{
    export_github_sync_package, import_github_sync_package, GitHubSyncPackage,
    GitHubSyncShardSelector,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncRunMode {
    Normal,
    ForceBootstrap,
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

pub async fn run_once(
    repository: &TokenScopeRepository,
    force_bootstrap: bool,
) -> Result<GitHubSyncRunResult, String> {
    let result = run_once_inner(repository, force_bootstrap).await;
    record_error_result(repository, &result).await;
    result
}

async fn run_once_inner(
    repository: &TokenScopeRepository,
    force_bootstrap: bool,
) -> Result<GitHubSyncRunResult, String> {
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
    let result = run_with_settings(repository, transport, mode, settings).await;
    record_error_result(repository, &result).await;
    result
}

async fn run_with_settings<T: GitHubSyncTransport>(
    repository: &TokenScopeRepository,
    transport: &T,
    mode: SyncRunMode,
    settings: GitHubSyncSettings,
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
    let bootstrap_package =
        export_github_sync_package(repository, GitHubSyncShardSelector::Bootstrap).await?;
    let local_dates = bootstrap_package
        .calls
        .iter()
        .map(|call| call.date_local.clone())
        .collect::<BTreeSet<_>>();

    let mut uploaded_shards = 0;
    let should_upload_bootstrap =
        mode == SyncRunMode::ForceBootstrap || !settings.bootstrap_uploaded;
    if should_upload_bootstrap {
        let path = layout.bootstrap_path(&device_id);
        let hash = package_content_hash(&bootstrap_package)?;
        upload_package(
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
        for date in local_dates {
            let package =
                export_github_sync_package(repository, GitHubSyncShardSelector::Day(date.clone()))
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
            upload_plaintext(
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
                    github_path: path,
                    imported_at: None,
                })
                .await
                .map_err(|err| err.to_string())?;
            uploaded_shards += 1;
        }
    }

    if uploaded_shards > 0 {
        upload_manifest(transport, &layout, &device_id, &sync_password).await?;
    }
    let download_summary =
        download_remote_updates(repository, transport, &layout, &device_id, &sync_password).await?;
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

async fn download_remote_updates<T: GitHubSyncTransport>(
    repository: &TokenScopeRepository,
    transport: &T,
    layout: &GitHubSyncLayout,
    local_device_id: &str,
    sync_password: &str,
) -> Result<GitHubSyncDownloadSummary, String> {
    let mut summary = GitHubSyncDownloadSummary::default();
    let mut device_ids = transport.list_device_dirs(layout).await?;
    device_ids.sort();
    device_ids.dedup();

    for remote_device_id in device_ids {
        if remote_device_id == local_device_id {
            continue;
        }

        let bootstrap_path = layout.bootstrap_path(&remote_device_id);
        import_remote_shard(
            repository,
            transport,
            &bootstrap_path,
            sync_password,
            &mut summary,
        )
        .await?;

        let mut day_files = transport.list_day_files(layout, &remote_device_id).await?;
        day_files.sort_by(|left, right| left.path.cmp(&right.path));
        for day_file in day_files {
            if parse_day_file_date(&day_file.name).is_none() {
                summary.skipped += 1;
                continue;
            }
            import_remote_shard(
                repository,
                transport,
                &day_file.path,
                sync_password,
                &mut summary,
            )
            .await?;
        }
    }

    Ok(summary)
}

async fn import_remote_shard<T: GitHubSyncTransport>(
    repository: &TokenScopeRepository,
    transport: &T,
    path: &str,
    sync_password: &str,
    summary: &mut GitHubSyncDownloadSummary,
) -> Result<(), String> {
    let Some(file) = transport.get_file(path).await? else {
        return Ok(());
    };
    let envelope = serde_json::from_slice::<SyncEncryptedEnvelope>(&file.content)
        .map_err(|err| format!("GitHub 同步分片加密信封解析失败：{err}"))?;
    let plaintext = decrypt_sync_payload(&envelope, sync_password)?;
    let content_hash = content_hash_hex(&plaintext);
    let package = serde_json::from_slice::<GitHubSyncPackage>(&plaintext)
        .map_err(|err| format!("GitHub 同步分片解析失败：{err}"))?;
    let remote_device_id = package.device.id.clone();
    let shard_kind = package.shard.kind.clone();
    let shard_date = package.shard.date.clone();

    if repository
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
            github_path: path.to_string(),
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

async fn upload_package<T: GitHubSyncTransport>(
    transport: &T,
    path: &str,
    package: &GitHubSyncPackage,
    sync_password: &str,
    message: &str,
) -> Result<String, String> {
    let plaintext = package_bytes(package)?;
    let hash = content_hash_hex(&plaintext);
    upload_plaintext(transport, path, &plaintext, sync_password, message).await?;
    Ok(hash)
}

async fn upload_manifest<T: GitHubSyncTransport>(
    transport: &T,
    layout: &GitHubSyncLayout,
    device_id: &str,
    sync_password: &str,
) -> Result<(), String> {
    let manifest = GitHubSyncManifest {
        version: 1,
        device_id: device_id.to_string(),
        updated_at: Local::now().to_rfc3339(),
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
) -> Result<(), String> {
    let envelope = encrypt_sync_payload(plaintext, sync_password)?;
    let encrypted =
        serde_json::to_vec(&envelope).map_err(|err| format!("GitHub 加密信封序列化失败：{err}"))?;
    let sha = transport.get_file(path).await?.map(|file| file.sha);
    transport
        .put_file(path, encrypted, sha, message)
        .await
        .map(|_| ())
}

fn package_bytes(package: &GitHubSyncPackage) -> Result<Vec<u8>, String> {
    serde_json::to_vec(package).map_err(|err| format!("GitHub 同步分片序列化失败：{err}"))
}

fn package_content_hash(package: &GitHubSyncPackage) -> Result<String, String> {
    let mut stable_package = package.clone();
    stable_package.exported_at.clear();
    package_bytes(&stable_package).map(|bytes| content_hash_hex(&bytes))
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

#[derive(Debug, Serialize)]
struct GitHubSyncManifest {
    version: i64,
    device_id: String,
    updated_at: String,
}

#[cfg(test)]
#[derive(Default)]
pub struct FakeGitHubTransport {
    uploaded: std::sync::Mutex<Vec<FakeUploadedFile>>,
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
    use crate::github_sync::packages::{GitHubSyncDevice, GitHubSyncShard};

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
        assert_eq!(
            app_setting_value(&repository, "github_sync_last_status").await,
            Some("error".to_string())
        );
        assert_eq!(
            app_setting_value(&repository, "github_sync_last_message").await,
            Some(err)
        );
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
