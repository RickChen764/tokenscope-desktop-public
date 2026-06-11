import {
  getCurrentWindow,
  LogicalPosition,
  LogicalSize,
} from "@tauri-apps/api/window";
import {
  useEffect,
  useRef,
  useState,
  type MouseEvent,
  type PointerEvent,
} from "react";
import {
  getTokenPulsePositionLocked,
  hideTokenPulseWindow,
  openTokenPulseHome,
  setTokenPulseDetailHovered,
  setTokenPulseDragging,
  setTokenPulsePositionLocked,
  showTokenPulseContextMenu,
  syncTodayTokenPulseData,
} from "../services/dashboard";
import { formatInteger } from "../utils/format";
import { CodexUsageLimitsPanel } from "./tokenPulse/CodexUsageLimitsPanel";
import { useTokenPulseViewModel } from "./tokenPulse/hooks";
import {
  PulseDetailIcon,
  TokenPulseActionIcon,
  TokenPulseTrendIcon,
  type TokenPulseActionIconKind,
} from "./tokenPulse/Icons";
import { CodexUsageLimitRing, TokenPulseRing } from "./tokenPulse/Rings";
import { type TokenPulseViewModel } from "./tokenPulse/viewModel";

const TOKEN_PULSE_MINI_WIDTH = 284;
const TOKEN_PULSE_MINI_CODEX_WIDTH = 444;
const TOKEN_PULSE_MINI_HEIGHT = 64;
const TOKEN_PULSE_DETAIL_WIDTH = 360;
const TOKEN_PULSE_DETAIL_CODEX_WIDTH = 590;
const TOKEN_PULSE_DETAIL_HEIGHT = 364;

type TokenPulseHoverSource = "mini" | "detail";

function isTokenPulsePerfLogEnabled() {
  if (import.meta.env.DEV) {
    return true;
  }

  try {
    const value = window.localStorage.getItem("tokenscope.perfLog");
    return value === "1" || value === "true";
  } catch {
    return false;
  }
}

function logTokenPulsePerf(stage: string, startedAt: number) {
  if (!isTokenPulsePerfLogEnabled()) {
    return;
  }

  const elapsedMs = Math.round(performance.now() - startedAt);
  console.info(
    `[tokenscope][perf] token_pulse.${stage} elapsed_ms=${elapsedMs}`,
  );
}

function useTokenPulseBodyClass() {
  useEffect(() => {
    document.body.classList.add("token-pulse-body");
    return () => document.body.classList.remove("token-pulse-body");
  }, []);
}

function useTokenPulseWindowSize(width: number, height: number) {
  useEffect(() => {
    async function resizeWindow() {
      try {
        const currentWindow = getCurrentWindow();
        const [position, size, scaleFactor] = await Promise.all([
          currentWindow.outerPosition(),
          currentWindow.outerSize(),
          currentWindow.scaleFactor(),
        ]);
        const currentLogicalWidth = size.width / scaleFactor;
        const nextX = Math.max(
          0,
          position.x / scaleFactor + currentLogicalWidth - width,
        );
        await currentWindow.setSize(new LogicalSize(width, height));
        await currentWindow.setPosition(
          new LogicalPosition(nextX, position.y / scaleFactor),
        );
      } catch {
        // Browser preview renders the window without desktop sizing commands.
      }
    }

    void resizeWindow();
  }, [height, width]);
}

function useTokenPulseHover(
  source: TokenPulseHoverSource,
  disabled = false,
  detailSize?: { detailWidth: number; detailHeight: number },
) {
  function setHovered(hovered: boolean) {
    if (hovered && disabled) {
      return;
    }

    void setTokenPulseDetailHovered(source, hovered, detailSize).catch(() => {
      // Browser preview renders the window without desktop hover commands.
    });
  }

  return {
    onPointerEnter: () => setHovered(true),
    onPointerLeave: () => setHovered(false),
    onMouseEnter: () => setHovered(true),
    onMouseLeave: () => setHovered(false),
  };
}

function useTokenPulseDrag(
  isPositionLocked: boolean,
  refreshPositionLocked: () => Promise<boolean>,
) {
  const [isDragging, setIsDragging] = useState(false);

  function finishDrag() {
    setIsDragging(false);
    void setTokenPulseDragging(false).catch(() => {
      // Browser preview renders the window without desktop dragging commands.
    });
  }

  async function beginDrag(event: PointerEvent<HTMLElement>) {
    if (event.button !== 0) {
      return;
    }

    if (isPositionLocked) {
      return;
    }

    if (await refreshPositionLocked()) {
      return;
    }

    event.preventDefault();
    setIsDragging(true);
    void setTokenPulseDragging(true).catch(() => {
      // Browser preview renders the window without desktop dragging commands.
    });

    try {
      await getCurrentWindow().startDragging();
    } catch {
      // Browser preview renders the window without desktop dragging commands.
    } finally {
      finishDrag();
    }
  }

  return {
    dragHandlers: {
      onPointerDown: beginDrag,
      onPointerCancel: finishDrag,
      onPointerUp: finishDrag,
    },
    isDragging,
  };
}

function useTokenPulsePositionLock() {
  const [isPositionLocked, setIsPositionLocked] = useState(false);

  async function refreshPositionLocked() {
    const locked = await getTokenPulsePositionLocked();
    setIsPositionLocked(locked);
    return locked;
  }

  async function updatePositionLocked(locked: boolean) {
    await setTokenPulsePositionLocked(locked);
    setIsPositionLocked(locked);
  }

  useEffect(() => {
    void refreshPositionLocked().catch(() => {
      setIsPositionLocked(false);
    });
  }, []);

  return { isPositionLocked, refreshPositionLocked, updatePositionLocked };
}

function TokenPulseHourlyChart({
  viewModel,
}: {
  viewModel: TokenPulseViewModel;
}) {
  const chartWidth = 304;
  const chartHeight = 86;
  const plotLeft = 8;
  const plotRight = 296;
  const plotBottom = 58;
  const plotHeight = 48;
  const hourWidth = (plotRight - plotLeft) / 24;
  const barWidth = 7;
  const maxTokens = Math.max(
    ...viewModel.hourlyBars.map((point) => point.tokens),
    1,
  );

  function axisX(hour: number) {
    return plotLeft + (plotRight - plotLeft) * (hour / 24);
  }

  return (
    <svg
      aria-label="今日每小时 Token 用量"
      className="token-pulse-hour-chart"
      role="img"
      viewBox="0 0 304 86"
    >
      <line
        className="token-pulse-chart-baseline"
        x1={plotLeft}
        x2={plotRight}
        y1={plotBottom}
        y2={plotBottom}
      />
      {viewModel.hourlyBars.map((point) => {
        if (point.tokens <= 0) {
          return null;
        }

        const barHeight = Math.max(5, (point.tokens / maxTokens) * plotHeight);
        const x =
          plotLeft + point.hour * hourWidth + (hourWidth - barWidth) / 2;
        const y = plotBottom - barHeight;

        return (
          <rect
            className={`token-pulse-chart-bar ${point.tier}`}
            height={barHeight}
            key={point.hour}
            rx="3.5"
            width={barWidth}
            x={x}
            y={y}
          >
            <title>{`${point.hour}:00 ${formatInteger(point.tokens, "zh-CN")} Token`}</title>
          </rect>
        );
      })}
      {[0, 6, 12, 18, 24].map((hour) => (
        <text
          className="token-pulse-chart-axis"
          key={hour}
          textAnchor={hour === 0 ? "start" : hour === 24 ? "end" : "middle"}
          x={axisX(hour)}
          y="80"
        >
          {hour}
        </text>
      ))}
    </svg>
  );
}

function TokenPulseMini({
  dragHandlers,
  isDragging,
  isPositionLocked,
  isRefreshing,
  onContextMenu,
  onHide,
  onOpenHome,
  onRefresh,
  onToggleLock,
  viewModel,
}: {
  dragHandlers: ReturnType<typeof useTokenPulseDrag>["dragHandlers"];
  isDragging: boolean;
  isPositionLocked: boolean;
  isRefreshing: boolean;
  onContextMenu: (event: MouseEvent<HTMLElement>) => void;
  onHide: () => void;
  onOpenHome: () => void;
  onRefresh: () => void;
  onToggleLock: () => void;
  viewModel: TokenPulseViewModel;
}) {
  function stopActionPointer(event: PointerEvent<HTMLButtonElement>) {
    event.stopPropagation();
  }

  const miniClassName = `${viewModel.showCodexUsageLimits ? "token-pulse-mini has-codex" : "token-pulse-mini"}${isDragging ? " is-dragging" : ""}`;

  return (
    <section className={miniClassName} onContextMenu={onContextMenu}>
      <div
        className={`token-pulse-mini-content token-pulse-drag-handle${isDragging ? " is-dragging" : ""}${isPositionLocked ? " token-pulse-locked" : ""}`}
        {...dragHandlers}
      >
        <TokenPulseRing
          label={`今日已达日均 ${viewModel.ratioLabel}`}
          percent={viewModel.progressPercent}
          tone="today"
        />
        <div className="token-pulse-mini-main">
          <div className="token-pulse-mini-title">
            <span className="token-pulse-mini-label">今日</span>
            <strong aria-label={viewModel.todayTitle}>
              {viewModel.todayLabel}
            </strong>
          </div>
        </div>
        {viewModel.todayDeltaLabel ? (
          <span
            aria-label={viewModel.todayDeltaTitle ?? viewModel.todayDeltaLabel}
            className="token-pulse-delta-chip"
            title={viewModel.todayDeltaTitle ?? viewModel.todayDeltaLabel}
          >
            <TokenPulseTrendIcon direction="up" />
            {viewModel.todayDeltaLabel}
          </span>
        ) : null}
      </div>
      {viewModel.showCodexUsageLimits ? (
        <div className="token-pulse-codex" aria-label="Codex 剩余用量">
          {viewModel.codexUsageLimits ? (
            <div className="token-pulse-codex-rings">
              {viewModel.codexUsageLimits.windows.map((window) => (
                <CodexUsageLimitRing key={window.label} window={window} />
              ))}
            </div>
          ) : (
            <span className="token-pulse-codex-empty">等待订阅余量</span>
          )}
        </div>
      ) : null}
      <div className="token-pulse-action-panel">
        <button
          aria-label="打开主页"
          className="token-pulse-action-button home"
          onClick={onOpenHome}
          onPointerDown={stopActionPointer}
          title="打开主页"
          type="button"
        >
          <TokenPulseActionIcon kind="home" />
        </button>
        <button
          aria-label={isPositionLocked ? "解锁位置" : "锁定位置"}
          className={`token-pulse-action-button lock${isPositionLocked ? " is-active" : ""}`}
          onClick={onToggleLock}
          onPointerDown={stopActionPointer}
          title={isPositionLocked ? "解锁位置" : "锁定位置"}
          type="button"
        >
          <TokenPulseActionIcon kind={isPositionLocked ? "lock" : "unlock"} />
        </button>
        <button
          aria-label="刷新数据"
          className={`token-pulse-action-button refresh${viewModel.isLoading || isRefreshing ? " is-loading" : ""}`}
          disabled={isRefreshing}
          onClick={onRefresh}
          onPointerDown={stopActionPointer}
          title="刷新数据"
          type="button"
        >
          <TokenPulseActionIcon kind="refresh" />
        </button>
        <button
          aria-label="隐藏小窗"
          className="token-pulse-action-button hide"
          onClick={onHide}
          onPointerDown={stopActionPointer}
          title="隐藏小窗"
          type="button"
        >
          <TokenPulseActionIcon kind="hide" />
        </button>
      </div>
    </section>
  );
}

function TokenPulseDetail({ viewModel }: { viewModel: TokenPulseViewModel }) {
  const detailClassName = viewModel.showCodexUsageLimits
    ? "token-pulse-detail has-codex"
    : "token-pulse-detail";
  const detailBodyClassName = `token-pulse-detail-body${viewModel.showCodexUsageLimits ? " has-codex" : ""}`;

  return (
    <section className={detailClassName}>
      <div className={detailBodyClassName}>
        <div className="token-pulse-today-detail">
          <div className="token-pulse-detail-head">
            <PulseDetailIcon />
            <span>今日用量</span>
            <small>{viewModel.todayDateLabel}（实时）</small>
          </div>
          <div className="token-pulse-comparison">
            <span>
              {viewModel.isLoading
                ? "正在刷新用量快照"
                : viewModel.comparisonLabel}
            </span>
          </div>
          <div className="token-pulse-detail-rows">
            <div>
              <span>历史日均（近 {viewModel.snapshot.history_days} 天）</span>
              <strong>{viewModel.averageLabel}</strong>
            </div>
            <div>
              <span>昨日用量（前一日）</span>
              <strong>{viewModel.yesterdayLabel}</strong>
            </div>
            <div>
              <span>距离日均目标还需</span>
              <strong>
                {viewModel.snapshot.remaining_to_average > 0
                  ? viewModel.remainingLabel
                  : "0"}
              </strong>
            </div>
          </div>
          <div className="token-pulse-hour-label">今日每小时用量</div>
          <TokenPulseHourlyChart viewModel={viewModel} />
          <div className="token-pulse-legend">
            <span>
              <i className="high" />高
            </span>
            <span>
              <i className="mid" />中
            </span>
            <span>
              <i className="low" />低
            </span>
          </div>
        </div>
        {viewModel.showCodexUsageLimits ? (
          <CodexUsageLimitsPanel viewModel={viewModel.codexUsageLimits} />
        ) : null}
      </div>
    </section>
  );
}

export function TokenPulseWindow() {
  useTokenPulseBodyClass();
  const viewModel = useTokenPulseViewModel();
  useTokenPulseWindowSize(
    viewModel.showCodexUsageLimits
      ? TOKEN_PULSE_MINI_CODEX_WIDTH
      : TOKEN_PULSE_MINI_WIDTH,
    TOKEN_PULSE_MINI_HEIGHT,
  );
  const { isPositionLocked, refreshPositionLocked, updatePositionLocked } =
    useTokenPulsePositionLock();
  const { dragHandlers, isDragging } = useTokenPulseDrag(
    isPositionLocked,
    refreshPositionLocked,
  );
  const [isManualRefreshing, setIsManualRefreshing] = useState(false);
  const isManualRefreshingRef = useRef(false);
  const hoverHandlers = useTokenPulseHover("mini", isDragging, {
    detailWidth: viewModel.showCodexUsageLimits
      ? TOKEN_PULSE_DETAIL_CODEX_WIDTH
      : TOKEN_PULSE_DETAIL_WIDTH,
    detailHeight: TOKEN_PULSE_DETAIL_HEIGHT,
  });

  function handleContextMenu(event: MouseEvent<HTMLElement>) {
    event.preventDefault();
    void showTokenPulseContextMenu()
      .then(() => refreshPositionLocked())
      .catch(() => {
        // Browser preview renders the window without desktop context menus.
      });
  }

  function handleOpenHome() {
    void openTokenPulseHome().catch(() => {
      // Browser preview renders the window without desktop window commands.
    });
  }

  function handleToggleLock() {
    void updatePositionLocked(!isPositionLocked).catch(() => {
      // Browser preview renders the window without desktop lock commands.
    });
  }

  function handleRefresh() {
    if (isManualRefreshingRef.current) {
      return;
    }

    void refreshAfterManualSync().catch(() => {
      // Browser preview renders the window without desktop data commands.
    });
  }

  async function refreshAfterManualSync() {
    isManualRefreshingRef.current = true;
    setIsManualRefreshing(true);
    const refreshStartedAt = performance.now();
    const finishPulseDeltaAggregation = viewModel.beginPulseDeltaAggregation();
    const codexUsageLimitsRefresh = viewModel.showCodexUsageLimits
      ? viewModel
          .refreshCodexUsageLimits({ forceRefresh: true })
          .finally(() => {
            logTokenPulsePerf("manual_refresh.codex_usage", refreshStartedAt);
          })
      : Promise.resolve();
    try {
      try {
        const todaySyncStartedAt = performance.now();
        try {
          await syncTodayTokenPulseData();
        } finally {
          logTokenPulsePerf("manual_refresh.today_sync", todaySyncStartedAt);
        }
      } finally {
        const pulseStartedAt = performance.now();
        try {
          await Promise.all([
            viewModel.refreshPulseSnapshot({ showDelta: false }).finally(() => {
              logTokenPulsePerf(
                "manual_refresh.pulse_snapshot",
                pulseStartedAt,
              );
            }),
            codexUsageLimitsRefresh,
          ]);
        } finally {
          finishPulseDeltaAggregation();
        }
      }
    } finally {
      logTokenPulsePerf("manual_refresh.total", refreshStartedAt);
      isManualRefreshingRef.current = false;
      setIsManualRefreshing(false);
    }
  }

  function handleHide() {
    void hideTokenPulseWindow().catch(() => {
      // Browser preview renders the window without desktop window commands.
    });
  }

  return (
    <main
      className="token-pulse-shell token-pulse-mini-shell"
      aria-label="今日 Token 用量"
      {...hoverHandlers}
    >
      <TokenPulseMini
        dragHandlers={dragHandlers}
        isDragging={isDragging}
        isPositionLocked={isPositionLocked}
        isRefreshing={isManualRefreshing}
        onContextMenu={handleContextMenu}
        onHide={handleHide}
        onOpenHome={handleOpenHome}
        onRefresh={handleRefresh}
        onToggleLock={handleToggleLock}
        viewModel={viewModel}
      />
    </main>
  );
}

export function TokenPulseDetailWindow() {
  useTokenPulseBodyClass();
  const viewModel = useTokenPulseViewModel();
  useTokenPulseWindowSize(
    viewModel.showCodexUsageLimits
      ? TOKEN_PULSE_DETAIL_CODEX_WIDTH
      : TOKEN_PULSE_DETAIL_WIDTH,
    TOKEN_PULSE_DETAIL_HEIGHT,
  );
  const hoverHandlers = useTokenPulseHover("detail");

  return (
    <main
      className="token-pulse-shell token-pulse-detail-shell"
      aria-label="今日 Token 用量详情"
      {...hoverHandlers}
    >
      <TokenPulseDetail viewModel={viewModel} />
    </main>
  );
}
