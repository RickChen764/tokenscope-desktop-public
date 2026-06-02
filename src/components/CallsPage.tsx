import { useCallback, useEffect, useMemo, useState } from "react";
import { exportCallsCsv, getCallFilterOptions, listLlmCalls } from "../services/dashboard";
import type {
  CallFilterOptions,
  DashboardRange,
  LlmCallFilters,
  LlmCallPage,
} from "../types/dashboard";
import { useI18n } from "../i18n";
import { getLocalDateWindow } from "../utils/date";
import { CallsTable } from "./RecentCallsTable";

const pageSizes = [10, 20, 50];

const emptyOptions: CallFilterOptions = {
  providers: [],
  agents: [],
  models: [],
  statuses: [],
};

const emptyPage: LlmCallPage = {
  rows: [],
  total: 0,
};

function emptyToNull(value: string) {
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function statusLabel(status: string, t: (message: string) => string) {
  const labels: Record<string, string> = {
    error: t("失败"),
    success: t("成功"),
  };
  return labels[status] ?? status;
}

export function CallsPage() {
  const { t } = useI18n();
  const [range, setRange] = useState<DashboardRange>("7d");
  const [provider, setProvider] = useState("");
  const [agentId, setAgentId] = useState("");
  const [model, setModel] = useState("");
  const [status, setStatus] = useState("");
  const [pageSize, setPageSize] = useState(20);
  const [pageIndex, setPageIndex] = useState(0);
  const [page, setPage] = useState<LlmCallPage>(emptyPage);
  const [options, setOptions] = useState<CallFilterOptions>(emptyOptions);
  const [isLoading, setIsLoading] = useState(true);
  const [isExporting, setIsExporting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [exportNotice, setExportNotice] = useState<string | null>(null);

  const dateWindow = useMemo(() => getLocalDateWindow(range), [range]);

  const filters = useMemo<LlmCallFilters>(
    () => ({
      from: dateWindow.from,
      to: dateWindow.to,
      provider: emptyToNull(provider),
      agent_id: emptyToNull(agentId),
      model: emptyToNull(model),
      status: emptyToNull(status),
      limit: pageSize,
      offset: pageIndex * pageSize,
    }),
    [agentId, dateWindow.from, dateWindow.to, model, pageIndex, pageSize, provider, status],
  );

  const loadCalls = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const nextPage = await listLlmCalls(filters);
      setPage(nextPage);
    } catch (err) {
      setError(t("加载调用明细失败：{error}", {
        error: err instanceof Error ? err.message : String(err),
      }));
    } finally {
      setIsLoading(false);
    }
  }, [filters]);

  useEffect(() => {
    void loadCalls();
  }, [loadCalls]);

  useEffect(() => {
    let ignore = false;
    async function loadOptions() {
      try {
        const nextOptions = await getCallFilterOptions();
        if (!ignore) {
          setOptions(nextOptions);
        }
      } catch (err) {
        if (!ignore) {
          setError(t("加载筛选项失败：{error}", {
            error: err instanceof Error ? err.message : String(err),
          }));
        }
      }
    }

    void loadOptions();
    return () => {
      ignore = true;
    };
  }, []);

  function resetPageAnd<T>(setter: (value: T) => void, value: T) {
    setPageIndex(0);
    setter(value);
  }

  function handleReset() {
    setRange("7d");
    setProvider("");
    setAgentId("");
    setModel("");
    setStatus("");
    setPageSize(20);
    setPageIndex(0);
    setExportNotice(null);
  }

  async function handleExportCurrentFilters() {
    setIsExporting(true);
    setError(null);
    setExportNotice(null);
    try {
      const path = await exportCallsCsv(filters);
      setExportNotice(t("CSV 已导出：{path}", { path }));
    } catch (err) {
      setError(t("导出当前筛选 CSV 失败：{error}", {
        error: err instanceof Error ? err.message : String(err),
      }));
    } finally {
      setIsExporting(false);
    }
  }

  const pageStart = page.total === 0 ? 0 : filters.offset + 1;
  const pageEnd = Math.min(filters.offset + page.rows.length, page.total);
  const canGoPrevious = pageIndex > 0 && !isLoading;
  const canGoNext = filters.offset + page.rows.length < page.total && !isLoading;

  const rangeLabels: Record<DashboardRange, string> = {
    today: t("今日"),
    "7d": t("近 7 天"),
    "30d": t("近 30 天"),
    "90d": t("近 90 天"),
  };

  return (
    <section className="panel calls-page">
      <div className="panel-heading calls-heading">
        <div>
          <h2>{t("筛选结果")}</h2>
          <p>{t("按时间、来源和状态查看本地记录的调用元数据。")}</p>
        </div>
        <div className="heading-actions">
          <button
            className="primary"
            disabled={isExporting}
            onClick={() => void handleExportCurrentFilters()}
            type="button"
          >
            {isExporting ? t("导出中...") : t("导出当前筛选 CSV")}
          </button>
          <button className="primary secondary" onClick={handleReset} type="button">
            {t("重置筛选")}
          </button>
        </div>
      </div>

      <div className="calls-filter-bar">
        <div className="filter-control range-control">
          <span>{t("时间")}</span>
          <div className="segmented compact-segmented" aria-label={t("调用日期范围")}>
            {(["today", "7d", "30d", "90d"] as DashboardRange[]).map((option) => (
              <button
                className={option === range ? "active" : ""}
                key={option}
                onClick={() => resetPageAnd(setRange, option)}
                type="button"
              >
                {rangeLabels[option]}
              </button>
            ))}
          </div>
        </div>

        <label className="filter-control">
          <span>Provider</span>
          <select value={provider} onChange={(event) => resetPageAnd(setProvider, event.target.value)}>
            <option value="">{t("全部")}</option>
            {options.providers.map((value) => (
              <option key={value} value={value}>
                {value}
              </option>
            ))}
          </select>
        </label>

        <label className="filter-control">
          <span>Agent</span>
          <select value={agentId} onChange={(event) => resetPageAnd(setAgentId, event.target.value)}>
            <option value="">{t("全部")}</option>
            {options.agents.map((value) => (
              <option key={value} value={value}>
                {value}
              </option>
            ))}
          </select>
        </label>

        <label className="filter-control">
          <span>{t("模型")}</span>
          <select value={model} onChange={(event) => resetPageAnd(setModel, event.target.value)}>
            <option value="">{t("全部")}</option>
            {options.models.map((value) => (
              <option key={value} value={value}>
                {value}
              </option>
            ))}
          </select>
        </label>

        <label className="filter-control">
          <span>{t("状态")}</span>
          <select value={status} onChange={(event) => resetPageAnd(setStatus, event.target.value)}>
            <option value="">{t("全部")}</option>
            {options.statuses.map((value) => (
              <option key={value} value={value}>
                {statusLabel(value, t)}
              </option>
            ))}
          </select>
        </label>
      </div>

      {error ? <div className="notice error inline-notice">{error}</div> : null}
      {exportNotice ? <div className="notice success inline-notice">{exportNotice}</div> : null}

      <CallsTable
        emptyLabel={t("当前筛选条件下暂无调用记录")}
        isLoading={isLoading}
        rows={page.rows}
      />

      <div className="pagination-bar">
        <span>
          {pageStart}-{pageEnd} / {page.total}
        </span>
        <div className="pagination-controls">
          <label>
            {t("每页")}
            <select
              value={pageSize}
              onChange={(event) => {
                setPageIndex(0);
                setPageSize(Number(event.target.value));
              }}
            >
              {pageSizes.map((value) => (
                <option key={value} value={value}>
                  {value}
                </option>
              ))}
            </select>
          </label>
          <button
            className="pagination-button"
            disabled={!canGoPrevious}
            onClick={() => setPageIndex((value) => Math.max(0, value - 1))}
            type="button"
          >
            {t("上一页")}
          </button>
          <button
            className="pagination-button"
            disabled={!canGoNext}
            onClick={() => setPageIndex((value) => value + 1)}
            type="button"
          >
            {t("下一页")}
          </button>
        </div>
      </div>
    </section>
  );
}
