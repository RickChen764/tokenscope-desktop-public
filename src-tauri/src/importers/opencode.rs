use std::path::{Path, PathBuf};

use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{query, query_as, Row};

use crate::db::{NewLlmCall, TokenScopeRepository};

use super::ImportScope;

pub const OPENCODE_MESSAGE_SOURCE: &str = "opencode_messages";
pub const OPENCODE_PART_SOURCE: &str = "opencode_parts";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodeImportResult {
    pub imported: i64,
    pub skipped: i64,
    pub source_path: String,
}

#[derive(Debug, Clone)]
struct OpenCodeUsage {
    input_tokens: i64,
    output_tokens: i64,
    cached_input_tokens: i64,
    cache_write_input_tokens: i64,
    reasoning_output_tokens: i64,
    total_tokens: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct OpenCodeMessageRow {
    id: String,
    session_id: String,
    time_created: i64,
    time_updated: i64,
    data: String,
    session_directory: Option<String>,
    session_agent: Option<String>,
    session_model: Option<String>,
    project_name: Option<String>,
    project_worktree: Option<String>,
    workspace_name: Option<String>,
    workspace_directory: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct OpenCodePartRow {
    id: String,
    session_id: String,
    time_created: i64,
    time_updated: i64,
    data: String,
    message_data: Option<String>,
    session_directory: Option<String>,
    session_agent: Option<String>,
    session_model: Option<String>,
    project_name: Option<String>,
    project_worktree: Option<String>,
    workspace_name: Option<String>,
    workspace_directory: Option<String>,
}

pub fn default_opencode_state_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
        let home = PathBuf::from(home);
        paths.push(
            home.join(".local")
                .join("share")
                .join("opencode")
                .join("opencode.db"),
        );
        paths.push(
            home.join(".local")
                .join("share")
                .join("opencode")
                .join("state.db"),
        );
        paths.push(home.join(".opencode").join("opencode.db"));
        paths.push(home.join(".opencode").join("state.db"));
    }

    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        let root = PathBuf::from(local_app_data).join("opencode");
        paths.push(root.join("opencode.db"));
        paths.push(root.join("state.db"));
        paths.push(root.join("data.db"));
    }

    if let Ok(app_data) = std::env::var("APPDATA") {
        let root = PathBuf::from(app_data).join("opencode");
        paths.push(root.join("opencode.db"));
        paths.push(root.join("state.db"));
        paths.push(root.join("data.db"));
    }

    paths
}

pub fn is_candidate_database_file(path: &Path) -> bool {
    path.metadata()
        .map(|metadata| metadata.is_file() && metadata.len() > 0)
        .unwrap_or(false)
}

#[allow(dead_code)]
pub async fn import_opencode_usage_from_path(
    repository: &TokenScopeRepository,
    source_path: &Path,
) -> Result<OpenCodeImportResult, sqlx::Error> {
    import_opencode_usage_from_path_with_scope(repository, source_path, &ImportScope::full()).await
}

pub async fn import_opencode_usage_from_path_with_scope(
    repository: &TokenScopeRepository,
    source_path: &Path,
    scope: &ImportScope,
) -> Result<OpenCodeImportResult, sqlx::Error> {
    let options = SqliteConnectOptions::new()
        .filename(source_path)
        .read_only(true);
    let source_pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    let since_ms = scope
        .since
        .as_ref()
        .map(|timestamp| timestamp.timestamp_millis());

    let rows = query_as::<_, OpenCodeMessageRow>(
        r#"
    SELECT
      m.id,
      m.session_id,
      m.time_created,
      m.time_updated,
      m.data,
      s.directory AS session_directory,
      s.agent AS session_agent,
      s.model AS session_model,
      p.name AS project_name,
      p.worktree AS project_worktree,
      w.name AS workspace_name,
      w.directory AS workspace_directory
    FROM message m
    LEFT JOIN session s ON s.id = m.session_id
    LEFT JOIN project p ON p.id = s.project_id
    LEFT JOIN workspace w ON w.id = s.workspace_id
    WHERE (?1 IS NULL OR COALESCE(m.time_updated, m.time_created, 0) >= ?1)
    ORDER BY m.time_created ASC, m.id ASC
    "#,
    )
    .bind(since_ms)
    .fetch_all(&source_pool)
    .await?;

    let mut imported = 0;
    let mut skipped = 0;
    let mut importable_messages = 0;

    for row in rows {
        let Some(call) = opencode_message_to_call(&row) else {
            continue;
        };
        importable_messages += 1;

        if has_imported(repository, OPENCODE_MESSAGE_SOURCE, &row.id).await? {
            if should_refresh_imported_call(repository, &call).await? {
                repository.insert_llm_call(&call).await?;
                record_import(repository, OPENCODE_MESSAGE_SOURCE, &row.id, &call.id).await?;
                imported += 1;
                continue;
            }

            skipped += 1;
            continue;
        }

        repository.insert_llm_call(&call).await?;
        record_import(repository, OPENCODE_MESSAGE_SOURCE, &row.id, &call.id).await?;
        imported += 1;
    }

    if importable_messages == 0 {
        let part_result = import_part_usage(repository, &source_pool, scope).await?;
        imported += part_result.imported;
        skipped += part_result.skipped;
    }

    source_pool.close().await;

    Ok(OpenCodeImportResult {
        imported,
        skipped,
        source_path: source_path.display().to_string(),
    })
}

async fn import_part_usage(
    repository: &TokenScopeRepository,
    source_pool: &sqlx::SqlitePool,
    scope: &ImportScope,
) -> Result<OpenCodeImportResult, sqlx::Error> {
    let since_ms = scope
        .since
        .as_ref()
        .map(|timestamp| timestamp.timestamp_millis());
    let rows = query_as::<_, OpenCodePartRow>(
        r#"
    SELECT
      part.id,
      part.session_id,
      part.time_created,
      part.time_updated,
      part.data,
      message.data AS message_data,
      s.directory AS session_directory,
      s.agent AS session_agent,
      s.model AS session_model,
      p.name AS project_name,
      p.worktree AS project_worktree,
      w.name AS workspace_name,
      w.directory AS workspace_directory
    FROM part
    LEFT JOIN message ON message.id = part.message_id
    LEFT JOIN session s ON s.id = part.session_id
    LEFT JOIN project p ON p.id = s.project_id
    LEFT JOIN workspace w ON w.id = s.workspace_id
    WHERE (?1 IS NULL OR COALESCE(part.time_updated, part.time_created, 0) >= ?1)
    ORDER BY part.time_created ASC, part.id ASC
    "#,
    )
    .bind(since_ms)
    .fetch_all(source_pool)
    .await?;

    let mut imported = 0;
    let mut skipped = 0;
    for row in rows {
        let Some(call) = opencode_part_to_call(&row) else {
            continue;
        };

        if has_imported(repository, OPENCODE_PART_SOURCE, &row.id).await? {
            if should_refresh_imported_call(repository, &call).await? {
                repository.insert_llm_call(&call).await?;
                record_import(repository, OPENCODE_PART_SOURCE, &row.id, &call.id).await?;
                imported += 1;
                continue;
            }

            skipped += 1;
            continue;
        }

        repository.insert_llm_call(&call).await?;
        record_import(repository, OPENCODE_PART_SOURCE, &row.id, &call.id).await?;
        imported += 1;
    }

    Ok(OpenCodeImportResult {
        imported,
        skipped,
        source_path: String::new(),
    })
}

async fn has_imported(
    repository: &TokenScopeRepository,
    source: &str,
    external_id: &str,
) -> Result<bool, sqlx::Error> {
    let existing = query(
        r#"
    SELECT 1
    FROM agent_import_map
    WHERE source = ?1 AND external_id = ?2
    LIMIT 1
    "#,
    )
    .bind(source)
    .bind(external_id)
    .fetch_optional(repository.pool())
    .await?;

    Ok(existing.is_some())
}

async fn should_refresh_imported_call(
    repository: &TokenScopeRepository,
    call: &NewLlmCall,
) -> Result<bool, sqlx::Error> {
    let existing = query(
        r#"
    SELECT
      started_at,
      ended_at,
      date_local,
      input_tokens,
      output_tokens,
      cached_input_tokens,
      cache_write_input_tokens,
      reasoning_output_tokens,
      total_tokens,
      estimated_cost_usd,
      cost_currency
    FROM llm_call
    WHERE id = ?1
    LIMIT 1
    "#,
    )
    .bind(&call.id)
    .fetch_optional(repository.pool())
    .await?;

    let Some(existing) = existing else {
        return Ok(true);
    };

    let started_at = existing.try_get::<String, _>("started_at")?;
    let ended_at = existing.try_get::<Option<String>, _>("ended_at")?;
    let date_local = existing.try_get::<String, _>("date_local")?;
    let input_tokens = existing.try_get::<i64, _>("input_tokens")?;
    let output_tokens = existing.try_get::<i64, _>("output_tokens")?;
    let cached_input_tokens = existing.try_get::<i64, _>("cached_input_tokens")?;
    let cache_write_input_tokens = existing.try_get::<i64, _>("cache_write_input_tokens")?;
    let reasoning_output_tokens = existing.try_get::<i64, _>("reasoning_output_tokens")?;
    let total_tokens = existing.try_get::<i64, _>("total_tokens")?;
    let estimated_cost_usd = existing.try_get::<f64, _>("estimated_cost_usd")?;
    let cost_currency = existing.try_get::<String, _>("cost_currency")?;

    Ok(started_at != call.started_at
        || ended_at != call.ended_at
        || date_local != call.date_local
        || input_tokens != call.input_tokens
        || output_tokens != call.output_tokens
        || cached_input_tokens != call.cached_input_tokens
        || cache_write_input_tokens != call.cache_write_input_tokens
        || reasoning_output_tokens != call.reasoning_output_tokens
        || total_tokens != call.total_tokens
        || (estimated_cost_usd - call.estimated_cost_usd).abs() > f64::EPSILON
        || cost_currency != call.cost_currency)
}

async fn record_import(
    repository: &TokenScopeRepository,
    source: &str,
    external_id: &str,
    llm_call_id: &str,
) -> Result<(), sqlx::Error> {
    query(
        r#"
    INSERT INTO agent_import_map (
      source,
      external_id,
      llm_call_id,
      imported_at
    ) VALUES (?1, ?2, ?3, ?4)
    ON CONFLICT(source, external_id) DO UPDATE SET
      llm_call_id = excluded.llm_call_id,
      imported_at = excluded.imported_at
    "#,
    )
    .bind(source)
    .bind(external_id)
    .bind(llm_call_id)
    .bind(Local::now().to_rfc3339())
    .execute(repository.pool())
    .await?;

    Ok(())
}

fn opencode_message_to_call(row: &OpenCodeMessageRow) -> Option<NewLlmCall> {
    let value: Value = serde_json::from_str(&row.data).ok()?;
    if string_at(&value, &["role"]).as_deref() != Some("assistant") {
        return None;
    }

    let usage = usage_from_value(&value)?;
    let model = model_id_from_values(&value, row.session_model.as_deref());
    let provider_id = provider_id_from_values(&value, row.session_model.as_deref());
    let cost = number_at(&value, &["cost"]);
    let created_at = timestamp_at(&value, &["time", "created"])
        .or_else(|| timestamp_from_epoch_value(row.time_created as f64))
        .unwrap_or_else(Local::now);
    let completed_at = timestamp_at(&value, &["time", "completed"])
        .or_else(|| timestamp_from_epoch_value(row.time_updated as f64));

    Some(new_opencode_call(
        OPENCODE_MESSAGE_SOURCE,
        &format!("opencode-message-{}", row.id),
        &row.id,
        &row.session_id,
        "opencode_message_import",
        "assistant_message",
        created_at,
        completed_at,
        usage,
        model,
        provider_id,
        cost,
        project_id_from_context(
            row.project_name.as_deref(),
            row.project_worktree.as_deref(),
            row.workspace_name.as_deref(),
            row.workspace_directory.as_deref(),
            row.session_directory.as_deref(),
        ),
        row.session_agent.as_deref(),
    ))
}

fn opencode_part_to_call(row: &OpenCodePartRow) -> Option<NewLlmCall> {
    let value: Value = serde_json::from_str(&row.data).ok()?;
    let usage = usage_from_value(&value)?;
    let message_value = row
        .message_data
        .as_deref()
        .and_then(|data| serde_json::from_str::<Value>(data).ok());
    let message_value_ref = message_value.as_ref();
    let model = model_id_from_values(&value, row.session_model.as_deref())
        .or_else(|| message_value_ref.and_then(|value| model_id_from_values(value, None)));
    let provider_id = provider_id_from_values(&value, row.session_model.as_deref())
        .or_else(|| message_value_ref.and_then(|value| provider_id_from_values(value, None)));
    let cost = number_at(&value, &["cost"]);
    let created_at = timestamp_at(&value, &["time", "start"])
        .or_else(|| timestamp_at(&value, &["state", "time", "start"]))
        .or_else(|| timestamp_at(&value, &["time", "created"]))
        .or_else(|| timestamp_from_epoch_value(row.time_created as f64))
        .unwrap_or_else(Local::now);
    let completed_at = timestamp_at(&value, &["time", "end"])
        .or_else(|| timestamp_at(&value, &["state", "time", "end"]))
        .or_else(|| timestamp_at(&value, &["time", "completed"]))
        .or_else(|| timestamp_from_epoch_value(row.time_updated as f64));

    Some(new_opencode_call(
        OPENCODE_PART_SOURCE,
        &format!("opencode-part-{}", row.id),
        &row.id,
        &row.session_id,
        "opencode_part_import",
        "usage_part",
        created_at,
        completed_at,
        usage,
        model,
        provider_id,
        cost,
        project_id_from_context(
            row.project_name.as_deref(),
            row.project_worktree.as_deref(),
            row.workspace_name.as_deref(),
            row.workspace_directory.as_deref(),
            row.session_directory.as_deref(),
        ),
        row.session_agent.as_deref(),
    ))
}

#[allow(clippy::too_many_arguments)]
fn new_opencode_call(
    source: &str,
    call_id: &str,
    external_id: &str,
    session_id: &str,
    api_type: &str,
    workflow_step: &str,
    started_at: DateTime<Local>,
    ended_at: Option<DateTime<Local>>,
    usage: OpenCodeUsage,
    model: Option<String>,
    provider_id: Option<String>,
    cost: Option<f64>,
    project_id: Option<String>,
    agent_name: Option<&str>,
) -> NewLlmCall {
    let latency_ms = ended_at.map(|ended_at| {
        ended_at
            .signed_duration_since(started_at)
            .num_milliseconds()
            .max(0)
    });
    let billable_tokens = usage.total_tokens;
    let estimated_cost_usd = cost.unwrap_or(0.0).max(0.0);
    let cost_source = if cost.is_some() {
        "opencode_reported_cost"
    } else {
        "opencode_import_no_cost"
    };

    NewLlmCall {
        id: call_id.to_string(),
        started_at: started_at.to_rfc3339(),
        ended_at: ended_at.map(|value| value.to_rfc3339()),
        date_local: started_at.date_naive().to_string(),
        provider: "opencode".to_string(),
        provider_config_id: provider_id.clone(),
        api_type: Some(api_type.to_string()),
        model_requested: model.clone(),
        model_response: model.clone(),
        agent_id: Some("opencode".to_string()),
        agent_name: Some(
            agent_name
                .filter(|value| !value.is_empty())
                .unwrap_or("opencode")
                .to_string(),
        ),
        agent_run_id: Some(session_id.to_string()),
        workflow_id: Some("opencode_session".to_string()),
        workflow_step: Some(workflow_step.to_string()),
        session_id: Some(session_id.to_string()),
        trace_id: None,
        span_id: None,
        parent_span_id: None,
        project_id,
        user_id: None,
        environment: Some("local".to_string()),
        feature: Some("opencode_import".to_string()),
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        cached_input_tokens: usage.cached_input_tokens,
        cache_write_input_tokens: usage.cache_write_input_tokens,
        reasoning_output_tokens: usage.reasoning_output_tokens,
        audio_input_tokens: 0,
        audio_output_tokens: 0,
        image_input_tokens: 0,
        image_output_tokens: 0,
        total_tokens: usage.total_tokens,
        total_billable_tokens: billable_tokens,
        request_count: 1,
        tool_call_count: 0,
        retry_count: 0,
        latency_ms,
        http_status: None,
        status: "success".to_string(),
        error_type: None,
        error_message: None,
        estimated_cost_usd,
        cost_currency: "USD".to_string(),
        provider_reported_cost_usd: cost,
        reconciled_cost_usd: None,
        cost_source: Some(cost_source.to_string()),
        usage_source: Some("opencode_usage_json".to_string()),
        raw_usage_json: Some(
            json!({
              "source": source,
              "external_id": external_id,
              "session_id": session_id,
              "provider_id": provider_id,
              "model": model,
              "tokens": {
                "input": usage.input_tokens,
                "output": usage.output_tokens,
                "cache": {
                  "read": usage.cached_input_tokens,
                  "write": usage.cache_write_input_tokens,
                },
                "reasoning": usage.reasoning_output_tokens,
                "total": usage.total_tokens,
              },
              "cost": cost,
            })
            .to_string(),
        ),
        raw_response_json: None,
        request_hash: None,
        response_hash: None,
        prompt_template_id: None,
        created_at: Local::now().to_rfc3339(),
    }
}

fn usage_from_value(value: &Value) -> Option<OpenCodeUsage> {
    let input_tokens = int_at(value, &["tokens", "input"])
        .unwrap_or_default()
        .max(0);
    let output_tokens = int_at(value, &["tokens", "output"])
        .unwrap_or_default()
        .max(0);
    let cached_input_tokens = int_at(value, &["tokens", "cache", "read"])
        .or_else(|| int_at(value, &["tokens", "cache_read"]))
        .unwrap_or_default()
        .max(0);
    let cache_write_input_tokens = int_at(value, &["tokens", "cache", "write"])
        .or_else(|| int_at(value, &["tokens", "cache_write"]))
        .unwrap_or_default()
        .max(0);
    let reasoning_output_tokens = int_at(value, &["tokens", "reasoning"])
        .unwrap_or_default()
        .max(0);
    let fallback_total = input_tokens
        + output_tokens
        + cached_input_tokens
        + cache_write_input_tokens
        + reasoning_output_tokens;
    let explicit_total = int_at(value, &["tokens", "total"]).map(|value| value.max(0));
    let total_tokens = explicit_total.unwrap_or(fallback_total);

    if total_tokens <= 0 && fallback_total <= 0 {
        return None;
    }

    Some(OpenCodeUsage {
        input_tokens,
        output_tokens,
        cached_input_tokens,
        cache_write_input_tokens,
        reasoning_output_tokens,
        total_tokens,
    })
}

fn model_id_from_values(value: &Value, session_model: Option<&str>) -> Option<String> {
    string_at(value, &["model", "modelID"])
        .or_else(|| string_at(value, &["model", "id"]))
        .or_else(|| string_at(value, &["modelID"]))
        .or_else(|| string_at(value, &["model_id"]))
        .or_else(|| model_field_from_session(session_model, "modelID"))
        .or_else(|| model_field_from_session(session_model, "id"))
        .filter(|value| !value.is_empty())
}

fn provider_id_from_values(value: &Value, session_model: Option<&str>) -> Option<String> {
    string_at(value, &["model", "providerID"])
        .or_else(|| string_at(value, &["providerID"]))
        .or_else(|| string_at(value, &["provider_id"]))
        .or_else(|| model_field_from_session(session_model, "providerID"))
        .filter(|value| !value.is_empty())
}

fn model_field_from_session(session_model: Option<&str>, key: &str) -> Option<String> {
    let session_model = session_model?.trim();
    if session_model.is_empty() {
        return None;
    }

    if !session_model.starts_with('{') {
        return if key == "id" || key == "modelID" {
            Some(session_model.to_string())
        } else {
            None
        };
    }

    let value: Value = serde_json::from_str(session_model).ok()?;
    string_at(&value, &[key])
}

fn project_id_from_context(
    project_name: Option<&str>,
    project_worktree: Option<&str>,
    workspace_name: Option<&str>,
    workspace_directory: Option<&str>,
    session_directory: Option<&str>,
) -> Option<String> {
    non_empty(project_name)
        .or_else(|| non_empty(workspace_name))
        .or_else(|| project_worktree.and_then(project_name_from_path))
        .or_else(|| workspace_directory.and_then(project_name_from_path))
        .or_else(|| session_directory.and_then(project_name_from_path))
}

fn project_name_from_path(path: &str) -> Option<String> {
    path.replace('\\', "/")
        .rsplit('/')
        .find(|segment| !segment.is_empty())
        .map(ToString::to_string)
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn value_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(value, |current, key| current.get(key))
}

fn string_at(value: &Value, path: &[&str]) -> Option<String> {
    let value = value_at(value, path)?;
    if let Some(value) = value.as_str() {
        return Some(value.to_string());
    }

    if let Some(value) = value.as_i64() {
        return Some(value.to_string());
    }

    value.as_u64().map(|value| value.to_string())
}

fn int_at(value: &Value, path: &[&str]) -> Option<i64> {
    let value = value_at(value, path)?;
    if let Some(value) = value.as_i64() {
        return Some(value);
    }
    if let Some(value) = value.as_u64() {
        return i64::try_from(value).ok();
    }
    if let Some(value) = value.as_f64() {
        return value.is_finite().then_some(value.round() as i64);
    }

    value
        .as_str()?
        .parse::<f64>()
        .ok()
        .map(|value| value.round() as i64)
}

fn number_at(value: &Value, path: &[&str]) -> Option<f64> {
    let value = value_at(value, path)?;
    if let Some(value) = value.as_f64() {
        return value.is_finite().then_some(value);
    }

    value
        .as_str()?
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}

fn timestamp_at(value: &Value, path: &[&str]) -> Option<DateTime<Local>> {
    let value = value_at(value, path)?;
    if let Some(timestamp) = value.as_str().and_then(timestamp_from_string) {
        return Some(timestamp);
    }
    if let Some(timestamp) = value.as_f64().and_then(timestamp_from_epoch_value) {
        return Some(timestamp);
    }

    None
}

fn timestamp_from_string(value: &str) -> Option<DateTime<Local>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|timestamp| timestamp.with_timezone(&Local))
        .or_else(|| {
            value
                .parse::<f64>()
                .ok()
                .and_then(timestamp_from_epoch_value)
        })
}

fn timestamp_from_epoch_value(value: f64) -> Option<DateTime<Local>> {
    if !value.is_finite() || value <= 0.0 {
        return None;
    }

    let timestamp = if value.abs() >= 10_000_000_000.0 {
        DateTime::<Utc>::from_timestamp_millis(value.round() as i64)?
    } else {
        let seconds = value.floor() as i64;
        let nanos = ((value - seconds as f64) * 1_000_000_000.0).round() as u32;
        DateTime::<Utc>::from_timestamp(seconds, nanos.min(999_999_999))?
    };

    Some(timestamp.with_timezone(&Local))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use chrono::{DateTime, Local, Utc};
    use serde_json::json;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use sqlx::{query, Row};
    use uuid::Uuid;

    use crate::db::TokenScopeRepository;

    use crate::importers::ImportScope;

    use super::{import_opencode_usage_from_path, import_opencode_usage_from_path_with_scope};

    #[tokio::test]
    async fn imports_opencode_messages_without_prompt_or_response_text() {
        let source_path = create_opencode_db(true).await;
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("target repository connects");
        repository.migrate().await.expect("target migrations run");

        let result = import_opencode_usage_from_path(&repository, &source_path)
            .await
            .expect("opencode import succeeds");

        assert_eq!(result.imported, 1);
        assert_eq!(result.skipped, 0);

        let row = query(
            r#"
      SELECT
        provider,
        provider_config_id,
        api_type,
        model_response,
        project_id,
        session_id,
        input_tokens,
        output_tokens,
        cached_input_tokens,
        cache_write_input_tokens,
        reasoning_output_tokens,
        total_tokens,
        latency_ms,
        estimated_cost_usd,
        provider_reported_cost_usd,
        cost_source,
        usage_source,
        raw_usage_json,
        raw_response_json
      FROM llm_call
      WHERE id = 'opencode-message-message_assistant_1'
      "#,
        )
        .fetch_one(repository.pool())
        .await
        .expect("imported call exists");

        assert_eq!(row.get::<String, _>("provider"), "opencode");
        assert_eq!(row.get::<String, _>("provider_config_id"), "openai");
        assert_eq!(row.get::<String, _>("api_type"), "opencode_message_import");
        assert_eq!(row.get::<String, _>("model_response"), "gpt-5.5");
        assert_eq!(row.get::<String, _>("project_id"), "sample-project");
        assert_eq!(row.get::<String, _>("session_id"), "session_1");
        assert_eq!(row.get::<i64, _>("input_tokens"), 1000);
        assert_eq!(row.get::<i64, _>("output_tokens"), 200);
        assert_eq!(row.get::<i64, _>("cached_input_tokens"), 300);
        assert_eq!(row.get::<i64, _>("cache_write_input_tokens"), 40);
        assert_eq!(row.get::<i64, _>("reasoning_output_tokens"), 50);
        assert_eq!(row.get::<i64, _>("total_tokens"), 1590);
        assert_eq!(row.get::<i64, _>("latency_ms"), 2500);
        assert!((row.get::<f64, _>("estimated_cost_usd") - 0.1234).abs() < f64::EPSILON);
        assert!((row.get::<f64, _>("provider_reported_cost_usd") - 0.1234).abs() < f64::EPSILON);
        assert_eq!(
            row.get::<String, _>("cost_source"),
            "opencode_reported_cost"
        );
        assert_eq!(row.get::<String, _>("usage_source"), "opencode_usage_json");
        assert_eq!(row.get::<Option<String>, _>("raw_response_json"), None);

        let raw_usage_json = row.get::<String, _>("raw_usage_json");
        assert!(raw_usage_json.contains("\"input\":1000"));
        assert!(!raw_usage_json.contains("secret user prompt"));
        assert!(!raw_usage_json.contains("private assistant answer"));
    }

    #[tokio::test]
    async fn import_opencode_messages_is_idempotent() {
        let source_path = create_opencode_db(true).await;
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("target repository connects");
        repository.migrate().await.expect("target migrations run");

        let first = import_opencode_usage_from_path(&repository, &source_path)
            .await
            .expect("first import succeeds");
        let second = import_opencode_usage_from_path(&repository, &source_path)
            .await
            .expect("second import succeeds");

        assert_eq!(first.imported, 1);
        assert_eq!(first.skipped, 0);
        assert_eq!(second.imported, 0);
        assert_eq!(second.skipped, 1);
    }

    #[tokio::test]
    async fn import_opencode_with_incremental_scope_skips_older_messages() {
        let source_path = create_opencode_db(true).await;
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("target repository connects");
        repository.migrate().await.expect("target migrations run");
        let since = DateTime::<Utc>::from_timestamp_millis(1791000000000)
            .expect("test timestamp is valid")
            .with_timezone(&Local);
        let scope = ImportScope::incremental(Some(since));

        let result = import_opencode_usage_from_path_with_scope(&repository, &source_path, &scope)
            .await
            .expect("incremental import succeeds");

        assert_eq!(result.imported, 0);
        assert_eq!(result.skipped, 0);
    }

    #[tokio::test]
    async fn imports_part_usage_when_messages_have_no_usage() {
        let source_path = create_opencode_db(false).await;
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("target repository connects");
        repository.migrate().await.expect("target migrations run");

        let result = import_opencode_usage_from_path(&repository, &source_path)
            .await
            .expect("part fallback import succeeds");

        assert_eq!(result.imported, 1);
        assert_eq!(result.skipped, 0);

        let row = query(
            r#"
      SELECT
        provider,
        provider_config_id,
        api_type,
        model_response,
        input_tokens,
        output_tokens,
        cached_input_tokens,
        cache_write_input_tokens,
        reasoning_output_tokens,
        total_tokens,
        estimated_cost_usd,
        raw_usage_json,
        raw_response_json
      FROM llm_call
      WHERE id = 'opencode-part-part_finish_1'
      "#,
        )
        .fetch_one(repository.pool())
        .await
        .expect("fallback call exists");

        assert_eq!(row.get::<String, _>("provider"), "opencode");
        assert_eq!(row.get::<String, _>("provider_config_id"), "openai");
        assert_eq!(row.get::<String, _>("api_type"), "opencode_part_import");
        assert_eq!(row.get::<String, _>("model_response"), "gpt-5.5");
        assert_eq!(row.get::<i64, _>("input_tokens"), 700);
        assert_eq!(row.get::<i64, _>("output_tokens"), 80);
        assert_eq!(row.get::<i64, _>("cached_input_tokens"), 120);
        assert_eq!(row.get::<i64, _>("cache_write_input_tokens"), 30);
        assert_eq!(row.get::<i64, _>("reasoning_output_tokens"), 10);
        assert_eq!(row.get::<i64, _>("total_tokens"), 940);
        assert!((row.get::<f64, _>("estimated_cost_usd") - 0.045).abs() < f64::EPSILON);
        assert_eq!(row.get::<Option<String>, _>("raw_response_json"), None);

        let raw_usage_json = row.get::<String, _>("raw_usage_json");
        assert!(raw_usage_json.contains("\"source\":\"opencode_parts\""));
        assert!(!raw_usage_json.contains("secret user prompt"));
        assert!(!raw_usage_json.contains("private assistant answer"));
    }

    async fn create_opencode_db(with_message_usage: bool) -> PathBuf {
        let path = std::env::temp_dir().join(format!("tokenscope-opencode-{}.db", Uuid::new_v4()));
        let _ = fs::remove_file(&path);
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("source db connects");

        create_schema(&pool).await;
        insert_fixture_rows(&pool, with_message_usage).await;
        pool.close().await;

        path
    }

    async fn create_schema(pool: &sqlx::SqlitePool) {
        query(
            r#"
      CREATE TABLE workspace (
        id TEXT PRIMARY KEY,
        type TEXT NOT NULL,
        name TEXT DEFAULT '' NOT NULL,
        branch TEXT,
        directory TEXT,
        extra TEXT,
        project_id TEXT NOT NULL,
        time_used INTEGER NOT NULL
      )
      "#,
        )
        .execute(pool)
        .await
        .expect("workspace schema created");

        query(
            r#"
      CREATE TABLE project (
        id TEXT PRIMARY KEY,
        worktree TEXT NOT NULL,
        vcs TEXT,
        name TEXT,
        icon_url TEXT,
        icon_color TEXT,
        time_created INTEGER NOT NULL,
        time_updated INTEGER NOT NULL,
        time_initialized INTEGER,
        sandboxes TEXT NOT NULL
      )
      "#,
        )
        .execute(pool)
        .await
        .expect("project schema created");

        query(
            r#"
      CREATE TABLE session (
        id TEXT PRIMARY KEY,
        project_id TEXT NOT NULL,
        parent_id TEXT,
        slug TEXT NOT NULL,
        directory TEXT NOT NULL,
        title TEXT NOT NULL,
        version TEXT NOT NULL,
        time_created INTEGER NOT NULL,
        time_updated INTEGER NOT NULL,
        workspace_id TEXT,
        path TEXT,
        agent TEXT,
        model TEXT
      )
      "#,
        )
        .execute(pool)
        .await
        .expect("session schema created");

        query(
            r#"
      CREATE TABLE message (
        id TEXT PRIMARY KEY,
        session_id TEXT NOT NULL,
        time_created INTEGER NOT NULL,
        time_updated INTEGER NOT NULL,
        data TEXT NOT NULL
      )
      "#,
        )
        .execute(pool)
        .await
        .expect("message schema created");

        query(
            r#"
      CREATE TABLE part (
        id TEXT PRIMARY KEY,
        message_id TEXT NOT NULL,
        session_id TEXT NOT NULL,
        time_created INTEGER NOT NULL,
        time_updated INTEGER NOT NULL,
        data TEXT NOT NULL
      )
      "#,
        )
        .execute(pool)
        .await
        .expect("part schema created");
    }

    async fn insert_fixture_rows(pool: &sqlx::SqlitePool, with_message_usage: bool) {
        query(
            r#"
      INSERT INTO project (
        id,
        worktree,
        name,
        time_created,
        time_updated,
        sandboxes
      ) VALUES (
        'project_1',
        'D:\Project\sample-project',
        'sample-project',
        1790000000000,
        1790000000000,
        '{}'
      )
      "#,
        )
        .execute(pool)
        .await
        .expect("project inserted");

        query(
            r#"
      INSERT INTO workspace (
        id,
        type,
        name,
        directory,
        project_id,
        time_used
      ) VALUES (
        'workspace_1',
        'local',
        'sample-workspace',
        'D:\Project\sample-project',
        'project_1',
        1790000000000
      )
      "#,
        )
        .execute(pool)
        .await
        .expect("workspace inserted");

        query(
            r#"
      INSERT INTO session (
        id,
        project_id,
        slug,
        directory,
        title,
        version,
        time_created,
        time_updated,
        workspace_id,
        agent,
        model
      ) VALUES (
        'session_1',
        'project_1',
        'session-1',
        'D:\Project\sample-project',
        'sensitive session title',
        '1.0.0',
        1790000000000,
        1790000010000,
        'workspace_1',
        'builder',
        '{"providerID":"openai","modelID":"gpt-5.5"}'
      )
      "#,
        )
        .execute(pool)
        .await
        .expect("session inserted");

        let assistant_data = if with_message_usage {
            json!({
                "role": "assistant",
                "content": "private assistant answer",
                "cost": 0.1234,
                "model": {
                    "providerID": "openai",
                    "modelID": "gpt-5.5"
                },
                "time": {
                    "created": 1790000000000_i64,
                    "completed": 1790000002500_i64
                },
                "tokens": {
                    "input": 1000,
                    "output": 200,
                    "cache": {
                        "read": 300,
                        "write": 40
                    },
                    "reasoning": 50,
                    "total": 1590
                }
            })
        } else {
            json!({
                "role": "assistant",
                "content": "private assistant answer",
                "model": {
                    "providerID": "openai",
                    "modelID": "gpt-5.5"
                },
                "time": {
                    "created": 1790000000000_i64,
                    "completed": 1790000002500_i64
                }
            })
        };

        query(
            r#"
      INSERT INTO message (
        id,
        session_id,
        time_created,
        time_updated,
        data
      ) VALUES (
        'message_user_1',
        'session_1',
        1790000000000,
        1790000000000,
        ?1
      ), (
        'message_assistant_1',
        'session_1',
        1790000001000,
        1790000002500,
        ?2
      )
      "#,
        )
        .bind(
            json!({
                "role": "user",
                "content": "secret user prompt"
            })
            .to_string(),
        )
        .bind(assistant_data.to_string())
        .execute(pool)
        .await
        .expect("messages inserted");

        query(
            r#"
      INSERT INTO part (
        id,
        message_id,
        session_id,
        time_created,
        time_updated,
        data
      ) VALUES (
        'part_finish_1',
        'message_assistant_1',
        'session_1',
        1790000002000,
        1790000002500,
        ?1
      )
      "#,
        )
        .bind(
            json!({
                "type": "step-finish",
                "cost": 0.045,
                "time": {
                    "start": 1790000002000_i64,
                    "end": 1790000002500_i64
                },
                "tokens": {
                    "input": 700,
                    "output": 80,
                    "cache": {
                        "read": 120,
                        "write": 30
                    },
                    "reasoning": 10,
                    "total": 940
                }
            })
            .to_string(),
        )
        .execute(pool)
        .await
        .expect("part inserted");
    }
}
