# TokenScope Desktop

[中文](README.md) | English

TokenScope Desktop is a local-first desktop dashboard for tracking LLM token usage. It does not implement a real proxy and does not call external LLM APIs. Instead, it reads existing local usage records from supported Agent tools, imports normalized metadata into a local SQLite database, and provides dashboards and reports on top of that data.

The project is built with Tauri v2, React, TypeScript, Rust, and SQLite.

## Current Features

- Local SQLite database with managed migrations.
- Dashboard summary: total tokens, call count, error rate, average latency, cached input, top Agent, and top model.
- Daily usage charts: stacked bar and line views for total token trends and Agent trends.
- Date ranges: today, last 7 days, last 30 days, last 90 days, and custom date windows.
- Ranking and detail views for Agent, model, Provider, workflow, project, and session dimensions.
- Call details page with local filters, pagination, status, and usage metadata.
- Data health checks for missing models, missing usage, failed calls, and other statistics quality issues.
- CSV export for local call metadata.
- Multi-device merge: export/import `.tokenscope` packages. Imported external device datasets can be removed without affecting local data.
- Background sync: startup incremental sync, scheduled sync, manual incremental sync, and full refresh.
- Custom SQLite importer: configure read-only SQL and field mappings for additional local Agent data sources.

## Supported Data Sources

- Codex: local Codex state and rollout token count data.
- Hermes: local Hermes state database.
- opencode: local opencode SQLite database.
- Claude Code: local Claude Code transcript JSONL records.
- Custom SQLite: user-defined import profiles for other local Agent tools.

More Agent data sources can be added through the importer framework.

## Privacy Boundary

- Prompt and response plaintext is not stored by default.
- CSV exports and `.tokenscope` packages do not include raw prompt/response content.
- Authorization headers, API keys, and other credentials are not read or stored.
- The app does not fetch official pricing data from the network and does not call external LLM APIs.
- Cost display and pricing-rule entry points are currently sealed in the product UI; the active focus is token and call statistics.

## Prerequisites

- Node.js 24+
- pnpm 11+
- Rust toolchain with `cargo` and `rustc`
- Windows, macOS, or Linux desktop environment

## Install Dependencies

```bash
pnpm install
```

## Development

Start the frontend preview only:

```bash
pnpm dev
```

Start the Tauri desktop client:

```bash
pnpm tauri dev
```

The default frontend development URL is:

```text
http://127.0.0.1:1420/
```

## Build

Build the frontend:

```bash
pnpm build
```

Build the desktop application:

```bash
pnpm tauri build
```

On Windows, the recommended release artifact is the NSIS installer:

```text
src-tauri/target/release/bundle/nsis/TokenScope Desktop_0.1.1_x64-setup.exe
```

Build a release package with in-app update support:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release.ps1
```

The script reads the Tauri updater private key from `C:\Users\<name>\.tauri\tokenscope-desktop.key` and generates the NSIS installer, signature file, and `latest.json`. Do not commit the private key. Upload these files to the GitHub Release:

- `TokenScope.Desktop_0.1.1_x64-setup.exe`
- `TokenScope.Desktop_0.1.1_x64-setup.exe.sig`
- `latest.json`

`v0.1.1` is the first version with the updater built in, so users on older versions still need to install it manually once. Later versions can be installed from the in-app update card on the settings page.

## Checks

```bash
pnpm lint
pnpm test
```

Rust tests:

```bash
cd src-tauri
cargo test
```

## Database

On startup, the app creates a local SQLite database in the Tauri app data directory and runs migrations automatically.

Core tables include:

- `provider_config`
- `llm_call`
- `pricing_rule`
- `daily_usage_agg`
- `agent_import_map`
- `app_setting`
- `custom_importer_profile`
- `external_dataset`

Dashboard SQL lives in `src-tauri/sql/`, allowing the Rust repository and Node-based SQL tests to exercise the same SQL text.

## Sync Strategy

- Incremental sync maintains an independent cursor for each local Agent data source.
- When no cursor exists, the first incremental sync performs a full scan for that source.
- After a successful sync, the cursor is stored in `app_setting`.
- The settings page keeps a full refresh action, which bypasses cursors and rescans local data to correct possible incremental-sync drift.

## Version Control

The project is managed with Git, and major feature changes are tracked in the Chinese `CHANGELOG.md`.
