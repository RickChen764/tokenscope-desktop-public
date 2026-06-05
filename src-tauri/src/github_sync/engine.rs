use std::collections::BTreeSet;

use chrono::Local;
use serde::Serialize;

use crate::db::{
    GitHubSyncConnectionTestResult, GitHubSyncRunResult, GitHubSyncSettings,
    GitHubSyncShardStateInput, TokenScopeRepository,
};
use crate::github_sync::crypto::{content_hash_hex, encrypt_sync_payload};
use crate::github_sync::github::{GitHubContentFile, GitHubContentsClient, GitHubSyncLayout};
use crate::github_sync::packages::{
    export_github_sync_package, GitHubSyncPackage, GitHubSyncShardSelector,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncRunMode {
    Normal,
    ForceBootstrap,
}

#[allow(async_fn_in_trait)]
pub trait GitHubSyncTransport {
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
    run_with_settings(repository, transport, mode, settings).await
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
        let hash = upload_package(
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
            let hash = content_hash_hex(&plaintext);
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

    upload_manifest(transport, &layout, &device_id, &sync_password).await?;
    let finished_at = Local::now().to_rfc3339();
    let message = format!("GitHub 同步完成：上传 {uploaded_shards} 个本机分片。");
    repository
        .record_github_sync_run("success", &message, Some(&finished_at), None)
        .await
        .map_err(|err| err.to_string())?;

    Ok(GitHubSyncRunResult {
        status: "success".to_string(),
        message,
        uploaded_shards,
        downloaded_shards: 0,
        imported: 0,
        skipped: 0,
        started_at,
        finished_at,
    })
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
}

#[cfg(test)]
impl GitHubSyncTransport for FakeGitHubTransport {
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
                content: Vec::new(),
            }))
    }

    async fn put_file(
        &self,
        path: &str,
        _content: Vec<u8>,
        _sha: Option<String>,
        _message: &str,
    ) -> Result<GitHubContentFile, String> {
        let mut uploaded = self.uploaded.lock().expect("fake transport lock");
        let sha = format!("fake-sha-{}", uploaded.len() + 1);
        uploaded.push(FakeUploadedFile {
            path: path.to_string(),
            sha: sha.clone(),
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
    use sqlx::query;

    use super::*;
    use crate::db::{GitHubSyncSettingsInput, TokenScopeRepository};

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
}
