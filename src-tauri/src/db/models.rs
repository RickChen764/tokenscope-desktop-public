use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardSummary {
    pub total_tokens: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_input_tokens: i64,
    pub reasoning_output_tokens: i64,
    pub estimated_cost_usd: f64,
    pub cost_currency: String,
    pub calls: i64,
    pub success_calls: i64,
    pub error_calls: i64,
    pub error_rate: f64,
    pub avg_latency_ms: Option<f64>,
    pub top_agent_id: Option<String>,
    pub top_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyUsagePoint {
    pub date_local: String,
    pub dimension: Option<String>,
    pub calls: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_input_tokens: i64,
    pub reasoning_output_tokens: i64,
    pub total_tokens: i64,
    pub estimated_cost_usd: f64,
    pub cost_currency: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPulseHourlyPoint {
    pub hour: i64,
    pub total_tokens: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPulseSnapshot {
    pub today_local: String,
    pub today_tokens: i64,
    pub today_calls: i64,
    pub yesterday_tokens: i64,
    pub average_daily_tokens: f64,
    pub history_days: i64,
    pub ratio_to_average: Option<f64>,
    pub remaining_to_average: i64,
    pub hourly_tokens: Vec<TokenPulseHourlyPoint>,
}

#[derive(Debug, Clone, Copy)]
pub struct TokenPulseWindowPosition {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopDimensionRow {
    pub dimension: String,
    pub calls: i64,
    pub total_tokens: i64,
    pub estimated_cost_usd: f64,
    pub cost_currency: String,
    pub avg_latency_ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSourceStats {
    pub source_key: String,
    pub imported_calls: i64,
    pub total_tokens: i64,
    pub estimated_cost_usd: f64,
    pub cost_currency: String,
    pub last_imported_at: Option<String>,
    pub last_call_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ExternalDataset {
    pub id: String,
    pub device_id: String,
    pub device_name: String,
    pub package_version: i64,
    pub source_path: Option<String>,
    pub imported_at: String,
    pub updated_at: String,
    pub calls: i64,
    pub total_tokens: i64,
    pub estimated_cost_usd: f64,
    pub cost_currency: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalDatasetInput {
    pub id: String,
    pub device_id: String,
    pub device_name: String,
    pub package_version: i64,
    pub source_path: Option<String>,
    pub imported_at: String,
    pub updated_at: String,
    pub calls: i64,
    pub total_tokens: i64,
    pub estimated_cost_usd: f64,
    pub cost_currency: String,
}

#[derive(Debug, Clone)]
pub struct ExternalDatasetImportCall {
    pub source_key: String,
    pub external_id: String,
    pub call: NewLlmCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevicePackageImportResult {
    pub dataset: ExternalDataset,
    pub imported: i64,
    pub skipped: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataHealthIssueSummary {
    pub issue_type: String,
    pub calls: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataHealthSummary {
    pub total_calls: i64,
    pub issue_calls: i64,
    pub issues: Vec<DataHealthIssueSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataHealthIssueRow {
    pub call_id: String,
    pub issue_type: String,
    pub started_at: String,
    pub date_local: String,
    pub provider: String,
    pub model: Option<String>,
    pub agent_id: Option<String>,
    pub workflow_id: Option<String>,
    pub project_id: Option<String>,
    pub session_id: Option<String>,
    pub status: String,
    pub total_tokens: i64,
    pub estimated_cost_usd: f64,
    pub cost_currency: String,
    pub cost_source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnknownPricingModel {
    pub provider: String,
    pub model: String,
    pub calls: i64,
    pub total_tokens: i64,
    pub estimated_cost_usd: f64,
    pub cost_currency: String,
    pub first_seen_at: String,
    pub last_seen_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmCallRow {
    pub id: String,
    pub started_at: String,
    pub provider: String,
    pub model_requested: Option<String>,
    pub model_response: Option<String>,
    pub agent_id: Option<String>,
    pub workflow_id: Option<String>,
    pub project_id: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_input_tokens: i64,
    pub reasoning_output_tokens: i64,
    pub total_tokens: i64,
    pub estimated_cost_usd: f64,
    pub cost_currency: String,
    pub latency_ms: Option<i64>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmCallFilters {
    pub from: Option<String>,
    pub to: Option<String>,
    pub provider: Option<String>,
    pub agent_id: Option<String>,
    pub model: Option<String>,
    pub status: Option<String>,
    pub workflow_id: Option<String>,
    pub project_id: Option<String>,
    pub session_id: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmCallPage {
    pub rows: Vec<LlmCallRow>,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallFilterOptions {
    pub providers: Vec<String>,
    pub agents: Vec<String>,
    pub models: Vec<String>,
    pub statuses: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub id: String,
    pub provider: String,
    pub display_name: String,
    pub base_url: String,
    pub api_key_redacted: Option<String>,
    pub is_default: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfigInput {
    pub id: Option<String>,
    pub provider: String,
    pub display_name: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub proxy_port: i64,
    pub debug_capture_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettingsInput {
    pub proxy_port: i64,
    pub debug_capture_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncSettings {
    pub enabled: bool,
    pub interval_minutes: i64,
    pub sync_on_startup: bool,
    pub last_sync_at: Option<String>,
    pub next_sync_at: Option<String>,
    pub last_result: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncSettingsInput {
    pub enabled: bool,
    pub interval_minutes: i64,
    pub sync_on_startup: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRunResult {
    pub status: String,
    pub message: String,
    pub imported: i64,
    pub skipped: i64,
    pub started_at: String,
    pub finished_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubSyncSettings {
    pub enabled: bool,
    pub owner: String,
    pub repo: String,
    pub branch: String,
    pub path_prefix: String,
    pub token_configured: bool,
    pub token_redacted: Option<String>,
    pub sync_password_configured: bool,
    pub bootstrap_uploaded: bool,
    pub last_upload_at: Option<String>,
    pub last_import_at: Option<String>,
    pub last_status: Option<String>,
    pub last_message: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct GitHubSyncRemoteDevice {
    pub device_id: String,
    pub device_name: Option<String>,
    pub bootstrap_shards: i64,
    pub day_shards: i64,
    pub last_import_at: Option<String>,
    pub calls: i64,
    pub total_tokens: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubSyncSettingsInput {
    pub enabled: bool,
    pub owner: String,
    pub repo: String,
    pub branch: String,
    pub path_prefix: String,
    pub token: Option<String>,
    pub sync_password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct GitHubSyncShardState {
    pub id: String,
    pub device_id: String,
    pub shard_kind: String,
    pub shard_date: Option<String>,
    pub content_hash: String,
    pub github_path: String,
    pub imported_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubSyncShardStateInput {
    pub device_id: String,
    pub shard_kind: String,
    pub shard_date: Option<String>,
    pub content_hash: String,
    pub github_path: String,
    pub imported_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubSyncConnectionTestResult {
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubSyncRunResult {
    pub status: String,
    pub message: String,
    pub uploaded_shards: i64,
    pub downloaded_shards: i64,
    pub imported: i64,
    pub skipped: i64,
    pub started_at: String,
    pub finished_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CustomImporterProfile {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub source_key: String,
    pub database_path: String,
    pub import_sql: String,
    pub mappings_json: String,
    pub created_at: String,
    pub updated_at: String,
    pub imported_calls: i64,
    pub total_tokens: i64,
    pub estimated_cost_usd: f64,
    pub cost_currency: String,
    pub last_imported_at: Option<String>,
    pub last_call_at: Option<String>,
    pub last_run_status: Option<String>,
    pub last_run_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomImporterProfileInput {
    pub id: Option<String>,
    pub name: String,
    pub enabled: bool,
    pub source_key: String,
    pub database_path: String,
    pub import_sql: String,
    pub mappings_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomImporterPreview {
    pub columns: Vec<String>,
    pub rows: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomImporterRunResult {
    pub profile_id: String,
    pub status: String,
    pub imported: i64,
    pub skipped: i64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomImporterMappings {
    pub external_id: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub date_local: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub model_requested: Option<String>,
    pub model_response: Option<String>,
    pub agent_id: Option<String>,
    pub agent_name: Option<String>,
    pub session_id: Option<String>,
    pub project_id: Option<String>,
    pub workflow_id: Option<String>,
    pub workflow_step: Option<String>,
    pub input_tokens: Option<String>,
    pub output_tokens: Option<String>,
    pub cached_input_tokens: Option<String>,
    pub cache_write_input_tokens: Option<String>,
    pub reasoning_output_tokens: Option<String>,
    pub total_tokens: Option<String>,
    pub estimated_cost_usd: Option<String>,
    pub cost_currency: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewLlmCall {
    pub id: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub date_local: String,
    pub provider: String,
    pub provider_config_id: Option<String>,
    pub api_type: Option<String>,
    pub model_requested: Option<String>,
    pub model_response: Option<String>,
    pub agent_id: Option<String>,
    pub agent_name: Option<String>,
    pub agent_run_id: Option<String>,
    pub workflow_id: Option<String>,
    pub workflow_step: Option<String>,
    pub session_id: Option<String>,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub parent_span_id: Option<String>,
    pub project_id: Option<String>,
    pub user_id: Option<String>,
    pub environment: Option<String>,
    pub feature: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_input_tokens: i64,
    pub cache_write_input_tokens: i64,
    pub reasoning_output_tokens: i64,
    pub audio_input_tokens: i64,
    pub audio_output_tokens: i64,
    pub image_input_tokens: i64,
    pub image_output_tokens: i64,
    pub total_tokens: i64,
    pub total_billable_tokens: i64,
    pub request_count: i64,
    pub tool_call_count: i64,
    pub retry_count: i64,
    pub latency_ms: Option<i64>,
    pub http_status: Option<i64>,
    pub status: String,
    pub error_type: Option<String>,
    pub error_message: Option<String>,
    pub estimated_cost_usd: f64,
    pub cost_currency: String,
    pub provider_reported_cost_usd: Option<f64>,
    pub reconciled_cost_usd: Option<f64>,
    pub cost_source: Option<String>,
    pub usage_source: Option<String>,
    pub raw_usage_json: Option<String>,
    pub raw_response_json: Option<String>,
    pub request_hash: Option<String>,
    pub response_hash: Option<String>,
    pub prompt_template_id: Option<String>,
    pub created_at: String,
}
