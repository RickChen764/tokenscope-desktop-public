use std::path::{Path, PathBuf};

use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{query, query_as};

use crate::db::{NewLlmCall, TokenScopeRepository};

use super::ImportScope;

const HERMES_SOURCE: &str = "hermes_state_sessions";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HermesImportResult {
    pub imported: i64,
    pub skipped: i64,
    pub source_path: String,
}

#[derive(Debug, sqlx::FromRow)]
struct HermesSessionRow {
    id: String,
    source: Option<String>,
    model: Option<String>,
    started_at: Option<f64>,
    ended_at: Option<f64>,
    message_count: Option<i64>,
    tool_call_count: Option<i64>,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    cache_read_tokens: Option<i64>,
    cache_write_tokens: Option<i64>,
    reasoning_tokens: Option<i64>,
    estimated_cost_usd: Option<f64>,
    actual_cost_usd: Option<f64>,
    cost_source: Option<String>,
    api_call_count: Option<i64>,
}

pub fn default_hermes_state_path() -> Result<PathBuf, String> {
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        return Ok(PathBuf::from(local_app_data)
            .join("hermes")
            .join("state.db"));
    }

    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map_err(|_| "unable to resolve user home directory".to_string())?;

    Ok(PathBuf::from(home)
        .join(".local")
        .join("share")
        .join("hermes")
        .join("state.db"))
}

#[allow(dead_code)]
pub async fn import_hermes_sessions_from_path(
    repository: &TokenScopeRepository,
    source_path: &Path,
) -> Result<HermesImportResult, sqlx::Error> {
    import_hermes_sessions_from_path_with_scope(repository, source_path, &ImportScope::full()).await
}

pub async fn import_hermes_sessions_from_path_with_scope(
    repository: &TokenScopeRepository,
    source_path: &Path,
    scope: &ImportScope,
) -> Result<HermesImportResult, sqlx::Error> {
    let options = SqliteConnectOptions::new()
        .filename(source_path)
        .read_only(true);
    let source_pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    let since_seconds = scope
        .since
        .as_ref()
        .map(|timestamp| timestamp.timestamp_millis() as f64 / 1000.0);

    let rows = query_as::<_, HermesSessionRow>(
        r#"
    SELECT
      id,
      source,
      model,
      started_at,
      ended_at,
      message_count,
      tool_call_count,
      input_tokens,
      output_tokens,
      cache_read_tokens,
      cache_write_tokens,
      reasoning_tokens,
      estimated_cost_usd * 1.0 AS estimated_cost_usd,
      actual_cost_usd * 1.0 AS actual_cost_usd,
      cost_source,
      api_call_count
    FROM sessions
    WHERE COALESCE(input_tokens, 0)
      + COALESCE(output_tokens, 0)
      + COALESCE(cache_read_tokens, 0)
      + COALESCE(cache_write_tokens, 0)
      + COALESCE(reasoning_tokens, 0) > 0
      AND (?1 IS NULL OR COALESCE(ended_at, started_at, 0) >= ?1)
    ORDER BY started_at ASC, id ASC
    "#,
    )
    .bind(since_seconds)
    .fetch_all(&source_pool)
    .await?;
    source_pool.close().await;

    let mut imported = 0;
    let mut skipped = 0;
    for row in rows {
        if has_imported(repository, &row.id).await? {
            skipped += 1;
            continue;
        }

        let call = hermes_session_to_call(&row);
        repository.insert_llm_call(&call).await?;
        record_import(repository, &row.id, &call.id).await?;
        imported += 1;
    }

    Ok(HermesImportResult {
        imported,
        skipped,
        source_path: source_path.display().to_string(),
    })
}

async fn has_imported(
    repository: &TokenScopeRepository,
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
    .bind(HERMES_SOURCE)
    .bind(external_id)
    .fetch_optional(repository.pool())
    .await?;

    Ok(existing.is_some())
}

async fn record_import(
    repository: &TokenScopeRepository,
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
    "#,
    )
    .bind(HERMES_SOURCE)
    .bind(external_id)
    .bind(llm_call_id)
    .bind(Local::now().to_rfc3339())
    .execute(repository.pool())
    .await?;

    Ok(())
}

fn hermes_session_to_call(row: &HermesSessionRow) -> NewLlmCall {
    let input_tokens = row.input_tokens.unwrap_or_default().max(0);
    let output_tokens = row.output_tokens.unwrap_or_default().max(0);
    let cache_read_tokens = row.cache_read_tokens.unwrap_or_default().max(0);
    let cache_write_tokens = row.cache_write_tokens.unwrap_or_default().max(0);
    let reasoning_tokens = row.reasoning_tokens.unwrap_or_default().max(0);
    let total_tokens =
        input_tokens + output_tokens + cache_read_tokens + cache_write_tokens + reasoning_tokens;
    let request_count = row.api_call_count.unwrap_or(1).max(1);
    let model = row.model.clone().filter(|value| !value.is_empty());
    let (estimated_cost_usd, cost_source) = imported_cost(row);

    NewLlmCall {
        id: format!("hermes-session-{}", row.id),
        started_at: epoch_seconds_to_local(row.started_at)
            .unwrap_or_else(|| Local::now().to_rfc3339()),
        ended_at: epoch_seconds_to_local(row.ended_at),
        date_local: epoch_seconds_to_date(row.started_at)
            .unwrap_or_else(|| Local::now().date_naive().to_string()),
        provider: "hermes".to_string(),
        provider_config_id: None,
        api_type: Some("hermes_session_import".to_string()),
        model_requested: model.clone(),
        model_response: model,
        agent_id: Some("hermes".to_string()),
        agent_name: Some("Hermes".to_string()),
        agent_run_id: Some(row.id.clone()),
        workflow_id: Some("hermes_session".to_string()),
        workflow_step: row.source.clone().filter(|value| !value.is_empty()),
        session_id: Some(row.id.clone()),
        trace_id: None,
        span_id: None,
        parent_span_id: None,
        project_id: None,
        user_id: None,
        environment: Some("local".to_string()),
        feature: Some("hermes_import".to_string()),
        input_tokens,
        output_tokens,
        cached_input_tokens: cache_read_tokens,
        cache_write_input_tokens: cache_write_tokens,
        reasoning_output_tokens: reasoning_tokens,
        audio_input_tokens: 0,
        audio_output_tokens: 0,
        image_input_tokens: 0,
        image_output_tokens: 0,
        total_tokens,
        total_billable_tokens: total_tokens,
        request_count,
        tool_call_count: row.tool_call_count.unwrap_or_default().max(0),
        retry_count: 0,
        latency_ms: None,
        http_status: None,
        status: "success".to_string(),
        error_type: None,
        error_message: None,
        estimated_cost_usd,
        cost_currency: "USD".to_string(),
        provider_reported_cost_usd: None,
        reconciled_cost_usd: None,
        cost_source: Some(cost_source),
        usage_source: Some("provider_response".to_string()),
        raw_usage_json: Some(
            json!({
              "source": HERMES_SOURCE,
              "session_id": row.id,
              "input_tokens": input_tokens,
              "output_tokens": output_tokens,
              "cache_read_tokens": cache_read_tokens,
              "cache_write_tokens": cache_write_tokens,
              "reasoning_tokens": reasoning_tokens,
              "message_count": row.message_count,
              "tool_call_count": row.tool_call_count,
              "api_call_count": row.api_call_count,
              "estimated_cost_usd": row.estimated_cost_usd,
              "actual_cost_usd": row.actual_cost_usd,
              "cost_source": row.cost_source,
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

fn imported_cost(row: &HermesSessionRow) -> (f64, String) {
    if let Some(actual_cost) = row.actual_cost_usd {
        return (actual_cost, "hermes_actual_cost".to_string());
    }

    if let Some(estimated_cost) = row.estimated_cost_usd {
        return (estimated_cost, "hermes_estimated_cost".to_string());
    }

    (0.0, "hermes_no_cost".to_string())
}

fn epoch_seconds_to_local(value: Option<f64>) -> Option<String> {
    epoch_seconds_to_utc(value).map(|timestamp| timestamp.with_timezone(&Local).to_rfc3339())
}

fn epoch_seconds_to_date(value: Option<f64>) -> Option<String> {
    epoch_seconds_to_utc(value)
        .map(|timestamp| timestamp.with_timezone(&Local).date_naive().to_string())
}

fn epoch_seconds_to_utc(value: Option<f64>) -> Option<DateTime<Utc>> {
    let value = value?;
    if !value.is_finite() {
        return None;
    }

    let seconds = value.floor() as i64;
    let nanos = ((value - seconds as f64) * 1_000_000_000.0).round() as u32;
    DateTime::<Utc>::from_timestamp(seconds, nanos.min(999_999_999))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use chrono::{DateTime, Local, Utc};
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use sqlx::{query, Row};
    use uuid::Uuid;

    use crate::db::TokenScopeRepository;

    use crate::importers::ImportScope;

    use super::{import_hermes_sessions_from_path, import_hermes_sessions_from_path_with_scope};

    #[tokio::test]
    async fn imports_hermes_sessions_without_prompt_or_message_text() {
        let source_path = create_hermes_state_db().await;
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("target repository connects");
        repository.migrate().await.expect("target migrations run");

        let result = import_hermes_sessions_from_path(&repository, &source_path)
            .await
            .expect("hermes import succeeds");

        assert_eq!(result.imported, 1);
        assert_eq!(result.skipped, 0);

        let row = query(
            r#"
      SELECT
        provider,
        api_type,
        model_response,
        input_tokens,
        output_tokens,
        cached_input_tokens,
        cache_write_input_tokens,
        reasoning_output_tokens,
        total_tokens,
        request_count,
        tool_call_count,
        estimated_cost_usd,
        cost_source,
        usage_source,
        raw_usage_json,
        raw_response_json
      FROM llm_call
      WHERE id = 'hermes-session-session_1'
      "#,
        )
        .fetch_one(repository.pool())
        .await
        .expect("imported call exists");

        assert_eq!(row.get::<String, _>("provider"), "hermes");
        assert_eq!(row.get::<String, _>("api_type"), "hermes_session_import");
        assert_eq!(
            row.get::<String, _>("model_response"),
            "MiniMax-M2.7-highspeed"
        );
        assert_eq!(row.get::<i64, _>("input_tokens"), 100);
        assert_eq!(row.get::<i64, _>("output_tokens"), 20);
        assert_eq!(row.get::<i64, _>("cached_input_tokens"), 30);
        assert_eq!(row.get::<i64, _>("cache_write_input_tokens"), 40);
        assert_eq!(row.get::<i64, _>("reasoning_output_tokens"), 5);
        assert_eq!(row.get::<i64, _>("total_tokens"), 195);
        assert_eq!(row.get::<i64, _>("request_count"), 3);
        assert_eq!(row.get::<i64, _>("tool_call_count"), 2);
        assert_eq!(row.get::<f64, _>("estimated_cost_usd"), 0.23);
        assert_eq!(row.get::<String, _>("cost_source"), "hermes_actual_cost");
        assert_eq!(row.get::<String, _>("usage_source"), "provider_response");
        assert_eq!(row.get::<Option<String>, _>("raw_response_json"), None);

        let raw_usage_json = row.get::<String, _>("raw_usage_json");
        assert!(raw_usage_json.contains("\"input_tokens\":100"));
        assert!(!raw_usage_json.contains("secret system prompt"));
        assert!(!raw_usage_json.contains("sensitive title"));
    }

    #[tokio::test]
    async fn import_hermes_sessions_is_idempotent() {
        let source_path = create_hermes_state_db().await;
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("target repository connects");
        repository.migrate().await.expect("target migrations run");

        let first = import_hermes_sessions_from_path(&repository, &source_path)
            .await
            .expect("first import succeeds");
        let second = import_hermes_sessions_from_path(&repository, &source_path)
            .await
            .expect("second import succeeds");

        assert_eq!(first.imported, 1);
        assert_eq!(first.skipped, 0);
        assert_eq!(second.imported, 0);
        assert_eq!(second.skipped, 1);
    }

    #[tokio::test]
    async fn import_hermes_sessions_with_incremental_scope_skips_older_sessions() {
        let source_path = create_hermes_state_db().await;
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("target repository connects");
        repository.migrate().await.expect("target migrations run");
        let since = DateTime::<Utc>::from_timestamp(1791000000, 0)
            .expect("test timestamp is valid")
            .with_timezone(&Local);
        let scope = ImportScope::incremental(Some(since));

        let result = import_hermes_sessions_from_path_with_scope(&repository, &source_path, &scope)
            .await
            .expect("incremental import succeeds");

        assert_eq!(result.imported, 0);
        assert_eq!(result.skipped, 0);
    }

    async fn create_hermes_state_db() -> PathBuf {
        let path = std::env::temp_dir().join(format!("tokenscope-hermes-{}.db", Uuid::new_v4()));
        let _ = fs::remove_file(&path);
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("source db connects");

        query(
            r#"
      CREATE TABLE sessions (
        id TEXT PRIMARY KEY,
        source TEXT,
        user_id TEXT,
        model TEXT,
        model_config TEXT,
        system_prompt TEXT,
        parent_session_id TEXT,
        started_at REAL,
        ended_at REAL,
        end_reason TEXT,
        message_count INTEGER,
        tool_call_count INTEGER,
        input_tokens INTEGER,
        output_tokens INTEGER,
        cache_read_tokens INTEGER,
        cache_write_tokens INTEGER,
        reasoning_tokens INTEGER,
        billing_provider TEXT,
        billing_base_url TEXT,
        billing_mode TEXT,
        estimated_cost_usd REAL,
        actual_cost_usd REAL,
        cost_status TEXT,
        cost_source TEXT,
        pricing_version TEXT,
        title TEXT,
        api_call_count INTEGER,
        handoff_state TEXT,
        handoff_platform TEXT,
        handoff_error TEXT
      )
      "#,
        )
        .execute(&pool)
        .await
        .expect("source schema created");

        query(
            r#"
      INSERT INTO sessions (
        id,
        source,
        model,
        model_config,
        system_prompt,
        started_at,
        ended_at,
        message_count,
        tool_call_count,
        input_tokens,
        output_tokens,
        cache_read_tokens,
        cache_write_tokens,
        reasoning_tokens,
        estimated_cost_usd,
        actual_cost_usd,
        cost_source,
        title,
        api_call_count
      ) VALUES (
        'session_1',
        'cli',
        'MiniMax-M2.7-highspeed',
        'secret model config',
        'secret system prompt',
        1790000000.25,
        1790000300.5,
        4,
        2,
        100,
        20,
        30,
        40,
        5,
        0.12,
        0.23,
        'provider',
        'sensitive title',
        3
      )
      "#,
        )
        .execute(&pool)
        .await
        .expect("source session inserted");
        pool.close().await;

        path
    }
}
