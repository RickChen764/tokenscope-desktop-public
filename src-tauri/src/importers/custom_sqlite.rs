use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

use chrono::{DateTime, Local, Utc};
use serde_json::{json, Map, Value};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteRow};
use sqlx::{query, Column, Row};

use crate::db::{
    CustomImporterMappings, CustomImporterPreview, CustomImporterProfile,
    CustomImporterProfileInput, NewLlmCall, TokenScopeRepository,
};

#[derive(Debug, Clone)]
pub struct CustomSqliteImportOutcome {
    pub imported: i64,
    pub skipped: i64,
}

pub fn validate_profile_input(input: &CustomImporterProfileInput) -> Result<(), String> {
    if input.name.trim().is_empty() {
        return Err("custom importer name is required".to_string());
    }
    validate_source_key(&input.source_key)?;
    if input.database_path.trim().is_empty() {
        return Err("database path is required".to_string());
    }
    validate_import_sql(&input.import_sql)?;
    parse_mappings(&input.mappings_json)?;

    Ok(())
}

pub fn validate_source_key(source_key: &str) -> Result<(), String> {
    let source_key = source_key.trim();
    if source_key.is_empty() {
        return Err("source key is required".to_string());
    }
    if source_key.len() > 80 {
        return Err("source key must be 80 characters or fewer".to_string());
    }
    if !source_key
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == ':')
    {
        return Err(
            "source key only supports letters, numbers, colon, dash, and underscore".into(),
        );
    }

    Ok(())
}

pub fn validate_import_sql(sql: &str) -> Result<String, String> {
    let trimmed = sql.trim();
    if trimmed.is_empty() {
        return Err("import SQL is required".to_string());
    }

    let without_trailing_semicolon = trimmed.trim_end_matches(';').trim();
    if without_trailing_semicolon.contains(';') {
        return Err("import SQL must be a single SELECT statement".to_string());
    }

    let lower = without_trailing_semicolon.to_ascii_lowercase();
    if !(lower.starts_with("select ")
        || lower.starts_with("select\n")
        || lower.starts_with("with "))
    {
        return Err("import SQL must start with SELECT or WITH".to_string());
    }

    let blocked = [
        "insert", "update", "delete", "drop", "alter", "attach", "detach", "vacuum", "pragma",
        "create", "replace",
    ];
    for token in sql_tokens(&lower) {
        if blocked.contains(&token.as_str()) {
            return Err(format!("import SQL cannot use {token}"));
        }
    }

    Ok(without_trailing_semicolon.to_string())
}

pub fn parse_mappings(json: &str) -> Result<CustomImporterMappings, String> {
    let mappings: CustomImporterMappings =
        serde_json::from_str(json).map_err(|err| format!("invalid mappings JSON: {err}"))?;
    if mappings.external_id.trim().is_empty() {
        return Err("mappings.external_id is required".to_string());
    }
    if mappings.started_at.trim().is_empty() {
        return Err("mappings.started_at is required".to_string());
    }

    Ok(mappings)
}

pub async fn preview_custom_sqlite_importer(
    input: &CustomImporterProfileInput,
) -> Result<CustomImporterPreview, String> {
    validate_profile_input(input)?;
    let sql = validate_import_sql(&input.import_sql)?;
    let pool = connect_source_database(&input.database_path).await?;
    let preview_sql = format!("SELECT * FROM ({sql}) AS tokenscope_custom_preview LIMIT 20");
    let rows = query(&preview_sql)
        .fetch_all(&pool)
        .await
        .map_err(|err| err.to_string())?;
    pool.close().await;

    Ok(rows_to_preview(rows))
}

pub async fn import_custom_sqlite_profile(
    repository: &TokenScopeRepository,
    profile: &CustomImporterProfile,
) -> Result<CustomSqliteImportOutcome, String> {
    let sql = validate_import_sql(&profile.import_sql)?;
    let mappings = parse_mappings(&profile.mappings_json)?;
    let pool = connect_source_database(&profile.database_path).await?;
    let rows = query(&sql)
        .fetch_all(&pool)
        .await
        .map_err(|err| err.to_string())?;
    pool.close().await;

    let mut imported = 0;
    let mut skipped = 0;
    for (index, row) in rows.into_iter().enumerate() {
        let object = row_to_object(&row);
        let call = row_object_to_call(profile, &mappings, &object)
            .map_err(|err| format!("row {} import failed: {err}", index + 1))?;
        let external_id = required_string(&object, &mappings.external_id)?;

        if has_imported(repository, &profile.source_key, &external_id).await? {
            if should_refresh_imported_call(repository, &call).await? {
                repository
                    .insert_llm_call(&call)
                    .await
                    .map_err(|err| err.to_string())?;
                record_import(repository, &profile.source_key, &external_id, &call.id).await?;
                imported += 1;
                continue;
            }

            skipped += 1;
            continue;
        }

        repository
            .insert_llm_call(&call)
            .await
            .map_err(|err| err.to_string())?;
        record_import(repository, &profile.source_key, &external_id, &call.id).await?;
        imported += 1;
    }

    Ok(CustomSqliteImportOutcome { imported, skipped })
}

async fn connect_source_database(path: &str) -> Result<sqlx::SqlitePool, String> {
    let path = Path::new(path.trim());
    if !path.exists() {
        return Err(format!("database file does not exist: {}", path.display()));
    }
    if !path.is_file() {
        return Err(format!("database path is not a file: {}", path.display()));
    }

    let options = SqliteConnectOptions::new().filename(path).read_only(true);
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|err| err.to_string())
}

async fn has_imported(
    repository: &TokenScopeRepository,
    source: &str,
    external_id: &str,
) -> Result<bool, String> {
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
    .await
    .map_err(|err| err.to_string())?;

    Ok(existing.is_some())
}

async fn should_refresh_imported_call(
    repository: &TokenScopeRepository,
    call: &NewLlmCall,
) -> Result<bool, String> {
    let existing = query(
        r#"
    SELECT
      started_at,
      ended_at,
      date_local,
      provider,
      model_requested,
      model_response,
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
    .await
    .map_err(|err| err.to_string())?;

    let Some(existing) = existing else {
        return Ok(true);
    };

    let started_at = existing
        .try_get::<String, _>("started_at")
        .map_err(|err| err.to_string())?;
    let ended_at = existing
        .try_get::<Option<String>, _>("ended_at")
        .map_err(|err| err.to_string())?;
    let date_local = existing
        .try_get::<String, _>("date_local")
        .map_err(|err| err.to_string())?;
    let provider = existing
        .try_get::<String, _>("provider")
        .map_err(|err| err.to_string())?;
    let model_requested = existing
        .try_get::<Option<String>, _>("model_requested")
        .map_err(|err| err.to_string())?;
    let model_response = existing
        .try_get::<Option<String>, _>("model_response")
        .map_err(|err| err.to_string())?;
    let input_tokens = existing
        .try_get::<i64, _>("input_tokens")
        .map_err(|err| err.to_string())?;
    let output_tokens = existing
        .try_get::<i64, _>("output_tokens")
        .map_err(|err| err.to_string())?;
    let cached_input_tokens = existing
        .try_get::<i64, _>("cached_input_tokens")
        .map_err(|err| err.to_string())?;
    let cache_write_input_tokens = existing
        .try_get::<i64, _>("cache_write_input_tokens")
        .map_err(|err| err.to_string())?;
    let reasoning_output_tokens = existing
        .try_get::<i64, _>("reasoning_output_tokens")
        .map_err(|err| err.to_string())?;
    let total_tokens = existing
        .try_get::<i64, _>("total_tokens")
        .map_err(|err| err.to_string())?;
    let estimated_cost_usd = existing
        .try_get::<f64, _>("estimated_cost_usd")
        .map_err(|err| err.to_string())?;
    let cost_currency = existing
        .try_get::<String, _>("cost_currency")
        .map_err(|err| err.to_string())?;

    Ok(started_at != call.started_at
        || ended_at != call.ended_at
        || date_local != call.date_local
        || provider != call.provider
        || model_requested != call.model_requested
        || model_response != call.model_response
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
) -> Result<(), String> {
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
    .await
    .map_err(|err| err.to_string())?;

    Ok(())
}

fn rows_to_preview(rows: Vec<SqliteRow>) -> CustomImporterPreview {
    let columns = rows
        .first()
        .map(|row| {
            row.columns()
                .iter()
                .map(|column| column.name().to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let rows = rows
        .into_iter()
        .map(|row| Value::Object(row_to_object(&row)))
        .collect();

    CustomImporterPreview { columns, rows }
}

fn row_to_object(row: &SqliteRow) -> Map<String, Value> {
    let mut object = Map::new();
    for (index, column) in row.columns().iter().enumerate() {
        object.insert(column.name().to_string(), cell_to_value(row, index));
    }

    object
}

fn cell_to_value(row: &SqliteRow, index: usize) -> Value {
    if let Ok(value) = row.try_get::<Option<i64>, _>(index) {
        return value.map(Value::from).unwrap_or(Value::Null);
    }
    if let Ok(value) = row.try_get::<Option<f64>, _>(index) {
        return value.map(Value::from).unwrap_or(Value::Null);
    }
    if let Ok(value) = row.try_get::<Option<String>, _>(index) {
        return value.map(Value::from).unwrap_or(Value::Null);
    }
    if let Ok(value) = row.try_get::<Option<Vec<u8>>, _>(index) {
        return value
            .map(|bytes| Value::from(format!("<{} bytes>", bytes.len())))
            .unwrap_or(Value::Null);
    }

    Value::Null
}

fn row_object_to_call(
    profile: &CustomImporterProfile,
    mappings: &CustomImporterMappings,
    row: &Map<String, Value>,
) -> Result<NewLlmCall, String> {
    let external_id = required_string(row, &mappings.external_id)?;
    let started_at = timestamp_from_value(
        row.get(&mappings.started_at)
            .ok_or_else(|| format!("missing started_at column: {}", mappings.started_at))?,
    )
    .ok_or_else(|| format!("invalid started_at value in column {}", mappings.started_at))?;
    let ended_at =
        optional_column(row, mappings.ended_at.as_deref()).and_then(timestamp_from_value);
    let date_local = optional_column(row, mappings.date_local.as_deref())
        .and_then(value_as_string)
        .unwrap_or_else(|| started_at.date_naive().to_string());
    let provider = optional_column(row, mappings.provider.as_deref())
        .and_then(value_as_string)
        .unwrap_or_else(|| profile.source_key.clone());
    let model = optional_column(row, mappings.model.as_deref()).and_then(value_as_string);
    let model_requested = optional_column(row, mappings.model_requested.as_deref())
        .and_then(value_as_string)
        .or_else(|| model.clone());
    let model_response = optional_column(row, mappings.model_response.as_deref())
        .and_then(value_as_string)
        .or_else(|| model.clone());
    let input_tokens = optional_column(row, mappings.input_tokens.as_deref())
        .and_then(value_as_i64)
        .unwrap_or_default()
        .max(0);
    let output_tokens = optional_column(row, mappings.output_tokens.as_deref())
        .and_then(value_as_i64)
        .unwrap_or_default()
        .max(0);
    let cached_input_tokens = optional_column(row, mappings.cached_input_tokens.as_deref())
        .and_then(value_as_i64)
        .unwrap_or_default()
        .max(0);
    let cache_write_input_tokens =
        optional_column(row, mappings.cache_write_input_tokens.as_deref())
            .and_then(value_as_i64)
            .unwrap_or_default()
            .max(0);
    let reasoning_output_tokens = optional_column(row, mappings.reasoning_output_tokens.as_deref())
        .and_then(value_as_i64)
        .unwrap_or_default()
        .max(0);
    let fallback_total = input_tokens
        + output_tokens
        + cached_input_tokens
        + cache_write_input_tokens
        + reasoning_output_tokens;
    let total_tokens = optional_column(row, mappings.total_tokens.as_deref())
        .and_then(value_as_i64)
        .unwrap_or(fallback_total)
        .max(0);
    let estimated_cost_usd = optional_column(row, mappings.estimated_cost_usd.as_deref())
        .and_then(value_as_f64)
        .unwrap_or_default()
        .max(0.0);
    let cost_currency = optional_column(row, mappings.cost_currency.as_deref())
        .and_then(value_as_string)
        .map(|value| normalize_custom_currency(&value))
        .unwrap_or_else(|| "USD".to_string());
    let cost_source = if mappings.estimated_cost_usd.is_some() {
        "custom_sqlite_mapped_cost"
    } else {
        "custom_sqlite_no_cost"
    };

    Ok(NewLlmCall {
        id: format!(
            "custom-{}-{:016x}",
            safe_identifier(&profile.source_key),
            stable_hash(&external_id)
        ),
        started_at: started_at.to_rfc3339(),
        ended_at: ended_at.map(|value| value.to_rfc3339()),
        date_local,
        provider,
        provider_config_id: None,
        api_type: Some("custom_sqlite_import".to_string()),
        model_requested,
        model_response,
        agent_id: optional_column(row, mappings.agent_id.as_deref()).and_then(value_as_string),
        agent_name: optional_column(row, mappings.agent_name.as_deref()).and_then(value_as_string),
        agent_run_id: None,
        workflow_id: optional_column(row, mappings.workflow_id.as_deref())
            .and_then(value_as_string)
            .or_else(|| Some("custom_sqlite".to_string())),
        workflow_step: optional_column(row, mappings.workflow_step.as_deref())
            .and_then(value_as_string),
        session_id: optional_column(row, mappings.session_id.as_deref()).and_then(value_as_string),
        trace_id: None,
        span_id: None,
        parent_span_id: None,
        project_id: optional_column(row, mappings.project_id.as_deref()).and_then(value_as_string),
        user_id: None,
        environment: Some("local".to_string()),
        feature: Some("custom_importer".to_string()),
        input_tokens,
        output_tokens,
        cached_input_tokens,
        cache_write_input_tokens,
        reasoning_output_tokens,
        audio_input_tokens: 0,
        audio_output_tokens: 0,
        image_input_tokens: 0,
        image_output_tokens: 0,
        total_tokens,
        total_billable_tokens: total_tokens,
        request_count: 1,
        tool_call_count: 0,
        retry_count: 0,
        latency_ms: ended_at.map(|ended_at| {
            ended_at
                .signed_duration_since(started_at)
                .num_milliseconds()
                .max(0)
        }),
        http_status: None,
        status: "success".to_string(),
        error_type: None,
        error_message: None,
        estimated_cost_usd,
        cost_currency,
        provider_reported_cost_usd: mappings.estimated_cost_usd.as_ref().map(|_| estimated_cost_usd),
        reconciled_cost_usd: None,
        cost_source: Some(cost_source.to_string()),
        usage_source: Some("custom_sqlite_mapping".to_string()),
        raw_usage_json: Some(
            json!({
              "source": profile.source_key,
              "profile_id": profile.id,
              "external_id": external_id,
              "session_id": optional_column(row, mappings.session_id.as_deref()).and_then(value_as_string),
              "provider": optional_column(row, mappings.provider.as_deref()).and_then(value_as_string),
              "model": model,
              "tokens": {
                "input": input_tokens,
                "output": output_tokens,
                "cached_input": cached_input_tokens,
                "cache_write_input": cache_write_input_tokens,
                "reasoning_output": reasoning_output_tokens,
                "total": total_tokens,
              },
              "cost": estimated_cost_usd,
            })
            .to_string(),
        ),
        raw_response_json: None,
        request_hash: None,
        response_hash: None,
        prompt_template_id: None,
        created_at: Local::now().to_rfc3339(),
    })
}

fn optional_column<'a>(row: &'a Map<String, Value>, column: Option<&str>) -> Option<&'a Value> {
    let column = column?.trim();
    if column.is_empty() {
        return None;
    }

    row.get(column)
}

fn required_string(row: &Map<String, Value>, column: &str) -> Result<String, String> {
    let value = row
        .get(column.trim())
        .ok_or_else(|| format!("missing required column: {column}"))?;
    value_as_string(value)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("required column is empty: {column}"))
}

fn value_as_string(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn value_as_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Number(value) => value
            .as_i64()
            .or_else(|| value.as_f64().map(|value| value.round() as i64)),
        Value::String(value) => value.parse::<f64>().ok().map(|value| value.round() as i64),
        _ => None,
    }
}

fn value_as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(value) => value.as_f64(),
        Value::String(value) => value.parse::<f64>().ok(),
        _ => None,
    }
    .filter(|value| value.is_finite())
}

fn normalize_custom_currency(value: &str) -> String {
    let currency = value.trim().to_ascii_uppercase();
    if currency.is_empty() {
        "USD".to_string()
    } else {
        currency
    }
}

fn timestamp_from_value(value: &Value) -> Option<DateTime<Local>> {
    match value {
        Value::String(value) => DateTime::parse_from_rfc3339(value)
            .ok()
            .map(|timestamp| timestamp.with_timezone(&Local))
            .or_else(|| value.parse::<f64>().ok().and_then(timestamp_from_epoch)),
        Value::Number(value) => value.as_f64().and_then(timestamp_from_epoch),
        _ => None,
    }
}

fn timestamp_from_epoch(value: f64) -> Option<DateTime<Local>> {
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

fn safe_identifier(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

fn stable_hash(value: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

fn sql_tokens(sql: &str) -> Vec<String> {
    sql.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .filter(|token| !token.is_empty())
        .map(ToString::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use serde_json::json;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use sqlx::{query, Row};
    use uuid::Uuid;

    use crate::db::{CustomImporterProfileInput, TokenScopeRepository};

    use super::{
        import_custom_sqlite_profile, preview_custom_sqlite_importer, validate_import_sql,
    };

    #[test]
    fn validate_import_sql_rejects_write_statements() {
        assert!(validate_import_sql("SELECT id FROM calls").is_ok());
        assert!(
            validate_import_sql("WITH rows AS (SELECT id FROM calls) SELECT * FROM rows").is_ok()
        );
        assert!(validate_import_sql("DELETE FROM calls").is_err());
        assert!(validate_import_sql("SELECT id FROM calls; DROP TABLE calls").is_err());
        assert!(validate_import_sql("PRAGMA table_info(calls)").is_err());
    }

    #[tokio::test]
    async fn preview_custom_sqlite_importer_returns_rows() {
        let source_path = create_source_db().await;
        let input = profile_input(&source_path);

        let preview = preview_custom_sqlite_importer(&input)
            .await
            .expect("preview succeeds");

        assert!(preview.columns.contains(&"id".to_string()));
        assert!(preview.columns.contains(&"started_at".to_string()));
        assert!(preview.columns.contains(&"prompt".to_string()));
        assert_eq!(preview.rows.len(), 1);
    }

    #[tokio::test]
    async fn import_custom_sqlite_profile_is_idempotent_and_sanitizes_raw_usage() {
        let source_path = create_source_db().await;
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("repository connects");
        repository.migrate().await.expect("migrations run");
        let profile = repository
            .upsert_custom_importer_profile(&profile_input(&source_path))
            .await
            .expect("profile saved");

        let first = import_custom_sqlite_profile(&repository, &profile)
            .await
            .expect("first import succeeds");
        let second = import_custom_sqlite_profile(&repository, &profile)
            .await
            .expect("second import succeeds");

        assert_eq!(first.imported, 1);
        assert_eq!(first.skipped, 0);
        assert_eq!(second.imported, 0);
        assert_eq!(second.skipped, 1);

        let row = query(
            r#"
      SELECT
        provider,
        model_response,
        input_tokens,
        output_tokens,
        total_tokens,
        estimated_cost_usd,
        raw_usage_json,
        raw_response_json
      FROM llm_call
      WHERE api_type = 'custom_sqlite_import'
      "#,
        )
        .fetch_one(repository.pool())
        .await
        .expect("imported call exists");

        assert_eq!(row.get::<String, _>("provider"), "custom-agent");
        assert_eq!(row.get::<String, _>("model_response"), "gpt-custom");
        assert_eq!(row.get::<i64, _>("input_tokens"), 100);
        assert_eq!(row.get::<i64, _>("output_tokens"), 25);
        assert_eq!(row.get::<i64, _>("total_tokens"), 125);
        assert!((row.get::<f64, _>("estimated_cost_usd") - 0.01).abs() < f64::EPSILON);
        assert_eq!(row.get::<Option<String>, _>("raw_response_json"), None);

        let raw_usage_json = row.get::<String, _>("raw_usage_json");
        assert!(raw_usage_json.contains("\"source\":\"custom:test-agent\""));
        assert!(!raw_usage_json.contains("secret prompt"));
        assert!(!raw_usage_json.contains("private answer"));
    }

    async fn create_source_db() -> PathBuf {
        let path = std::env::temp_dir().join(format!("tokenscope-custom-{}.db", Uuid::new_v4()));
        let _ = fs::remove_file(&path);
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("source connects");

        query(
            r#"
      CREATE TABLE calls (
        id TEXT PRIMARY KEY,
        started_at INTEGER NOT NULL,
        provider TEXT NOT NULL,
        model TEXT NOT NULL,
        input_tokens INTEGER NOT NULL,
        output_tokens INTEGER NOT NULL,
        total_tokens INTEGER NOT NULL,
        cost_usd REAL NOT NULL,
        prompt TEXT,
        response TEXT
      )
      "#,
        )
        .execute(&pool)
        .await
        .expect("source schema created");
        query(
            r#"
      INSERT INTO calls (
        id,
        started_at,
        provider,
        model,
        input_tokens,
        output_tokens,
        total_tokens,
        cost_usd,
        prompt,
        response
      ) VALUES (
        'call_1',
        1790000000000,
        'custom-agent',
        'gpt-custom',
        100,
        25,
        125,
        0.01,
        'secret prompt',
        'private answer'
      )
      "#,
        )
        .execute(&pool)
        .await
        .expect("source row inserted");
        pool.close().await;

        path
    }

    fn profile_input(source_path: &Path) -> CustomImporterProfileInput {
        CustomImporterProfileInput {
            id: Some("custom-test-profile".to_string()),
            name: "Test Agent".to_string(),
            enabled: true,
            source_key: "custom:test-agent".to_string(),
            database_path: source_path.display().to_string(),
            import_sql: "SELECT id, started_at, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, prompt, response FROM calls".to_string(),
            mappings_json: json!({
                "external_id": "id",
                "started_at": "started_at",
                "provider": "provider",
                "model": "model",
                "input_tokens": "input_tokens",
                "output_tokens": "output_tokens",
                "total_tokens": "total_tokens",
                "estimated_cost_usd": "cost_usd"
            })
            .to_string(),
        }
    }
}
