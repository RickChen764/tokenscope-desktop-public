import { useEffect, useMemo, useRef, useState } from "react";
import { BarChart, LineChart } from "echarts/charts";
import {
  GridComponent,
  LegendComponent,
  TooltipComponent,
} from "echarts/components";
import { init, use } from "echarts/core";
import { CanvasRenderer } from "echarts/renderers";
import { useI18n } from "../i18n";
import type { DailyUsagePoint } from "../types/dashboard";
import { formatCompactToken, formatInteger } from "../utils/format";

interface MiniSeriesChartProps {
  agentPoints?: DailyUsagePoint[];
  isLoading: boolean;
  points: DailyUsagePoint[];
  title?: string;
}

type ChartMode = "bar" | "line";
type ChartInstance = ReturnType<typeof init>;
type ChartOption = Parameters<ChartInstance["setOption"]>[0];

interface DayBucket {
  date: string;
  totalTokens: number;
}

interface UsageLineSeries {
  color: string;
  key: string;
  label: string;
  values: number[];
}

interface TooltipParam {
  axisValue?: string;
  axisValueLabel?: string;
  marker?: string;
  name?: string;
  seriesName?: string;
  value?: number | string | Array<number | string | null> | null;
}

const agentPalette = ["#3794ff", "#4ec9b0", "#b180d7", "#d7ba7d", "#ce9178", "#9cdcfe"];

use([
  BarChart,
  LineChart,
  GridComponent,
  LegendComponent,
  TooltipComponent,
  CanvasRenderer,
]);

function pointDateKey(point: DailyUsagePoint) {
  return point.date_local;
}

function agentKey(point: DailyUsagePoint) {
  return point.dimension?.trim() || "unknown";
}

function agentLabel(agent: string, unknownLabel: string) {
  if (agent === "unknown") {
    return unknownLabel;
  }
  if (agent === "other") {
    return "其他";
  }
  return agent;
}

function chartDataAverage(totalTokens: number, count: number) {
  return count > 0 ? totalTokens / count : 0;
}

function toTooltipParams(params: unknown): TooltipParam[] {
  const items = Array.isArray(params) ? params : [params];

  return items
    .filter((item): item is Record<string, unknown> => typeof item === "object" && item !== null)
    .map((item) => ({
      axisValue: typeof item.axisValue === "string" ? item.axisValue : undefined,
      axisValueLabel:
        typeof item.axisValueLabel === "string" ? item.axisValueLabel : undefined,
      marker: typeof item.marker === "string" ? item.marker : undefined,
      name: typeof item.name === "string" ? item.name : undefined,
      seriesName: typeof item.seriesName === "string" ? item.seriesName : undefined,
      value: Array.isArray(item.value)
        ? item.value.filter(
            (value): value is number | string | null =>
              typeof value === "number" || typeof value === "string" || value === null,
          )
        : typeof item.value === "number" || typeof item.value === "string"
          ? item.value
          : null,
    }));
}

function tooltipValue(param: TooltipParam) {
  if (Array.isArray(param.value)) {
    const lastValue = param.value[param.value.length - 1];
    return typeof lastValue === "number" ? lastValue : Number(lastValue ?? 0);
  }

  return typeof param.value === "number" ? param.value : Number(param.value ?? 0);
}

function tooltipDate(param: TooltipParam) {
  return param.axisValue ?? param.name ?? param.axisValueLabel ?? "";
}

function formatTooltipValue(value: number, locale: string) {
  return `${formatInteger(value, locale)} Token`;
}

export function MiniSeriesChart({
  agentPoints = [],
  isLoading,
  points,
  title,
}: MiniSeriesChartProps) {
  const { numberLocale, t } = useI18n();
  const [chartMode, setChartMode] = useState<ChartMode>("bar");
  const chartNodeRef = useRef<HTMLDivElement | null>(null);
  const chartInstanceRef = useRef<ChartInstance | null>(null);

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
      return { date, totalTokens };
    });

    const barSeries =
      topAgents.length > 0
        ? [
            ...topAgents.map((agent) => ({
              color: colors.get(agent) ?? agentPalette[0],
              key: agent,
              label: agentLabel(agent, t("未知 Agent")),
              values: dates.map((date) => agentTokensByDate.get(date)?.get(agent) ?? 0),
            })),
            ...(hasOtherAgent
              ? [
                  {
                    color: colors.get("other") ?? agentPalette[agentPalette.length - 1],
                    key: "other",
                    label: t("其他"),
                    values: dates.map((date) => {
                      const dayAgents = agentTokensByDate.get(date) ?? new Map<string, number>();
                      return [...dayAgents.entries()]
                        .filter(([agent]) => !topAgents.includes(agent))
                        .reduce((total, [, tokens]) => total + tokens, 0);
                    }),
                  },
                ]
              : []),
          ]
        : [
            {
              color: agentPalette[0],
              key: "total-bar",
              label: t("总 Token"),
              values: dates.map(totalForDate),
            },
          ];

    const lineSeries: UsageLineSeries[] = [
      {
        color: "#cccccc",
        key: "total",
        label: t("总量"),
        values: dates.map(totalForDate),
      },
      ...barSeries
        .filter((series) => series.key !== "total-bar")
        .map((series) => ({
          color: series.color,
          key: series.key,
          label: series.label,
          values: series.values,
        })),
    ];

    const totalTokens = dayBuckets.reduce((total, bucket) => total + bucket.totalTokens, 0);
    const peakBucket = dayBuckets.reduce<DayBucket | null>(
      (currentPeak, bucket) =>
        currentPeak === null || bucket.totalTokens > currentPeak.totalTokens ? bucket : currentPeak,
      null,
    );
    const averageDailyTokens = chartDataAverage(totalTokens, dayBuckets.length);
    const totalsByDateForTooltip = new Map(dayBuckets.map((bucket) => [bucket.date, bucket.totalTokens]));

    return {
      activeAgentCount: agentTotals.size,
      averageDailyTokens,
      barSeries,
      dates,
      dayBuckets,
      lineSeries,
      peakBucket,
      totalTokens,
      totalsByDateForTooltip,
    };
  }, [agentPoints, points, t]);

  const chartOption = useMemo<ChartOption>(() => {
    const totalLineValues = chartData.dates.map(
      (date) => chartData.totalsByDateForTooltip.get(date) ?? 0,
    );
    const commonLine = {
      areaStyle: { color: "rgba(204, 204, 204, 0.05)" },
      data: totalLineValues,
      emphasis: { focus: "series" },
      itemStyle: { borderColor: "#1e1e1e", borderWidth: 2, color: "#cccccc" },
      lineStyle: { color: "#b7b7b7", width: 2.2 },
      name: t("总量"),
      smooth: 0.18,
      symbol: "circle",
      symbolSize: 7,
      type: "line",
      z: 8,
    };
    const tooltipFormatter = (params: unknown) => {
      const items = toTooltipParams(params);
      const date = tooltipDate(items[0] ?? {});
      const total = chartData.totalsByDateForTooltip.get(date) ?? 0;
      const rows = items
        .filter((item) => item.seriesName !== t("总量"))
        .filter((item) => tooltipValue(item) > 0)
        .map((item) => {
          const value = tooltipValue(item);
          const percent = total > 0 ? ` (${((value / total) * 100).toFixed(1)}%)` : "";
          return `<div class="usage-tooltip-row"><span>${item.marker ?? ""}${item.seriesName ?? ""}</span><strong title="${formatTooltipValue(
            value,
            numberLocale,
          )}">${formatCompactToken(value, numberLocale)}${percent}</strong></div>`;
        })
        .join("");

      return `<div class="usage-tooltip"><div class="usage-tooltip-title">${date}${
        chartData.peakBucket?.date === date ? ` (${t("峰值")})` : ""
      }</div>${rows}<div class="usage-tooltip-total"><span>${t("总量")}</span><strong title="${formatTooltipValue(
        total,
        numberLocale,
      )}">${formatCompactToken(total, numberLocale)}</strong></div></div>`;
    };
    const barSeries = chartData.barSeries.map((series) => ({
      barMaxWidth: 34,
      data: series.values,
      emphasis: { focus: "series" },
      itemStyle: { borderRadius: [4, 4, 0, 0], color: series.color },
      name: series.label,
      stack: "tokens",
      type: "bar",
    }));
    const lineOnlySeries = chartData.lineSeries.map((series) => ({
      data: series.values,
      emphasis: { focus: "series" },
      itemStyle: { borderColor: "#1e1e1e", borderWidth: 2, color: series.color },
      lineStyle: { color: series.color, width: series.key === "total" ? 2.4 : 1.8 },
      name: series.label,
      smooth: 0.18,
      symbol: "circle",
      symbolSize: series.key === "total" ? 7 : 5,
      type: "line",
    }));
    const series =
      chartMode === "bar"
        ? [
            ...barSeries,
            commonLine,
          ]
        : lineOnlySeries;

    return {
      animationDuration: 360,
      backgroundColor: "transparent",
      color: [
        ...chartData.barSeries.map((series) => series.color),
        "#cccccc",
      ],
      grid: {
        bottom: 58,
        containLabel: true,
        left: 8,
        right: 18,
        top: 58,
      },
      legend: {
        icon: "roundRect",
        itemGap: 18,
        itemHeight: 8,
        itemWidth: 16,
        left: 0,
        textStyle: {
          color: "#a9a9a9",
          fontSize: 12,
          fontWeight: 700,
        },
        top: 4,
      },
      series,
      tooltip: {
        appendToBody: true,
        axisPointer: {
          label: { color: "#cccccc" },
          lineStyle: { color: "#6a6a6a", type: "dashed" },
          type: "line",
        },
        backgroundColor: "rgba(37, 37, 38, 0.96)",
        borderColor: "#3c3c3c",
        className: "usage-echarts-tooltip",
        confine: true,
        extraCssText: "box-shadow: none; border-radius: 6px;",
        formatter: tooltipFormatter,
        padding: 0,
        textStyle: { color: "#cccccc" },
        trigger: "axis",
      },
      xAxis: {
        axisLabel: {
          color: "#b9b9b9",
          fontSize: 12,
          formatter: (value: string) => value.slice(5),
          margin: 12,
        },
        axisLine: { lineStyle: { color: "#3c3c3c" } },
        axisTick: { show: false },
        data: chartData.dates,
        type: "category",
      },
      yAxis: {
        axisLabel: {
          color: "#b9b9b9",
          formatter: (value: number) => formatCompactToken(value, numberLocale),
        },
        name: "Token",
        nameGap: 16,
        nameTextStyle: { align: "left", color: "#b9b9b9", fontWeight: 700 },
        splitLine: { lineStyle: { color: "#333333", type: "dashed" } },
        type: "value",
      },
    };
  }, [chartData, chartMode, numberLocale, t]);

  useEffect(() => {
    const node = chartNodeRef.current;
    if (!node || chartData.dayBuckets.length === 0) {
      return;
    }

    const chart = chartInstanceRef.current ?? init(node, undefined, { renderer: "canvas" });
    chartInstanceRef.current = chart;
    chart.setOption(chartOption, true);
    chart.resize();

    const resizeObserver = new ResizeObserver(() => chart.resize());
    resizeObserver.observe(node);

    return () => resizeObserver.disconnect();
  }, [chartData.dayBuckets.length, chartOption]);

  useEffect(
    () => () => {
      chartInstanceRef.current?.dispose();
      chartInstanceRef.current = null;
    },
    [],
  );

  return (
    <section className="panel usage-chart-panel usage-chart-main" aria-busy={isLoading}>
      <div className="panel-heading usage-chart-heading">
        <div className="usage-chart-title-block">
          <p className="eyebrow">{t("趋势分析")}</p>
          <h2>{title ?? t("每日用量")}</h2>
          <p>{t("按本地日期汇总，柱状图展示每日 Agent 构成，折线图展示总量和 Agent 趋势。")}</p>
        </div>
        <div className="usage-chart-toolbar">
          <div className="segmented chart-mode-toggle" aria-label={t("每日用量图表形式")}>
            <button
              className={chartMode === "bar" ? "active" : ""}
              onClick={() => setChartMode("bar")}
              type="button"
            >
              {t("柱状")}
            </button>
            <button
              className={chartMode === "line" ? "active" : ""}
              onClick={() => setChartMode("line")}
              type="button"
            >
              {t("折线")}
            </button>
          </div>
        </div>
      </div>
      {chartData.dayBuckets.length === 0 ? (
        <div className="empty-state">{isLoading ? t("加载中...") : t("暂无调用记录")}</div>
      ) : (
        <>
          <div className="usage-chart-summary">
            <div>
              <span>{t("区间 Token")}</span>
              <strong title={`${formatInteger(chartData.totalTokens, numberLocale)} Token`}>
                {formatCompactToken(chartData.totalTokens, numberLocale)}
              </strong>
            </div>
            <div>
              <span>{t("峰值日")}</span>
              <strong
                title={
                  chartData.peakBucket
                    ? `${chartData.peakBucket.date} / ${formatInteger(chartData.peakBucket.totalTokens, numberLocale)} Token`
                    : t("无")
                }
              >
                {chartData.peakBucket
                  ? `${chartData.peakBucket.date.slice(5)} / ${formatCompactToken(chartData.peakBucket.totalTokens, numberLocale)}`
                  : t("无")}
              </strong>
            </div>
            <div>
              <span>{t("日均 Token")}</span>
              <strong title={`${formatInteger(chartData.averageDailyTokens, numberLocale)} Token`}>
                {formatCompactToken(chartData.averageDailyTokens, numberLocale)}
              </strong>
            </div>
            <div>
              <span>{t("活跃 Agent")}</span>
              <strong>{formatInteger(chartData.activeAgentCount || 1, numberLocale)}</strong>
            </div>
          </div>

          <div className="usage-chart-stage usage-echarts-stage">
            <div
              aria-label={
                chartMode === "bar" ? t("每日 Token 用量柱状图") : t("每日 Token 用量折线图")
              }
              className="usage-echarts"
              ref={chartNodeRef}
              role="img"
            />
          </div>
        </>
      )}
    </section>
  );
}
