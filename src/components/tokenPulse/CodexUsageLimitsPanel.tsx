import type { CodexUsageLimitsViewModel } from "./viewModel";
import { TokenPulseRing } from "./Rings";

export function CodexUsageLimitsPanel({
  viewModel,
}: {
  viewModel: CodexUsageLimitsViewModel | null;
}) {
  return (
    <aside className="token-pulse-codex-detail">
      <div className="token-pulse-codex-detail-head">
        <span>Codex 剩余用量</span>
        <small>{viewModel?.planLabel ?? "等待订阅余量"}</small>
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
        <div className="token-pulse-codex-empty">等待订阅用量快照</div>
      )}
    </aside>
  );
}
