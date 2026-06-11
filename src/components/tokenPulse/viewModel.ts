import type {
  CodexUsageLimitSnapshot,
  CodexUsageLimitWindow,
  TokenPulseSnapshot,
} from "../../types/dashboard";

export const TOKEN_PULSE_HISTORY_DAYS = 30;

export type HourlyTokenBar = {
  hour: number;
  tokens: number;
  height: number;
  tier: "high" | "mid" | "low";
};

export type TokenPulseRingTone = "today" | "codex-primary" | "codex-secondary";

export type CodexUsageLimitWindowViewModel = {
  label: string;
  miniLabel: string;
  remainingLabel: string;
  resetLabel: string;
  tone: Extract<TokenPulseRingTone, "codex-primary" | "codex-secondary">;
  usedLabel: string;
  usedPercent: number;
  remainingPercent: number;
};

export type CodexUsageLimitsViewModel = {
  capturedLabel: string;
  planLabel: string;
  windows: CodexUsageLimitWindowViewModel[];
};

export type PulseSnapshotCursor = {
  todayLocal: string;
  todayTokens: number;
};

export type PulseRefreshOptions = {
  showDelta?: boolean;
};

export type CodexUsageLimitRefreshOptions = {
  forceRefresh?: boolean;
};

export type TokenPulseViewModel = {
  averageLabel: string;
  beginPulseDeltaAggregation: () => () => void;
  codexUsageLimits: CodexUsageLimitsViewModel | null;
  hourlyBars: HourlyTokenBar[];
  isLoading: boolean;
  progressPercent: number;
  comparisonLabel: string;
  ratioLabel: string;
  remainingLabel: string;
  refreshCodexUsageLimits: (options?: CodexUsageLimitRefreshOptions) => Promise<void>;
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

export function emptyPulseSnapshot(): TokenPulseSnapshot {
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

export function formatRatio(snapshot: TokenPulseSnapshot, locale: string) {
  if (snapshot.ratio_to_average === null) {
    return "0%";
  }

  return new Intl.NumberFormat(locale, {
    style: "percent",
    maximumFractionDigits: 0,
  }).format(snapshot.ratio_to_average);
}

export function getProgressPercent(snapshot: TokenPulseSnapshot) {
  if (snapshot.ratio_to_average === null) {
    return 0;
  }

  return Math.max(0, snapshot.ratio_to_average * 100);
}

export function getRingProgressPercent(percent: number, allowOverflow: boolean) {
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

function getTodayRingStageIndex(percent: number) {
  const boundedPercent = Math.max(0, percent);
  return Math.min(
    Math.max(0, Math.ceil(boundedPercent / 100) - 1),
    TOKEN_PULSE_TODAY_RING_STAGES.length - 1,
  );
}

export function buildTodayRingPalette(percent: number) {
  const stageIndex = getTodayRingStageIndex(percent);

  return TOKEN_PULSE_TODAY_RING_STAGES[stageIndex];
}

export function getTodayRingStageClass(percent: number) {
  return `today-stage-${getTodayRingStageIndex(percent)}`;
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

export function getDisplayedCodexUsageLimitWindow(
  limit: CodexUsageLimitWindow,
  nowMs: number,
): CodexUsageLimitWindow {
  if (!limit.resets_at || limit.resets_at * 1000 > nowMs) {
    return limit;
  }

  return { ...limit, used_percent: 0, remaining_percent: 100 };
}

function buildCodexUsageLimitWindowViewModel(
  limit: CodexUsageLimitWindow,
  tone: CodexUsageLimitWindowViewModel["tone"],
  locale: string,
  nowMs: number,
): CodexUsageLimitWindowViewModel {
  const displayedLimit = getDisplayedCodexUsageLimitWindow(limit, nowMs);

  return {
    label: formatCodexWindowLabel(displayedLimit.window_minutes),
    miniLabel: formatCodexMiniCountdown(displayedLimit, nowMs),
    remainingLabel: formatCodexPercent(displayedLimit.remaining_percent, locale),
    resetLabel: formatCodexResetLabel(displayedLimit, locale),
    tone,
    usedLabel: `已用 ${formatCodexPercent(displayedLimit.used_percent, locale)}`,
    usedPercent: Math.max(0, Math.min(100, displayedLimit.used_percent)),
    remainingPercent: Math.max(0, Math.min(100, displayedLimit.remaining_percent)),
  };
}

export function buildCodexUsageLimitsViewModel(
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

export function buildComparisonLabel(
  snapshot: TokenPulseSnapshot,
  ratioLabel: string,
  locale: string,
) {
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

export function buildHourlyBars(snapshot: TokenPulseSnapshot): HourlyTokenBar[] {
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
