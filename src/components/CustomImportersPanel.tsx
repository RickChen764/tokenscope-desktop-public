import { useCallback, useEffect, useMemo, useState } from "react";
import {
  deleteCustomImporterProfile,
  listCustomImporterProfiles,
  previewCustomImporter,
  runCustomImporter,
  upsertCustomImporterProfile,
} from "../services/dashboard";
import type {
  CustomImporterPreview,
  CustomImporterProfile,
  CustomImporterProfileInput,
} from "../types/dashboard";
import { useI18n } from "../i18n";
import { formatInteger } from "../utils/format";

const defaultMappings = JSON.stringify(
  {
    external_id: "id",
    started_at: "started_at",
    ended_at: "ended_at",
    provider: "provider",
    model: "model",
    agent_id: "agent_id",
    session_id: "session_id",
    project_id: "project_id",
    input_tokens: "input_tokens",
    output_tokens: "output_tokens",
    cached_input_tokens: "cached_input_tokens",
    cache_write_input_tokens: "cache_write_input_tokens",
    reasoning_output_tokens: "reasoning_output_tokens",
    total_tokens: "total_tokens",
  },
  null,
  2,
);

const defaultDraft: CustomImporterProfileInput = {
  id: null,
  name: "",
  enabled: true,
  source_key: "custom:agent-name",
  database_path: "",
  import_sql:
    "SELECT id, started_at, provider, model, input_tokens, output_tokens, total_tokens FROM usage_records",
  mappings_json: defaultMappings,
};

interface CustomImportersPanelProps {
  onNotice: (notice: { kind: "error" | "success"; message: string }) => void;
}

function draftFromProfile(profile: CustomImporterProfile): CustomImporterProfileInput {
  return {
    id: profile.id,
    name: profile.name,
    enabled: profile.enabled,
    source_key: profile.source_key,
    database_path: profile.database_path,
    import_sql: profile.import_sql,
    mappings_json: profile.mappings_json,
  };
}

function statusLabel(profile: CustomImporterProfile, t: (message: string) => string) {
  if (!profile.enabled) {
    return t("已停用");
  }
  if (profile.last_run_status === "error") {
    return t("最近失败");
  }
  if (profile.imported_calls > 0) {
    return t("已同步");
  }
  return t("可同步");
}

function previewValue(value: unknown, emptyLabel: string) {
  if (value === null || value === undefined) {
    return emptyLabel;
  }
  if (typeof value === "object") {
    return JSON.stringify(value);
  }

  return String(value);
}

export function CustomImportersPanel({ onNotice }: CustomImportersPanelProps) {
  const { numberLocale, t } = useI18n();
  const [profiles, setProfiles] = useState<CustomImporterProfile[]>([]);
  const [draft, setDraft] = useState<CustomImporterProfileInput>(defaultDraft);
  const [preview, setPreview] = useState<CustomImporterPreview | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isSaving, setIsSaving] = useState(false);
  const [isPreviewing, setIsPreviewing] = useState(false);
  const [runningProfileId, setRunningProfileId] = useState<string | null>(null);

  const previewColumns = useMemo(() => preview?.columns.slice(0, 8) ?? [], [preview]);

  const loadProfiles = useCallback(async () => {
    setIsLoading(true);
    try {
      setProfiles(await listCustomImporterProfiles());
    } catch (err) {
      onNotice({
        kind: "error",
        message: t("读取自定义数据源失败：{error}", {
          error: err instanceof Error ? err.message : String(err),
        }),
      });
    } finally {
      setIsLoading(false);
    }
  }, [onNotice]);

  useEffect(() => {
    void loadProfiles();
  }, [loadProfiles]);

  function updateDraft<K extends keyof CustomImporterProfileInput>(
    key: K,
    value: CustomImporterProfileInput[K],
  ) {
    setDraft((current) => ({ ...current, [key]: value }));
  }

  async function handlePreview() {
    setIsPreviewing(true);
    setPreview(null);
    try {
      const nextPreview = await previewCustomImporter(draft);
      setPreview(nextPreview);
      onNotice({
        kind: "success",
        message: t("预览成功：读取 {count} 行样例。", {
          count: nextPreview.rows.length,
        }),
      });
    } catch (err) {
      onNotice({
        kind: "error",
        message: t("预览失败：{error}", {
          error: err instanceof Error ? err.message : String(err),
        }),
      });
    } finally {
      setIsPreviewing(false);
    }
  }

  async function handleSave() {
    setIsSaving(true);
    try {
      const saved = await upsertCustomImporterProfile(draft);
      setDraft(draftFromProfile(saved));
      await loadProfiles();
      onNotice({ kind: "success", message: t("自定义数据源配置已保存。") });
    } catch (err) {
      onNotice({
        kind: "error",
        message: t("保存失败：{error}", {
          error: err instanceof Error ? err.message : String(err),
        }),
      });
    } finally {
      setIsSaving(false);
    }
  }

  async function handleRun(profile: CustomImporterProfile) {
    setRunningProfileId(profile.id);
    try {
      const result = await runCustomImporter(profile.id);
      await loadProfiles();
      onNotice({
        kind: "success",
        message: t("自定义数据源同步完成：写入 {imported} 条，跳过 {skipped} 条。", {
          imported: result.imported,
          skipped: result.skipped,
        }),
      });
    } catch (err) {
      await loadProfiles();
      onNotice({
        kind: "error",
        message: t("自定义数据源同步失败：{error}", {
          error: err instanceof Error ? err.message : String(err),
        }),
      });
    } finally {
      setRunningProfileId(null);
    }
  }

  async function handleDelete(profile: CustomImporterProfile) {
    try {
      await deleteCustomImporterProfile(profile.id);
      if (draft.id === profile.id) {
        setDraft(defaultDraft);
        setPreview(null);
      }
      await loadProfiles();
      onNotice({ kind: "success", message: t("自定义数据源配置已删除。") });
    } catch (err) {
      onNotice({
        kind: "error",
        message: t("删除失败：{error}", {
          error: err instanceof Error ? err.message : String(err),
        }),
      });
    }
  }

  return (
    <section className="panel custom-importers-panel">
      <div className="panel-heading settings-heading">
        <div>
          <h2>{t("自定义数据源")}</h2>
          <p>{t("用只读 SQLite 查询接入其他 Agent。默认只保存统计字段，不写入 prompt/response 明文。")}</p>
        </div>
        <button
          className="primary secondary"
          onClick={() => {
            setDraft(defaultDraft);
            setPreview(null);
          }}
          type="button"
        >
          {t("新建配置")}
        </button>
      </div>

      <div className="custom-importer-layout">
        <div className="custom-profile-list" aria-busy={isLoading}>
          {isLoading ? <div className="empty-state small">{t("正在读取自定义数据源...")}</div> : null}
          {!isLoading && profiles.length === 0 ? (
            <div className="empty-state small">{t("暂无自定义数据源配置")}</div>
          ) : null}
          {profiles.map((profile) => (
            <article className="custom-profile-row" key={profile.id}>
              <div>
                <div className="source-title">
                  <strong>{profile.name}</strong>
                  <span className={`agent-state ${profile.last_run_status === "error" ? "missing" : "synced"}`}>
                    {statusLabel(profile, t)}
                  </span>
                </div>
                <p>{profile.source_key}</p>
                <div className="custom-profile-stats">
                  <span>{formatInteger(profile.imported_calls, numberLocale)} {t("条")}</span>
                  <span>{formatInteger(profile.total_tokens, numberLocale)} Token</span>
                </div>
                {profile.last_run_error ? (
                  <p className="danger-text">{profile.last_run_error}</p>
                ) : null}
              </div>
              <div className="row-actions">
                <button
                  className="primary secondary"
                  onClick={() => {
                    setDraft(draftFromProfile(profile));
                    setPreview(null);
                  }}
                  type="button"
                >
                  {t("编辑")}
                </button>
                <button
                  className="primary"
                  disabled={!profile.enabled || runningProfileId === profile.id}
                  onClick={() => void handleRun(profile)}
                  type="button"
                >
                  {runningProfileId === profile.id ? t("同步中...") : t("同步")}
                </button>
                <button
                  className="primary secondary danger-button"
                  onClick={() => void handleDelete(profile)}
                  type="button"
                >
                  {t("删除")}
                </button>
              </div>
            </article>
          ))}
        </div>

        <div className="custom-importer-form">
          <div className="form-grid">
            <label>
              <span>{t("名称")}</span>
              <input
                value={draft.name}
                onChange={(event) => updateDraft("name", event.target.value)}
              />
            </label>
            <label>
              <span>Source Key</span>
              <input
                value={draft.source_key}
                onChange={(event) => updateDraft("source_key", event.target.value)}
              />
            </label>
            <label className="checkbox-field">
              <input
                checked={draft.enabled}
                onChange={(event) => updateDraft("enabled", event.target.checked)}
                type="checkbox"
              />
              <span>{t("启用这个数据源")}</span>
            </label>
            <label className="field wide">
              <span>{t("SQLite 数据库路径")}</span>
              <input
                value={draft.database_path}
                onChange={(event) => updateDraft("database_path", event.target.value)}
                placeholder="C:\Users\name\.local\share\agent\agent.db"
              />
            </label>
            <label className="field wide">
              <span>{t("只读 SELECT 查询")}</span>
              <textarea
                value={draft.import_sql}
                onChange={(event) => updateDraft("import_sql", event.target.value)}
                rows={5}
              />
            </label>
            <label className="field wide">
              <span>{t("字段映射 JSON")}</span>
              <textarea
                value={draft.mappings_json}
                onChange={(event) => updateDraft("mappings_json", event.target.value)}
                rows={9}
              />
            </label>
          </div>

          <div className="form-actions">
            <button
              className="primary secondary"
              disabled={isPreviewing}
              onClick={() => void handlePreview()}
              type="button"
            >
              {isPreviewing ? t("预览中...") : t("预览查询")}
            </button>
            <button
              className="primary"
              disabled={isSaving}
              onClick={() => void handleSave()}
              type="button"
            >
              {isSaving ? t("保存中...") : t("保存配置")}
            </button>
          </div>

          {preview ? (
            <div className="custom-preview">
              <div className="custom-preview-heading">
                <strong>{t("预览结果")}</strong>
                <span>
                  {formatInteger(preview.rows.length, numberLocale)} {t("行，显示")}{" "}
                  {formatInteger(previewColumns.length, numberLocale)} {t("列")}
                </span>
              </div>
              <div className="custom-preview-table-wrap">
                <table className="custom-preview-table">
                  <thead>
                    <tr>
                      {previewColumns.map((column) => (
                        <th key={column}>{column}</th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {preview.rows.slice(0, 5).map((row, rowIndex) => (
                      <tr key={rowIndex}>
                        {previewColumns.map((column) => (
                          <td key={column}>{previewValue(row[column], t("空"))}</td>
                        ))}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          ) : null}
        </div>
      </div>
    </section>
  );
}
