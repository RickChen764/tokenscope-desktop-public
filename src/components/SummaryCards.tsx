import type { DashboardSummary } from "../types/dashboard";
import { formatInteger, formatLatency, formatPercent } from "../utils/format";

interface SummaryCardsProps {
  isLoading: boolean;
  summary: DashboardSummary;
}

export function SummaryCards({ isLoading, summary }: SummaryCardsProps) {
  const cards = [
    { label: "Token 总量", value: formatInteger(summary.total_tokens) },
    { label: "调用次数", value: formatInteger(summary.calls) },
    { label: "错误率", value: formatPercent(summary.error_rate) },
    { label: "平均延迟", value: formatLatency(summary.avg_latency_ms) },
    { label: "缓存输入", value: formatInteger(summary.cached_input_tokens) },
    { label: "最高 Agent", value: summary.top_agent_id ?? "无" },
    { label: "最高模型", value: summary.top_model ?? "无" },
  ];

  return (
    <section className="summary-grid" aria-busy={isLoading}>
      {cards.map((card) => (
        <article className="summary-card" key={card.label}>
          <span>{card.label}</span>
          <strong>{isLoading ? "加载中..." : card.value}</strong>
        </article>
      ))}
    </section>
  );
}
