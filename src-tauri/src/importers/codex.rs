use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{query, query_as, Row};

use crate::db::{NewLlmCall, TokenScopeRepository};

use super::ImportScope;

const CODEX_THREAD_SOURCE: &str = "codex_state_threads";
const CODEX_ROLLOUT_SOURCE: &str = "codex_rollout_token_counts";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexImportResult {
    pub imported: i64,
    pub skipped: i64,
    pub source_path: String,
}

#[derive(Debug, sqlx::FromRow)]
struct CodexThreadRow {
    id: String,
    rollout_path: Option<String>,
    created_at_ms: Option<i64>,
    updated_at_ms: Option<i64>,
    model_provider: Option<String>,
    cwd: Option<String>,
    tokens_used: Option<i64>,
    model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RolloutTokenUsage {
    input_tokens: Option<i64>,
    cached_input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    reasoning_output_tokens: Option<i64>,
    total_tokens: Option<i64>,
}

#[derive(Debug)]
struct CodexRolloutTokenCount {
    external_id: String,
    line_number: usize,
    timestamp: String,
    last_token_usage: RolloutTokenUsage,
    total_token_usage: Option<RolloutTokenUsage>,
}

pub fn default_codex_state_path() -> Result<PathBuf, String> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map_err(|_| "unable to resolve user home directory".to_string())?;

    Ok(PathBuf::from(home).join(".codex").join("state_5.sqlite"))
}

pub async fn import_default_codex_threads(
    repository: &TokenScopeRepository,
) -> Result<CodexImportResult, String> {
    let path = default_codex_state_path()?;
    import_codex_threads_from_path(repository, &path)
        .await
        .map_err(|err| err.to_string())
}

pub async fn import_codex_threads_from_path(
    repository: &TokenScopeRepository,
    source_path: &Path,
) -> Result<CodexImportResult, sqlx::Error> {
    import_codex_threads_from_path_with_scope(repository, source_path, &ImportScope::full()).await
}

pub async fn import_codex_threads_from_path_with_scope(
    repository: &TokenScopeRepository,
    source_path: &Path,
    scope: &ImportScope,
) -> Result<CodexImportResult, sqlx::Error> {
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

    let rows = query_as::<_, CodexThreadRow>(
        r#"
    SELECT
      id,
      rollout_path,
      created_at_ms,
      updated_at_ms,
      model_provider,
      cwd,
      tokens_used,
      model
    FROM threads
    WHERE tokens_used IS NOT NULL
      AND tokens_used > 0
      AND (?1 IS NULL OR COALESCE(updated_at_ms, created_at_ms, 0) >= ?1)
    ORDER BY created_at_ms ASC, id ASC
    "#,
    )
    .bind(since_ms)
    .fetch_all(&source_pool)
    .await?;
    source_pool.close().await;

    let mut imported = 0;
    let mut skipped = 0;
    for row in rows {
        let rollout_calls = codex_rollout_to_calls(source_path, &row, scope);
        if !rollout_calls.is_empty() {
            delete_imported_call(repository, CODEX_THREAD_SOURCE, &row.id).await?;

            for (external_id, call) in rollout_calls {
                if has_imported(repository, CODEX_ROLLOUT_SOURCE, &external_id).await? {
                    if should_refresh_imported_call(repository, &call).await? {
                        repository.insert_llm_call(&call).await?;
                        record_import(repository, CODEX_ROLLOUT_SOURCE, &external_id, &call.id)
                            .await?;
                        imported += 1;
                        continue;
                    }

                    skipped += 1;
                    continue;
                }

                repository.insert_llm_call(&call).await?;
                record_import(repository, CODEX_ROLLOUT_SOURCE, &external_id, &call.id).await?;
                imported += 1;
            }

            continue;
        }

        let call = codex_thread_to_call(&row);
        if has_imported(repository, CODEX_THREAD_SOURCE, &row.id).await? {
            if should_refresh_imported_call(repository, &call).await? {
                repository.insert_llm_call(&call).await?;
                record_import(repository, CODEX_THREAD_SOURCE, &row.id, &call.id).await?;
                imported += 1;
                continue;
            }
            skipped += 1;
            continue;
        }

        repository.insert_llm_call(&call).await?;
        record_import(repository, CODEX_THREAD_SOURCE, &row.id, &call.id).await?;
        imported += 1;
    }

    Ok(CodexImportResult {
        imported,
        skipped,
        source_path: source_path.display().to_string(),
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
      reasoning_output_tokens,
      total_tokens,
      agent_id,
      agent_name
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
    let reasoning_output_tokens = existing.try_get::<i64, _>("reasoning_output_tokens")?;
    let total_tokens = existing.try_get::<i64, _>("total_tokens")?;
    let agent_id = existing.try_get::<Option<String>, _>("agent_id")?;
    let agent_name = existing.try_get::<Option<String>, _>("agent_name")?;

    Ok(started_at != call.started_at
        || ended_at != call.ended_at
        || date_local != call.date_local
        || input_tokens != call.input_tokens
        || output_tokens != call.output_tokens
        || cached_input_tokens != call.cached_input_tokens
        || reasoning_output_tokens != call.reasoning_output_tokens
        || total_tokens != call.total_tokens
        || agent_id != call.agent_id
        || agent_name != call.agent_name)
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

async fn delete_imported_call(
    repository: &TokenScopeRepository,
    source: &str,
    external_id: &str,
) -> Result<(), sqlx::Error> {
    let existing = query(
        r#"
    SELECT llm_call_id
    FROM agent_import_map
    WHERE source = ?1 AND external_id = ?2
    LIMIT 1
    "#,
    )
    .bind(source)
    .bind(external_id)
    .fetch_optional(repository.pool())
    .await?;

    let Some(existing) = existing else {
        return Ok(());
    };

    let llm_call_id = existing.try_get::<String, _>("llm_call_id")?;
    query("DELETE FROM agent_import_map WHERE source = ?1 AND external_id = ?2")
        .bind(source)
        .bind(external_id)
        .execute(repository.pool())
        .await?;
    query("DELETE FROM llm_call WHERE id = ?1")
        .bind(llm_call_id)
        .execute(repository.pool())
        .await?;

    Ok(())
}

fn codex_rollout_to_calls(
    source_path: &Path,
    row: &CodexThreadRow,
    scope: &ImportScope,
) -> Vec<(String, NewLlmCall)> {
    let Some(rollout_path) = row.rollout_path.as_deref() else {
        return Vec::new();
    };

    let rollout_path = resolve_rollout_path(source_path, rollout_path);
    let token_counts = read_rollout_token_counts(&rollout_path);
    let mut previous_total_usage: Option<RolloutTokenUsage> = None;
    let mut calls = Vec::new();

    for token_count in token_counts {
        let before_scope = token_count_is_before_scope(&token_count, scope);
        let usage_delta = if let Some(total_token_usage) = token_count.total_token_usage.as_ref() {
            let delta = token_usage_delta(total_token_usage, previous_total_usage.as_ref());
            previous_total_usage = Some(total_token_usage.clone());
            delta
        } else {
            Some(token_count.last_token_usage.clone())
        };

        if before_scope {
            continue;
        }

        let Some(usage_delta) = usage_delta else {
            continue;
        };
        let Some(call) = codex_rollout_token_count_to_call(row, token_count, usage_delta) else {
            continue;
        };
        calls.push(call);
    }

    calls
}

fn token_count_is_before_scope(token_count: &CodexRolloutTokenCount, scope: &ImportScope) -> bool {
    let Some(since) = scope.since.as_ref() else {
        return false;
    };
    let Some(timestamp) = DateTime::parse_from_rfc3339(&token_count.timestamp)
        .ok()
        .map(|timestamp| timestamp.with_timezone(&Local))
    else {
        return false;
    };

    timestamp < since.clone()
}

fn resolve_rollout_path(source_path: &Path, rollout_path: &str) -> PathBuf {
    let path = PathBuf::from(rollout_path);
    if path.is_absolute() {
        return path;
    }

    source_path
        .parent()
        .map(|parent| parent.join(&path))
        .unwrap_or(path)
}

fn read_rollout_token_counts(path: &Path) -> Vec<CodexRolloutTokenCount> {
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };

    BufReader::new(file)
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let line = line.ok()?;
            rollout_line_to_token_count(index + 1, &line)
        })
        .collect()
}

fn rollout_line_to_token_count(line_number: usize, line: &str) -> Option<CodexRolloutTokenCount> {
    if !line.contains("\"token_count\"") {
        return None;
    }

    let value: Value = serde_json::from_str(line).ok()?;
    if value.get("type")?.as_str()? != "event_msg" {
        return None;
    }

    let payload = value.get("payload")?;
    if payload.get("type")?.as_str()? != "token_count" {
        return None;
    }

    let info = payload.get("info")?;
    let last_token_usage: RolloutTokenUsage =
        serde_json::from_value(info.get("last_token_usage")?.clone()).ok()?;
    let total_token_usage = info
        .get("total_token_usage")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok());
    let timestamp = value.get("timestamp")?.as_str()?.to_string();

    Some(CodexRolloutTokenCount {
        external_id: format!("line:{line_number}"),
        line_number,
        timestamp,
        last_token_usage,
        total_token_usage,
    })
}

fn token_usage_delta(
    current_total: &RolloutTokenUsage,
    previous_total: Option<&RolloutTokenUsage>,
) -> Option<RolloutTokenUsage> {
    let Some(previous_total) = previous_total else {
        return token_usage_if_positive(current_total.clone());
    };

    token_usage_if_positive(RolloutTokenUsage {
        input_tokens: delta_token(current_total.input_tokens, previous_total.input_tokens),
        cached_input_tokens: delta_token(
            current_total.cached_input_tokens,
            previous_total.cached_input_tokens,
        ),
        output_tokens: delta_token(current_total.output_tokens, previous_total.output_tokens),
        reasoning_output_tokens: delta_token(
            current_total.reasoning_output_tokens,
            previous_total.reasoning_output_tokens,
        ),
        total_tokens: delta_token(current_total.total_tokens, previous_total.total_tokens),
    })
}

fn delta_token(current: Option<i64>, previous: Option<i64>) -> Option<i64> {
    let current = current?;
    let previous = previous.unwrap_or_default();

    Some((current - previous).max(0))
}

fn token_usage_if_positive(usage: RolloutTokenUsage) -> Option<RolloutTokenUsage> {
    let total_tokens = usage.total_tokens.unwrap_or_else(|| {
        usage.input_tokens.unwrap_or_default().max(0)
            + usage.output_tokens.unwrap_or_default().max(0)
    });

    if total_tokens > 0 {
        return Some(usage);
    }

    None
}

fn codex_rollout_token_count_to_call(
    row: &CodexThreadRow,
    token_count: CodexRolloutTokenCount,
    usage: RolloutTokenUsage,
) -> Option<(String, NewLlmCall)> {
    let timestamp = DateTime::parse_from_rfc3339(&token_count.timestamp)
        .ok()?
        .with_timezone(&Local);
    let timestamp_rfc3339 = timestamp.to_rfc3339();
    let date_local = timestamp.date_naive().to_string();
    let input_tokens = usage.input_tokens.unwrap_or_default().max(0);
    let output_tokens = usage.output_tokens.unwrap_or_default().max(0);
    let cached_input_tokens = usage.cached_input_tokens.unwrap_or_default().max(0);
    let reasoning_output_tokens = usage.reasoning_output_tokens.unwrap_or_default().max(0);
    let total_tokens = usage
        .total_tokens
        .unwrap_or(input_tokens + output_tokens)
        .max(0);
    let billable_input_tokens = input_tokens.saturating_sub(cached_input_tokens);
    let model = row.model.clone().filter(|value| !value.is_empty());
    let external_id = format!("{}:{}", row.id, token_count.external_id);
    let call_id = format!("codex-rollout-{}-{}", row.id, token_count.line_number);

    Some((
        external_id,
        NewLlmCall {
            id: call_id,
            started_at: timestamp_rfc3339.clone(),
            ended_at: Some(timestamp_rfc3339),
            date_local,
            provider: "codex".to_string(),
            provider_config_id: None,
            api_type: Some("codex_rollout_token_count".to_string()),
            model_requested: model.clone(),
            model_response: model,
            agent_id: Some("codex".to_string()),
            agent_name: Some("Codex".to_string()),
            agent_run_id: Some(row.id.clone()),
            workflow_id: Some("codex_rollout".to_string()),
            workflow_step: Some("token_count".to_string()),
            session_id: Some(row.id.clone()),
            trace_id: None,
            span_id: None,
            parent_span_id: None,
            project_id: row.cwd.as_deref().and_then(project_name_from_cwd),
            user_id: None,
            environment: Some("local".to_string()),
            feature: Some("codex_import".to_string()),
            input_tokens,
            output_tokens,
            cached_input_tokens,
            cache_write_input_tokens: 0,
            reasoning_output_tokens,
            audio_input_tokens: 0,
            audio_output_tokens: 0,
            image_input_tokens: 0,
            image_output_tokens: 0,
            total_tokens,
            total_billable_tokens: billable_input_tokens + output_tokens,
            request_count: 1,
            tool_call_count: 0,
            retry_count: 0,
            latency_ms: None,
            http_status: None,
            status: "success".to_string(),
            error_type: None,
            error_message: None,
            estimated_cost_usd: 0.0,
            cost_currency: "USD".to_string(),
            provider_reported_cost_usd: None,
            reconciled_cost_usd: None,
            cost_source: Some("codex_rollout_import_no_cost".to_string()),
            usage_source: Some("codex_rollout_token_count".to_string()),
            raw_usage_json: Some(
                json!({
                  "source": CODEX_ROLLOUT_SOURCE,
                  "thread_id": row.id,
                  "line_number": token_count.line_number,
                  "delta_token_usage": usage,
                  "last_token_usage": token_count.last_token_usage,
                  "total_token_usage": token_count.total_token_usage,
                  "model_provider": row.model_provider,
                  "model": row.model,
                })
                .to_string(),
            ),
            raw_response_json: None,
            request_hash: None,
            response_hash: None,
            prompt_template_id: None,
            created_at: Local::now().to_rfc3339(),
        },
    ))
}

fn codex_thread_to_call(row: &CodexThreadRow) -> NewLlmCall {
    let started_at =
        timestamp_ms_to_local(row.created_at_ms).unwrap_or_else(|| Local::now().to_rfc3339());
    let ended_at = timestamp_ms_to_local(row.updated_at_ms);
    let date_local = timestamp_ms_to_date(row.updated_at_ms.or(row.created_at_ms))
        .unwrap_or_else(|| Local::now().date_naive().to_string());
    let tokens_used = row.tokens_used.unwrap_or_default().max(0);
    let model = row.model.clone().filter(|value| !value.is_empty());

    NewLlmCall {
        id: format!("codex-thread-{}", row.id),
        started_at,
        ended_at,
        date_local,
        provider: "codex".to_string(),
        provider_config_id: None,
        api_type: Some("codex_thread_import".to_string()),
        model_requested: model.clone(),
        model_response: model,
        agent_id: Some("codex".to_string()),
        agent_name: Some("Codex".to_string()),
        agent_run_id: Some(row.id.clone()),
        workflow_id: Some("codex_thread".to_string()),
        workflow_step: None,
        session_id: Some(row.id.clone()),
        trace_id: None,
        span_id: None,
        parent_span_id: None,
        project_id: row.cwd.as_deref().and_then(project_name_from_cwd),
        user_id: None,
        environment: Some("local".to_string()),
        feature: Some("codex_import".to_string()),
        input_tokens: 0,
        output_tokens: 0,
        cached_input_tokens: 0,
        cache_write_input_tokens: 0,
        reasoning_output_tokens: 0,
        audio_input_tokens: 0,
        audio_output_tokens: 0,
        image_input_tokens: 0,
        image_output_tokens: 0,
        total_tokens: tokens_used,
        total_billable_tokens: tokens_used,
        request_count: 1,
        tool_call_count: 0,
        retry_count: 0,
        latency_ms: None,
        http_status: None,
        status: "success".to_string(),
        error_type: None,
        error_message: None,
        estimated_cost_usd: 0.0,
        cost_currency: "USD".to_string(),
        provider_reported_cost_usd: None,
        reconciled_cost_usd: None,
        cost_source: Some("codex_thread_import_no_cost".to_string()),
        usage_source: Some("estimated".to_string()),
        raw_usage_json: Some(
            json!({
              "source": CODEX_THREAD_SOURCE,
              "thread_id": row.id,
              "tokens_used": tokens_used,
              "model_provider": row.model_provider,
              "model": row.model,
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

fn timestamp_ms_to_local(value: Option<i64>) -> Option<String> {
    value
        .and_then(DateTime::<Utc>::from_timestamp_millis)
        .map(|timestamp| timestamp.with_timezone(&Local).to_rfc3339())
}

fn timestamp_ms_to_date(value: Option<i64>) -> Option<String> {
    value
        .and_then(DateTime::<Utc>::from_timestamp_millis)
        .map(|timestamp| timestamp.with_timezone(&Local).date_naive().to_string())
}

fn project_name_from_cwd(cwd: &str) -> Option<String> {
    cwd.replace('\\', "/")
        .rsplit('/')
        .find(|segment| !segment.is_empty())
        .map(ToString::to_string)
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

    use super::{import_codex_threads_from_path, import_codex_threads_from_path_with_scope};

    #[tokio::test]
    async fn imports_codex_threads_without_prompt_or_preview_text() {
        let source_path = create_codex_state_db().await;
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("target repository connects");
        repository.migrate().await.expect("target migrations run");

        let result = import_codex_threads_from_path(&repository, &source_path)
            .await
            .expect("codex import succeeds");

        assert_eq!(result.imported, 1);
        assert_eq!(result.skipped, 0);

        let row = query(
            r#"
      SELECT
        provider,
        api_type,
        model_response,
        project_id,
        total_tokens,
        input_tokens,
        output_tokens,
        estimated_cost_usd,
        cost_source,
        usage_source,
        raw_usage_json,
        raw_response_json
      FROM llm_call
      WHERE id = 'codex-thread-thread_1'
      "#,
        )
        .fetch_one(repository.pool())
        .await
        .expect("imported call exists");

        assert_eq!(row.get::<String, _>("provider"), "codex");
        assert_eq!(row.get::<String, _>("api_type"), "codex_thread_import");
        assert_eq!(row.get::<String, _>("model_response"), "gpt-5.3-codex");
        assert_eq!(row.get::<String, _>("project_id"), "sample-project");
        assert_eq!(row.get::<i64, _>("total_tokens"), 4096);
        assert_eq!(row.get::<i64, _>("input_tokens"), 0);
        assert_eq!(row.get::<i64, _>("output_tokens"), 0);
        assert_eq!(row.get::<f64, _>("estimated_cost_usd"), 0.0);
        assert_eq!(
            row.get::<String, _>("cost_source"),
            "codex_thread_import_no_cost"
        );
        assert_eq!(row.get::<String, _>("usage_source"), "estimated");
        assert_eq!(row.get::<Option<String>, _>("raw_response_json"), None);

        let raw_usage_json = row.get::<String, _>("raw_usage_json");
        assert!(raw_usage_json.contains("\"tokens_used\":4096"));
        assert!(!raw_usage_json.contains("secret prompt"));
        assert!(!raw_usage_json.contains("preview text"));
    }

    #[tokio::test]
    async fn codex_import_collapses_internal_roles_to_codex_agent() {
        let source_path = create_codex_state_db().await;
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("target repository connects");
        repository.migrate().await.expect("target migrations run");

        import_codex_threads_from_path(&repository, &source_path)
            .await
            .expect("codex import succeeds");

        let row = query(
            r#"
      SELECT agent_id, agent_name
      FROM llm_call
      WHERE id = 'codex-thread-thread_1'
      "#,
        )
        .fetch_one(repository.pool())
        .await
        .expect("imported call exists");

        assert_eq!(row.get::<String, _>("agent_id"), "codex");
        assert_eq!(row.get::<String, _>("agent_name"), "Codex");
    }

    #[tokio::test]
    async fn codex_import_refreshes_legacy_agent_labels_on_existing_rows() {
        let source_path = create_codex_state_db().await;
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("target repository connects");
        repository.migrate().await.expect("target migrations run");

        import_codex_threads_from_path(&repository, &source_path)
            .await
            .expect("initial codex import succeeds");
        query(
            r#"
      UPDATE llm_call
      SET agent_id = 'worker', agent_name = 'Builder'
      WHERE id = 'codex-thread-thread_1'
      "#,
        )
        .execute(repository.pool())
        .await
        .expect("legacy agent labels simulated");

        let result = import_codex_threads_from_path(&repository, &source_path)
            .await
            .expect("codex re-import succeeds");
        let row = query(
            r#"
      SELECT agent_id, agent_name
      FROM llm_call
      WHERE id = 'codex-thread-thread_1'
      "#,
        )
        .fetch_one(repository.pool())
        .await
        .expect("imported call exists");

        assert_eq!(result.imported, 1);
        assert_eq!(result.skipped, 0);
        assert_eq!(row.get::<String, _>("agent_id"), "codex");
        assert_eq!(row.get::<String, _>("agent_name"), "Codex");
    }

    #[tokio::test]
    async fn import_codex_threads_is_idempotent() {
        let source_path = create_codex_state_db().await;
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("target repository connects");
        repository.migrate().await.expect("target migrations run");

        let first = import_codex_threads_from_path(&repository, &source_path)
            .await
            .expect("first import succeeds");
        let second = import_codex_threads_from_path(&repository, &source_path)
            .await
            .expect("second import succeeds");

        assert_eq!(first.imported, 1);
        assert_eq!(first.skipped, 0);
        assert_eq!(second.imported, 0);
        assert_eq!(second.skipped, 1);
    }

    #[tokio::test]
    async fn import_codex_threads_with_incremental_scope_skips_older_threads() {
        let source_path = create_codex_state_db().await;
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("target repository connects");
        repository.migrate().await.expect("target migrations run");
        let since = DateTime::<Utc>::from_timestamp_millis(1791000000000)
            .expect("test timestamp is valid")
            .with_timezone(&Local);
        let scope = ImportScope::incremental(Some(since));

        let result = import_codex_threads_from_path_with_scope(&repository, &source_path, &scope)
            .await
            .expect("incremental import succeeds");

        assert_eq!(result.imported, 0);
        assert_eq!(result.skipped, 0);
    }

    #[tokio::test]
    async fn import_codex_threads_refreshes_existing_snapshot_when_thread_updates() {
        let source_path = create_codex_state_db().await;
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("target repository connects");
        repository.migrate().await.expect("target migrations run");

        let first = import_codex_threads_from_path(&repository, &source_path)
            .await
            .expect("first import succeeds");
        update_codex_source_thread(&source_path, 8192, 1790086700000).await;
        let second = import_codex_threads_from_path(&repository, &source_path)
            .await
            .expect("second import succeeds");

        assert_eq!(first.imported, 1);
        assert_eq!(second.imported, 1);
        assert_eq!(second.skipped, 0);

        let row = query(
            r#"
      SELECT total_tokens, date_local, ended_at
      FROM llm_call
      WHERE id = 'codex-thread-thread_1'
      "#,
        )
        .fetch_one(repository.pool())
        .await
        .expect("refreshed call exists");
        let updated_at = DateTime::<Utc>::from_timestamp_millis(1790086700000)
            .expect("test timestamp is valid")
            .with_timezone(&Local);

        assert_eq!(row.get::<i64, _>("total_tokens"), 8192);
        assert_eq!(
            row.get::<String, _>("date_local"),
            updated_at.date_naive().to_string()
        );
        assert_eq!(row.get::<String, _>("ended_at"), updated_at.to_rfc3339());
    }

    #[tokio::test]
    async fn imports_codex_rollout_token_counts_without_prompt_or_response_text() {
        let source_path = create_codex_state_db().await;
        let rollout_path = create_codex_rollout_file();
        set_codex_source_rollout_path(&source_path, &rollout_path).await;
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("target repository connects");
        repository.migrate().await.expect("target migrations run");

        let result = import_codex_threads_from_path(&repository, &source_path)
            .await
            .expect("codex import succeeds");

        assert_eq!(result.imported, 2);
        assert_eq!(result.skipped, 0);

        let aggregate = query(
            r#"
      SELECT
        COUNT(*) AS calls,
        SUM(input_tokens) AS input_tokens,
        SUM(output_tokens) AS output_tokens,
        SUM(cached_input_tokens) AS cached_input_tokens,
        SUM(reasoning_output_tokens) AS reasoning_output_tokens,
        SUM(total_tokens) AS total_tokens
      FROM llm_call
      WHERE api_type = 'codex_rollout_token_count'
      "#,
        )
        .fetch_one(repository.pool())
        .await
        .expect("rollout calls exist");

        assert_eq!(aggregate.get::<i64, _>("calls"), 2);
        assert_eq!(aggregate.get::<i64, _>("input_tokens"), 3000);
        assert_eq!(aggregate.get::<i64, _>("output_tokens"), 700);
        assert_eq!(aggregate.get::<i64, _>("cached_input_tokens"), 1500);
        assert_eq!(aggregate.get::<i64, _>("reasoning_output_tokens"), 120);
        assert_eq!(aggregate.get::<i64, _>("total_tokens"), 3700);

        let row = query(
            r#"
      SELECT
        provider,
        workflow_id,
        usage_source,
        cost_source,
        raw_usage_json,
        raw_response_json
      FROM llm_call
      WHERE id = 'codex-rollout-thread_1-2'
      "#,
        )
        .fetch_one(repository.pool())
        .await
        .expect("first rollout call exists");

        assert_eq!(row.get::<String, _>("provider"), "codex");
        assert_eq!(row.get::<String, _>("workflow_id"), "codex_rollout");
        assert_eq!(
            row.get::<String, _>("usage_source"),
            "codex_rollout_token_count"
        );
        assert_eq!(
            row.get::<String, _>("cost_source"),
            "codex_rollout_import_no_cost"
        );
        assert_eq!(row.get::<Option<String>, _>("raw_response_json"), None);

        let raw_usage_json = row.get::<String, _>("raw_usage_json");
        assert!(raw_usage_json.contains("\"line_number\":2"));
        assert!(!raw_usage_json.contains("secret prompt"));
        assert!(!raw_usage_json.contains("private answer"));
    }

    #[tokio::test]
    async fn incremental_rollout_import_keeps_total_usage_baseline_before_scope() {
        let source_path = create_codex_state_db().await;
        let rollout_path = create_codex_rollout_file();
        set_codex_source_rollout_path(&source_path, &rollout_path).await;
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("target repository connects");
        repository.migrate().await.expect("target migrations run");
        let since = DateTime::parse_from_rfc3339("2026-05-30T16:13:30.000Z")
            .expect("test timestamp is valid")
            .with_timezone(&Local);
        let scope = ImportScope::incremental(Some(since));

        let result = import_codex_threads_from_path_with_scope(&repository, &source_path, &scope)
            .await
            .expect("incremental rollout import succeeds");

        assert_eq!(result.imported, 1);
        assert_eq!(result.skipped, 0);

        let aggregate = query(
            r#"
      SELECT
        COUNT(*) AS calls,
        SUM(input_tokens) AS input_tokens,
        SUM(output_tokens) AS output_tokens,
        SUM(total_tokens) AS total_tokens
      FROM llm_call
      WHERE api_type = 'codex_rollout_token_count'
      "#,
        )
        .fetch_one(repository.pool())
        .await
        .expect("rollout call exists");

        assert_eq!(aggregate.get::<i64, _>("calls"), 1);
        assert_eq!(aggregate.get::<i64, _>("input_tokens"), 2000);
        assert_eq!(aggregate.get::<i64, _>("output_tokens"), 500);
        assert_eq!(aggregate.get::<i64, _>("total_tokens"), 2500);
    }

    #[tokio::test]
    async fn rollout_token_count_import_replaces_legacy_thread_snapshot() {
        let source_path = create_codex_state_db().await;
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("target repository connects");
        repository.migrate().await.expect("target migrations run");

        let first = import_codex_threads_from_path(&repository, &source_path)
            .await
            .expect("snapshot import succeeds");
        let rollout_path = create_codex_rollout_file();
        set_codex_source_rollout_path(&source_path, &rollout_path).await;
        let second = import_codex_threads_from_path(&repository, &source_path)
            .await
            .expect("rollout import succeeds");
        let third = import_codex_threads_from_path(&repository, &source_path)
            .await
            .expect("rollout import is idempotent");

        assert_eq!(first.imported, 1);
        assert_eq!(second.imported, 2);
        assert_eq!(second.skipped, 0);
        assert_eq!(third.imported, 0);
        assert_eq!(third.skipped, 2);

        let legacy_calls: i64 =
            query("SELECT COUNT(*) FROM llm_call WHERE id = 'codex-thread-thread_1'")
                .fetch_one(repository.pool())
                .await
                .expect("legacy count succeeds")
                .get(0);
        let legacy_imports: i64 = query(
            r#"
      SELECT COUNT(*)
      FROM agent_import_map
      WHERE source = 'codex_state_threads' AND external_id = 'thread_1'
      "#,
        )
        .fetch_one(repository.pool())
        .await
        .expect("legacy import count succeeds")
        .get(0);
        let rollout_imports: i64 = query(
            r#"
      SELECT COUNT(*)
      FROM agent_import_map
      WHERE source = 'codex_rollout_token_counts'
      "#,
        )
        .fetch_one(repository.pool())
        .await
        .expect("rollout import count succeeds")
        .get(0);

        assert_eq!(legacy_calls, 0);
        assert_eq!(legacy_imports, 0);
        assert_eq!(rollout_imports, 2);
    }

    async fn create_codex_state_db() -> PathBuf {
        let path = std::env::temp_dir().join(format!("tokenscope-codex-{}.sqlite", Uuid::new_v4()));
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
      CREATE TABLE threads (
        id TEXT PRIMARY KEY,
        rollout_path TEXT,
        created_at INTEGER,
        updated_at INTEGER,
        source TEXT,
        model_provider TEXT,
        cwd TEXT,
        title TEXT,
        sandbox_policy TEXT,
        approval_mode TEXT,
        tokens_used INTEGER,
        has_user_event INTEGER,
        archived INTEGER,
        archived_at INTEGER,
        git_sha TEXT,
        git_branch TEXT,
        git_origin_url TEXT,
        cli_version TEXT,
        first_user_message TEXT,
        agent_nickname TEXT,
        agent_role TEXT,
        memory_mode TEXT,
        model TEXT,
        reasoning_effort TEXT,
        agent_path TEXT,
        created_at_ms INTEGER,
        updated_at_ms INTEGER,
        thread_source TEXT,
        preview TEXT
      )
      "#,
        )
        .execute(&pool)
        .await
        .expect("source schema created");

        query(
            r#"
      INSERT INTO threads (
        id,
        created_at_ms,
        updated_at_ms,
        model_provider,
        cwd,
        tokens_used,
        first_user_message,
        agent_nickname,
        agent_role,
        model,
        preview
      ) VALUES (
        'thread_1',
        1790000000000,
        1790000300000,
        'openai',
        'D:\Project\sample-project',
        4096,
        'secret prompt text',
        'Builder',
        'worker',
        'gpt-5.3-codex',
        'preview text that must not be imported'
      )
      "#,
        )
        .execute(&pool)
        .await
        .expect("source thread inserted");
        pool.close().await;

        path
    }

    fn create_codex_rollout_file() -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("tokenscope-rollout-{}.jsonl", Uuid::new_v4()));
        let content = r#"{"timestamp":"2026-05-30T16:10:00.000Z","type":"session_meta","payload":{"id":"thread_1"}}
{"timestamp":"2026-05-30T16:11:00.000Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":1000,"cached_input_tokens":400,"output_tokens":200,"reasoning_output_tokens":50,"total_tokens":1200},"total_token_usage":{"input_tokens":1000,"cached_input_tokens":400,"output_tokens":200,"reasoning_output_tokens":50,"total_tokens":1200}}}}
{"timestamp":"2026-05-30T16:12:00.000Z","type":"event_msg","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"secret prompt"}]}}
{"timestamp":"2026-05-30T16:13:00.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"private answer"}]}}
{"timestamp":"2026-05-30T16:14:00.000Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":2000,"cached_input_tokens":1100,"output_tokens":500,"reasoning_output_tokens":70,"total_tokens":2500},"total_token_usage":{"input_tokens":3000,"cached_input_tokens":1500,"output_tokens":700,"reasoning_output_tokens":120,"total_tokens":3700}}}}
{"timestamp":"2026-05-30T16:14:01.000Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":2000,"cached_input_tokens":1100,"output_tokens":500,"reasoning_output_tokens":70,"total_tokens":2500},"total_token_usage":{"input_tokens":3000,"cached_input_tokens":1500,"output_tokens":700,"reasoning_output_tokens":120,"total_tokens":3700}}}}
"#;
        fs::write(&path, content).expect("rollout fixture written");
        path
    }

    async fn set_codex_source_rollout_path(source_path: &PathBuf, rollout_path: &PathBuf) {
        let options = SqliteConnectOptions::new().filename(source_path);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("source db reconnects");

        query(
            r#"
      UPDATE threads
      SET rollout_path = ?1
      WHERE id = 'thread_1'
      "#,
        )
        .bind(rollout_path.to_string_lossy().to_string())
        .execute(&pool)
        .await
        .expect("source rollout path updated");
        pool.close().await;
    }

    async fn update_codex_source_thread(path: &PathBuf, tokens_used: i64, updated_at_ms: i64) {
        let options = SqliteConnectOptions::new().filename(path);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("source db reconnects");

        query(
            r#"
      UPDATE threads
      SET tokens_used = ?1, updated_at_ms = ?2
      WHERE id = 'thread_1'
      "#,
        )
        .bind(tokens_used)
        .bind(updated_at_ms)
        .execute(&pool)
        .await
        .expect("source thread updated");
        pool.close().await;
    }
}
