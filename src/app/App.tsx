import { useCallback, useEffect, useMemo, useState } from "react";
import { CallsPage } from "../components/CallsPage";
import { DataHealthPage } from "../components/DataHealthPage";
import { DimensionDetailPage } from "../components/DimensionDetailPage";
import { DimensionIndexPage } from "../components/DimensionIndexPage";
import { MiniSeriesChart } from "../components/MiniSeriesChart";
import { ReportsPage } from "../components/ReportsPage";
import { SettingsPage } from "../components/SettingsPage";
import { SummaryCards } from "../components/SummaryCards";
import { TopList } from "../components/TopList";
import {
  clearDemoData,
  getDailyUsageSeries,
  getDashboardSummaryForDates,
  getTopAgents,
  getTopModels,
  getTopProjects,
  getTopProviders,
  getTopSessions,
  getTopWorkflows,
  importDetectedAgents,
  seedDemoData,
} from "../services/dashboard";
import type {
  DashboardRange,
  DashboardSummary,
  DailyUsagePoint,
  DimensionKind,
  TopDimensionRow,
} from "../types/dashboard";
import { getLocalDateWindow } from "../utils/date";

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

const rangeLabels: Record<DashboardRange, string> = {
  today: "今日",
  "7d": "近 7 天",
  "30d": "近 30 天",
  "90d": "近 90 天",
};

type DashboardRangeMode = DashboardRange | "custom";

const rangeModeLabels: Record<DashboardRangeMode, string> = {
  ...rangeLabels,
  custom: "自定义",
};

type DashboardView =
  | "overview"
  | "health"
  | "reports"
  | "dimensions"
  | "calls"
  | "settings";

const navItems: Array<{ id: DashboardView; label: string }> = [
  { id: "overview", label: "概览" },
  { id: "health", label: "健康" },
  { id: "reports", label: "报表" },
  { id: "dimensions", label: "分析" },
  { id: "calls", label: "调用" },
  { id: "settings", label: "设置" },
];

const viewTitles: Record<DashboardView, string> = {
  overview: "用量仪表盘",
  health: "数据健康",
  reports: "报表导出",
  dimensions: "维度分析",
  calls: "调用明细",
  settings: "偏好设置",
};

export function App() {
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
  const [dimensionDetail, setDimensionDetail] = useState<{
    kind: DimensionKind;
    value: string;
  } | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isSyncing, setIsSyncing] = useState(false);
  const [syncVersion, setSyncVersion] = useState(0);
  const [notice, setNotice] = useState<{ kind: "error" | "success"; message: string } | null>(null);

  const dateWindow = useMemo(
    () => (range === "custom" ? { from: customFrom, to: customTo } : getLocalDateWindow(range)),
    [customFrom, customTo, range],
  );
  const isDateWindowValid =
    Boolean(dateWindow.from && dateWindow.to) && dateWindow.from <= dateWindow.to;

  const loadDashboard = useCallback(
    async (options?: { clearNotice?: boolean }) => {
      setIsLoading(true);
      if (options?.clearNotice ?? true) {
        setNotice(null);
      }

      if (!isDateWindowValid) {
        setNotice({ kind: "error", message: "请选择完整日期区间，且起始日期不能晚于结束日期。" });
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
          message: `加载仪表盘失败：${err instanceof Error ? err.message : String(err)}`,
        });
        return false;
      } finally {
        setIsLoading(false);
      }
    },
    [dateWindow.from, dateWindow.to, isDateWindowValid],
  );

  useEffect(() => {
    void loadDashboard();
  }, [loadDashboard]);

  async function handleSyncLocalData() {
    setIsSyncing(true);
    setNotice(null);
    try {
      const results = await importDetectedAgents("incremental");
      const clearedDemoRows = await clearDemoData();
      const imported = results.reduce((total, result) => total + result.imported, 0);
      const skipped = results.reduce((total, result) => total + result.skipped, 0);
      const refreshed = await loadDashboard({ clearNotice: false });
      setSyncVersion((value) => value + 1);
      setView("overview");
      setDimensionDetail(null);
      if (refreshed) {
        const cleanupText =
          clearedDemoRows > 0 ? `，清理演示数据 ${clearedDemoRows} 条` : "";
        setNotice({
          kind: "success",
          message: `本机数据已同步：写入 ${imported} 条，跳过 ${skipped} 条${cleanupText}。`,
        });
      }
    } catch (err) {
      setNotice({
        kind: "error",
        message: `同步本机数据失败：${err instanceof Error ? err.message : String(err)}`,
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
        setNotice({ kind: "success", message: "演示数据已生成，仪表盘已刷新。" });
      }
    } catch (err) {
      setNotice({
        kind: "error",
        message: `生成演示数据失败：${err instanceof Error ? err.message : String(err)}`,
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

  const showRangeSelector = view === "overview";
  const activeTitle =
    view === "dimensions" && dimensionDetail ? "维度详情" : viewTitles[view];

  return (
    <main className="app-frame">
      <aside className="side-rail" aria-label="主导航">
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
      </aside>

      <section className="workspace">
        <header className="topbar">
          <div>
            <p className="eyebrow">TokenScope Desktop</p>
            <h1>{activeTitle}</h1>
          </div>
          <div className="actions">
            <button
              className="primary secondary"
              disabled={isSyncing}
              onClick={() => void handleSyncLocalData()}
              type="button"
            >
              {isSyncing ? "同步中..." : "同步本机数据"}
            </button>
            {showRangeSelector ? (
              <div className="range-control-group">
                <div className="segmented range-segmented" aria-label="日期范围">
                  {(["today", "7d", "30d", "90d", "custom"] as DashboardRangeMode[]).map(
                    (option) => (
                      <button
                        className={option === range ? "active" : ""}
                        key={option}
                        onClick={() => setRange(option)}
                        type="button"
                      >
                        {rangeModeLabels[option]}
                      </button>
                    ),
                  )}
                </div>
                {range === "custom" ? (
                  <div className="date-range-picker" aria-label="自定义日期范围">
                    <label>
                      <span>开始</span>
                      <input
                        max={customTo}
                        onChange={(event) => setCustomFrom(event.target.value)}
                        required
                        type="date"
                        value={customFrom}
                      />
                    </label>
                    <label>
                      <span>结束</span>
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

        {notice ? <div className={`notice ${notice.kind}`}>{notice.message}</div> : null}

        {view === "overview" ? (
          <>
            <SummaryCards isLoading={isLoading} summary={summary} />

            <section className="overview-focus" aria-label="每日用量趋势">
              <MiniSeriesChart agentPoints={agentSeries} isLoading={isLoading} points={series} />
            </section>

            <section className="overview-secondary" aria-label="排行分析">
              <div className="top-lists">
                <TopList
                  isLoading={isLoading}
                  kind="agent"
                  onRowClick={(value) => openDimensionDetail("agent", value)}
                  rows={topAgentRows}
                  title="Agent 排行"
                />
                <TopList
                  isLoading={isLoading}
                  kind="model"
                  onRowClick={(value) => openDimensionDetail("model", value)}
                  rows={models}
                  title="模型排行"
                />
                <TopList
                  isLoading={isLoading}
                  kind="provider"
                  onRowClick={(value) => openDimensionDetail("provider", value)}
                  rows={providers}
                  title="Provider 排行"
                />
                <TopList
                  isLoading={isLoading}
                  kind="workflow"
                  onRowClick={(value) => openDimensionDetail("workflow", value)}
                  rows={workflows}
                  title="工作流排行"
                />
                <TopList
                  isLoading={isLoading}
                  kind="project"
                  onRowClick={(value) => openDimensionDetail("project", value)}
                  rows={projects}
                  title="项目排行"
                />
                <TopList
                  isLoading={isLoading}
                  kind="session"
                  onRowClick={(value) => openDimensionDetail("session", value)}
                  rows={sessions}
                  title="会话排行"
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
            isSeedLoading={isLoading}
            isSyncing={isSyncing}
            onSeedDemoData={handleSeed}
            onSyncLocalData={handleSyncLocalData}
          />
        ) : null}
      </section>
    </main>
  );
}
