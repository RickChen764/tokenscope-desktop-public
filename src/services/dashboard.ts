import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";
import { translateRuntime as tr } from "../i18n";
import {
  createAppUpdateInfo,
  defaultAppUpdateInfo,
  normalizeAppUpdateInfo,
  recoverStoredAppUpdateInfo,
} from "./appUpdateState";
import type {
  AgentImportResult,
  AgentSourceSummary,
  CallFilterOptions,
  CodexImportResult,
  CodexUsageLimitSnapshot,
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
  TokenPulseSnapshot,
  TopDimensionRow,
  CustomImporterPreview,
  CustomImporterProfile,
  CustomImporterProfileInput,
  CustomImporterRunResult,
  DevicePackageImportResult,
  ExternalDataset,
  GitHubSyncConnectionTestResult,
  GitHubSyncRemoteDevice,
  GitHubSyncRunResult,
  GitHubSyncRuntimeStatus,
  GitHubSyncSettings,
  GitHubSyncSettingsInput,
} from "../types/dashboard";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

export type AgentImportMode = "incremental" | "full";

let pendingAppUpdate: Update | null = null;
let appUpdateCheckPromise: Promise<AppUpdateInfo> | null = null;
let hasWrittenAppUpdateInfoThisSession = false;
const SYNC_SETTINGS_STORAGE_KEY = "tokenscope.syncSettings";
const APP_UPDATE_STATE_STORAGE_KEY = "tokenscope.appUpdateInfo";
export const APP_UPDATE_INFO_EVENT = "tokenscope:app-update-info";

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

function browserAgentFallback(): LocalAgentStatus[] {
  const message = tr("需要在 Tauri 桌面运行时中检测。");
  return [
    {
      id: "codex",
      name: "Codex",
      detected: false,
      import_supported: true,
      source_path: null,
      message,
    },
    {
      id: "hermes",
      name: "Hermes",
      detected: false,
      import_supported: true,
      source_path: null,
      message,
    },
    {
      id: "opencode",
      name: "opencode",
      detected: false,
      import_supported: true,
      source_path: null,
      message,
    },
    {
      id: "claude-code",
      name: "Claude Code",
      detected: false,
      import_supported: true,
      source_path: null,
      message,
    },
  ];
}

function browserAgentSourceFallback(): AgentSourceSummary[] {
  return browserAgentFallback().map((agent) => ({
    ...agent,
    imported_calls: 0,
    total_tokens: 0,
    estimated_cost_usd: 0,
    cost_currency: "USD",
    last_imported_at: null,
    last_call_at: null,
  }));
}

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

function localDateString(date = new Date()) {
  const year = date.getFullYear();
  const month = `${date.getMonth() + 1}`.padStart(2, "0");
  const day = `${date.getDate()}`.padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function emptyTokenPulse(historyDays: number): TokenPulseSnapshot {
  return {
    today_local: localDateString(),
    today_tokens: 0,
    today_calls: 0,
    yesterday_tokens: 0,
    average_daily_tokens: 0,
    history_days: historyDays,
    ratio_to_average: null,
    remaining_to_average: 0,
    hourly_tokens: [],
  };
}

function defaultSyncSettings(): SyncSettings {
  return {
    enabled: true,
    interval_minutes: 30,
    sync_on_startup: true,
    last_sync_at: null,
    next_sync_at: null,
    last_result: null,
    last_error: null,
  };
}

function normalizeSyncInterval(value: unknown) {
  const parsed = typeof value === "number" ? value : Number(value);
  if (!Number.isFinite(parsed)) {
    return 30;
  }

  return Math.min(1440, Math.max(1, Math.round(parsed)));
}

function defaultGitHubSyncSettings(): GitHubSyncSettings {
  return {
    enabled: false,
    owner: "",
    repo: "",
    branch: "main",
    path_prefix: "tokenscope-sync",
    data_mode: "aggregate_v3",
    token_configured: false,
    token_redacted: null,
    sync_password_configured: false,
    bootstrap_uploaded: false,
    last_upload_at: null,
    last_import_at: null,
    last_status: null,
    last_message: null,
    last_error: null,
  };
}

function defaultGitHubSyncRuntimeStatus(): GitHubSyncRuntimeStatus {
  return {
    running: false,
    mode: null,
    phase: null,
    message: null,
    started_at: null,
    updated_at: null,
    last_status: null,
    current_step: 0,
    total_steps: 0,
    uploaded_shards: 0,
    downloaded_shards: 0,
    imported: 0,
    skipped: 0,
  };
}

function nextBrowserSyncAt(settings: SyncSettings) {
  if (!settings.enabled) {
    return null;
  }

  if (!settings.last_sync_at) {
    return new Date().toISOString();
  }

  const lastSyncMs = Date.parse(settings.last_sync_at);
  if (Number.isNaN(lastSyncMs)) {
    return new Date().toISOString();
  }

  return new Date(lastSyncMs + settings.interval_minutes * 60_000).toISOString();
}

function normalizeSyncSettings(input: Partial<SyncSettings>): SyncSettings {
  const defaults = defaultSyncSettings();
  const normalized: SyncSettings = {
    ...defaults,
    ...input,
    enabled: typeof input.enabled === "boolean" ? input.enabled : defaults.enabled,
    interval_minutes: normalizeSyncInterval(input.interval_minutes),
    sync_on_startup:
      typeof input.sync_on_startup === "boolean"
        ? input.sync_on_startup
        : defaults.sync_on_startup,
    last_error: input.last_error ?? null,
    last_result: input.last_result ?? null,
  };
  normalized.next_sync_at = nextBrowserSyncAt(normalized);
  return normalized;
}

function readBrowserSyncSettings() {
  if (typeof window === "undefined") {
    return defaultSyncSettings();
  }

  try {
    const stored = window.localStorage.getItem(SYNC_SETTINGS_STORAGE_KEY);
    if (!stored) {
      return normalizeSyncSettings({});
    }

    return normalizeSyncSettings(JSON.parse(stored) as Partial<SyncSettings>);
  } catch {
    return normalizeSyncSettings({});
  }
}

function writeBrowserSyncSettings(settings: Partial<SyncSettings>) {
  const nextSettings = normalizeSyncSettings({
    ...readBrowserSyncSettings(),
    ...settings,
  });

  try {
    window.localStorage.setItem(SYNC_SETTINGS_STORAGE_KEY, JSON.stringify(nextSettings));
  } catch {
    // Browser preview storage is best effort; desktop builds persist via SQLite.
  }

  return nextSettings;
}

function stringifyError(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function readStoredAppUpdateInfo() {
  if (typeof window === "undefined") {
    return defaultAppUpdateInfo();
  }

  try {
    const stored = window.localStorage.getItem(APP_UPDATE_STATE_STORAGE_KEY);
    if (!stored) {
      return defaultAppUpdateInfo();
    }

    return normalizeAppUpdateInfo(JSON.parse(stored) as Partial<AppUpdateInfo>);
  } catch {
    return defaultAppUpdateInfo();
  }
}

export function getStoredAppUpdateInfo() {
  const storedInfo = readStoredAppUpdateInfo();
  const recoveredInfo = hasWrittenAppUpdateInfoThisSession
    ? storedInfo
    : recoverStoredAppUpdateInfo(storedInfo);

  if (storedInfo.status !== recoveredInfo.status && typeof window !== "undefined") {
    try {
      window.localStorage.setItem(APP_UPDATE_STATE_STORAGE_KEY, JSON.stringify(recoveredInfo));
    } catch {
      // Update state only improves UX; the updater itself remains authoritative.
    }
  }

  return recoveredInfo;
}

function writeStoredAppUpdateInfo(info: Partial<AppUpdateInfo>) {
  const nextInfo = normalizeAppUpdateInfo({
    ...readStoredAppUpdateInfo(),
    ...info,
  });
  hasWrittenAppUpdateInfoThisSession = true;

  if (typeof window !== "undefined") {
    try {
      window.localStorage.setItem(APP_UPDATE_STATE_STORAGE_KEY, JSON.stringify(nextInfo));
    } catch {
      // Update state only improves UX; the updater itself remains authoritative.
    }

    window.dispatchEvent(new CustomEvent<AppUpdateInfo>(APP_UPDATE_INFO_EVENT, { detail: nextInfo }));
  }

  return nextInfo;
}

function isDesktopRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function requireDesktopRuntime(action: string) {
  if (!isDesktopRuntime()) {
    throw new Error(
      tr("{action}需要在 Tauri 桌面运行时中执行。", {
        action: tr(action),
      }),
    );
  }
}

async function readCurrentAppVersion() {
  if (!isDesktopRuntime()) {
    return null;
  }

  try {
    return await getVersion();
  } catch {
    return null;
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

export function getTokenPulse(historyDays = 30) {
  return invokeOrFallback<TokenPulseSnapshot>(
    "get_token_pulse",
    { historyDays },
    emptyTokenPulse(historyDays),
  );
}

type TokenPulseDetailHoverSize = {
  detailWidth?: number;
  detailHeight?: number;
};

export function setTokenPulseDetailHovered(
  source: "mini" | "detail",
  hovered: boolean,
  detailSize?: TokenPulseDetailHoverSize,
) {
  return invokeOrFallback<void>(
    "set_token_pulse_detail_hovered",
    { source, hovered, ...detailSize },
    undefined,
  );
}

export function setTokenPulseDragging(dragging: boolean) {
  return invokeOrFallback<void>(
    "set_token_pulse_dragging",
    { dragging },
    undefined,
  );
}

export function showTokenPulseContextMenu() {
  return invokeOrFallback<void>("show_token_pulse_context_menu", {}, undefined);
}

export function openTokenPulseHome() {
  return invokeOrFallback<void>("open_token_pulse_home", {}, undefined);
}

export function hideTokenPulseWindow() {
  return invokeOrFallback<void>("hide_token_pulse_window", {}, undefined);
}

export function getTokenPulsePositionLocked() {
  return invokeOrFallback<boolean>("get_token_pulse_position_locked", {}, false);
}

export function setTokenPulsePositionLocked(locked: boolean) {
  return invokeOrFallback<void>(
    "set_token_pulse_position_locked",
    { locked },
    undefined,
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
  return invokeOrFallback<SyncSettings>("get_sync_settings", {}, readBrowserSyncSettings());
}

export async function saveSyncSettings(settings: SyncSettingsInput) {
  if (!isDesktopRuntime()) {
    return writeBrowserSyncSettings(settings);
  }

  return invoke<SyncSettings>("save_sync_settings", { input: settings });
}

export async function runBackgroundSyncOnce() {
  if (!isDesktopRuntime()) {
    return writeBrowserSyncSettings({
      last_sync_at: new Date().toISOString(),
      last_error: null,
      last_result: tr("浏览器预览环境已跳过后台同步。"),
    });
  }

  return invoke<SyncSettings>("run_background_sync_once");
}

export function getGitHubSyncSettings() {
  return invokeOrFallback<GitHubSyncSettings>(
    "get_github_sync_settings",
    {},
    defaultGitHubSyncSettings(),
  );
}

export function getGitHubSyncRuntimeStatus() {
  return invokeOrFallback<GitHubSyncRuntimeStatus>(
    "get_github_sync_runtime_status",
    {},
    defaultGitHubSyncRuntimeStatus(),
  );
}

export function listGitHubSyncRemoteDevices() {
  return invokeOrFallback<GitHubSyncRemoteDevice[]>(
    "list_github_sync_remote_devices",
    {},
    [],
  );
}

export async function saveGitHubSyncSettings(settings: GitHubSyncSettingsInput) {
  if (!isDesktopRuntime()) {
    return {
      ...defaultGitHubSyncSettings(),
      ...settings,
      token_configured: Boolean(settings.token?.trim()),
      token_redacted: settings.token ? "已配置" : null,
      sync_password_configured: Boolean(settings.sync_password?.trim()),
    };
  }

  return invoke<GitHubSyncSettings>("save_github_sync_settings", { input: settings });
}

export function testGitHubSyncConnection() {
  return invokeOrFallback<GitHubSyncConnectionTestResult>(
    "test_github_sync_connection",
    {},
    {
      status: "browser-preview",
      message: tr("浏览器预览环境无法测试 GitHub 连接。"),
    },
  );
}

export function runGitHubSyncOnce() {
  return invokeOrFallback<GitHubSyncRunResult>(
    "run_github_sync_once",
    {},
    {
      status: "browser-preview",
      message: tr("浏览器预览环境已跳过 GitHub 同步。"),
      uploaded_shards: 0,
      downloaded_shards: 0,
      imported: 0,
      skipped: 0,
      started_at: new Date().toISOString(),
      finished_at: new Date().toISOString(),
    },
  );
}

export function forceGitHubSyncBootstrapUpload() {
  return invokeOrFallback<GitHubSyncRunResult>(
    "force_github_sync_bootstrap_upload",
    {},
    {
      status: "browser-preview",
      message: tr("浏览器预览环境已跳过 GitHub bootstrap 重传。"),
      uploaded_shards: 0,
      downloaded_shards: 0,
      imported: 0,
      skipped: 0,
      started_at: new Date().toISOString(),
      finished_at: new Date().toISOString(),
    },
  );
}

export function forceReimportGitHubSyncRemoteDevice(deviceId: string) {
  return invokeOrFallback<GitHubSyncRunResult>(
    "force_reimport_github_sync_remote_device",
    { deviceId },
    {
      status: "browser-preview",
      message: tr("浏览器预览环境已跳过 GitHub 远端设备重新导入。"),
      uploaded_shards: 0,
      downloaded_shards: 0,
      imported: 0,
      skipped: 0,
      started_at: new Date().toISOString(),
      finished_at: new Date().toISOString(),
    },
  );
}

async function runAppUpdateCheck() {
  if (!isDesktopRuntime()) {
    return writeStoredAppUpdateInfo({
      available: false,
      current_version: null,
      version: null,
      date: null,
      body: tr("浏览器预览环境无法检查应用更新。"),
      status: "browser-preview",
      checked_at: new Date().toISOString(),
      error: null,
    });
  }

  const currentVersionPromise = readCurrentAppVersion();
  try {
    const [nextUpdate, currentVersion] = await Promise.all([check(), currentVersionPromise]);
    pendingAppUpdate = nextUpdate;
    return writeStoredAppUpdateInfo(createAppUpdateInfo(pendingAppUpdate, currentVersion));
  } catch (err) {
    pendingAppUpdate = null;
    const currentVersion = await currentVersionPromise;
    writeStoredAppUpdateInfo({
      available: false,
      current_version: currentVersion,
      status: "error",
      checked_at: new Date().toISOString(),
      error: stringifyError(err),
    });
    throw err;
  }
}

export async function checkForAppUpdate() {
  if (appUpdateCheckPromise) {
    return appUpdateCheckPromise;
  }

  appUpdateCheckPromise = runAppUpdateCheck();
  try {
    return await appUpdateCheckPromise;
  } finally {
    appUpdateCheckPromise = null;
  }
}

export async function installPendingAppUpdate(
  onProgress?: (progress: AppUpdateProgress) => void,
) {
  requireDesktopRuntime("安装应用更新");

  if (!pendingAppUpdate) {
    const currentVersion = await readCurrentAppVersion();
    try {
      pendingAppUpdate = await check();
    } catch (err) {
      writeStoredAppUpdateInfo({
        current_version: currentVersion,
        status: "error",
        checked_at: new Date().toISOString(),
        error: stringifyError(err),
      });
      throw err;
    }
  }

  if (!pendingAppUpdate) {
    const currentVersion = await readCurrentAppVersion();
    writeStoredAppUpdateInfo(createAppUpdateInfo(null, currentVersion));
    throw new Error(tr("没有可安装的待处理更新，请先检查更新。"));
  }

  let downloadedBytes = 0;
  let contentLength: number | null = null;

  function emitProgress(event: DownloadEvent) {
    if (event.event === "Started") {
      downloadedBytes = 0;
      contentLength = event.data.contentLength ?? null;
    } else if (event.event === "Progress") {
      downloadedBytes += event.data.chunkLength;
    } else if (event.event === "Finished") {
      downloadedBytes = contentLength ?? downloadedBytes;
      writeStoredAppUpdateInfo({
        status: "installing",
        error: null,
      });
    }

    onProgress?.({
      event: event.event,
      downloaded_bytes: downloadedBytes,
      content_length: contentLength,
    });
  }

  const update = pendingAppUpdate;
  writeStoredAppUpdateInfo({
    status: "downloading",
    error: null,
  });

  try {
    await update.downloadAndInstall(emitProgress);
    pendingAppUpdate = null;
    writeStoredAppUpdateInfo({
      status: "installing",
      error: null,
    });
    await relaunch();
  } catch (err) {
    writeStoredAppUpdateInfo({
      status: "error",
      error: stringifyError(err),
    });
    throw err;
  }
}

export function detectLocalAgents() {
  return invokeOrFallback<LocalAgentStatus[]>("detect_local_agents", {}, browserAgentFallback());
}

export function listAgentSources() {
  return invokeOrFallback<AgentSourceSummary[]>(
    "list_agent_sources",
    {},
    browserAgentSourceFallback(),
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

export async function getCodexUsageLimits() {
  requireDesktopRuntime("读取 Codex 剩余用量");

  return invoke<CodexUsageLimitSnapshot | null>("get_codex_usage_limits");
}

export async function importDetectedAgents(mode = "incremental" as AgentImportMode) {
  requireDesktopRuntime("导入本机 Agent 数据");

  return invoke<AgentImportResult[]>("import_detected_agents", { mode });
}
