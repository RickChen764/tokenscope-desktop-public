# TokenScope Desktop

中文 | [English](README.en.md)

TokenScope Desktop 是一个本地优先的 LLM Token 用量统计桌面应用。它不实现真实代理，不调用外部 LLM API，而是读取本机已经存在的 Agent 使用记录，统一导入到本地 SQLite 数据库后做统计分析。

项目基于 Tauri v2、React、TypeScript、Rust 和 SQLite 构建。

## 当前能力

- 本地 SQLite 数据库和 migration 管理。
- Dashboard 汇总：Token 总量、调用次数、错误率、平均延迟、缓存输入、最高 Agent、最高模型。
- 每日用量图表：支持柱状图和折线图，展示总 Token 趋势和各 Agent 趋势。
- 自定义时间范围：今日、近 7 天、近 30 天、近 90 天、自定义日期区间。
- 多维排行和详情：Agent、模型、Provider、工作流、项目、会话。
- 调用明细：本地筛选、分页、状态和用量查看。
- 数据健康检查：缺失模型、缺失 Token、异常状态等统计质量问题。
- 报表导出：导出本地调用元数据 CSV。
- 多设备数据合并：支持 `.tokenscope` 数据包导出/导入，外部设备数据可移除，不破坏本机数据。
- 后台同步：支持启动后增量同步、定时同步、手动增量同步和全量刷新。
- 自定义 SQLite importer：可为未内置支持的 Agent 配置只读 SQL 和字段映射。

## 已支持的数据源

- Codex：读取本机 Codex state 和 rollout token count 数据。
- Hermes：读取本机 Hermes state 数据库。
- opencode：读取本机 opencode SQLite 数据库。
- Claude Code：读取本机 Claude Code transcript JSONL 记录。
- 自定义 SQLite：通过用户配置扩展其他本地 Agent 数据源。

更多 Agent 数据源可以继续通过 importer 框架扩展。

## 隐私边界

- 默认不保存 prompt/response 明文。
- CSV 和 `.tokenscope` 数据包不会导出原始 prompt/response 内容。
- 不读取或保存 Authorization、API key 等鉴权材料。
- 不联网抓取官方价格，也不调用任何外部 LLM API。
- 当前产品界面已封存成本展示和定价规则入口，主要关注 Token 和调用统计。

## 环境要求

- Node.js 24+
- pnpm 11+
- Rust toolchain，包含 `cargo` 和 `rustc`
- Windows、macOS 或 Linux 桌面环境

## 安装依赖

```bash
pnpm install
```

## 开发启动

仅启动前端预览：

```bash
pnpm dev
```

启动 Tauri 桌面客户端：

```bash
pnpm tauri dev
```

默认前端开发地址为：

```text
http://127.0.0.1:1420/
```

## 构建

前端构建：

```bash
pnpm build
```

桌面应用构建：

```bash
pnpm tauri build
```

构建完成后，Windows 推荐发布产物为 NSIS 安装包：

```text
src-tauri/target/release/bundle/nsis/TokenScope Desktop_0.1.2_x64-setup.exe
```

构建带应用内更新能力的发布包：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release.ps1
```

该命令会读取本机 `C:\Users\<name>\.tauri\tokenscope-desktop.key` 作为 Tauri updater 私钥，生成 NSIS 安装包、签名文件和 `latest.json`。私钥不要提交到仓库。发布 GitHub Release 时需要上传：

- `TokenScope.Desktop_0.1.2_x64-setup.exe`
- `TokenScope.Desktop_0.1.2_x64-setup.exe.sig`
- `latest.json`

`v0.1.1` 是首个内置 updater 的版本，旧版用户仍需手动安装一次；之后可以在设置页使用“应用更新”检查并安装新版本。

## 检查命令

```bash
pnpm lint
pnpm test
```

Rust 测试：

```bash
cd src-tauri
cargo test
```

## 数据库

应用启动时会在 Tauri app data 目录创建本地 SQLite 数据库，并自动执行 migrations。

核心表包括：

- `provider_config`
- `llm_call`
- `pricing_rule`
- `daily_usage_agg`
- `agent_import_map`
- `app_setting`
- `custom_importer_profile`
- `external_dataset`

Dashboard SQL 位于 `src-tauri/sql/`，Rust repository 和 Node 测试会复用这些 SQL 文本。

## 同步策略

- 增量同步按每个本机 Agent 数据源维护独立 cursor。
- 首次没有 cursor 时会完整扫描对应数据源。
- 成功同步后 cursor 写入 `app_setting`。
- 设置页保留“全量刷新”，用于跳过 cursor 重新扫描本机数据，修正增量同步可能产生的偏差。

## 版本管理

项目已使用 Git 管理，并维护中文 `CHANGELOG.md` 记录主要功能变更。
