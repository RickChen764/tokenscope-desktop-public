import { useCallback, useEffect, useState } from "react";
import { AgentSourcesPanel } from "./AgentSourcesPanel";
import { CustomImportersPanel } from "./CustomImportersPanel";
import { DeviceDatasetsPanel } from "./DeviceDatasetsPanel";
import {
  detectLocalAgents,
  exportCallsCsv,
  getSyncSettings,
  importDetectedAgents,
  listAgentSources,
  runBackgroundSyncOnce,
  saveSyncSettings,
} from "../services/dashboard";
import type { AgentSourceSummary, SyncSettings, SyncSettingsInput } from "../types/dashboard";
import { formatDateTime } from "../utils/format";

const SYNC_INTERVAL_OPTIONS = [
  { label: "15 分钟", value: 15 },
  { label: "30 分钟", value: 30 },
  { label: "60 分钟", value: 60 },
  { label: "180 分钟", value: 180 },
];

const defaultSyncDraft: SyncSettingsInput = {
  enabled: false,
  interval_minutes: 30,
  sync_on_startup: true,
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
        message: `读取本机 Agent 来源失败：${err instanceof Error ? err.message : String(err)}`,
      });
      return null;
    } finally {
      if (options?.showLoading ?? true) {
        setIsSourcesLoading(false);
      }
    }
  }, []);

  const loadSyncSettings = useCallback(async () => {
    setIsSyncSettingsLoading(true);
    try {
      const settings = await getSyncSettings();
      setSyncSettings(settings);
      setSyncDraft(syncDraftFromSettings(settings));
    } catch (err) {
      setNotice({
        kind: "error",
        message: `读取后台自动同步设置失败：${err instanceof Error ? err.message : String(err)}`,
      });
    } finally {
      setIsSyncSettingsLoading(false);
    }
  }, []);

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
        message: `生成演示数据失败：${err instanceof Error ? err.message : String(err)}`,
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
        message: `本地 Agent 检测完成：发现 ${detectedCount} 个来源，其中 ${syncableCount} 个可同步。`,
      });
    } catch (err) {
      setNotice({
        kind: "error",
        message: `检测失败：${err instanceof Error ? err.message : String(err)}`,
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
          .join("；");
        setNotice({
          kind: "error",
          message: `同步失败：已写入 ${imported} 条，跳过 ${skipped} 条。${errors}`,
        });
        return;
      }
      setNotice({
        kind: "success",
        message: `同步完成：写入 ${imported} 条，跳过 ${skipped} 条。`,
      });
    } catch (err) {
      setNotice({
        kind: "error",
        message: `同步失败：${err instanceof Error ? err.message : String(err)}`,
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
      setNotice({ kind: "success", message: `CSV 已导出：${path}` });
    } catch (err) {
      setNotice({
        kind: "error",
        message: `导出 CSV 失败：${err instanceof Error ? err.message : String(err)}`,
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
          .join("；");
        setNotice({
          kind: "error",
          message: `全量刷新失败：已写入 ${imported} 条，跳过 ${skipped} 条。${errors}`,
        });
        return;
      }
      setNotice({
        kind: "success",
        message: `全量刷新完成：写入 ${imported} 条，跳过 ${skipped} 条。`,
      });
    } catch (err) {
      setNotice({
        kind: "error",
        message: `全量刷新失败：${err instanceof Error ? err.message : String(err)}`,
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
      setNotice({ kind: "success", message: "后台自动同步设置已保存。" });
    } catch (err) {
      setNotice({
        kind: "error",
        message: `保存后台自动同步设置失败：${err instanceof Error ? err.message : String(err)}`,
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
      setNotice({ kind: "success", message: "已触发一次后台同步。" });
      void loadSources({ showLoading: false });
    } catch (err) {
      setNotice({
        kind: "error",
        message: `触发后台同步失败：${err instanceof Error ? err.message : String(err)}`,
      });
    } finally {
      setIsRunningBackgroundSync(false);
    }
  }

  const syncControlsDisabled =
    isSyncSettingsLoading || isSavingSyncSettings || isRunningBackgroundSync;
  const lastSyncLabel = isSyncSettingsLoading
    ? "读取中..."
    : formatDateTime(syncSettings?.last_sync_at ?? null);
  const nextSyncLabel = isSyncSettingsLoading
    ? "读取中..."
    : syncSettings?.enabled
      ? formatDateTime(syncSettings.next_sync_at)
      : "未启用";
  const lastResultLabel = isSyncSettingsLoading
    ? "读取中..."
    : syncSettings?.last_result || "尚未执行";
  const lastErrorLabel = isSyncSettingsLoading
    ? "读取中..."
    : syncSettings?.last_error || "无";

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

      <section className="panel sync-settings-card" aria-busy={isSyncSettingsLoading}>
        <div className="panel-heading settings-heading">
          <div>
            <p className="eyebrow">Background Sync</p>
            <h2>后台自动同步</h2>
            <p>按固定间隔自动同步本机 Agent 来源，也可以手动触发一次后台同步。</p>
          </div>
          <button
            className="primary secondary"
            disabled={syncControlsDisabled}
            onClick={() => void handleRunBackgroundSyncOnce()}
            type="button"
          >
            {isRunningBackgroundSync ? "同步中..." : "立即同步一次"}
          </button>
        </div>

        <div className="settings-form">
          <div className="sync-control-grid">
            <label className="switch-field">
              <span>启用后台自动同步</span>
              <input
                checked={syncDraft.enabled}
                disabled={syncControlsDisabled}
                onChange={(event) => updateSyncDraft("enabled", event.target.checked)}
                role="switch"
                type="checkbox"
              />
            </label>

            <label className="field sync-interval-field">
              <span>同步间隔</span>
              <select
                disabled={syncControlsDisabled}
                value={syncDraft.interval_minutes}
                onChange={(event) =>
                  updateSyncDraft("interval_minutes", Number(event.target.value))
                }
              >
                {SYNC_INTERVAL_OPTIONS.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
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
              <span>启动后立即同步</span>
            </label>
          </div>

          <div className="detail-stat-list sync-status-list">
            <div>
              <span>最近自动同步</span>
              <strong>{lastSyncLabel}</strong>
            </div>
            <div>
              <span>下一次计划</span>
              <strong>{nextSyncLabel}</strong>
            </div>
            <div>
              <span>最近结果</span>
              <strong>{lastResultLabel}</strong>
            </div>
            <div>
              <span>最近错误</span>
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
              {isSavingSyncSettings ? "保存中..." : "保存同步设置"}
            </button>
          </div>
        </div>
      </section>

      <section className="settings-grid compact-grid">
        <section className="panel settings-utility">
          <div>
            <p className="eyebrow">Data Tools</p>
            <h2>数据维护</h2>
            <p>手动同步本机数据后，可在上方查看来源路径、最近导入、最近调用和导入量。</p>
          </div>
          <div className="utility-actions">
            <button
              className="primary secondary"
              disabled={isSeedLoading}
              onClick={() => void handleSeed()}
              type="button"
            >
              {isSeedLoading ? "处理中..." : "生成演示数据"}
            </button>
            <button
              className="primary secondary"
              disabled={isImporting || isSyncing}
              onClick={() => void handleFullSync()}
              type="button"
            >
              {isImporting || isSyncing ? "刷新中..." : "全量刷新"}
            </button>
            <button
              className="primary"
              disabled={isExporting}
              onClick={() => void handleExport()}
              type="button"
            >
              {isExporting ? "导出中..." : "导出 CSV"}
            </button>
          </div>
        </section>

        <section className="panel settings-utility">
          <div>
            <p className="eyebrow">Privacy Boundary</p>
            <h2>统计数据范围</h2>
            <p>当前只读取本机已有记录和导入后的统计元数据，不保存 prompt、response 或 Authorization。</p>
          </div>
          <div className="detail-stat-list">
            <div>
              <span>默认采集方式</span>
              <strong>本机数据库读取</strong>
            </div>
            <div>
              <span>明文内容</span>
              <strong>不保存</strong>
            </div>
            <div>
              <span>导出内容</span>
              <strong>调用元数据、Token、状态</strong>
            </div>
          </div>
        </section>
      </section>
    </section>
  );
}
