use chrono::Local;
use serde::{Deserialize, Serialize};

use crate::db::{DevicePackageImportResult, ExternalDatasetInput, TokenScopeRepository};
use crate::device_packages::{
    dataset_id_for_device, exportable_local_calls, local_device_name, summarize_cost_currency,
    DevicePackageCall,
};

const GITHUB_SYNC_PACKAGE_TYPE: &str = "tokenscope.github_sync";
const GITHUB_SYNC_PACKAGE_VERSION: i64 = 1;

#[derive(Debug, Clone)]
pub enum GitHubSyncShardSelector {
    Bootstrap,
    Day(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubSyncDevice {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubSyncShard {
    pub kind: String,
    pub date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubSyncPackage {
    pub package_type: String,
    pub version: i64,
    pub exported_at: String,
    pub device: GitHubSyncDevice,
    pub shard: GitHubSyncShard,
    pub calls: Vec<DevicePackageCall>,
    pub total_tokens: i64,
}

pub async fn export_github_sync_package(
    repository: &TokenScopeRepository,
    selector: GitHubSyncShardSelector,
) -> Result<GitHubSyncPackage, String> {
    let device_id = repository
        .get_or_create_local_device_id()
        .await
        .map_err(|err| err.to_string())?;
    let all_calls = exportable_local_calls(repository)
        .await
        .map_err(|err| err.to_string())?;
    let (shard, calls) = match selector {
        GitHubSyncShardSelector::Bootstrap => (
            GitHubSyncShard {
                kind: "bootstrap".to_string(),
                date: None,
            },
            all_calls,
        ),
        GitHubSyncShardSelector::Day(date) => {
            if date.trim().is_empty() {
                return Err("GitHub 同步日期不能为空".to_string());
            }
            let calls = all_calls
                .into_iter()
                .filter(|call| call.date_local == date)
                .collect::<Vec<_>>();
            (
                GitHubSyncShard {
                    kind: "day".to_string(),
                    date: Some(date),
                },
                calls,
            )
        }
    };
    let total_tokens = calls.iter().map(|call| call.total_tokens).sum::<i64>();

    Ok(GitHubSyncPackage {
        package_type: GITHUB_SYNC_PACKAGE_TYPE.to_string(),
        version: GITHUB_SYNC_PACKAGE_VERSION,
        exported_at: Local::now().to_rfc3339(),
        device: GitHubSyncDevice {
            id: device_id,
            name: local_device_name(),
        },
        shard,
        calls,
        total_tokens,
    })
}

#[allow(dead_code)]
pub async fn import_github_sync_package(
    repository: &TokenScopeRepository,
    package: GitHubSyncPackage,
) -> Result<DevicePackageImportResult, String> {
    validate_package(&package)?;
    let local_device_id = repository
        .get_or_create_local_device_id()
        .await
        .map_err(|err| err.to_string())?;
    if package.device.id == local_device_id {
        return Err("不能导入当前设备自己的 GitHub 同步分片。".to_string());
    }

    let dataset_id = dataset_id_for_device(&package.device.id);
    let now = Local::now().to_rfc3339();
    let mut skipped = 0;
    let mut import_calls = Vec::new();
    for call in package.calls {
        match call.into_import_call(&dataset_id) {
            Some(import_call) => import_calls.push(import_call),
            None => skipped += 1,
        }
    }

    let calls = import_calls.len() as i64;
    let total_tokens = import_calls
        .iter()
        .map(|call| call.call.total_tokens)
        .sum::<i64>();
    let estimated_cost_usd = import_calls
        .iter()
        .map(|call| call.call.estimated_cost_usd)
        .sum::<f64>();
    let cost_currency = summarize_cost_currency(
        import_calls
            .iter()
            .map(|call| call.call.cost_currency.as_str()),
    );
    let input = ExternalDatasetInput {
        id: dataset_id,
        device_id: package.device.id,
        device_name: package.device.name,
        package_version: package.version,
        source_path: Some(format!("github-sync:{}", package.shard.kind)),
        imported_at: now.clone(),
        updated_at: now,
        calls,
        total_tokens,
        estimated_cost_usd,
        cost_currency,
    };

    let dataset = match package.shard.kind.as_str() {
        "bootstrap" => repository
            .replace_external_dataset(&input, &import_calls)
            .await
            .map_err(|err| err.to_string())?,
        "day" => {
            let date = package
                .shard
                .date
                .as_deref()
                .ok_or_else(|| "GitHub day 分片缺少日期".to_string())?;
            repository
                .replace_external_dataset_date(&input, date, &import_calls)
                .await
                .map_err(|err| err.to_string())?
        }
        _ => return Err(format!("不支持的 GitHub 同步分片：{}", package.shard.kind)),
    };

    Ok(DevicePackageImportResult {
        dataset,
        imported: calls,
        skipped,
    })
}

#[allow(dead_code)]
fn validate_package(package: &GitHubSyncPackage) -> Result<(), String> {
    if package.package_type != GITHUB_SYNC_PACKAGE_TYPE {
        return Err("不是有效的 GitHub 同步分片。".to_string());
    }
    if package.version != GITHUB_SYNC_PACKAGE_VERSION {
        return Err(format!("不支持的 GitHub 同步分片版本：{}", package.version));
    }
    if package.device.id.trim().is_empty() {
        return Err("GitHub 同步分片缺少设备 ID。".to_string());
    }
    match package.shard.kind.as_str() {
        "bootstrap" => Ok(()),
        "day"
            if package
                .shard
                .date
                .as_deref()
                .unwrap_or("")
                .trim()
                .is_empty() =>
        {
            Err("GitHub day 分片缺少日期。".to_string())
        }
        "day" => Ok(()),
        _ => Err(format!("不支持的 GitHub 同步分片：{}", package.shard.kind)),
    }
}

#[cfg(test)]
mod tests {
    use sqlx::{query, Row};

    use super::*;
    use crate::db::TokenScopeRepository;
    use crate::device_packages::DevicePackageCall;

    #[tokio::test]
    async fn bootstrap_exports_all_local_calls_and_excludes_imported_rows() {
        let repository = TokenScopeRepository::connect_in_memory().await.unwrap();
        repository.migrate().await.unwrap();
        insert_test_call(&repository, "local-a", "2026-06-04", 100).await;
        insert_test_call(&repository, "local-b", "2026-06-05", 200).await;
        insert_external_dataset_row(&repository, "device-other").await;
        insert_external_dataset_call(&repository, "device-other", "external-a", "2026-06-05", 300)
            .await;

        let package = export_github_sync_package(&repository, GitHubSyncShardSelector::Bootstrap)
            .await
            .expect("bootstrap exports");

        assert_eq!(package.shard.kind, "bootstrap");
        assert_eq!(package.calls.len(), 2);
        assert_eq!(package.total_tokens, 300);
    }

    #[tokio::test]
    async fn day_shard_exports_only_selected_local_date() {
        let repository = TokenScopeRepository::connect_in_memory().await.unwrap();
        repository.migrate().await.unwrap();
        insert_test_call(&repository, "day-a", "2026-06-04", 100).await;
        insert_test_call(&repository, "day-b", "2026-06-05", 200).await;

        let package = export_github_sync_package(
            &repository,
            GitHubSyncShardSelector::Day("2026-06-05".to_string()),
        )
        .await
        .expect("day exports");

        assert_eq!(package.shard.kind, "day");
        assert_eq!(package.shard.date.as_deref(), Some("2026-06-05"));
        assert_eq!(package.calls.len(), 1);
        assert_eq!(package.total_tokens, 200);
    }

    #[tokio::test]
    async fn day_shard_import_replaces_only_that_remote_device_date() {
        let repository = TokenScopeRepository::connect_in_memory().await.unwrap();
        repository.migrate().await.unwrap();
        import_github_sync_package(&repository, package_for("remote-a", "2026-06-04", 100))
            .await
            .unwrap();
        import_github_sync_package(&repository, package_for("remote-a", "2026-06-05", 200))
            .await
            .unwrap();
        import_github_sync_package(&repository, package_for("remote-a", "2026-06-05", 350))
            .await
            .unwrap();

        let june4 =
            count_imported_rows_for_date(&repository, "device-remote-a", "2026-06-04").await;
        let june5_total =
            sum_imported_tokens_for_date(&repository, "device-remote-a", "2026-06-05").await;

        assert_eq!(june4, 1);
        assert_eq!(june5_total, 350);
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

    async fn insert_external_dataset_row(repository: &TokenScopeRepository, device_id: &str) {
        query(
            r#"
      INSERT INTO external_dataset (
        id,
        device_id,
        device_name,
        package_version,
        imported_at,
        updated_at,
        calls,
        total_tokens,
        estimated_cost_usd
      ) VALUES (?1, ?2, ?2, 1, '2026-06-05T10:00:00+08:00', '2026-06-05T10:00:00+08:00', 0, 0, 0.0)
      "#,
        )
        .bind(format!("device-{device_id}"))
        .bind(device_id)
        .execute(repository.pool())
        .await
        .expect("external dataset inserted");
    }

    async fn insert_external_dataset_call(
        repository: &TokenScopeRepository,
        device_id: &str,
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
        total_tokens,
        total_billable_tokens,
        estimated_cost_usd,
        status,
        origin_dataset_id,
        created_at
      ) VALUES (?1, ?2, ?3, 'codex', ?4, ?4, 0.0, 'success', ?5, ?2)
      "#,
        )
        .bind(id)
        .bind(format!("{date_local}T10:00:00+08:00"))
        .bind(date_local)
        .bind(total_tokens)
        .bind(format!("device-{device_id}"))
        .execute(repository.pool())
        .await
        .expect("external dataset call inserted");
    }

    fn package_for(device_id: &str, date_local: &str, total_tokens: i64) -> GitHubSyncPackage {
        GitHubSyncPackage {
            package_type: "tokenscope.github_sync".to_string(),
            version: 1,
            exported_at: "2026-06-05T10:00:00+08:00".to_string(),
            device: GitHubSyncDevice {
                id: device_id.to_string(),
                name: device_id.to_string(),
            },
            shard: GitHubSyncShard {
                kind: "day".to_string(),
                date: Some(date_local.to_string()),
            },
            calls: vec![DevicePackageCall {
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

    async fn count_imported_rows_for_date(
        repository: &TokenScopeRepository,
        dataset_id: &str,
        date_local: &str,
    ) -> i64 {
        query(
            "SELECT COUNT(*) AS count FROM llm_call WHERE origin_dataset_id = ?1 AND date_local = ?2",
        )
        .bind(dataset_id)
        .bind(date_local)
        .fetch_one(repository.pool())
        .await
        .expect("count reads")
        .get("count")
    }

    async fn sum_imported_tokens_for_date(
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
}
