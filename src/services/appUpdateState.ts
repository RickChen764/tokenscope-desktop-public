import type { AppUpdateInfo, AppUpdateStatus } from "../types/dashboard";

const TRANSIENT_APP_UPDATE_STATUSES = new Set<AppUpdateStatus>([
  "checking",
  "downloading",
  "installing",
]);

export function defaultAppUpdateInfo(status: AppUpdateStatus = "idle"): AppUpdateInfo {
  return {
    available: false,
    current_version: null,
    version: null,
    date: null,
    body: null,
    status,
    checked_at: null,
    error: null,
  };
}

function normalizeAppUpdateStatus(value: unknown): AppUpdateStatus {
  switch (value) {
    case "checking":
    case "current":
    case "available":
    case "downloading":
    case "installing":
    case "error":
    case "browser-preview":
      return value;
    case "idle":
    default:
      return "idle";
  }
}

export function normalizeAppUpdateInfo(input: Partial<AppUpdateInfo>): AppUpdateInfo {
  const defaults = defaultAppUpdateInfo();
  return {
    ...defaults,
    ...input,
    available: Boolean(input.available),
    current_version: input.current_version ?? null,
    version: input.version ?? null,
    date: input.date ?? null,
    body: input.body ?? null,
    status: normalizeAppUpdateStatus(input.status),
    checked_at: input.checked_at ?? null,
    error: input.error ?? null,
  };
}

export function recoverStoredAppUpdateInfo(info: AppUpdateInfo): AppUpdateInfo {
  if (!TRANSIENT_APP_UPDATE_STATUSES.has(info.status)) {
    return info;
  }

  return defaultAppUpdateInfo();
}
