use chrono::{Duration, Local, NaiveDate};
use serde::Serialize;
use tauri::State;

use crate::background_sync;
use crate::db::{
    AgentSourceStats, CallFilterOptions, CustomImporterPreview, CustomImporterProfile,
    CustomImporterProfileInput, CustomImporterRunResult, DailyUsagePoint, DashboardSummary,
    DataHealthIssueRow, DataHealthSummary, LlmCallFilters, LlmCallPage, LlmCallRow, SyncSettings,
    SyncSettingsInput, TokenPulseSnapshot, TopDimensionRow,
};
use crate::importers::codex::{import_default_codex_threads, CodexImportResult};
use crate::importers::custom_sqlite::{
    import_custom_sqlite_profile, preview_custom_sqlite_importer, validate_profile_input,
};
use crate::importers::{
    detect_local_agents as detect_agents, import_detected_agents_with_mode as import_agents,
    source_keys_for_agent,
};
use crate::importers::{AgentImportResult, ImportMode, LocalAgentStatus};
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct AgentSourceSummary {
    pub id: String,
    pub name: String,
    pub detected: bool,
    pub import_supported: bool,
    pub source_path: Option<String>,
    pub message: String,
    pub imported_calls: i64,
    pub total_tokens: i64,
    pub estimated_cost_usd: f64,
    pub cost_currency: String,
    pub last_imported_at: Option<String>,
    pub last_call_at: Option<String>,
}

fn date_window_for_range(range: &str) -> Result<(String, String), String> {
    let today: NaiveDate = Local::now().date_naive();
    let from = match range {
        "today" => today,
        "7d" => today - Duration::days(6),
        "30d" => today - Duration::days(29),
        "90d" => today - Duration::days(89),
        other => return Err(format!("unsupported dashboard range: {other}")),
    };

    Ok((from.to_string(), today.to_string()))
}

fn normalize_limit(limit: i64) -> i64 {
    limit.clamp(1, 100)
}

#[tauri::command]
pub async fn get_dashboard_summary(
    state: State<'_, AppState>,
    range: String,
) -> Result<DashboardSummary, String> {
    let (from, to) = date_window_for_range(&range)?;
    state
        .repository
        .dashboard_summary(&from, &to)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn get_dashboard_summary_for_dates(
    state: State<'_, AppState>,
    from: String,
    to: String,
) -> Result<DashboardSummary, String> {
    validate_date_range(&from, &to)?;
    state
        .repository
        .dashboard_summary(&from, &to)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn get_daily_usage_series(
    state: State<'_, AppState>,
    from: String,
    to: String,
    group_by: Option<String>,
) -> Result<Vec<DailyUsagePoint>, String> {
    state
        .repository
        .daily_usage_series(&from, &to, group_by.as_deref())
        .await
}

#[tauri::command]
pub async fn get_token_pulse(
    state: State<'_, AppState>,
    history_days: Option<i64>,
) -> Result<TokenPulseSnapshot, String> {
    state
        .repository
        .token_pulse_snapshot(history_days.unwrap_or(30))
        .await
}

fn validate_date_range(from: &str, to: &str) -> Result<(), String> {
    let from_date = NaiveDate::parse_from_str(from, "%Y-%m-%d")
        .map_err(|_| format!("invalid from date: {from}"))?;
    let to_date =
        NaiveDate::parse_from_str(to, "%Y-%m-%d").map_err(|_| format!("invalid to date: {to}"))?;
    if from_date > to_date {
        return Err("from date must be before or equal to to date".to_string());
    }

    Ok(())
}

#[tauri::command]
pub async fn get_dimension_summary(
    state: State<'_, AppState>,
    from: String,
    to: String,
    dimension: String,
    value: String,
) -> Result<DashboardSummary, String> {
    state
        .repository
        .dimension_summary(&from, &to, &dimension, &value)
        .await
}

#[tauri::command]
pub async fn get_dimension_daily_series(
    state: State<'_, AppState>,
    from: String,
    to: String,
    dimension: String,
    value: String,
) -> Result<Vec<DailyUsagePoint>, String> {
    state
        .repository
        .dimension_daily_series(&from, &to, &dimension, &value)
        .await
}

#[tauri::command]
pub async fn get_top_agents(
    state: State<'_, AppState>,
    from: String,
    to: String,
    limit: i64,
) -> Result<Vec<TopDimensionRow>, String> {
    state
        .repository
        .top_agents(&from, &to, normalize_limit(limit))
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn get_top_models(
    state: State<'_, AppState>,
    from: String,
    to: String,
    limit: i64,
) -> Result<Vec<TopDimensionRow>, String> {
    state
        .repository
        .top_models(&from, &to, normalize_limit(limit))
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn get_top_providers(
    state: State<'_, AppState>,
    from: String,
    to: String,
    limit: i64,
) -> Result<Vec<TopDimensionRow>, String> {
    state
        .repository
        .top_providers(&from, &to, normalize_limit(limit))
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn get_top_workflows(
    state: State<'_, AppState>,
    from: String,
    to: String,
    limit: i64,
) -> Result<Vec<TopDimensionRow>, String> {
    state
        .repository
        .top_workflows(&from, &to, normalize_limit(limit))
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn get_top_projects(
    state: State<'_, AppState>,
    from: String,
    to: String,
    limit: i64,
) -> Result<Vec<TopDimensionRow>, String> {
    state
        .repository
        .top_projects(&from, &to, normalize_limit(limit))
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn get_top_sessions(
    state: State<'_, AppState>,
    from: String,
    to: String,
    limit: i64,
) -> Result<Vec<TopDimensionRow>, String> {
    state
        .repository
        .top_sessions(&from, &to, normalize_limit(limit))
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn list_recent_calls(
    state: State<'_, AppState>,
    limit: i64,
) -> Result<Vec<LlmCallRow>, String> {
    state
        .repository
        .recent_calls(normalize_limit(limit))
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn list_llm_calls(
    state: State<'_, AppState>,
    filters: LlmCallFilters,
) -> Result<LlmCallPage, String> {
    state
        .repository
        .list_llm_calls(&filters)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn get_call_filter_options(
    state: State<'_, AppState>,
) -> Result<CallFilterOptions, String> {
    state
        .repository
        .call_filter_options()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn get_data_health_summary(
    state: State<'_, AppState>,
) -> Result<DataHealthSummary, String> {
    state
        .repository
        .data_health_summary()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn list_data_health_issues(
    state: State<'_, AppState>,
    filters: LlmCallFilters,
) -> Result<Vec<DataHealthIssueRow>, String> {
    state
        .repository
        .list_data_health_issues(&filters)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn list_custom_importer_profiles(
    state: State<'_, AppState>,
) -> Result<Vec<CustomImporterProfile>, String> {
    state
        .repository
        .list_custom_importer_profiles()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn upsert_custom_importer_profile(
    state: State<'_, AppState>,
    input: CustomImporterProfileInput,
) -> Result<CustomImporterProfile, String> {
    validate_profile_input(&input)?;
    state
        .repository
        .upsert_custom_importer_profile(&input)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn delete_custom_importer_profile(
    state: State<'_, AppState>,
    id: String,
) -> Result<bool, String> {
    if id.trim().is_empty() {
        return Err("custom importer id is required".to_string());
    }

    state
        .repository
        .delete_custom_importer_profile(&id)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn preview_custom_importer(
    input: CustomImporterProfileInput,
) -> Result<CustomImporterPreview, String> {
    preview_custom_sqlite_importer(&input).await
}

#[tauri::command]
pub async fn run_custom_importer(
    state: State<'_, AppState>,
    id: String,
) -> Result<CustomImporterRunResult, String> {
    let profile = state
        .repository
        .get_custom_importer_profile(&id)
        .await
        .map_err(|err| err.to_string())?
        .ok_or_else(|| format!("unknown custom importer profile: {id}"))?;

    if !profile.enabled {
        return Err(format!(
            "custom importer profile is disabled: {}",
            profile.name
        ));
    }

    let started_at = Local::now().to_rfc3339();
    match import_custom_sqlite_profile(&state.repository, &profile).await {
        Ok(outcome) => state
            .repository
            .record_custom_importer_run(
                &profile.id,
                "success",
                outcome.imported,
                outcome.skipped,
                None,
                &started_at,
            )
            .await
            .map_err(|err| err.to_string()),
        Err(err) => {
            let _ = state
                .repository
                .record_custom_importer_run(&profile.id, "error", 0, 0, Some(&err), &started_at)
                .await;
            Err(err)
        }
    }
}

#[tauri::command]
pub async fn list_agent_sources(
    state: State<'_, AppState>,
) -> Result<Vec<AgentSourceSummary>, String> {
    let statuses = detect_agents();
    let stats = state
        .repository
        .agent_source_stats()
        .await
        .map_err(|err| err.to_string())?;

    Ok(merge_agent_source_summaries(statuses, stats))
}

#[tauri::command]
pub async fn seed_demo_data(state: State<'_, AppState>) -> Result<(), String> {
    state
        .repository
        .seed_demo_data()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn clear_demo_data(state: State<'_, AppState>) -> Result<i64, String> {
    state
        .repository
        .clear_demo_data()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn import_codex_threads(state: State<'_, AppState>) -> Result<CodexImportResult, String> {
    import_default_codex_threads(&state.repository).await
}

#[tauri::command]
pub async fn detect_local_agents() -> Result<Vec<LocalAgentStatus>, String> {
    Ok(detect_agents())
}

#[tauri::command]
pub async fn import_detected_agents(
    state: State<'_, AppState>,
    mode: Option<String>,
) -> Result<Vec<AgentImportResult>, String> {
    Ok(import_agents(&state.repository, ImportMode::from_option(mode.as_deref())).await)
}

#[tauri::command]
pub async fn get_sync_settings(state: State<'_, AppState>) -> Result<SyncSettings, String> {
    state
        .repository
        .get_sync_settings()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn save_sync_settings(
    state: State<'_, AppState>,
    input: SyncSettingsInput,
) -> Result<SyncSettings, String> {
    state
        .repository
        .save_sync_settings(&SyncSettingsInput {
            enabled: input.enabled,
            interval_minutes: input.interval_minutes.clamp(5, 1440),
            sync_on_startup: input.sync_on_startup,
        })
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn run_background_sync_once(state: State<'_, AppState>) -> Result<SyncSettings, String> {
    let result = background_sync::run_once(&state.repository, &state.sync_runtime).await?;
    let mut settings = state
        .repository
        .get_sync_settings()
        .await
        .map_err(|err| err.to_string())?;
    if result.status == "busy" {
        settings.last_result = Some(result.message);
        settings.last_error = None;
    }

    Ok(settings)
}

fn merge_agent_source_summaries(
    statuses: Vec<LocalAgentStatus>,
    stats: Vec<AgentSourceStats>,
) -> Vec<AgentSourceSummary> {
    statuses
        .into_iter()
        .map(|status| {
            let source_stats = stats_for_agent(&status.id, &stats);
            AgentSourceSummary {
                id: status.id,
                name: status.name,
                detected: status.detected,
                import_supported: status.import_supported,
                source_path: status.source_path,
                message: status.message,
                imported_calls: source_stats.imported_calls,
                total_tokens: source_stats.total_tokens,
                estimated_cost_usd: source_stats.estimated_cost_usd,
                cost_currency: source_stats.cost_currency,
                last_imported_at: source_stats.last_imported_at,
                last_call_at: source_stats.last_call_at,
            }
        })
        .collect()
}

struct MergedAgentStats {
    imported_calls: i64,
    total_tokens: i64,
    estimated_cost_usd: f64,
    cost_currency: String,
    last_imported_at: Option<String>,
    last_call_at: Option<String>,
}

fn stats_for_agent(agent_id: &str, stats: &[AgentSourceStats]) -> MergedAgentStats {
    let source_keys = source_keys_for_agent(agent_id);
    let mut merged = MergedAgentStats {
        imported_calls: 0,
        total_tokens: 0,
        estimated_cost_usd: 0.0,
        cost_currency: "USD".to_string(),
        last_imported_at: None,
        last_call_at: None,
    };

    for row in stats
        .iter()
        .filter(|row| source_keys.contains(&row.source_key.as_str()) || row.source_key == agent_id)
    {
        let previous_currency = if merged.imported_calls == 0 {
            None
        } else {
            Some(merged.cost_currency.as_str())
        };
        merged.imported_calls += row.imported_calls;
        merged.total_tokens += row.total_tokens;
        merged.estimated_cost_usd += row.estimated_cost_usd;
        merged.cost_currency = merge_cost_currency(previous_currency, &row.cost_currency);
        keep_latest(&mut merged.last_imported_at, &row.last_imported_at);
        keep_latest(&mut merged.last_call_at, &row.last_call_at);
    }

    merged
}

fn merge_cost_currency(current: Option<&str>, next: &str) -> String {
    let normalized_next = if next.trim().is_empty() {
        "USD"
    } else {
        next.trim()
    };
    match current {
        None => normalized_next.to_string(),
        Some("MIXED") => "MIXED".to_string(),
        Some(value) if value == normalized_next => value.to_string(),
        Some(_) => "MIXED".to_string(),
    }
}

fn keep_latest(current: &mut Option<String>, candidate: &Option<String>) {
    let Some(candidate) = candidate else {
        return;
    };

    if current
        .as_deref()
        .map(|value| candidate.as_str() > value)
        .unwrap_or(true)
    {
        *current = Some(candidate.clone());
    }
}

#[cfg(test)]
mod tests {
    use crate::db::AgentSourceStats;
    use crate::importers::LocalAgentStatus;

    use super::merge_agent_source_summaries;

    #[test]
    fn merge_agent_source_summaries_uses_importer_source_keys() {
        let statuses = vec![LocalAgentStatus {
            id: "codex".to_string(),
            name: "Codex".to_string(),
            detected: true,
            import_supported: true,
            source_path: Some("state.sqlite".to_string()),
            message: "detected".to_string(),
        }];
        let stats = vec![AgentSourceStats {
            source_key: "codex_state_threads".to_string(),
            imported_calls: 2,
            total_tokens: 4096,
            estimated_cost_usd: 0.0,
            cost_currency: "USD".to_string(),
            last_imported_at: Some("2026-05-30T12:00:00+08:00".to_string()),
            last_call_at: Some("2026-05-30T11:00:00+08:00".to_string()),
        }];

        let summaries = merge_agent_source_summaries(statuses, stats);

        assert_eq!(summaries[0].id, "codex");
        assert_eq!(summaries[0].imported_calls, 2);
        assert_eq!(summaries[0].total_tokens, 4096);
        assert_eq!(
            summaries[0].last_imported_at.as_deref(),
            Some("2026-05-30T12:00:00+08:00")
        );
    }
}
