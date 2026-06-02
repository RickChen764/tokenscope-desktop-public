import { useCallback, useEffect, useState } from "react";
import { AgentSourcesPanel } from "./AgentSourcesPanel";
import { CustomImportersPanel } from "./CustomImportersPanel";
import { DeviceDatasetsPanel } from "./DeviceDatasetsPanel";
import {
  checkForAppUpdate,
  detectLocalAgents,
  exportCallsCsv,
  getSyncSettings,
  importDetectedAgents,
  installPendingAppUpdate,
  listAgentSources,
  runBackgroundSyncOnce,
  saveSyncSettings,
} from "../services/dashboard";
import { useI18n, type AppLanguage } from "../i18n";
import type {
  AgentSourceSummary,
  AppUpdateInfo,
  AppUpdateProgress,
  SyncSettings,
  SyncSettingsInput,
} from "../types/dashboard";
import { formatDateTime } from "../utils/format";

const SYNC_INTERVAL_VALUES = [15, 30, 60, 180];

const defaultSyncDraft: SyncSettingsInput = {
  enabled: false,
  interval_minutes: 30,
  sync_on_startup: true,
};

const emptyUpdateProgress: AppUpdateProgress = {
  event: "Started",
  downloaded_bytes: 0,
  content_length: null,
};

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
  onSyncLocalData: () => Promise<void>;
}

export function SettingsPage({
  isSeedLoading,
  isSyncing,
  onSeedDemoData,
}: SettingsPageProps) {
  const { language, setLanguage, t } = useI18n();
  const [sources, setSources] = useState<AgentSourceSummary[]>([]);
  const [isSourcesLoading, setIsSourcesLoading] = useState(true);
  const [isDetecting, setIsDetecting] = useState(false);
  const [isImporting, setIsImporting] = useState(false);
  const [isExporting, setIsExporting] = useState(false);
  const [syncSettings, setSyncSettings] = useState<SyncSettings | null>(null);
  const [syncDraft, setSyncDraft] = useState<SyncSettingsInput>(defaultSyncDraft);
  const [isSyncSettingsLoading, setIsSyncSettingsLoading] = useState(true);
  const [isSavingSyncSettings, setIsSavingSyncSettings] = useState(false);
  const [isRunningBackgroundSync, setIsRunningBackgroundSync] = useState(false);
  const [updateInfo, setUpdateInfo] = useState<AppUpdateInfo | null>(null);
  const [isCheckingUpdate, setIsCheckingUpdate] = useState(false);
  const [isInstallingUpdate, setIsInstallingUpdate] = useState(false);
  const [updateProgress, setUpdateProgress] =
    useState<AppUpdateProgress>(emptyUpdateProgress);
  const [notice, setNotice] = useState<{ kind: "error" | "success"; message: string } | null>(
    null,
  );

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

  const loadSyncSettings = useCallback(async () => {
    setIsSyncSettingsLoading(true);
    try {
      const settings = await getSyncSettings();
      setSyncSettings(settings);
      setSyncDraft(syncDraftFromSettings(settings));
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
  }, [t]);

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
      setNotice({
        kind: "success",
        message: t("同步完成：写入 {imported} 条，跳过 {skipped} 条。", {
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
      setNotice({
        kind: "success",
        message: t("全量刷新完成：写入 {imported} 条，跳过 {skipped} 条。", {
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

  function updateSyncDraft<K extends keyof SyncSettingsInput>(key: K, value: SyncSettingsInput[K]) {
    setSyncDraft((current) => ({ ...current, [key]: value }));
  }

  async function handleSaveSyncSettings() {
    setIsSavingSyncSettings(true);
    setNotice(null);
    try {
      const settings = await saveSyncSettings(syncDraft);
      setSyncSettings(settings);
      setSyncDraft(syncDraftFromSettings(settings));
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
      setSyncSettings(settings);
      setSyncDraft(syncDraftFromSettings(settings));
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
    try {
      const nextUpdateInfo = await checkForAppUpdate();
      setUpdateInfo(nextUpdateInfo);
      setNotice({
        kind: "success",
        message: nextUpdateInfo.available
          ? t("发现新版本 {version}，可以下载并安装。", {
              version: nextUpdateInfo.version ?? "",
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
    } finally {
      setIsCheckingUpdate(false);
    }
  }

  async function handleInstallUpdate() {
    setIsInstallingUpdate(true);
    setNotice(null);
    setUpdateProgress(emptyUpdateProgress);
    try {
      await installPendingAppUpdate((progress) => {
        setUpdateProgress(progress);
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
      setIsInstallingUpdate(false);
    }
  }

  const syncControlsDisabled =
    isSyncSettingsLoading || isSavingSyncSettings || isRunningBackgroundSync;
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
  const updateVersionLabel = updateInfo?.available
    ? `${updateInfo.current_version || t("当前版本")} → ${updateInfo.version}`
    : updateInfo
      ? t("当前已是最新版本")
      : t("尚未检查");

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

  return (
    <section className="settings-page">
      {notice ? <div className={`notice ${notice.kind} inline-notice`}>{notice.message}</div> : null}

      <AgentSourcesPanel
        isDetecting={isDetecting}
        isImporting={isImporting || isSyncing}
        isLoading={isSourcesLoading}
        onDetect={() => void handleDetect()}
        onImport={() => void handleSync()}
        sources={sources}
      />

      <CustomImportersPanel onNotice={setNotice} />

      <DeviceDatasetsPanel onNotice={setNotice} />

      <section className="panel language-card">
        <div className="panel-heading settings-heading">
          <div>
            <p className="eyebrow">Language</p>
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
            <p className="eyebrow">App Update</p>
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
            <span>{t("更新状态")}</span>
            <strong>{updateVersionLabel}</strong>
          </div>
          <div>
            <span>{t("发布时间")}</span>
            <strong>{formatDateTime(updateInfo?.date ?? null, t("无"))}</strong>
          </div>
        </div>

        {updateInfo?.body ? <p className="update-notes">{updateInfo.body}</p> : null}

        {isInstallingUpdate || updateProgress.downloaded_bytes > 0 ? (
          <div className="update-progress-block">
            <div className="update-progress-meta">
              <span>{t("下载进度")}</span>
              <strong>{updateProgressPercent}%</strong>
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

      <section className="panel sync-settings-card" aria-busy={isSyncSettingsLoading}>
        <div className="panel-heading settings-heading">
          <div>
            <p className="eyebrow">Background Sync</p>
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

        <div className="settings-form">
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
            <div>
              <span>{t("最近结果")}</span>
              <strong>{lastResultLabel}</strong>
            </div>
            <div>
              <span>{t("最近错误")}</span>
              <strong className={syncSettings?.last_error ? "danger-text" : ""}>
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

      <section className="settings-grid compact-grid">
        <section className="panel settings-utility">
          <div>
            <p className="eyebrow">Data Tools</p>
            <h2>{t("数据维护")}</h2>
            <p>{t("手动同步本机数据后，可在上方查看来源路径、最近导入、最近调用和导入量。")}</p>
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
            <p className="eyebrow">Privacy Boundary</p>
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
    </section>
  );
}
