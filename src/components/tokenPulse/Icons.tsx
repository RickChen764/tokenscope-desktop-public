export type TokenPulseActionIconKind =
  | "home"
  | "lock"
  | "unlock"
  | "refresh"
  | "hide";

export function PulseDetailIcon() {
  return (
    <svg
      aria-hidden="true"
      className="pulse-detail-icon"
      fill="none"
      viewBox="0 0 22 22"
    >
      <rect height="7" rx="1.5" width="4" x="3" y="12" />
      <rect height="12" rx="1.5" width="4" x="9" y="7" />
      <rect height="16" rx="1.5" width="4" x="15" y="3" />
    </svg>
  );
}

export function TokenPulseActionIcon({
  kind,
}: {
  kind: TokenPulseActionIconKind;
}) {
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

export function TokenPulseTrendIcon({
  direction,
}: {
  direction: "up" | "down";
}) {
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
