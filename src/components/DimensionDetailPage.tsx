import { useCallback, useEffect, useMemo, useState } from "react";
import {
  getDimensionDailySeries,
  getDimensionSummary,
  listLlmCalls,
} from "../services/dashboard";
import type {
  DashboardRange,
  DashboardSummary,
  DailyUsagePoint,
  DimensionKind,
  LlmCallFilters,
  LlmCallPage,
} from "../types/dashboard";
import { getLocalDateWindow } from "../utils/date";
import { formatInteger, formatLatency, formatPercent } from "../utils/format";
import { CallsTable } from "./RecentCallsTable";
import { MiniSeriesChart } from "./MiniSeriesChart";
import { SummaryCards } from "./SummaryCards";

interface DimensionDetailPageProps {
  kind: DimensionKind;
  onBack: () => void;
  value: string;
}

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

const emptyPage: LlmCallPage = {
  rows: [],
  total: 0,
};

const kindLabels: Record<DimensionKind, string> = {
  agent: "Agent",
  model: "模型",
  provider: "Provider",
  workflow: "工作流",
  project: "项目",
  session: "会话",
};

const rangeLabels: Record<DashboardRange, string> = {
  today: "今日",
  "7d": "近 7 天",
  "30d": "近 30 天",
  "90d": "近 90 天",
};

function filtersForDimension(
  kind: DimensionKind,
  value: string,
  from: string,
  to: string,
): LlmCallFilters {
  return {
    from,
    to,
    provider: kind === "provider" ? value : null,
    agent_id: kind === "agent" ? value : null,
    workflow_id: kind === "workflow" ? value : null,
    project_id: kind === "project" ? value : null,
    session_id: kind === "session" ? value : null,
    model: kind === "model" ? value : null,
    status: null,
    limit: 8,
    offset: 0,
  };
}

export function DimensionDetailPage({ kind, onBack, value }: DimensionDetailPageProps) {
  const [range, setRange] = useState<DashboardRange>("7d");
  const [summary, setSummary] = useState<DashboardSummary>(emptySummary);
  const [series, setSeries] = useState<DailyUsagePoint[]>([]);
  const [calls, setCalls] = useState<LlmCallPage>(emptyPage);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const dateWindow = useMemo(() => getLocalDateWindow(range), [range]);

  const loadDetail = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const [nextSummary, nextSeries, nextCalls] = await Promise.all([
        getDimensionSummary(dateWindow.from, dateWindow.to, kind, value),
        getDimensionDailySeries(dateWindow.from, dateWindow.to, kind, value),
        listLlmCalls(filtersForDimension(kind, value, dateWindow.from, dateWindow.to)),
      ]);
      setSummary(nextSummary);
      setSeries(nextSeries);
      setCalls(nextCalls);
    } catch (err) {
      setError(`加载维度详情失败：${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setIsLoading(false);
    }
  }, [dateWindow.from, dateWindow.to, kind, value]);

  useEffect(() => {
    void loadDetail();
  }, [loadDetail]);

  return (
    <section className="dimension-detail">
      <section className="panel dimension-hero">
        <div>
          <button className="text-button" onClick={onBack} type="button">
            返回分析
          </button>
          <p className="eyebrow">{kindLabels[kind]} 详情</p>
          <h2>{value}</h2>
        </div>
        <div className="segmented compact-segmented" aria-label="详情日期范围">
          {(["today", "7d", "30d", "90d"] as DashboardRange[]).map((option) => (
            <button
              className={option === range ? "active" : ""}
              key={option}
              onClick={() => setRange(option)}
              type="button"
            >
              {rangeLabels[option]}
            </button>
          ))}
        </div>
      </section>

      {error ? <div className="notice error inline-notice">{error}</div> : null}

      <SummaryCards isLoading={isLoading} summary={summary} />

      <section className="dimension-grid">
        <MiniSeriesChart isLoading={isLoading} points={series} title="维度每日用量" />
        <section className="panel compact">
          <div className="panel-heading">
            <h2>关联指标</h2>
          </div>
          <div className="detail-stat-list">
            <div>
              <span>输入 Token</span>
              <strong>{isLoading ? "加载中..." : formatInteger(summary.input_tokens)}</strong>
            </div>
            <div>
              <span>输出 Token</span>
              <strong>{isLoading ? "加载中..." : formatInteger(summary.output_tokens)}</strong>
            </div>
            <div>
              <span>成功 / 失败</span>
              <strong>
                {isLoading
                  ? "加载中..."
                  : `${formatInteger(summary.success_calls)} / ${formatInteger(summary.error_calls)}`}
              </strong>
            </div>
            <div>
              <span>平均延迟</span>
              <strong>{isLoading ? "加载中..." : formatLatency(summary.avg_latency_ms)}</strong>
            </div>
            <div>
              <span>错误率</span>
              <strong>{isLoading ? "加载中..." : formatPercent(summary.error_rate)}</strong>
            </div>
          </div>
        </section>
      </section>

      <section className="panel">
        <div className="panel-heading">
          <h2>相关调用</h2>
          <span className="panel-meta">{formatInteger(calls.total)} 条</span>
        </div>
        <CallsTable
          emptyLabel="当前维度和时间范围下暂无调用记录"
          isLoading={isLoading}
          rows={calls.rows}
        />
      </section>
    </section>
  );
}
