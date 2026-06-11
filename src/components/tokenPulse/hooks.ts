import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useI18n } from "../../i18n";
import { useDisplayPreference } from "../../preferences/display";
import { getCodexUsageLimits, getTokenPulse } from "../../services/dashboard";
import type { CodexUsageLimitSnapshot, TokenPulseSnapshot } from "../../types/dashboard";
import { formatCompactToken, formatInteger, formatTokenByDisplayMode } from "../../utils/format";
import {
  TOKEN_PULSE_HISTORY_DAYS,
  buildCodexUsageLimitsViewModel,
  buildComparisonLabel,
  buildHourlyBars,
  emptyPulseSnapshot,
  formatRatio,
  getProgressPercent,
  type CodexUsageLimitRefreshOptions,
  type PulseRefreshOptions,
  type PulseSnapshotCursor,
  type TokenPulseViewModel,
} from "./viewModel";

const TOKEN_PULSE_REFRESH_MS = 60000;
const TOKEN_PULSE_DELTA_VISIBLE_MS = 10000;

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

  const refreshSnapshot = useCallback(
    async (options: CodexUsageLimitRefreshOptions = {}) => {
      if (!enabled) {
        setSnapshot(null);
        return;
      }

      try {
        const nextSnapshot = await getCodexUsageLimits({ forceRefresh: options.forceRefresh });
        if (isMountedRef.current) {
          setSnapshot(nextSnapshot);
        }
      } catch {
        // Keep the last usable snapshot visible when the desktop command itself fails.
      }
    },
    [enabled],
  );

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

export function useTokenPulseViewModel(): TokenPulseViewModel {
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
