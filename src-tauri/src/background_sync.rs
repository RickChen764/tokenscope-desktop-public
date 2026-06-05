use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Local};

use crate::db::{SyncRunResult, TokenScopeRepository};
use crate::github_sync;
use crate::importers::{import_detected_agents, AgentImportResult};

#[derive(Clone, Default)]
pub struct BackgroundSyncRuntime {
    running: Arc<AtomicBool>,
}

impl BackgroundSyncRuntime {
    fn try_start(&self) -> bool {
        self.running
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    }

    fn finish(&self) {
        self.running.store(false, Ordering::Release);
    }
}

pub async fn run_once(
    repository: &TokenScopeRepository,
    runtime: &BackgroundSyncRuntime,
) -> Result<SyncRunResult, String> {
    let now = Local::now().to_rfc3339();
    if !runtime.try_start() {
        return Ok(SyncRunResult {
            status: "busy".to_string(),
            message: "已有同步任务正在执行。".to_string(),
            imported: 0,
            skipped: 0,
            started_at: now.clone(),
            finished_at: now,
        });
    }

    let started_at = Local::now().to_rfc3339();
    let results = import_detected_agents(repository).await;
    let imported = results.iter().map(|result| result.imported).sum();
    let skipped = results.iter().map(|result| result.skipped).sum();
    let failed = has_failed_import(&results);
    let status = if failed { "error" } else { "success" };
    let message = if results.is_empty() {
        "未检测到可同步的本机 Agent 数据源。".to_string()
    } else {
        results
            .iter()
            .map(|result| format!("{}: {}", result.name, result.message))
            .collect::<Vec<_>>()
            .join("；")
    };
    let finished_at = Local::now().to_rfc3339();

    let record_result = repository
        .record_sync_run(
            &started_at,
            &finished_at,
            status,
            &message,
            imported,
            skipped,
        )
        .await
        .map_err(|err| err.to_string());
    runtime.finish();
    record_result?;
    if !failed {
        let _ = github_sync::engine::run_once(repository, false).await;
    }

    Ok(SyncRunResult {
        status: status.to_string(),
        message,
        imported,
        skipped,
        started_at,
        finished_at,
    })
}

pub fn spawn_background_sync_loop(
    repository: TokenScopeRepository,
    runtime: BackgroundSyncRuntime,
) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            let Ok(settings) = repository.get_sync_settings().await else {
                continue;
            };
            if !settings.enabled || !is_due(settings.next_sync_at.as_deref()) {
                continue;
            }

            let _ = run_once(&repository, &runtime).await;
        }
    });
}

pub fn spawn_launch_sync_if_enabled(
    repository: TokenScopeRepository,
    runtime: BackgroundSyncRuntime,
) {
    tauri::async_runtime::spawn(async move {
        let Ok(settings) = repository.get_sync_settings().await else {
            return;
        };
        if settings.sync_on_startup {
            let _ = run_once(&repository, &runtime).await;
        }
    });
}

fn is_due(next_run_at: Option<&str>) -> bool {
    let Some(next_run_at) = next_run_at else {
        return false;
    };
    let Ok(next_run_at) = DateTime::parse_from_rfc3339(next_run_at) else {
        return true;
    };

    next_run_at <= Local::now()
}

fn has_failed_import(results: &[AgentImportResult]) -> bool {
    results.iter().any(|result| result.status == "error")
}

#[cfg(test)]
mod tests {
    use crate::importers::AgentImportResult;

    #[test]
    fn sync_failure_detection_uses_import_status_not_message_text() {
        let successful_but_mentions_failed = AgentImportResult {
            id: "codex".to_string(),
            name: "Codex".to_string(),
            detected: true,
            import_supported: true,
            imported: 1,
            skipped: 0,
            source_path: Some("state.sqlite".to_string()),
            message: "Imported record with failed_call status.".to_string(),
            status: "success".to_string(),
            error: None,
        };
        let failed_without_failed_word = AgentImportResult {
            status: "error".to_string(),
            message: "Codex 无法读取本地数据库。".to_string(),
            error: Some("database is locked".to_string()),
            ..successful_but_mentions_failed.clone()
        };

        assert!(!super::has_failed_import(&[successful_but_mentions_failed]));
        assert!(super::has_failed_import(&[failed_without_failed_word]));
    }
}
