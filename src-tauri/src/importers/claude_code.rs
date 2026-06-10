use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Local, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{query, Row};

use crate::db::{NewLlmCall, TokenScopeRepository};

use super::ImportScope;

pub const CLAUDE_CODE_TRANSCRIPT_SOURCE: &str = "claude_code_transcripts";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeCodeImportResult {
    pub imported: i64,
    pub skipped: i64,
    pub source_path: String,
}

#[derive(Debug, Clone)]
struct ClaudeCodeTranscriptRecord {
    external_id: String,
    call: NewLlmCall,
}

#[derive(Debug, Clone)]
struct ClaudeCodeUsage {
    input_tokens: i64,
    output_tokens: i64,
    cached_input_tokens: i64,
    cache_write_input_tokens: i64,
    reasoning_output_tokens: i64,
    total_tokens: i64,
}

pub fn default_claude_code_data_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Ok(config_dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        paths.push(PathBuf::from(config_dir));
    }

    if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
        paths.push(PathBuf::from(home).join(".claude"));
    }

    paths
}

pub fn is_candidate_data_path(path: &Path) -> bool {
    if path.is_file() {
        return path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.eq_ignore_ascii_case("jsonl"))
            .unwrap_or(false);
    }

    let projects_dir = if path.file_name().and_then(|name| name.to_str()) == Some("projects") {
        path.to_path_buf()
    } else {
        path.join("projects")
    };

    projects_dir.is_dir() && !transcript_paths_from_root(path).is_empty()
}

#[allow(dead_code)]
pub async fn import_claude_code_usage_from_path(
    repository: &TokenScopeRepository,
    source_path: &Path,
) -> Result<ClaudeCodeImportResult, String> {
    import_claude_code_usage_from_path_with_scope(repository, source_path, &ImportScope::full())
        .await
}

pub async fn import_claude_code_usage_from_path_with_scope(
    repository: &TokenScopeRepository,
    source_path: &Path,
    scope: &ImportScope,
) -> Result<ClaudeCodeImportResult, String> {
    let transcripts = transcript_paths_from_root(source_path);
    let mut imported = 0;
    let mut skipped = 0;

    for transcript_path in transcripts {
        for record in transcript_records_from_file(source_path, &transcript_path, scope)? {
            if has_imported(repository, &record.external_id).await? {
                if should_refresh_imported_call(repository, &record.call).await? {
                    repository
                        .insert_llm_call(&record.call)
                        .await
                        .map_err(|err| err.to_string())?;
                    record_import(repository, &record.external_id, &record.call.id).await?;
                    imported += 1;
                    continue;
                }

                skipped += 1;
                continue;
            }

            repository
                .insert_llm_call(&record.call)
                .await
                .map_err(|err| err.to_string())?;
            record_import(repository, &record.external_id, &record.call.id).await?;
            imported += 1;
        }
    }

    Ok(ClaudeCodeImportResult {
        imported,
        skipped,
        source_path: source_path.display().to_string(),
    })
}

fn transcript_paths_from_root(path: &Path) -> Vec<PathBuf> {
    if path.is_file() {
        return if path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.eq_ignore_ascii_case("jsonl"))
            .unwrap_or(false)
        {
            vec![path.to_path_buf()]
        } else {
            Vec::new()
        };
    }

    let projects_dir = if path.file_name().and_then(|name| name.to_str()) == Some("projects") {
        path.to_path_buf()
    } else {
        path.join("projects")
    };
    let mut paths = Vec::new();
    collect_jsonl_files(&projects_dir, &mut paths);
    paths.sort();
    paths
}

fn collect_jsonl_files(path: &Path, paths: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl_files(&path, paths);
            continue;
        }
        if path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.eq_ignore_ascii_case("jsonl"))
            .unwrap_or(false)
        {
            paths.push(path);
        }
    }
}

fn transcript_records_from_file(
    source_root: &Path,
    path: &Path,
    scope: &ImportScope,
) -> Result<Vec<ClaudeCodeTranscriptRecord>, String> {
    let file = File::open(path).map_err(|err| err.to_string())?;
    let reader = BufReader::new(file);
    let mut records = Vec::new();

    for (index, line) in reader.lines().enumerate() {
        let line = line.map_err(|err| err.to_string())?;
        let Some(record) = transcript_line_to_record(source_root, path, index + 1, &line)? else {
            continue;
        };
        if record_is_before_scope(&record, scope) {
            continue;
        }
        records.push(record);
    }

    Ok(records)
}

fn record_is_before_scope(record: &ClaudeCodeTranscriptRecord, scope: &ImportScope) -> bool {
    let Some(since) = scope.since.as_ref() else {
        return false;
    };
    let Some(started_at) = DateTime::parse_from_rfc3339(&record.call.started_at)
        .ok()
        .map(|timestamp| timestamp.with_timezone(&Local))
    else {
        return false;
    };

    started_at < *since
}

fn transcript_line_to_record(
    source_root: &Path,
    path: &Path,
    line_number: usize,
    line: &str,
) -> Result<Option<ClaudeCodeTranscriptRecord>, String> {
    if line.trim().is_empty() {
        return Ok(None);
    }

    let value: Value = serde_json::from_str(line)
        .map_err(|err| format!("{}:{} invalid JSONL: {err}", path.display(), line_number))?;
    if string_at(&value, &["type"]).as_deref() != Some("assistant") {
        return Ok(None);
    }
    if string_at(&value, &["message", "role"]).as_deref() != Some("assistant") {
        return Ok(None);
    }

    let Some(usage) = usage_from_value(&value) else {
        return Ok(None);
    };

    let session_id = string_at(&value, &["sessionId"])
        .or_else(|| string_at(&value, &["session_id"]))
        .unwrap_or_else(|| session_id_from_path(path));
    let uuid = string_at(&value, &["uuid"])
        .or_else(|| string_at(&value, &["message", "id"]))
        .unwrap_or_else(|| format!("line-{line_number}"));
    let relative_path = path
        .strip_prefix(source_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    let external_id = format!("{relative_path}:{uuid}");
    let call_id = format!("claude-code-{}-{}", session_id, uuid);
    let started_at = timestamp_at(&value, &["timestamp"]).unwrap_or_else(Local::now);
    let duration_ms = int_at(&value, &["durationMs"]).or_else(|| int_at(&value, &["duration_ms"]));
    let ended_at = duration_ms.map(|duration_ms| started_at + Duration::milliseconds(duration_ms));
    let model = string_at(&value, &["message", "model"])
        .or_else(|| string_at(&value, &["model"]))
        .filter(|value| !value.is_empty());
    let cwd = string_at(&value, &["cwd"]);
    let project_id = cwd.as_deref().and_then(project_name_from_path).or_else(|| {
        path.parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            .map(ToString::to_string)
    });
    let cost = number_at(&value, &["costUSD"])
        .or_else(|| number_at(&value, &["cost_usd"]))
        .or_else(|| number_at(&value, &["total_cost_usd"]));
    let cost_source = if cost.is_some() {
        "claude_code_reported_cost"
    } else {
        "claude_code_import_no_cost"
    };

    Ok(Some(ClaudeCodeTranscriptRecord {
        external_id: external_id.clone(),
        call: NewLlmCall {
            id: call_id,
            started_at: started_at.to_rfc3339(),
            ended_at: ended_at.map(|value| value.to_rfc3339()),
            date_local: started_at.date_naive().to_string(),
            provider: "claude-code".to_string(),
            provider_config_id: Some("anthropic".to_string()),
            api_type: Some("claude_code_transcript_import".to_string()),
            model_requested: model.clone(),
            model_response: model.clone(),
            agent_id: Some("claude-code".to_string()),
            agent_name: Some("Claude Code".to_string()),
            agent_run_id: Some(session_id.clone()),
            workflow_id: Some("claude_code_session".to_string()),
            workflow_step: Some("assistant_message".to_string()),
            session_id: Some(session_id.clone()),
            trace_id: None,
            span_id: None,
            parent_span_id: None,
            project_id,
            user_id: None,
            environment: Some("local".to_string()),
            feature: Some("claude_code_import".to_string()),
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
            total_billable_tokens: usage.total_tokens,
            request_count: 1,
            tool_call_count: 0,
            retry_count: 0,
            latency_ms: duration_ms,
            http_status: None,
            status: "success".to_string(),
            error_type: None,
            error_message: None,
            estimated_cost_usd: cost.unwrap_or_default().max(0.0),
            cost_currency: "USD".to_string(),
            provider_reported_cost_usd: cost,
            reconciled_cost_usd: None,
            cost_source: Some(cost_source.to_string()),
            usage_source: Some("claude_code_transcript_usage".to_string()),
            raw_usage_json: Some(
                json!({
                  "source": CLAUDE_CODE_TRANSCRIPT_SOURCE,
                  "external_id": external_id,
                  "session_id": session_id,
                  "message_uuid": uuid,
                  "model": model,
                  "tokens": {
                    "input": usage.input_tokens,
                    "output": usage.output_tokens,
                    "cache_read": usage.cached_input_tokens,
                    "cache_creation": usage.cache_write_input_tokens,
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
        },
    }))
}

async fn has_imported(
    repository: &TokenScopeRepository,
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
    .bind(CLAUDE_CODE_TRANSCRIPT_SOURCE)
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
      model_requested,
      model_response,
      input_tokens,
      output_tokens,
      cached_input_tokens,
      cache_write_input_tokens,
      reasoning_output_tokens,
      total_tokens,
      estimated_cost_usd
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

    Ok(started_at != call.started_at
        || ended_at != call.ended_at
        || date_local != call.date_local
        || model_requested != call.model_requested
        || model_response != call.model_response
        || input_tokens != call.input_tokens
        || output_tokens != call.output_tokens
        || cached_input_tokens != call.cached_input_tokens
        || cache_write_input_tokens != call.cache_write_input_tokens
        || reasoning_output_tokens != call.reasoning_output_tokens
        || total_tokens != call.total_tokens
        || (estimated_cost_usd - call.estimated_cost_usd).abs() > f64::EPSILON)
}

async fn record_import(
    repository: &TokenScopeRepository,
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
    .bind(CLAUDE_CODE_TRANSCRIPT_SOURCE)
    .bind(external_id)
    .bind(llm_call_id)
    .bind(Local::now().to_rfc3339())
    .execute(repository.pool())
    .await
    .map_err(|err| err.to_string())?;

    Ok(())
}

fn usage_from_value(value: &Value) -> Option<ClaudeCodeUsage> {
    let input_tokens = int_at(value, &["message", "usage", "input_tokens"])
        .or_else(|| int_at(value, &["usage", "input_tokens"]))
        .unwrap_or_default()
        .max(0);
    let output_tokens = int_at(value, &["message", "usage", "output_tokens"])
        .or_else(|| int_at(value, &["usage", "output_tokens"]))
        .unwrap_or_default()
        .max(0);
    let cached_input_tokens = int_at(value, &["message", "usage", "cache_read_input_tokens"])
        .or_else(|| int_at(value, &["message", "usage", "cache_read_tokens"]))
        .or_else(|| int_at(value, &["usage", "cache_read_input_tokens"]))
        .unwrap_or_default()
        .max(0);
    let cache_write_input_tokens =
        int_at(value, &["message", "usage", "cache_creation_input_tokens"])
            .or_else(|| int_at(value, &["message", "usage", "cache_write_tokens"]))
            .or_else(|| int_at(value, &["usage", "cache_creation_input_tokens"]))
            .unwrap_or_default()
            .max(0);
    let reasoning_output_tokens = int_at(value, &["message", "usage", "thinking_tokens"])
        .or_else(|| int_at(value, &["message", "usage", "reasoning_tokens"]))
        .or_else(|| int_at(value, &["usage", "thinking_tokens"]))
        .unwrap_or_default()
        .max(0);
    let fallback_total = input_tokens
        + output_tokens
        + cached_input_tokens
        + cache_write_input_tokens
        + reasoning_output_tokens;
    let total_tokens = int_at(value, &["message", "usage", "total_tokens"])
        .or_else(|| int_at(value, &["usage", "total_tokens"]))
        .unwrap_or(fallback_total)
        .max(0);

    if total_tokens <= 0 && fallback_total <= 0 {
        return None;
    }

    Some(ClaudeCodeUsage {
        input_tokens,
        output_tokens,
        cached_input_tokens,
        cache_write_input_tokens,
        reasoning_output_tokens,
        total_tokens,
    })
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
    let value = value.as_str()?;
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

fn project_name_from_path(path: &str) -> Option<String> {
    path.replace('\\', "/")
        .rsplit('/')
        .find(|segment| !segment.is_empty())
        .map(ToString::to_string)
}

fn session_id_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|name| name.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("claude-code-session")
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use chrono::{DateTime, Local, Utc};
    use serde_json::json;
    use sqlx::{query, Row};
    use uuid::Uuid;

    use crate::db::TokenScopeRepository;

    use crate::importers::ImportScope;

    use super::{
        import_claude_code_usage_from_path, import_claude_code_usage_from_path_with_scope,
    };

    #[tokio::test]
    async fn imports_claude_code_transcripts_without_prompt_or_response_text() {
        let source_path = create_claude_code_transcripts();
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("repository connects");
        repository.migrate().await.expect("migrations run");

        let result = import_claude_code_usage_from_path(&repository, &source_path)
            .await
            .expect("claude code import succeeds");

        assert_eq!(result.imported, 1);
        assert_eq!(result.skipped, 0);

        let row = query(
            r#"
      SELECT
        provider,
        provider_config_id,
        api_type,
        model_response,
        agent_id,
        session_id,
        project_id,
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
      WHERE api_type = 'claude_code_transcript_import'
      "#,
        )
        .fetch_one(repository.pool())
        .await
        .expect("imported call exists");

        assert_eq!(row.get::<String, _>("provider"), "claude-code");
        assert_eq!(row.get::<String, _>("provider_config_id"), "anthropic");
        assert_eq!(
            row.get::<String, _>("model_response"),
            "claude-sonnet-4-20250514"
        );
        assert_eq!(row.get::<String, _>("agent_id"), "claude-code");
        assert_eq!(row.get::<String, _>("session_id"), "session_1");
        assert_eq!(row.get::<String, _>("project_id"), "sample-project");
        assert_eq!(row.get::<i64, _>("input_tokens"), 1200);
        assert_eq!(row.get::<i64, _>("output_tokens"), 300);
        assert_eq!(row.get::<i64, _>("cached_input_tokens"), 800);
        assert_eq!(row.get::<i64, _>("cache_write_input_tokens"), 40);
        assert_eq!(row.get::<i64, _>("reasoning_output_tokens"), 12);
        assert_eq!(row.get::<i64, _>("total_tokens"), 2352);
        assert_eq!(row.get::<i64, _>("latency_ms"), 2500);
        assert!((row.get::<f64, _>("estimated_cost_usd") - 0.123).abs() < f64::EPSILON);
        assert!((row.get::<f64, _>("provider_reported_cost_usd") - 0.123).abs() < f64::EPSILON);
        assert_eq!(
            row.get::<String, _>("cost_source"),
            "claude_code_reported_cost"
        );
        assert_eq!(
            row.get::<String, _>("usage_source"),
            "claude_code_transcript_usage"
        );
        assert_eq!(row.get::<Option<String>, _>("raw_response_json"), None);

        let raw_usage_json = row.get::<String, _>("raw_usage_json");
        assert!(raw_usage_json.contains("\"source\":\"claude_code_transcripts\""));
        assert!(!raw_usage_json.contains("secret prompt"));
        assert!(!raw_usage_json.contains("private answer"));
    }

    #[tokio::test]
    async fn import_claude_code_transcripts_is_idempotent() {
        let source_path = create_claude_code_transcripts();
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("repository connects");
        repository.migrate().await.expect("migrations run");

        let first = import_claude_code_usage_from_path(&repository, &source_path)
            .await
            .expect("first import succeeds");
        let second = import_claude_code_usage_from_path(&repository, &source_path)
            .await
            .expect("second import succeeds");

        assert_eq!(first.imported, 1);
        assert_eq!(first.skipped, 0);
        assert_eq!(second.imported, 0);
        assert_eq!(second.skipped, 1);
    }

    #[tokio::test]
    async fn import_claude_code_with_incremental_scope_skips_older_transcripts() {
        let source_path = create_claude_code_transcripts();
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("repository connects");
        repository.migrate().await.expect("migrations run");
        let since = DateTime::<Utc>::from_timestamp_millis(1791000000000)
            .expect("test timestamp is valid")
            .with_timezone(&Local);
        let scope = ImportScope::incremental(Some(since));

        let result =
            import_claude_code_usage_from_path_with_scope(&repository, &source_path, &scope)
                .await
                .expect("incremental import succeeds");

        assert_eq!(result.imported, 0);
        assert_eq!(result.skipped, 0);
    }

    fn create_claude_code_transcripts() -> PathBuf {
        let root = std::env::temp_dir().join(format!("tokenscope-claude-{}", Uuid::new_v4()));
        let project_dir = root.join("projects").join("sample-project");
        fs::create_dir_all(&project_dir).expect("project dir created");
        let transcript_path = project_dir.join("session_1.jsonl");
        let user_line = json!({
            "type": "user",
            "sessionId": "session_1",
            "uuid": "user_1",
            "timestamp": "2026-05-31T10:00:00.000Z",
            "cwd": "D:\\Project\\sample-project",
            "message": {
                "role": "user",
                "content": "secret prompt"
            }
        });
        let assistant_line = json!({
            "type": "assistant",
            "sessionId": "session_1",
            "uuid": "assistant_1",
            "timestamp": "2026-05-31T10:00:02.000Z",
            "cwd": "D:\\Project\\sample-project",
            "durationMs": 2500,
            "costUSD": 0.123,
            "message": {
                "id": "msg_1",
                "type": "message",
                "role": "assistant",
                "model": "claude-sonnet-4-20250514",
                "content": [
                    {
                        "type": "text",
                        "text": "private answer"
                    }
                ],
                "usage": {
                    "input_tokens": 1200,
                    "cache_creation_input_tokens": 40,
                    "cache_read_input_tokens": 800,
                    "output_tokens": 300,
                    "thinking_tokens": 12
                }
            }
        });
        fs::write(
            &transcript_path,
            format!("{}\n{}\n", user_line, assistant_line),
        )
        .expect("transcript written");

        root
    }
}
