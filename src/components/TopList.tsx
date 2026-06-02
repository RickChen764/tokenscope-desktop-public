import type { DimensionKind, TopDimensionRow } from "../types/dashboard";
import { formatInteger } from "../utils/format";

interface TopListProps {
  isLoading: boolean;
  kind?: DimensionKind;
  onRowClick?: (dimension: string) => void;
  rows: TopDimensionRow[];
  title: string;
}

export function TopList({ isLoading, kind, onRowClick, rows, title }: TopListProps) {
  return (
    <section className="panel compact" aria-busy={isLoading}>
      <div className="panel-heading">
        <h2>{title}</h2>
      </div>
      {rows.length === 0 ? (
        <div className="empty-state small">{isLoading ? "加载中..." : "暂无数据"}</div>
      ) : (
        <table className="compact-table top-list-table">
          <tbody>
            {rows.map((row) => {
              const displayLabel = formatTopDimensionLabel(row.dimension, kind);

              return (
                <tr
                  className={onRowClick ? "clickable-row" : ""}
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
                  <td className="top-list-label-cell">
                    <span className="top-list-label" title={row.dimension}>
                      {displayLabel}
                    </span>
                  </td>
                  <td className="top-list-value">{formatInteger(row.total_tokens)}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
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
