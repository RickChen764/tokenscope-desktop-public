import { useEffect, useState } from "react";
import { useI18n } from "../i18n";
import {
  forceGitHubSyncBootstrapUpload,
  getGitHubSyncSettings,
  runGitHubSyncOnce,
  saveGitHubSyncSettings,
  testGitHubSyncConnection,
} from "../services/dashboard";
import type { GitHubSyncSettings, GitHubSyncSettingsInput } from "../types/dashboard";
import { formatDateTime } from "../utils/format";
import type { ToastNoticeValue } from "./ToastNotice";

const defaultDraft: GitHubSyncSettingsInput = {
  enabled: false,
  owner: "",
  repo: "",
  branch: "main",
  path_prefix: "tokenscope-sync",
  token: "",
  sync_password: "",
};

interface GitHubSyncPanelProps {
  onNotice: (notice: ToastNoticeValue) => void;
}

function draftFromSettings(settings: GitHubSyncSettings): GitHubSyncSettingsInput {
  return {
    enabled: settings.enabled,
    owner: settings.owner,
    repo: settings.repo,
    branch: settings.branch,
    path_prefix: settings.path_prefix,
    token: "",
    sync_password: "",
  };
}

export function GitHubSyncPanel({ onNotice }: GitHubSyncPanelProps) {
  const { t } = useI18n();
  const [settings, setSettings] = useState<GitHubSyncSettings | null>(null);
  const [draft, setDraft] = useState<GitHubSyncSettingsInput>(defaultDraft);
  const [isBusy, setIsBusy] = useState(false);

  useEffect(() => {
    let alive = true;
    void getGitHubSyncSettings().then((nextSettings) => {
      if (!alive) {
        return;
      }
      setSettings(nextSettings);
      setDraft(draftFromSettings(nextSettings));
    });
    return () => {
      alive = false;
    };
  }, []);

  function updateDraft<K extends keyof GitHubSyncSettingsInput>(
    key: K,
    value: GitHubSyncSettingsInput[K],
  ) {
    setDraft((current) => ({ ...current, [key]: value }));
  }

  async function handleSave() {
    setIsBusy(true);
    try {
      const nextSettings = await saveGitHubSyncSettings(draft);
      setSettings(nextSettings);
      setDraft(draftFromSettings(nextSettings));
      onNotice({ kind: "success", message: t("GitHub 同步设置已保存。") });
    } catch (err) {
      onNotice({
        kind: "error",
        message: t("保存 GitHub 同步设置失败：{error}", {
          error: err instanceof Error ? err.message : String(err),
        }),
      });
    } finally {
      setIsBusy(false);
    }
  }

  async function handleTestConnection() {
    setIsBusy(true);
    try {
      const result = await testGitHubSyncConnection();
      onNotice({
        kind: result.status === "error" ? "error" : "success",
        message: result.message,
      });
    } finally {
      setIsBusy(false);
    }
  }

  async function handleRunSync() {
    setIsBusy(true);
    try {
      const result = await runGitHubSyncOnce();
      onNotice({ kind: result.status === "error" ? "error" : "success", message: result.message });
    } finally {
      setIsBusy(false);
    }
  }

  async function handleForceBootstrap() {
    setIsBusy(true);
    try {
      const result = await forceGitHubSyncBootstrapUpload();
      onNotice({ kind: result.status === "error" ? "error" : "success", message: result.message });
    } finally {
      setIsBusy(false);
    }
  }

  async function handleInitializeRepository() {
    setIsBusy(true);
    try {
      const initializationDraft = { ...draft, enabled: true };
      const savedSettings = await saveGitHubSyncSettings(initializationDraft);
      setSettings(savedSettings);
      setDraft(draftFromSettings(savedSettings));

      const connectionResult = await testGitHubSyncConnection();
      if (connectionResult.status === "error") {
        throw new Error(connectionResult.message);
      }

      const syncResult = await forceGitHubSyncBootstrapUpload();
      const refreshedSettings = await getGitHubSyncSettings();
      setSettings(refreshedSettings);
      setDraft(draftFromSettings(refreshedSettings));
      onNotice({
        kind: syncResult.status === "error" ? "error" : "success",
        message: syncResult.message,
      });
    } catch (err) {
      onNotice({
        kind: "error",
        message: t("初始化同步仓库失败：{error}", {
          error: err instanceof Error ? err.message : String(err),
        }),
      });
    } finally {
      setIsBusy(false);
    }
  }

  const lastUploadLabel = settings?.last_upload_at
    ? formatDateTime(settings.last_upload_at)
    : t("尚未同步");
  const lastImportLabel = settings?.last_import_at
    ? formatDateTime(settings.last_import_at)
    : t("尚未同步");

  return (
    <section className="panel github-sync-panel">
      <div className="panel-heading settings-heading">
        <div>
          <h2>{t("GitHub 同步")}</h2>
          <p>
            {t(
              "使用私有 GitHub 仓库同步压缩加密后的多设备统计数据；GitHub 只保存加密文件。",
            )}
          </p>
        </div>
        <label className="switch-field compact-switch">
          <span>{t("启用")}</span>
          <input
            checked={draft.enabled}
            disabled={isBusy}
            onChange={(event) => updateDraft("enabled", event.target.checked)}
            role="switch"
            type="checkbox"
          />
        </label>
      </div>

      <div className="github-sync-wizard" aria-labelledby="github-sync-wizard-title">
        <div>
          <h3 id="github-sync-wizard-title">{t("同步仓库向导")}</h3>
          <p>
            {t(
              "此版本不会自动创建仓库。请先在 GitHub 手动创建私有仓库，再使用只包含 Contents: Read and write 权限的 token 初始化同步空间。",
            )}
          </p>
        </div>
        <ol>
          <li>{t("创建或选择一个私有 GitHub 仓库。")}</li>
          <li>{t("为该仓库创建 fine-grained personal access token，权限设置为 Contents: Read and write。")}</li>
          <li>{t("填写下方仓库信息、token 和同步密码。")}</li>
          <li>{t("点击保存并初始化同步仓库，应用会保存配置并上传当前设备的 bootstrap。")}</li>
        </ol>
        <button
          className="primary"
          disabled={isBusy}
          onClick={() => void handleInitializeRepository()}
          type="button"
        >
          {t("保存并初始化同步仓库")}
        </button>
      </div>

      <div className="settings-form github-sync-form">
        <label className="field">
          <span>owner</span>
          <input
            disabled={isBusy}
            onChange={(event) => updateDraft("owner", event.target.value)}
            value={draft.owner}
          />
        </label>
        <label className="field">
          <span>repo</span>
          <input
            disabled={isBusy}
            onChange={(event) => updateDraft("repo", event.target.value)}
            value={draft.repo}
          />
        </label>
        <label className="field">
          <span>branch</span>
          <input
            disabled={isBusy}
            onChange={(event) => updateDraft("branch", event.target.value)}
            value={draft.branch}
          />
        </label>
        <label className="field">
          <span>path prefix</span>
          <input
            disabled={isBusy}
            onChange={(event) => updateDraft("path_prefix", event.target.value)}
            value={draft.path_prefix}
          />
        </label>
        <label className="field">
          <span>personal access token</span>
          <input
            disabled={isBusy}
            onChange={(event) => updateDraft("token", event.target.value)}
            placeholder={settings?.token_redacted ?? t("未配置")}
            type="password"
            value={draft.token ?? ""}
          />
        </label>
        <label className="field">
          <span>{t("同步密码")}</span>
          <input
            disabled={isBusy}
            onChange={(event) => updateDraft("sync_password", event.target.value)}
            placeholder={settings?.sync_password_configured ? t("已配置") : t("未配置")}
            type="password"
            value={draft.sync_password ?? ""}
          />
        </label>
      </div>

      <div className="detail-stat-list github-sync-status">
        <div>
          <span>bootstrap</span>
          <strong>{settings?.bootstrap_uploaded ? t("已上传") : t("未上传")}</strong>
        </div>
        <div>
          <span>{t("最近上传")}</span>
          <strong>{lastUploadLabel}</strong>
        </div>
        <div>
          <span>{t("最近下载")}</span>
          <strong>{lastImportLabel}</strong>
        </div>
        <div>
          <span>{t("最近错误")}</span>
          <strong className={settings?.last_error ? "danger-text" : ""}>
            {settings?.last_error ?? t("无")}
          </strong>
        </div>
      </div>

      <p className="settings-footnote">
        {t("GitHub 只保存加密文件，但仓库路径、文件大小、commit 时间和日期文件名仍对 GitHub 可见。")}
      </p>

      <div className="form-actions">
        <button className="primary secondary" disabled={isBusy} onClick={() => void handleTestConnection()} type="button">
          {t("测试连接")}
        </button>
        <button className="primary secondary" disabled={isBusy} onClick={() => void handleForceBootstrap()} type="button">
          {t("强制重新上传 bootstrap")}
        </button>
        <button className="primary secondary" disabled={isBusy} onClick={() => void handleRunSync()} type="button">
          {t("立即同步")}
        </button>
        <button className="primary" disabled={isBusy} onClick={() => void handleSave()} type="button">
          {t("保存 GitHub 同步设置")}
        </button>
      </div>
    </section>
  );
}
