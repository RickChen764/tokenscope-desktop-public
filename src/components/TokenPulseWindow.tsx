import { getCurrentWindow, LogicalPosition } from "@tauri-apps/api/window";
import { useEffect, useMemo, useRef, useState, type MouseEvent, type PointerEvent } from "react";
import { useI18n } from "../i18n";
import {
  getTokenPulse,
  getTokenPulsePositionLocked,
  setTokenPulseDetailHovered,
  setTokenPulseDragging,
  setTokenPulsePositionLocked,
  showTokenPulseContextMenu,
} from "../services/dashboard";
import type { TokenPulseSnapshot } from "../types/dashboard";
import { formatCompactToken, formatInteger } from "../utils/format";

const TOKEN_PULSE_REFRESH_MS = 60000;
const TOKEN_PULSE_HISTORY_DAYS = 30;

type TokenPulseHoverSource = "mini" | "detail";

type HourlyTokenBar = {
  hour: number;
  tokens: number;
  height: number;
  tier: "high" | "mid" | "low";
};

type TokenPulseDragSession = {
  pointerId: number;
  startPointerX: number;
  startPointerY: number;
  startWindowX: number;
  startWindowY: number;
};

type TokenPulseViewModel = {
  averageLabel: string;
  hourlyBars: HourlyTokenBar[];
  isLoading: boolean;
  progressPercent: number;
  ratioLabel: string;
  remainingLabel: string;
  snapshot: TokenPulseSnapshot;
  todayDateLabel: string;
  todayLabel: string;
  todayTitle: string;
  trendDirection: "up" | "down";
  trendIcon: string;
  yesterdayLabel: string;
};

function emptyPulseSnapshot(): TokenPulseSnapshot {
  const today = new Date();
  const todayLocal = `${today.getFullYear()}-${`${today.getMonth() + 1}`.padStart(2, "0")}-${`${today.getDate()}`.padStart(2, "0")}`;

  return {
    today_local: todayLocal,
    today_tokens: 0,
    today_calls: 0,
    yesterday_tokens: 0,
    average_daily_tokens: 0,
    history_days: TOKEN_PULSE_HISTORY_DAYS,
    ratio_to_average: null,
    remaining_to_average: 0,
    hourly_tokens: [],
  };
}

function formatRatio(snapshot: TokenPulseSnapshot, locale: string) {
  if (snapshot.ratio_to_average === null) {
    return "0%";
  }

  return new Intl.NumberFormat(locale, {
    style: "percent",
    maximumFractionDigits: 0,
  }).format(snapshot.ratio_to_average);
}

function getProgressPercent(snapshot: TokenPulseSnapshot) {
  if (snapshot.ratio_to_average === null) {
    return 0;
  }

  return Math.max(0, Math.min(100, snapshot.ratio_to_average * 100));
}

function buildHourlyBars(snapshot: TokenPulseSnapshot): HourlyTokenBar[] {
  const byHour = new Map(snapshot.hourly_tokens.map((point) => [point.hour, point.total_tokens]));
  const values = Array.from({ length: 24 }, (_, hour) => ({
    hour,
    tokens: byHour.get(hour) ?? 0,
  }));
  const maxValue = Math.max(...values.map((point) => point.tokens), 1);
  const hourlyAverage = snapshot.average_daily_tokens > 0 ? snapshot.average_daily_tokens / 24 : 0;

  return values.map((point) => {
    const height = point.tokens > 0 ? Math.max(18, Math.round((point.tokens / maxValue) * 100)) : 8;
    const tier =
      point.tokens <= 0
        ? "low"
        : point.tokens >= Math.max(hourlyAverage, maxValue * 0.72)
          ? "high"
          : "mid";

    return {
      ...point,
      height,
      tier,
    };
  });
}

function usePulseSnapshot() {
  const [snapshot, setSnapshot] = useState<TokenPulseSnapshot>(() => emptyPulseSnapshot());
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    let isMounted = true;

    async function loadTokenPulse() {
      const nextSnapshot = await getTokenPulse(TOKEN_PULSE_HISTORY_DAYS);
      if (isMounted) {
        setSnapshot(nextSnapshot);
        setIsLoading(false);
      }
    }

    void loadTokenPulse();
    const refreshTimer = window.setInterval(() => {
      void loadTokenPulse();
    }, TOKEN_PULSE_REFRESH_MS);

    return () => {
      isMounted = false;
      window.clearInterval(refreshTimer);
    };
  }, []);

  return { snapshot, isLoading };
}

function useTokenPulseViewModel(): TokenPulseViewModel {
  const { numberLocale } = useI18n();
  const { snapshot, isLoading } = usePulseSnapshot();
  const hourlyBars = useMemo(() => buildHourlyBars(snapshot), [snapshot]);
  const progressPercent = getProgressPercent(snapshot);
  const ratioLabel = formatRatio(snapshot, numberLocale);
  const todayLabel = formatCompactToken(snapshot.today_tokens, numberLocale);
  const todayTitle = `${formatInteger(snapshot.today_tokens, numberLocale)} Token`;
  const averageLabel = formatCompactToken(snapshot.average_daily_tokens, numberLocale);
  const yesterdayLabel = formatCompactToken(snapshot.yesterday_tokens, numberLocale);
  const remainingLabel = formatCompactToken(snapshot.remaining_to_average, numberLocale);
  const todayDateLabel = snapshot.today_local.slice(5);
  const trendDirection =
    snapshot.average_daily_tokens > 0
      ? snapshot.today_tokens >= snapshot.average_daily_tokens
        ? "up"
        : "down"
      : snapshot.today_tokens >= snapshot.yesterday_tokens
        ? "up"
        : "down";
  const trendIcon = trendDirection === "up" ? "↑" : "↘";

  return {
    averageLabel,
    hourlyBars,
    isLoading,
    progressPercent,
    ratioLabel,
    remainingLabel,
    snapshot,
    todayDateLabel,
    todayLabel,
    todayTitle,
    trendDirection,
    trendIcon,
    yesterdayLabel,
  };
}

function useTokenPulseBodyClass() {
  useEffect(() => {
    document.body.classList.add("token-pulse-body");
    return () => document.body.classList.remove("token-pulse-body");
  }, []);
}

function useTokenPulseHover(source: TokenPulseHoverSource, disabled = false) {
  function setHovered(hovered: boolean) {
    if (hovered && disabled) {
      return;
    }

    void setTokenPulseDetailHovered(source, hovered).catch(() => {
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

function useTokenPulseDrag(isPositionLocked: boolean, refreshPositionLocked: () => Promise<boolean>) {
  const dragSessionRef = useRef<TokenPulseDragSession | null>(null);
  const [isDragging, setIsDragging] = useState(false);

  function finishDrag(pointerId?: number, target?: HTMLElement) {
    const dragSession = dragSessionRef.current;
    if (dragSession && pointerId !== undefined && dragSession.pointerId !== pointerId) {
      return;
    }

    dragSessionRef.current = null;
    if (target && pointerId !== undefined && target.hasPointerCapture(pointerId)) {
      target.releasePointerCapture(pointerId);
    }
    setIsDragging(false);
    void setTokenPulseDragging(false).catch(() => {
      // Browser preview renders the window without desktop dragging commands.
    });
  }

  useEffect(() => {
    function finishWindowDrag() {
      finishDrag();
    }

    window.addEventListener("pointerup", finishWindowDrag);
    window.addEventListener("blur", finishWindowDrag);
    return () => {
      window.removeEventListener("pointerup", finishWindowDrag);
      window.removeEventListener("blur", finishWindowDrag);
    };
  }, []);

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
    event.currentTarget.setPointerCapture(event.pointerId);
    setIsDragging(true);
    void setTokenPulseDragging(true).catch(() => {
      // Browser preview renders the window without desktop dragging commands.
    });

    try {
      const currentWindow = getCurrentWindow();
      const [position, scaleFactor] = await Promise.all([
        currentWindow.outerPosition(),
        currentWindow.scaleFactor(),
      ]);
      dragSessionRef.current = {
        pointerId: event.pointerId,
        startPointerX: event.screenX,
        startPointerY: event.screenY,
        startWindowX: position.x / scaleFactor,
        startWindowY: position.y / scaleFactor,
      };
    } catch {
      finishDrag(event.pointerId, event.currentTarget);
    }
  }

  function moveDrag(event: PointerEvent<HTMLElement>) {
    const dragSession = dragSessionRef.current;
    if (!dragSession || dragSession.pointerId !== event.pointerId) {
      return;
    }

    const nextX = dragSession.startWindowX + event.screenX - dragSession.startPointerX;
    const nextY = dragSession.startWindowY + event.screenY - dragSession.startPointerY;
    void getCurrentWindow()
      .setPosition(new LogicalPosition(nextX, nextY))
      .catch(() => {
        // Browser preview renders the window without desktop positioning commands.
      });
  }

  function endDrag(event: PointerEvent<HTMLElement>) {
    finishDrag(event.pointerId, event.currentTarget);
  }

  return {
    dragHandlers: {
      onPointerDown: beginDrag,
      onPointerCancel: endDrag,
      onLostPointerCapture: endDrag,
      onPointerMove: moveDrag,
      onPointerUp: endDrag,
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

function TokenPulseMini({
  dragHandlers,
  isDragging,
  isPositionLocked,
  onContextMenu,
  viewModel,
}: {
  dragHandlers: ReturnType<typeof useTokenPulseDrag>["dragHandlers"];
  isDragging: boolean;
  isPositionLocked: boolean;
  onContextMenu: (event: MouseEvent<HTMLElement>) => void;
  viewModel: TokenPulseViewModel;
}) {
  return (
    <section
      className={`token-pulse-mini token-pulse-drag-handle${isDragging ? " is-dragging" : ""}${isPositionLocked ? " token-pulse-locked" : ""}`}
      data-tauri-drag-region
      onContextMenu={onContextMenu}
      {...dragHandlers}
    >
      <span className="token-pulse-database-icon" aria-hidden="true" />
      <div className="token-pulse-mini-main">
        <div className="token-pulse-mini-row">
          <span>今日</span>
          <strong aria-label={viewModel.todayTitle}>{viewModel.todayLabel}</strong>
          <span>日均 {viewModel.averageLabel}</span>
          <b className={viewModel.trendDirection}>{viewModel.trendIcon}</b>
        </div>
        <div className="token-pulse-meter" aria-hidden="true">
          <i style={{ width: `${viewModel.progressPercent}%` }} />
        </div>
      </div>
    </section>
  );
}

function TokenPulseDetail({ viewModel }: { viewModel: TokenPulseViewModel }) {
  return (
    <section className="token-pulse-detail">
      <div className="token-pulse-detail-head">
        <span className="pulse-detail-icon" aria-hidden="true">
          <i />
          <i />
          <i />
        </span>
        <span>今日用量</span>
        <small>{viewModel.todayDateLabel}（实时）</small>
      </div>
      <div className="token-pulse-detail-main">
        <strong aria-label={viewModel.todayTitle}>{viewModel.todayLabel}</strong>
        <span>
          占日均的 <b>{viewModel.isLoading ? "同步中" : viewModel.ratioLabel}</b>
        </span>
        <i className={`pulse-trend-mark ${viewModel.trendDirection}`} aria-hidden="true" />
      </div>
      <div className="token-pulse-meter detail-meter" aria-hidden="true">
        <i style={{ width: `${viewModel.progressPercent}%` }} />
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
          <strong>{viewModel.snapshot.remaining_to_average > 0 ? viewModel.remainingLabel : "0"}</strong>
        </div>
      </div>
      <div className="token-pulse-hour-label">今日每小时用量</div>
      <div className="token-pulse-hours detail-hours" aria-hidden="true">
        {viewModel.hourlyBars.map((point) => (
          <i
            className={point.tier}
            key={point.hour}
            style={{ height: `${point.height}%` }}
            title={`${point.hour}:00 ${formatInteger(point.tokens, "zh-CN")} Token`}
          />
        ))}
      </div>
      <div className="token-pulse-hour-axis" aria-hidden="true">
        <span>0</span>
        <span>6</span>
        <span>12</span>
        <span>18</span>
        <span>24</span>
      </div>
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
        <span>
          <i className="average" />日均水平
        </span>
      </div>
    </section>
  );
}

export function TokenPulseWindow() {
  useTokenPulseBodyClass();
  const viewModel = useTokenPulseViewModel();
  const { isPositionLocked, refreshPositionLocked } = useTokenPulsePositionLock();
  const { dragHandlers, isDragging } = useTokenPulseDrag(isPositionLocked, refreshPositionLocked);
  const hoverHandlers = useTokenPulseHover("mini", isDragging);

  function handleContextMenu(event: MouseEvent<HTMLElement>) {
    event.preventDefault();
    void showTokenPulseContextMenu()
      .then(() => refreshPositionLocked())
      .catch(() => {
        // Browser preview renders the window without desktop context menus.
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
        onContextMenu={handleContextMenu}
        viewModel={viewModel}
      />
    </main>
  );
}

export function TokenPulseDetailWindow() {
  useTokenPulseBodyClass();
  const viewModel = useTokenPulseViewModel();
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
