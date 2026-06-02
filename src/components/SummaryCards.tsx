import type { DashboardSummary } from "../types/dashboard";
import { useI18n } from "../i18n";
import { formatInteger, formatLatency, formatPercent } from "../utils/format";

interface SummaryCardsProps {
  isLoading: boolean;
  summary: DashboardSummary;
}

export function SummaryCards({ isLoading, summary }: SummaryCardsProps) {
  const { numberLocale, t } = useI18n();
  const cards = [
    { label: t("Token 总量"), value: formatInteger(summary.total_tokens, numberLocale) },
    { label: t("调用次数"), value: formatInteger(summary.calls, numberLocale) },
    { label: t("错误率"), value: formatPercent(summary.error_rate, numberLocale) },
    { label: t("平均延迟"), value: formatLatency(summary.avg_latency_ms, t("无")) },
    { label: t("缓存输入"), value: formatInteger(summary.cached_input_tokens, numberLocale) },
    { label: t("最高 Agent"), value: summary.top_agent_id ?? t("无") },
    { label: t("最高模型"), value: summary.top_model ?? t("无") },
  ];

  return (
    <section className="summary-grid" aria-busy={isLoading}>
      {cards.map((card) => (
        <article className="summary-card" key={card.label}>
          <span>{card.label}</span>
          <strong>{isLoading ? t("加载中...") : card.value}</strong>
        </article>
      ))}
    </section>
  );
}
