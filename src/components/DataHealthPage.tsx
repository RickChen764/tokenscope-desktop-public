import { useCallback, useEffect, useState } from "react";
import {
  getDataHealthSummary,
  listDataHealthIssues,
} from "../services/dashboard";
import type { DataHealthIssueRow, DataHealthSummary } from "../types/dashboard";
import { formatInteger, formatPercent } from "../utils/format";

const emptySummary: DataHealthSummary = {
  total_calls: 0,
  issue_calls: 0,
  issues: [],
};

const issueLabels: Record<string, string> = {
  failed_call: "失败调用",
  missing_model: "缺少模型",
  missing_tokens: "缺少 Token",
};

const issueDescriptions: Record<string, string> = {
  failed_call: "调用状态不是 success，可能需要单独排查失败率。",
  missing_model: "记录没有可用模型名，模型维度分析会缺失。",
  missing_tokens: "记录没有有效 Token 数，Token 报表会被低估。",
};

function issueLabel(type: string) {
  return issueLabels[type] ?? type;
}

function issueDetail(row: DataHealthIssueRow) {
  const model = row.model ?? "未知模型";
  const source = row.agent_id ?? row.workflow_id ?? row.project_id ?? row.session_id ?? "未标注来源";
  return `${row.provider} / ${model} / ${source}`;
}

export function DataHealthPage() {
  const [summary, setSummary] = useState<DataHealthSummary>(emptySummary);
  const [issues, setIssues] = useState<DataHealthIssueRow[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const loadHealth = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const [nextSummary, nextIssues] = await Promise.all([
        getDataHealthSummary(),
        listDataHealthIssues(),
      ]);
      setSummary(nextSummary);
      setIssues(nextIssues);
    } catch (err) {
      setError(`加载数据健康状态失败：${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadHealth();
  }, [loadHealth]);

  const healthyRate =
    summary.total_calls > 0
      ? (summary.total_calls - summary.issue_calls) / summary.total_calls
      : 1;

  return (
    <section className="data-health-page">
      {error ? <div className="notice error inline-notice">{error}</div> : null}

      <section className="health-grid">
        <section className="panel health-hero">
          <div>
            <p className="eyebrow">Data Health</p>
            <h2>数据健康检查</h2>
            <p>
              检查本地调用记录是否存在缺少模型、缺少 Token 和失败调用等问题。
            </p>
          </div>
          <button className="primary secondary" onClick={() => void loadHealth()} type="button">
            {isLoading ? "刷新中..." : "刷新状态"}
          </button>
        </section>

        <section className="panel compact">
          <div className="panel-heading">
            <h2>问题分布</h2>
          </div>
          <div className="detail-stat-list">
            {summary.issues.length === 0 ? (
              <div>
                <span>当前状态</span>
                <strong>{isLoading ? "加载中..." : "未发现问题"}</strong>
              </div>
            ) : (
              summary.issues.map((issue) => (
                <div key={issue.issue_type}>
                  <span>{issueLabel(issue.issue_type)}</span>
                  <strong>{formatInteger(issue.calls)}</strong>
                </div>
              ))
            )}
          </div>
        </section>
      </section>

      <section className="summary-grid compact-summary">
        <div className="summary-card">
          <span>调用记录</span>
          <strong>{isLoading ? "加载中..." : formatInteger(summary.total_calls)}</strong>
        </div>
        <div className="summary-card">
          <span>问题调用</span>
          <strong>{isLoading ? "加载中..." : formatInteger(summary.issue_calls)}</strong>
        </div>
        <div className="summary-card">
          <span>健康率</span>
          <strong>{isLoading ? "加载中..." : formatPercent(healthyRate)}</strong>
        </div>
      </section>

      <section className="panel">
        <div className="panel-heading">
          <h2>健康问题</h2>
          <span className="panel-meta">{formatInteger(issues.length)} 条</span>
        </div>
        {issues.length === 0 ? (
          <div className="empty-state">
            {isLoading ? "加载中..." : "暂无需要处理的数据健康问题"}
          </div>
        ) : (
          <div className="issue-list">
            {issues.map((issue) => (
              <article className="issue-row" key={`${issue.call_id}:${issue.issue_type}`}>
                <span className="issue-severity warning">{issueLabel(issue.issue_type)}</span>
                <div>
                  <strong>{issue.call_id}</strong>
                  <p>{issueDescriptions[issue.issue_type] ?? issue.issue_type}</p>
                  <p>{issueDetail(issue)}</p>
                </div>
                <span className="panel-meta">
                  {formatInteger(issue.total_tokens)} Token
                </span>
              </article>
            ))}
          </div>
        )}
      </section>
    </section>
  );
}
