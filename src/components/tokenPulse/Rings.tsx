import {
  getRingProgressPercent,
  getTodayRingStageClass,
  type CodexUsageLimitWindowViewModel,
  type TokenPulseRingTone,
} from "./viewModel";

export function TokenPulseRing({
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
  const stageClassName = allowOverflow ? ` ${getTodayRingStageClass(boundedPercent)}` : "";

  return (
    <span aria-label={label} className={`token-pulse-ring ${tone}${stageClassName}`} role="img">
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

export function CodexUsageLimitRing({ window }: { window: CodexUsageLimitWindowViewModel }) {
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
