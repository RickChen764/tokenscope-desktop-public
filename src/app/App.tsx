import { useCallback, useEffect, useMemo, useState } from "react";
import { CallsPage } from "../components/CallsPage";
import { DataHealthPage } from "../components/DataHealthPage";
import { DimensionDetailPage } from "../components/DimensionDetailPage";
import { DimensionIndexPage } from "../components/DimensionIndexPage";
import { MiniSeriesChart } from "../components/MiniSeriesChart";
import { ReportsPage } from "../components/ReportsPage";
import { SettingsPage } from "../components/SettingsPage";
import { SummaryCards } from "../components/SummaryCards";
import {
  TokenPulseDetailWindow,
  TokenPulseWindow,
} from "../components/TokenPulseWindow";
import { TopList } from "../components/TopList";
import { ToastNotice, type ToastNoticeValue } from "../components/ToastNotice";
import { useQuietModeStatus } from "../hooks/useQuietMode";
import { useI18n } from "../i18n";
import {
  APP_UPDATE_INFO_EVENT,
  checkForAppUpdate,
  clearDemoData,
  getDailyUsageSeries,
  getDashboardSummaryForDates,
  getStoredAppUpdateInfo,
  getTopAgents,
  getTopModels,
  getTopProjects,
  getTopProviders,
  getTopSessions,
  getTopWorkflows,
  importDetectedAgents,
  installPendingAppUpdate,
  isDemoDataEnabled,
  listAgentSources,
  seedDemoData,
} from "../services/dashboard";
import { appUpdateVersionRange } from "../services/appUpdateState";
import type {
  AgentSourceSummary,
  AppUpdateInfo,
  DashboardRange,
  DashboardSummary,
  DailyUsagePoint,
  DimensionKind,
  TopDimensionRow,
} from "../types/dashboard";
import { getLocalDateWindow } from "../utils/date";
import { formatDateTime, formatInteger } from "../utils/format";

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

type DashboardRangeMode = DashboardRange | "custom";

type DashboardView =
  | "overview"
  | "health"
  | "reports"
  | "dimensions"
  | "calls"
  | "settings";

const isTokenPulseWindow =
  typeof window !== "undefined" &&
  new URLSearchParams(window.location.search).get("tokenPulse") === "1";
const isTokenPulseDetailWindow =
  typeof window !== "undefined" &&
  new URLSearchParams(window.location.search).get("tokenPulseDetail") === "1";
const APP_UPDATE_AUTO_CHECK_INTERVAL_MS = 30 * 60_000;

function getInitialAppUpdateInfo() {
  const storedInfo = getStoredAppUpdateInfo();

  if (
    import.meta.env.DEV &&
    typeof window !== "undefined" &&
    (window.location.hostname === "127.0.0.1" ||
      window.location.hostname === "localhost")
  ) {
    const params = new URLSearchParams(window.location.search);
    if (params.get("mockUpdate") === "1") {
      const now = new Date().toISOString();
      return {
        ...storedInfo,
        available: true,
        current_version: storedInfo.current_version ?? "0.1.4",
        version: params.get("mockUpdateVersion") ?? "0.1.5",
        date: now,
        body: "模拟更新：左侧栏会显示升级入口，悬浮后展示版本、发布时间和这段说明。此数据仅用于预览 UI，不会触发真实下载。",
        status: "available" as const,
        checked_at: now,
        error: null,
      };
    }
  }

  return storedInfo;
}

export function App() {
  if (isTokenPulseWindow) {
    return <TokenPulseWindow />;
  }

  if (isTokenPulseDetailWindow) {
    return <TokenPulseDetailWindow />;
  }

  const { numberLocale, t } = useI18n();
  const quietMode = useQuietModeStatus();
  const [view, setView] = useState<DashboardView>("overview");
  const [range, setRange] = useState<DashboardRangeMode>("30d");
  const initialCustomWindow = useMemo(() => getLocalDateWindow("90d"), []);
  const [customFrom, setCustomFrom] = useState(initialCustomWindow.from);
  const [customTo, setCustomTo] = useState(initialCustomWindow.to);
  const [summary, setSummary] = useState<DashboardSummary>(emptySummary);
  const [series, setSeries] = useState<DailyUsagePoint[]>([]);
  const [agentSeries, setAgentSeries] = useState<DailyUsagePoint[]>([]);
  const [topAgentRows, setTopAgentRows] = useState<TopDimensionRow[]>([]);
  const [models, setModels] = useState<TopDimensionRow[]>([]);
  const [providers, setProviders] = useState<TopDimensionRow[]>([]);
  const [workflows, setWorkflows] = useState<TopDimensionRow[]>([]);
  const [projects, setProjects] = useState<TopDimensionRow[]>([]);
  const [sessions, setSessions] = useState<TopDimensionRow[]>([]);
  const [agentSources, setAgentSources] = useState<AgentSourceSummary[]>([]);
  const [dimensionDetail, setDimensionDetail] = useState<{
    kind: DimensionKind;
    value: string;
  } | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isSourceStatusLoading, setIsSourceStatusLoading] = useState(true);
  const [isSyncing, setIsSyncing] = useState(false);
  const [appUpdateInfo, setAppUpdateInfo] = useState<AppUpdateInfo>(() =>
    getInitialAppUpdateInfo(),
  );
  const [isInstallingAppUpdate, setIsInstallingAppUpdate] = useState(false);
  const [syncVersion, setSyncVersion] = useState(0);
  const [notice, setNotice] = useState<ToastNoticeValue | null>(null);

  const dateWindow = useMemo(
    () =>
      range === "custom"
        ? { from: customFrom, to: customTo }
        : getLocalDateWindow(range),
    [customFrom, customTo, range],
  );
  const isDateWindowValid =
    Boolean(dateWindow.from && dateWindow.to) &&
    dateWindow.from <= dateWindow.to;

  const loadDashboard = useCallback(
    async (options?: { clearNotice?: boolean }) => {
      setIsLoading(true);
      if (options?.clearNotice ?? true) {
        setNotice(null);
      }

      if (!isDateWindowValid) {
        setNotice({
          kind: "error",
          message: t("请选择完整日期区间，且起始日期不能晚于结束日期。"),
        });
        setIsLoading(false);
        return false;
      }

      try {
        const [
          nextSummary,
          nextSeries,
          nextAgentSeries,
          nextAgents,
          nextModels,
          nextProviders,
          nextWorkflows,
          nextProjects,
          nextSessions,
        ] = await Promise.all([
          getDashboardSummaryForDates(dateWindow.from, dateWindow.to),
          getDailyUsageSeries(dateWindow.from, dateWindow.to),
          getDailyUsageSeries(dateWindow.from, dateWindow.to, "agent"),
          getTopAgents(dateWindow.from, dateWindow.to, 5),
          getTopModels(dateWindow.from, dateWindow.to, 5),
          getTopProviders(dateWindow.from, dateWindow.to, 5),
          getTopWorkflows(dateWindow.from, dateWindow.to, 5),
          getTopProjects(dateWindow.from, dateWindow.to, 5),
          getTopSessions(dateWindow.from, dateWindow.to, 5),
        ]);

        setSummary(nextSummary);
        setSeries(nextSeries);
        setAgentSeries(nextAgentSeries);
        setTopAgentRows(nextAgents);
        setModels(nextModels);
        setProviders(nextProviders);
        setWorkflows(nextWorkflows);
        setProjects(nextProjects);
        setSessions(nextSessions);
        return true;
      } catch (err) {
        setNotice({
          kind: "error",
          message: t("加载仪表盘失败：{error}", {
            error: err instanceof Error ? err.message : String(err),
          }),
        });
        return false;
      } finally {
        setIsLoading(false);
      }
    },
    [dateWindow.from, dateWindow.to, isDateWindowValid, t],
  );

  const loadAgentSourceStatus = useCallback(async () => {
    setIsSourceStatusLoading(true);
    try {
      const sources = await listAgentSources();
      setAgentSources(sources);
    } finally {
      setIsSourceStatusLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadDashboard();
  }, [loadDashboard]);

  useEffect(() => {
    void loadAgentSourceStatus();
  }, [loadAgentSourceStatus]);

  useEffect(() => {
    function handleAppUpdateInfo(event: Event) {
      const detail = (event as CustomEvent<AppUpdateInfo>).detail;
      setAppUpdateInfo(detail ?? getStoredAppUpdateInfo());
    }

    window.addEventListener(APP_UPDATE_INFO_EVENT, handleAppUpdateInfo);
    return () =>
      window.removeEventListener(APP_UPDATE_INFO_EVENT, handleAppUpdateInfo);
  }, []);

  useEffect(() => {
    if (quietMode.active) {
      return;
    }

    let isDisposed = false;
    let isRunning = false;

    async function runScheduledAppUpdateCheck() {
      if (isDisposed || isRunning) {
        return;
      }

      const currentStatus = getStoredAppUpdateInfo().status;
      if (["checking", "downloading", "installing"].includes(currentStatus)) {
        return;
      }

      isRunning = true;
      try {
        await checkForAppUpdate();
      } catch {
        // Automatic checks update stored state; the Settings page surfaces details.
      } finally {
        isRunning = false;
      }
    }

    void runScheduledAppUpdateCheck();
    const intervalId = window.setInterval(
      () => void runScheduledAppUpdateCheck(),
      APP_UPDATE_AUTO_CHECK_INTERVAL_MS,
    );

    return () => {
      isDisposed = true;
      window.clearInterval(intervalId);
    };
  }, [quietMode.active]);

  async function handleInstallAppUpdate() {
    setIsInstallingAppUpdate(true);
    setNotice(null);
    setAppUpdateInfo((current) => ({
      ...current,
      status: "downloading",
      error: null,
    }));

    try {
      await installPendingAppUpdate((progress) => {
        setAppUpdateInfo((current) => ({
          ...current,
          status: progress.event === "Finished" ? "installing" : "downloading",
          error: null,
        }));
      });
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setAppUpdateInfo((current) => ({
        ...current,
        status: "error",
        error: message,
      }));
      setNotice({
        kind: "error",
        message: t("\u5b89\u88c5\u66f4\u65b0\u5931\u8d25\uff1a{error}", {
          error: message,
        }),
      });
      setIsInstallingAppUpdate(false);
    }
  }

  async function handleSyncLocalData() {
    setIsSyncing(true);
    setNotice(null);
    try {
      const results = await importDetectedAgents("incremental");
      const clearedDemoRows = await clearDemoData();
      const imported = results.reduce(
        (total, result) => total + result.imported,
        0,
      );
      const skipped = results.reduce(
        (total, result) => total + result.skipped,
        0,
      );
      const refreshed = await loadDashboard({ clearNotice: false });
      await loadAgentSourceStatus();
      setSyncVersion((value) => value + 1);
      setView("overview");
      setDimensionDetail(null);
      if (refreshed) {
        const cleanupText =
          clearedDemoRows > 0
            ? t("，清理演示数据 {count} 条", { count: clearedDemoRows })
            : "";
        setNotice({
          kind: "success",
          message: t(
            "本机数据已同步：写入 {imported} 条，跳过 {skipped} 条{cleanupText}。",
            {
              cleanupText,
              imported,
              skipped,
            },
          ),
        });
      }
    } catch (err) {
      setNotice({
        kind: "error",
        message: t("同步本机数据失败：{error}", {
          error: err instanceof Error ? err.message : String(err),
        }),
      });
    } finally {
      setIsSyncing(false);
    }
  }

  async function handleSeed() {
    setIsLoading(true);
    setNotice(null);
    try {
      await seedDemoData();
      const refreshed = await loadDashboard({ clearNotice: false });
      if (refreshed) {
        setNotice({
          kind: "success",
          message: t("演示数据已生成，仪表盘已刷新。"),
        });
      }
    } catch (err) {
      setNotice({
        kind: "error",
        message: t("生成演示数据失败：{error}", {
          error: err instanceof Error ? err.message : String(err),
        }),
      });
      setIsLoading(false);
    }
  }

  function openDimensionDetail(kind: DimensionKind, value: string) {
    setDimensionDetail({ kind, value });
    setView("dimensions");
  }

  function handleNavChange(nextView: DashboardView) {
    setView(nextView);
    if (nextView === "dimensions") {
      setDimensionDetail(null);
    }
  }

  function openDimensionIndex() {
    setDimensionDetail(null);
    setView("dimensions");
  }

  const rangeLabels: Record<DashboardRange, string> = {
    today: t("今日"),
    "7d": t("近 7 天"),
    "30d": t("近 30 天"),
    "90d": t("近 90 天"),
  };
  const rangeModeLabels: Record<DashboardRangeMode, string> = {
    ...rangeLabels,
    custom: t("自定义"),
  };
  const navItems: Array<{ id: DashboardView; label: string }> = [
    { id: "overview", label: t("概览") },
    { id: "health", label: t("健康") },
    { id: "reports", label: t("报表") },
    { id: "dimensions", label: t("分析") },
    { id: "calls", label: t("调用") },
    { id: "settings", label: t("设置") },
  ];
  const viewTitles: Record<DashboardView, string> = {
    overview: t("用量仪表盘"),
    health: t("数据健康"),
    reports: t("报表导出"),
    dimensions: t("维度分析"),
    calls: t("调用明细"),
    settings: t("偏好设置"),
  };
  const showRangeSelector = view === "overview";
  const activeTitle =
    view === "dimensions" && dimensionDetail ? t("维度详情") : viewTitles[view];
  const latestImportedAt = latestDateTime(
    agentSources.map((source) => source.last_imported_at),
  );
  const localImportedRows = agentSources.reduce(
    (total, source) => total + source.imported_calls,
    0,
  );
  const hasSyncedData = localImportedRows > 0 || Boolean(latestImportedAt);
  const syncStatusLabel = isSourceStatusLoading
    ? t("读取中...")
    : hasSyncedData
      ? t("已同步")
      : t("未同步");
  const lastSyncLabel = isSourceStatusLoading
    ? t("读取中...")
    : formatRelativeDateTime(latestImportedAt, t);
  const localRowsLabel = isSourceStatusLoading
    ? t("读取中...")
    : formatInteger(localImportedRows, numberLocale);
  const shouldShowUpdateRail =
    appUpdateInfo.available &&
    ["available", "downloading", "installing", "error"].includes(
      appUpdateInfo.status,
    );
  const appUpdateStatusLabel =
    appUpdateInfo.status === "downloading"
      ? t("下载中...")
      : appUpdateInfo.status === "installing"
        ? t("安装中...")
        : appUpdateInfo.status === "error"
          ? t("检查失败")
          : t("可更新");
  const appUpdateActionLabel =
    appUpdateInfo.status === "downloading" ||
    appUpdateInfo.status === "installing"
      ? t("处理中...")
      : t("升级");
  const appUpdateBodyLabel =
    appUpdateInfo.error ||
    appUpdateInfo.body ||
    t("发现新版本，可以下载并安装。");
  const appUpdateDateLabel = formatDateTime(appUpdateInfo.date, t("无"));
  const appUpdateVersionLabel =
    appUpdateVersionRange(
      appUpdateInfo.current_version,
      appUpdateInfo.version,
    ) ?? t("可更新");

  return (
    <main className="app-frame">
      <aside className="side-rail" aria-label={t("主导航")}>
        <div className="rail-logo">TS</div>
        <nav className="rail-nav">
          {navItems.map((item) => (
            <button
              className={view === item.id ? "active" : ""}
              key={item.id}
              onClick={() => handleNavChange(item.id)}
              type="button"
            >
              {item.label}
            </button>
          ))}
        </nav>

        {shouldShowUpdateRail ? (
          <section
            className={`update-status-rail ${appUpdateInfo.status}`}
            aria-describedby="update-status-popover"
            aria-label={t("应用更新")}
            tabIndex={0}
          >
            <button
              className="update-status-button"
              disabled={
                isInstallingAppUpdate ||
                appUpdateInfo.status === "downloading" ||
                appUpdateInfo.status === "installing"
              }
              onClick={() => void handleInstallAppUpdate()}
              type="button"
            >
              <span>{appUpdateStatusLabel}</span>
              <strong>{appUpdateActionLabel}</strong>
            </button>
            <div
              className="update-status-popover"
              id="update-status-popover"
              role="status"
            >
              <div>
                <span>{t("可用版本")}</span>
                <strong>{appUpdateVersionLabel}</strong>
              </div>
              <div>
                <span>{t("发布时间")}</span>
                <strong>{appUpdateDateLabel}</strong>
              </div>
              <p>{appUpdateBodyLabel}</p>
            </div>
          </section>
        ) : null}

        <section
          className={`sync-status-rail ${hasSyncedData ? "synced" : "idle"}`}
          aria-busy={isSourceStatusLoading}
          aria-describedby="sync-status-popover"
          aria-label={t("同步状态")}
          tabIndex={0}
        >
          <div className="rail-status-compact">
            <i aria-hidden="true" />
            <span>{syncStatusLabel}</span>
            <strong>{localRowsLabel}</strong>
          </div>
          <div
            className="rail-status-popover"
            id="sync-status-popover"
            role="status"
          >
            <div>
              <span>{t("同步状态")}</span>
              <strong>
                <i aria-hidden="true" />
                {syncStatusLabel}
              </strong>
            </div>
            <div>
              <span>{t("最后同步")}</span>
              <strong>{lastSyncLabel}</strong>
            </div>
            <div>
              <span>{t("本机数据")}</span>
              <strong>
                {localRowsLabel} {t("条")}
              </strong>
            </div>
          </div>
        </section>
      </aside>

      <section className="workspace">
        <header className="topbar">
          <div className="topbar-title">
            <h1>{activeTitle}</h1>
          </div>
          <div className="actions">
            <button
              className="primary secondary"
              disabled={isSyncing}
              onClick={() => void handleSyncLocalData()}
              type="button"
            >
              {isSyncing ? t("同步中...") : t("同步本机数据")}
            </button>
            {showRangeSelector ? (
              <div className="range-control-group">
                <div
                  className="segmented range-segmented"
                  aria-label={t("日期范围")}
                >
                  {(
                    [
                      "today",
                      "7d",
                      "30d",
                      "90d",
                      "custom",
                    ] as DashboardRangeMode[]
                  ).map((option) => (
                    <button
                      className={option === range ? "active" : ""}
                      key={option}
                      onClick={() => setRange(option)}
                      type="button"
                    >
                      {rangeModeLabels[option]}
                    </button>
                  ))}
                </div>
                {range === "custom" ? (
                  <div
                    className="date-range-picker"
                    aria-label={t("自定义日期范围")}
                  >
                    <label>
                      <span>{t("开始")}</span>
                      <input
                        max={customTo}
                        onChange={(event) => setCustomFrom(event.target.value)}
                        required
                        type="date"
                        value={customFrom}
                      />
                    </label>
                    <label>
                      <span>{t("结束")}</span>
                      <input
                        min={customFrom}
                        onChange={(event) => setCustomTo(event.target.value)}
                        required
                        type="date"
                        value={customTo}
                      />
                    </label>
                  </div>
                ) : null}
              </div>
            ) : null}
          </div>
        </header>

        <ToastNotice notice={notice} onClose={() => setNotice(null)} />

        {view === "overview" ? (
          <>
            <SummaryCards isLoading={isLoading} summary={summary} />

            <section className="overview-focus" aria-label={t("每日用量趋势")}>
              <MiniSeriesChart
                agentPoints={agentSeries}
                isLoading={isLoading}
                points={series}
              />
            </section>

            <section className="overview-secondary" aria-label={t("排行分析")}>
              <div className="top-lists">
                <TopList
                  dimensionLabel="Agent"
                  footerLabel={t("进入分析")}
                  isLoading={isLoading}
                  kind="agent"
                  maxRows={5}
                  onRowClick={(value) => openDimensionDetail("agent", value)}
                  onViewAll={openDimensionIndex}
                  rows={topAgentRows}
                  title={t("Agent 排行")}
                  variant="overview"
                />
                <TopList
                  dimensionLabel={t("模型")}
                  footerLabel={t("进入分析")}
                  isLoading={isLoading}
                  kind="model"
                  maxRows={5}
                  onRowClick={(value) => openDimensionDetail("model", value)}
                  onViewAll={openDimensionIndex}
                  rows={models}
                  title={t("模型排行")}
                  variant="overview"
                />
                <TopList
                  dimensionLabel="Provider"
                  footerLabel={t("进入分析")}
                  isLoading={isLoading}
                  kind="provider"
                  maxRows={5}
                  onRowClick={(value) => openDimensionDetail("provider", value)}
                  onViewAll={openDimensionIndex}
                  rows={providers}
                  title={t("Provider 排行")}
                  variant="overview"
                />
                <TopList
                  dimensionLabel={t("项目")}
                  footerLabel={t("进入分析")}
                  isLoading={isLoading}
                  kind="project"
                  maxRows={5}
                  onRowClick={(value) => openDimensionDetail("project", value)}
                  onViewAll={openDimensionIndex}
                  rows={projects}
                  title={t("项目排行")}
                  variant="overview"
                />
                <TopList
                  dimensionLabel={t("会话")}
                  footerLabel={t("进入分析")}
                  isLoading={isLoading}
                  kind="session"
                  maxRows={5}
                  onRowClick={(value) => openDimensionDetail("session", value)}
                  onViewAll={openDimensionIndex}
                  rows={sessions}
                  title={t("会话排行")}
                  variant="overview"
                />
              </div>
            </section>
          </>
        ) : null}

        {view === "health" ? <DataHealthPage key={syncVersion} /> : null}

        {view === "reports" ? <ReportsPage /> : null}

        {view === "dimensions" && !dimensionDetail ? (
          <DimensionIndexPage
            agents={topAgentRows}
            isLoading={isLoading}
            models={models}
            onOpenDetail={openDimensionDetail}
            projects={projects}
            providers={providers}
            sessions={sessions}
            workflows={workflows}
          />
        ) : null}

        {view === "dimensions" && dimensionDetail ? (
          <DimensionDetailPage
            kind={dimensionDetail.kind}
            onBack={() => setDimensionDetail(null)}
            value={dimensionDetail.value}
          />
        ) : null}

        {view === "calls" ? <CallsPage key={syncVersion} /> : null}

        {view === "settings" ? (
          <SettingsPage
            isDemoDataEnabled={isDemoDataEnabled()}
            isSeedLoading={isLoading}
            isSyncing={isSyncing}
            onSeedDemoData={handleSeed}
          />
        ) : null}
      </section>
    </main>
  );
}

function latestDateTime(values: Array<string | null>) {
  const sortedValues = values
    .filter((value): value is string => Boolean(value))
    .sort();
  return sortedValues.length > 0 ? sortedValues[sortedValues.length - 1] : null;
}

function formatRelativeDateTime(
  value: string | null,
  t: (message: string, params?: Record<string, string | number>) => string,
) {
  if (!value) {
    return t("尚未同步");
  }

  const timestamp = Date.parse(value);
  if (Number.isNaN(timestamp)) {
    return formatDateTime(value, t("无"));
  }

  const diffMinutes = Math.max(0, Math.floor((Date.now() - timestamp) / 60000));
  if (diffMinutes < 1) {
    return t("刚刚");
  }
  if (diffMinutes < 60) {
    return t("{count} 分钟前", { count: diffMinutes });
  }

  const diffHours = Math.floor(diffMinutes / 60);
  if (diffHours < 24) {
    return t("{count} 小时前", { count: diffHours });
  }

  const diffDays = Math.floor(diffHours / 24);
  if (diffDays <= 7) {
    return t("{count} 天前", { count: diffDays });
  }

  return formatDateTime(value, t("无"));
}
