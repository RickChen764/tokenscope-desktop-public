import { invoke } from "@tauri-apps/api/core";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";
import type {
  AgentImportResult,
  AgentSourceSummary,
  CallFilterOptions,
  CodexImportResult,
  DashboardRange,
  DashboardSummary,
  DataHealthIssueRow,
  DataHealthSummary,
  DailyUsagePoint,
  DimensionKind,
  LlmCallFilters,
  LlmCallPage,
  LlmCallRow,
  LocalAgentStatus,
  SyncSettings,
  SyncSettingsInput,
  AppUpdateInfo,
  AppUpdateProgress,
  TopDimensionRow,
  CustomImporterPreview,
  CustomImporterProfile,
  CustomImporterProfileInput,
  CustomImporterRunResult,
  DevicePackageImportResult,
  ExternalDataset,
} from "../types/dashboard";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

export type AgentImportMode = "incremental" | "full";

let pendingAppUpdate: Update | null = null;

const emptySummary: DashboardSummary = {
  total_tokens: 0,
  input_tokens: 0,
  output_tokens: 0,
  cached_input_tokens: 0,
  reasoning_output_tokens: 0,
  estimated_cost_usd: 0,
  cost_currency: "USD",
  calls: 0,
  success_calls: 0,
  error_calls: 0,
  error_rate: 0,
  avg_latency_ms: null,
  top_agent_id: null,
  top_model: null,
};

const browserAgentFallback: LocalAgentStatus[] = [
  {
    id: "codex",
    name: "Codex",
    detected: false,
    import_supported: true,
    source_path: null,
    message: "需要在 Tauri 桌面运行时中检测。",
  },
  {
    id: "hermes",
    name: "Hermes",
    detected: false,
    import_supported: true,
    source_path: null,
    message: "需要在 Tauri 桌面运行时中检测。",
  },
  {
    id: "opencode",
    name: "opencode",
    detected: false,
    import_supported: true,
    source_path: null,
    message: "需要在 Tauri 桌面运行时中检测。",
  },
  {
    id: "claude-code",
    name: "Claude Code",
    detected: false,
    import_supported: true,
    source_path: null,
    message: "需要在 Tauri 桌面运行时中检测。",
  },
];

const browserAgentSourceFallback: AgentSourceSummary[] = browserAgentFallback.map((agent) => ({
  ...agent,
  imported_calls: 0,
  total_tokens: 0,
  estimated_cost_usd: 0,
  cost_currency: "USD",
  last_imported_at: null,
  last_call_at: null,
}));

const emptyCallPage: LlmCallPage = {
  rows: [],
  total: 0,
};

const emptyFilterOptions: CallFilterOptions = {
  providers: [],
  agents: [],
  models: [],
  statuses: [],
};

const emptyDataHealthSummary: DataHealthSummary = {
  total_calls: 0,
  issue_calls: 0,
  issues: [],
};

const defaultSyncSettings: SyncSettings = {
  enabled: false,
  interval_minutes: 30,
  sync_on_startup: true,
  last_sync_at: null,
  next_sync_at: null,
  last_result: "浏览器预览环境未启用后台同步。",
  last_error: null,
};

function isDesktopRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function requireDesktopRuntime(action: string) {
  if (!isDesktopRuntime()) {
    throw new Error(`${action}需要在 Tauri 桌面运行时中执行。`);
  }
}

function normalizeCallFilters(overrides: Partial<LlmCallFilters> = {}): LlmCallFilters {
  return {
    from: overrides.from ?? null,
    to: overrides.to ?? null,
    provider: overrides.provider ?? null,
    agent_id: overrides.agent_id ?? null,
    workflow_id: overrides.workflow_id ?? null,
    project_id: overrides.project_id ?? null,
    session_id: overrides.session_id ?? null,
    model: overrides.model ?? null,
    status: overrides.status ?? null,
    limit: overrides.limit ?? 100,
    offset: overrides.offset ?? 0,
  };
}

async function invokeOrFallback<T>(command: string, args: Record<string, unknown>, fallback: T) {
  if (!isDesktopRuntime()) {
    return fallback;
  }

  return invoke<T>(command, args);
}

export function getDashboardSummary(range: DashboardRange) {
  return invokeOrFallback<DashboardSummary>("get_dashboard_summary", { range }, emptySummary);
}

export function getDashboardSummaryForDates(from: string, to: string) {
  return invokeOrFallback<DashboardSummary>(
    "get_dashboard_summary_for_dates",
    { from, to },
    emptySummary,
  );
}

export function getDailyUsageSeries(
  from: string,
  to: string,
  groupBy: DimensionKind | null = null,
) {
  return invokeOrFallback<DailyUsagePoint[]>(
    "get_daily_usage_series",
    { from, to, groupBy },
    [],
  );
}

export function getDimensionSummary(
  from: string,
  to: string,
  dimension: DimensionKind,
  value: string,
) {
  return invokeOrFallback<DashboardSummary>(
    "get_dimension_summary",
    { from, to, dimension, value },
    emptySummary,
  );
}

export function getDimensionDailySeries(
  from: string,
  to: string,
  dimension: DimensionKind,
  value: string,
) {
  return invokeOrFallback<DailyUsagePoint[]>(
    "get_dimension_daily_series",
    { from, to, dimension, value },
    [],
  );
}

export function getTopAgents(from: string, to: string, limit: number) {
  return invokeOrFallback<TopDimensionRow[]>("get_top_agents", { from, to, limit }, []);
}

export function getTopModels(from: string, to: string, limit: number) {
  return invokeOrFallback<TopDimensionRow[]>("get_top_models", { from, to, limit }, []);
}

export function getTopProviders(from: string, to: string, limit: number) {
  return invokeOrFallback<TopDimensionRow[]>("get_top_providers", { from, to, limit }, []);
}

export function getTopWorkflows(from: string, to: string, limit: number) {
  return invokeOrFallback<TopDimensionRow[]>("get_top_workflows", { from, to, limit }, []);
}

export function getTopProjects(from: string, to: string, limit: number) {
  return invokeOrFallback<TopDimensionRow[]>("get_top_projects", { from, to, limit }, []);
}

export function getTopSessions(from: string, to: string, limit: number) {
  return invokeOrFallback<TopDimensionRow[]>("get_top_sessions", { from, to, limit }, []);
}

export function getRecentCalls(limit: number) {
  return invokeOrFallback<LlmCallRow[]>("list_recent_calls", { limit }, []);
}

export function listLlmCalls(filters: LlmCallFilters) {
  return invokeOrFallback<LlmCallPage>("list_llm_calls", { filters }, emptyCallPage);
}

export function getCallFilterOptions() {
  return invokeOrFallback<CallFilterOptions>(
    "get_call_filter_options",
    {},
    emptyFilterOptions,
  );
}

export async function exportCallsCsv(filters?: Partial<LlmCallFilters>) {
  requireDesktopRuntime("导出 CSV");
  return invoke<string>("export_calls_csv", {
    filters: filters ? normalizeCallFilters(filters) : null,
  });
}

export async function exportDeviceDatasetPackage(exportDir: string) {
  requireDesktopRuntime("导出本机数据包");

  return invoke<string>("export_device_dataset_package", { exportDir });
}

export async function importDeviceDatasetPackage(path: string) {
  requireDesktopRuntime("导入设备数据包");

  return invoke<DevicePackageImportResult>("import_device_dataset_package", { path });
}

export async function openExportFolder(path?: string) {
  requireDesktopRuntime("打开导出文件夹");

  return invoke<string>("open_export_folder", { path: path ?? null });
}

export function listExternalDatasets() {
  return invokeOrFallback<ExternalDataset[]>("list_external_datasets", {}, []);
}

export async function removeExternalDataset(datasetId: string) {
  requireDesktopRuntime("移除设备数据");

  return invoke<number>("remove_external_dataset", { datasetId });
}

export function getDataHealthSummary() {
  return invokeOrFallback<DataHealthSummary>(
    "get_data_health_summary",
    {},
    emptyDataHealthSummary,
  );
}

export function listDataHealthIssues(filters?: Partial<LlmCallFilters>) {
  return invokeOrFallback<DataHealthIssueRow[]>(
    "list_data_health_issues",
    { filters: normalizeCallFilters({ limit: 50, offset: 0, ...filters }) },
    [],
  );
}

export function listCustomImporterProfiles() {
  return invokeOrFallback<CustomImporterProfile[]>("list_custom_importer_profiles", {}, []);
}

export async function upsertCustomImporterProfile(input: CustomImporterProfileInput) {
  requireDesktopRuntime("保存自定义数据源");

  return invoke<CustomImporterProfile>("upsert_custom_importer_profile", { input });
}

export async function deleteCustomImporterProfile(id: string) {
  requireDesktopRuntime("删除自定义数据源");

  return invoke<boolean>("delete_custom_importer_profile", { id });
}

export async function previewCustomImporter(input: CustomImporterProfileInput) {
  requireDesktopRuntime("预览自定义数据源");

  return invoke<CustomImporterPreview>("preview_custom_importer", { input });
}

export async function runCustomImporter(id: string) {
  requireDesktopRuntime("同步自定义数据源");

  return invoke<CustomImporterRunResult>("run_custom_importer", { id });
}

export function getSyncSettings() {
  return invokeOrFallback<SyncSettings>("get_sync_settings", {}, defaultSyncSettings);
}

export async function saveSyncSettings(settings: SyncSettingsInput) {
  if (!isDesktopRuntime()) {
    return { ...defaultSyncSettings, ...settings };
  }

  return invoke<SyncSettings>("save_sync_settings", { input: settings });
}

export async function runBackgroundSyncOnce() {
  if (!isDesktopRuntime()) {
    return {
      ...defaultSyncSettings,
      last_result: "浏览器预览环境已跳过后台同步。",
    };
  }

  return invoke<SyncSettings>("run_background_sync_once");
}

function toAppUpdateInfo(update: Update | null): AppUpdateInfo {
  return {
    available: update !== null,
    current_version: update?.currentVersion ?? null,
    version: update?.version ?? null,
    date: update?.date ?? null,
    body: update?.body ?? null,
  };
}

export async function checkForAppUpdate() {
  if (!isDesktopRuntime()) {
    return {
      available: false,
      current_version: null,
      version: null,
      date: null,
      body: "浏览器预览环境无法检查应用更新。",
    };
  }

  pendingAppUpdate = await check();
  return toAppUpdateInfo(pendingAppUpdate);
}

export async function installPendingAppUpdate(
  onProgress?: (progress: AppUpdateProgress) => void,
) {
  requireDesktopRuntime("安装应用更新");

  if (!pendingAppUpdate) {
    throw new Error("没有可安装的待处理更新，请先检查更新。");
  }

  let downloadedBytes = 0;
  let contentLength: number | null = null;

  function emitProgress(event: DownloadEvent) {
    if (event.event === "Started") {
      downloadedBytes = 0;
      contentLength = event.data.contentLength ?? null;
    } else if (event.event === "Progress") {
      downloadedBytes += event.data.chunkLength;
    }

    onProgress?.({
      event: event.event,
      downloaded_bytes: downloadedBytes,
      content_length: contentLength,
    });
  }

  const update = pendingAppUpdate;
  await update.downloadAndInstall(emitProgress);
  pendingAppUpdate = null;
  await relaunch();
}

export function detectLocalAgents() {
  return invokeOrFallback<LocalAgentStatus[]>("detect_local_agents", {}, browserAgentFallback);
}

export function listAgentSources() {
  return invokeOrFallback<AgentSourceSummary[]>(
    "list_agent_sources",
    {},
    browserAgentSourceFallback,
  );
}

export async function seedDemoData() {
  requireDesktopRuntime("生成演示数据");

  await invoke("seed_demo_data");
}

export async function clearDemoData() {
  requireDesktopRuntime("清理演示数据");

  return invoke<number>("clear_demo_data");
}

export async function importCodexThreads() {
  requireDesktopRuntime("导入 Codex 数据");

  return invoke<CodexImportResult>("import_codex_threads");
}

export async function importDetectedAgents(mode = "incremental" as AgentImportMode) {
  requireDesktopRuntime("导入本机 Agent 数据");

  return invoke<AgentImportResult[]>("import_detected_agents", { mode });
}
