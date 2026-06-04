import type { DashboardSummary } from "../types/dashboard";
import { useI18n } from "../i18n";
import { useDisplayPreference } from "../preferences/display";
import { formatInteger, formatLatency, formatPercent, formatTokenByDisplayMode } from "../utils/format";

interface SummaryCardsProps {
  isLoading: boolean;
  summary: DashboardSummary;
}

export function SummaryCards({ isLoading, summary }: SummaryCardsProps) {
  const { numberLocale, t } = useI18n();
  const { numberDisplayMode } = useDisplayPreference();
  const cards = [
    {
      exactValue: `${formatInteger(summary.total_tokens, numberLocale)} Token`,
      label: t("Token 总量"),
      value: formatTokenByDisplayMode(summary.total_tokens, numberLocale, numberDisplayMode),
    },
    { label: t("调用次数"), value: formatInteger(summary.calls, numberLocale) },
    { label: t("错误率"), value: formatPercent(summary.error_rate, numberLocale) },
    { label: t("平均延迟"), value: formatLatency(summary.avg_latency_ms, t("无")) },
    {
      exactValue: `${formatInteger(summary.cached_input_tokens, numberLocale)} Token`,
      label: t("缓存输入"),
      value: formatTokenByDisplayMode(summary.cached_input_tokens, numberLocale, numberDisplayMode),
    },
    { label: t("最高 Agent"), value: summary.top_agent_id ?? t("无") },
    { label: t("最高模型"), value: summary.top_model ?? t("无") },
  ];

  return (
    <section className="summary-grid" aria-busy={isLoading}>
      {cards.map((card) => (
        <article className="summary-card" key={card.label}>
          <span>{card.label}</span>
          <strong title={card.exactValue ?? card.value}>{isLoading ? t("加载中...") : card.value}</strong>
        </article>
      ))}
    </section>
  );
}
