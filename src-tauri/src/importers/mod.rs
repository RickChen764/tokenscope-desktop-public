use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::Instant;

use chrono::{DateTime, Duration, Local};
use serde::{Deserialize, Serialize};

use crate::db::TokenScopeRepository;

pub mod claude_code;
pub mod codex;
pub mod custom_sqlite;
pub mod hermes;
pub mod opencode;

type AgentImportFuture<'a> = Pin<Box<dyn Future<Output = AgentImportResult> + Send + 'a>>;

const INCREMENTAL_CURSOR_OVERLAP_MINUTES: i64 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportMode {
    Incremental,
    Full,
}

impl ImportMode {
    pub fn from_option(value: Option<&str>) -> Self {
        match value {
            Some("full") => Self::Full,
            _ => Self::Incremental,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImportScope {
    pub since: Option<DateTime<Local>>,
}

impl ImportScope {
    pub fn full() -> Self {
        Self { since: None }
    }

    pub fn incremental(since: Option<DateTime<Local>>) -> Self {
        Self { since }
    }
}

pub trait AgentImporter: Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn import_supported(&self) -> bool;
    fn source_keys(&self) -> &'static [&'static str];
    fn candidate_paths(&self) -> Result<Vec<PathBuf>, String>;
    fn detected_message(&self) -> &'static str;
    fn missing_message(&self) -> &'static str;

    fn detect(&self) -> LocalAgentStatus {
        match self.candidate_paths() {
            Ok(paths) => status_from_candidates(
                self.id(),
                self.name(),
                paths,
                self.import_supported(),
                self.detected_message(),
                self.missing_message(),
            ),
            Err(err) => missing_status(self.id(), self.name(), self.import_supported(), err),
        }
    }

    fn import<'a>(
        &'a self,
        _repository: &'a TokenScopeRepository,
        status: &'a LocalAgentStatus,
        _scope: &'a ImportScope,
    ) -> AgentImportFuture<'a> {
        Box::pin(async move { AgentImportResult::from_status(status.clone(), 0, 0, None) })
    }
}

struct CodexImporter;
struct ClaudeCodeImporter;
struct HermesImporter;
struct OpenCodeImporter;

static CODEX_IMPORTER: CodexImporter = CodexImporter;
static CLAUDE_CODE_IMPORTER: ClaudeCodeImporter = ClaudeCodeImporter;
static HERMES_IMPORTER: HermesImporter = HermesImporter;
static OPENCODE_IMPORTER: OpenCodeImporter = OpenCodeImporter;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalAgentStatus {
    pub id: String,
    pub name: String,
    pub detected: bool,
    pub import_supported: bool,
    pub source_path: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentImportResult {
    pub id: String,
    pub name: String,
    pub detected: bool,
    pub import_supported: bool,
    pub imported: i64,
    pub skipped: i64,
    pub source_path: Option<String>,
    pub message: String,
    pub status: String,
    pub error: Option<String>,
}

pub fn agent_importers() -> Vec<&'static dyn AgentImporter> {
    vec![
        &CODEX_IMPORTER,
        &HERMES_IMPORTER,
        &OPENCODE_IMPORTER,
        &CLAUDE_CODE_IMPORTER,
    ]
}

pub fn source_keys_for_agent(agent_id: &str) -> &'static [&'static str] {
    agent_importers()
        .into_iter()
        .find(|importer| importer.id() == agent_id)
        .map(|importer| importer.source_keys())
        .unwrap_or(&[])
}

pub fn detect_local_agents() -> Vec<LocalAgentStatus> {
    agent_importers()
        .into_iter()
        .map(AgentImporter::detect)
        .collect()
}

pub async fn import_detected_agents(repository: &TokenScopeRepository) -> Vec<AgentImportResult> {
    import_detected_agents_with_mode(repository, ImportMode::Incremental).await
}

pub async fn import_detected_agents_with_mode(
    repository: &TokenScopeRepository,
    mode: ImportMode,
) -> Vec<AgentImportResult> {
    let mut results = Vec::new();
    for importer in agent_importers() {
        let status = importer.detect();
        if !status.detected {
            results.push(AgentImportResult::from_status(status, 0, 0, None));
            continue;
        }

        if !status.import_supported {
            results.push(AgentImportResult::from_status(status, 0, 0, None));
            continue;
        }

        let cursor_at = Local::now().to_rfc3339();
        let scope = match import_scope(repository, importer.id(), mode).await {
            Ok(scope) => scope,
            Err(err) => {
                results.push(AgentImportResult::failed(
                    status,
                    "Failed to read import cursor.",
                    err,
                ));
                continue;
            }
        };

        let import_started = Instant::now();
        let mut result = importer.import(repository, &status, &scope).await;
        eprintln!(
            "[tokenscope][perf] importer.{} elapsed_ms={} status={} imported={} skipped={} mode={:?}",
            importer.id(),
            import_started.elapsed().as_millis(),
            result.status,
            result.imported,
            result.skipped,
            mode
        );
        if result.status == "success" {
            if let Err(err) = repository
                .save_import_cursor(importer.id(), &cursor_at)
                .await
            {
                result = AgentImportResult::failed(
                    status.clone(),
                    "Failed to save import cursor.",
                    err.to_string(),
                );
            }
        }
        results.push(result);
    }

    results
}

async fn import_scope(
    repository: &TokenScopeRepository,
    source_id: &str,
    mode: ImportMode,
) -> Result<ImportScope, String> {
    if mode == ImportMode::Full {
        return Ok(ImportScope::full());
    }

    let since = repository
        .import_cursor(source_id)
        .await
        .map_err(|err| err.to_string())?
        .as_deref()
        .and_then(import_cursor_to_since);

    Ok(ImportScope::incremental(since))
}

fn import_cursor_to_since(value: &str) -> Option<DateTime<Local>> {
    DateTime::parse_from_rfc3339(value).ok().map(|timestamp| {
        timestamp.with_timezone(&Local) - Duration::minutes(INCREMENTAL_CURSOR_OVERLAP_MINUTES)
    })
}

impl AgentImporter for CodexImporter {
    fn id(&self) -> &'static str {
        "codex"
    }

    fn name(&self) -> &'static str {
        "Codex"
    }

    fn import_supported(&self) -> bool {
        true
    }

    fn source_keys(&self) -> &'static [&'static str] {
        &["codex_rollout_token_counts", "codex_state_threads"]
    }

    fn candidate_paths(&self) -> Result<Vec<PathBuf>, String> {
        codex::default_codex_state_path().map(|path| vec![path])
    }

    fn detected_message(&self) -> &'static str {
        "Codex state database detected."
    }

    fn missing_message(&self) -> &'static str {
        "Codex state database was not found."
    }

    fn import<'a>(
        &'a self,
        repository: &'a TokenScopeRepository,
        status: &'a LocalAgentStatus,
        scope: &'a ImportScope,
    ) -> AgentImportFuture<'a> {
        Box::pin(async move { import_codex(repository, status, scope).await })
    }
}

impl AgentImporter for HermesImporter {
    fn id(&self) -> &'static str {
        "hermes"
    }

    fn name(&self) -> &'static str {
        "Hermes"
    }

    fn import_supported(&self) -> bool {
        true
    }

    fn source_keys(&self) -> &'static [&'static str] {
        &["hermes_state_sessions"]
    }

    fn candidate_paths(&self) -> Result<Vec<PathBuf>, String> {
        hermes::default_hermes_state_path().map(|path| vec![path])
    }

    fn detected_message(&self) -> &'static str {
        "Hermes state database detected."
    }

    fn missing_message(&self) -> &'static str {
        "Hermes state database was not found."
    }

    fn import<'a>(
        &'a self,
        repository: &'a TokenScopeRepository,
        status: &'a LocalAgentStatus,
        scope: &'a ImportScope,
    ) -> AgentImportFuture<'a> {
        Box::pin(async move { import_hermes(repository, status, scope).await })
    }
}

impl AgentImporter for OpenCodeImporter {
    fn id(&self) -> &'static str {
        "opencode"
    }

    fn name(&self) -> &'static str {
        "opencode"
    }

    fn import_supported(&self) -> bool {
        true
    }

    fn source_keys(&self) -> &'static [&'static str] {
        &[
            opencode::OPENCODE_MESSAGE_SOURCE,
            opencode::OPENCODE_PART_SOURCE,
        ]
    }

    fn candidate_paths(&self) -> Result<Vec<PathBuf>, String> {
        Ok(opencode::default_opencode_state_paths()
            .into_iter()
            .filter(|path| opencode::is_candidate_database_file(path))
            .collect())
    }

    fn detected_message(&self) -> &'static str {
        "opencode database detected."
    }

    fn missing_message(&self) -> &'static str {
        "opencode database was not found on this machine."
    }

    fn import<'a>(
        &'a self,
        repository: &'a TokenScopeRepository,
        status: &'a LocalAgentStatus,
        scope: &'a ImportScope,
    ) -> AgentImportFuture<'a> {
        Box::pin(async move { import_opencode(repository, status, scope).await })
    }
}

impl AgentImporter for ClaudeCodeImporter {
    fn id(&self) -> &'static str {
        "claude-code"
    }

    fn name(&self) -> &'static str {
        "Claude Code"
    }

    fn import_supported(&self) -> bool {
        true
    }

    fn source_keys(&self) -> &'static [&'static str] {
        &[claude_code::CLAUDE_CODE_TRANSCRIPT_SOURCE]
    }

    fn candidate_paths(&self) -> Result<Vec<PathBuf>, String> {
        Ok(claude_code::default_claude_code_data_paths()
            .into_iter()
            .filter(|path| claude_code::is_candidate_data_path(path))
            .collect())
    }

    fn detected_message(&self) -> &'static str {
        "Claude Code local transcript data detected."
    }

    fn missing_message(&self) -> &'static str {
        "Claude Code local transcript data was not found on this machine."
    }

    fn import<'a>(
        &'a self,
        repository: &'a TokenScopeRepository,
        status: &'a LocalAgentStatus,
        scope: &'a ImportScope,
    ) -> AgentImportFuture<'a> {
        Box::pin(async move { import_claude_code(repository, status, scope).await })
    }
}

async fn import_codex(
    repository: &TokenScopeRepository,
    status: &LocalAgentStatus,
    scope: &ImportScope,
) -> AgentImportResult {
    let Some(source_path) = status.source_path.as_deref() else {
        return AgentImportResult::from_status(status.clone(), 0, 0, None);
    };

    match codex::import_codex_threads_from_path_with_scope(
        repository,
        Path::new(source_path),
        scope,
    )
    .await
    {
        Ok(result) => AgentImportResult::from_status(
            status.clone(),
            result.imported,
            result.skipped,
            Some(format!(
                "Synced {} Codex usage record(s); skipped {} unchanged record(s).",
                result.imported, result.skipped
            )),
        ),
        Err(err) => {
            AgentImportResult::failed(status.clone(), "Codex import failed.", err.to_string())
        }
    }
}

async fn import_hermes(
    repository: &TokenScopeRepository,
    status: &LocalAgentStatus,
    scope: &ImportScope,
) -> AgentImportResult {
    let Some(source_path) = status.source_path.as_deref() else {
        return AgentImportResult::from_status(status.clone(), 0, 0, None);
    };

    match hermes::import_hermes_sessions_from_path_with_scope(
        repository,
        Path::new(source_path),
        scope,
    )
    .await
    {
        Ok(result) => AgentImportResult::from_status(
            status.clone(),
            result.imported,
            result.skipped,
            Some(format!(
                "Imported {} Hermes session(s); skipped {} already imported session(s).",
                result.imported, result.skipped
            )),
        ),
        Err(err) => {
            AgentImportResult::failed(status.clone(), "Hermes import failed.", err.to_string())
        }
    }
}

async fn import_opencode(
    repository: &TokenScopeRepository,
    status: &LocalAgentStatus,
    scope: &ImportScope,
) -> AgentImportResult {
    let Some(source_path) = status.source_path.as_deref() else {
        return AgentImportResult::from_status(status.clone(), 0, 0, None);
    };

    match opencode::import_opencode_usage_from_path_with_scope(
        repository,
        Path::new(source_path),
        scope,
    )
    .await
    {
        Ok(result) => AgentImportResult::from_status(
            status.clone(),
            result.imported,
            result.skipped,
            Some(format!(
                "Imported {} opencode usage record(s); skipped {} unchanged record(s).",
                result.imported, result.skipped
            )),
        ),
        Err(err) => {
            AgentImportResult::failed(status.clone(), "opencode import failed.", err.to_string())
        }
    }
}

async fn import_claude_code(
    repository: &TokenScopeRepository,
    status: &LocalAgentStatus,
    scope: &ImportScope,
) -> AgentImportResult {
    let Some(source_path) = status.source_path.as_deref() else {
        return AgentImportResult::from_status(status.clone(), 0, 0, None);
    };

    match claude_code::import_claude_code_usage_from_path_with_scope(
        repository,
        Path::new(source_path),
        scope,
    )
    .await
    {
        Ok(result) => AgentImportResult::from_status(
            status.clone(),
            result.imported,
            result.skipped,
            Some(format!(
                "Imported {} Claude Code usage record(s); skipped {} unchanged record(s).",
                result.imported, result.skipped
            )),
        ),
        Err(err) => AgentImportResult::failed(
            status.clone(),
            "Claude Code import failed.",
            err.to_string(),
        ),
    }
}

fn status_from_candidates(
    id: &str,
    name: &str,
    paths: Vec<PathBuf>,
    import_supported: bool,
    found_message: &str,
    missing_message: &str,
) -> LocalAgentStatus {
    if let Some(path) = paths.into_iter().find(|path| path.exists()) {
        return status_from_path(
            id,
            name,
            &path,
            import_supported,
            found_message,
            missing_message,
        );
    }

    missing_status(id, name, import_supported, missing_message.to_string())
}

fn status_from_path(
    id: &str,
    name: &str,
    path: &Path,
    import_supported: bool,
    found_message: &str,
    missing_message: &str,
) -> LocalAgentStatus {
    if path.exists() {
        LocalAgentStatus {
            id: id.to_string(),
            name: name.to_string(),
            detected: true,
            import_supported,
            source_path: Some(path.display().to_string()),
            message: found_message.to_string(),
        }
    } else {
        missing_status(id, name, import_supported, missing_message.to_string())
    }
}

fn missing_status(
    id: &str,
    name: &str,
    import_supported: bool,
    message: String,
) -> LocalAgentStatus {
    LocalAgentStatus {
        id: id.to_string(),
        name: name.to_string(),
        detected: false,
        import_supported,
        source_path: None,
        message,
    }
}

impl AgentImportResult {
    fn from_status(
        status: LocalAgentStatus,
        imported: i64,
        skipped: i64,
        message: Option<String>,
    ) -> Self {
        Self {
            status: import_result_status(&status).to_string(),
            error: None,
            id: status.id,
            name: status.name,
            detected: status.detected,
            import_supported: status.import_supported,
            imported,
            skipped,
            source_path: status.source_path,
            message: message.unwrap_or(status.message),
        }
    }

    fn failed(status: LocalAgentStatus, message: &str, error: String) -> Self {
        Self {
            status: "error".to_string(),
            error: Some(error.clone()),
            id: status.id,
            name: status.name,
            detected: status.detected,
            import_supported: status.import_supported,
            imported: 0,
            skipped: 0,
            source_path: status.source_path,
            message: format!("{message} {error}"),
        }
    }
}

fn import_result_status(status: &LocalAgentStatus) -> &'static str {
    if !status.detected {
        return "missing";
    }
    if !status.import_supported {
        return "unsupported";
    }

    "success"
}

#[cfg(test)]
mod tests {
    use std::fs;

    use uuid::Uuid;

    use super::{agent_importers, status_from_candidates, status_from_path, ImportMode};

    #[test]
    fn agent_importer_registry_exposes_metadata_and_source_keys() {
        let importers = agent_importers();
        let ids = importers
            .iter()
            .map(|importer| importer.id())
            .collect::<Vec<_>>();

        assert_eq!(ids, vec!["codex", "hermes", "opencode", "claude-code"]);

        let codex = importers
            .iter()
            .find(|importer| importer.id() == "codex")
            .expect("codex importer is registered");
        assert_eq!(codex.name(), "Codex");
        assert!(codex.import_supported());
        assert_eq!(
            codex.source_keys(),
            &["codex_rollout_token_counts", "codex_state_threads"]
        );

        let opencode = importers
            .iter()
            .find(|importer| importer.id() == "opencode")
            .expect("opencode importer is registered");
        assert!(opencode.import_supported());
        assert_eq!(
            opencode.source_keys(),
            &["opencode_messages", "opencode_parts"]
        );

        let claude_code = importers
            .iter()
            .find(|importer| importer.id() == "claude-code")
            .expect("claude code importer is registered");
        assert_eq!(claude_code.name(), "Claude Code");
        assert!(claude_code.import_supported());
        assert_eq!(claude_code.source_keys(), &["claude_code_transcripts"]);
    }

    #[test]
    fn status_from_path_marks_existing_supported_agent() {
        let path = std::env::temp_dir().join(format!("tokenscope-agent-{}.db", Uuid::new_v4()));
        fs::write(&path, "").expect("temp db marker created");

        let status = status_from_path(
            "hermes",
            "Hermes",
            &path,
            true,
            "Hermes state database detected.",
            "Hermes state database was not found.",
        );

        assert_eq!(status.id, "hermes");
        assert!(status.detected);
        assert!(status.import_supported);
        assert_eq!(status.source_path, Some(path.display().to_string()));
    }

    #[test]
    fn status_from_candidates_reports_missing_agent() {
        let path = std::env::temp_dir().join(format!("missing-opencode-{}.db", Uuid::new_v4()));

        let status = status_from_candidates(
            "opencode",
            "opencode",
            vec![path],
            true,
            "opencode database detected.",
            "opencode database was not found on this machine.",
        );

        assert_eq!(status.id, "opencode");
        assert!(!status.detected);
        assert!(status.import_supported);
        assert_eq!(status.source_path, None);
    }

    #[test]
    fn import_result_exposes_machine_readable_status_and_error() {
        let status = super::LocalAgentStatus {
            id: "codex".to_string(),
            name: "Codex".to_string(),
            detected: true,
            import_supported: true,
            source_path: Some("state.sqlite".to_string()),
            message: "Codex state database detected.".to_string(),
        };

        let success =
            super::AgentImportResult::from_status(status.clone(), 3, 2, Some("Synced.".into()));
        let failure = super::AgentImportResult::failed(
            status,
            "Codex import failed.",
            "database is locked".to_string(),
        );

        assert_eq!(success.status, "success");
        assert_eq!(success.error, None);
        assert_eq!(failure.status, "error");
        assert_eq!(failure.error.as_deref(), Some("database is locked"));
        assert!(failure.message.contains("Codex import failed."));
        assert!(failure.message.contains("database is locked"));
    }

    #[test]
    fn import_mode_defaults_to_incremental_and_parses_full_refresh() {
        assert_eq!(ImportMode::from_option(None), ImportMode::Incremental);
        assert_eq!(
            ImportMode::from_option(Some("incremental")),
            ImportMode::Incremental
        );
        assert_eq!(ImportMode::from_option(Some("full")), ImportMode::Full);
        assert_eq!(
            ImportMode::from_option(Some("unexpected")),
            ImportMode::Incremental
        );
    }
}
