use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};

use crate::db::{SyncRunResult, TokenScopeRepository};
use crate::github_sync::{self, engine::GitHubSyncRuntime};
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
    github_sync_runtime: &GitHubSyncRuntime,
) -> Result<SyncRunResult, String> {
    let total_started = Instant::now();
    let now = Local::now().to_rfc3339();
    if !runtime.try_start() {
        eprintln!("[tokenscope][perf] background_sync.total elapsed_ms=0 status=busy");
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
    let import_started = Instant::now();
    let results = import_detected_agents(repository).await;
    let imported = results.iter().map(|result| result.imported).sum();
    let skipped = results.iter().map(|result| result.skipped).sum();
    let failed = has_failed_import(&results);
    eprintln!(
        "[tokenscope][perf] background_sync.import_agents elapsed_ms={} imported={} skipped={} failed={}",
        import_started.elapsed().as_millis(),
        imported,
        skipped,
        failed
    );
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

    let record_started = Instant::now();
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
    eprintln!(
        "[tokenscope][perf] background_sync.record_sync_run elapsed_ms={} status={}",
        record_started.elapsed().as_millis(),
        if record_result.is_ok() { "ok" } else { "error" }
    );
    if record_result.is_ok() && !failed {
        let github_started = Instant::now();
        let github_result =
            github_sync::engine::run_once_with_runtime(repository, github_sync_runtime, false)
                .await;
        match &github_result {
            Ok(result) => eprintln!(
                "[tokenscope][perf] background_sync.github_sync elapsed_ms={} status={}",
                github_started.elapsed().as_millis(),
                result.status
            ),
            Err(_) => eprintln!(
                "[tokenscope][perf] background_sync.github_sync elapsed_ms={} status=error",
                github_started.elapsed().as_millis()
            ),
        }
    }
    runtime.finish();
    eprintln!(
        "[tokenscope][perf] background_sync.total elapsed_ms={} status={} imported={} skipped={}",
        total_started.elapsed().as_millis(),
        if record_result.is_ok() {
            status
        } else {
            "error"
        },
        imported,
        skipped
    );
    record_result?;

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
    github_sync_runtime: GitHubSyncRuntime,
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

            let _ = run_once(&repository, &runtime, &github_sync_runtime).await;
        }
    });
}

pub fn spawn_launch_sync_if_enabled(
    repository: TokenScopeRepository,
    runtime: BackgroundSyncRuntime,
    github_sync_runtime: GitHubSyncRuntime,
) {
    tauri::async_runtime::spawn(async move {
        let Ok(settings) = repository.get_sync_settings().await else {
            return;
        };
        if settings.sync_on_startup {
            let _ = run_once(&repository, &runtime, &github_sync_runtime).await;
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
