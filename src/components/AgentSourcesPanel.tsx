import type { AgentSourceSummary } from "../types/dashboard";
import { useI18n } from "../i18n";
import { formatInteger } from "../utils/format";

interface AgentSourcesPanelProps {
  isDetecting: boolean;
  isImporting: boolean;
  isLoading: boolean;
  onDetect: () => void;
  onImport: () => void;
  sources: AgentSourceSummary[];
}

function sourceStatus(source: AgentSourceSummary, t: (message: string) => string) {
  if (!source.detected) {
    return { className: "missing", label: t("未找到") };
  }
  if (!source.import_supported) {
    return { className: "unsupported", label: t("暂不支持导入") };
  }
  if (source.imported_calls > 0) {
    return { className: "synced", label: t("已同步") };
  }
  return { className: "ready", label: t("可导入") };
}

function formatDateTime(value: string | null, emptyLabel: string) {
  if (!value) {
    return emptyLabel;
  }

  return value.replace("T", " ").slice(0, 19);
}

function latestDateTime(values: Array<string | null>) {
  const sortedValues = values.filter((value): value is string => Boolean(value)).sort();
  return sortedValues.length > 0 ? sortedValues[sortedValues.length - 1] : null;
}

export function AgentSourcesPanel({
  isDetecting,
  isImporting,
  isLoading,
  onDetect,
  onImport,
  sources,
}: AgentSourcesPanelProps) {
  const { numberLocale, t } = useI18n();
  const detectedCount = sources.filter((source) => source.detected).length;
  const supportedDetectedCount = sources.filter(
    (source) => source.detected && source.import_supported,
  ).length;
  const totalImportedCalls = sources.reduce((total, source) => total + source.imported_calls, 0);
  const lastImportedAt = latestDateTime(sources.map((source) => source.last_imported_at));
  const lastCallAt = latestDateTime(sources.map((source) => source.last_call_at));

  return (
    <section className="panel source-manager">
      <div className="panel-heading source-heading">
        <div>
          <h2>{t("本地 Agent 检测")}</h2>
          <p>{t("检测本机可读取的 Agent 来源路径，并展示已导入到 TokenScope 的同步状态。")}</p>
        </div>
        <div className="agent-actions">
          <button
            className="primary secondary"
            disabled={isDetecting || isImporting || isLoading}
            onClick={onDetect}
            type="button"
          >
            {isDetecting ? t("检测中...") : t("重新检测")}
          </button>
          <button
            className="primary"
            disabled={isImporting || isDetecting || isLoading || supportedDetectedCount === 0}
            onClick={onImport}
            type="button"
          >
            {isImporting ? t("同步中...") : t("手动同步")}
          </button>
        </div>
      </div>

      <div className="source-overview" aria-label={t("本地 Agent 检测概览")}>
        <div>
          <span>{t("检测结果")}</span>
          <strong>{isLoading ? t("读取中...") : `${detectedCount}/${sources.length}`}</strong>
        </div>
        <div>
          <span>{t("可同步来源")}</span>
          <strong>{isLoading ? t("读取中...") : formatInteger(supportedDetectedCount, numberLocale)}</strong>
        </div>
        <div>
          <span>{t("导入量")}</span>
          <strong>{isLoading ? t("读取中...") : formatInteger(totalImportedCalls, numberLocale)}</strong>
        </div>
        <div>
          <span>{t("最近导入")}</span>
          <strong>{isLoading ? t("读取中...") : formatDateTime(lastImportedAt, t("无"))}</strong>
        </div>
        <div>
          <span>{t("最近调用")}</span>
          <strong>{isLoading ? t("读取中...") : formatDateTime(lastCallAt, t("无"))}</strong>
        </div>
      </div>

      {isLoading ? <div className="empty-state small">{t("正在读取本机 Agent 来源...")}</div> : null}

      <div className="source-list">
        {!isLoading && sources.length === 0 ? (
          <div className="empty-state small">{t("暂无本地 Agent 来源")}</div>
        ) : null}

        {!isLoading && sources.map((source) => {
          const status = sourceStatus(source, t);
          return (
            <article className="source-row" key={source.id}>
              <div className="source-main">
                <div className="source-title">
                  <strong>{source.name}</strong>
                  <span className={`agent-state ${status.className}`}>{status.label}</span>
                </div>
                <p className="source-message">{source.message}</p>
                <div className="source-path">
                  <span>{t("来源路径")}</span>
                  <code title={source.source_path ?? t("未发现本地数据库路径")}>
                    {source.source_path ?? t("未发现本地数据库路径")}
                  </code>
                </div>
              </div>

              <div className="source-stats" aria-label={`${source.name} ${t("导入统计")}`}>
                <div className="source-stat">
                  <span>{t("导入量")}</span>
                  <strong>{formatInteger(source.imported_calls, numberLocale)}</strong>
                </div>
                <div className="source-stat">
                  <span>Token</span>
                  <strong>{formatInteger(source.total_tokens, numberLocale)}</strong>
                </div>
                <div className="source-stat wide">
                  <span>{t("最近导入")}</span>
                  <strong>{formatDateTime(source.last_imported_at, t("无"))}</strong>
                </div>
                <div className="source-stat wide">
                  <span>{t("最近调用")}</span>
                  <strong>{formatDateTime(source.last_call_at, t("无"))}</strong>
                </div>
              </div>
            </article>
          );
        })}
      </div>
    </section>
  );
}
