import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";

export type AppLanguage = "zh-CN" | "en-US";

const LANGUAGE_STORAGE_KEY = "TokenScopeLanguage";

const englishMessages: Record<string, string> = {
  数字显示: "Number Display",
  "控制 Token 数值在概览、图表和排行中的展示方式。":
    "Control how Token values are displayed in overview, charts, and rankings.",
  缩略显示: "Compact",
  完整显示: "Full",
  "数字显示已切换为缩略显示。": "Number display changed to compact.",
  "数字显示已切换为完整显示。": "Number display changed to full.",
  常驻小窗: "Resident Window",
  "控制常驻小窗是否展示 Codex 剩余用量；默认关闭，避免占用小窗空间。":
    "Control whether the resident window shows Codex remaining usage. It is off by default to keep the window compact.",
  "显示 Codex 剩余用量": "Show Codex Remaining Usage",
  "默认关闭。开启后常驻小窗会增加 Codex 5 小时和 1 周剩余额度；关闭时布局自动收缩。":
    "Off by default. When enabled, the resident window adds Codex 5-hour and 1-week remaining usage; when disabled, the layout shrinks automatically.",
  今日: "Today",
  "近 7 天": "Last 7 days",
  "近 30 天": "Last 30 days",
  "近 90 天": "Last 90 days",
  自定义: "Custom",
  概览: "Overview",
  健康: "Health",
  报表: "Reports",
  分析: "Analysis",
  调用: "Calls",
  设置: "Settings",
  用量仪表盘: "Usage Dashboard",
  数据健康: "Data Health",
  报表导出: "Report Export",
  维度分析: "Dimension Analysis",
  调用明细: "Call Details",
  偏好设置: "Preferences",
  数据同步: "Data Sync",
  "管理本机 Agent 来源、增量同步、后台同步和必要时的全量刷新。":
    "Manage local Agent sources, incremental sync, background sync, and full refresh when needed.",
  设置分类: "Settings categories",
  同步概览: "Sync Overview",
  "查看同步状态和常用操作。": "Review sync status and common actions.",
  数据来源: "Data Sources",
  "管理本机 Agent 和自定义 SQLite 来源。":
    "Manage local Agent and custom SQLite sources.",
  多设备: "Multi-device",
  "配置 GitHub 同步和设备数据包。":
    "Configure GitHub sync and device packages.",
  应用: "App",
  "数字显示、语言和更新。": "Number display, language, and updates.",
  高级: "Advanced",
  "低频维护和统计边界。": "Occasional maintenance and statistics boundaries.",
  "把同步状态、常用动作和后续配置入口集中在一屏。":
    "Keep sync status, common actions, and configuration entry points on one screen.",
  设置摘要: "Settings summary",
  本地来源: "Local Sources",
  "已检测 / 总来源": "Detected / total sources",
  已导入调用: "Imported Calls",
  "最近导入 {time}": "Last imported {time}",
  后台同步: "Background Sync",
  已启用: "Enabled",
  最近问题: "Recent Issue",
  同步和更新状态: "Sync and update status",
  "本机 Agent 同步": "Local Agent Sync",
  "查看本机来源检测和导入状态，常用同步动作可直接执行。":
    "Review local source detection and import status, with common sync actions available directly.",
  管理数据来源: "Manage Data Sources",
  多设备同步: "Multi-device Sync",
  "GitHub 加密同步和设备数据包都归入多设备管理，避免配置散落。":
    "GitHub encrypted sync and device packages are grouped under multi-device management.",
  同步通道: "Sync Channel",
  操作状态: "Action State",
  更新处理中: "Update in progress",
  可配置: "Configurable",
  设备数据包: "Device Packages",
  独立管理: "Managed separately",
  管理多设备: "Manage Multi-device",
  快捷入口: "Quick links",
  "管理 Agent 来源和自定义 SQLite。": "Manage Agent sources and custom SQLite.",
  "配置 GitHub 同步和导入设备包。":
    "Configure GitHub sync and import device packages.",
  打开应用设置: "Open App Settings",
  "演示数据、全量刷新和导出。": "Demo data, full refresh, and export.",
  "全量刷新和导出。": "Full refresh and export.",
  "管理本机 Agent 来源和只读 SQLite 自定义数据源。":
    "Manage local Agent sources and read-only custom SQLite sources.",
  "配置 GitHub 加密同步，或导入其他设备导出的本地数据包。":
    "Configure GitHub encrypted sync or import local packages exported from other devices.",
  "管理语言、更新和本地显示偏好。":
    "Manage language, updates, and local display preferences.",
  "集中放置低频维护、全量刷新、导出和统计边界说明。":
    "Group occasional maintenance, full refresh, export, and statistics boundary notes.",
  "执行低频维护动作：生成演示数据、全量刷新本机来源或导出 CSV。":
    "Run occasional maintenance actions: generate demo data, fully refresh local sources, or export CSV.",
  "全量刷新会跳过增量游标重新扫描本机来源；不会删除源端已不存在的历史记录。":
    "Full refresh skips the incremental cursor and rescans local sources; it does not delete historical rows that no longer exist upstream.",
  数据迁移与扩展: "Portability & Extensions",
  "导入其他设备数据，或配置只读 SQLite 自定义数据源。":
    "Import data from other devices or configure read-only SQLite custom sources.",
  应用设置: "Application Settings",
  "管理语言、更新和本地统计边界。":
    "Manage language, updates, and local statistics boundaries.",
  "\u53ef\u66f4\u65b0": "Update Available",
  "\u4e0b\u8f7d\u4e2d...": "Downloading...",
  "\u5b89\u88c5\u4e2d...": "Installing...",
  "\u68c0\u67e5\u5931\u8d25": "Check Failed",
  "\u9884\u89c8\u73af\u5883": "Preview Environment",
  "\u5df2\u662f\u6700\u65b0\u7248\u672c": "Already Latest",
  "\u53ef\u7528\u7248\u672c": "Available Version",
  "\u6700\u540e\u68c0\u67e5": "Last Check",
  维度详情: "Dimension Details",
  主导航: "Main navigation",
  "同步中...": "Syncing...",
  同步本机数据: "Sync Local Data",
  同步状态: "Sync Status",
  最后同步: "Last Sync",
  本机数据: "Local Data",
  未同步: "Not Synced",
  尚未同步: "Not synced yet",
  刚刚: "Just now",
  "{count} 分钟前": "{count} min ago",
  "{count} 小时前": "{count} hr ago",
  "{count} 天前": "{count} days ago",
  日期范围: "Date range",
  自定义日期范围: "Custom date range",
  开始: "Start",
  结束: "End",
  每日用量趋势: "Daily usage trend",
  排行分析: "Ranking analysis",
  排名: "Rank",
  "Token 用量": "Token Usage",
  进入分析: "Open Analysis",
  "Agent 排行": "Agent Ranking",
  模型排行: "Model Ranking",
  "Provider 排行": "Provider Ranking",
  工作流排行: "Workflow Ranking",
  项目排行: "Project Ranking",
  会话排行: "Session Ranking",
  "Token 总量": "Total Tokens",
  调用次数: "Calls",
  错误率: "Error Rate",
  平均延迟: "Average Latency",
  缓存输入: "Cached Input",
  "最高 Agent": "Top Agent",
  最高模型: "Top Model",
  "加载中...": "Loading...",
  无: "None",
  暂无数据: "No data",
  暂无调用记录: "No calls",
  未知: "Unknown",
  "未知 Agent": "Unknown Agent",
  未知模型: "Unknown model",
  未标注来源: "Unlabeled source",
  成功: "Success",
  失败: "Failed",
  趋势分析: "Trend Analysis",
  每日用量: "Daily Usage",
  "按本地日期汇总，柱状图展示每日 Agent 构成，折线图展示总量和 Agent 趋势。":
    "Aggregated by local date. Bars show the daily Agent mix; lines show total and Agent trends.",
  每日用量图表形式: "Daily usage chart type",
  柱状: "Bars",
  折线: "Lines",
  "区间 Token": "Range Tokens",
  峰值日: "Peak Day",
  "日均 Token": "Avg Daily Tokens",
  "活跃 Agent": "Active Agents",
  天数: "Days",
  最高: "Peak",
  折线显示选择: "Line series selection",
  "Agent 图例": "Agent legend",
  全部: "All",
  "总 Token": "Total Tokens",
  总量: "Total",
  其他: "Other",
  "其他 Agent": "Other Agents",
  峰值: "Peak",
  "每日 Token 用量柱状图": "Daily token usage bar chart",
  "每日 Token 用量折线图": "Daily token usage line chart",
  "本地 Agent 检测": "Local Agent Detection",
  "检测本机可读取的 Agent 来源路径，并展示已导入到 TokenScope 的同步状态。":
    "Detect readable local Agent source paths and show their TokenScope import status.",
  "检测中...": "Detecting...",
  重新检测: "Detect Again",
  手动同步: "Manual Sync",
  "本地 Agent 检测概览": "Local Agent detection overview",
  检测结果: "Detected",
  可同步来源: "Syncable Sources",
  导入量: "Imported",
  最近导入: "Last Imported",
  最近调用: "Last Call",
  "正在读取本机 Agent 来源...": "Reading local Agent sources...",
  "暂无本地 Agent 来源": "No local Agent sources",
  未找到: "Missing",
  暂不支持导入: "Import Unsupported",
  已同步: "Synced",
  可导入: "Ready",
  来源路径: "Source Path",
  未发现本地数据库路径: "No local database path found",
  导入统计: "Import stats",
  筛选结果: "Filtered Results",
  "按时间、来源和状态查看本地记录的调用元数据。":
    "View locally recorded call metadata by time, source, and status.",
  "导出中...": "Exporting...",
  "导出当前筛选 CSV": "Export Filtered CSV",
  重置筛选: "Reset Filters",
  时间: "Time",
  调用日期范围: "Call date range",
  模型: "Model",
  状态: "Status",
  当前筛选条件下暂无调用记录: "No calls match the current filters",
  每页: "Per page",
  上一页: "Previous",
  下一页: "Next",
  开始时间: "Started At",
  工作流: "Workflow",
  延迟: "Latency",
  自定义数据源: "Custom Sources",
  "用只读 SQLite 查询接入其他 Agent。默认只保存统计字段，不写入 prompt/response 明文。":
    "Connect other Agents with read-only SQLite queries. Only statistics fields are stored by default; prompt/response plaintext is not written.",
  新建配置: "New Profile",
  "正在读取自定义数据源...": "Reading custom sources...",
  暂无自定义数据源配置: "No custom source profiles",
  已停用: "Disabled",
  最近失败: "Recently Failed",
  可同步: "Ready",
  空: "Empty",
  条: "rows",
  编辑: "Edit",
  同步: "Sync",
  删除: "Delete",
  名称: "Name",
  启用这个数据源: "Enable this source",
  "SQLite 数据库路径": "SQLite database path",
  "只读 SELECT 查询": "Read-only SELECT query",
  "字段映射 JSON": "Field mapping JSON",
  "预览中...": "Previewing...",
  预览查询: "Preview Query",
  "保存中...": "Saving...",
  保存配置: "Save Profile",
  预览结果: "Preview Results",
  "行，显示": "rows, showing",
  列: "columns",
  数据健康检查: "Data Health Check",
  "检查本地调用记录是否存在缺少模型、缺少 Token 和失败调用等问题。":
    "Check local call records for missing models, missing tokens, failed calls, and related issues.",
  "刷新中...": "Refreshing...",
  刷新状态: "Refresh Status",
  问题分布: "Issue Distribution",
  当前状态: "Current Status",
  未发现问题: "No issues found",
  调用记录: "Call Records",
  问题调用: "Issue Calls",
  健康率: "Health Rate",
  健康问题: "Health Issues",
  暂无需要处理的数据健康问题: "No data health issues need attention",
  失败调用: "Failed Call",
  缺少模型: "Missing Model",
  "缺少 Token": "Missing Tokens",
  "调用状态不是 success，可能需要单独排查失败率。":
    "Call status is not success; the failure rate may need separate investigation.",
  "记录没有可用模型名，模型维度分析会缺失。":
    "The record has no usable model name, so model dimension analysis will be incomplete.",
  "记录没有有效 Token 数，Token 报表会被低估。":
    "The record has no valid token count, so token reports may be underestimated.",
  多设备数据包: "Multi-device Packages",
  "用 .tokenscope 数据包合并其他电脑的统计数据；导入、更新和移除都不会影响本机数据。":
    "Merge statistics from other computers with .tokenscope packages. Import, update, and removal do not affect local data.",
  "打开中...": "Opening...",
  打开导出文件夹: "Open Export Folder",
  导出本机数据包: "Export Local Package",
  导入设备数据包: "Import Device Package",
  "从其他电脑导出的 .tokenscope 文件中选择一个导入，重复导入同一设备会刷新该设备数据。":
    "Choose a .tokenscope file exported from another computer. Re-importing the same device refreshes that device dataset.",
  "导入中...": "Importing...",
  选择并导入: "Choose and Import",
  "正在读取设备数据...": "Reading device data...",
  "还没有导入其他设备的数据。": "No external device data has been imported.",
  最近更新: "Updated",
  来源: "Source",
  次调用: "calls",
  "移除中...": "Removing...",
  移除: "Remove",
  选择导出目录: "Choose export directory",
  "TokenScope 数据包": "TokenScope package",
  "选择 .tokenscope 数据包": "Choose .tokenscope package",
  "确认移除 {device} 的导入数据？这不会影响本机数据。":
    "Remove imported data for {device}? This will not affect local data.",
  返回分析: "Back to Analysis",
  详情: "Details",
  详情日期范围: "Detail date range",
  维度每日用量: "Dimension Daily Usage",
  关联指标: "Related Metrics",
  "输入 Token": "Input Tokens",
  "输出 Token": "Output Tokens",
  "成功 / 失败": "Success / Failed",
  相关调用: "Related Calls",
  当前维度和时间范围下暂无调用记录:
    "No calls for this dimension and time range",
  "按维度检查 Token 和调用质量": "Inspect Tokens and Call Quality by Dimension",
  "从 Agent、模型、Provider、工作流、项目或会话排行进入详情，查看单一维度的趋势、关键指标和相关调用。":
    "Open details from Agent, model, Provider, workflow, project, or session rankings to inspect trends, key metrics, and related calls for one dimension.",
  项目: "Project",
  会话: "Session",
  报表日期范围: "Report date range",
  "导出 CSV": "Export CSV",
  导出内容: "Export Content",
  "导出本地已统计的调用元数据、Token 和状态，用于审计或进一步分析。":
    "Export locally aggregated call metadata, tokens, and status for audit or further analysis.",
  时间范围: "Time Range",
  至: "to",
  字段: "Fields",
  "调用元数据、Token、状态": "Call metadata, tokens, status",
  隐私边界: "Privacy Boundary",
  "导出面向统计分析，不包含明文 prompt、response 或 Authorization。":
    "Exports are for statistical analysis and do not include prompt/response plaintext or Authorization.",
  应用更新: "App Update",
  "通过 GitHub Releases 检查签名更新包。下载并安装时，Windows 可能会自动关闭当前应用。":
    "Check signed update packages from GitHub Releases. Windows may close the app while installing.",
  "检查中...": "Checking...",
  升级: "Upgrade",
  检查更新: "Check for Updates",
  更新状态: "Update Status",
  发布时间: "Published At",
  下载进度: "Download Progress",
  "下载并安装中...": "Downloading and Installing...",
  下载并安装: "Download and Install",
  "GitHub 同步中": "GitHub Syncing",
  同步已锁定: "Sync Locked",
  "GitHub 同步正在执行...": "GitHub sync is running...",
  "应用更新正在处理，GitHub 同步操作已暂时锁定。":
    "An app update is in progress. GitHub sync actions are temporarily locked.",
  "已耗时 {elapsed}": "Elapsed {elapsed}",
  "GitHub 同步进度": "GitHub sync progress",
  同步模式: "Sync Mode",
  模式: "Mode",
  "统计同步（推荐）": "Statistics sync (recommended)",
  "明细同步（兼容旧行为）": "Detail sync (legacy behavior)",
  "统计同步（V3）": "Stats sync (V3)",
  "明细同步（V2）": "Detail sync (V2)",
  "上传 {count} 个分片": "Uploaded {count} shards",
  "下载 {count} 个分片": "Downloaded {count} shards",
  "导入 {count} 条记录": "Imported {count} records",
  "跳过 {count} 条记录": "Skipped {count} records",
  后台自动同步: "Background Auto Sync",
  "按固定间隔自动同步本机 Agent 来源，也可以手动触发一次后台同步。":
    "Automatically sync local Agent sources on a fixed interval, or trigger a background sync manually.",
  立即同步一次: "Sync Once Now",
  启用后台自动同步: "Enable background auto sync",
  同步间隔: "Sync Interval",
  启动后立即同步: "Sync on Startup",
  最近自动同步: "Last Auto Sync",
  下一次计划: "Next Scheduled",
  最近结果: "Last Result",
  最近错误: "Last Error",
  未启用: "Disabled",
  尚未执行: "Not run yet",
  保存同步设置: "Save Sync Settings",
  数据维护: "Data Maintenance",
  "手动同步本机数据后，可在上方查看来源路径、最近导入、最近调用和导入量。":
    "After syncing local data manually, review source path, last import, last call, and import counts above.",
  "处理中...": "Processing...",
  生成演示数据: "Generate Demo Data",
  全量刷新: "Full Refresh",
  统计数据范围: "Statistics Scope",
  "当前只读取本机已有记录和导入后的统计元数据，不保存 prompt、response 或 Authorization。":
    "The app only reads existing local records and imported statistical metadata. It does not store prompt, response, or Authorization.",
  默认采集方式: "Default Collection",
  本机数据库读取: "Local database read",
  明文内容: "Plaintext Content",
  不保存: "Not stored",
  界面语言: "Interface Language",
  "跟随系统语言，中文系统默认中文，其他语言默认英文。":
    "Follows the system language: Chinese systems use Chinese; all other languages use English.",
  中文: "Chinese",
  English: "English",
  "界面语言已切换为中文。": "Interface language changed to Chinese.",
  尚未检查: "Not checked yet",
  当前版本: "current version",
  当前已是最新版本: "Already up to date",
  查看: "View",
  "读取中...": "Reading...",
  分钟: "min",
  "请选择完整日期区间，且起始日期不能晚于结束日期。":
    "Choose a complete date range. The start date cannot be later than the end date.",
  "加载仪表盘失败：{error}": "Failed to load dashboard: {error}",
  "，清理演示数据 {count} 条": ", cleared {count} demo rows",
  "本机数据已同步：写入 {imported} 条，跳过 {skipped} 条{cleanupText}。":
    "Local data synced: wrote {imported} rows, skipped {skipped} rows{cleanupText}.",
  "同步本机数据失败：{error}": "Failed to sync local data: {error}",
  "演示数据已生成，仪表盘已刷新。":
    "Demo data generated and the dashboard refreshed.",
  "生成演示数据失败：{error}": "Failed to generate demo data: {error}",
  "演示数据只在开发模式可用。":
    "Demo data is only available in development mode.",
  "读取本机 Agent 来源失败：{error}":
    "Failed to read local Agent sources: {error}",
  "读取后台自动同步设置失败：{error}":
    "Failed to read background sync settings: {error}",
  "本地 Agent 检测完成：发现 {detectedCount} 个来源，其中 {syncableCount} 个可同步。":
    "Local Agent detection finished: found {detectedCount} sources, {syncableCount} syncable.",
  "检测失败：{error}": "Detection failed: {error}",
  "同步失败：已写入 {imported} 条，跳过 {skipped} 条。{errors}":
    "Sync failed: wrote {imported} rows, skipped {skipped} rows. {errors}",
  "同步完成：写入 {imported} 条，跳过 {skipped} 条。":
    "Sync complete: wrote {imported} rows, skipped {skipped} rows.",
  "同步完成：写入 {imported} 条，跳过 {skipped} 条{cleanupText}。":
    "Sync complete: wrote {imported} rows, skipped {skipped} rows{cleanupText}.",
  "同步失败：{error}": "Sync failed: {error}",
  "CSV 已导出：{path}": "CSV exported: {path}",
  "导出 CSV 失败：{error}": "Failed to export CSV: {error}",
  "全量刷新失败：已写入 {imported} 条，跳过 {skipped} 条。{errors}":
    "Full refresh failed: wrote {imported} rows, skipped {skipped} rows. {errors}",
  "全量刷新完成：写入 {imported} 条，跳过 {skipped} 条。":
    "Full refresh complete: wrote {imported} rows, skipped {skipped} rows.",
  "全量刷新完成：写入 {imported} 条，跳过 {skipped} 条{cleanupText}。":
    "Full refresh complete: wrote {imported} rows, skipped {skipped} rows{cleanupText}.",
  "全量刷新失败：{error}": "Full refresh failed: {error}",
  "后台自动同步设置已保存。": "Background sync settings saved.",
  "保存后台自动同步设置失败：{error}":
    "Failed to save background sync settings: {error}",
  "已触发一次后台同步。": "Background sync has been triggered once.",
  "触发后台同步失败：{error}": "Failed to trigger background sync: {error}",
  "发现新版本，可以下载并安装。":
    "A new version is available. You can download and install it.",
  "发现新版本 {version}，可以下载并安装。":
    "Found version {version}. You can download and install it.",
  "当前已经是最新版本。": "You are already on the latest version.",
  "检查更新失败：{error}": "Failed to check for updates: {error}",
  "更新安装程序已启动。Windows 会在安装更新时自动关闭当前应用。":
    "The update installer has started. Windows may close the current app while installing.",
  "安装更新失败：{error}": "Failed to install update: {error}",
  "读取自定义数据源失败：{error}": "Failed to read custom sources: {error}",
  "预览成功：读取 {count} 行样例。":
    "Preview succeeded: read {count} sample rows.",
  "预览失败：{error}": "Preview failed: {error}",
  "自定义数据源配置已保存。": "Custom source profile saved.",
  "保存失败：{error}": "Save failed: {error}",
  "自定义数据源同步完成：写入 {imported} 条，跳过 {skipped} 条。":
    "Custom source sync complete: wrote {imported} rows, skipped {skipped} rows.",
  "自定义数据源同步失败：{error}": "Custom source sync failed: {error}",
  "自定义数据源配置已删除。": "Custom source profile deleted.",
  "自定义数据源配置和已导入数据已删除。":
    "Custom source profile and imported data deleted.",
  "删除失败：{error}": "Delete failed: {error}",
  "读取设备数据失败：{error}": "Failed to read device data: {error}",
  ".tokenscope 数据包已导出：{path}": ".tokenscope package exported: {path}",
  "导出本机数据包失败：{error}": "Failed to export local package: {error}",
  "已打开导出文件夹：{path}": "Opened export folder: {path}",
  "打开导出文件夹失败：{error}": "Failed to open export folder: {error}",
  "导入设备数据包完成：{device} 写入 {imported} 条，跳过 {skipped} 条。":
    "Device package imported for {device}: wrote {imported} rows, skipped {skipped} rows.",
  "导入设备数据包失败：{error}": "Failed to import device package: {error}",
  "已移除 {device} 的导入数据：{removed} 条；不会影响本机数据。":
    "Removed imported data for {device}: {removed} rows. Local data is unaffected.",
  "移除设备数据失败：{error}": "Failed to remove device data: {error}",
  "加载维度详情失败：{error}": "Failed to load dimension details: {error}",
  "加载数据健康状态失败：{error}": "Failed to load data health status: {error}",
  "加载调用明细失败：{error}": "Failed to load call details: {error}",
  "加载筛选项失败：{error}": "Failed to load filter options: {error}",
  "导出当前筛选 CSV 失败：{error}": "Failed to export filtered CSV: {error}",
  "导出报表失败：{error}": "Failed to export report: {error}",
  "需要在 Tauri 桌面运行时中检测。": "Requires the Tauri desktop runtime.",
  "浏览器预览环境未启用后台同步。":
    "Background sync is not enabled in the browser preview environment.",
  "浏览器预览环境已跳过后台同步。":
    "Background sync was skipped in the browser preview environment.",
  "浏览器预览环境无法检查应用更新。":
    "The browser preview environment cannot check app updates.",
  "{action}需要在 Tauri 桌面运行时中执行。":
    "{action} must run in the Tauri desktop runtime.",
  "没有可安装的待处理更新，请先检查更新。":
    "No pending update is available to install. Check for updates first.",
  移除设备数据: "Remove device data",
  保存自定义数据源: "Save custom source",
  删除自定义数据源: "Delete custom source",
  预览自定义数据源: "Preview custom source",
  同步自定义数据源: "Sync custom source",
  安装应用更新: "Install app update",
  清理演示数据: "Clear demo data",
  "导入 Codex 数据": "Import Codex data",
  "导入本机 Agent 数据": "Import local Agent data",
};

interface I18nContextValue {
  language: AppLanguage;
  numberLocale: string;
  setLanguage: (language: AppLanguage) => void;
  t: (message: string, params?: Record<string, string | number>) => string;
}

const I18nContext = createContext<I18nContextValue | null>(null);

function normalizeLanguage(value: string | null | undefined): AppLanguage {
  return value?.toLowerCase().startsWith("zh") ? "zh-CN" : "en-US";
}

function readInitialLanguage(): AppLanguage {
  const stored = window.localStorage.getItem(LANGUAGE_STORAGE_KEY);
  if (stored === "zh-CN" || stored === "en-US") {
    return stored;
  }

  return normalizeLanguage(window.navigator.language);
}

function readRuntimeLanguage(): AppLanguage {
  const stored = window.localStorage.getItem(LANGUAGE_STORAGE_KEY);
  if (stored === "zh-CN" || stored === "en-US") {
    return stored;
  }

  return normalizeLanguage(window.navigator.language);
}

function interpolate(
  message: string,
  params?: Record<string, string | number>,
) {
  if (!params) {
    return message;
  }

  return Object.entries(params).reduce(
    (current, [key, value]) => current.split(`{${key}}`).join(String(value)),
    message,
  );
}

export function LocaleProvider({ children }: { children: ReactNode }) {
  const [language, setLanguageState] = useState<AppLanguage>(() =>
    readInitialLanguage(),
  );

  useEffect(() => {
    document.documentElement.lang = language === "zh-CN" ? "zh-CN" : "en";
    window.localStorage.setItem(LANGUAGE_STORAGE_KEY, language);
  }, [language]);

  const setLanguage = useCallback((nextLanguage: AppLanguage) => {
    setLanguageState(nextLanguage);
  }, []);

  const t = useCallback(
    (message: string, params?: Record<string, string | number>) => {
      const translated =
        language === "en-US" ? (englishMessages[message] ?? message) : message;
      return interpolate(translated, params);
    },
    [language],
  );

  const value = useMemo<I18nContextValue>(
    () => ({
      language,
      numberLocale: language === "zh-CN" ? "zh-CN" : "en-US",
      setLanguage,
      t,
    }),
    [language, setLanguage, t],
  );

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n() {
  const context = useContext(I18nContext);
  if (!context) {
    throw new Error("useI18n must be used inside LocaleProvider");
  }

  return context;
}

export function translateRuntime(
  message: string,
  params?: Record<string, string | number>,
) {
  const language = readRuntimeLanguage();
  const translated =
    language === "en-US" ? (englishMessages[message] ?? message) : message;
  return interpolate(translated, params);
}
