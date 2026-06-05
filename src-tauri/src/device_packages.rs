use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

use chrono::Local;
use serde::{Deserialize, Serialize};
use sqlx::query_as;

use crate::db::{
    DevicePackageImportResult, ExternalDatasetImportCall, ExternalDatasetInput, NewLlmCall,
    TokenScopeRepository,
};

const PACKAGE_TYPE: &str = "tokenscope.device_dataset";
const PACKAGE_VERSION: i64 = 2;

fn default_cost_currency() -> String {
    "USD".to_string()
}

#[derive(Debug, Serialize, Deserialize)]
struct DeviceDatasetPackage {
    package_type: String,
    version: i64,
    exported_at: String,
    device: DevicePackageDevice,
    calls: Vec<DevicePackageCall>,
}

#[derive(Debug, Serialize, Deserialize)]
struct DevicePackageDevice {
    id: String,
    name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub(crate) struct DevicePackageCall {
    pub(crate) source_key: String,
    pub(crate) external_id: String,
    pub(crate) id: String,
    pub(crate) started_at: String,
    pub(crate) ended_at: Option<String>,
    pub(crate) date_local: String,
    pub(crate) provider: String,
    pub(crate) provider_config_id: Option<String>,
    pub(crate) api_type: Option<String>,
    pub(crate) model_requested: Option<String>,
    pub(crate) model_response: Option<String>,
    pub(crate) agent_id: Option<String>,
    pub(crate) agent_name: Option<String>,
    pub(crate) agent_run_id: Option<String>,
    pub(crate) workflow_id: Option<String>,
    pub(crate) workflow_step: Option<String>,
    pub(crate) session_id: Option<String>,
    pub(crate) trace_id: Option<String>,
    pub(crate) span_id: Option<String>,
    pub(crate) parent_span_id: Option<String>,
    pub(crate) project_id: Option<String>,
    pub(crate) user_id: Option<String>,
    pub(crate) environment: Option<String>,
    pub(crate) feature: Option<String>,
    pub(crate) input_tokens: i64,
    pub(crate) output_tokens: i64,
    pub(crate) cached_input_tokens: i64,
    pub(crate) cache_write_input_tokens: i64,
    pub(crate) reasoning_output_tokens: i64,
    pub(crate) audio_input_tokens: i64,
    pub(crate) audio_output_tokens: i64,
    pub(crate) image_input_tokens: i64,
    pub(crate) image_output_tokens: i64,
    pub(crate) total_tokens: i64,
    pub(crate) total_billable_tokens: i64,
    pub(crate) request_count: i64,
    pub(crate) tool_call_count: i64,
    pub(crate) retry_count: i64,
    pub(crate) latency_ms: Option<i64>,
    pub(crate) http_status: Option<i64>,
    pub(crate) status: String,
    pub(crate) error_type: Option<String>,
    pub(crate) error_message: Option<String>,
    pub(crate) estimated_cost_usd: f64,
    #[serde(default = "default_cost_currency")]
    pub(crate) cost_currency: String,
    pub(crate) provider_reported_cost_usd: Option<f64>,
    pub(crate) reconciled_cost_usd: Option<f64>,
    pub(crate) cost_source: Option<String>,
    pub(crate) usage_source: Option<String>,
    pub(crate) request_hash: Option<String>,
    pub(crate) response_hash: Option<String>,
    pub(crate) prompt_template_id: Option<String>,
    pub(crate) created_at: String,
}

pub async fn export_device_dataset_package(
    repository: &TokenScopeRepository,
    export_dir: &Path,
) -> Result<String, String> {
    std::fs::create_dir_all(export_dir).map_err(|err| err.to_string())?;
    let device_id = repository
        .get_or_create_local_device_id()
        .await
        .map_err(|err| err.to_string())?;
    let device_name = local_device_name();
    let exported_at = Local::now().to_rfc3339();
    let calls = exportable_local_calls(repository)
        .await
        .map_err(|err| err.to_string())?;
    let package = DeviceDatasetPackage {
        package_type: PACKAGE_TYPE.to_string(),
        version: PACKAGE_VERSION,
        exported_at,
        device: DevicePackageDevice {
            id: device_id,
            name: device_name.clone(),
        },
        calls,
    };

    let filename = format!(
        "tokenscope-{}-{}.tokenscope",
        safe_file_segment(&device_name),
        Local::now().format("%Y%m%d-%H%M%S")
    );
    let path = export_dir.join(filename);
    let json = serde_json::to_string_pretty(&package).map_err(|err| err.to_string())?;
    std::fs::write(&path, json).map_err(|err| err.to_string())?;

    Ok(path.to_string_lossy().to_string())
}

pub async fn import_device_dataset_package(
    repository: &TokenScopeRepository,
    path: &Path,
) -> Result<DevicePackageImportResult, String> {
    let json = std::fs::read_to_string(path).map_err(|err| err.to_string())?;
    let package: DeviceDatasetPackage =
        serde_json::from_str(&json).map_err(|err| err.to_string())?;
    validate_package(&package)?;
    let local_device_id = repository
        .get_or_create_local_device_id()
        .await
        .map_err(|err| err.to_string())?;
    if package.device.id == local_device_id {
        return Err("不能导入当前设备自己的 .tokenscope 数据包。".to_string());
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
        source_path: Some(path.to_string_lossy().to_string()),
        imported_at: now.clone(),
        updated_at: now,
        calls,
        total_tokens,
        estimated_cost_usd,
        cost_currency,
    };
    let dataset = repository
        .replace_external_dataset(&input, &import_calls)
        .await
        .map_err(|err| err.to_string())?;

    Ok(DevicePackageImportResult {
        dataset,
        imported: calls,
        skipped,
    })
}

pub(crate) async fn exportable_local_calls(
    repository: &TokenScopeRepository,
) -> Result<Vec<DevicePackageCall>, sqlx::Error> {
    query_as::<_, DevicePackageCall>(
        r#"
      WITH import_sources AS (
        SELECT
          llm_call_id,
          MIN(source) AS source_key,
          MIN(external_id) AS external_id
        FROM agent_import_map
        WHERE dataset_id IS NULL
        GROUP BY llm_call_id
      )
      SELECT
        COALESCE(import_sources.source_key, 'tokenscope_local') AS source_key,
        COALESCE(import_sources.external_id, c.id) AS external_id,
        c.id,
        c.started_at,
        c.ended_at,
        c.date_local,
        c.provider,
        c.provider_config_id,
        c.api_type,
        c.model_requested,
        c.model_response,
        c.agent_id,
        c.agent_name,
        c.agent_run_id,
        c.workflow_id,
        c.workflow_step,
        c.session_id,
        c.trace_id,
        c.span_id,
        c.parent_span_id,
        c.project_id,
        c.user_id,
        c.environment,
        c.feature,
        c.input_tokens,
        c.output_tokens,
        c.cached_input_tokens,
        c.cache_write_input_tokens,
        c.reasoning_output_tokens,
        c.audio_input_tokens,
        c.audio_output_tokens,
        c.image_input_tokens,
        c.image_output_tokens,
        c.total_tokens,
        c.total_billable_tokens,
        c.request_count,
        c.tool_call_count,
        c.retry_count,
        c.latency_ms,
        c.http_status,
        c.status,
        c.error_type,
        c.error_message,
        c.estimated_cost_usd,
        c.cost_currency,
        c.provider_reported_cost_usd,
        c.reconciled_cost_usd,
        c.cost_source,
        c.usage_source,
        c.request_hash,
        c.response_hash,
        c.prompt_template_id,
        c.created_at
      FROM llm_call c
      LEFT JOIN import_sources ON import_sources.llm_call_id = c.id
      WHERE c.origin_dataset_id IS NULL
      ORDER BY c.started_at ASC, c.id ASC
      "#,
    )
    .fetch_all(repository.pool())
    .await
}

fn validate_package(package: &DeviceDatasetPackage) -> Result<(), String> {
    if package.package_type != PACKAGE_TYPE {
        return Err("不是有效的 TokenScope 设备数据包。".to_string());
    }
    if !(1..=PACKAGE_VERSION).contains(&package.version) {
        return Err(format!("不支持的数据包版本：{}", package.version));
    }
    if package.device.id.trim().is_empty() {
        return Err("数据包缺少设备 ID。".to_string());
    }

    Ok(())
}

impl DevicePackageCall {
    pub(crate) fn into_import_call(self, dataset_id: &str) -> Option<ExternalDatasetImportCall> {
        if self.source_key.trim().is_empty()
            || self.external_id.trim().is_empty()
            || self.started_at.trim().is_empty()
            || self.date_local.trim().is_empty()
            || self.provider.trim().is_empty()
        {
            return None;
        }

        let call_id = format!(
            "external-{:016x}",
            stable_hash(&format!(
                "{}:{}:{}",
                dataset_id,
                self.source_key.trim(),
                self.external_id.trim()
            ))
        );

        Some(ExternalDatasetImportCall {
            source_key: self.source_key,
            external_id: self.external_id,
            call: NewLlmCall {
                id: call_id,
                started_at: self.started_at,
                ended_at: self.ended_at,
                date_local: self.date_local,
                provider: self.provider,
                provider_config_id: self.provider_config_id,
                api_type: self.api_type,
                model_requested: self.model_requested,
                model_response: self.model_response,
                agent_id: self.agent_id,
                agent_name: self.agent_name,
                agent_run_id: self.agent_run_id,
                workflow_id: self.workflow_id,
                workflow_step: self.workflow_step,
                session_id: self.session_id,
                trace_id: self.trace_id,
                span_id: self.span_id,
                parent_span_id: self.parent_span_id,
                project_id: self.project_id,
                user_id: self.user_id,
                environment: self.environment,
                feature: self.feature,
                input_tokens: self.input_tokens,
                output_tokens: self.output_tokens,
                cached_input_tokens: self.cached_input_tokens,
                cache_write_input_tokens: self.cache_write_input_tokens,
                reasoning_output_tokens: self.reasoning_output_tokens,
                audio_input_tokens: self.audio_input_tokens,
                audio_output_tokens: self.audio_output_tokens,
                image_input_tokens: self.image_input_tokens,
                image_output_tokens: self.image_output_tokens,
                total_tokens: self.total_tokens,
                total_billable_tokens: self.total_billable_tokens,
                request_count: self.request_count,
                tool_call_count: self.tool_call_count,
                retry_count: self.retry_count,
                latency_ms: self.latency_ms,
                http_status: self.http_status,
                status: self.status,
                error_type: self.error_type,
                error_message: self.error_message,
                estimated_cost_usd: self.estimated_cost_usd,
                cost_currency: normalize_package_currency(&self.cost_currency),
                provider_reported_cost_usd: self.provider_reported_cost_usd,
                reconciled_cost_usd: self.reconciled_cost_usd,
                cost_source: self.cost_source,
                usage_source: self.usage_source,
                raw_usage_json: None,
                raw_response_json: None,
                request_hash: self.request_hash,
                response_hash: self.response_hash,
                prompt_template_id: self.prompt_template_id,
                created_at: self.created_at,
            },
        })
    }
}

pub(crate) fn local_device_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .or_else(|_| std::env::var("USERDOMAIN"))
        .unwrap_or_else(|_| "local-device".to_string())
}

pub(crate) fn dataset_id_for_device(device_id: &str) -> String {
    format!("device-{}", safe_id_segment(device_id))
}

fn normalize_package_currency(value: &str) -> String {
    let currency = value.trim().to_ascii_uppercase();
    if currency.is_empty() {
        "USD".to_string()
    } else {
        currency
    }
}

pub(crate) fn summarize_cost_currency<'a>(currencies: impl Iterator<Item = &'a str>) -> String {
    let mut current: Option<String> = None;
    for currency in currencies {
        let currency = normalize_package_currency(currency);
        match current.as_deref() {
            None => current = Some(currency),
            Some("MIXED") => return "MIXED".to_string(),
            Some(existing) if existing == currency => {}
            Some(_) => return "MIXED".to_string(),
        }
    }
    current.unwrap_or_else(|| "USD".to_string())
}

fn safe_file_segment(value: &str) -> String {
    let safe = safe_id_segment(value);
    if safe.is_empty() {
        "device".to_string()
    } else {
        safe
    }
}

fn safe_id_segment(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn stable_hash(value: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use sqlx::{query, Row};
    use uuid::Uuid;

    use super::{export_device_dataset_package, import_device_dataset_package};
    use crate::db::TokenScopeRepository;

    #[tokio::test]
    async fn package_round_trip_sanitizes_raw_payloads_and_imports_external_dataset() {
        let source = TokenScopeRepository::connect_in_memory()
            .await
            .expect("source database connects");
        source.migrate().await.expect("source migrations run");
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
        raw_usage_json,
        raw_response_json,
        created_at
      ) VALUES (
        'local-secret-call',
        '2026-05-30T12:00:00+08:00',
        '2026-05-30',
        'codex',
        'gpt-5',
        123,
        123,
        0.42,
        'success',
        '{"prompt":"secret prompt"}',
        '{"response":"secret response"}',
        '2026-05-30T12:00:01+08:00'
      )
      "#,
        )
        .execute(source.pool())
        .await
        .expect("source call inserted");

        let export_dir = std::env::temp_dir().join(format!("tokenscope-test-{}", Uuid::new_v4()));
        let package_path = export_device_dataset_package(&source, &export_dir)
            .await
            .expect("package exported");
        assert!(package_path.ends_with(".tokenscope"));
        let package_json = std::fs::read_to_string(&package_path).expect("package file readable");
        assert!(!package_json.contains("secret prompt"));
        assert!(!package_json.contains("secret response"));

        let target = TokenScopeRepository::connect_in_memory()
            .await
            .expect("target database connects");
        target.migrate().await.expect("target migrations run");
        let result = import_device_dataset_package(&target, std::path::Path::new(&package_path))
            .await
            .expect("package imported");

        assert_eq!(result.imported, 1);
        assert_eq!(result.skipped, 0);
        let summary = target
            .dashboard_summary("2026-05-30", "2026-05-30")
            .await
            .expect("summary reads imported package");
        assert_eq!(summary.calls, 1);
        assert_eq!(summary.total_tokens, 123);

        let imported_raw_count: i64 = query(
            r#"
      SELECT COUNT(*)
      FROM llm_call
      WHERE origin_dataset_id = ?1
        AND raw_usage_json IS NULL
        AND raw_response_json IS NULL
      "#,
        )
        .bind(&result.dataset.id)
        .fetch_one(target.pool())
        .await
        .expect("imported raw count")
        .try_get(0)
        .expect("count decodes");
        assert_eq!(imported_raw_count, 1);
    }
}
