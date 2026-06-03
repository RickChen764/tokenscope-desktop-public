import { useCallback, useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  exportDeviceDatasetPackage,
  importDeviceDatasetPackage,
  listExternalDatasets,
  openExportFolder,
  removeExternalDataset,
} from "../services/dashboard";
import type { ExternalDataset } from "../types/dashboard";
import { useI18n } from "../i18n";
import { formatDateTime, formatInteger } from "../utils/format";

interface DeviceDatasetsPanelProps {
  onNotice: (notice: { kind: "error" | "success"; message: string } | null) => void;
}

export function DeviceDatasetsPanel({ onNotice }: DeviceDatasetsPanelProps) {
  const { numberLocale, t } = useI18n();
  const [datasets, setDatasets] = useState<ExternalDataset[]>([]);
  const [lastExportDir, setLastExportDir] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isExporting, setIsExporting] = useState(false);
  const [isOpeningFolder, setIsOpeningFolder] = useState(false);
  const [isImporting, setIsImporting] = useState(false);
  const [removingId, setRemovingId] = useState<string | null>(null);

  const loadDatasets = useCallback(async () => {
    setIsLoading(true);
    try {
      setDatasets(await listExternalDatasets());
    } catch (err) {
      onNotice({
        kind: "error",
        message: t("读取设备数据失败：{error}", {
          error: err instanceof Error ? err.message : String(err),
        }),
      });
    } finally {
      setIsLoading(false);
    }
  }, [onNotice]);

  useEffect(() => {
    void loadDatasets();
  }, [loadDatasets]);

  async function handleExportPackage() {
    setIsExporting(true);
    onNotice(null);
    try {
      const selectedDir = await openDialog({
        directory: true,
        multiple: false,
        title: t("选择导出目录"),
      });
      if (!selectedDir || Array.isArray(selectedDir)) {
        return;
      }

      const path = await exportDeviceDatasetPackage(selectedDir);
      setLastExportDir(selectedDir);
      onNotice({ kind: "success", message: t(".tokenscope 数据包已导出：{path}", { path }) });
    } catch (err) {
      onNotice({
        kind: "error",
        message: t("导出本机数据包失败：{error}", {
          error: err instanceof Error ? err.message : String(err),
        }),
      });
    } finally {
      setIsExporting(false);
    }
  }

  async function handleOpenExportFolder() {
    setIsOpeningFolder(true);
    onNotice(null);
    try {
      const path = await openExportFolder(lastExportDir ?? undefined);
      onNotice({ kind: "success", message: t("已打开导出文件夹：{path}", { path }) });
    } catch (err) {
      onNotice({
        kind: "error",
        message: t("打开导出文件夹失败：{error}", {
          error: err instanceof Error ? err.message : String(err),
        }),
      });
    } finally {
      setIsOpeningFolder(false);
    }
  }

  async function handleImportPackage() {
    setIsImporting(true);
    onNotice(null);
    try {
      const selectedFile = await openDialog({
        filters: [{ name: t("TokenScope 数据包"), extensions: ["tokenscope"] }],
        multiple: false,
        title: t("选择 .tokenscope 数据包"),
      });
      if (!selectedFile || Array.isArray(selectedFile)) {
        return;
      }

      const path = selectedFile;
      const result = await importDeviceDatasetPackage(path);
      await loadDatasets();
      onNotice({
        kind: "success",
        message: t("导入设备数据包完成：{device} 写入 {imported} 条，跳过 {skipped} 条。", {
          device: result.dataset.device_name,
          imported: result.imported,
          skipped: result.skipped,
        }),
      });
    } catch (err) {
      onNotice({
        kind: "error",
        message: t("导入设备数据包失败：{error}", {
          error: err instanceof Error ? err.message : String(err),
        }),
      });
    } finally {
      setIsImporting(false);
    }
  }

  async function handleRemoveDataset(dataset: ExternalDataset) {
    const confirmed = window.confirm(
      t("确认移除 {device} 的导入数据？这不会影响本机数据。", {
        device: dataset.device_name,
      }),
    );
    if (!confirmed) {
      return;
    }

    setRemovingId(dataset.id);
    onNotice(null);
    try {
      const removed = await removeExternalDataset(dataset.id);
      await loadDatasets();
      onNotice({
        kind: "success",
        message: t("已移除 {device} 的导入数据：{removed} 条；不会影响本机数据。", {
          device: dataset.device_name,
          removed,
        }),
      });
    } catch (err) {
      onNotice({
        kind: "error",
        message: t("移除设备数据失败：{error}", {
          error: err instanceof Error ? err.message : String(err),
        }),
      });
    } finally {
      setRemovingId(null);
    }
  }

  return (
    <section className="panel device-datasets-panel" aria-busy={isLoading}>
      <div className="panel-heading settings-heading">
        <div>
          <h2>{t("多设备数据包")}</h2>
          <p>{t("用 .tokenscope 数据包合并其他电脑的统计数据；导入、更新和移除都不会影响本机数据。")}</p>
        </div>
        <div className="heading-actions">
          <button
            className="primary secondary"
            disabled={isOpeningFolder}
            onClick={() => void handleOpenExportFolder()}
            type="button"
          >
            {isOpeningFolder ? t("打开中...") : t("打开导出文件夹")}
          </button>
          <button
            className="primary secondary"
            disabled={isExporting}
            onClick={() => void handleExportPackage()}
            type="button"
          >
            {isExporting ? t("导出中...") : t("导出本机数据包")}
          </button>
        </div>
      </div>

      <div className="device-package-import">
        <div className="device-package-copy">
          <strong>{t("导入设备数据包")}</strong>
          <span>{t("从其他电脑导出的 .tokenscope 文件中选择一个导入，重复导入同一设备会刷新该设备数据。")}</span>
        </div>
        <button
          className="primary"
          disabled={isImporting}
          onClick={() => void handleImportPackage()}
          type="button"
        >
          {isImporting ? t("导入中...") : t("选择并导入")}
        </button>
      </div>

      {datasets.length === 0 ? (
        <div className="empty-state small">
          {isLoading ? t("正在读取设备数据...") : t("还没有导入其他设备的数据。")}
        </div>
      ) : (
        <div className="device-dataset-list">
          {datasets.map((dataset) => (
            <div className="device-dataset-row" key={dataset.id}>
              <div>
                <strong>{dataset.device_name}</strong>
                <p>
                  {t("最近更新")} {formatDateTime(dataset.updated_at, t("无"))} · {t("来源")}{" "}
                  {dataset.source_path || t("未知")}
                </p>
              </div>
              <div className="device-dataset-stats">
                <span>{formatInteger(dataset.calls, numberLocale)} {t("次调用")}</span>
                <span>{formatInteger(dataset.total_tokens, numberLocale)} Token</span>
              </div>
              <button
                className="pagination-button danger-button"
                disabled={removingId === dataset.id}
                onClick={() => void handleRemoveDataset(dataset)}
                type="button"
              >
                {removingId === dataset.id ? t("移除中...") : t("移除")}
              </button>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
