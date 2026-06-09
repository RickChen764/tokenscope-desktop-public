use std::collections::BTreeMap;

use chrono::Local;
use serde::{Deserialize, Serialize};

use crate::db::{
    DevicePackageImportResult, ExternalDatasetImportCall, ExternalDatasetInput,
    ExternalDimensionUsageAggregateInput, ExternalUsageAggregateInput, TokenScopeRepository,
    GITHUB_SYNC_DATA_MODE_AGGREGATE_V3, GITHUB_SYNC_DATA_MODE_DETAIL_V2,
};
use crate::device_packages::{
    dataset_id_for_device, exportable_local_calls, local_device_name, summarize_cost_currency,
    DevicePackageCall,
};

const GITHUB_SYNC_PACKAGE_TYPE: &str = "tokenscope.github_sync";
const GITHUB_SYNC_LEGACY_PACKAGE_VERSION: i64 = 1;
const GITHUB_SYNC_PACKAGE_VERSION: i64 = 2;
const GITHUB_SYNC_AGGREGATE_PACKAGE_VERSION: i64 = 3;

pub const GITHUB_SYNC_COMPACT_CALL_SCHEMA: [&str; 35] = [
    "source_key",
    "external_id",
    "started_at",
    "ended_at",
    "date_local",
    "provider",
    "model_requested",
    "model_response",
    "agent_id",
    "agent_name",
    "workflow_id",
    "workflow_step",
    "session_id",
    "project_id",
    "input_tokens",
    "output_tokens",
    "cached_input_tokens",
    "cache_write_input_tokens",
    "reasoning_output_tokens",
    "audio_input_tokens",
    "audio_output_tokens",
    "image_input_tokens",
    "image_output_tokens",
    "total_tokens",
    "total_billable_tokens",
    "request_count",
    "tool_call_count",
    "retry_count",
    "latency_ms",
    "http_status",
    "status",
    "error_type",
    "estimated_cost_usd",
    "cost_currency",
    "created_at",
];

pub const GITHUB_SYNC_AGGREGATE_DAILY_SCHEMA: [&str; 14] = [
    "date_local",
    "calls",
    "success_calls",
    "error_calls",
    "input_tokens",
    "output_tokens",
    "cached_input_tokens",
    "cache_write_input_tokens",
    "reasoning_output_tokens",
    "total_tokens",
    "estimated_cost_usd",
    "cost_currency",
    "latency_sum_ms",
    "latency_count",
];

pub const GITHUB_SYNC_AGGREGATE_DIMENSION_SCHEMA: [&str; 17] = [
    "date_local",
    "dimension_type",
    "dimension_value",
    "dimension_label",
    "calls",
    "success_calls",
    "error_calls",
    "input_tokens",
    "output_tokens",
    "cached_input_tokens",
    "cache_write_input_tokens",
    "reasoning_output_tokens",
    "total_tokens",
    "estimated_cost_usd",
    "cost_currency",
    "latency_sum_ms",
    "latency_count",
];

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubSyncCompactPackage {
    pub package_type: String,
    pub version: i64,
    pub exported_at: String,
    pub device: GitHubSyncDevice,
    pub shard: GitHubSyncShard,
    pub schema: Vec<String>,
    pub rows: Vec<GitHubSyncCompactCall>,
    pub total_tokens: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubSyncAggregatePackage {
    pub package_type: String,
    pub version: i64,
    pub exported_at: String,
    pub device: GitHubSyncDevice,
    pub shard: GitHubSyncShard,
    pub daily_schema: Vec<String>,
    pub daily_rows: Vec<GitHubSyncAggregateDailyRow>,
    pub dimension_schema: Vec<String>,
    pub dimension_rows: Vec<GitHubSyncAggregateDimensionRow>,
    pub total_tokens: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubSyncCompactCall(
    pub String,
    pub String,
    pub String,
    pub Option<String>,
    pub String,
    pub String,
    pub Option<String>,
    pub Option<String>,
    pub Option<String>,
    pub Option<String>,
    pub Option<String>,
    pub Option<String>,
    pub Option<String>,
    pub Option<String>,
    pub i64,
    pub i64,
    pub i64,
    pub i64,
    pub i64,
    pub i64,
    pub i64,
    pub i64,
    pub i64,
    pub i64,
    pub i64,
    pub i64,
    pub i64,
    pub i64,
    pub Option<i64>,
    pub Option<i64>,
    pub String,
    pub Option<String>,
    pub f64,
    pub String,
    pub String,
);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubSyncAggregateDailyRow(
    pub String,
    pub i64,
    pub i64,
    pub i64,
    pub i64,
    pub i64,
    pub i64,
    pub i64,
    pub i64,
    pub i64,
    pub f64,
    pub String,
    pub i64,
    pub i64,
);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubSyncAggregateDimensionRow(
    pub String,
    pub String,
    pub String,
    pub Option<String>,
    pub i64,
    pub i64,
    pub i64,
    pub i64,
    pub i64,
    pub i64,
    pub i64,
    pub i64,
    pub i64,
    pub f64,
    pub String,
    pub i64,
    pub i64,
);

#[derive(Debug, Clone)]
pub enum GitHubSyncPackageDocument {
    Legacy(GitHubSyncPackage),
    Compact(GitHubSyncCompactPackage),
    Aggregate(GitHubSyncAggregatePackage),
}

#[derive(Debug, Deserialize)]
struct GitHubSyncPackageHeader {
    version: i64,
}

pub async fn export_github_sync_package(
    repository: &TokenScopeRepository,
    selector: GitHubSyncShardSelector,
) -> Result<GitHubSyncCompactPackage, String> {
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

    Ok(GitHubSyncCompactPackage {
        package_type: GITHUB_SYNC_PACKAGE_TYPE.to_string(),
        version: GITHUB_SYNC_PACKAGE_VERSION,
        exported_at: Local::now().to_rfc3339(),
        device: GitHubSyncDevice {
            id: device_id,
            name: local_device_name(),
        },
        shard,
        schema: GITHUB_SYNC_COMPACT_CALL_SCHEMA
            .iter()
            .map(ToString::to_string)
            .collect(),
        rows: calls
            .into_iter()
            .map(GitHubSyncCompactCall::from_device_call)
            .collect(),
        total_tokens,
    })
}

pub async fn export_github_sync_aggregate_package(
    repository: &TokenScopeRepository,
    selector: GitHubSyncShardSelector,
) -> Result<GitHubSyncAggregatePackage, String> {
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
    let (daily_rows, dimension_rows) = aggregate_calls(calls);
    let total_tokens = daily_rows.iter().map(|row| row.9).sum::<i64>();

    Ok(GitHubSyncAggregatePackage {
        package_type: GITHUB_SYNC_PACKAGE_TYPE.to_string(),
        version: GITHUB_SYNC_AGGREGATE_PACKAGE_VERSION,
        exported_at: Local::now().to_rfc3339(),
        device: GitHubSyncDevice {
            id: device_id,
            name: local_device_name(),
        },
        shard,
        daily_schema: GITHUB_SYNC_AGGREGATE_DAILY_SCHEMA
            .iter()
            .map(ToString::to_string)
            .collect(),
        daily_rows,
        dimension_schema: GITHUB_SYNC_AGGREGATE_DIMENSION_SCHEMA
            .iter()
            .map(ToString::to_string)
            .collect(),
        dimension_rows,
        total_tokens,
    })
}

#[allow(dead_code)]
pub async fn import_github_sync_package<P>(
    repository: &TokenScopeRepository,
    package: P,
) -> Result<DevicePackageImportResult, String>
where
    P: Into<GitHubSyncPackageDocument>,
{
    let package = package.into();
    validate_package(&package)?;
    let local_device_id = repository
        .get_or_create_local_device_id()
        .await
        .map_err(|err| err.to_string())?;
    if package.device_id() == local_device_id {
        return Err("不能导入当前设备自己的 GitHub 同步分片。".to_string());
    }
    let detail_package = match package {
        GitHubSyncPackageDocument::Aggregate(package) => {
            return import_github_sync_aggregate_package(repository, package).await;
        }
        package => package,
    };

    let device_id = detail_package.device_id().to_string();
    let device_name = detail_package.device_name().to_string();
    let package_version = detail_package.version();
    let shard_kind = detail_package.shard_kind().to_string();
    let shard_date = detail_package.shard_date().map(ToString::to_string);
    let dataset_id = dataset_id_for_device(&device_id);
    let now = Local::now().to_rfc3339();
    let mut skipped = 0;
    let mut import_calls = Vec::new();
    for call in detail_package.into_import_calls(&dataset_id) {
        match call {
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
        device_id,
        device_name,
        package_version,
        source_path: Some(format!("github-sync:{shard_kind}")),
        imported_at: now.clone(),
        updated_at: now,
        calls,
        total_tokens,
        estimated_cost_usd,
        cost_currency,
        sync_data_mode: GITHUB_SYNC_DATA_MODE_DETAIL_V2.to_string(),
    };

    let dataset = match shard_kind.as_str() {
        "bootstrap" => repository
            .replace_external_dataset(&input, &import_calls)
            .await
            .map_err(|err| err.to_string())?,
        "day" => {
            let date = shard_date
                .as_deref()
                .ok_or_else(|| "GitHub day 分片缺少日期".to_string())?;
            repository
                .replace_external_dataset_date(&input, date, &import_calls)
                .await
                .map_err(|err| err.to_string())?
        }
        _ => return Err(format!("不支持的 GitHub 同步分片：{shard_kind}")),
    };

    Ok(DevicePackageImportResult {
        dataset,
        imported: calls,
        skipped,
    })
}

async fn import_github_sync_aggregate_package(
    repository: &TokenScopeRepository,
    package: GitHubSyncAggregatePackage,
) -> Result<DevicePackageImportResult, String> {
    let device_id = package.device.id.clone();
    let device_name = package.device.name.clone();
    let package_version = package.version;
    let shard_kind = package.shard.kind.clone();
    let shard_date = package.shard.date.clone();
    let dataset_id = dataset_id_for_device(&device_id);
    let now = Local::now().to_rfc3339();
    let mut skipped = 0;
    let mut daily_rows = Vec::new();
    for row in package.daily_rows {
        match row.into_input() {
            Some(row) => daily_rows.push(row),
            None => skipped += 1,
        }
    }
    let mut dimension_rows = Vec::new();
    for row in package.dimension_rows {
        match row.into_input() {
            Some(row) => dimension_rows.push(row),
            None => skipped += 1,
        }
    }

    if let Some(date) = shard_date.as_deref() {
        let before_daily = daily_rows.len();
        daily_rows.retain(|row| row.date_local == date);
        skipped += (before_daily - daily_rows.len()) as i64;
        let before_dimensions = dimension_rows.len();
        dimension_rows.retain(|row| row.date_local == date);
        skipped += (before_dimensions - dimension_rows.len()) as i64;
    }

    let calls = daily_rows.iter().map(|row| row.calls).sum::<i64>();
    let total_tokens = daily_rows.iter().map(|row| row.total_tokens).sum::<i64>();
    let estimated_cost_usd = daily_rows
        .iter()
        .map(|row| row.estimated_cost_usd)
        .sum::<f64>();
    let cost_currency =
        summarize_cost_currency(daily_rows.iter().map(|row| row.cost_currency.as_str()));
    let input = ExternalDatasetInput {
        id: dataset_id,
        device_id,
        device_name,
        package_version,
        source_path: Some(format!("github-sync:{shard_kind}")),
        imported_at: now.clone(),
        updated_at: now,
        calls,
        total_tokens,
        estimated_cost_usd,
        cost_currency,
        sync_data_mode: GITHUB_SYNC_DATA_MODE_AGGREGATE_V3.to_string(),
    };

    let dataset = match shard_kind.as_str() {
        "bootstrap" => repository
            .replace_external_aggregate_dataset(&input, &daily_rows, &dimension_rows)
            .await
            .map_err(|err| err.to_string())?,
        "day" => {
            let date = shard_date
                .as_deref()
                .ok_or_else(|| "GitHub day 分片缺少日期".to_string())?;
            repository
                .replace_external_aggregate_dataset_date(&input, date, &daily_rows, &dimension_rows)
                .await
                .map_err(|err| err.to_string())?
        }
        _ => return Err(format!("不支持的 GitHub 同步分片：{shard_kind}")),
    };

    Ok(DevicePackageImportResult {
        dataset,
        imported: calls,
        skipped,
    })
}

#[allow(dead_code)]
pub fn serialize_github_sync_package<T: Serialize>(package: &T) -> Result<Vec<u8>, String> {
    serde_json::to_vec(package).map_err(|err| format!("GitHub 同步分片序列化失败：{err}"))
}

#[allow(dead_code)]
pub fn decode_github_sync_package(bytes: &[u8]) -> Result<GitHubSyncPackageDocument, String> {
    let header = serde_json::from_slice::<GitHubSyncPackageHeader>(bytes)
        .map_err(|err| format!("GitHub 同步分片解析失败：{err}"))?;
    match header.version {
        GITHUB_SYNC_LEGACY_PACKAGE_VERSION => serde_json::from_slice::<GitHubSyncPackage>(bytes)
            .map(GitHubSyncPackageDocument::Legacy)
            .map_err(|err| format!("GitHub v1 同步分片解析失败：{err}")),
        GITHUB_SYNC_PACKAGE_VERSION => serde_json::from_slice::<GitHubSyncCompactPackage>(bytes)
            .map(GitHubSyncPackageDocument::Compact)
            .map_err(|err| format!("GitHub v2 同步分片解析失败：{err}")),
        GITHUB_SYNC_AGGREGATE_PACKAGE_VERSION => {
            serde_json::from_slice::<GitHubSyncAggregatePackage>(bytes)
                .map(GitHubSyncPackageDocument::Aggregate)
                .map_err(|err| format!("GitHub v3 同步分片解析失败：{err}"))
        }
        _ => Err(format!("不支持的 GitHub 同步分片版本：{}", header.version)),
    }
}

#[allow(dead_code)]
fn validate_package(package: &GitHubSyncPackageDocument) -> Result<(), String> {
    if package.package_type() != GITHUB_SYNC_PACKAGE_TYPE {
        return Err("不是有效的 GitHub 同步分片。".to_string());
    }
    if package.version() != GITHUB_SYNC_LEGACY_PACKAGE_VERSION
        && package.version() != GITHUB_SYNC_PACKAGE_VERSION
        && package.version() != GITHUB_SYNC_AGGREGATE_PACKAGE_VERSION
    {
        return Err(format!(
            "不支持的 GitHub 同步分片版本：{}",
            package.version()
        ));
    }
    if let GitHubSyncPackageDocument::Compact(compact) = package {
        let expected_schema = GITHUB_SYNC_COMPACT_CALL_SCHEMA
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        if compact.schema != expected_schema {
            return Err("GitHub v2 同步分片 schema 不匹配。".to_string());
        }
    }
    if let GitHubSyncPackageDocument::Aggregate(aggregate) = package {
        let expected_daily_schema = GITHUB_SYNC_AGGREGATE_DAILY_SCHEMA
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        if aggregate.daily_schema != expected_daily_schema {
            return Err("GitHub v3 同步分片 daily schema 不匹配。".to_string());
        }
        let expected_dimension_schema = GITHUB_SYNC_AGGREGATE_DIMENSION_SCHEMA
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        if aggregate.dimension_schema != expected_dimension_schema {
            return Err("GitHub v3 同步分片 dimension schema 不匹配。".to_string());
        }
    }
    if package.device_id().trim().is_empty() {
        return Err("GitHub 同步分片缺少设备 ID。".to_string());
    }
    match package.shard_kind() {
        "bootstrap" => Ok(()),
        "day" if package.shard_date().unwrap_or("").trim().is_empty() => {
            Err("GitHub day 分片缺少日期。".to_string())
        }
        "day" => Ok(()),
        _ => Err(format!(
            "不支持的 GitHub 同步分片：{}",
            package.shard_kind()
        )),
    }
}

#[derive(Default)]
struct UsageAggregate {
    calls: i64,
    success_calls: i64,
    error_calls: i64,
    input_tokens: i64,
    output_tokens: i64,
    cached_input_tokens: i64,
    cache_write_input_tokens: i64,
    reasoning_output_tokens: i64,
    total_tokens: i64,
    estimated_cost_usd: f64,
    latency_sum_ms: i64,
    latency_count: i64,
    cost_currencies: Vec<String>,
}

impl UsageAggregate {
    fn add_call(&mut self, call: &DevicePackageCall) {
        self.calls += 1;
        if call.status == "success" {
            self.success_calls += 1;
        }
        if call.status == "error" {
            self.error_calls += 1;
        }
        self.input_tokens += call.input_tokens;
        self.output_tokens += call.output_tokens;
        self.cached_input_tokens += call.cached_input_tokens;
        self.cache_write_input_tokens += call.cache_write_input_tokens;
        self.reasoning_output_tokens += call.reasoning_output_tokens;
        self.total_tokens += call.total_tokens;
        self.estimated_cost_usd += call.estimated_cost_usd;
        if let Some(latency_ms) = call.latency_ms {
            self.latency_sum_ms += latency_ms;
            self.latency_count += 1;
        }
        self.cost_currencies.push(call.cost_currency.clone());
    }

    fn cost_currency(&self) -> String {
        summarize_cost_currency(self.cost_currencies.iter().map(String::as_str))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct DimensionKey {
    date_local: String,
    dimension_type: String,
    dimension_value: String,
    dimension_label: Option<String>,
}

fn aggregate_calls(
    calls: Vec<DevicePackageCall>,
) -> (
    Vec<GitHubSyncAggregateDailyRow>,
    Vec<GitHubSyncAggregateDimensionRow>,
) {
    let mut daily = BTreeMap::<String, UsageAggregate>::new();
    let mut dimensions = BTreeMap::<DimensionKey, UsageAggregate>::new();

    for call in calls {
        daily
            .entry(call.date_local.clone())
            .or_default()
            .add_call(&call);
        for key in dimension_keys_for_call(&call) {
            dimensions.entry(key).or_default().add_call(&call);
        }
    }

    let daily_rows = daily
        .into_iter()
        .map(|(date_local, aggregate)| {
            GitHubSyncAggregateDailyRow::from_aggregate(date_local, aggregate)
        })
        .collect();
    let dimension_rows = dimensions
        .into_iter()
        .map(|(key, aggregate)| GitHubSyncAggregateDimensionRow::from_aggregate(key, aggregate))
        .collect();

    (daily_rows, dimension_rows)
}

fn dimension_keys_for_call(call: &DevicePackageCall) -> Vec<DimensionKey> {
    let mut keys = Vec::new();
    push_dimension_key(
        &mut keys,
        &call.date_local,
        "provider",
        Some(call.provider.as_str()),
        Some(call.provider.as_str()),
    );
    push_dimension_key(
        &mut keys,
        &call.date_local,
        "model",
        call.model_response
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| call.model_requested.as_deref()),
        call.model_response
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| call.model_requested.as_deref()),
    );
    push_dimension_key(
        &mut keys,
        &call.date_local,
        "agent",
        call.agent_id.as_deref(),
        call.agent_name.as_deref().or(call.agent_id.as_deref()),
    );
    push_dimension_key(
        &mut keys,
        &call.date_local,
        "workflow",
        call.workflow_id.as_deref(),
        call.workflow_id.as_deref(),
    );
    push_dimension_key(
        &mut keys,
        &call.date_local,
        "project",
        call.project_id.as_deref(),
        call.project_id.as_deref(),
    );
    push_dimension_key(
        &mut keys,
        &call.date_local,
        "session",
        call.session_id.as_deref(),
        call.session_id.as_deref(),
    );
    keys
}

fn push_dimension_key(
    keys: &mut Vec<DimensionKey>,
    date_local: &str,
    dimension_type: &str,
    dimension_value: Option<&str>,
    dimension_label: Option<&str>,
) {
    let Some(value) = dimension_value
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return;
    };
    keys.push(DimensionKey {
        date_local: date_local.to_string(),
        dimension_type: dimension_type.to_string(),
        dimension_value: value.to_string(),
        dimension_label: dimension_label
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string),
    });
}

impl GitHubSyncCompactCall {
    pub fn from_device_call(call: DevicePackageCall) -> Self {
        Self(
            call.source_key,
            call.external_id,
            call.started_at,
            call.ended_at,
            call.date_local,
            call.provider,
            call.model_requested,
            call.model_response,
            call.agent_id,
            call.agent_name,
            call.workflow_id,
            call.workflow_step,
            call.session_id,
            call.project_id,
            call.input_tokens,
            call.output_tokens,
            call.cached_input_tokens,
            call.cache_write_input_tokens,
            call.reasoning_output_tokens,
            call.audio_input_tokens,
            call.audio_output_tokens,
            call.image_input_tokens,
            call.image_output_tokens,
            call.total_tokens,
            call.total_billable_tokens,
            call.request_count,
            call.tool_call_count,
            call.retry_count,
            call.latency_ms,
            call.http_status,
            call.status,
            call.error_type,
            call.estimated_cost_usd,
            call.cost_currency,
            call.created_at,
        )
    }

    pub fn into_device_call(self) -> Option<DevicePackageCall> {
        let Self(
            source_key,
            external_id,
            started_at,
            ended_at,
            date_local,
            provider,
            model_requested,
            model_response,
            agent_id,
            agent_name,
            workflow_id,
            workflow_step,
            session_id,
            project_id,
            input_tokens,
            output_tokens,
            cached_input_tokens,
            cache_write_input_tokens,
            reasoning_output_tokens,
            audio_input_tokens,
            audio_output_tokens,
            image_input_tokens,
            image_output_tokens,
            total_tokens,
            total_billable_tokens,
            request_count,
            tool_call_count,
            retry_count,
            latency_ms,
            http_status,
            status,
            error_type,
            estimated_cost_usd,
            cost_currency,
            created_at,
        ) = self;

        if source_key.trim().is_empty()
            || external_id.trim().is_empty()
            || started_at.trim().is_empty()
            || date_local.trim().is_empty()
            || provider.trim().is_empty()
        {
            return None;
        }

        Some(DevicePackageCall {
            source_key,
            external_id: external_id.clone(),
            id: external_id,
            started_at,
            ended_at,
            date_local,
            provider,
            provider_config_id: None,
            api_type: None,
            model_requested,
            model_response,
            agent_id,
            agent_name,
            agent_run_id: None,
            workflow_id,
            workflow_step,
            session_id,
            trace_id: None,
            span_id: None,
            parent_span_id: None,
            project_id,
            user_id: None,
            environment: None,
            feature: None,
            input_tokens,
            output_tokens,
            cached_input_tokens,
            cache_write_input_tokens,
            reasoning_output_tokens,
            audio_input_tokens,
            audio_output_tokens,
            image_input_tokens,
            image_output_tokens,
            total_tokens,
            total_billable_tokens,
            request_count,
            tool_call_count,
            retry_count,
            latency_ms,
            http_status,
            status,
            error_type,
            error_message: None,
            estimated_cost_usd,
            cost_currency,
            provider_reported_cost_usd: None,
            reconciled_cost_usd: None,
            cost_source: None,
            usage_source: None,
            request_hash: None,
            response_hash: None,
            prompt_template_id: None,
            created_at,
        })
    }

    fn into_import_call(self, dataset_id: &str) -> Option<ExternalDatasetImportCall> {
        let call = self.into_device_call()?;
        call.into_import_call(dataset_id)
    }

    pub fn date_local(&self) -> &str {
        &self.4
    }
}

impl GitHubSyncAggregateDailyRow {
    fn from_aggregate(date_local: String, aggregate: UsageAggregate) -> Self {
        Self(
            date_local,
            aggregate.calls,
            aggregate.success_calls,
            aggregate.error_calls,
            aggregate.input_tokens,
            aggregate.output_tokens,
            aggregate.cached_input_tokens,
            aggregate.cache_write_input_tokens,
            aggregate.reasoning_output_tokens,
            aggregate.total_tokens,
            aggregate.estimated_cost_usd,
            aggregate.cost_currency(),
            aggregate.latency_sum_ms,
            aggregate.latency_count,
        )
    }

    fn into_input(self) -> Option<ExternalUsageAggregateInput> {
        let Self(
            date_local,
            calls,
            success_calls,
            error_calls,
            input_tokens,
            output_tokens,
            cached_input_tokens,
            cache_write_input_tokens,
            reasoning_output_tokens,
            total_tokens,
            estimated_cost_usd,
            cost_currency,
            latency_sum_ms,
            latency_count,
        ) = self;
        if date_local.trim().is_empty() {
            return None;
        }

        Some(ExternalUsageAggregateInput {
            date_local,
            calls: calls.max(0),
            success_calls: success_calls.max(0),
            error_calls: error_calls.max(0),
            input_tokens: input_tokens.max(0),
            output_tokens: output_tokens.max(0),
            cached_input_tokens: cached_input_tokens.max(0),
            cache_write_input_tokens: cache_write_input_tokens.max(0),
            reasoning_output_tokens: reasoning_output_tokens.max(0),
            total_tokens: total_tokens.max(0),
            estimated_cost_usd,
            cost_currency,
            latency_sum_ms: latency_sum_ms.max(0),
            latency_count: latency_count.max(0),
        })
    }
}

impl GitHubSyncAggregateDimensionRow {
    fn from_aggregate(key: DimensionKey, aggregate: UsageAggregate) -> Self {
        Self(
            key.date_local,
            key.dimension_type,
            key.dimension_value,
            key.dimension_label,
            aggregate.calls,
            aggregate.success_calls,
            aggregate.error_calls,
            aggregate.input_tokens,
            aggregate.output_tokens,
            aggregate.cached_input_tokens,
            aggregate.cache_write_input_tokens,
            aggregate.reasoning_output_tokens,
            aggregate.total_tokens,
            aggregate.estimated_cost_usd,
            aggregate.cost_currency(),
            aggregate.latency_sum_ms,
            aggregate.latency_count,
        )
    }

    fn into_input(self) -> Option<ExternalDimensionUsageAggregateInput> {
        let Self(
            date_local,
            dimension_type,
            dimension_value,
            dimension_label,
            calls,
            success_calls,
            error_calls,
            input_tokens,
            output_tokens,
            cached_input_tokens,
            cache_write_input_tokens,
            reasoning_output_tokens,
            total_tokens,
            estimated_cost_usd,
            cost_currency,
            latency_sum_ms,
            latency_count,
        ) = self;
        if date_local.trim().is_empty()
            || dimension_type.trim().is_empty()
            || dimension_value.trim().is_empty()
        {
            return None;
        }

        Some(ExternalDimensionUsageAggregateInput {
            date_local,
            dimension_type,
            dimension_value,
            dimension_label,
            calls: calls.max(0),
            success_calls: success_calls.max(0),
            error_calls: error_calls.max(0),
            input_tokens: input_tokens.max(0),
            output_tokens: output_tokens.max(0),
            cached_input_tokens: cached_input_tokens.max(0),
            cache_write_input_tokens: cache_write_input_tokens.max(0),
            reasoning_output_tokens: reasoning_output_tokens.max(0),
            total_tokens: total_tokens.max(0),
            estimated_cost_usd,
            cost_currency,
            latency_sum_ms: latency_sum_ms.max(0),
            latency_count: latency_count.max(0),
        })
    }
}

impl GitHubSyncPackageDocument {
    pub fn package_type(&self) -> &str {
        match self {
            Self::Legacy(package) => &package.package_type,
            Self::Compact(package) => &package.package_type,
            Self::Aggregate(package) => &package.package_type,
        }
    }

    pub fn version(&self) -> i64 {
        match self {
            Self::Legacy(package) => package.version,
            Self::Compact(package) => package.version,
            Self::Aggregate(package) => package.version,
        }
    }

    pub fn device_id(&self) -> &str {
        match self {
            Self::Legacy(package) => &package.device.id,
            Self::Compact(package) => &package.device.id,
            Self::Aggregate(package) => &package.device.id,
        }
    }

    fn device_name(&self) -> &str {
        match self {
            Self::Legacy(package) => &package.device.name,
            Self::Compact(package) => &package.device.name,
            Self::Aggregate(package) => &package.device.name,
        }
    }

    pub fn shard_kind(&self) -> &str {
        match self {
            Self::Legacy(package) => &package.shard.kind,
            Self::Compact(package) => &package.shard.kind,
            Self::Aggregate(package) => &package.shard.kind,
        }
    }

    pub fn shard_date(&self) -> Option<&str> {
        match self {
            Self::Legacy(package) => package.shard.date.as_deref(),
            Self::Compact(package) => package.shard.date.as_deref(),
            Self::Aggregate(package) => package.shard.date.as_deref(),
        }
    }

    pub fn data_mode(&self) -> &'static str {
        match self {
            Self::Legacy(_) | Self::Compact(_) => GITHUB_SYNC_DATA_MODE_DETAIL_V2,
            Self::Aggregate(_) => GITHUB_SYNC_DATA_MODE_AGGREGATE_V3,
        }
    }

    fn into_import_calls(self, dataset_id: &str) -> Vec<Option<ExternalDatasetImportCall>> {
        match self {
            Self::Legacy(package) => package
                .calls
                .into_iter()
                .map(|call| call.into_import_call(dataset_id))
                .collect(),
            Self::Compact(package) => package
                .rows
                .into_iter()
                .map(|row| row.into_import_call(dataset_id))
                .collect(),
            Self::Aggregate(_) => Vec::new(),
        }
    }
}

impl From<GitHubSyncPackage> for GitHubSyncPackageDocument {
    fn from(value: GitHubSyncPackage) -> Self {
        Self::Legacy(value)
    }
}

impl From<GitHubSyncCompactPackage> for GitHubSyncPackageDocument {
    fn from(value: GitHubSyncCompactPackage) -> Self {
        Self::Compact(value)
    }
}

impl From<GitHubSyncAggregatePackage> for GitHubSyncPackageDocument {
    fn from(value: GitHubSyncAggregatePackage) -> Self {
        Self::Aggregate(value)
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
        assert_eq!(package.rows.len(), 2);
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
        assert_eq!(package.rows.len(), 1);
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

    #[tokio::test]
    async fn github_sync_exports_compact_v2_rows_smaller_than_legacy_json() {
        let repository = TokenScopeRepository::connect_in_memory().await.unwrap();
        repository.migrate().await.unwrap();
        insert_test_call(&repository, "compact-a", "2026-06-05", 200).await;
        insert_test_call(&repository, "compact-b", "2026-06-05", 350).await;

        let compact = export_github_sync_package(
            &repository,
            GitHubSyncShardSelector::Day("2026-06-05".to_string()),
        )
        .await
        .expect("compact day package exports");
        let legacy = legacy_package_from_compact(&compact);
        let compact_json = serialize_github_sync_package(&compact).expect("compact serializes");
        let legacy_json = serde_json::to_vec(&legacy).expect("legacy serializes");

        assert_eq!(compact.version, 2);
        assert_eq!(
            compact.schema,
            GITHUB_SYNC_COMPACT_CALL_SCHEMA
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        );
        assert_eq!(compact.rows.len(), 2);
        assert!(
            compact_json.len() < legacy_json.len(),
            "compact v2 json should be smaller than legacy v1 json: compact={} legacy={}",
            compact_json.len(),
            legacy_json.len()
        );
    }

    #[tokio::test]
    async fn github_sync_imports_compact_v2_rows_with_summary_dimensions() {
        let repository = TokenScopeRepository::connect_in_memory().await.unwrap();
        repository.migrate().await.unwrap();

        let compact = compact_package_for("remote-compact", "2026-06-05", 450);
        let result = import_github_sync_package(&repository, compact)
            .await
            .expect("compact package imports");
        let row = query(
            r#"
      SELECT
        provider,
        model_response,
        agent_name,
        project_id,
        session_id,
        workflow_id,
        total_tokens,
        estimated_cost_usd,
        status,
        origin_dataset_id
      FROM llm_call
      WHERE origin_dataset_id = 'device-remote-compact'
      LIMIT 1
      "#,
        )
        .fetch_one(repository.pool())
        .await
        .expect("imported row exists");

        assert_eq!(result.imported, 1);
        assert_eq!(row.get::<String, _>("provider"), "codex");
        assert_eq!(row.get::<String, _>("model_response"), "gpt-5");
        assert_eq!(row.get::<String, _>("agent_name"), "Codex");
        assert_eq!(row.get::<String, _>("project_id"), "tokenscope");
        assert_eq!(row.get::<String, _>("session_id"), "session-compact");
        assert_eq!(row.get::<String, _>("workflow_id"), "sync-test");
        assert_eq!(row.get::<i64, _>("total_tokens"), 450);
        assert_eq!(row.get::<f64, _>("estimated_cost_usd"), 0.045);
        assert_eq!(row.get::<String, _>("status"), "success");
        assert_eq!(
            row.get::<String, _>("origin_dataset_id"),
            "device-remote-compact"
        );
    }

    #[tokio::test]
    async fn github_sync_exports_aggregate_v3_without_call_details() {
        let repository = TokenScopeRepository::connect_in_memory().await.unwrap();
        repository.migrate().await.unwrap();
        insert_test_call(&repository, "aggregate-a", "2026-06-05", 200).await;
        insert_test_call(&repository, "aggregate-b", "2026-06-05", 350).await;

        let aggregate = export_github_sync_aggregate_package(
            &repository,
            GitHubSyncShardSelector::Day("2026-06-05".to_string()),
        )
        .await
        .expect("aggregate day package exports");
        let aggregate_json =
            serialize_github_sync_package(&aggregate).expect("aggregate serializes");
        let aggregate_text =
            String::from_utf8(aggregate_json.clone()).expect("aggregate json is utf8");
        let decoded = decode_github_sync_package(&aggregate_json).expect("aggregate decodes");

        assert_eq!(aggregate.version, 3);
        assert_eq!(aggregate.daily_rows.len(), 1);
        assert_eq!(aggregate.daily_rows[0].1, 2);
        assert_eq!(aggregate.daily_rows[0].9, 550);
        assert!(aggregate
            .dimension_rows
            .iter()
            .any(|row| row.1 == "provider" && row.2 == "codex"));
        assert_eq!(decoded.data_mode(), GITHUB_SYNC_DATA_MODE_AGGREGATE_V3);
        assert!(!aggregate_text.contains("external_id"));
        assert!(!aggregate_text.contains("aggregate-a"));
        assert!(!aggregate_text.contains("aggregate-b"));
    }

    #[tokio::test]
    async fn github_sync_imports_v2_and_v3_as_mutually_exclusive_dataset_modes() {
        let repository = TokenScopeRepository::connect_in_memory().await.unwrap();
        repository.migrate().await.unwrap();

        import_github_sync_package(
            &repository,
            compact_package_for("remote-switch", "2026-06-05", 200),
        )
        .await
        .expect("compact package imports");
        assert_eq!(
            sum_imported_tokens_for_date(&repository, "device-remote-switch", "2026-06-05").await,
            200
        );

        let aggregate_result = import_github_sync_package(
            &repository,
            aggregate_package_for("remote-switch", "2026-06-05", 450),
        )
        .await
        .expect("aggregate package imports");
        let detail_rows_after_v3 =
            count_imported_rows_for_date(&repository, "device-remote-switch", "2026-06-05").await;
        let aggregate_tokens_after_v3: i64 = query(
            r#"
      SELECT COALESCE(SUM(total_tokens), 0) AS total
      FROM external_daily_usage
      WHERE dataset_id = 'device-remote-switch'
        AND date_local = '2026-06-05'
      "#,
        )
        .fetch_one(repository.pool())
        .await
        .expect("aggregate total reads")
        .get("total");

        assert_eq!(aggregate_result.imported, 1);
        assert_eq!(
            aggregate_result.dataset.sync_data_mode,
            GITHUB_SYNC_DATA_MODE_AGGREGATE_V3
        );
        assert_eq!(detail_rows_after_v3, 0);
        assert_eq!(aggregate_tokens_after_v3, 450);

        let detail_result = import_github_sync_package(
            &repository,
            compact_package_for("remote-switch", "2026-06-05", 300),
        )
        .await
        .expect("compact package reimports");
        let aggregate_rows_after_v2: i64 = query(
            "SELECT COUNT(*) AS count FROM external_daily_usage WHERE dataset_id = 'device-remote-switch'",
        )
        .fetch_one(repository.pool())
        .await
        .expect("aggregate count reads")
        .get("count");

        assert_eq!(detail_result.imported, 1);
        assert_eq!(
            detail_result.dataset.sync_data_mode,
            GITHUB_SYNC_DATA_MODE_DETAIL_V2
        );
        assert_eq!(aggregate_rows_after_v2, 0);
        assert_eq!(
            sum_imported_tokens_for_date(&repository, "device-remote-switch", "2026-06-05").await,
            300
        );
    }

    #[test]
    fn github_sync_decodes_legacy_v1_packages_for_backward_compatibility() {
        let legacy = package_for("remote-legacy", "2026-06-05", 120);
        let json = serde_json::to_vec(&legacy).expect("legacy serializes");
        let decoded = decode_github_sync_package(&json).expect("legacy decodes");

        assert_eq!(decoded.device_id(), "remote-legacy");
        assert_eq!(decoded.shard_kind(), "day");
        assert_eq!(decoded.shard_date(), Some("2026-06-05"));
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

    fn compact_package_for(
        device_id: &str,
        date_local: &str,
        total_tokens: i64,
    ) -> GitHubSyncCompactPackage {
        GitHubSyncCompactPackage {
            package_type: "tokenscope.github_sync".to_string(),
            version: 2,
            exported_at: "2026-06-05T10:00:00+08:00".to_string(),
            device: GitHubSyncDevice {
                id: device_id.to_string(),
                name: device_id.to_string(),
            },
            shard: GitHubSyncShard {
                kind: "day".to_string(),
                date: Some(date_local.to_string()),
            },
            schema: GITHUB_SYNC_COMPACT_CALL_SCHEMA
                .iter()
                .map(ToString::to_string)
                .collect(),
            rows: vec![GitHubSyncCompactCall::from_device_call(DevicePackageCall {
                source_key: "github-sync-test".to_string(),
                external_id: format!("{device_id}-{date_local}-{total_tokens}"),
                id: format!("{device_id}-{date_local}-{total_tokens}"),
                started_at: format!("{date_local}T10:00:00+08:00"),
                ended_at: None,
                date_local: date_local.to_string(),
                provider: "codex".to_string(),
                provider_config_id: Some("openai".to_string()),
                api_type: Some("codex_thread_import".to_string()),
                model_requested: Some("gpt-5".to_string()),
                model_response: Some("gpt-5".to_string()),
                agent_id: Some("codex".to_string()),
                agent_name: Some("Codex".to_string()),
                agent_run_id: Some("run-compact".to_string()),
                workflow_id: Some("sync-test".to_string()),
                workflow_step: Some("summary".to_string()),
                session_id: Some("session-compact".to_string()),
                trace_id: Some("trace-omitted".to_string()),
                span_id: Some("span-omitted".to_string()),
                parent_span_id: Some("parent-omitted".to_string()),
                project_id: Some("tokenscope".to_string()),
                user_id: Some("user-omitted".to_string()),
                environment: Some("local".to_string()),
                feature: Some("github_sync".to_string()),
                input_tokens: 100,
                output_tokens: total_tokens - 100,
                cached_input_tokens: 20,
                cache_write_input_tokens: 0,
                reasoning_output_tokens: 30,
                audio_input_tokens: 0,
                audio_output_tokens: 0,
                image_input_tokens: 0,
                image_output_tokens: 0,
                total_tokens,
                total_billable_tokens: total_tokens,
                request_count: 1,
                tool_call_count: 2,
                retry_count: 0,
                latency_ms: Some(1234),
                http_status: Some(200),
                status: "success".to_string(),
                error_type: None,
                error_message: Some("omitted from compact sync".to_string()),
                estimated_cost_usd: 0.045,
                cost_currency: "USD".to_string(),
                provider_reported_cost_usd: Some(0.045),
                reconciled_cost_usd: Some(0.045),
                cost_source: Some("pricing-rule".to_string()),
                usage_source: Some("importer".to_string()),
                request_hash: Some("request-hash-omitted".to_string()),
                response_hash: Some("response-hash-omitted".to_string()),
                prompt_template_id: Some("template-omitted".to_string()),
                created_at: format!("{date_local}T10:00:00+08:00"),
            })],
            total_tokens,
        }
    }

    fn aggregate_package_for(
        device_id: &str,
        date_local: &str,
        total_tokens: i64,
    ) -> GitHubSyncAggregatePackage {
        GitHubSyncAggregatePackage {
            package_type: "tokenscope.github_sync".to_string(),
            version: 3,
            exported_at: "2026-06-05T10:00:00+08:00".to_string(),
            device: GitHubSyncDevice {
                id: device_id.to_string(),
                name: device_id.to_string(),
            },
            shard: GitHubSyncShard {
                kind: "day".to_string(),
                date: Some(date_local.to_string()),
            },
            daily_schema: GITHUB_SYNC_AGGREGATE_DAILY_SCHEMA
                .iter()
                .map(ToString::to_string)
                .collect(),
            daily_rows: vec![GitHubSyncAggregateDailyRow(
                date_local.to_string(),
                1,
                1,
                0,
                100,
                total_tokens - 100,
                20,
                0,
                30,
                total_tokens,
                0.045,
                "USD".to_string(),
                1234,
                1,
            )],
            dimension_schema: GITHUB_SYNC_AGGREGATE_DIMENSION_SCHEMA
                .iter()
                .map(ToString::to_string)
                .collect(),
            dimension_rows: vec![GitHubSyncAggregateDimensionRow(
                date_local.to_string(),
                "provider".to_string(),
                "codex".to_string(),
                Some("codex".to_string()),
                1,
                1,
                0,
                100,
                total_tokens - 100,
                20,
                0,
                30,
                total_tokens,
                0.045,
                "USD".to_string(),
                1234,
                1,
            )],
            total_tokens,
        }
    }

    fn legacy_package_from_compact(compact: &GitHubSyncCompactPackage) -> GitHubSyncPackage {
        GitHubSyncPackage {
            package_type: compact.package_type.clone(),
            version: 1,
            exported_at: compact.exported_at.clone(),
            device: compact.device.clone(),
            shard: compact.shard.clone(),
            calls: compact
                .rows
                .clone()
                .into_iter()
                .filter_map(|row| row.into_device_call())
                .collect(),
            total_tokens: compact.total_tokens,
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
