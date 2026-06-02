import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";

const root = process.cwd();

function readProjectFile(path) {
  return readFileSync(join(root, path), "utf8");
}

test("settings page stays focused on data statistics without proxy setup", () => {
  const settingsPage = readProjectFile("src/components/SettingsPage.tsx");
  const appShell = readProjectFile("src/app/App.tsx");
  const callsPage = readProjectFile("src/components/CallsPage.tsx");
  const tauriEntrypoint = readProjectFile("src-tauri/src/lib.rs");

  assert.equal(settingsPage.includes("Provider 配置"), false);
  assert.equal(settingsPage.includes("API key"), false);
  assert.equal(settingsPage.includes("Proxy 端口"), false);
  assert.equal(settingsPage.includes("debug capture"), false);
  assert.equal(settingsPage.includes("真实 proxy"), false);
  assert.equal(appShell.includes("数据源"), false);
  assert.equal(tauriEntrypoint.includes("list_provider_configs"), false);
  assert.equal(tauriEntrypoint.includes("save_app_settings"), false);
  assert.ok(settingsPage.includes("导出 CSV"));
  assert.ok(settingsPage.includes("生成演示数据"));
  assert.ok(settingsPage.includes("同步本机数据"));
  assert.ok(settingsPage.includes("全量刷新"));
  assert.ok(appShell.includes("clearDemoData"));
  assert.ok(appShell.includes("importDetectedAgents(\"incremental\")"));
  assert.ok(callsPage.includes("导出当前筛选 CSV"));
  assert.ok(appShell.includes("数据健康"));
  assert.ok(appShell.includes("报表导出"));
  assert.ok(appShell.includes("同步本机数据"));
  assert.ok(tauriEntrypoint.includes("clear_demo_data"));
  assert.ok(tauriEntrypoint.includes("get_data_health_summary"));
  assert.ok(tauriEntrypoint.includes("get_top_projects"));
});

test("cost-related UI and pricing actions are sealed from active product surfaces", () => {
  const appShell = readProjectFile("src/app/App.tsx");
  const summaryCards = readProjectFile("src/components/SummaryCards.tsx");
  const topList = readProjectFile("src/components/TopList.tsx");
  const callsTable = readProjectFile("src/components/RecentCallsTable.tsx");
  const chart = readProjectFile("src/components/MiniSeriesChart.tsx");
  const dataHealthPage = readProjectFile("src/components/DataHealthPage.tsx");
  const reportsPage = readProjectFile("src/components/ReportsPage.tsx");
  const settingsPage = readProjectFile("src/components/SettingsPage.tsx");
  const dashboardService = readProjectFile("src/services/dashboard.ts");
  const tauriEntrypoint = readProjectFile("src-tauri/src/lib.rs");
  const repository = readProjectFile("src-tauri/src/db/repository.rs");

  assert.equal(appShell.includes("CostRulesPage"), false);
  assert.equal(appShell.includes("\"costs\""), false);
  assert.equal(summaryCards.includes("formatCost"), false);
  assert.equal(topList.includes("formatCost"), false);
  assert.equal(callsTable.includes("formatCost"), false);
  assert.equal(chart.includes("formatCost"), false);
  assert.equal(dataHealthPage.includes("missing_cost"), false);
  assert.equal(dataHealthPage.includes("missing_pricing_rule"), false);
  assert.equal(reportsPage.includes("费用"), false);
  assert.equal(settingsPage.includes("费用"), false);
  assert.equal(tauriEntrypoint.includes("list_pricing_rules"), false);
  assert.equal(tauriEntrypoint.includes("upsert_pricing_rule"), false);
  assert.equal(tauriEntrypoint.includes("delete_pricing_rule"), false);
  assert.equal(tauriEntrypoint.includes("recalculate_estimated_costs"), false);
  assert.equal(dashboardService.includes("list_pricing_rules"), false);
  assert.equal(dashboardService.includes("upsert_pricing_rule"), false);
  assert.equal(dashboardService.includes("delete_pricing_rule"), false);
  assert.equal(dashboardService.includes("recalculate_estimated_costs"), false);
  const csvHeaders = repository.slice(
    repository.indexOf("const CSV_HEADERS"),
    repository.indexOf("fn render_llm_calls_csv"),
  );
  assert.equal(csvHeaders.includes("\"estimated_cost_usd\""), false);
  assert.equal(csvHeaders.includes("\"cost_currency\""), false);
  assert.equal(csvHeaders.includes("\"cost_source\""), false);
});

test("settings page exposes local agent source sync status", () => {
  const settingsPage = readProjectFile("src/components/SettingsPage.tsx");
  const agentSourcesPanel = readProjectFile("src/components/AgentSourcesPanel.tsx");
  const dashboardService = readProjectFile("src/services/dashboard.ts");
  const importerRegistry = readProjectFile("src-tauri/src/importers/mod.rs");
  const claudeCodeImporter = readProjectFile("src-tauri/src/importers/claude_code.rs");

  assert.ok(settingsPage.includes("AgentSourcesPanel"));
  assert.ok(settingsPage.includes("listAgentSources"));
  assert.ok(settingsPage.includes("detectLocalAgents"));
  assert.ok(settingsPage.includes("importDetectedAgents"));
  assert.ok(settingsPage.includes("result.status"));
  assert.ok(settingsPage.includes("result.error"));
  assert.ok(dashboardService.includes("claude-code"));
  assert.ok(dashboardService.includes("Claude Code"));
  assert.ok(importerRegistry.includes("CLAUDE_CODE_IMPORTER"));
  assert.ok(importerRegistry.includes("claude_code_transcripts"));
  assert.ok(claudeCodeImporter.includes(".claude"));
  assert.ok(claudeCodeImporter.includes("projects"));
  assert.ok(claudeCodeImporter.includes("raw_response_json: None"));
  assert.ok(settingsPage.includes("同步完成"));
  assert.ok(settingsPage.includes("同步失败"));

  assert.ok(agentSourcesPanel.includes("本地 Agent 检测"));
  assert.ok(agentSourcesPanel.includes("正在读取本机 Agent 来源"));
  assert.ok(agentSourcesPanel.includes("来源路径"));
  assert.ok(agentSourcesPanel.includes("最近导入"));
  assert.ok(agentSourcesPanel.includes("最近调用"));
  assert.ok(agentSourcesPanel.includes("导入量"));
  assert.ok(agentSourcesPanel.includes("手动同步"));
});

test("settings page exposes background auto sync settings without proxy setup", () => {
  const settingsPage = readProjectFile("src/components/SettingsPage.tsx");
  const dashboardService = readProjectFile("src/services/dashboard.ts");
  const dashboardTypes = readProjectFile("src/types/dashboard.ts");
  const dashboardCommands = readProjectFile("src-tauri/src/commands/dashboard.rs");

  assert.equal(/proxy/i.test(settingsPage), false);
  assert.ok(settingsPage.includes("后台自动同步"));
  assert.ok(settingsPage.includes("启用后台自动同步"));
  assert.ok(settingsPage.includes("同步间隔"));
  assert.ok(settingsPage.includes("15 分钟"));
  assert.ok(settingsPage.includes("30 分钟"));
  assert.ok(settingsPage.includes("60 分钟"));
  assert.ok(settingsPage.includes("180 分钟"));
  assert.ok(settingsPage.includes("启动后立即同步"));
  assert.ok(settingsPage.includes("最近自动同步"));
  assert.ok(settingsPage.includes("下一次计划"));
  assert.ok(settingsPage.includes("最近结果"));
  assert.ok(settingsPage.includes("最近错误"));
  assert.ok(settingsPage.includes("立即同步一次"));
  assert.ok(settingsPage.includes("保存同步设置"));
  assert.ok(settingsPage.includes("getSyncSettings"));
  assert.ok(settingsPage.includes("saveSyncSettings"));
  assert.ok(settingsPage.includes("runBackgroundSyncOnce"));
  assert.ok(settingsPage.includes("handleFullSync"));

  assert.ok(dashboardService.includes("get_sync_settings"));
  assert.ok(dashboardService.includes("save_sync_settings"));
  assert.ok(dashboardService.includes("run_background_sync_once"));
  assert.ok(dashboardService.includes("AgentImportMode"));
  assert.ok(dashboardService.includes("mode = \"incremental\""));
  assert.ok(dashboardService.includes("import_detected_agents"));
  assert.ok(dashboardService.includes("enabled: false"));
  assert.ok(dashboardService.includes("sync_on_startup: true"));
  assert.ok(dashboardTypes.includes("interface SyncSettings"));
  assert.ok(dashboardTypes.includes("interface SyncSettingsInput"));
  assert.ok(dashboardTypes.includes("status: string"));
  assert.ok(dashboardTypes.includes("error: string | null"));
  assert.ok(dashboardCommands.includes("mode: Option<String>"));
});

test("frontend date windows use local calendar dates instead of UTC ISO dates", () => {
  const files = [
    "src/app/App.tsx",
    "src/components/CallsPage.tsx",
    "src/components/DimensionDetailPage.tsx",
    "src/components/ReportsPage.tsx",
  ];

  for (const file of files) {
    assert.equal(readProjectFile(file).includes("toISOString().slice(0, 10)"), false, file);
  }
});

test("overview supports custom history date ranges and richer daily charts", () => {
  const appShell = readProjectFile("src/app/App.tsx");
  const chart = readProjectFile("src/components/MiniSeriesChart.tsx");
  const dashboardService = readProjectFile("src/services/dashboard.ts");
  const tauriEntrypoint = readProjectFile("src-tauri/src/lib.rs");
  const dashboardCommands = readProjectFile("src-tauri/src/commands/dashboard.rs");

  assert.ok(appShell.includes("customFrom"));
  assert.ok(appShell.includes("customTo"));
  assert.ok(appShell.includes("getDashboardSummaryForDates"));
  assert.ok(appShell.includes("90d"));
  assert.ok(appShell.includes('type="date"'));
  assert.ok(appShell.includes("agentSeries"));
  assert.ok(appShell.includes('getDailyUsageSeries(dateWindow.from, dateWindow.to, "agent")'));
  assert.ok(appShell.includes("agentPoints={agentSeries}"));
  assert.ok(appShell.includes("overview-focus"));
  assert.ok(appShell.includes("overview-secondary"));
  assert.ok(chart.includes("chartMode"));
  assert.ok(chart.includes("agentPoints"));
  assert.ok(chart.includes("usage-chart-main"));
  assert.ok(chart.includes("usage-chart-toolbar"));
  assert.ok(chart.includes("usage-chart-title-block"));
  assert.ok(chart.includes("stacked-bar-segment"));
  assert.ok(chart.includes("line-series-agent"));
  assert.ok(chart.includes("line-series-total"));
  assert.ok(chart.includes("usage-chart-legend"));
  assert.ok(chart.includes("selectedLineSeriesKeys"));
  assert.ok(chart.includes("toggleLineSeries"));
  assert.ok(chart.includes("line-series-toggle"));
  assert.ok(chart.includes("all-line-series-toggle"));
  assert.ok(chart.includes("aria-pressed"));
  assert.ok(chart.includes("line-chart-svg"));
  assert.ok(chart.includes("柱状"));
  assert.ok(chart.includes("折线"));
  assert.ok(dashboardService.includes("groupBy: DimensionKind | null = null"));
  assert.ok(dashboardService.includes("{ from, to, groupBy }"));
  assert.ok(dashboardService.includes("get_dashboard_summary_for_dates"));
  assert.ok(tauriEntrypoint.includes("get_dashboard_summary_for_dates"));
  assert.ok(dashboardCommands.includes("get_dashboard_summary_for_dates"));
});

test("top ranking lists constrain long labels without overflowing cards", () => {
  const topList = readProjectFile("src/components/TopList.tsx");
  const appShell = readProjectFile("src/app/App.tsx");
  const styles = readProjectFile("src/styles.css");

  assert.ok(topList.includes("formatTopDimensionLabel"));
  assert.ok(topList.includes("title={row.dimension}"));
  assert.ok(topList.includes("className=\"top-list-label\""));
  assert.ok(topList.includes("className=\"top-list-value\""));
  assert.ok(appShell.includes("kind=\"session\""));
  assert.ok(styles.includes(".top-list-table"));
  assert.ok(styles.includes("table-layout: fixed"));
  assert.ok(styles.includes(".top-list-label"));
  assert.ok(styles.includes("text-overflow: ellipsis"));
  assert.ok(styles.includes(".top-list-value"));
  assert.ok(styles.includes("white-space: nowrap"));
});

test("settings page exposes configurable sqlite importers without proxy capture", () => {
  const settingsPage = readProjectFile("src/components/SettingsPage.tsx");
  const customImportersPanel = readProjectFile("src/components/CustomImportersPanel.tsx");
  const dashboardService = readProjectFile("src/services/dashboard.ts");
  const dashboardTypes = readProjectFile("src/types/dashboard.ts");
  const tauriEntrypoint = readProjectFile("src-tauri/src/lib.rs");
  const dashboardCommands = readProjectFile("src-tauri/src/commands/dashboard.rs");
  const customSqliteImporter = readProjectFile("src-tauri/src/importers/custom_sqlite.rs");

  assert.ok(settingsPage.includes("CustomImportersPanel"));
  assert.ok(customImportersPanel.includes("previewCustomImporter"));
  assert.ok(customImportersPanel.includes("runCustomImporter"));
  assert.ok(customImportersPanel.includes("mappings_json"));
  assert.ok(customImportersPanel.includes("import_sql"));
  assert.equal(/proxy/i.test(customImportersPanel), false);

  assert.ok(dashboardService.includes("list_custom_importer_profiles"));
  assert.ok(dashboardService.includes("upsert_custom_importer_profile"));
  assert.ok(dashboardService.includes("delete_custom_importer_profile"));
  assert.ok(dashboardService.includes("preview_custom_importer"));
  assert.ok(dashboardService.includes("run_custom_importer"));

  assert.ok(dashboardTypes.includes("interface CustomImporterProfileInput"));
  assert.ok(dashboardTypes.includes("interface CustomImporterPreview"));
  assert.ok(dashboardTypes.includes("interface CustomImporterRunResult"));

  assert.ok(tauriEntrypoint.includes("list_custom_importer_profiles"));
  assert.ok(tauriEntrypoint.includes("upsert_custom_importer_profile"));
  assert.ok(tauriEntrypoint.includes("preview_custom_importer"));
  assert.ok(tauriEntrypoint.includes("run_custom_importer"));

  assert.ok(dashboardCommands.includes("validate_profile_input"));
  assert.ok(customSqliteImporter.includes("read_only(true)"));
  assert.ok(customSqliteImporter.includes("raw_response_json: None"));
  assert.ok(customSqliteImporter.includes("validate_import_sql"));
});

test("settings page supports device dataset packages for multi-device merge", () => {
  const settingsPage = readProjectFile("src/components/SettingsPage.tsx");
  const deviceDatasetsPanel = readProjectFile("src/components/DeviceDatasetsPanel.tsx");
  const dashboardService = readProjectFile("src/services/dashboard.ts");
  const dashboardTypes = readProjectFile("src/types/dashboard.ts");
  const tauriEntrypoint = readProjectFile("src-tauri/src/lib.rs");
  const settingsCommands = readProjectFile("src-tauri/src/commands/settings.rs");

  assert.ok(settingsPage.includes("DeviceDatasetsPanel"));
  assert.ok(deviceDatasetsPanel.includes(".tokenscope"));
  assert.ok(deviceDatasetsPanel.includes("@tauri-apps/plugin-dialog"));
  assert.ok(deviceDatasetsPanel.includes("directory: true"));
  assert.ok(deviceDatasetsPanel.includes("extensions: [\"tokenscope\"]"));
  assert.ok(deviceDatasetsPanel.includes("选择导出目录"));
  assert.ok(deviceDatasetsPanel.includes("选择并导入"));
  assert.ok(deviceDatasetsPanel.includes("导出本机数据包"));
  assert.ok(deviceDatasetsPanel.includes("打开导出文件夹"));
  assert.ok(deviceDatasetsPanel.includes("openExportFolder"));
  assert.ok(deviceDatasetsPanel.includes("导入设备数据包"));
  assert.ok(deviceDatasetsPanel.includes("移除"));
  assert.ok(deviceDatasetsPanel.includes("不会影响本机数据"));

  assert.ok(dashboardService.includes("export_device_dataset_package"));
  assert.ok(dashboardService.includes("import_device_dataset_package"));
  assert.ok(dashboardService.includes("list_external_datasets"));
  assert.ok(dashboardService.includes("remove_external_dataset"));
  assert.ok(dashboardService.includes("open_export_folder"));
  assert.ok(dashboardService.includes("exportDir"));

  assert.ok(dashboardTypes.includes("interface ExternalDataset"));
  assert.ok(dashboardTypes.includes("interface DevicePackageImportResult"));
  assert.ok(tauriEntrypoint.includes("tauri_plugin_dialog::init()"));
  assert.ok(tauriEntrypoint.includes("export_device_dataset_package"));
  assert.ok(tauriEntrypoint.includes("import_device_dataset_package"));
  assert.ok(tauriEntrypoint.includes("remove_external_dataset"));
  assert.ok(tauriEntrypoint.includes("open_export_folder"));
  assert.ok(settingsCommands.includes("std::env::temp_dir()"));
  assert.ok(settingsCommands.includes("export_dir: Option<String>"));
});

test("windows release binary is configured without a console window", () => {
  const mainEntrypoint = readProjectFile("src-tauri/src/main.rs");

  assert.ok(mainEntrypoint.includes("windows_subsystem = \"windows\""));
  assert.ok(mainEntrypoint.includes("not(debug_assertions)"));
});

test("release packaging uses a Windows installer instead of a bare executable only", () => {
  const tauriConfig = JSON.parse(readProjectFile("src-tauri/tauri.conf.json"));

  assert.equal(tauriConfig.bundle.active, true);
  assert.deepEqual(tauriConfig.bundle.targets, ["nsis"]);
  assert.equal(tauriConfig.bundle.windows.nsis.installerIcon, "icons/icon.ico");
  assert.equal(tauriConfig.bundle.windows.nsis.uninstallerIcon, "icons/icon.ico");
});

test("application exposes a signed Tauri updater workflow", () => {
  const packageJson = JSON.parse(readProjectFile("package.json"));
  const cargoToml = readProjectFile("src-tauri/Cargo.toml");
  const tauriConfig = JSON.parse(readProjectFile("src-tauri/tauri.conf.json"));
  const capabilities = JSON.parse(readProjectFile("src-tauri/capabilities/default.json"));
  const tauriEntrypoint = readProjectFile("src-tauri/src/lib.rs");
  const settingsPage = readProjectFile("src/components/SettingsPage.tsx");
  const dashboardService = readProjectFile("src/services/dashboard.ts");
  const dashboardTypes = readProjectFile("src/types/dashboard.ts");
  const releaseScript = readProjectFile("scripts/create-latest-json.ps1");
  const buildScript = readProjectFile("scripts/build-release.ps1");

  assert.ok(packageJson.dependencies["@tauri-apps/plugin-updater"]);
  assert.ok(packageJson.dependencies["@tauri-apps/plugin-process"]);
  assert.ok(cargoToml.includes("tauri-plugin-updater"));
  assert.ok(cargoToml.includes("tauri-plugin-process"));
  assert.equal(tauriConfig.bundle.createUpdaterArtifacts, true);
  assert.ok(tauriConfig.plugins.updater.pubkey.length > 80);
  assert.deepEqual(tauriConfig.plugins.updater.endpoints, [
    "https://github.com/RickChen764/tokenscope-desktop-public/releases/latest/download/latest.json",
  ]);
  assert.equal(tauriConfig.plugins.updater.windows.installMode, "passive");
  assert.ok(capabilities.permissions.includes("updater:default"));
  assert.ok(capabilities.permissions.includes("process:default"));
  assert.ok(tauriEntrypoint.includes("tauri_plugin_updater::Builder::new().build()"));
  assert.ok(tauriEntrypoint.includes("tauri_plugin_process::init()"));

  assert.ok(settingsPage.includes("checkForAppUpdate"));
  assert.ok(settingsPage.includes("installPendingAppUpdate"));
  assert.ok(settingsPage.includes("update-progress-bar"));
  assert.ok(settingsPage.includes("应用更新"));
  assert.ok(settingsPage.includes("下载并安装"));
  assert.ok(dashboardService.includes("@tauri-apps/plugin-updater"));
  assert.ok(dashboardService.includes("@tauri-apps/plugin-process"));
  assert.ok(dashboardService.includes("downloadAndInstall"));
  assert.ok(dashboardService.includes("relaunch"));
  assert.ok(dashboardTypes.includes("interface AppUpdateInfo"));
  assert.ok(dashboardTypes.includes("interface AppUpdateProgress"));

  assert.ok(releaseScript.includes("latest.json"));
  assert.ok(releaseScript.includes("windows-x86_64"));
  assert.ok(releaseScript.includes("$publishedSignaturePath"));
  assert.ok(releaseScript.includes("$installerPattern"));
  assert.ok(releaseScript.includes("_x64-setup.exe"));
  assert.ok(releaseScript.includes('RepoFullName = "RickChen764/tokenscope-desktop-public"'));
  assert.ok(releaseScript.includes("releases/download"));
  assert.ok(buildScript.includes("TAURI_SIGNING_PRIVATE_KEY_PATH"));
  assert.ok(buildScript.includes("TAURI_SIGNING_PRIVATE_KEY = Get-Content"));
  assert.ok(buildScript.includes("pnpm exec tauri build --ci"));
  assert.ok(buildScript.includes("tokenscope-desktop.key"));
});
