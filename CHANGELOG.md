# 变更日志

## 2026-06-08

### 0.1.12 更新包

- 改进今日 Token 常驻小窗交互，修复拖动状态和 hover 详情窗口定位问题。
- 重组设置页布局，优化常驻小窗圆环、详情悬停和 Codex 剩余用量展示。
- 移除小窗小时图日均参考线，减少迷你详情视图中的视觉干扰。
- 增加 GitHub 远端设备重新导入能力，并完善 GitHub 同步运行状态、互斥锁和远端设备详情。
- 版本提升到 `0.1.12`，用于发布新的 Windows NSIS 安装包、Tauri updater 签名文件和 `latest.json`。

## 2026-06-05

### 0.1.7 更新包

- 发布基于公开主干的新 Windows NSIS 更新包，包含自动更新检查、常驻 Token 小窗和托盘交互改进。
- 显式声明 Tauri 主窗口 `label: "main"`，减少托盘恢复主窗口时的隐式 label 风险。
- 清理旧公开同步流程相关测试依赖，公开仓库改为直接以 `master` 作为主干。
- 版本提升到 `0.1.7`，用于发布新的 Windows NSIS 安装包、Tauri updater 签名文件和 `latest.json`。

### 今日 Token 常驻小窗

- 新增桌面常驻的今日 Token 迷你用量表，展示今日用量、历史日均、进度条和趋势方向，方便在主窗口外快速观察实时消耗。
- 新增 hover 详情窗口，展示今日用量占日均比例、近 30 日历史日均、昨日用量、距离日均目标差值和小时用量热力条。
- 迷你用量表支持鼠标拖动，并会保存窗口位置；默认靠近任务栏上方，用户可以自行放置到不遮挡工作的区域。
- 拖动开始时自动隐藏详情窗口，释放、窗口失焦和指针捕获丢失时都会退出拖动状态，避免小窗持续跟随鼠标。
- 修复 hover 详情窗口漂移问题：详情窗口只跟随迷你用量表定位，不再在鼠标进入详情窗口后把自身当作锚点重新定位。
- Tauri 托盘 tooltip 接入今日 Token 摘要，窗口权限、桌面子窗口和前端服务层补齐今日 Token 快照查询。
- 版本提升到 `0.1.6`，用于发布新的 Windows NSIS 安装包、Tauri updater 签名文件和 `latest.json`。

## 2026-06-04

### 更新提示与通知体验

- 左侧导航栏新增应用更新提示位：检测到新版本时显示“升级”入口，hover 后展示目标版本、发布时间和 release notes；最新版本时不占位显示。
- 开发预览支持 `?mockUpdate=1` 模拟新版本状态，便于在本地验证更新提示 UI，正式桌面运行时不使用该模拟数据。
- 将操作成功、失败和警告信息改为右上角 toast/tip，不再占用主内容布局；默认 5 秒淡出，鼠标悬停时保持显示，移开后再淡出。
- 设置页和报表页的操作反馈统一接入 toast 组件，保留页面内部数据状态类错误的 inline 展示。
- 发布脚本支持从 UTF-8 文件读取 release notes，避免中文 changelog 写入 `latest.json` 时出现编码或换行问题。
- 版本提升到 `0.1.5`，用于发布新的 Windows NSIS 安装包、Tauri updater 签名文件和 `latest.json`。

## 2026-06-03

### 产品化报表界面与 0.1.3 安装包

- 将概览、健康、报表、分析、调用、设置和详情页统一为 VS Code 风格的报表式布局，减少浮层卡片、圆角和装饰性视觉噪音。
- 每日用量图表改用 ECharts 渲染，支持堆叠柱状图、折线趋势、图例和 hover 明细。
- Token 数值改为 `K` / `M` / `B` 紧凑单位展示，并在 hover 或 title 中保留精确值。
- 分析页排行列表复用概览的表格化样式，设置页和数据来源列表收敛为更统一的分隔线布局。
- 版本提升到 `0.1.3`，生成新的 Windows NSIS 安装包、Tauri updater 签名文件和 `latest.json`。
- 当前安装包尚未接入 Windows Authenticode 代码签名证书，因此系统安装提示仍会显示“未知发布者”；Tauri updater 的 `.sig` 签名已生成。

## 2026-06-02

### 中英文多语言与安装包语言

- Windows NSIS 安装包新增 `English` 和 `SimpChinese` 语言配置，中文系统自动使用简体中文，其他系统默认英文。
- 主程序新增轻量 i18n 层，启动时按系统语言选择中文或英文，并在设置页提供“界面语言”手动切换。
- 概览、图表、调用明细、数据健康、报表、设置、本地 Agent 来源、自定义数据源、多设备数据包和应用更新等主要界面接入中英文文案。
- service/browser fallback 提示接入 runtime 翻译，浏览器预览环境和桌面运行时错误提示会跟随当前语言。
- 版本提升到 `0.1.2`，用于通过已接入的 updater 发布新的多语言安装包。

### 应用内更新

- 接入 Tauri updater，设置页新增“应用更新”卡片，支持检查新版、查看版本说明、下载并安装更新。
- 配置 GitHub Releases `latest.json` 作为更新端点，Windows 更新安装方式使用被动安装，减少用户手动操作。
- 生成并配置 Tauri updater 公钥，签名私钥保存在本机 `.tauri` 目录，不提交到仓库。
- 新增 `scripts/build-release.ps1` 和 `scripts/create-latest-json.ps1`，用于生成带签名的 NSIS 安装包、签名文件和 updater 元数据。
- 版本提升到 `0.1.1`，该版本作为首个内置 updater 的安装包；旧版用户需要手动安装一次，后续版本可在应用内更新。

### Windows 安装包发布

- Windows release 打包方式从裸 `tokenscope-desktop.exe` 调整为 NSIS 安装包，主发布产物位于 `src-tauri/target/release/bundle/nsis/TokenScope Desktop_0.1.1_x64-setup.exe`。
- Tauri `bundle.active` 改为启用，并将 `bundle.targets` 收敛为 `["nsis"]`，为后续接入 Tauri updater 和签名更新包打基础。
- 新增产品范围测试，确保后续 release 配置不会退回只发布裸 exe 的状态。

## 2026-06-01

### 排行列表溢出修正

- 排行卡片改为固定表格布局，名称列允许省略，Token 数值列保持右对齐且不换行，避免长文本把卡片撑出网格。
- 会话排行对 UUID/长 session id 使用中间省略显示，完整值保留在 hover title 和点击详情参数中。

### 增量同步与全量刷新

- 新增每个本地 Agent 数据源独立的同步 cursor，记录在 `app_setting`，用于后续增量刷新时只处理上次成功同步时间之后的数据。
- Codex、Hermes、opencode、Claude Code importer 支持 `incremental` / `full` 同步模式；首次增量没有 cursor 时仍会完整扫描，成功后推进 cursor。
- 应用启动时默认触发一次增量同步；后台定时同步仍由设置里的开关和间隔控制。
- 设置页新增“全量刷新”入口，用于跳过 cursor 重新扫描本机 Agent 数据，便于增量状态异常时校正。
- 前端本机同步按钮默认调用增量同步，保留后台“立即同步一次”和手动全量刷新两种操作路径。

### Agent 趋势图表与 Codex 聚合

- 概览每日用量图表增加 Agent 分组序列，柱状图按每天各 Agent 的 Token 用量堆叠展示。
- 折线图同时展示总 Token 趋势和 Top Agent 趋势，支持按图例选择显示哪些折线，默认显示全部并可单独隐藏 `其他 Agent`。
- 图表图例、汇总指标和颜色层级升级，保留柱状图/折线图切换与自定义日期区间。
- 概览页将每日用量升级为全宽主趋势区域，放大图表高度和统计指标，排行列表下移为次级分析区。
- Codex importer 将内部 `agent_role`、`agent_nickname` 统一归集为 `codex` / `Codex`，避免把 Codex 内部子 Agent 拆成多个 Agent。
- Codex 重新同步时会比较并刷新旧记录的 Agent 标识，已导入过的 `worker`、`explorer`、`default` 等旧标签会被回写为 `codex`。
- 同步本机数据时会清理 `demo_seed` 演示记录，避免 `researcher`、`coder`、`analyst` 等演示 Agent 混入真实统计。

### 封存成本展示与定价入口

- 从左侧导航和当前产品界面移除成本规则页，不再暴露定价规则新增、导入、删除和历史成本重算入口。
- 从概览卡片、每日用量图表、排行列表、调用明细、维度详情、本地 Agent 来源、自定义 importer、多设备数据包、报表导出和设置页移除费用展示。
- 数据健康检查不再把缺少成本或缺少定价规则作为问题项，只保留模型、Token 和失败调用等统计质量问题。
- CSV 导出不再包含 `estimated_cost_usd`、`cost_currency`、`provider_reported_cost_usd`、`reconciled_cost_usd` 和 `cost_source` 字段。
- Dashboard Top 维度和最高 Agent/模型改为按 Token 与调用量排序，不再使用成本作为排序依据。
- 保留数据库成本字段、导入器成本写入和 `.tokenscope` 数据包兼容字段，避免破坏已有数据，后续如需恢复成本模块可以继续迁移。

### 成本币种与人民币结算

- 新增 `0006_cost_currency` migration，给 `pricing_rule` 增加 `currency`，给 `llm_call`、`daily_usage_agg`、`external_dataset` 增加 `cost_currency`。
- 定价规则支持选择 `USD` 美元或 `CNY` 人民币；本地规则、JSON 导入和预置规则导入都会保存币种，未声明币种的旧规则默认按 `USD` 处理。
- 历史成本重算会把命中的定价规则币种写回调用记录，缺少定价规则的记录继续默认 `USD`。
- Dashboard、每日用量、Top 维度、调用明细、数据健康、本地 Agent 来源、自定义 importer 和多设备数据包列表会按记录币种格式化成本；混合币种汇总显示为 `多币种`。
- `.tokenscope` 设备数据包升级到 v2，导出调用成本币种；导入仍兼容 v1 数据包并默认补齐 `USD`。
- 自定义 SQLite importer 的字段映射新增可选 `cost_currency`，没有映射时默认 `USD`。

### 初始化项目版本管理

- 初始化 TokenScope Desktop 的 Git 版本管理。
- 建立中文变更日志，用于记录主要功能、数据结构、导入器、界面和构建流程的修改内容。

### TokenScope Desktop 基础能力

- 搭建 Tauri v2 + React + TypeScript + Rust + SQLite 桌面应用骨架。
- 新增 SQLite migration 管理，建立 `provider_config`、`llm_call`、`pricing_rule`、`daily_usage_agg`、`agent_import_map`、`app_setting` 等基础表。
- 新增 Rust repository layer，用于写入调用记录、读取 dashboard summary、每日用量、Top 维度和调用明细。
- 默认不保存 prompt/response 明文；CSV 导出也排除 raw response 和敏感字段。

### 本地 Agent 数据统计

- 实现 Codex 数据读取和导入，支持从本机 Codex 记录中提取 token/cost 元数据。
- 实现 Hermes、opencode、Claude Code importer，按本地可用数据源检测后导入统计记录。
- 新增本地 Agent 检测、同步状态、错误状态和来源统计，避免依赖 message 文本判断同步失败。
- 新增自定义 SQLite importer 框架，支持用户为其他 Agent 配置只读 SQL 和字段映射。

### 成本规则与定价

- 新增本地定价规则管理、预置规则导入、JSON 导入和历史成本重算。
- 支持编辑定价规则生效区间。
- 官方价格维护策略保持本地预置/手动导入，不联网获取。

### 数据健康、报表和调用明细

- 新增数据健康页，统计缺少价格、缺少 usage、异常状态等问题。
- 新增调用明细页，支持筛选、分页和 CSV 导出。
- 新增报表导出页，用于导出本地已统计的调用元数据。

### Dashboard 和界面优化

- 将界面中文化，保留 Agent、Provider、Token 等技术术语。
- 优化概览页视觉层级、导航轨道、卡片、排行榜和设置页布局。
- 每日用量图表新增柱状图/折线图切换。
- 概览支持今日、近 7 天、近 30 天、近 90 天和自定义日期区间。
- 调用明细、维度详情和报表页同步支持近 90 天。

### 多设备数据合并

- 新增 `.tokenscope` 设备数据包格式，用于离线导出/导入其他电脑的统计数据。
- 新增 `external_dataset` 表，并给 `llm_call` 增加 `origin_dataset_id`，区分本机数据和外部设备数据。
- 导入其他设备数据时按设备数据集隔离，重复导入同一设备会刷新该设备数据。
- 移除外部设备数据时只删除对应 `origin_dataset_id` 的记录，不影响本机数据。
- 导出 `.tokenscope` 包时不包含 prompt/response 明文。
- 导出流程改为系统目录选择器，导入流程改为系统文件选择器并筛选 `.tokenscope` 文件。
- 保留“打开导出文件夹”作为辅助操作。

### 构建和验证

- 增加 Node 产品级测试，覆盖功能范围、隐私约束、导入器、定价规则、多设备数据包等行为。
- 增加 Rust 单元测试，覆盖 repository、importer、pricing、usage normalization 和设备数据包导入导出。
- 已验证 `pnpm test`、`pnpm lint`、`pnpm build`、`cargo test`、`pnpm tauri build` 可通过。
