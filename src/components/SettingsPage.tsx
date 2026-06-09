import { useCallback, useEffect, useRef, useState } from "react";
import { AgentSourcesPanel } from "./AgentSourcesPanel";
import { CustomImportersPanel } from "./CustomImportersPanel";
import { DeviceDatasetsPanel } from "./DeviceDatasetsPanel";
import { GitHubSyncPanel } from "./GitHubSyncPanel";
import { ToastNotice, type ToastNoticeValue } from "./ToastNotice";
import {
  checkForAppUpdate,
  clearDemoData,
  detectLocalAgents,
  exportCallsCsv,
  getStoredAppUpdateInfo,
  getSyncSettings,
  importDetectedAgents,
  installPendingAppUpdate,
  listAgentSources,
  runBackgroundSyncOnce,
  saveSyncSettings,
} from "../services/dashboard";
import { appUpdateVersionRange } from "../services/appUpdateState";
import { useI18n, type AppLanguage } from "../i18n";
import { useDisplayPreference, type NumberDisplayMode } from "../preferences/display";
import type {
  AgentSourceSummary,
  AppUpdateInfo,
  AppUpdateProgress,
  SyncSettings,
  SyncSettingsInput,
} from "../types/dashboard";
import { formatBytes, formatDateTime, formatInteger } from "../utils/format";

const SYNC_INTERVAL_VALUES = [1, 5, 15, 30, 60];

const defaultSyncDraft: SyncSettingsInput = {
  enabled: true,
  interval_minutes: 30,
  sync_on_startup: true,
};

const emptyUpdateProgress: AppUpdateProgress = {
  event: "Started",
  downloaded_bytes: 0,
  content_length: null,
};

type SettingsTabId = "overview" | "sources" | "devices" | "app" | "advanced";

function latestDateTime(values: Array<string | null>) {
  const sortedValues = values.filter((value): value is string => Boolean(value)).sort();
  return sortedValues.length > 0 ? sortedValues[sortedValues.length - 1] : null;
}

function syncDraftFromSettings(settings: SyncSettings): SyncSettingsInput {
  return {
    enabled: settings.enabled,
    interval_minutes: settings.interval_minutes,
    sync_on_startup: settings.sync_on_startup,
  };
}

interface SettingsPageProps {
  isSeedLoading: boolean;
  isSyncing: boolean;
  onSeedDemoData: () => Promise<void>;
}

export function SettingsPage({
  isSeedLoading,
  isSyncing,
  onSeedDemoData,
}: SettingsPageProps) {
  const { language, numberLocale, setLanguage, t } = useI18n();
  const {
    numberDisplayMode,
    setNumberDisplayMode,
    showCodexUsageLimits,
    setShowCodexUsageLimits,
  } = useDisplayPreference();
  const [activeSettingsTab, setActiveSettingsTab] = useState<SettingsTabId>("overview");
  const [sources, setSources] = useState<AgentSourceSummary[]>([]);
  const [isSourcesLoading, setIsSourcesLoading] = useState(true);
  const [isDetecting, setIsDetecting] = useState(false);
  const [isImporting, setIsImporting] = useState(false);
  const [isExporting, setIsExporting] = useState(false);
  const [syncSettings, setSyncSettings] = useState<SyncSettings | null>(null);
  const [syncDraft, setSyncDraft] = useState<SyncSettingsInput>(defaultSyncDraft);
  const syncDraftRef = useRef<SyncSettingsInput>(defaultSyncDraft);
  const syncSaveRequestRef = useRef(0);
  const [isSyncSettingsLoading, setIsSyncSettingsLoading] = useState(true);
  const [isSavingSyncSettings, setIsSavingSyncSettings] = useState(false);
  const [isRunningBackgroundSync, setIsRunningBackgroundSync] = useState(false);
  const [updateInfo, setUpdateInfo] = useState<AppUpdateInfo>(() => getStoredAppUpdateInfo());
  const [isCheckingUpdate, setIsCheckingUpdate] = useState(false);
  const [isInstallingUpdate, setIsInstallingUpdate] = useState(false);
  const [updateProgress, setUpdateProgress] =
    useState<AppUpdateProgress>(emptyUpdateProgress);
  const [notice, setNotice] = useState<ToastNoticeValue | null>(null);

  const loadSources = useCallback(async (options?: { showLoading?: boolean }) => {
    if (options?.showLoading ?? true) {
      setIsSourcesLoading(true);
    }

    try {
      const nextSources = await listAgentSources();
      setSources(nextSources);
      return nextSources;
    } catch (err) {
      setNotice({
        kind: "error",
        message: t("读取本机 Agent 来源失败：{error}", {
          error: err instanceof Error ? err.message : String(err),
        }),
      });
      return null;
    } finally {
      if (options?.showLoading ?? true) {
        setIsSourcesLoading(false);
      }
    }
  }, [t]);

  const applySyncSettings = useCallback((settings: SyncSettings) => {
    const nextDraft = syncDraftFromSettings(settings);
    syncDraftRef.current = nextDraft;
    setSyncSettings(settings);
    setSyncDraft(nextDraft);
  }, []);

  const loadSyncSettings = useCallback(async () => {
    setIsSyncSettingsLoading(true);
    try {
      const settings = await getSyncSettings();
      applySyncSettings(settings);
    } catch (err) {
      setNotice({
        kind: "error",
        message: t("读取后台自动同步设置失败：{error}", {
          error: err instanceof Error ? err.message : String(err),
        }),
      });
    } finally {
      setIsSyncSettingsLoading(false);
    }
  }, [applySyncSettings, t]);

  useEffect(() => {
    void loadSources();
    void loadSyncSettings();
  }, [loadSources, loadSyncSettings]);

  async function handleSeed() {
    setNotice(null);
    try {
      await onSeedDemoData();
    } catch (err) {
      setNotice({
        kind: "error",
        message: t("生成演示数据失败：{error}", {
          error: err instanceof Error ? err.message : String(err),
        }),
      });
    }
  }

  async function handleDetect() {
    setIsDetecting(true);
    setNotice(null);
    try {
      const detectedAgents = await detectLocalAgents();
      const detectedCount = detectedAgents.filter((agent) => agent.detected).length;
      const syncableCount = detectedAgents.filter(
        (agent) => agent.detected && agent.import_supported,
      ).length;
      const nextSources = await loadSources({ showLoading: false });
      if (!nextSources) {
        return;
      }
      setNotice({
        kind: "success",
        message: t("本地 Agent 检测完成：发现 {detectedCount} 个来源，其中 {syncableCount} 个可同步。", {
          detectedCount,
          syncableCount,
        }),
      });
    } catch (err) {
      setNotice({
        kind: "error",
        message: t("检测失败：{error}", {
          error: err instanceof Error ? err.message : String(err),
        }),
      });
    } finally {
      setIsDetecting(false);
    }
  }

  async function handleSync() {
    setIsImporting(true);
    setNotice(null);
    try {
      const results = await importDetectedAgents("incremental");
      const imported = results.reduce((total, result) => total + result.imported, 0);
      const skipped = results.reduce((total, result) => total + result.skipped, 0);
      const failedResults = results.filter((result) => result.status === "error");
      const nextSources = await loadSources({ showLoading: false });
      if (!nextSources) {
        return;
      }
      if (failedResults.length > 0) {
        const errors = failedResults
          .map((result) => `${result.name}: ${result.error || result.message}`)
          .join(language === "zh-CN" ? "；" : "; ");
        setNotice({
          kind: "error",
          message: t("同步失败：已写入 {imported} 条，跳过 {skipped} 条。{errors}", {
            errors,
            imported,
            skipped,
          }),
        });
        return;
      }
      const clearedDemoRows = await clearDemoData();
      const cleanupText =
        clearedDemoRows > 0 ? t("，清理演示数据 {count} 条", { count: clearedDemoRows }) : "";
      setNotice({
        kind: "success",
        message: t("同步完成：写入 {imported} 条，跳过 {skipped} 条{cleanupText}。", {
          cleanupText,
          imported,
          skipped,
        }),
      });
    } catch (err) {
      setNotice({
        kind: "error",
        message: t("同步失败：{error}", {
          error: err instanceof Error ? err.message : String(err),
        }),
      });
    } finally {
      setIsImporting(false);
    }
  }

  async function handleExport() {
    setIsExporting(true);
    setNotice(null);
    try {
      const path = await exportCallsCsv();
      setNotice({ kind: "success", message: t("CSV 已导出：{path}", { path }) });
    } catch (err) {
      setNotice({
        kind: "error",
        message: t("导出 CSV 失败：{error}", {
          error: err instanceof Error ? err.message : String(err),
        }),
      });
    } finally {
      setIsExporting(false);
    }
  }

  async function handleFullSync() {
    setIsImporting(true);
    setNotice(null);
    try {
      const results = await importDetectedAgents("full");
      const imported = results.reduce((total, result) => total + result.imported, 0);
      const skipped = results.reduce((total, result) => total + result.skipped, 0);
      const failedResults = results.filter((result) => result.status === "error");
      const nextSources = await loadSources({ showLoading: false });
      if (!nextSources) {
        return;
      }
      if (failedResults.length > 0) {
        const errors = failedResults
          .map((result) => `${result.name}: ${result.error || result.message}`)
          .join(language === "zh-CN" ? "；" : "; ");
        setNotice({
          kind: "error",
          message: t("全量刷新失败：已写入 {imported} 条，跳过 {skipped} 条。{errors}", {
            errors,
            imported,
            skipped,
          }),
        });
        return;
      }
      const clearedDemoRows = await clearDemoData();
      const cleanupText =
        clearedDemoRows > 0 ? t("，清理演示数据 {count} 条", { count: clearedDemoRows }) : "";
      setNotice({
        kind: "success",
        message: t("全量刷新完成：写入 {imported} 条，跳过 {skipped} 条{cleanupText}。", {
          cleanupText,
          imported,
          skipped,
        }),
      });
    } catch (err) {
      setNotice({
        kind: "error",
        message: t("全量刷新失败：{error}", {
          error: err instanceof Error ? err.message : String(err),
        }),
      });
    } finally {
      setIsImporting(false);
    }
  }

  async function persistSyncSettingsDraft(nextDraft: SyncSettingsInput) {
    const requestId = syncSaveRequestRef.current + 1;
    syncSaveRequestRef.current = requestId;
    setIsSavingSyncSettings(true);
    try {
      const settings = await saveSyncSettings(nextDraft);
      if (requestId === syncSaveRequestRef.current) {
        applySyncSettings(settings);
      }
    } catch (err) {
      if (requestId === syncSaveRequestRef.current) {
        setNotice({
          kind: "error",
          message: t("保存后台自动同步设置失败：{error}", {
            error: err instanceof Error ? err.message : String(err),
          }),
        });
      }
    } finally {
      if (requestId === syncSaveRequestRef.current) {
        setIsSavingSyncSettings(false);
      }
    }
  }

  function updateSyncDraft<K extends keyof SyncSettingsInput>(key: K, value: SyncSettingsInput[K]) {
    const nextDraft = { ...syncDraftRef.current, [key]: value };
    syncDraftRef.current = nextDraft;
    setSyncDraft(nextDraft);
    void persistSyncSettingsDraft(nextDraft);
  }

  async function handleSaveSyncSettings() {
    setIsSavingSyncSettings(true);
    setNotice(null);
    try {
      const settings = await saveSyncSettings(syncDraft);
      applySyncSettings(settings);
      setNotice({ kind: "success", message: t("后台自动同步设置已保存。") });
    } catch (err) {
      setNotice({
        kind: "error",
        message: t("保存后台自动同步设置失败：{error}", {
          error: err instanceof Error ? err.message : String(err),
        }),
      });
    } finally {
      setIsSavingSyncSettings(false);
    }
  }

  async function handleRunBackgroundSyncOnce() {
    setIsRunningBackgroundSync(true);
    setNotice(null);
    try {
      const settings = await runBackgroundSyncOnce();
      applySyncSettings(settings);
      setNotice({ kind: "success", message: t("已触发一次后台同步。") });
      void loadSources({ showLoading: false });
    } catch (err) {
      setNotice({
        kind: "error",
        message: t("触发后台同步失败：{error}", {
          error: err instanceof Error ? err.message : String(err),
        }),
      });
    } finally {
      setIsRunningBackgroundSync(false);
    }
  }

  async function handleCheckForUpdate() {
    setIsCheckingUpdate(true);
    setNotice(null);
    setUpdateProgress(emptyUpdateProgress);
    setUpdateInfo((current) => ({
      ...current,
      status: "checking",
      checked_at: new Date().toISOString(),
      error: null,
    }));
    try {
      const nextUpdateInfo = await checkForAppUpdate();
      setUpdateInfo(nextUpdateInfo);
      const updateVersionLabel =
        appUpdateVersionRange(nextUpdateInfo.current_version, nextUpdateInfo.version) ??
        nextUpdateInfo.version ??
        "";
      setNotice({
        kind: "success",
        message: nextUpdateInfo.available
          ? t("发现新版本 {version}，可以下载并安装。", {
              version: updateVersionLabel,
            })
          : t("当前已经是最新版本。"),
      });
    } catch (err) {
      setNotice({
        kind: "error",
        message: t("检查更新失败：{error}", {
          error: err instanceof Error ? err.message : String(err),
        }),
      });
      setUpdateInfo(getStoredAppUpdateInfo());
    } finally {
      setIsCheckingUpdate(false);
    }
  }

  async function handleInstallUpdate() {
    setIsInstallingUpdate(true);
    setNotice(null);
    setUpdateProgress(emptyUpdateProgress);
    setUpdateInfo((current) => ({
      ...current,
      status: "downloading",
      error: null,
    }));
    try {
      await installPendingAppUpdate((progress) => {
        setUpdateProgress(progress);
        setUpdateInfo((current) => ({
          ...current,
          status: progress.event === "Finished" ? "installing" : "downloading",
          error: null,
        }));
      });
      setNotice({
        kind: "success",
        message: t("更新安装程序已启动。Windows 会在安装更新时自动关闭当前应用。"),
      });
    } catch (err) {
      setNotice({
        kind: "error",
        message: t("安装更新失败：{error}", {
          error: err instanceof Error ? err.message : String(err),
        }),
      });
      setUpdateInfo(getStoredAppUpdateInfo());
      setIsInstallingUpdate(false);
    }
  }

  const syncControlsDisabled =
    isSyncSettingsLoading || isSavingSyncSettings || isRunningBackgroundSync;
  const isAppUpdateBusy =
    isCheckingUpdate ||
    isInstallingUpdate ||
    updateInfo.status === "downloading" ||
    updateInfo.status === "installing";
  const lastSyncLabel = isSyncSettingsLoading
    ? t("读取中...")
    : formatDateTime(syncSettings?.last_sync_at ?? null, t("无"));
  const nextSyncLabel = isSyncSettingsLoading
    ? t("读取中...")
    : syncSettings?.enabled
      ? formatDateTime(syncSettings.next_sync_at, t("无"))
      : t("未启用");
  const lastResultLabel = isSyncSettingsLoading
    ? t("读取中...")
    : syncSettings?.last_result || t("尚未执行");
  const lastErrorLabel = isSyncSettingsLoading
    ? t("读取中...")
    : syncSettings?.last_error || t("无");

  const updateProgressPercent =
    updateProgress.content_length && updateProgress.content_length > 0
      ? Math.min(
          100,
          Math.round((updateProgress.downloaded_bytes / updateProgress.content_length) * 100),
        )
      : updateProgress.event === "Finished"
        ? 100
        : 0;
  const updateStatusLabel =
    updateInfo.status === "checking"
      ? t("\u68c0\u67e5\u4e2d...")
      : updateInfo.status === "current"
        ? t("\u5f53\u524d\u5df2\u662f\u6700\u65b0\u7248\u672c")
        : updateInfo.status === "available"
          ? t("\u53ef\u66f4\u65b0")
          : updateInfo.status === "downloading"
            ? t("\u4e0b\u8f7d\u4e2d...")
            : updateInfo.status === "installing"
              ? t("\u5b89\u88c5\u4e2d...")
              : updateInfo.status === "error"
                ? t("\u68c0\u67e5\u5931\u8d25")
                : updateInfo.status === "browser-preview"
                  ? t("\u9884\u89c8\u73af\u5883")
                  : t("\u5c1a\u672a\u68c0\u67e5");
  const updateLastCheckedLabel = formatDateTime(
    updateInfo.checked_at,
    t("\u5c1a\u672a\u68c0\u67e5"),
  );
  const updateCurrentVersionLabel = updateInfo.current_version || t("\u672a\u77e5");
  const updateAvailableVersionValue = appUpdateVersionRange(
    updateInfo.current_version,
    updateInfo.version,
  );
  const updateAvailableVersionLabel =
    updateAvailableVersionValue ||
    (updateInfo.status === "current" ? t("\u5df2\u662f\u6700\u65b0\u7248\u672c") : t("\u65e0"));
  const updateProgressBytesLabel = updateProgress.content_length
    ? `${formatBytes(updateProgress.downloaded_bytes, language)} / ${formatBytes(
        updateProgress.content_length,
        language,
      )}`
    : formatBytes(updateProgress.downloaded_bytes, language);

  function handleLanguageChange(nextLanguage: AppLanguage) {
    setLanguage(nextLanguage);
    setNotice({
      kind: "success",
      message:
        nextLanguage === "zh-CN"
          ? t("界面语言已切换为中文。")
          : "Interface language changed to English.",
    });
  }

  function handleNumberDisplayModeChange(nextMode: NumberDisplayMode) {
    setNumberDisplayMode(nextMode);
    setNotice({
      kind: "success",
      message:
        nextMode === "compact"
          ? t("数字显示已切换为缩略显示。")
          : t("数字显示已切换为完整显示。"),
    });
  }

  const settingsTabs: Array<{
    id: SettingsTabId;
    label: string;
    description: string;
  }> = [
    {
      id: "overview",
      label: t("同步概览"),
      description: t("查看同步状态和常用操作。"),
    },
    {
      id: "sources",
      label: t("数据来源"),
      description: t("管理本机 Agent 和自定义 SQLite 来源。"),
    },
    {
      id: "devices",
      label: t("多设备"),
      description: t("配置 GitHub 同步和设备数据包。"),
    },
    {
      id: "app",
      label: t("应用"),
      description: t("数字显示、语言和更新。"),
    },
    {
      id: "advanced",
      label: t("高级"),
      description: t("低频维护和统计边界。"),
    },
  ];
  const activeTabLabel =
    settingsTabs.find((tab) => tab.id === activeSettingsTab)?.label ?? t("同步概览");
  const detectedSourceCount = sources.filter((source) => source.detected).length;
  const supportedDetectedCount = sources.filter(
    (source) => source.detected && source.import_supported,
  ).length;
  const totalImportedCalls = sources.reduce((total, source) => total + source.imported_calls, 0);
  const lastImportedAt = latestDateTime(sources.map((source) => source.last_imported_at));
  const lastCallAt = latestDateTime(sources.map((source) => source.last_call_at));
  const sourceSummaryLabel = isSourcesLoading
    ? t("读取中...")
    : `${detectedSourceCount}/${sources.length}`;
  const syncEnabledLabel = isSyncSettingsLoading
    ? t("读取中...")
    : syncSettings?.enabled
      ? t("已启用")
      : t("已停用");
  const syncIntervalLabel = isSyncSettingsLoading
    ? t("读取中...")
    : syncSettings?.enabled
      ? `${syncDraft.interval_minutes} ${t("分钟")}`
      : t("未启用");
  const sourceImportedLabel = isSourcesLoading
    ? t("读取中...")
    : formatInteger(totalImportedCalls, numberLocale);
  const sourceLastCallLabel = isSourcesLoading
    ? t("读取中...")
    : formatDateTime(lastCallAt, t("无"));
  const sourceLastImportedLabel = isSourcesLoading
    ? t("读取中...")
    : formatDateTime(lastImportedAt, t("无"));
  const issueSummaryLabel = isSyncSettingsLoading
    ? t("读取中...")
    : syncSettings?.last_error || updateInfo.error || t("无");
  const issueSummaryHasError = Boolean(syncSettings?.last_error || updateInfo.error);

  return (
    <section className="settings-page">
      <ToastNotice notice={notice} onClose={() => setNotice(null)} />

      <nav className="settings-tab-list" role="tablist" aria-label={t("设置分类")}>
        {settingsTabs.map((tab) => (
          <button
            aria-controls={`settings-panel-${tab.id}`}
            aria-selected={activeSettingsTab === tab.id}
            className={`settings-tab-button${activeSettingsTab === tab.id ? " active" : ""}`}
            id={`settings-tab-${tab.id}`}
            key={tab.id}
            onClick={() => setActiveSettingsTab(tab.id)}
            role="tab"
            type="button"
          >
            <span>{tab.label}</span>
            <small>{tab.description}</small>
          </button>
        ))}
      </nav>

      {activeSettingsTab === "overview" ? (
        <section
          aria-label={activeTabLabel}
          aria-labelledby="settings-tab-overview"
          className="settings-tab-panel settings-overview-panel"
          id="settings-panel-overview"
          role="tabpanel"
        >
          <div className="settings-section-heading">
            <div>
              <h2>{t("同步概览")}</h2>
              <p>{t("把同步状态、常用动作和后续配置入口集中在一屏。")}</p>
            </div>
          </div>

          <div className="settings-summary-grid" aria-label={t("设置摘要")}>
            <article className="settings-summary-card">
              <span>{t("本地来源")}</span>
              <strong>{sourceSummaryLabel}</strong>
              <small>{t("已检测 / 总来源")}</small>
            </article>
            <article className="settings-summary-card">
              <span>{t("已导入调用")}</span>
              <strong>{sourceImportedLabel}</strong>
              <small>{t("最近导入 {time}", { time: sourceLastImportedLabel })}</small>
            </article>
            <article className="settings-summary-card">
              <span>{t("后台同步")}</span>
              <strong>{syncEnabledLabel}</strong>
              <small>{syncIntervalLabel}</small>
            </article>
            <article className={`settings-summary-card${issueSummaryHasError ? " attention" : ""}`}>
              <span>{t("最近问题")}</span>
              <strong title={issueSummaryLabel}>{issueSummaryLabel}</strong>
              <small>{t("同步和更新状态")}</small>
            </article>
          </div>

          <div className="settings-overview-grid">
            <section className="panel settings-overview-card">
              <div className="panel-heading settings-heading">
                <div>
                  <h2>{t("本机 Agent 同步")}</h2>
                  <p>{t("查看本机来源检测和导入状态，常用同步动作可直接执行。")}</p>
                </div>
              </div>
              <div className="detail-stat-list settings-overview-list">
                <div>
                  <span>{t("检测结果")}</span>
                  <strong>{sourceSummaryLabel}</strong>
                </div>
                <div>
                  <span>{t("可同步来源")}</span>
                  <strong>
                    {isSourcesLoading ? t("读取中...") : formatInteger(supportedDetectedCount, numberLocale)}
                  </strong>
                </div>
                <div>
                  <span>{t("最近调用")}</span>
                  <strong>{sourceLastCallLabel}</strong>
                </div>
              </div>
              <div className="form-actions settings-overview-actions">
                <button
                  className="primary secondary"
                  disabled={isDetecting || isImporting || isSyncing || isSourcesLoading}
                  onClick={() => void handleDetect()}
                  type="button"
                >
                  {isDetecting ? t("检测中...") : t("重新检测")}
                </button>
                <button
                  aria-label={t("同步本机数据")}
                  className="primary"
                  disabled={
                    isImporting ||
                    isSyncing ||
                    isDetecting ||
                    isSourcesLoading ||
                    supportedDetectedCount === 0
                  }
                  onClick={() => void handleSync()}
                  type="button"
                >
                  {isImporting || isSyncing ? t("同步中...") : t("手动同步")}
                </button>
                <button
                  className="primary secondary"
                  onClick={() => setActiveSettingsTab("sources")}
                  type="button"
                >
                  {t("管理数据来源")}
                </button>
              </div>
            </section>

            <section className="panel sync-settings-card" aria-busy={isSyncSettingsLoading}>
              <div className="panel-heading settings-heading">
                <div>
                  <h2>{t("后台自动同步")}</h2>
                  <p>{t("按固定间隔自动同步本机 Agent 来源，也可以手动触发一次后台同步。")}</p>
                </div>
                <button
                  className="primary secondary"
                  disabled={syncControlsDisabled}
                  onClick={() => void handleRunBackgroundSyncOnce()}
                  type="button"
                >
                  {isRunningBackgroundSync ? t("同步中...") : t("立即同步一次")}
                </button>
              </div>

              <div className="sync-settings-layout">
                <div className="sync-control-grid">
                  <label className="switch-field">
                    <span>{t("启用后台自动同步")}</span>
                    <input
                      checked={syncDraft.enabled}
                      disabled={syncControlsDisabled}
                      onChange={(event) => updateSyncDraft("enabled", event.target.checked)}
                      role="switch"
                      type="checkbox"
                    />
                  </label>

                  <label className="field sync-interval-field">
                    <span>{t("同步间隔")}</span>
                    <select
                      disabled={syncControlsDisabled}
                      value={syncDraft.interval_minutes}
                      onChange={(event) =>
                        updateSyncDraft("interval_minutes", Number(event.target.value))
                      }
                    >
                      {SYNC_INTERVAL_VALUES.map((value) => (
                        <option key={value} value={value}>
                          {value} {t("分钟")}
                        </option>
                      ))}
                    </select>
                  </label>

                  <label className="checkbox-field sync-startup-field">
                    <input
                      checked={syncDraft.sync_on_startup}
                      disabled={syncControlsDisabled}
                      onChange={(event) => updateSyncDraft("sync_on_startup", event.target.checked)}
                      type="checkbox"
                    />
                    <span>{t("启动后立即同步")}</span>
                  </label>
                </div>

                <div className="detail-stat-list sync-status-list">
                  <div>
                    <span>{t("最近自动同步")}</span>
                    <strong>{lastSyncLabel}</strong>
                  </div>
                  <div>
                    <span>{t("下一次计划")}</span>
                    <strong>{nextSyncLabel}</strong>
                  </div>
                  <div className="sync-result-panel sync-status-message">
                    <span>{t("最近结果")}</span>
                    <strong className="sync-result-text" title={lastResultLabel}>
                      {lastResultLabel}
                    </strong>
                  </div>
                  <div className="sync-result-panel sync-status-message">
                    <span>{t("最近错误")}</span>
                    <strong
                      className={`sync-result-text${syncSettings?.last_error ? " danger-text" : ""}`}
                      title={lastErrorLabel}
                    >
                      {lastErrorLabel}
                    </strong>
                  </div>
                </div>

                <div className="form-actions">
                  <button
                    className="primary"
                    disabled={syncControlsDisabled}
                    onClick={() => void handleSaveSyncSettings()}
                    type="button"
                  >
                    {isSavingSyncSettings ? t("保存中...") : t("保存同步设置")}
                  </button>
                </div>
              </div>
            </section>

            <section className="panel settings-overview-card">
              <div className="panel-heading settings-heading">
                <div>
                  <h2>{t("多设备同步")}</h2>
                  <p>{t("GitHub 加密同步和设备数据包都归入多设备管理，避免配置散落。")}</p>
                </div>
              </div>
              <div className="detail-stat-list settings-overview-list">
                <div>
                  <span>{t("同步通道")}</span>
                  <strong>GitHub</strong>
                </div>
                <div>
                  <span>{t("操作状态")}</span>
                  <strong>{isAppUpdateBusy ? t("更新处理中") : t("可配置")}</strong>
                </div>
                <div>
                  <span>{t("设备数据包")}</span>
                  <strong>{t("独立管理")}</strong>
                </div>
              </div>
              <div className="form-actions settings-overview-actions">
                <button
                  className="primary"
                  disabled={isAppUpdateBusy}
                  onClick={() => setActiveSettingsTab("devices")}
                  type="button"
                >
                  {t("管理多设备")}
                </button>
              </div>
            </section>
          </div>

          <div className="settings-quick-grid" aria-label={t("快捷入口")}>
            <button
              className="settings-shortcut-button"
              onClick={() => setActiveSettingsTab("sources")}
              type="button"
            >
              <strong>{t("数据来源")}</strong>
              <span>{t("管理 Agent 来源和自定义 SQLite。")}</span>
            </button>
            <button
              className="settings-shortcut-button"
              onClick={() => setActiveSettingsTab("devices")}
              type="button"
            >
              <strong>{t("多设备")}</strong>
              <span>{t("配置 GitHub 同步和导入设备包。")}</span>
            </button>
            <button
              className="settings-shortcut-button"
              onClick={() => setActiveSettingsTab("app")}
              type="button"
            >
              <strong>{t("打开应用设置")}</strong>
              <span>{t("数字显示、语言和更新。")}</span>
            </button>
            <button
              className="settings-shortcut-button"
              onClick={() => setActiveSettingsTab("advanced")}
              type="button"
            >
              <strong>{t("高级")}</strong>
              <span>{t("演示数据、全量刷新和导出。")}</span>
            </button>
          </div>
        </section>
      ) : null}

      {activeSettingsTab === "sources" ? (
        <section
          aria-labelledby="settings-tab-sources"
          className="settings-tab-panel settings-section data-sync-section"
          id="settings-panel-sources"
          role="tabpanel"
        >
          <div className="settings-section-heading">
            <div>
              <h2>{t("数据来源")}</h2>
              <p>{t("管理本机 Agent 来源和只读 SQLite 自定义数据源。")}</p>
            </div>
          </div>

          <AgentSourcesPanel
            isDetecting={isDetecting}
            isImporting={isImporting || isSyncing}
            isLoading={isSourcesLoading}
            onDetect={() => void handleDetect()}
            onImport={() => void handleSync()}
            sources={sources}
          />

          <CustomImportersPanel onNotice={setNotice} />
        </section>
      ) : null}

      {activeSettingsTab === "devices" ? (
        <section
          aria-labelledby="settings-tab-devices"
          className="settings-tab-panel settings-section data-portability-section"
          id="settings-panel-devices"
          role="tabpanel"
        >
          <div className="settings-section-heading">
            <div>
              <h2>{t("多设备")}</h2>
              <p>{t("配置 GitHub 加密同步，或导入其他设备导出的本地数据包。")}</p>
            </div>
          </div>

          <div className="sync-layout-grid">
            <GitHubSyncPanel onNotice={setNotice} isAppUpdateBusy={isAppUpdateBusy} />
            <DeviceDatasetsPanel onNotice={setNotice} />
          </div>
        </section>
      ) : null}

      {activeSettingsTab === "app" ? (
        <section
          aria-labelledby="settings-tab-app"
          className="settings-tab-panel settings-section app-preferences-section"
          id="settings-panel-app"
          role="tabpanel"
        >
          <div className="settings-section-heading">
            <div>
              <h2>{t("应用设置")}</h2>
              <p>{t("管理语言、更新和本地显示偏好。")}</p>
            </div>
          </div>

          <div className="settings-two-column settings-app-grid">
            <section className="panel display-preference-card">
              <div className="panel-heading settings-heading">
                <div>
                  <h2>{t("数字显示")}</h2>
                  <p>{t("控制 Token 数值在概览、图表和排行中的展示方式。")}</p>
                </div>
                <div className="segmented display-mode-segmented" aria-label={t("数字显示")}>
                  <button
                    className={numberDisplayMode === "compact" ? "active" : ""}
                    onClick={() => handleNumberDisplayModeChange("compact")}
                    type="button"
                  >
                    {t("缩略显示")}
                  </button>
                  <button
                    className={numberDisplayMode === "full" ? "active" : ""}
                    onClick={() => handleNumberDisplayModeChange("full")}
                    type="button"
                  >
                    {t("完整显示")}
                  </button>
                </div>
              </div>
            </section>

            <section className="panel token-pulse-preference-card">
              <div className="panel-heading settings-heading">
                <div>
                  <h2>{t("常驻小窗")}</h2>
                  <p>{t("控制常驻小窗是否展示 Codex 剩余用量；默认关闭，避免占用小窗空间。")}</p>
                </div>
                <label className="switch-field">
                  <span>{t("显示 Codex 剩余用量")}</span>
                  <input
                    checked={showCodexUsageLimits}
                    onChange={(event) => setShowCodexUsageLimits(event.target.checked)}
                    role="switch"
                    type="checkbox"
                  />
                </label>
              </div>
              <p className="settings-card-note">
                {t("默认关闭。开启后常驻小窗会增加 Codex 5 小时和 1 周剩余额度；关闭时布局自动收缩。")}
              </p>
            </section>

            <section className="panel language-card">
              <div className="panel-heading settings-heading">
                <div>
                  <h2>{t("界面语言")}</h2>
                  <p>{t("跟随系统语言，中文系统默认中文，其他语言默认英文。")}</p>
                </div>
                <label className="language-select">
                  <span>{t("界面语言")}</span>
                  <select
                    value={language}
                    onChange={(event) => handleLanguageChange(event.target.value as AppLanguage)}
                  >
                    <option value="zh-CN">{t("中文")}</option>
                    <option value="en-US">English</option>
                  </select>
                </label>
              </div>
            </section>

            <section className="panel app-update-card">
              <div className="panel-heading settings-heading">
                <div>
                  <h2>{t("应用更新")}</h2>
                  <p>{t("通过 GitHub Releases 检查签名更新包。下载并安装时，Windows 可能会自动关闭当前应用。")}</p>
                </div>
                <button
                  className="primary secondary"
                  disabled={isCheckingUpdate || isInstallingUpdate}
                  onClick={() => void handleCheckForUpdate()}
                  type="button"
                >
                  {isCheckingUpdate ? t("检查中...") : t("检查更新")}
                </button>
              </div>

              <div className="detail-stat-list update-status-list">
                <div>
                  <span>{t("\u66f4\u65b0\u72b6\u6001")}</span>
                  <strong>{updateStatusLabel}</strong>
                </div>
                <div>
                  <span>{t("\u53d1\u5e03\u65f6\u95f4")}</span>
                  <strong>{formatDateTime(updateInfo.date, t("\u65e0"))}</strong>
                </div>
                <div>
                  <span>{t("\u5f53\u524d\u7248\u672c")}</span>
                  <strong>{updateCurrentVersionLabel}</strong>
                </div>
                <div>
                  <span>{t("\u53ef\u7528\u7248\u672c")}</span>
                  <strong>{updateAvailableVersionLabel}</strong>
                </div>
                <div>
                  <span>{t("\u6700\u540e\u68c0\u67e5")}</span>
                  <strong>{updateLastCheckedLabel}</strong>
                </div>
              </div>

              {updateInfo.error ? (
                <p className="update-notes danger-text">{updateInfo.error}</p>
              ) : updateInfo.body ? (
                <p className="update-notes">{updateInfo.body}</p>
              ) : null}

              {isInstallingUpdate || updateProgress.downloaded_bytes > 0 ? (
                <div className="update-progress-block">
                  <div className="update-progress-meta">
                    <span>{t("下载进度")}</span>
                    <strong>{updateProgressPercent}%</strong>
                    <small className="update-progress-bytes">{updateProgressBytesLabel}</small>
                  </div>
                  <div className="update-progress-bar" aria-label={t("下载进度")}>
                    <span style={{ width: `${updateProgressPercent}%` }} />
                  </div>
                </div>
              ) : null}

              <div className="form-actions">
                <button
                  className="primary"
                  disabled={!updateInfo?.available || isCheckingUpdate || isInstallingUpdate}
                  onClick={() => void handleInstallUpdate()}
                  type="button"
                >
                  {isInstallingUpdate ? t("下载并安装中...") : t("下载并安装")}
                </button>
              </div>
            </section>
          </div>
        </section>
      ) : null}

      {activeSettingsTab === "advanced" ? (
        <section
          aria-labelledby="settings-tab-advanced"
          className="settings-tab-panel settings-section advanced-settings-section"
          id="settings-panel-advanced"
          role="tabpanel"
        >
          <div className="settings-section-heading">
            <div>
              <h2>{t("高级")}</h2>
              <p>{t("集中放置低频维护、全量刷新、导出和统计边界说明。")}</p>
            </div>
          </div>

          <section className="panel settings-utility settings-action-strip">
            <div>
              <h2>{t("数据维护")}</h2>
              <p>{t("全量刷新会跳过增量游标重新扫描本机来源；不会删除源端已不存在的历史记录。")}</p>
            </div>
            <div className="utility-actions">
              <button
                className="primary secondary"
                disabled={isSeedLoading}
                onClick={() => void handleSeed()}
                type="button"
              >
                {isSeedLoading ? t("处理中...") : t("生成演示数据")}
              </button>
              <button
                className="primary secondary"
                disabled={isImporting || isSyncing}
                onClick={() => void handleFullSync()}
                type="button"
              >
                {isImporting || isSyncing ? t("刷新中...") : t("全量刷新")}
              </button>
              <button
                className="primary"
                disabled={isExporting}
                onClick={() => void handleExport()}
                type="button"
              >
                {isExporting ? t("导出中...") : t("导出 CSV")}
              </button>
            </div>
          </section>

          <section className="panel settings-utility">
            <div>
              <h2>{t("统计数据范围")}</h2>
              <p>{t("当前只读取本机已有记录和导入后的统计元数据，不保存 prompt、response 或 Authorization。")}</p>
            </div>
            <div className="detail-stat-list">
              <div>
                <span>{t("默认采集方式")}</span>
                <strong>{t("本机数据库读取")}</strong>
              </div>
              <div>
                <span>{t("明文内容")}</span>
                <strong>{t("不保存")}</strong>
              </div>
              <div>
                <span>{t("导出内容")}</span>
                <strong>{t("调用元数据、Token、状态")}</strong>
              </div>
            </div>
          </section>
        </section>
      ) : null}
    </section>
  );
}
