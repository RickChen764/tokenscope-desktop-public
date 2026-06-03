import type { DimensionKind, TopDimensionRow } from "../types/dashboard";
import { useI18n } from "../i18n";
import { formatCompactToken, formatInteger } from "../utils/format";

interface TopListProps {
  dimensionLabel?: string;
  footerLabel?: string;
  isLoading: boolean;
  kind?: DimensionKind;
  maxRows?: number;
  onRowClick?: (dimension: string) => void;
  onViewAll?: () => void;
  rows: TopDimensionRow[];
  title: string;
  valueLabel?: string;
  variant?: "default" | "overview";
}

export function TopList({
  dimensionLabel,
  footerLabel,
  isLoading,
  kind,
  maxRows,
  onRowClick,
  onViewAll,
  rows,
  title,
  valueLabel,
  variant = "default",
}: TopListProps) {
  const { numberLocale, t } = useI18n();
  const visibleRows = typeof maxRows === "number" ? rows.slice(0, maxRows) : rows;
  const resolvedDimensionLabel = dimensionLabel ?? title;
  const resolvedValueLabel = valueLabel ?? t("Token 用量");

  return (
    <section className={`panel compact top-list-card ${variant === "overview" ? "overview-rank-card" : ""}`} aria-busy={isLoading}>
      <div className="panel-heading">
        <h2>{title}</h2>
      </div>
      {rows.length === 0 ? (
        <>
          <div className="empty-state small">{isLoading ? t("加载中...") : t("暂无数据")}</div>
          {onViewAll ? (
            <button className="top-list-footer" onClick={onViewAll} type="button">
              <span>{footerLabel ?? t("进入分析")}</span>
              <span aria-hidden="true">›</span>
            </button>
          ) : null}
        </>
      ) : (
        <>
          <table className="compact-table top-list-table">
            {variant === "overview" ? (
              <thead>
                <tr>
                  <th className="top-list-rank-head" aria-label={t("排名")} />
                  <th>{resolvedDimensionLabel}</th>
                  <th>{resolvedValueLabel}</th>
                </tr>
              </thead>
            ) : null}
            <tbody>
              {visibleRows.map((row, index) => {
                const displayLabel = formatTopDimensionLabel(row.dimension, kind);

                return (
                  <tr
                    className={onRowClick ? "clickable-row" : ""}
                    data-action-label={onRowClick && variant !== "overview" ? t("查看") : undefined}
                    key={row.dimension}
                    onClick={onRowClick ? () => onRowClick(row.dimension) : undefined}
                    onKeyDown={
                      onRowClick
                        ? (event) => {
                            if (event.key === "Enter" || event.key === " ") {
                              event.preventDefault();
                              onRowClick(row.dimension);
                            }
                          }
                        : undefined
                    }
                    tabIndex={onRowClick ? 0 : undefined}
                  >
                    {variant === "overview" ? (
                      <td className="top-list-rank-cell">{index + 1}</td>
                    ) : null}
                    <td className="top-list-label-cell">
                      <span className="top-list-label" title={row.dimension}>
                        {displayLabel}
                      </span>
                    </td>
                    <td className="top-list-value" title={formatInteger(row.total_tokens, numberLocale)}>
                      {variant === "overview"
                        ? formatCompactToken(row.total_tokens, numberLocale)
                        : formatInteger(row.total_tokens, numberLocale)}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          {onViewAll ? (
            <button className="top-list-footer" onClick={onViewAll} type="button">
              <span>{footerLabel ?? t("进入分析")}</span>
              <span aria-hidden="true">›</span>
            </button>
          ) : null}
        </>
      )}
    </section>
  );
}

function formatTopDimensionLabel(value: string, kind?: DimensionKind) {
  if (kind !== "session") {
    return value;
  }

  return shortenMiddle(value, 8, 6);
}

function shortenMiddle(value: string, prefixLength: number, suffixLength: number) {
  const trimmed = value.trim();
  if (trimmed.length <= prefixLength + suffixLength + 3) {
    return value;
  }

  return `${trimmed.slice(0, prefixLength)}...${trimmed.slice(-suffixLength)}`;
}
