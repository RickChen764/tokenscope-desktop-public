import { getCurrentWindow, LogicalPosition, LogicalSize } from "@tauri-apps/api/window";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type MouseEvent,
  type PointerEvent,
} from "react";
import { useI18n } from "../i18n";
import { useDisplayPreference } from "../preferences/display";
import {
  getCodexUsageLimits,
  getTokenPulse,
  getTokenPulsePositionLocked,
  hideTokenPulseWindow,
  openTokenPulseHome,
  runBackgroundSyncOnce,
  setTokenPulseDetailHovered,
  setTokenPulseDragging,
  setTokenPulsePositionLocked,
  showTokenPulseContextMenu,
} from "../services/dashboard";
import type {
  CodexUsageLimitSnapshot,
  CodexUsageLimitWindow,
  TokenPulseSnapshot,
} from "../types/dashboard";
import { formatCompactToken, formatInteger, formatTokenByDisplayMode } from "../utils/format";

const TOKEN_PULSE_REFRESH_MS = 60000;
const TOKEN_PULSE_DELTA_VISIBLE_MS = 10000;
const TOKEN_PULSE_HISTORY_DAYS = 30;
const TOKEN_PULSE_MINI_WIDTH = 284;
const TOKEN_PULSE_MINI_CODEX_WIDTH = 444;
const TOKEN_PULSE_MINI_HEIGHT = 64;
const TOKEN_PULSE_DETAIL_WIDTH = 360;
const TOKEN_PULSE_DETAIL_CODEX_WIDTH = 590;
const TOKEN_PULSE_DETAIL_HEIGHT = 364;

type TokenPulseHoverSource = "mini" | "detail";

type HourlyTokenBar = {
  hour: number;
  tokens: number;
  height: number;
  tier: "high" | "mid" | "low";
};

type TokenPulseActionIconKind = "home" | "lock" | "unlock" | "refresh" | "hide";
type TokenPulseRingTone = "today" | "codex-primary" | "codex-secondary";

const TOKEN_PULSE_TODAY_RING_STAGES = [
  {
    color: "var(--ring-today-green)",
    glow: "rgb(78 201 176 / 22%)",
    track: "var(--ring-track-neutral)",
  },
  {
    color: "var(--ring-boost-orange)",
    glow: "rgb(245 158 11 / 24%)",
    track: "var(--ring-today-green)",
  },
  {
    color: "var(--ring-boost-rose)",
    glow: "rgb(244 63 94 / 24%)",
    track: "var(--ring-boost-orange)",
  },
  {
    color: "var(--ring-boost-gold)",
    glow: "rgb(250 204 21 / 24%)",
    track: "var(--ring-boost-rose)",
  },
  {
    color: "var(--ring-boost-cyan)",
    glow: "rgb(34 211 238 / 24%)",
    track: "var(--ring-boost-gold)",
  },
] as const;

type CodexUsageLimitWindowViewModel = {
  label: string;
  miniLabel: string;
  remainingLabel: string;
  resetLabel: string;
  tone: Extract<TokenPulseRingTone, "codex-primary" | "codex-secondary">;
  usedLabel: string;
  usedPercent: number;
  remainingPercent: number;
};

type CodexUsageLimitsViewModel = {
  capturedLabel: string;
  planLabel: string;
  windows: CodexUsageLimitWindowViewModel[];
};

type PulseSnapshotCursor = {
  todayLocal: string;
  todayTokens: number;
};

type PulseRefreshOptions = {
  showDelta?: boolean;
};

type TokenPulseViewModel = {
  averageLabel: string;
  beginPulseDeltaAggregation: () => () => void;
  codexUsageLimits: CodexUsageLimitsViewModel | null;
  hourlyBars: HourlyTokenBar[];
  isLoading: boolean;
  progressPercent: number;
  comparisonLabel: string;
  ratioLabel: string;
  remainingLabel: string;
  refreshCodexUsageLimits: () => Promise<void>;
  refreshPulseSnapshot: (options?: PulseRefreshOptions) => Promise<void>;
  refreshSnapshot: () => Promise<void>;
  snapshot: TokenPulseSnapshot;
  showCodexUsageLimits: boolean;
  todayDateLabel: string;
  todayDeltaLabel: string | null;
  todayDeltaTitle: string | null;
  todayLabel: string;
  todayTitle: string;
  trendDirection: "up" | "down";
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

  return Math.max(0, snapshot.ratio_to_average * 100);
}

function logTokenPulsePerf(stage: string, startedAt: number) {
  const elapsedMs = Math.round(performance.now() - startedAt);
  console.info(`[tokenscope][perf] token_pulse.${stage} elapsed_ms=${elapsedMs}`);
}

function getRingProgressPercent(percent: number, allowOverflow: boolean) {
  const boundedPercent = Math.max(0, percent);
  if (!allowOverflow) {
    return Math.min(100, boundedPercent);
  }

  const loopPercent = percent % 100;
  if (boundedPercent > 0 && loopPercent === 0) {
    return 100;
  }

  return Math.max(0, loopPercent);
}

function buildTodayRingPalette(percent: number) {
  const boundedPercent = Math.max(0, percent);
  const stageIndex = Math.min(
    Math.max(0, Math.ceil(boundedPercent / 100) - 1),
    TOKEN_PULSE_TODAY_RING_STAGES.length - 1,
  );

  return TOKEN_PULSE_TODAY_RING_STAGES[stageIndex];
}

function formatPercent(value: number, locale: string) {
  return new Intl.NumberFormat(locale, {
    style: "percent",
    maximumFractionDigits: 0,
  }).format(value);
}

function formatCodexPercent(percent: number, locale: string) {
  return new Intl.NumberFormat(locale, {
    style: "percent",
    maximumFractionDigits: 0,
  }).format(Math.max(0, Math.min(100, percent)) / 100);
}

function formatCodexResetLabel(limit: CodexUsageLimitWindow, locale: string) {
  if (!limit.resets_at) {
    return "重置 无";
  }

  const resetDate = new Date(limit.resets_at * 1000);
  if (limit.window_minutes <= 24 * 60) {
    return `重置 ${new Intl.DateTimeFormat(locale, {
      hour: "2-digit",
      minute: "2-digit",
      hour12: false,
    }).format(resetDate)}`;
  }

  return `重置 ${new Intl.DateTimeFormat(locale, {
    month: "numeric",
    day: "numeric",
  }).format(resetDate)}`;
}

function formatCodexWindowLabel(minutes: number) {
  if (minutes === 300) {
    return "5 小时窗口";
  }

  if (minutes === 10080) {
    return "1 周窗口";
  }

  if (minutes >= 60 && minutes % 60 === 0) {
    return `${minutes / 60} 小时窗口`;
  }

  return `${minutes} 分钟窗口`;
}

function formatCodexMiniLabel(minutes: number) {
  if (minutes === 300) {
    return "5h";
  }

  if (minutes === 10080) {
    return "1w";
  }

  if (minutes >= 60 && minutes % 60 === 0) {
    return `${minutes / 60}h`;
  }

  return `${minutes}m`;
}

function formatCodexMiniCountdown(limit: CodexUsageLimitWindow, nowMs: number) {
  if (!limit.resets_at) {
    return formatCodexMiniLabel(limit.window_minutes);
  }

  const remainingMs = limit.resets_at * 1000 - nowMs;
  if (remainingMs <= 0) {
    return "刷新中";
  }

  const totalMinutes = Math.max(1, Math.ceil(remainingMs / 60_000));
  const dayMinutes = 24 * 60;
  if (totalMinutes >= dayMinutes) {
    const days = Math.floor(totalMinutes / dayMinutes);
    const hours = Math.floor((totalMinutes % dayMinutes) / 60);
    return hours > 0 ? `${days}d${hours}h` : `${days}d`;
  }

  if (totalMinutes >= 60) {
    const hours = Math.floor(totalMinutes / 60);
    const minutes = totalMinutes % 60;
    return minutes > 0 ? `${hours}h${minutes}m` : `${hours}h`;
  }

  return `${totalMinutes}m`;
}

function buildCodexUsageLimitWindowViewModel(
  limit: CodexUsageLimitWindow,
  tone: CodexUsageLimitWindowViewModel["tone"],
  locale: string,
  nowMs: number,
): CodexUsageLimitWindowViewModel {
  return {
    label: formatCodexWindowLabel(limit.window_minutes),
    miniLabel: formatCodexMiniCountdown(limit, nowMs),
    remainingLabel: formatCodexPercent(limit.remaining_percent, locale),
    resetLabel: formatCodexResetLabel(limit, locale),
    tone,
    usedLabel: `已用 ${formatCodexPercent(limit.used_percent, locale)}`,
    usedPercent: Math.max(0, Math.min(100, limit.used_percent)),
    remainingPercent: Math.max(0, Math.min(100, limit.remaining_percent)),
  };
}

function buildCodexUsageLimitsViewModel(
  snapshot: CodexUsageLimitSnapshot | null,
  locale: string,
  nowMs: number,
): CodexUsageLimitsViewModel | null {
  if (!snapshot) {
    return null;
  }

  const capturedDate = new Date(snapshot.captured_at);
  const capturedLabel = Number.isNaN(capturedDate.getTime())
    ? "采集时间未知"
    : `采集 ${new Intl.DateTimeFormat(locale, {
        hour: "2-digit",
        minute: "2-digit",
        hour12: false,
      }).format(capturedDate)}`;
  const planLabel =
    snapshot.limit_name ||
    (snapshot.plan_type
      ? snapshot.plan_type.slice(0, 1).toUpperCase() + snapshot.plan_type.slice(1)
      : "Codex");

  return {
    capturedLabel,
    planLabel,
    windows: [
      buildCodexUsageLimitWindowViewModel(snapshot.primary, "codex-primary", locale, nowMs),
      buildCodexUsageLimitWindowViewModel(snapshot.secondary, "codex-secondary", locale, nowMs),
    ],
  };
}

function buildComparisonLabel(snapshot: TokenPulseSnapshot, ratioLabel: string, locale: string) {
  const averageText =
    snapshot.ratio_to_average === null ? "今日暂无日均参照" : `今日已达日均 ${ratioLabel}`;

  if (snapshot.yesterday_tokens <= 0) {
    return `${averageText}，昨日暂无可比数据`;
  }

  const differenceRatio =
    (snapshot.today_tokens - snapshot.yesterday_tokens) / snapshot.yesterday_tokens;
  if (Math.abs(differenceRatio) < 0.005) {
    return `${averageText}，与昨日基本持平`;
  }

  const directionText = differenceRatio > 0 ? "高" : "低";
  return `${averageText}，较昨日${directionText} ${formatPercent(Math.abs(differenceRatio), locale)}`;
}

function buildHourlyBars(snapshot: TokenPulseSnapshot): HourlyTokenBar[] {
  const byHour = new Map(snapshot.hourly_tokens.map((point) => [point.hour, point.total_tokens]));
  const values = Array.from({ length: 24 }, (_, hour) => ({
    hour,
    tokens: byHour.get(hour) ?? 0,
  }));
  const maxValue = Math.max(...values.map((point) => point.tokens), 1);

  return values.map((point) => {
    const height = point.tokens > 0 ? Math.max(18, Math.round((point.tokens / maxValue) * 100)) : 8;
    const tier =
      point.tokens <= 0
        ? "low"
        : point.tokens >= maxValue * 0.72
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
  const [todayDeltaTokens, setTodayDeltaTokens] = useState<number | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const isMountedRef = useRef(true);
  const previousPulseSnapshotRef = useRef<PulseSnapshotCursor | null>(null);
  const manualDeltaAggregationRef = useRef<{
    base: PulseSnapshotCursor | null;
    latest: PulseSnapshotCursor | null;
  } | null>(null);
  const deltaTimerRef = useRef<number | null>(null);

  const clearDeltaTimer = useCallback(() => {
    if (deltaTimerRef.current !== null) {
      window.clearTimeout(deltaTimerRef.current);
      deltaTimerRef.current = null;
    }
  }, []);

  const showTodayDelta = useCallback(
    (deltaTokens: number) => {
      if (deltaTokens <= 0) {
        clearDeltaTimer();
        setTodayDeltaTokens(null);
        return;
      }

      clearDeltaTimer();
      setTodayDeltaTokens(deltaTokens);
      deltaTimerRef.current = window.setTimeout(() => {
        deltaTimerRef.current = null;
        if (isMountedRef.current) {
          setTodayDeltaTokens(null);
        }
      }, TOKEN_PULSE_DELTA_VISIBLE_MS);
    },
    [clearDeltaTimer],
  );

  const updateTodayDelta = useCallback(
    (nextSnapshot: TokenPulseSnapshot, options: PulseRefreshOptions = {}) => {
      const previous = previousPulseSnapshotRef.current;
      const next = {
        todayLocal: nextSnapshot.today_local,
        todayTokens: nextSnapshot.today_tokens,
      };
      previousPulseSnapshotRef.current = next;

      const deltaTokens =
        previous && previous.todayLocal === nextSnapshot.today_local
          ? nextSnapshot.today_tokens - previous.todayTokens
          : 0;

      if (manualDeltaAggregationRef.current) {
        manualDeltaAggregationRef.current.latest = next;
        return;
      }

      if (options.showDelta === false) {
        return;
      }

      showTodayDelta(deltaTokens);
    },
    [showTodayDelta],
  );

  const beginPulseDeltaAggregation = useCallback(() => {
    clearDeltaTimer();
    setTodayDeltaTokens(null);
    const base = previousPulseSnapshotRef.current;
    manualDeltaAggregationRef.current = {
      base,
      latest: base,
    };

    let finished = false;
    return function finishPulseDeltaAggregation() {
      if (finished) {
        return;
      }
      finished = true;
      const aggregation = manualDeltaAggregationRef.current;
      manualDeltaAggregationRef.current = null;
      const baseSnapshot = aggregation?.base;
      const latestSnapshot = aggregation?.latest;
      const deltaTokens =
        baseSnapshot &&
        latestSnapshot &&
        baseSnapshot.todayLocal === latestSnapshot.todayLocal
          ? latestSnapshot.todayTokens - baseSnapshot.todayTokens
          : 0;
      showTodayDelta(deltaTokens);
    };
  }, [clearDeltaTimer, showTodayDelta]);

  const refreshSnapshot = useCallback(
    async (options: PulseRefreshOptions = {}) => {
      setIsLoading(true);
      try {
        const nextSnapshot = await getTokenPulse(TOKEN_PULSE_HISTORY_DAYS);
        if (isMountedRef.current) {
          updateTodayDelta(nextSnapshot, options);
          setSnapshot(nextSnapshot);
        }
      } finally {
        if (isMountedRef.current) {
          setIsLoading(false);
        }
      }
    },
    [updateTodayDelta],
  );

  useEffect(() => {
    isMountedRef.current = true;
    void refreshSnapshot();
    const refreshTimer = window.setInterval(() => {
      void refreshSnapshot();
    }, TOKEN_PULSE_REFRESH_MS);

    return () => {
      isMountedRef.current = false;
      clearDeltaTimer();
      window.clearInterval(refreshTimer);
    };
  }, [clearDeltaTimer, refreshSnapshot]);

  return { snapshot, todayDeltaTokens, isLoading, beginPulseDeltaAggregation, refreshSnapshot };
}

function useCodexUsageLimitSnapshot(enabled: boolean) {
  const [snapshot, setSnapshot] = useState<CodexUsageLimitSnapshot | null>(null);
  const isMountedRef = useRef(true);

  const refreshSnapshot = useCallback(async () => {
    if (!enabled) {
      setSnapshot(null);
      return;
    }

    try {
      const nextSnapshot = await getCodexUsageLimits();
      if (isMountedRef.current) {
        setSnapshot(nextSnapshot);
      }
    } catch {
      if (isMountedRef.current) {
        setSnapshot(null);
      }
    }
  }, [enabled]);

  useEffect(() => {
    isMountedRef.current = true;
    if (!enabled) {
      setSnapshot(null);
      return () => {
        isMountedRef.current = false;
      };
    }

    void refreshSnapshot();
    const refreshTimer = window.setInterval(() => {
      void refreshSnapshot();
    }, TOKEN_PULSE_REFRESH_MS);

    return () => {
      isMountedRef.current = false;
      window.clearInterval(refreshTimer);
    };
  }, [enabled, refreshSnapshot]);

  return { snapshot, refreshSnapshot };
}

function useNowMs(enabled: boolean) {
  const [nowMs, setNowMs] = useState(() => Date.now());

  useEffect(() => {
    if (!enabled) {
      return;
    }

    setNowMs(Date.now());
    const timer = window.setInterval(() => {
      setNowMs(Date.now());
    }, TOKEN_PULSE_REFRESH_MS);

    return () => window.clearInterval(timer);
  }, [enabled]);

  return nowMs;
}

function useTokenPulseViewModel(): TokenPulseViewModel {
  const { numberLocale } = useI18n();
  const { numberDisplayMode, showCodexUsageLimits } = useDisplayPreference();
  const nowMs = useNowMs(showCodexUsageLimits);
  const {
    snapshot,
    todayDeltaTokens,
    isLoading,
    beginPulseDeltaAggregation,
    refreshSnapshot: refreshPulseSnapshot,
  } = usePulseSnapshot();
  const {
    snapshot: codexUsageLimitSnapshot,
    refreshSnapshot: refreshCodexUsageLimits,
  } = useCodexUsageLimitSnapshot(showCodexUsageLimits);
  const hourlyBars = useMemo(() => buildHourlyBars(snapshot), [snapshot]);
  const progressPercent = getProgressPercent(snapshot);
  const ratioLabel = formatRatio(snapshot, numberLocale);
  const comparisonLabel = buildComparisonLabel(snapshot, ratioLabel, numberLocale);
  const todayLabel = formatTokenByDisplayMode(snapshot.today_tokens, numberLocale, numberDisplayMode);
  const todayTitle = `${formatInteger(snapshot.today_tokens, numberLocale)} Token`;
  const todayDeltaLabel =
    todayDeltaTokens !== null
      ? `+${formatTokenByDisplayMode(todayDeltaTokens, numberLocale, numberDisplayMode)}`
      : null;
  const todayDeltaTitle =
    todayDeltaTokens !== null
      ? `本次刷新 +${formatInteger(todayDeltaTokens, numberLocale)} Token`
      : null;
  const averageLabel = formatCompactToken(snapshot.average_daily_tokens, numberLocale);
  const yesterdayLabel = formatCompactToken(snapshot.yesterday_tokens, numberLocale);
  const remainingLabel = formatCompactToken(snapshot.remaining_to_average, numberLocale);
  const todayDateLabel = snapshot.today_local.slice(5);
  const codexUsageLimits = useMemo(
    () => buildCodexUsageLimitsViewModel(codexUsageLimitSnapshot, numberLocale, nowMs),
    [codexUsageLimitSnapshot, nowMs, numberLocale],
  );
  const refreshSnapshot = useCallback(async () => {
    if (showCodexUsageLimits) {
      await Promise.all([refreshPulseSnapshot(), refreshCodexUsageLimits()]);
      return;
    }

    await refreshPulseSnapshot();
  }, [refreshCodexUsageLimits, refreshPulseSnapshot, showCodexUsageLimits]);
  const trendDirection =
    snapshot.average_daily_tokens > 0
      ? snapshot.today_tokens >= snapshot.average_daily_tokens
        ? "up"
        : "down"
      : snapshot.today_tokens >= snapshot.yesterday_tokens
        ? "up"
        : "down";

  return {
    averageLabel,
    beginPulseDeltaAggregation,
    codexUsageLimits,
    comparisonLabel,
    hourlyBars,
    isLoading,
    progressPercent,
    ratioLabel,
    remainingLabel,
    refreshCodexUsageLimits,
    refreshPulseSnapshot,
    refreshSnapshot,
    snapshot,
    showCodexUsageLimits,
    todayDateLabel,
    todayDeltaLabel,
    todayDeltaTitle,
    todayLabel,
    todayTitle,
    trendDirection,
    yesterdayLabel,
  };
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
        const nextX = Math.max(0, position.x / scaleFactor + currentLogicalWidth - width);
        await currentWindow.setSize(new LogicalSize(width, height));
        await currentWindow.setPosition(new LogicalPosition(nextX, position.y / scaleFactor));
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

function useTokenPulseDrag(isPositionLocked: boolean, refreshPositionLocked: () => Promise<boolean>) {
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

function TokenPulseDatabaseIcon() {
  return (
    <svg
      aria-hidden="true"
      className="token-pulse-database-icon"
      fill="none"
      viewBox="0 0 24 24"
    >
      <ellipse cx="12" cy="6" rx="8" ry="3.5" />
      <path d="M4 6v6c0 1.93 3.58 3.5 8 3.5s8-1.57 8-3.5V6" />
      <path d="M4 12v6c0 1.93 3.58 3.5 8 3.5s8-1.57 8-3.5v-6" />
      <path d="M4 12c0 1.93 3.58 3.5 8 3.5s8-1.57 8-3.5" />
    </svg>
  );
}

function PulseDetailIcon() {
  return (
    <svg aria-hidden="true" className="pulse-detail-icon" fill="none" viewBox="0 0 22 22">
      <rect height="7" rx="1.5" width="4" x="3" y="12" />
      <rect height="12" rx="1.5" width="4" x="9" y="7" />
      <rect height="16" rx="1.5" width="4" x="15" y="3" />
    </svg>
  );
}

function TokenPulseActionIcon({ kind }: { kind: TokenPulseActionIconKind }) {
  return (
    <svg
      aria-hidden="true"
      className="token-pulse-action-icon"
      fill="none"
      viewBox="0 0 24 24"
    >
      {kind === "home" ? (
        <>
          <path d="M4 11.5 12 5l8 6.5" />
          <path d="M6.5 10.5V19h4.25v-5h2.5v5h4.25v-8.5" />
        </>
      ) : null}
      {kind === "lock" ? (
        <>
          <rect height="9" rx="2" width="14" x="5" y="11" />
          <path d="M8 11V8a4 4 0 0 1 8 0v3" />
        </>
      ) : null}
      {kind === "unlock" ? (
        <>
          <rect height="9" rx="2" width="14" x="5" y="11" />
          <path d="M8 11V8a4 4 0 0 1 7.4-2.1" />
        </>
      ) : null}
      {kind === "refresh" ? (
        <>
          <path d="M19 7v5h-5" />
          <path d="M5 17v-5h5" />
          <path d="M18.2 11.2A6.5 6.5 0 0 0 6.5 8" />
          <path d="M5.8 12.8A6.5 6.5 0 0 0 17.5 16" />
        </>
      ) : null}
      {kind === "hide" ? (
        <>
          <path d="m7 7 10 10" />
          <path d="m17 7-10 10" />
        </>
      ) : null}
    </svg>
  );
}

function TokenPulseTrendIcon({ direction }: { direction: "up" | "down" }) {
  return (
    <svg
      aria-hidden="true"
      className={`token-pulse-trend-icon ${direction}`}
      fill="none"
      viewBox="0 0 20 20"
    >
      {direction === "up" ? (
        <>
          <path d="M4 12.5 10 6l6 6.5" />
          <path d="M10 6v10" />
        </>
      ) : (
        <>
          <path d="M4 7.5 10 14l6-6.5" />
          <path d="M10 4v10" />
        </>
      )}
    </svg>
  );
}

function TokenPulseRing({
  label,
  percent,
  tone,
}: {
  label: string;
  percent: number;
  tone: TokenPulseRingTone;
}) {
  const allowOverflow = tone === "today";
  const boundedPercent = allowOverflow
    ? Math.max(0, percent)
    : Math.max(0, Math.min(100, percent));
  const ringProgressPercent = getRingProgressPercent(percent, allowOverflow);
  const percentLabel = `${Math.round(boundedPercent)}%`;
  const valueClassName = `token-pulse-ring-value${percentLabel.length >= 4 ? " is-compact" : ""}`;
  const dashOffset = 100 - ringProgressPercent;
  const todayPalette = allowOverflow ? buildTodayRingPalette(boundedPercent) : null;
  const style = todayPalette
    ? ({
        "--ring-color": todayPalette.color,
        "--ring-glow": todayPalette.glow,
        "--ring-track": todayPalette.track,
      } as CSSProperties)
    : undefined;

  return (
    <span aria-label={label} className={`token-pulse-ring ${tone}`} role="img" style={style}>
      <svg
        aria-hidden="true"
        className="token-pulse-ring-svg"
        focusable="false"
        viewBox="0 0 48 48"
      >
        <circle className="token-pulse-ring-track" cx="24" cy="24" pathLength={100} r="20" />
        <circle
          className="token-pulse-ring-progress"
          cx="24"
          cy="24"
          pathLength={100}
          r="20"
          strokeDasharray={100}
          strokeDashoffset={dashOffset}
          strokeLinecap="round"
        />
      </svg>
      <span className={valueClassName}>{percentLabel}</span>
    </span>
  );
}

function CodexUsageLimitRing({ window }: { window: CodexUsageLimitWindowViewModel }) {
  return (
    <div className="token-pulse-codex-ring">
      <TokenPulseRing
        label={`${window.label}剩余 ${window.remainingLabel}`}
        percent={window.remainingPercent}
        tone={window.tone}
      />
      <span>{window.miniLabel}</span>
    </div>
  );
}

function CodexUsageLimitsPanel({
  viewModel,
}: {
  viewModel: CodexUsageLimitsViewModel | null;
}) {
  return (
    <aside className="token-pulse-codex-detail">
      <div className="token-pulse-codex-detail-head">
        <span>Codex 剩余用量</span>
        <small>{viewModel?.planLabel ?? "等待快照"}</small>
      </div>
      {viewModel ? (
        <>
          <div className="token-pulse-codex-detail-list">
            {viewModel.windows.map((window) => (
              <div className="token-pulse-codex-detail-row" key={window.label}>
                <TokenPulseRing
                  label={`${window.label}剩余 ${window.remainingLabel}`}
                  percent={window.remainingPercent}
                  tone={window.tone}
                />
                <div>
                  <strong>{window.label}</strong>
                  <span>{window.resetLabel}</span>
                  <span>{window.usedLabel}</span>
                </div>
              </div>
            ))}
          </div>
          <p className="token-pulse-codex-foot">{viewModel.capturedLabel}</p>
        </>
      ) : (
        <div className="token-pulse-codex-empty">等待 Codex 用量快照</div>
      )}
    </aside>
  );
}

function TokenPulseHourlyChart({ viewModel }: { viewModel: TokenPulseViewModel }) {
  const chartWidth = 304;
  const chartHeight = 86;
  const plotLeft = 8;
  const plotRight = 296;
  const plotBottom = 58;
  const plotHeight = 48;
  const hourWidth = (plotRight - plotLeft) / 24;
  const barWidth = 7;
  const maxTokens = Math.max(...viewModel.hourlyBars.map((point) => point.tokens), 1);

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
      <line className="token-pulse-chart-baseline" x1={plotLeft} x2={plotRight} y1={plotBottom} y2={plotBottom} />
      {viewModel.hourlyBars.map((point) => {
        if (point.tokens <= 0) {
          return null;
        }

        const barHeight = Math.max(5, (point.tokens / maxTokens) * plotHeight);
        const x = plotLeft + point.hour * hourWidth + (hourWidth - barWidth) / 2;
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
            <strong aria-label={viewModel.todayTitle}>{viewModel.todayLabel}</strong>
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
            <span className="token-pulse-codex-empty">等待快照</span>
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
            <span>{viewModel.isLoading ? "正在刷新用量快照" : viewModel.comparisonLabel}</span>
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
    viewModel.showCodexUsageLimits ? TOKEN_PULSE_MINI_CODEX_WIDTH : TOKEN_PULSE_MINI_WIDTH,
    TOKEN_PULSE_MINI_HEIGHT,
  );
  const { isPositionLocked, refreshPositionLocked, updatePositionLocked } =
    useTokenPulsePositionLock();
  const { dragHandlers, isDragging } = useTokenPulseDrag(isPositionLocked, refreshPositionLocked);
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
      ? viewModel.refreshCodexUsageLimits().finally(() => {
          logTokenPulsePerf("manual_refresh.codex_usage", refreshStartedAt);
        })
      : Promise.resolve();
    try {
      try {
        const syncStartedAt = performance.now();
        try {
          await runBackgroundSyncOnce();
        } finally {
          logTokenPulsePerf("manual_refresh.background_sync", syncStartedAt);
        }
      } finally {
        const pulseStartedAt = performance.now();
        try {
          await Promise.all([
            viewModel.refreshPulseSnapshot({ showDelta: false }).finally(() => {
              logTokenPulsePerf("manual_refresh.pulse_snapshot", pulseStartedAt);
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
    viewModel.showCodexUsageLimits ? TOKEN_PULSE_DETAIL_CODEX_WIDTH : TOKEN_PULSE_DETAIL_WIDTH,
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
