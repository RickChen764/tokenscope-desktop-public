import type { LlmCallRow } from "../types/dashboard";
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

export function CallsTable({ emptyLabel = "暂无调用记录", isLoading, rows }: CallsTableProps) {
  if (rows.length === 0) {
    return <div className="empty-state">{isLoading ? "加载中..." : emptyLabel}</div>;
  }

  return (
    <div className="table-scroll">
      <table className="calls-table">
        <thead>
          <tr>
            <th>开始时间</th>
            <th>Provider</th>
            <th>模型</th>
            <th>Agent</th>
            <th>工作流</th>
            <th>Token</th>
            <th>延迟</th>
            <th>状态</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={row.id}>
              <td>{row.started_at.replace("T", " ").slice(0, 19)}</td>
              <td>{row.provider}</td>
              <td>{row.model_response ?? row.model_requested ?? "未知"}</td>
              <td>{row.agent_id ?? "未知"}</td>
              <td>{row.workflow_id ?? "未知"}</td>
              <td>{formatInteger(row.total_tokens)}</td>
              <td>{formatLatency(row.latency_ms)}</td>
              <td>
                <span className={`status ${row.status}`}>
                  {statusLabels[row.status] ?? row.status}
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
  title = "最近调用",
}: RecentCallsTableProps) {
  return (
    <section className="panel">
      <div className="panel-heading">
        <h2>{title}</h2>
      </div>
      <CallsTable emptyLabel={emptyLabel} isLoading={isLoading} rows={rows} />
    </section>
  );
}
