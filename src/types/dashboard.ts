export type DashboardRange = "today" | "7d" | "30d" | "90d";
export type DimensionKind = "agent" | "model" | "provider" | "workflow" | "project" | "session";

export interface CodexImportResult {
  imported: number;
  skipped: number;
  source_path: string;
}

export interface LocalAgentStatus {
  id: string;
  name: string;
  detected: boolean;
  import_supported: boolean;
  source_path: string | null;
  message: string;
}

export interface AgentImportResult extends LocalAgentStatus {
  imported: number;
  skipped: number;
  status: string;
  error: string | null;
}

export interface AgentSourceSummary extends LocalAgentStatus {
  imported_calls: number;
  total_tokens: number;
  estimated_cost_usd: number;
  cost_currency: string;
  last_imported_at: string | null;
  last_call_at: string | null;
}

export interface ExternalDataset {
  id: string;
  device_id: string;
  device_name: string;
  package_version: number;
  source_path: string | null;
  imported_at: string;
  updated_at: string;
  calls: number;
  total_tokens: number;
  estimated_cost_usd: number;
  cost_currency: string;
}

export interface DevicePackageImportResult {
  dataset: ExternalDataset;
  imported: number;
  skipped: number;
}

export interface SyncSettingsInput {
  enabled: boolean;
  interval_minutes: number;
  sync_on_startup: boolean;
}

export interface SyncSettings extends SyncSettingsInput {
  last_sync_at: string | null;
  next_sync_at: string | null;
  last_result: string | null;
  last_error: string | null;
}

export type AppUpdateStatus =
  | "idle"
  | "checking"
  | "current"
  | "available"
  | "downloading"
  | "installing"
  | "error"
  | "browser-preview";

export interface AppUpdateInfo {
  available: boolean;
  current_version: string | null;
  version: string | null;
  date: string | null;
  body: string | null;
  status: AppUpdateStatus;
  checked_at: string | null;
  error: string | null;
}

export interface AppUpdateProgress {
  event: "Started" | "Progress" | "Finished";
  downloaded_bytes: number;
  content_length: number | null;
}

export interface DashboardSummary {
  total_tokens: number;
  input_tokens: number;
  output_tokens: number;
  cached_input_tokens: number;
  reasoning_output_tokens: number;
  estimated_cost_usd: number;
  cost_currency: string;
  calls: number;
  success_calls: number;
  error_calls: number;
  error_rate: number;
  avg_latency_ms: number | null;
  top_agent_id: string | null;
  top_model: string | null;
}

export interface DailyUsagePoint {
  date_local: string;
  dimension: string | null;
  calls: number;
  input_tokens: number;
  output_tokens: number;
  cached_input_tokens: number;
  reasoning_output_tokens: number;
  total_tokens: number;
  estimated_cost_usd: number;
  cost_currency: string;
}

export interface TokenPulseHourlyPoint {
  hour: number;
  total_tokens: number;
}

export interface TokenPulseSnapshot {
  today_local: string;
  today_tokens: number;
  today_calls: number;
  yesterday_tokens: number;
  average_daily_tokens: number;
  history_days: number;
  ratio_to_average: number | null;
  remaining_to_average: number;
  hourly_tokens: TokenPulseHourlyPoint[];
}

export interface TopDimensionRow {
  dimension: string;
  calls: number;
  total_tokens: number;
  estimated_cost_usd: number;
  cost_currency: string;
  avg_latency_ms: number | null;
}

export interface LlmCallRow {
  id: string;
  started_at: string;
  provider: string;
  model_requested: string | null;
  model_response: string | null;
  agent_id: string | null;
  workflow_id: string | null;
  project_id: string | null;
  input_tokens: number;
  output_tokens: number;
  cached_input_tokens: number;
  reasoning_output_tokens: number;
  total_tokens: number;
  estimated_cost_usd: number;
  cost_currency: string;
  latency_ms: number | null;
  status: string;
}

export interface LlmCallFilters {
  from: string | null;
  to: string | null;
  provider: string | null;
  agent_id: string | null;
  workflow_id?: string | null;
  project_id?: string | null;
  session_id?: string | null;
  model: string | null;
  status: string | null;
  limit: number;
  offset: number;
}

export interface LlmCallPage {
  rows: LlmCallRow[];
  total: number;
}

export interface CallFilterOptions {
  providers: string[];
  agents: string[];
  models: string[];
  statuses: string[];
}

export interface DataHealthSummary {
  total_calls: number;
  issue_calls: number;
  issues: DataHealthIssueSummary[];
}

export interface DataHealthIssueSummary {
  issue_type: string;
  calls: number;
}

export interface DataHealthIssueRow {
  call_id: string;
  issue_type: string;
  started_at: string;
  date_local: string;
  provider: string;
  model: string | null;
  agent_id: string | null;
  workflow_id: string | null;
  project_id: string | null;
  session_id: string | null;
  status: string;
  total_tokens: number;
  estimated_cost_usd: number;
  cost_currency: string;
  cost_source: string | null;
}

export interface PricingRule {
  id: string;
  provider: string;
  model: string;
  currency: string;
  input_usd_per_1m: number;
  cached_input_usd_per_1m: number;
  output_usd_per_1m: number;
  reasoning_output_usd_per_1m: number | null;
  effective_from: string;
  effective_to: string | null;
  source: string | null;
}

export interface PricingRuleInput {
  id: string | null;
  provider: string;
  model: string;
  currency: string;
  input_usd_per_1m: number;
  cached_input_usd_per_1m: number;
  output_usd_per_1m: number;
  reasoning_output_usd_per_1m: number | null;
  effective_from: string;
  effective_to: string | null;
  source: string | null;
}

export interface CostRecalculationResult {
  updated: number;
  missing: number;
}

export interface PricingRulePresetSummary {
  id: string;
  name: string;
  description: string;
  source: string;
  source_url: string | null;
  checked_at: string | null;
  pricing_scope: string | null;
  rule_count: number;
}

export interface PricingRuleImportResult {
  imported: number;
  updated: number;
  total: number;
}

export interface UnknownPricingModel {
  provider: string;
  model: string;
  calls: number;
  total_tokens: number;
  estimated_cost_usd: number;
  cost_currency: string;
  first_seen_at: string;
  last_seen_at: string;
}

export interface CustomImporterProfileInput {
  id: string | null;
  name: string;
  enabled: boolean;
  source_key: string;
  database_path: string;
  import_sql: string;
  mappings_json: string;
}

export interface CustomImporterProfile extends CustomImporterProfileInput {
  id: string;
  created_at: string;
  updated_at: string;
  imported_calls: number;
  total_tokens: number;
  estimated_cost_usd: number;
  cost_currency: string;
  last_imported_at: string | null;
  last_call_at: string | null;
  last_run_status: string | null;
  last_run_error: string | null;
}

export interface CustomImporterPreview {
  columns: string[];
  rows: Array<Record<string, unknown>>;
}

export interface CustomImporterRunResult {
  profile_id: string;
  status: string;
  imported: number;
  skipped: number;
  error: string | null;
}
