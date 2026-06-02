import { useEffect, useMemo, useState } from "react";
import type { DailyUsagePoint } from "../types/dashboard";
import { formatInteger } from "../utils/format";

interface MiniSeriesChartProps {
  agentPoints?: DailyUsagePoint[];
  isLoading: boolean;
  points: DailyUsagePoint[];
  title?: string;
}

type ChartMode = "bar" | "line";

interface AgentSegment {
  agent: string;
  color: string;
  tokens: number;
}

interface DayBucket {
  date: string;
  segments: AgentSegment[];
  totalTokens: number;
}

interface LineSeries {
  className: "line-series-total" | "line-series-agent";
  color: string;
  key: string;
  label: string;
  values: number[];
}

const lineChartSize = {
  height: 320,
  paddingBottom: 46,
  paddingLeft: 62,
  paddingRight: 28,
  paddingTop: 26,
};

const agentPalette = ["#0f766e", "#2563eb", "#d97706", "#be123c", "#7c3aed", "#475569"];

function pointDateKey(point: DailyUsagePoint) {
  return point.date_local;
}

function agentKey(point: DailyUsagePoint) {
  return point.dimension?.trim() || "unknown";
}

function agentLabel(agent: string) {
  return agent === "unknown" ? "未知 Agent" : agent;
}

function pathForValues(values: number[], maxTokens: number, plotWidth: number, plotHeight: number) {
  return values
    .map((tokens, index) => {
      const x =
        lineChartSize.paddingLeft +
        (values.length === 1 ? plotWidth / 2 : (index / (values.length - 1)) * plotWidth);
      const y = lineChartSize.paddingTop + plotHeight - (tokens / maxTokens) * plotHeight;

      return `${index === 0 ? "M" : "L"} ${x.toFixed(1)} ${y.toFixed(1)}`;
    })
    .join(" ");
}

export function MiniSeriesChart({
  agentPoints = [],
  isLoading,
  points,
  title = "每日用量",
}: MiniSeriesChartProps) {
  const [chartMode, setChartMode] = useState<ChartMode>("bar");
  const [selectedLineSeriesKeys, setSelectedLineSeriesKeys] = useState<string[]>([]);

  const chartData = useMemo(() => {
    const totalsByDate = new Map(points.map((point) => [pointDateKey(point), point.total_tokens]));
    const dates = [
      ...new Set([...points.map(pointDateKey), ...agentPoints.map(pointDateKey)]),
    ].sort();
    const agentTotals = new Map<string, number>();
    const agentTokensByDate = new Map<string, Map<string, number>>();

    for (const point of agentPoints) {
      const date = pointDateKey(point);
      const agent = agentKey(point);
      agentTotals.set(agent, (agentTotals.get(agent) ?? 0) + point.total_tokens);
      if (!agentTokensByDate.has(date)) {
        agentTokensByDate.set(date, new Map());
      }
      const day = agentTokensByDate.get(date)!;
      day.set(agent, (day.get(agent) ?? 0) + point.total_tokens);
    }

    const topAgents = [...agentTotals.entries()]
      .sort((left, right) => right[1] - left[1] || left[0].localeCompare(right[0]))
      .slice(0, 5)
      .map(([agent]) => agent);
    const displayAgents = topAgents.length > 0 ? topAgents : ["total"];
    const colors = new Map(displayAgents.map((agent, index) => [agent, agentPalette[index]]));
    const hasOtherAgent = [...agentTotals.keys()].some((agent) => !topAgents.includes(agent));
    if (hasOtherAgent) {
      colors.set("other", agentPalette[Math.min(topAgents.length, agentPalette.length - 1)]);
    }

    const totalForDate = (date: string) => {
      const knownTotal = totalsByDate.get(date);
      if (knownTotal !== undefined) {
        return knownTotal;
      }

      const dayAgents = agentTokensByDate.get(date) ?? new Map<string, number>();
      return [...dayAgents.values()].reduce((total, value) => total + value, 0);
    };

    const dayBuckets: DayBucket[] = dates.map((date) => {
      const dayAgents = agentTokensByDate.get(date) ?? new Map<string, number>();
      const totalTokens = totalForDate(date);
      const segments: AgentSegment[] =
        topAgents.length > 0
          ? topAgents
              .map((agent) => ({
                agent,
                color: colors.get(agent) ?? agentPalette[0],
                tokens: dayAgents.get(agent) ?? 0,
              }))
              .filter((segment) => segment.tokens > 0)
          : [
              {
                agent: "total",
                color: agentPalette[0],
                tokens: totalTokens,
              },
            ];

      if (hasOtherAgent) {
        const otherTokens = [...dayAgents.entries()]
          .filter(([agent]) => !topAgents.includes(agent))
          .reduce((total, [, tokens]) => total + tokens, 0);
        if (otherTokens > 0) {
          segments.push({
            agent: "other",
            color: colors.get("other") ?? agentPalette[agentPalette.length - 1],
            tokens: otherTokens,
          });
        }
      }

      return { date, segments, totalTokens };
    });

    const lineSeries: LineSeries[] = [
      {
        className: "line-series-total",
        color: "#172124",
        key: "total",
        label: "总 Token",
        values: dates.map(totalForDate),
      },
      ...topAgents.map((agent) => ({
        className: "line-series-agent" as const,
        color: colors.get(agent) ?? agentPalette[0],
        key: agent,
        label: agentLabel(agent),
        values: dates.map((date) => agentTokensByDate.get(date)?.get(agent) ?? 0),
      })),
    ];

    if (hasOtherAgent) {
      lineSeries.push({
        className: "line-series-agent",
        color: colors.get("other") ?? agentPalette[agentPalette.length - 1],
        key: "other",
        label: "其他 Agent",
        values: dates.map((date) => {
          const dayAgents = agentTokensByDate.get(date) ?? new Map<string, number>();
          return [...dayAgents.entries()]
            .filter(([agent]) => !topAgents.includes(agent))
            .reduce((total, [, tokens]) => total + tokens, 0);
        }),
      });
    }

    return {
      activeAgentCount: agentTotals.size,
      dayBuckets,
      dates,
      lineSeries,
      maxDailyTokens: Math.max(
        ...dayBuckets.map((bucket) => bucket.totalTokens),
        ...lineSeries.flatMap((series) => series.values),
        1,
      ),
      totalTokens: dayBuckets.reduce((total, bucket) => total + bucket.totalTokens, 0),
    };
  }, [agentPoints, points]);

  const allLineSeriesKeys = useMemo(
    () => chartData.lineSeries.map((series) => series.key),
    [chartData.lineSeries],
  );
  const allLineSeriesKeySignature = allLineSeriesKeys.join("|");

  useEffect(() => {
    setSelectedLineSeriesKeys(allLineSeriesKeys);
  }, [allLineSeriesKeySignature]);

  const selectedLineSeriesKeysInCurrentData = selectedLineSeriesKeys.filter((key) =>
    allLineSeriesKeys.includes(key),
  );
  const activeLineSeriesKeys =
    selectedLineSeriesKeysInCurrentData.length > 0
      ? selectedLineSeriesKeysInCurrentData
      : allLineSeriesKeys;
  const activeLineSeriesKeySet = new Set(activeLineSeriesKeys);
  const visibleLineSeries = chartData.lineSeries.filter((series) =>
    activeLineSeriesKeySet.has(series.key),
  );
  const visibleLineMaxTokens = Math.max(
    ...visibleLineSeries.flatMap((series) => series.values),
    1,
  );
  const allLineSeriesSelected = activeLineSeriesKeys.length === allLineSeriesKeys.length;

  function selectAllLineSeries() {
    setSelectedLineSeriesKeys(allLineSeriesKeys);
  }

  function toggleLineSeries(key: string) {
    setSelectedLineSeriesKeys((currentKeys) => {
      const currentKeysInData = currentKeys.filter((item) => allLineSeriesKeys.includes(item));
      const keys = currentKeysInData.length > 0 ? currentKeysInData : allLineSeriesKeys;
      if (!keys.includes(key)) {
        return [...keys, key];
      }
      if (keys.length === 1) {
        return keys;
      }

      return keys.filter((item) => item !== key);
    });
  }

  const svgWidth = Math.max(640, chartData.dates.length * 54);
  const plotHeight = lineChartSize.height - lineChartSize.paddingTop - lineChartSize.paddingBottom;
  const plotWidth = svgWidth - lineChartSize.paddingLeft - lineChartSize.paddingRight;
  const labelStep = Math.max(1, Math.ceil(chartData.dates.length / 8));
  const baselineY = lineChartSize.height - lineChartSize.paddingBottom;
  const totalLine = visibleLineSeries.find((series) => series.key === "total");
  const totalLinePath = totalLine
    ? pathForValues(totalLine.values, visibleLineMaxTokens, plotWidth, plotHeight)
    : "";
  const totalAreaPath =
    totalLinePath && chartData.dates.length > 0
      ? `${totalLinePath} L ${svgWidth - lineChartSize.paddingRight} ${baselineY} L ${lineChartSize.paddingLeft} ${baselineY} Z`
      : "";

  return (
    <section className="panel usage-chart-panel usage-chart-main" aria-busy={isLoading}>
      <div className="panel-heading usage-chart-heading">
        <div className="usage-chart-title-block">
          <p className="eyebrow">趋势分析</p>
          <h2>{title}</h2>
          <p>按本地日期汇总，柱状图展示每日 Agent 构成，折线图展示总量和 Agent 趋势。</p>
        </div>
        <div className="usage-chart-toolbar">
          <div className="segmented chart-mode-toggle" aria-label="每日用量图表形式">
            <button
              className={chartMode === "bar" ? "active" : ""}
              onClick={() => setChartMode("bar")}
              type="button"
            >
              柱状
            </button>
            <button
              className={chartMode === "line" ? "active" : ""}
              onClick={() => setChartMode("line")}
              type="button"
            >
              折线
            </button>
          </div>
        </div>
      </div>
      {chartData.dayBuckets.length === 0 ? (
        <div className="empty-state">{isLoading ? "加载中..." : "暂无调用记录"}</div>
      ) : (
        <>
          <div className="usage-chart-summary">
            <div>
              <span>区间 Token</span>
              <strong>{formatInteger(chartData.totalTokens)}</strong>
            </div>
            <div>
              <span>活跃 Agent</span>
              <strong>{formatInteger(chartData.activeAgentCount || 1)}</strong>
            </div>
            <div>
              <span>天数</span>
              <strong>{formatInteger(chartData.dates.length)}</strong>
            </div>
          </div>

          <div
            className={`usage-chart-legend${chartMode === "line" ? " selectable-legend" : ""}`}
            aria-label={chartMode === "line" ? "折线显示选择" : "Agent 图例"}
          >
            {chartMode === "line" ? (
              <>
                <button
                  aria-pressed={allLineSeriesSelected}
                  className={`line-series-toggle all-line-series-toggle${
                    allLineSeriesSelected ? " active" : ""
                  }`}
                  onClick={selectAllLineSeries}
                  type="button"
                >
                  全部
                </button>
                {chartData.lineSeries.map((series) => {
                  const selected = activeLineSeriesKeySet.has(series.key);

                  return (
                    <button
                      aria-pressed={selected}
                      className={`line-series-toggle${selected ? " active" : ""}`}
                      key={series.key}
                      onClick={() => toggleLineSeries(series.key)}
                      type="button"
                    >
                      <i className="legend-dot" style={{ background: series.color }} />
                      {series.label}
                    </button>
                  );
                })}
              </>
            ) : (
              chartData.lineSeries.map((series) => (
                <span key={series.key}>
                  <i className="legend-dot" style={{ background: series.color }} />
                  {series.label}
                </span>
              ))
            )}
          </div>

          {chartMode === "bar" ? (
            <div className="series enhanced-series stacked-series" role="list">
              {chartData.dayBuckets.map((bucket) => (
                <div className="series-item" key={bucket.date}>
                  <div className="bar-wrap stacked-bar-wrap">
                    <div className="stacked-bar">
                      {bucket.segments.map((segment) => {
                        const height = Math.max(
                          3,
                          (segment.tokens / chartData.maxDailyTokens) * 160,
                        );

                        return (
                          <div
                            className="stacked-bar-segment"
                            key={`${bucket.date}-${segment.agent}`}
                            style={{ background: segment.color, height }}
                            title={`${agentLabel(segment.agent)} / ${bucket.date} / ${formatInteger(
                              segment.tokens,
                            )} Token`}
                          />
                        );
                      })}
                    </div>
                  </div>
                  <span className="series-date">{bucket.date.slice(5)}</span>
                  <span className="series-value">{formatInteger(bucket.totalTokens)}</span>
                </div>
              ))}
            </div>
          ) : (
            <div className="line-chart-wrap" role="img" aria-label="每日 Token 用量折线图">
              <svg
                className="line-chart-svg"
                height={lineChartSize.height}
                viewBox={`0 0 ${svgWidth} ${lineChartSize.height}`}
                width={svgWidth}
              >
                <defs>
                  <linearGradient id="usageLineArea" x1="0" x2="0" y1="0" y2="1">
                    <stop offset="0%" stopColor="#172124" stopOpacity="0.14" />
                    <stop offset="100%" stopColor="#172124" stopOpacity="0" />
                  </linearGradient>
                </defs>
                {[0.25, 0.5, 0.75, 1].map((ratio) => {
                  const y = lineChartSize.paddingTop + plotHeight * ratio;

                  return (
                    <line
                      className="line-chart-grid"
                      key={ratio}
                      x1={lineChartSize.paddingLeft}
                      x2={svgWidth - lineChartSize.paddingRight}
                      y1={y}
                      y2={y}
                    />
                  );
                })}
                <path className="line-chart-area" d={totalAreaPath} />
                {visibleLineSeries.map((series) => {
                  const path = pathForValues(
                    series.values,
                    visibleLineMaxTokens,
                    plotWidth,
                    plotHeight,
                  );

                  return (
                    <g key={series.key}>
                      <path
                        className={`line-chart-line ${series.className}`}
                        d={path}
                        style={{ stroke: series.color }}
                      />
                      {series.values.map((tokens, index) => {
                        const x =
                          lineChartSize.paddingLeft +
                          (series.values.length === 1
                            ? plotWidth / 2
                            : (index / (series.values.length - 1)) * plotWidth);
                        const y =
                          lineChartSize.paddingTop +
                          plotHeight -
                          (tokens / visibleLineMaxTokens) * plotHeight;

                        return (
                          <circle
                            className={`line-chart-point ${series.className}`}
                            cx={x}
                            cy={y}
                            key={`${series.key}-${chartData.dates[index]}`}
                            r={series.key === "total" ? "4" : "3"}
                            style={{ stroke: series.color }}
                          >
                            <title>
                              {series.label} / {chartData.dates[index]} / {formatInteger(tokens)}{" "}
                              Token
                            </title>
                          </circle>
                        );
                      })}
                    </g>
                  );
                })}
                {chartData.dates.map((date, index) => {
                  if (index !== 0 && index !== chartData.dates.length - 1 && index % labelStep !== 0) {
                    return null;
                  }
                  const x =
                    lineChartSize.paddingLeft +
                    (chartData.dates.length === 1
                      ? plotWidth / 2
                      : (index / (chartData.dates.length - 1)) * plotWidth);

                  return (
                    <text className="line-chart-label" key={date} x={x} y={lineChartSize.height - 10}>
                      {date.slice(5)}
                    </text>
                  );
                })}
                <text className="line-chart-axis-label" x="8" y={lineChartSize.paddingTop + 4}>
                  Token
                </text>
              </svg>
            </div>
          )}
        </>
      )}
    </section>
  );
}
