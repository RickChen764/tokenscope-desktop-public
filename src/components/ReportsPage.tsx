import { useMemo, useState } from "react";
import { exportCallsCsv } from "../services/dashboard";
import type { DashboardRange, LlmCallFilters } from "../types/dashboard";
import { useI18n } from "../i18n";
import { getLocalDateWindow } from "../utils/date";

export function ReportsPage() {
  const { t } = useI18n();
  const [range, setRange] = useState<DashboardRange>("30d");
  const [isExporting, setIsExporting] = useState(false);
  const [notice, setNotice] = useState<{ kind: "error" | "success"; message: string } | null>(
    null,
  );

  const dateWindow = useMemo(() => getLocalDateWindow(range), [range]);

  async function handleExport() {
    setIsExporting(true);
    setNotice(null);
    const filters: Partial<LlmCallFilters> = {
      from: dateWindow.from,
      to: dateWindow.to,
      provider: null,
      agent_id: null,
      model: null,
      status: null,
    };

    try {
      const path = await exportCallsCsv(filters);
      setNotice({ kind: "success", message: t("CSV 已导出：{path}", { path }) });
    } catch (err) {
      setNotice({
        kind: "error",
        message: t("导出报表失败：{error}", {
          error: err instanceof Error ? err.message : String(err),
        }),
      });
    } finally {
      setIsExporting(false);
    }
  }

  const rangeLabels: Record<DashboardRange, string> = {
    today: t("今日"),
    "7d": t("近 7 天"),
    "30d": t("近 30 天"),
    "90d": t("近 90 天"),
  };

  return (
    <section className="reports-page">
      {notice ? <div className={`notice ${notice.kind} inline-notice`}>{notice.message}</div> : null}

      <section className="panel report-export-panel">
        <div>
          <p className="eyebrow">Reports</p>
          <h2>{t("报表导出")}</h2>
          <p>{t("导出本地已统计的调用元数据、Token 和状态，用于审计或进一步分析。")}</p>
        </div>
        <div className="report-export-controls">
          <div className="segmented compact-segmented" aria-label={t("报表日期范围")}>
            {(["today", "7d", "30d", "90d"] as DashboardRange[]).map((option) => (
              <button
                className={option === range ? "active" : ""}
                key={option}
                onClick={() => setRange(option)}
                type="button"
              >
                {rangeLabels[option]}
              </button>
            ))}
          </div>
          <button className="primary" disabled={isExporting} onClick={() => void handleExport()} type="button">
            {isExporting ? t("导出中...") : t("导出 CSV")}
          </button>
        </div>
      </section>

      <section className="settings-grid compact-grid">
        <section className="panel settings-utility">
          <div>
            <p className="eyebrow">Export Scope</p>
            <h2>{t("导出内容")}</h2>
          </div>
          <div className="detail-stat-list">
            <div>
              <span>{t("时间范围")}</span>
              <strong>
                {dateWindow.from} {t("至")} {dateWindow.to}
              </strong>
            </div>
            <div>
              <span>{t("字段")}</span>
              <strong>{t("调用元数据、Token、状态")}</strong>
            </div>
          </div>
        </section>

        <section className="panel settings-utility">
          <div>
            <p className="eyebrow">Privacy Boundary</p>
            <h2>{t("隐私边界")}</h2>
            <p>{t("导出面向统计分析，不包含明文 prompt、response 或 Authorization。")}</p>
          </div>
        </section>
      </section>
    </section>
  );
}
