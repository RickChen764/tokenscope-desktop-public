import type { AppUpdateInfo, AppUpdateStatus } from "../types/dashboard";

export interface AppUpdateMetadata {
  currentVersion?: string | null;
  version?: string | null;
  date?: string | null;
  body?: string | null;
}

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

function normalizeVersionText(value: string | null | undefined) {
  const version = value?.trim();
  return version ? version : null;
}

export function createAppUpdateInfo(
  update: AppUpdateMetadata | null,
  currentVersion: string | null,
  checkedAt = new Date().toISOString(),
): AppUpdateInfo {
  return {
    available: update !== null,
    current_version:
      normalizeVersionText(update?.currentVersion) ?? normalizeVersionText(currentVersion),
    version: normalizeVersionText(update?.version),
    date: update?.date ?? null,
    body: update?.body ?? null,
    status: update ? "available" : "current",
    checked_at: checkedAt,
    error: null,
  };
}

export function appUpdateVersionRange(
  currentVersion: string | null,
  availableVersion: string | null,
) {
  const current = normalizeVersionText(currentVersion);
  const available = normalizeVersionText(availableVersion);
  if (!available) {
    return null;
  }
  if (current && current !== available) {
    return `${current} → ${available}`;
  }
  return available;
}

export function recoverStoredAppUpdateInfo(info: AppUpdateInfo): AppUpdateInfo {
  if (!TRANSIENT_APP_UPDATE_STATUSES.has(info.status)) {
    return info;
  }

  return defaultAppUpdateInfo();
}
