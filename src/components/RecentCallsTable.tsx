import type { LlmCallRow } from "../types/dashboard";
import { useI18n } from "../i18n";
import { formatInteger, formatLatency } from "../utils/format";

interface RecentCallsTableProps {
  emptyLabel?: string;
  isLoading: boolean;
  rows: LlmCallRow[];
  title?: string;
}

const statusLabels: Record<string, string> = {
  success: "成功",
  error: "失败",
};

interface CallsTableProps {
  emptyLabel?: string;
  isLoading: boolean;
  rows: LlmCallRow[];
}

export function CallsTable({ emptyLabel, isLoading, rows }: CallsTableProps) {
  const { numberLocale, t } = useI18n();
  const resolvedEmptyLabel = emptyLabel ?? t("暂无调用记录");
  if (rows.length === 0) {
    return <div className="empty-state">{isLoading ? t("加载中...") : resolvedEmptyLabel}</div>;
  }

  return (
    <div className="table-scroll">
      <table className="calls-table">
        <thead>
          <tr>
            <th>{t("开始时间")}</th>
            <th>Provider</th>
            <th>{t("模型")}</th>
            <th>Agent</th>
            <th>{t("工作流")}</th>
            <th>Token</th>
            <th>{t("延迟")}</th>
            <th>{t("状态")}</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={row.id}>
              <td>{row.started_at.replace("T", " ").slice(0, 19)}</td>
              <td>{row.provider}</td>
              <td>{row.model_response ?? row.model_requested ?? t("未知")}</td>
              <td>{row.agent_id ?? t("未知")}</td>
              <td>{row.workflow_id ?? t("未知")}</td>
              <td>{formatInteger(row.total_tokens, numberLocale)}</td>
              <td>{formatLatency(row.latency_ms, t("无"))}</td>
              <td>
                <span className={`status ${row.status}`}>
                  {t(statusLabels[row.status] ?? row.status)}
                </span>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export function RecentCallsTable({
  emptyLabel,
  isLoading,
  rows,
  title,
}: RecentCallsTableProps) {
  const { t } = useI18n();
  return (
    <section className="panel">
      <div className="panel-heading">
        <h2>{title ?? t("最近调用")}</h2>
      </div>
      <CallsTable emptyLabel={emptyLabel} isLoading={isLoading} rows={rows} />
    </section>
  );
}
