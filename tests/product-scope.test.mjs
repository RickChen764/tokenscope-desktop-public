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
  const styles = readProjectFile("src/styles.css");
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
  assert.ok(styles.includes(".source-stats"));
  assert.ok(styles.includes("grid-template-columns: repeat(4, minmax(0, 1fr))"));
  assert.ok(styles.includes("align-items: end"));
  assert.ok(styles.includes(".source-stat {"));
  assert.ok(styles.includes("justify-content: space-between"));
  assert.ok(styles.includes("grid-template-columns: auto minmax(0, 1fr)"));
  assert.ok(styles.includes("text-overflow: ellipsis"));
  assert.ok(styles.includes("white-space: nowrap"));
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
  assert.ok(settingsPage.includes("SYNC_INTERVAL_VALUES"));
  assert.ok(settingsPage.includes("[1, 5, 15, 30, 60]"));
  assert.ok(settingsPage.includes('t("分钟")'));
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
  assert.ok(dashboardService.includes("enabled: true"));
  assert.ok(dashboardService.includes("sync_on_startup: true"));
  assert.ok(dashboardService.includes("SYNC_SETTINGS_STORAGE_KEY"));
  assert.ok(dashboardService.includes("window.localStorage.setItem"));
  assert.ok(dashboardService.includes("window.localStorage.getItem"));
  assert.ok(dashboardTypes.includes("interface SyncSettings"));
  assert.ok(dashboardTypes.includes("interface SyncSettingsInput"));
  assert.ok(dashboardTypes.includes("status: string"));
  assert.ok(dashboardTypes.includes("error: string | null"));
  assert.ok(dashboardCommands.includes("mode: Option<String>"));
});

test("background sync settings auto-save and use a contained split layout", () => {
  const settingsPage = readProjectFile("src/components/SettingsPage.tsx");
  const styles = readProjectFile("src/styles.css");

  assert.ok(settingsPage.includes("syncDraftRef"));
  assert.ok(settingsPage.includes("persistSyncSettingsDraft"));
  assert.ok(settingsPage.includes("void persistSyncSettingsDraft(nextDraft)"));
  assert.ok(styles.includes(".sync-settings-card .form-actions"));
  assert.ok(styles.includes("display: none"));
  assert.ok(styles.includes(".sync-settings-layout"));
  assert.ok(styles.includes("grid-template-columns: minmax(280px, 0.75fr) minmax(0, 1.25fr)"));
  assert.ok(styles.includes(".sync-result-panel"));
  assert.ok(styles.includes(".sync-result-text"));
  assert.ok(styles.includes("overflow-wrap: anywhere"));
});

test("settings page is organized into clear grouped sections", () => {
  const settingsPage = readProjectFile("src/components/SettingsPage.tsx");
  const styles = readProjectFile("src/styles.css");

  assert.ok(settingsPage.includes("settings-section data-sync-section"));
  assert.ok(settingsPage.includes("settings-section data-portability-section"));
  assert.ok(settingsPage.includes("settings-section app-preferences-section"));
  assert.ok(settingsPage.includes("settings-section-heading"));
  assert.ok(settingsPage.includes("sync-layout-grid"));
  assert.ok(settingsPage.includes("settings-action-strip"));
  assert.ok(settingsPage.includes("sync-status-message"));
  assert.ok(settingsPage.includes("title={lastResultLabel}"));
  assert.ok(settingsPage.includes("title={lastErrorLabel}"));
  assert.ok(settingsPage.includes("settings-two-column"));
  assert.ok(settingsPage.includes("settings-app-grid"));
  assert.ok(settingsPage.indexOf("AgentSourcesPanel") < settingsPage.indexOf("sync-settings-card"));
  assert.ok(settingsPage.indexOf("<DeviceDatasetsPanel") < settingsPage.indexOf("<CustomImportersPanel"));
  assert.ok(styles.includes(".settings-section"));
  assert.ok(styles.includes(".settings-section-heading"));
  assert.ok(styles.includes(".sync-layout-grid"));
  assert.ok(styles.includes(".settings-action-strip"));
  assert.ok(styles.includes(".sync-status-message strong"));
  assert.ok(styles.includes("-webkit-line-clamp: 3"));
  assert.ok(styles.includes(".settings-two-column"));
  assert.ok(styles.includes(".settings-app-grid"));
});

test("primary page headings avoid redundant English eyebrow labels", () => {
  const files = [
    "src/app/App.tsx",
    "src/components/SettingsPage.tsx",
    "src/components/DeviceDatasetsPanel.tsx",
    "src/components/CustomImportersPanel.tsx",
    "src/components/DataHealthPage.tsx",
    "src/components/DimensionIndexPage.tsx",
    "src/components/ReportsPage.tsx",
  ];
  const removedLabels = [
    "TokenScope Desktop",
    "Data Sync",
    "Background Sync",
    "Data Tools",
    "Portability & Extensions",
    "Application",
    "Language",
    "App Update",
    "Privacy Boundary",
    "Device Packages",
    "Custom Sources",
    "Data Health",
    "Dimension Analysis",
    "Reports",
    "Export Scope",
  ];

  for (const file of files) {
    const source = readProjectFile(file);
    for (const label of removedLabels) {
      assert.equal(source.includes(`className="eyebrow">${label}`), false, `${file}: ${label}`);
    }
  }
});

test("visual theme is restrained and uses a VS Code style dark palette", () => {
  const styles = readProjectFile("src/styles.css");
  const themeStart = styles.indexOf("VS Code restrained theme");

  assert.ok(themeStart > 0);

  const theme = styles.slice(themeStart);
  assert.ok(theme.includes("--app-bg: #1e1e1e"));
  assert.ok(theme.includes("--surface: #252526"));
  assert.ok(theme.includes("--accent: #007acc"));
  assert.ok(theme.includes(".summary-card::before"));
  assert.ok(theme.includes("background: #007acc"));
  assert.ok(theme.includes(".usage-chart-main"));
  assert.ok(theme.includes("box-shadow: none"));
  assert.equal(theme.includes("rgb(45 212 191"), false);
  assert.equal(theme.includes("radial-gradient"), false);
});

test("overview visual treatment keeps content but reduces card framing", () => {
  const styles = readProjectFile("src/styles.css");
  const themeStart = styles.indexOf("VS Code restrained theme");
  const theme = styles.slice(themeStart);

  assert.ok(theme.includes(".summary-grid"));
  assert.ok(theme.includes("grid-template-columns: repeat(7, minmax(0, 1fr))"));
  assert.ok(theme.includes(".summary-card:not(:last-child)"));
  assert.ok(theme.includes("border-right: 1px solid #333333"));
  assert.ok(theme.includes(".overview-rank-card"));
  assert.ok(theme.includes("border: 0"));
  assert.ok(theme.includes(".usage-chart-main"));
  assert.ok(theme.includes("border: 0"));
  assert.ok(theme.includes(".usage-echarts-stage"));
  assert.ok(theme.includes(".usage-echarts"));
  assert.ok(theme.includes(".usage-tooltip"));
});

test("non-overview pages reuse the overview report visual treatment", () => {
  const styles = readProjectFile("src/styles.css");
  const dimensionIndex = readProjectFile("src/components/DimensionIndexPage.tsx");
  const pageSkinStart = styles.indexOf("Page-wide report skin");
  const pageSkin = styles.slice(pageSkinStart);

  assert.ok(pageSkinStart > 0);
  assert.ok(pageSkin.includes(".data-health-page"));
  assert.ok(pageSkin.includes(".reports-page"));
  assert.ok(pageSkin.includes(".dimension-index"));
  assert.ok(pageSkin.includes(".dimension-detail"));
  assert.ok(pageSkin.includes(".settings-page"));
  assert.ok(pageSkin.includes(".dimension-list-grid"));
  assert.ok(pageSkin.includes("grid-template-columns: repeat(3, minmax(260px, 1fr))"));
  assert.ok(pageSkin.includes(".calls-filter-bar"));
  assert.ok(pageSkin.includes(".settings-section"));
  assert.ok(pageSkin.includes("border-radius: 0"));
  assert.ok(pageSkin.includes("border-left: 0"));
  assert.ok(pageSkin.includes("border-right: 0"));
  assert.ok(dimensionIndex.includes('variant="overview"'));
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
  assert.ok(chart.includes("echarts/core"));
  assert.ok(chart.includes("BarChart"));
  assert.ok(chart.includes("LineChart"));
  assert.ok(chart.includes("LegendComponent"));
  assert.ok(chart.includes("TooltipComponent"));
  assert.ok(chart.includes("CanvasRenderer"));
  assert.ok(chart.includes("axisPointer"));
  assert.ok(chart.includes("peakBucket"));
  assert.ok(chart.includes("usage-echarts"));
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

test("overview token numbers use compact labels while preserving exact hover values", () => {
  const summaryCards = readProjectFile("src/components/SummaryCards.tsx");
  const chart = readProjectFile("src/components/MiniSeriesChart.tsx");
  const topList = readProjectFile("src/components/TopList.tsx");

  assert.ok(summaryCards.includes("formatTokenByDisplayMode"));
  assert.ok(summaryCards.includes("exactValue"));
  assert.ok(summaryCards.includes("title={card.exactValue"));
  assert.ok(chart.includes("formatTokenByDisplayMode"));
  assert.ok(chart.includes("formatTooltipValue"));
  assert.ok(chart.includes("formatInteger(value, locale)"));
  assert.ok(chart.includes("title={`${formatInteger(chartData.totalTokens"));
  assert.ok(topList.includes("formatTokenByDisplayMode"));
  assert.ok(topList.includes("variant === \"overview\""));
  assert.ok(topList.includes("title={formatInteger(row.total_tokens"));
});

test("token number display mode is user configurable and persisted", () => {
  const main = readProjectFile("src/main.tsx");
  const preferences = readProjectFile("src/preferences/display.tsx");
  const settingsPage = readProjectFile("src/components/SettingsPage.tsx");
  const summaryCards = readProjectFile("src/components/SummaryCards.tsx");
  const chart = readProjectFile("src/components/MiniSeriesChart.tsx");
  const topList = readProjectFile("src/components/TopList.tsx");

  assert.ok(main.includes("DisplayPreferenceProvider"));
  assert.ok(preferences.includes("TokenScopeNumberDisplayMode"));
  assert.ok(preferences.includes("numberDisplayMode"));
  assert.ok(preferences.includes("setNumberDisplayMode"));
  assert.ok(preferences.includes("window.localStorage.setItem"));
  assert.ok(settingsPage.includes("useDisplayPreference"));
  assert.ok(settingsPage.includes("数字显示"));
  assert.ok(settingsPage.includes("缩略显示"));
  assert.ok(settingsPage.includes("完整显示"));
  assert.ok(summaryCards.includes("useDisplayPreference"));
  assert.ok(summaryCards.includes("formatTokenByDisplayMode"));
  assert.ok(chart.includes("useDisplayPreference"));
  assert.ok(chart.includes("formatTokenByDisplayMode"));
  assert.ok(topList.includes("useDisplayPreference"));
  assert.ok(topList.includes("formatTokenByDisplayMode"));
});

test("application settings controls stay contained in a balanced settings grid", () => {
  const settingsPage = readProjectFile("src/components/SettingsPage.tsx");
  const styles = readProjectFile("src/styles.css");

  assert.ok(settingsPage.includes("settings-section app-preferences-section"));
  assert.ok(settingsPage.includes("display-preference-card"));
  assert.ok(settingsPage.includes("display-mode-segmented"));
  assert.ok(settingsPage.includes("language-card"));
  assert.ok(settingsPage.includes("app-update-card"));
  assert.ok(styles.includes(".settings-app-grid"));
  assert.ok(styles.includes("grid-template-columns: repeat(3, minmax(260px, 1fr))"));
  assert.ok(styles.includes(".settings-app-grid .settings-heading"));
  assert.ok(styles.includes("flex-direction: column"));
  assert.ok(styles.includes(".display-mode-segmented"));
  assert.ok(styles.includes("grid-template-columns: repeat(2, minmax(0, 1fr))"));
  assert.ok(styles.includes(".settings-app-grid > .panel:not(:nth-child(3n))"));
  assert.ok(styles.includes("@media (max-width: 1280px)"));
});

test("daily chart uses an information-rich visual stage without losing scale context", () => {
  const chart = readProjectFile("src/components/MiniSeriesChart.tsx");
  const styles = readProjectFile("src/styles.css");

  assert.ok(chart.includes("peakBucket"));
  assert.ok(chart.includes("averageDailyTokens"));
  assert.ok(chart.includes("echarts/core"));
  assert.ok(chart.includes("BarChart"));
  assert.ok(chart.includes("LineChart"));
  assert.ok(chart.includes("TooltipComponent"));
  assert.ok(chart.includes("CanvasRenderer"));
  assert.ok(chart.includes("usage-echarts"));
  assert.ok(chart.includes("setOption"));
  assert.ok(styles.includes(".usage-echarts"));
  assert.ok(styles.includes(".usage-chart-stage"));
});

test("line chart y-axis unit does not overlap the top scale label", () => {
  const chart = readProjectFile("src/components/MiniSeriesChart.tsx");

  assert.ok(chart.includes('name: "Token"'));
  assert.ok(chart.includes("nameGap: 16"));
  assert.ok(chart.includes("axisLabel"));
  assert.equal(chart.includes("lineChartSize"), false);
  assert.equal(chart.includes("line-chart-svg"), false);
});

test("side rail sync status uses a top-layer custom popover", () => {
  const appShell = readProjectFile("src/app/App.tsx");
  const styles = readProjectFile("src/styles.css");

  assert.ok(appShell.includes("rail-status-popover"));
  assert.ok(appShell.includes("id=\"sync-status-popover\""));
  assert.ok(appShell.includes("aria-describedby=\"sync-status-popover\""));
  assert.equal(appShell.includes("title={syncStatusTitle}"), false);
  assert.ok(styles.includes(".rail-status-popover"));
  assert.ok(styles.includes("z-index: 2147483000"));
  assert.ok(styles.includes("overflow: visible"));
  assert.ok(styles.includes(".sync-status-rail:hover .rail-status-popover"));
});

test("action notices use auto-dismiss toast tips with hover persistence", () => {
  const appShell = readProjectFile("src/app/App.tsx");
  const settingsPage = readProjectFile("src/components/SettingsPage.tsx");
  const reportsPage = readProjectFile("src/components/ReportsPage.tsx");
  const toastNotice = readProjectFile("src/components/ToastNotice.tsx");
  const styles = readProjectFile("src/styles.css");

  assert.ok(appShell.includes("ToastNotice"));
  assert.ok(settingsPage.includes("ToastNotice"));
  assert.ok(reportsPage.includes("ToastNotice"));
  assert.ok(toastNotice.includes("TOAST_AUTO_DISMISS_MS = 5000"));
  assert.ok(toastNotice.includes("onMouseEnter"));
  assert.ok(toastNotice.includes("onMouseLeave"));
  assert.ok(toastNotice.includes("onPointerEnter"));
  assert.ok(toastNotice.includes("onPointerLeave"));
  assert.ok(toastNotice.includes("hoverRef.current"));
  assert.ok(toastNotice.includes("setCanDismiss(true)"));
  assert.ok(toastNotice.includes("exiting"));
  assert.ok(styles.includes(".toast-viewport"));
  assert.ok(styles.includes(".toast-notice"));
  assert.ok(styles.includes("@keyframes toast-in"));
  assert.ok(styles.includes("@keyframes toast-out"));
});

test("desktop tray exposes a today token pulse with historical average comparison", () => {
  const tauriEntrypoint = readProjectFile("src-tauri/src/lib.rs");
  const trayStatus = readProjectFile("src-tauri/src/tray_status.rs");
  const dashboardCommands = readProjectFile("src-tauri/src/commands/dashboard.rs");
  const dashboardModels = readProjectFile("src-tauri/src/db/models.rs");
  const repository = readProjectFile("src-tauri/src/db/repository.rs");
  const dashboardService = readProjectFile("src/services/dashboard.ts");
  const dashboardTypes = readProjectFile("src/types/dashboard.ts");
  const appShell = readProjectFile("src/app/App.tsx");
  const tokenPulseWindow = readProjectFile("src/components/TokenPulseWindow.tsx");
  const defaultCapability = readProjectFile("src-tauri/capabilities/default.json");
  const styles = readProjectFile("src/styles.css");

  assert.ok(tauriEntrypoint.includes("mod tray_status"));
  assert.ok(tauriEntrypoint.includes("tray_status::setup_token_pulse_tray"));
  assert.ok(tauriEntrypoint.includes("TokenScope Desktop"));
  assert.ok(tauriEntrypoint.includes("get_token_pulse"));
  assert.ok(trayStatus.includes("TrayIconBuilder"));
  assert.ok(trayStatus.includes("WebviewWindowBuilder"));
  assert.ok(trayStatus.includes("TOKEN_PULSE_WINDOW_LABEL"));
  assert.ok(trayStatus.includes("TOKEN_PULSE_DETAIL_WINDOW_LABEL"));
  assert.ok(trayStatus.includes("TOKEN_PULSE_COLLAPSED_WIDTH"));
  assert.ok(trayStatus.includes("TOKEN_PULSE_COLLAPSED_HEIGHT"));
  assert.ok(trayStatus.includes("TOKEN_PULSE_DETAIL_WIDTH"));
  assert.ok(trayStatus.includes("TOKEN_PULSE_DETAIL_HEIGHT"));
  assert.ok(trayStatus.includes("TOKEN_PULSE_POSITION_X_SETTING"));
  assert.ok(trayStatus.includes("TOKEN_PULSE_POSITION_Y_SETTING"));
  assert.ok(trayStatus.includes("TOKEN_PULSE_DRAG_DEBOUNCE_MS"));
  assert.ok(trayStatus.includes("tokenPulse=1"));
  assert.ok(trayStatus.includes("tokenPulseDetail=1"));
  assert.ok(trayStatus.includes(".always_on_top(true)"));
  assert.ok(trayStatus.includes(".skip_taskbar(true)"));
  assert.ok(trayStatus.includes(".decorations(false)"));
  assert.ok(trayStatus.includes(".visible(false)"));
  assert.ok(trayStatus.includes("work_area"));
  assert.ok(trayStatus.includes("set_tooltip"));
  assert.ok(trayStatus.includes("format_token_pulse_tooltip"));
  assert.ok(trayStatus.includes("TOKEN_PULSE_DETAIL_HIDE_DELAY_MS"));
  assert.ok(trayStatus.includes("pub fn set_token_pulse_detail_hovered"));
  assert.ok(trayStatus.includes("TokenPulseInteractionState"));
  assert.ok(trayStatus.includes("get_webview_window(TOKEN_PULSE_DETAIL_WINDOW_LABEL)"));
  assert.ok(trayStatus.includes('hovered && source == "mini"'));
  assert.ok(trayStatus.includes(".show()"));
  assert.ok(trayStatus.includes(".hide()"));
  assert.ok(trayStatus.includes("token_pulse_window_position_for_monitor"));
  assert.ok(trayStatus.includes("stored_token_pulse_window_position"));
  assert.ok(trayStatus.includes("save_token_pulse_window_position"));
  assert.ok(trayStatus.includes("clamp_token_pulse_position"));
  assert.ok(trayStatus.includes("set_token_pulse_dragging"));
  assert.ok(trayStatus.includes("hide_token_pulse_detail_window(&app)"));
  assert.ok(trayStatus.includes("state.mini_hovered = false"));
  assert.ok(trayStatus.includes("current_monitor"));

  assert.ok(appShell.includes("TokenPulseWindow"));
  assert.ok(appShell.includes("TokenPulseDetailWindow"));
  assert.ok(appShell.includes("isTokenPulseWindow"));
  assert.ok(appShell.includes("isTokenPulseDetailWindow"));
  assert.ok(tauriEntrypoint.includes("tray_status::set_token_pulse_detail_hovered"));
  assert.ok(tauriEntrypoint.includes("tray_status::set_token_pulse_dragging"));
  assert.ok(tokenPulseWindow.includes("setTokenPulseDetailHovered"));
  assert.ok(tokenPulseWindow.includes("setTokenPulseDragging"));
  assert.equal(tokenPulseWindow.includes("setTokenPulseExpanded"), false);
  assert.ok(tokenPulseWindow.includes("getTokenPulse"));
  assert.ok(tokenPulseWindow.includes("TOKEN_PULSE_REFRESH_MS = 60000"));
  assert.ok(tokenPulseWindow.includes("ratio_to_average"));
  assert.ok(tokenPulseWindow.includes("hourly_tokens"));
  assert.ok(tokenPulseWindow.includes("export function TokenPulseDetailWindow"));
  assert.ok(tokenPulseWindow.includes("token-pulse-mini"));
  assert.ok(tokenPulseWindow.includes("token-pulse-detail"));
  assert.ok(tokenPulseWindow.includes("onPointerEnter"));
  assert.ok(tokenPulseWindow.includes("onPointerLeave"));
  assert.ok(tokenPulseWindow.includes("onPointerDown"));
  assert.ok(tokenPulseWindow.includes("onPointerMove"));
  assert.ok(tokenPulseWindow.includes("onLostPointerCapture"));
  assert.ok(tokenPulseWindow.includes("window.addEventListener(\"pointerup\""));
  assert.ok(tokenPulseWindow.includes("window.addEventListener(\"blur\""));
  assert.ok(tokenPulseWindow.includes("setTokenPulseDragging(true)"));
  assert.ok(tokenPulseWindow.includes("token-pulse-drag-handle"));
  assert.ok(tokenPulseWindow.includes("data-tauri-drag-region"));
  assert.equal(tokenPulseWindow.includes("title={todayTitle}"), false);
  assert.ok(styles.includes(".token-pulse-shell"));
  assert.ok(styles.includes(".token-pulse-mini-shell"));
  assert.ok(styles.includes(".token-pulse-detail-shell"));
  assert.ok(styles.includes(".token-pulse-mini"));
  assert.ok(styles.includes(".token-pulse-drag-handle"));
  assert.equal(styles.includes("-webkit-app-region: drag"), false);
  assert.ok(styles.includes("cursor: grab"));
  assert.ok(styles.includes("cursor: grabbing"));
  assert.ok(styles.includes(".token-pulse-detail"));
  assert.ok(styles.includes(".token-pulse-meter"));

  assert.ok(dashboardCommands.includes("pub async fn get_token_pulse"));
  assert.ok(dashboardCommands.includes("history_days"));
  assert.ok(dashboardModels.includes("pub struct TokenPulseSnapshot"));
  assert.ok(dashboardModels.includes("pub today_tokens: i64"));
  assert.ok(dashboardModels.includes("pub average_daily_tokens: f64"));
  assert.ok(dashboardModels.includes("pub ratio_to_average: Option<f64>"));
  assert.ok(dashboardModels.includes("pub hourly_tokens: Vec<TokenPulseHourlyPoint>"));

  assert.ok(repository.includes("pub async fn token_pulse_snapshot"));
  assert.ok(repository.includes("daily_usage_series"));
  assert.ok(repository.includes("dashboard_summary"));

  assert.ok(dashboardTypes.includes("interface TokenPulseSnapshot"));
  assert.ok(dashboardTypes.includes("interface TokenPulseHourlyPoint"));
  assert.ok(dashboardService.includes("getTokenPulse"));
  assert.ok(dashboardService.includes("get_token_pulse"));
  assert.ok(dashboardService.includes("setTokenPulseDetailHovered"));
  assert.ok(dashboardService.includes("set_token_pulse_detail_hovered"));
  assert.ok(dashboardService.includes("setTokenPulseDragging"));
  assert.ok(dashboardService.includes("set_token_pulse_dragging"));
  assert.ok(defaultCapability.includes('"token-pulse"'));
  assert.ok(defaultCapability.includes('"token-pulse-detail"'));
  assert.ok(defaultCapability.includes("core:window:allow-set-size"));
  assert.ok(defaultCapability.includes("core:window:allow-set-position"));
});

test("token pulse tray and mini window expose menus and avoid fullscreen overlays", () => {
  const trayStatus = readProjectFile("src-tauri/src/tray_status.rs");
  const tauriEntrypoint = readProjectFile("src-tauri/src/lib.rs");
  const dashboardService = readProjectFile("src/services/dashboard.ts");
  const tokenPulseWindow = readProjectFile("src/components/TokenPulseWindow.tsx");
  const defaultCapability = readProjectFile("src-tauri/capabilities/default.json");

  assert.ok(trayStatus.includes("MenuBuilder"));
  assert.ok(trayStatus.includes("MenuItemBuilder"));
  assert.ok(trayStatus.includes("TOKEN_PULSE_MENU_OPEN_MAIN"));
  assert.ok(trayStatus.includes("TOKEN_PULSE_MENU_TOGGLE_VISIBLE"));
  assert.ok(trayStatus.includes("TOKEN_PULSE_MENU_EXIT"));
  assert.ok(trayStatus.includes("TOKEN_PULSE_CONTEXT_OPEN_MAIN"));
  assert.ok(trayStatus.includes("TOKEN_PULSE_CONTEXT_HIDE"));
  assert.ok(trayStatus.includes("TOKEN_PULSE_CONTEXT_LOCK_POSITION"));
  assert.ok(trayStatus.includes("show_menu_on_left_click(false)"));
  assert.ok(trayStatus.includes(".menu(&tray_menu)"));
  assert.ok(trayStatus.includes("pub fn handle_token_pulse_menu_event"));
  assert.ok(tauriEntrypoint.includes(".on_menu_event(|app, event| tray_status::handle_token_pulse_menu_event(app, event))"));
  assert.ok(trayStatus.includes("CheckMenuItemBuilder::with_id(TOKEN_PULSE_MENU_TOGGLE_VISIBLE"));
  assert.ok(trayStatus.includes("token_pulse_window_currently_visible(manager.app_handle())"));
  assert.ok(trayStatus.includes("set_token_pulse_tray_toggle_checked"));
  assert.ok(trayStatus.includes("set_checked(visible)"));
  assert.ok(trayStatus.includes("set_menu(Some(tray_menu))"));
  assert.ok(trayStatus.includes("toggle_token_pulse_window"));
  assert.ok(trayStatus.includes("token_pulse_window_currently_visible(app)"));
  assert.ok(trayStatus.includes("hide_token_pulse_window"));
  assert.ok(trayStatus.includes("window.unminimize()"));
  assert.ok(trayStatus.includes("app.webview_windows()"));
  assert.ok(trayStatus.includes("title == MAIN_WINDOW_TITLE"));
  assert.ok(trayStatus.includes("create_main_window(app)"));
  assert.ok(trayStatus.includes("exit_token_scope"));

  assert.ok(trayStatus.includes("TOKEN_PULSE_LOCKED_SETTING"));
  assert.ok(trayStatus.includes("TokenPulseInteractionState"));
  assert.ok(trayStatus.includes("position_locked"));
  assert.ok(trayStatus.includes("set_token_pulse_position_locked"));
  assert.ok(trayStatus.includes("get_token_pulse_position_locked"));
  assert.ok(trayStatus.includes("show_token_pulse_context_menu"));
  assert.ok(trayStatus.includes("popup_menu"));
  assert.ok(trayStatus.includes(".checked(token_pulse_position_locked(manager.app_handle()))"));
  assert.ok(trayStatus.includes("if token_pulse_position_locked(&app)"));

  assert.ok(trayStatus.includes("spawn_token_pulse_fullscreen_guard"));
  assert.ok(trayStatus.includes("is_probably_fullscreen"));
  assert.ok(trayStatus.includes("hide_for_fullscreen"));
  assert.ok(trayStatus.includes("TOKEN_PULSE_FULLSCREEN_GUARD_SECONDS"));
  assert.ok(trayStatus.includes("was_visible_before_fullscreen"));
  assert.ok(trayStatus.includes("hide_token_pulse_detail_window"));

  assert.ok(tauriEntrypoint.includes("tray_status::show_token_pulse_context_menu"));
  assert.ok(tauriEntrypoint.includes("tray_status::get_token_pulse_position_locked"));
  assert.ok(tauriEntrypoint.includes("tray_status::set_token_pulse_position_locked"));

  assert.ok(dashboardService.includes("showTokenPulseContextMenu"));
  assert.ok(dashboardService.includes("show_token_pulse_context_menu"));
  assert.ok(dashboardService.includes("getTokenPulsePositionLocked"));
  assert.ok(dashboardService.includes("get_token_pulse_position_locked"));
  assert.ok(dashboardService.includes("setTokenPulsePositionLocked"));
  assert.ok(dashboardService.includes("set_token_pulse_position_locked"));

  assert.ok(tokenPulseWindow.includes("onContextMenu"));
  assert.ok(tokenPulseWindow.includes("showTokenPulseContextMenu"));
  assert.ok(tokenPulseWindow.includes("getTokenPulsePositionLocked"));
  assert.ok(tokenPulseWindow.includes("setTokenPulsePositionLocked"));
  assert.ok(tokenPulseWindow.includes("isPositionLocked"));
  assert.ok(tokenPulseWindow.includes("if (isPositionLocked)"));
  assert.ok(tokenPulseWindow.includes("token-pulse-locked"));

  assert.ok(defaultCapability.includes("core:window:allow-show"));
  assert.ok(defaultCapability.includes("core:window:allow-hide"));
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

test("settings page exposes github encrypted sync controls", () => {
  const settingsPage = readProjectFile("src/components/SettingsPage.tsx");
  const githubSyncPanel = readProjectFile("src/components/GitHubSyncPanel.tsx");
  const dashboardService = readProjectFile("src/services/dashboard.ts");
  const dashboardTypes = readProjectFile("src/types/dashboard.ts");
  const tauriEntrypoint = readProjectFile("src-tauri/src/lib.rs");
  const settingsCommands = readProjectFile("src-tauri/src/commands/settings.rs");
  const styles = readProjectFile("src/styles.css");

  assert.ok(settingsPage.includes("GitHubSyncPanel"));
  assert.ok(githubSyncPanel.includes("GitHub 同步"));
  assert.ok(githubSyncPanel.includes("personal access token"));
  assert.ok(githubSyncPanel.includes("同步密码"));
  assert.ok(githubSyncPanel.includes("bootstrap"));
  assert.ok(githubSyncPanel.includes("GitHub 只保存加密文件"));
  assert.ok(githubSyncPanel.includes("立即同步"));
  assert.ok(githubSyncPanel.includes("测试连接"));
  assert.ok(githubSyncPanel.includes("同步仓库向导"));
  assert.ok(githubSyncPanel.includes("此版本不会自动创建仓库"));
  assert.ok(githubSyncPanel.includes("Contents: Read and write"));
  assert.ok(githubSyncPanel.includes("保存并初始化同步仓库"));
  assert.ok(githubSyncPanel.includes("handleInitializeRepository"));
  assert.ok(githubSyncPanel.includes("refreshGitHubSyncSettings"));
  assert.ok(githubSyncPanel.includes("lastMessageLabel"));
  assert.ok(githubSyncPanel.includes("最近结果"));
  assert.ok(githubSyncPanel.includes("window.setInterval"));
  assert.ok(githubSyncPanel.includes("await refreshGitHubSyncSettings({ resetDraft: false })"));
  assert.ok(githubSyncPanel.includes("GitHubSyncRemoteDeviceList"));
  assert.ok(githubSyncPanel.includes("远端设备详情"));
  assert.ok(githubSyncPanel.includes("bootstrap 分片"));
  assert.ok(githubSyncPanel.includes("day 分片"));
  assert.ok(githubSyncPanel.includes("listGitHubSyncRemoteDevices"));

  assert.ok(dashboardService.includes("get_github_sync_settings"));
  assert.ok(dashboardService.includes("save_github_sync_settings"));
  assert.ok(dashboardService.includes("test_github_sync_connection"));
  assert.ok(dashboardService.includes("run_github_sync_once"));
  assert.ok(dashboardService.includes("list_github_sync_remote_devices"));
  assert.ok(dashboardTypes.includes("interface GitHubSyncSettings"));
  assert.ok(dashboardTypes.includes("interface GitHubSyncRunResult"));
  assert.ok(dashboardTypes.includes("interface GitHubSyncRemoteDevice"));
  assert.ok(dashboardTypes.includes("last_message: string | null"));

  assert.ok(tauriEntrypoint.includes("get_github_sync_settings"));
  assert.ok(tauriEntrypoint.includes("save_github_sync_settings"));
  assert.ok(tauriEntrypoint.includes("test_github_sync_connection"));
  assert.ok(tauriEntrypoint.includes("run_github_sync_once"));
  assert.ok(tauriEntrypoint.includes("list_github_sync_remote_devices"));
  assert.ok(settingsCommands.includes("list_github_sync_remote_devices"));
  assert.ok(settingsCommands.includes("github_sync"));
  assert.ok(styles.includes(".github-sync-panel"));
  assert.ok(styles.includes(".github-sync-wizard"));
  assert.ok(styles.includes(".github-sync-form"));
  assert.ok(styles.includes(".github-sync-status"));
  assert.ok(styles.includes(".github-remote-device-list"));
});

test("background sync interval allows one minute for github sync freshness", () => {
  const settingsPage = readProjectFile("src/components/SettingsPage.tsx");
  const dashboardCommand = readProjectFile("src-tauri/src/commands/dashboard.rs");
  const repository = readProjectFile("src-tauri/src/db/repository.rs");
  const dashboardService = readProjectFile("src/services/dashboard.ts");

  assert.ok(settingsPage.includes("const SYNC_INTERVAL_VALUES = [1, 5, 15, 30, 60]"));
  assert.ok(dashboardCommand.includes("input.interval_minutes.clamp(1, 1440)"));
  assert.ok(repository.includes("input.interval_minutes.clamp(1, 1440)"));
  assert.ok(repository.includes(".clamp(1, 1440)"));
  assert.ok(dashboardService.includes("Math.max(1, Math.round(parsed))"));
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
  assert.ok(releaseScript.includes("UTF8Encoding"));
  assert.ok(releaseScript.includes("WriteAllText"));
  assert.ok(releaseScript.includes('RepoFullName = "RickChen764/tokenscope-desktop-public"'));
  assert.ok(releaseScript.includes("releases/download"));
  assert.ok(buildScript.includes("TAURI_SIGNING_PRIVATE_KEY_PATH"));
  assert.ok(buildScript.includes("TAURI_SIGNING_PRIVATE_KEY = Read-Utf8Text -Path $keyPath"));
  assert.ok(buildScript.includes("pnpm exec tauri build --ci"));
  assert.ok(buildScript.includes("tokenscope-desktop.key"));
});

test("application updater keeps explicit persisted state and complete progress", () => {
  const settingsPage = readProjectFile("src/components/SettingsPage.tsx");
  const appShell = readProjectFile("src/app/App.tsx");
  const dashboardService = readProjectFile("src/services/dashboard.ts");
  const dashboardTypes = readProjectFile("src/types/dashboard.ts");
  const styles = readProjectFile("src/styles.css");

  assert.ok(dashboardTypes.includes("type AppUpdateStatus"));
  assert.ok(dashboardTypes.includes("checked_at: string | null"));
  assert.ok(dashboardTypes.includes("error: string | null"));
  assert.ok(dashboardService.includes("APP_UPDATE_STATE_STORAGE_KEY"));
  assert.ok(dashboardService.includes("getStoredAppUpdateInfo"));
  assert.ok(dashboardService.includes("writeStoredAppUpdateInfo"));
  assert.ok(dashboardService.includes('status: update ? "available" : "current"'));
  assert.ok(dashboardService.includes('event.event === "Finished"'));
  assert.ok(dashboardService.includes("writeStoredAppUpdateInfo({"));
  assert.ok(settingsPage.includes("getStoredAppUpdateInfo"));
  assert.ok(settingsPage.includes("updateStatusLabel"));
  assert.ok(settingsPage.includes("updateLastCheckedLabel"));
  assert.ok(settingsPage.includes("updateProgressBytesLabel"));
  assert.ok(settingsPage.includes("formatBytes"));
  assert.ok(settingsPage.includes("setUpdateInfo(getStoredAppUpdateInfo())"));
  assert.ok(appShell.includes("getStoredAppUpdateInfo"));
  assert.ok(appShell.includes("installPendingAppUpdate"));
  assert.ok(appShell.includes("appUpdateInfo.available"));
  assert.ok(appShell.includes("update-status-rail"));
  assert.ok(appShell.includes("update-status-popover"));
  assert.ok(dashboardService.includes("APP_UPDATE_INFO_EVENT"));
  assert.ok(dashboardService.includes("window.dispatchEvent"));
  assert.ok(appShell.includes("addEventListener(APP_UPDATE_INFO_EVENT"));
  assert.ok(styles.includes(".update-status-list"));
  assert.ok(styles.includes(".update-progress-bytes"));
  assert.ok(styles.includes(".update-status-rail"));
  assert.ok(styles.includes(".update-status-popover"));
});

test("release manifest scripts validate versions and updater artifacts before publishing", () => {
  const createLatestJsonScript = readProjectFile("scripts/create-latest-json.ps1");
  const buildReleaseScript = readProjectFile("scripts/build-release.ps1");

  assert.ok(createLatestJsonScript.includes("Get-PackageVersion"));
  assert.ok(createLatestJsonScript.includes("Assert-VersionConsistency"));
  assert.ok(createLatestJsonScript.includes("Assert-ReleaseArtifact"));
  assert.ok(createLatestJsonScript.includes("Get-FileHash"));
  assert.ok(createLatestJsonScript.includes("Installer SHA256"));
  assert.ok(createLatestJsonScript.includes("ConvertFrom-Json"));
  assert.ok(createLatestJsonScript.includes("NotesPath"));
  assert.ok(createLatestJsonScript.includes("ReadAllText"));
  assert.ok(createLatestJsonScript.includes("validatedLatestJsonText"));
  assert.ok(buildReleaseScript.includes("Assert-ReleaseVersionConsistency"));
  assert.ok(buildReleaseScript.includes("package.json"));
  assert.ok(buildReleaseScript.includes("tauri.conf.json"));
});

test("powershell release scripts pin UTF-8 boundaries for Chinese release text", () => {
  const buildReleaseScript = readProjectFile("scripts/build-release.ps1");
  const createLatestJsonScript = readProjectFile("scripts/create-latest-json.ps1");

  for (const script of [buildReleaseScript, createLatestJsonScript]) {
    assert.ok(script.includes("$script:Utf8NoBom = [System.Text.UTF8Encoding]::new($false)"));
    assert.ok(script.includes("$OutputEncoding = $script:Utf8NoBom"));
    assert.ok(script.includes("[Console]::InputEncoding = $script:Utf8NoBom"));
    assert.ok(script.includes("[Console]::OutputEncoding = $script:Utf8NoBom"));
  }

  assert.ok(buildReleaseScript.includes("[string]$NotesPath"));
  assert.ok(buildReleaseScript.includes("@latestJsonArgs"));
  assert.ok(buildReleaseScript.includes("Read-JsonFile"));
  assert.ok(buildReleaseScript.includes("Read-Utf8Text"));

  assert.ok(createLatestJsonScript.includes("function Read-Utf8Text"));
  assert.ok(createLatestJsonScript.includes("function Write-Utf8Text"));
  assert.ok(createLatestJsonScript.includes("function Read-JsonFile"));
  assert.equal(createLatestJsonScript.includes("Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json"), false);
  assert.equal(createLatestJsonScript.includes("[System.IO.File]::WriteAllText($OutputPath"), false);
});

test("application and installer support Chinese and English localization", () => {
  const tauriConfig = JSON.parse(readProjectFile("src-tauri/tauri.conf.json"));
  const i18nModule = readProjectFile("src/i18n/index.tsx");
  const appShell = readProjectFile("src/app/App.tsx");
  const settingsPage = readProjectFile("src/components/SettingsPage.tsx");
  const miniSeriesChart = readProjectFile("src/components/MiniSeriesChart.tsx");

  assert.deepEqual(tauriConfig.bundle.windows.nsis.languages, ["English", "SimpChinese"]);
  assert.equal(tauriConfig.bundle.windows.nsis.displayLanguageSelector, false);
  assert.ok(i18nModule.includes("zh-CN"));
  assert.ok(i18nModule.includes("en-US"));
  assert.ok(i18nModule.includes("navigator.language"));
  assert.ok(i18nModule.includes("localStorage"));
  assert.ok(i18nModule.includes("TokenScopeLanguage"));
  assert.ok(appShell.includes("useI18n"));
  assert.ok(settingsPage.includes("language-select"));
  assert.ok(settingsPage.includes("setLanguage"));
  assert.ok(miniSeriesChart.includes("useI18n"));
});
