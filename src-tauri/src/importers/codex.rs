use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{mpsc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration as StdDuration, Instant, SystemTime};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use chrono::{DateTime, Duration as ChronoDuration, Local, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{query, query_as, Row};

use crate::db::{NewLlmCall, TokenScopeRepository};

use super::ImportScope;

const CODEX_THREAD_SOURCE: &str = "codex_state_threads";
const CODEX_ROLLOUT_SOURCE: &str = "codex_rollout_token_counts";
const CODEX_GENERAL_LIMIT_ID: &str = "codex";
const CODEX_APP_SERVER_SOURCE: &str = "codex app-server";
const CODEX_APP_SERVER_RATE_LIMITS_REQUEST_ID: u64 = 2;
const CODEX_APP_SERVER_TIMEOUT: StdDuration = StdDuration::from_secs(4);
#[cfg(windows)]
const WINDOWS_CREATE_NO_WINDOW: u32 = 0x08000000;
const CODEX_GENERAL_LIMIT_MAX_STALENESS_MINUTES: i64 = 15;
const CODEX_USAGE_LIMIT_SCAN_CACHE_TTL: StdDuration = StdDuration::from_secs(45);

static CODEX_USAGE_LIMIT_SCAN_CACHE: OnceLock<Mutex<CodexUsageLimitScanCache>> = OnceLock::new();

fn codex_usage_limit_scan_cache() -> &'static Mutex<CodexUsageLimitScanCache> {
    CODEX_USAGE_LIMIT_SCAN_CACHE.get_or_init(|| {
        Mutex::new(CodexUsageLimitScanCache {
            roots: HashMap::new(),
        })
    })
}

#[derive(Debug, Default)]
struct CodexUsageLimitScanCache {
    roots: HashMap<PathBuf, CodexUsageLimitRootState>,
}

#[derive(Debug, Default)]
struct CodexUsageLimitRootState {
    files: HashMap<PathBuf, CodexUsageLimitFileState>,
    latest_candidates: Vec<(DateTime<Utc>, CodexUsageLimitSnapshot)>,
    latest_general_candidates: Vec<(DateTime<Utc>, CodexUsageLimitSnapshot)>,
    scanned_at: Option<Instant>,
}

#[derive(Debug, Clone)]
struct CodexUsageLimitFileState {
    len: u64,
    modified: Option<SystemTime>,
    line_count: usize,
    latest: Option<(DateTime<Utc>, CodexUsageLimitSnapshot)>,
    latest_general: Option<(DateTime<Utc>, CodexUsageLimitSnapshot)>,
}

#[derive(Debug, Clone, Default)]
struct CodexUsageLimitScanStats {
    files: usize,
    read_files: usize,
    reused_files: usize,
    failed_files: usize,
    total_bytes: u64,
    bytes_read: u64,
    scanned_lines: u64,
    snapshots: u64,
    cache_hits: u64,
}

#[derive(Debug)]
struct CodexUsageLimitFileRead {
    latest: Option<(DateTime<Utc>, CodexUsageLimitSnapshot)>,
    latest_general: Option<(DateTime<Utc>, CodexUsageLimitSnapshot)>,
    scanned_lines: u64,
    snapshots: u64,
}

#[derive(Debug)]
struct CodexUsageLimitRootRead {
    latest_candidates: Vec<(DateTime<Utc>, CodexUsageLimitSnapshot)>,
    latest_general_candidates: Vec<(DateTime<Utc>, CodexUsageLimitSnapshot)>,
    #[cfg(test)]
    stats: CodexUsageLimitScanStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexImportResult {
    pub imported: i64,
    pub skipped: i64,
    pub source_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexUsageLimitSnapshot {
    pub captured_at: String,
    pub source_path: String,
    pub line_number: usize,
    pub limit_id: Option<String>,
    pub limit_name: Option<String>,
    pub plan_type: Option<String>,
    pub rate_limit_reached_type: Option<String>,
    pub primary: CodexUsageLimitWindow,
    pub secondary: CodexUsageLimitWindow,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexUsageLimitWindow {
    pub window_minutes: i64,
    pub used_percent: f64,
    pub remaining_percent: f64,
    pub resets_at: Option<i64>,
    pub resets_at_local: Option<String>,
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

#[derive(Debug, Deserialize)]
struct CodexRawRateLimits {
    limit_id: Option<String>,
    limit_name: Option<String>,
    plan_type: Option<String>,
    rate_limit_reached_type: Option<String>,
    primary: Option<CodexRawRateLimitWindow>,
    secondary: Option<CodexRawRateLimitWindow>,
}

#[derive(Debug, Deserialize)]
struct CodexRawRateLimitWindow {
    window_minutes: Option<i64>,
    used_percent: Option<f64>,
    resets_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct CodexAppServerRateLimitsPayload {
    #[serde(rename = "rateLimits")]
    rate_limits: Option<CodexAppServerRateLimit>,
    #[serde(rename = "rateLimitsByLimitId")]
    rate_limits_by_limit_id: Option<HashMap<String, CodexAppServerRateLimit>>,
}

#[derive(Debug, Clone, Deserialize)]
struct CodexAppServerRateLimit {
    #[serde(rename = "limitId")]
    limit_id: Option<String>,
    #[serde(rename = "limitName")]
    limit_name: Option<String>,
    #[serde(rename = "planType")]
    plan_type: Option<String>,
    #[serde(rename = "rateLimitReachedType")]
    rate_limit_reached_type: Option<String>,
    primary: Option<CodexAppServerRateLimitWindow>,
    secondary: Option<CodexAppServerRateLimitWindow>,
}

#[derive(Debug, Clone, Deserialize)]
struct CodexAppServerRateLimitWindow {
    #[serde(rename = "windowDurationMins")]
    window_duration_mins: Option<i64>,
    #[serde(rename = "usedPercent")]
    used_percent: Option<f64>,
    #[serde(rename = "resetsAt")]
    resets_at: Option<i64>,
}

fn user_home_path() -> Result<PathBuf, String> {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .map_err(|_| "unable to resolve user home directory".to_string())
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn default_codex_home_path() -> Result<PathBuf, String> {
    if let Some(path) = env_path("CODEX_HOME") {
        return Ok(path);
    }

    Ok(user_home_path()?.join(".codex"))
}

fn default_codex_sqlite_home_path() -> Option<PathBuf> {
    env_path("CODEX_SQLITE_HOME")
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|candidate| candidate == &path) {
        paths.push(path);
    }
}

fn codex_state_path_candidates(codex_home: &Path, sqlite_home: Option<&Path>) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(sqlite_home) = sqlite_home {
        push_unique_path(&mut paths, sqlite_home.join("state_5.sqlite"));
    }
    push_unique_path(&mut paths, codex_home.join("sqlite").join("state_5.sqlite"));
    push_unique_path(&mut paths, codex_home.join("state_5.sqlite"));
    paths
}

fn select_default_codex_state_path(paths: &[PathBuf]) -> PathBuf {
    paths
        .iter()
        .find(|path| path.exists())
        .cloned()
        .unwrap_or_else(|| {
            paths
                .first()
                .cloned()
                .unwrap_or_else(|| PathBuf::from("state_5.sqlite"))
        })
}

pub fn default_codex_state_paths() -> Result<Vec<PathBuf>, String> {
    let codex_home = default_codex_home_path()?;
    let sqlite_home = default_codex_sqlite_home_path();
    Ok(codex_state_path_candidates(
        &codex_home,
        sqlite_home.as_deref(),
    ))
}

pub fn default_codex_state_path() -> Result<PathBuf, String> {
    let paths = default_codex_state_paths()?;
    Ok(select_default_codex_state_path(&paths))
}

pub fn default_codex_sessions_path() -> Result<PathBuf, String> {
    Ok(default_codex_home_path()?.join("sessions"))
}

pub fn default_codex_archived_sessions_path() -> Result<PathBuf, String> {
    Ok(default_codex_home_path()?.join("archived_sessions"))
}

pub fn get_default_codex_usage_limits_with_options(
    force_refresh: bool,
) -> Result<Option<CodexUsageLimitSnapshot>, String> {
    match latest_codex_app_server_usage_limits() {
        Ok(Some(snapshot)) => return Ok(Some(snapshot)),
        Ok(None) => {}
        Err(err) => {
            crate::perf_log!(
                "[tokenscope][perf] codex_usage_limits.app_server fallback=true error={}",
                err
            );
        }
    }

    let sessions_path = default_codex_sessions_path()?;
    let archived_sessions_path = default_codex_archived_sessions_path()?;
    latest_codex_usage_limits_from_roots(&[sessions_path, archived_sessions_path], force_refresh)
}

fn latest_codex_app_server_usage_limits() -> Result<Option<CodexUsageLimitSnapshot>, String> {
    if std::env::var("TOKENSCOPE_CODEX_APP_SERVER_DISABLED")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        return Ok(None);
    }

    let command = std::env::var("TOKENSCOPE_CODEX_CLI_PATH")
        .or_else(|_| std::env::var("CODEX_CLI_PATH"))
        .unwrap_or_else(|_| {
            let local_app_data = std::env::var("LOCALAPPDATA").ok().map(PathBuf::from);
            let path_dirs = std::env::var_os("PATH")
                .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
                .unwrap_or_default();
            resolve_default_codex_cli_command(local_app_data.as_deref(), &path_dirs)
        });
    let args = vec!["app-server".to_string()];

    latest_codex_app_server_usage_limits_with_command(&command, &args, CODEX_APP_SERVER_TIMEOUT)
}

fn resolve_default_codex_cli_command(
    local_app_data: Option<&Path>,
    path_dirs: &[PathBuf],
) -> String {
    find_user_local_codex_cli(local_app_data)
        .or_else(|| find_path_codex_cli(path_dirs))
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|| "codex".to_string())
}

fn find_user_local_codex_cli(local_app_data: Option<&Path>) -> Option<PathBuf> {
    let bin_root = local_app_data?.join("OpenAI").join("Codex").join("bin");
    let entries = fs::read_dir(bin_root).ok()?;
    let mut candidates = Vec::new();

    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path().join("codex.exe");
        if path.is_file() {
            candidates.push(path);
        }
    }

    candidates.sort();
    candidates.pop()
}

fn find_path_codex_cli(path_dirs: &[PathBuf]) -> Option<PathBuf> {
    for dir in path_dirs {
        if is_windowsapps_codex_package_resource_dir(dir) {
            continue;
        }

        for file_name in ["codex.exe", "codex.cmd", "codex.bat", "codex"] {
            let path = dir.join(file_name);
            if path.is_file() {
                return Some(path);
            }
        }
    }

    None
}

fn is_windowsapps_codex_package_resource_dir(path: &Path) -> bool {
    let path_text = path.to_string_lossy().to_ascii_lowercase();
    path_text.contains("\\program files\\windowsapps\\openai.codex_")
        && path_text.ends_with("\\app\\resources")
}

fn latest_codex_app_server_usage_limits_with_command(
    command: &str,
    args: &[String],
    timeout: StdDuration,
) -> Result<Option<CodexUsageLimitSnapshot>, String> {
    let started = Instant::now();
    let mut command_builder = Command::new(command);
    configure_codex_app_server_command(&mut command_builder, args);
    let mut child = command_builder
        .spawn()
        .map_err(|err| format!("failed to start codex app-server: {err}"))?;

    let result = (|| {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| "codex app-server stdin is unavailable".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "codex app-server stdout is unavailable".to_string())?;
        let (line_tx, line_rx) = mpsc::channel::<String>();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else {
                    break;
                };
                if line_tx.send(line).is_err() {
                    break;
                }
            }
        });

        let initialize = json!({
            "method": "initialize",
            "id": 1,
            "params": {
                "clientInfo": {
                    "name": "tokenscope_desktop",
                    "title": "TokenScope Desktop",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "capabilities": {
                    "experimentalApi": true
                }
            }
        });
        let initialized = json!({
            "method": "initialized",
            "params": {}
        });
        let rate_limits = json!({
            "method": "account/rateLimits/read",
            "id": CODEX_APP_SERVER_RATE_LIMITS_REQUEST_ID
        });

        for message in [initialize, initialized, rate_limits] {
            writeln!(stdin, "{message}").map_err(|err| err.to_string())?;
        }
        stdin.flush().map_err(|err| err.to_string())?;

        let captured_at = Utc::now();
        loop {
            let elapsed = started.elapsed();
            if elapsed >= timeout {
                return Err("codex app-server timed out while reading rate limits".to_string());
            }

            let remaining = timeout
                .checked_sub(elapsed)
                .unwrap_or_else(|| StdDuration::from_millis(0));
            let line = match line_rx.recv_timeout(remaining) {
                Ok(line) => line,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    return Err("codex app-server timed out while reading rate limits".to_string());
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(None),
            };

            if let Some(snapshot) = codex_app_server_response_line_to_snapshot(
                &line,
                CODEX_APP_SERVER_RATE_LIMITS_REQUEST_ID,
                captured_at,
            )? {
                return Ok(Some(snapshot));
            }
        }
    })();

    let _ = child.kill();
    let _ = child.wait();
    crate::perf_log!(
        "[tokenscope][perf] codex_usage_limits.app_server elapsed_ms={} status={} found={}",
        started.elapsed().as_millis(),
        if result.is_ok() { "ok" } else { "error" },
        matches!(&result, Ok(Some(_)))
    );

    result
}

fn configure_codex_app_server_command(command: &mut Command, args: &[String]) {
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    hide_codex_app_server_console_window(command);
}

#[cfg(windows)]
fn hide_codex_app_server_console_window(command: &mut Command) {
    command.creation_flags(windows_codex_app_server_process_creation_flags());
}

#[cfg(not(windows))]
fn hide_codex_app_server_console_window(_command: &mut Command) {}

#[cfg(windows)]
fn windows_codex_app_server_process_creation_flags() -> u32 {
    WINDOWS_CREATE_NO_WINDOW
}

#[cfg(test)]
pub fn latest_codex_usage_limits_from_sessions_path(
    sessions_path: &Path,
) -> Result<Option<CodexUsageLimitSnapshot>, String> {
    latest_codex_usage_limits_from_sessions_path_with_stats(sessions_path)
        .map(|(snapshot, _)| snapshot)
}

#[cfg(test)]
fn codex_usage_limit_test_now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .expect("fixed test timestamp parses")
        .with_timezone(&Utc)
}

#[cfg(test)]
fn latest_codex_usage_limits_from_sessions_path_with_stats(
    sessions_path: &Path,
) -> Result<(Option<CodexUsageLimitSnapshot>, CodexUsageLimitScanStats), String> {
    let read = read_codex_usage_limit_root(sessions_path, false)?;
    let snapshot = select_codex_usage_limit_snapshot_at(
        read.latest_candidates,
        read.latest_general_candidates,
        codex_usage_limit_test_now(),
    );
    Ok((snapshot, read.stats))
}

#[cfg(test)]
fn latest_codex_usage_limits_from_sessions_path_with_stats_force(
    sessions_path: &Path,
) -> Result<(Option<CodexUsageLimitSnapshot>, CodexUsageLimitScanStats), String> {
    let read = read_codex_usage_limit_root(sessions_path, true)?;
    let snapshot = select_codex_usage_limit_snapshot_at(
        read.latest_candidates,
        read.latest_general_candidates,
        codex_usage_limit_test_now(),
    );
    Ok((snapshot, read.stats))
}

fn latest_codex_usage_limits_from_roots(
    roots: &[PathBuf],
    force_refresh: bool,
) -> Result<Option<CodexUsageLimitSnapshot>, String> {
    latest_codex_usage_limits_from_roots_at(roots, force_refresh, Utc::now())
}

fn latest_codex_usage_limits_from_roots_at(
    roots: &[PathBuf],
    force_refresh: bool,
    now: DateTime<Utc>,
) -> Result<Option<CodexUsageLimitSnapshot>, String> {
    let mut latest_candidates = Vec::new();
    let mut latest_general_candidates = Vec::new();

    for root in roots {
        let read = read_codex_usage_limit_root(root, force_refresh)?;
        latest_candidates.extend(read.latest_candidates);
        latest_general_candidates.extend(read.latest_general_candidates);
    }

    Ok(select_codex_usage_limit_snapshot_at(
        latest_candidates,
        latest_general_candidates,
        now,
    ))
}

fn read_codex_usage_limit_root(
    sessions_path: &Path,
    force_refresh: bool,
) -> Result<CodexUsageLimitRootRead, String> {
    let started = Instant::now();
    if !sessions_path.exists() {
        crate::perf_log!(
            "[tokenscope][perf] codex_usage_limits.scan elapsed_ms={} files=0 bytes=0 lines=0 snapshots=0 found=false missing_path=true",
            started.elapsed().as_millis()
        );
        return Ok(CodexUsageLimitRootRead {
            latest_candidates: Vec::new(),
            latest_general_candidates: Vec::new(),
            #[cfg(test)]
            stats: CodexUsageLimitScanStats::default(),
        });
    }

    let previous_state = {
        let cache = codex_usage_limit_scan_cache()
            .lock()
            .map_err(|err| err.to_string())?;
        cache.roots.get(sessions_path).map(|root| {
            (
                root.files.clone(),
                root.latest_candidates.clone(),
                root.latest_general_candidates.clone(),
                root.scanned_at,
            )
        })
    };
    if !force_refresh {
        if let Some((_, latest_candidates, latest_general_candidates, Some(scanned_at))) =
            previous_state.as_ref()
        {
            if scanned_at.elapsed() <= CODEX_USAGE_LIMIT_SCAN_CACHE_TTL {
                let result = select_codex_usage_limit_snapshot_at(
                    latest_candidates.clone(),
                    latest_general_candidates.clone(),
                    Utc::now(),
                );
                #[cfg(test)]
                let stats = CodexUsageLimitScanStats {
                    cache_hits: 1,
                    ..CodexUsageLimitScanStats::default()
                };
                crate::perf_log!(
                    "[tokenscope][perf] codex_usage_limits.scan elapsed_ms={} files=0 read_files=0 reused_files=0 failed_files=0 bytes=0 bytes_read=0 lines=0 snapshots=0 cache_hits=1 found={}",
                    started.elapsed().as_millis(),
                    result.is_some()
                );
                return Ok(CodexUsageLimitRootRead {
                    latest_candidates: latest_candidates.clone(),
                    latest_general_candidates: latest_general_candidates.clone(),
                    #[cfg(test)]
                    stats,
                });
            }
        }
    }
    let previous_files = previous_state
        .map(|(files, _, _, _)| files)
        .unwrap_or_default();

    let mut rollout_files = Vec::new();
    collect_rollout_jsonl_files(sessions_path, &mut rollout_files)?;
    let mut stats = CodexUsageLimitScanStats {
        files: rollout_files.len(),
        ..CodexUsageLimitScanStats::default()
    };
    let mut next_files = HashMap::new();

    for rollout_path in rollout_files {
        let Ok(metadata) = fs::metadata(&rollout_path) else {
            stats.failed_files += 1;
            continue;
        };
        let len = metadata.len();
        let modified = metadata.modified().ok();
        stats.total_bytes += len;

        if let Some(state) = previous_files.get(&rollout_path) {
            if state.len == len && state.modified == modified {
                stats.reused_files += 1;
                next_files.insert(rollout_path, state.clone());
                continue;
            }
        }

        let previous = previous_files.get(&rollout_path).cloned();
        let append_start = previous
            .as_ref()
            .filter(|state| len > state.len)
            .map(|state| (state.len, state.line_count));
        let read_start = append_start.map(|(offset, _)| offset).unwrap_or(0);
        let read = read_codex_usage_limit_file(
            &rollout_path,
            read_start,
            append_start.map(|(_, line_count)| line_count).unwrap_or(0),
        );
        let Ok(read) = read else {
            stats.failed_files += 1;
            continue;
        };

        let (latest, latest_general, line_count) =
            if let Some((_, previous_line_count)) = append_start {
                let mut latest = previous.as_ref().and_then(|state| state.latest.clone());
                let mut latest_general = previous
                    .as_ref()
                    .and_then(|state| state.latest_general.clone());
                merge_latest_snapshot(&mut latest, read.latest);
                merge_latest_snapshot(&mut latest_general, read.latest_general);
                (
                    latest,
                    latest_general,
                    previous_line_count + read.scanned_lines as usize,
                )
            } else {
                (
                    read.latest,
                    read.latest_general,
                    read.scanned_lines as usize,
                )
            };

        stats.read_files += 1;
        stats.bytes_read += len.saturating_sub(read_start);
        stats.scanned_lines += read.scanned_lines;
        stats.snapshots += read.snapshots;
        next_files.insert(
            rollout_path,
            CodexUsageLimitFileState {
                len,
                modified,
                line_count,
                latest,
                latest_general,
            },
        );
    }

    let mut latest_candidates = Vec::new();
    let mut latest_general_candidates = Vec::new();
    for file_state in next_files.values() {
        if let Some(candidate) = file_state.latest.clone() {
            latest_candidates.push(candidate);
        }
        if let Some(candidate) = file_state.latest_general.clone() {
            latest_general_candidates.push(candidate);
        }
    }

    let result = select_codex_usage_limit_snapshot_at(
        latest_candidates.clone(),
        latest_general_candidates.clone(),
        Utc::now(),
    );
    {
        let mut cache = codex_usage_limit_scan_cache()
            .lock()
            .map_err(|err| err.to_string())?;
        cache.roots.insert(
            sessions_path.to_path_buf(),
            CodexUsageLimitRootState {
                files: next_files,
                latest_candidates: latest_candidates.clone(),
                latest_general_candidates: latest_general_candidates.clone(),
                scanned_at: Some(Instant::now()),
            },
        );
    }
    crate::perf_log!(
        "[tokenscope][perf] codex_usage_limits.scan elapsed_ms={} files={} read_files={} reused_files={} failed_files={} bytes={} bytes_read={} lines={} snapshots={} cache_hits={} found={}",
        started.elapsed().as_millis(),
        stats.files,
        stats.read_files,
        stats.reused_files,
        stats.failed_files,
        stats.total_bytes,
        stats.bytes_read,
        stats.scanned_lines,
        stats.snapshots,
        stats.cache_hits,
        result.is_some()
    );

    Ok(CodexUsageLimitRootRead {
        latest_candidates,
        latest_general_candidates,
        #[cfg(test)]
        stats,
    })
}

fn read_codex_usage_limit_file(
    rollout_path: &Path,
    start_offset: u64,
    line_number_offset: usize,
) -> Result<CodexUsageLimitFileRead, String> {
    let mut file = File::open(rollout_path).map_err(|err| err.to_string())?;
    if start_offset > 0 {
        file.seek(SeekFrom::Start(start_offset))
            .map_err(|err| err.to_string())?;
    }

    let mut latest: Option<(DateTime<Utc>, CodexUsageLimitSnapshot)> = None;
    let mut latest_general: Option<(DateTime<Utc>, CodexUsageLimitSnapshot)> = None;
    let mut scanned_lines = 0;
    let mut snapshots = 0;
    for (index, line) in BufReader::new(file).lines().enumerate() {
        scanned_lines += 1;
        let Ok(line) = line else {
            continue;
        };
        let line_number = line_number_offset + index + 1;
        let Some(snapshot) = rollout_line_to_usage_limit_snapshot(rollout_path, line_number, &line)
        else {
            continue;
        };
        let Ok(captured_at) = DateTime::parse_from_rfc3339(&snapshot.captured_at) else {
            continue;
        };
        let captured_at = captured_at.with_timezone(&Utc);
        snapshots += 1;
        if is_general_codex_usage_limit_snapshot(&snapshot) {
            merge_latest_snapshot(&mut latest_general, Some((captured_at, snapshot.clone())));
        }
        merge_latest_snapshot(&mut latest, Some((captured_at, snapshot)));
    }

    Ok(CodexUsageLimitFileRead {
        latest,
        latest_general,
        scanned_lines,
        snapshots,
    })
}

fn merge_latest_snapshot(
    latest: &mut Option<(DateTime<Utc>, CodexUsageLimitSnapshot)>,
    candidate: Option<(DateTime<Utc>, CodexUsageLimitSnapshot)>,
) {
    let Some(candidate) = candidate else {
        return;
    };

    if latest
        .as_ref()
        .is_none_or(|(latest_at, _)| candidate.0 > *latest_at)
    {
        *latest = Some(candidate);
    }
}

fn select_codex_usage_limit_snapshot_at(
    _latest_candidates: Vec<(DateTime<Utc>, CodexUsageLimitSnapshot)>,
    latest_general_candidates: Vec<(DateTime<Utc>, CodexUsageLimitSnapshot)>,
    now: DateTime<Utc>,
) -> Option<CodexUsageLimitSnapshot> {
    let snapshot = select_general_codex_usage_limit_snapshot(latest_general_candidates)?;
    if codex_usage_limit_snapshot_has_active_window(&snapshot, now) {
        Some(snapshot)
    } else {
        None
    }
}

fn is_general_codex_usage_limit_snapshot(snapshot: &CodexUsageLimitSnapshot) -> bool {
    snapshot.limit_id.as_deref() == Some(CODEX_GENERAL_LIMIT_ID)
}

fn codex_usage_limit_snapshot_has_active_window(
    snapshot: &CodexUsageLimitSnapshot,
    now: DateTime<Utc>,
) -> bool {
    let now_timestamp = now.timestamp();
    [snapshot.primary.resets_at, snapshot.secondary.resets_at]
        .into_iter()
        .any(|resets_at| resets_at.is_none_or(|timestamp| timestamp > now_timestamp))
}

fn select_general_codex_usage_limit_snapshot(
    latest_general_candidates: Vec<(DateTime<Utc>, CodexUsageLimitSnapshot)>,
) -> Option<CodexUsageLimitSnapshot> {
    let (latest_general_at, latest_general_snapshot) = latest_general_candidates
        .iter()
        .max_by_key(|(captured_at, _)| *captured_at)
        .cloned()?;

    let max_staleness = ChronoDuration::minutes(CODEX_GENERAL_LIMIT_MAX_STALENESS_MINUTES);
    let fresh_candidates = latest_general_candidates
        .into_iter()
        .filter(|(captured_at, _)| {
            latest_general_at.signed_duration_since(*captured_at) <= max_staleness
        })
        .collect::<Vec<_>>();

    if fresh_candidates.is_empty() {
        return Some(latest_general_snapshot);
    }

    Some(merge_general_codex_usage_limit_snapshots(
        latest_general_snapshot,
        &fresh_candidates,
    ))
}

fn merge_general_codex_usage_limit_snapshots(
    mut snapshot: CodexUsageLimitSnapshot,
    candidates: &[(DateTime<Utc>, CodexUsageLimitSnapshot)],
) -> CodexUsageLimitSnapshot {
    snapshot.primary = select_conservative_codex_usage_limit_window(
        &snapshot.primary,
        candidates.iter().map(|(_, candidate)| &candidate.primary),
    );
    snapshot.secondary = select_conservative_codex_usage_limit_window(
        &snapshot.secondary,
        candidates.iter().map(|(_, candidate)| &candidate.secondary),
    );
    snapshot
}

fn select_conservative_codex_usage_limit_window<'a>(
    base_window: &CodexUsageLimitWindow,
    windows: impl Iterator<Item = &'a CodexUsageLimitWindow>,
) -> CodexUsageLimitWindow {
    let same_window = windows
        .filter(|window| window.window_minutes == base_window.window_minutes)
        .collect::<Vec<_>>();
    let latest_reset = same_window
        .iter()
        .filter_map(|window| window.resets_at)
        .max()
        .or(base_window.resets_at);
    let mut selected = same_window
        .iter()
        .find(|window| latest_reset.is_none() || window.resets_at == latest_reset)
        .map(|window| (*window).clone())
        .unwrap_or_else(|| base_window.clone());

    for window in same_window {
        if latest_reset.is_some() && window.resets_at != latest_reset {
            continue;
        }

        if window.used_percent > selected.used_percent {
            selected = window.clone();
        }
    }

    selected.used_percent = selected.used_percent.clamp(0.0, 100.0);
    selected.remaining_percent = (100.0 - selected.used_percent).clamp(0.0, 100.0);
    selected
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

    let thread_columns = table_columns(&source_pool, "threads").await?;
    let created_at_ms_expr =
        timestamp_ms_expression(&thread_columns, "created_at_ms", "created_at");
    let updated_at_ms_expr =
        timestamp_ms_expression(&thread_columns, "updated_at_ms", "updated_at");
    let rollout_path_expr = optional_column_expression(&thread_columns, "rollout_path");
    let model_provider_expr = optional_column_expression(&thread_columns, "model_provider");
    let cwd_expr = optional_column_expression(&thread_columns, "cwd");
    let tokens_used_expr = optional_column_expression(&thread_columns, "tokens_used");
    let model_expr = optional_column_expression(&thread_columns, "model");
    let filter_timestamp_expr = format!(
        "COALESCE({updated_at_ms_expr}, {created_at_ms_expr}, 0)",
        updated_at_ms_expr = updated_at_ms_expr,
        created_at_ms_expr = created_at_ms_expr
    );
    let thread_query = format!(
        r#"
    SELECT
      id,
      {rollout_path_expr} AS rollout_path,
      {created_at_ms_expr} AS created_at_ms,
      {updated_at_ms_expr} AS updated_at_ms,
      {model_provider_expr} AS model_provider,
      {cwd_expr} AS cwd,
      {tokens_used_expr} AS tokens_used,
      {model_expr} AS model
    FROM threads
    WHERE {tokens_used_expr} IS NOT NULL
      AND {tokens_used_expr} > 0
      AND (?1 IS NULL OR {filter_timestamp_expr} >= ?1)
    ORDER BY {created_at_ms_expr} ASC, id ASC
    "#,
        rollout_path_expr = rollout_path_expr,
        created_at_ms_expr = created_at_ms_expr,
        updated_at_ms_expr = updated_at_ms_expr,
        model_provider_expr = model_provider_expr,
        cwd_expr = cwd_expr,
        tokens_used_expr = tokens_used_expr,
        model_expr = model_expr,
        filter_timestamp_expr = filter_timestamp_expr,
    );

    let rows = query_as::<_, CodexThreadRow>(&thread_query)
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

async fn table_columns(
    pool: &sqlx::SqlitePool,
    table_name: &str,
) -> Result<HashSet<String>, sqlx::Error> {
    let rows = query(&format!("PRAGMA table_info({table_name})"))
        .fetch_all(pool)
        .await?;

    rows.into_iter()
        .map(|row| row.try_get::<String, _>("name"))
        .collect()
}

fn optional_column_expression(columns: &HashSet<String>, column_name: &str) -> String {
    if columns.contains(column_name) {
        column_name.to_string()
    } else {
        "NULL".to_string()
    }
}

fn timestamp_ms_expression(
    columns: &HashSet<String>,
    ms_column_name: &str,
    fallback_column_name: &str,
) -> String {
    if columns.contains(ms_column_name) {
        return ms_column_name.to_string();
    }

    if columns.contains(fallback_column_name) {
        return format!(
            "CASE WHEN {column} IS NULL THEN NULL \
             WHEN ABS({column}) >= 10000000000 THEN {column} \
             ELSE {column} * 1000 END",
            column = fallback_column_name
        );
    }

    "NULL".to_string()
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

    timestamp < *since
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
    let started = Instant::now();
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };

    let counts = BufReader::new(file)
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let line = line.ok()?;
            rollout_line_to_token_count(index + 1, &line)
        })
        .collect::<Vec<_>>();
    let elapsed_ms = started.elapsed().as_millis();
    if elapsed_ms >= 50 {
        let bytes = fs::metadata(path)
            .ok()
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        crate::perf_log!(
            "[tokenscope][perf] codex_rollout_token_counts.read elapsed_ms={} bytes={} token_counts={} path={}",
            elapsed_ms,
            bytes,
            counts.len(),
            path.display()
        );
    }
    counts
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

fn rollout_line_to_usage_limit_snapshot(
    source_path: &Path,
    line_number: usize,
    line: &str,
) -> Option<CodexUsageLimitSnapshot> {
    if !line.contains("\"token_count\"") || !line.contains("\"rate_limits\"") {
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

    let rate_limits: CodexRawRateLimits =
        serde_json::from_value(payload.get("rate_limits")?.clone()).ok()?;
    let primary = codex_raw_rate_limit_window_to_snapshot(rate_limits.primary?)?;
    let secondary = codex_raw_rate_limit_window_to_snapshot(rate_limits.secondary?)?;
    let captured_at = value.get("timestamp")?.as_str()?.to_string();

    Some(CodexUsageLimitSnapshot {
        captured_at,
        source_path: source_path.to_string_lossy().to_string(),
        line_number,
        limit_id: rate_limits.limit_id,
        limit_name: rate_limits.limit_name,
        plan_type: rate_limits.plan_type,
        rate_limit_reached_type: rate_limits.rate_limit_reached_type,
        primary,
        secondary,
    })
}

fn codex_app_server_rate_limits_to_snapshot(
    value: &Value,
    captured_at: DateTime<Utc>,
) -> Option<CodexUsageLimitSnapshot> {
    let payload: CodexAppServerRateLimitsPayload = serde_json::from_value(value.clone()).ok()?;
    let rate_limit = payload
        .rate_limits_by_limit_id
        .as_ref()
        .and_then(|rate_limits| rate_limits.get(CODEX_GENERAL_LIMIT_ID))
        .or_else(|| {
            payload
                .rate_limits
                .as_ref()
                .filter(|rate_limit| rate_limit.limit_id.as_deref() == Some(CODEX_GENERAL_LIMIT_ID))
        })?;
    let primary = codex_app_server_rate_limit_window_to_snapshot(rate_limit.primary.clone()?)?;
    let secondary = codex_app_server_rate_limit_window_to_snapshot(rate_limit.secondary.clone()?)?;

    Some(CodexUsageLimitSnapshot {
        captured_at: captured_at.to_rfc3339(),
        source_path: CODEX_APP_SERVER_SOURCE.to_string(),
        line_number: 0,
        limit_id: Some(CODEX_GENERAL_LIMIT_ID.to_string()),
        limit_name: rate_limit.limit_name.clone(),
        plan_type: rate_limit.plan_type.clone(),
        rate_limit_reached_type: rate_limit.rate_limit_reached_type.clone(),
        primary,
        secondary,
    })
}

fn codex_app_server_response_line_to_snapshot(
    line: &str,
    request_id: u64,
    captured_at: DateTime<Utc>,
) -> Result<Option<CodexUsageLimitSnapshot>, String> {
    let value: Value = serde_json::from_str(line).map_err(|err| err.to_string())?;
    if value.get("id").and_then(Value::as_u64) != Some(request_id) {
        return Ok(None);
    }

    if let Some(error) = value.get("error") {
        return Err(format!("codex app-server returned error: {error}"));
    }

    let Some(result) = value.get("result") else {
        return Ok(None);
    };

    Ok(codex_app_server_rate_limits_to_snapshot(
        result,
        captured_at,
    ))
}

fn codex_app_server_rate_limit_window_to_snapshot(
    window: CodexAppServerRateLimitWindow,
) -> Option<CodexUsageLimitWindow> {
    let window_minutes = window.window_duration_mins?;
    let used_percent = window.used_percent?;
    let remaining_percent = (100.0 - used_percent).clamp(0.0, 100.0);
    let resets_at_local = window.resets_at.and_then(|timestamp| {
        DateTime::from_timestamp(timestamp, 0)
            .map(|datetime| datetime.with_timezone(&Local).to_rfc3339())
    });

    Some(CodexUsageLimitWindow {
        window_minutes,
        used_percent,
        remaining_percent,
        resets_at: window.resets_at,
        resets_at_local,
    })
}

fn codex_raw_rate_limit_window_to_snapshot(
    window: CodexRawRateLimitWindow,
) -> Option<CodexUsageLimitWindow> {
    let window_minutes = window.window_minutes?;
    let used_percent = window.used_percent?;
    let remaining_percent = (100.0 - used_percent).clamp(0.0, 100.0);
    let resets_at_local = window.resets_at.and_then(|timestamp| {
        DateTime::from_timestamp(timestamp, 0)
            .map(|datetime| datetime.with_timezone(&Local).to_rfc3339())
    });

    Some(CodexUsageLimitWindow {
        window_minutes,
        used_percent,
        remaining_percent,
        resets_at: window.resets_at,
        resets_at_local,
    })
}

fn collect_rollout_jsonl_files(root: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(root).map_err(|err| err.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|err| err.to_string())?;
        if file_type.is_dir() {
            collect_rollout_jsonl_files(&path, files)?;
        } else if file_type.is_file()
            && path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("jsonl"))
        {
            files.push(path);
        }
    }

    Ok(())
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
    #![allow(clippy::too_many_arguments)]

    use std::fs;
    use std::io::Write;
    use std::path::{Path, PathBuf};

    use chrono::{DateTime, Local, Utc};
    use serde_json::json;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use sqlx::{query, Row};
    use std::time::Duration as StdDuration;
    use uuid::Uuid;

    use crate::db::TokenScopeRepository;

    use crate::importers::ImportScope;

    use super::{
        codex_app_server_rate_limits_to_snapshot, codex_app_server_response_line_to_snapshot,
        codex_state_path_candidates, codex_usage_limit_test_now, import_codex_threads_from_path,
        import_codex_threads_from_path_with_scope,
        latest_codex_app_server_usage_limits_with_command, latest_codex_usage_limits_from_roots,
        latest_codex_usage_limits_from_roots_at, latest_codex_usage_limits_from_sessions_path,
        latest_codex_usage_limits_from_sessions_path_with_stats,
        latest_codex_usage_limits_from_sessions_path_with_stats_force,
        resolve_default_codex_cli_command, rollout_line_to_usage_limit_snapshot,
        select_default_codex_state_path, windows_codex_app_server_process_creation_flags,
    };

    #[test]
    fn codex_state_path_candidates_prefer_sqlite_home_then_new_and_legacy_locations() {
        let root = std::env::temp_dir().join(format!("tokenscope-codex-home-{}", Uuid::new_v4()));
        let codex_home = root.join(".codex");
        let sqlite_home = root.join("custom-sqlite");

        let paths = codex_state_path_candidates(&codex_home, Some(&sqlite_home));

        assert_eq!(
            paths,
            vec![
                sqlite_home.join("state_5.sqlite"),
                codex_home.join("sqlite").join("state_5.sqlite"),
                codex_home.join("state_5.sqlite"),
            ]
        );
    }

    #[test]
    fn select_default_codex_state_path_uses_new_sqlite_state_before_legacy_state() {
        let root = std::env::temp_dir().join(format!("tokenscope-codex-home-{}", Uuid::new_v4()));
        let codex_home = root.join(".codex");
        let new_state = codex_home.join("sqlite").join("state_5.sqlite");
        let legacy_state = codex_home.join("state_5.sqlite");
        fs::create_dir_all(new_state.parent().expect("new state has parent"))
            .expect("new state parent created");
        fs::write(&new_state, "").expect("new state fixture written");
        fs::write(&legacy_state, "").expect("legacy state fixture written");

        let selected =
            select_default_codex_state_path(&codex_state_path_candidates(&codex_home, None));

        assert_eq!(selected, new_state);

        fs::remove_dir_all(root).expect("codex state fixture removed");
    }

    #[test]
    fn parses_codex_rate_limits_from_token_count_event() {
        let line = r#"{"timestamp":"2026-06-06T07:35:24.355Z","type":"event_msg","payload":{"type":"token_count","rate_limits":{"limit_id":"codex","limit_name":null,"plan_type":"pro","rate_limit_reached_type":null,"primary":{"resets_at":1780746229,"used_percent":5.0,"window_minutes":300},"secondary":{"resets_at":1781141316,"used_percent":18.0,"window_minutes":10080}},"info":{"last_token_usage":{"input_tokens":1000,"output_tokens":200,"total_tokens":1200}}}}"#;

        let snapshot = rollout_line_to_usage_limit_snapshot(Path::new("rollout.jsonl"), 9, line)
            .expect("rate limit snapshot is parsed");

        assert_eq!(snapshot.captured_at, "2026-06-06T07:35:24.355Z");
        assert_eq!(snapshot.source_path, "rollout.jsonl");
        assert_eq!(snapshot.line_number, 9);
        assert_eq!(snapshot.limit_id.as_deref(), Some("codex"));
        assert_eq!(snapshot.plan_type.as_deref(), Some("pro"));
        assert_eq!(snapshot.primary.window_minutes, 300);
        assert_eq!(snapshot.primary.used_percent, 5.0);
        assert_eq!(snapshot.primary.remaining_percent, 95.0);
        assert_eq!(snapshot.primary.resets_at, Some(1780746229));
        assert_eq!(snapshot.secondary.window_minutes, 10080);
        assert_eq!(snapshot.secondary.used_percent, 18.0);
        assert_eq!(snapshot.secondary.remaining_percent, 82.0);
        assert_eq!(snapshot.secondary.resets_at, Some(1781141316));
    }

    #[test]
    fn resolves_user_local_codex_cli_before_windowsapps_path_fallback() {
        let local_app_data =
            std::env::temp_dir().join(format!("tokenscope-codex-local-{}", Uuid::new_v4()));
        let rg_dir = local_app_data
            .join("OpenAI")
            .join("Codex")
            .join("bin")
            .join("rg-hash");
        let codex_dir = local_app_data
            .join("OpenAI")
            .join("Codex")
            .join("bin")
            .join("codex-hash");
        fs::create_dir_all(&rg_dir).expect("rg fixture dir created");
        fs::create_dir_all(&codex_dir).expect("codex fixture dir created");
        fs::write(rg_dir.join("rg.exe"), "").expect("rg fixture written");
        fs::write(codex_dir.join("codex.exe"), "").expect("codex fixture written");

        let command = resolve_default_codex_cli_command(Some(&local_app_data), &[]);

        assert_eq!(command, codex_dir.join("codex.exe").to_string_lossy());

        fs::remove_dir_all(local_app_data).expect("local app data fixture removed");
    }

    #[test]
    fn resolves_path_codex_cli_when_user_local_cli_is_missing() {
        let local_app_data =
            std::env::temp_dir().join(format!("tokenscope-codex-local-empty-{}", Uuid::new_v4()));
        let path_root =
            std::env::temp_dir().join(format!("tokenscope-codex-path-{}", Uuid::new_v4()));
        let windows_apps = path_root
            .join("Program Files")
            .join("WindowsApps")
            .join("OpenAI.Codex_26.609.3341.0_x64__2p2nqsd0c76g0")
            .join("app")
            .join("resources");
        let npm_bin = path_root
            .join("Users")
            .join("sample")
            .join("AppData")
            .join("Roaming")
            .join("npm");
        fs::create_dir_all(&local_app_data).expect("empty local app data fixture created");
        fs::create_dir_all(&windows_apps).expect("windowsapps fixture dir created");
        fs::create_dir_all(&npm_bin).expect("npm fixture dir created");
        fs::write(windows_apps.join("codex.exe"), "").expect("windowsapps fixture written");
        fs::write(npm_bin.join("codex.cmd"), "").expect("npm shim fixture written");
        let path_dirs = vec![windows_apps, npm_bin.clone()];

        let command = resolve_default_codex_cli_command(Some(&local_app_data), &path_dirs);

        assert_eq!(command, npm_bin.join("codex.cmd").to_string_lossy());

        fs::remove_dir_all(local_app_data).expect("local app data fixture removed");
        fs::remove_dir_all(path_root).expect("path fixture removed");
    }

    #[test]
    fn falls_back_to_codex_command_when_no_usable_cli_is_found() {
        let local_app_data =
            std::env::temp_dir().join(format!("tokenscope-codex-local-empty-{}", Uuid::new_v4()));
        fs::create_dir_all(&local_app_data).expect("empty local app data fixture created");

        let command = resolve_default_codex_cli_command(Some(&local_app_data), &[]);

        assert_eq!(command, "codex");

        fs::remove_dir_all(local_app_data).expect("local app data fixture removed");
    }

    #[cfg(windows)]
    #[test]
    fn windows_app_server_process_flags_hide_console_window() {
        assert_eq!(
            windows_codex_app_server_process_creation_flags() & 0x08000000,
            0x08000000
        );
    }

    #[test]
    fn parses_codex_subscription_rate_limits_from_app_server_response() {
        let value = json!({
            "rateLimits": {
                "limitId": "codex_bengalfox",
                "limitName": "GPT-5.3-Codex-Spark",
                "planType": "",
                "primary": { "usedPercent": 0.0, "windowDurationMins": 300, "resetsAt": 1780746229 },
                "secondary": { "usedPercent": 0.0, "windowDurationMins": 10080, "resetsAt": 1781141316 },
                "rateLimitReachedType": null
            },
            "rateLimitsByLimitId": {
                "codex_bengalfox": {
                    "limitId": "codex_bengalfox",
                    "limitName": "GPT-5.3-Codex-Spark",
                    "primary": { "usedPercent": 0.0, "windowDurationMins": 300, "resetsAt": 1780746229 },
                    "secondary": { "usedPercent": 0.0, "windowDurationMins": 10080, "resetsAt": 1781141316 }
                },
                "codex": {
                    "limitId": "codex",
                    "limitName": null,
                    "planType": "pro",
                    "primary": { "usedPercent": 13.0, "windowDurationMins": 300, "resetsAt": 1780750000 },
                    "secondary": { "usedPercent": 42.0, "windowDurationMins": 10080, "resetsAt": 1781150000 },
                    "rateLimitReachedType": null
                }
            }
        });
        let captured_at = DateTime::parse_from_rfc3339("2026-06-11T12:58:25Z")
            .expect("test timestamp parses")
            .with_timezone(&Utc);

        let snapshot = codex_app_server_rate_limits_to_snapshot(&value, captured_at)
            .expect("app-server subscription rate limits parse");

        assert_eq!(snapshot.captured_at, "2026-06-11T12:58:25+00:00");
        assert_eq!(snapshot.source_path, "codex app-server");
        assert_eq!(snapshot.line_number, 0);
        assert_eq!(snapshot.limit_id.as_deref(), Some("codex"));
        assert_eq!(snapshot.limit_name, None);
        assert_eq!(snapshot.plan_type.as_deref(), Some("pro"));
        assert_eq!(snapshot.primary.window_minutes, 300);
        assert_eq!(snapshot.primary.used_percent, 13.0);
        assert_eq!(snapshot.primary.remaining_percent, 87.0);
        assert_eq!(snapshot.primary.resets_at, Some(1780750000));
        assert_eq!(snapshot.secondary.window_minutes, 10080);
        assert_eq!(snapshot.secondary.used_percent, 42.0);
        assert_eq!(snapshot.secondary.remaining_percent, 58.0);
        assert_eq!(snapshot.secondary.resets_at, Some(1781150000));
    }

    #[test]
    fn ignores_app_server_model_specific_rate_limits_without_subscription_bucket() {
        let value = json!({
            "rateLimitsByLimitId": {
                "codex_bengalfox": {
                    "limitId": "codex_bengalfox",
                    "limitName": "GPT-5.3-Codex-Spark",
                    "primary": { "usedPercent": 0.0, "windowDurationMins": 300, "resetsAt": 1780746229 },
                    "secondary": { "usedPercent": 0.0, "windowDurationMins": 10080, "resetsAt": 1781141316 }
                }
            }
        });
        let captured_at = DateTime::parse_from_rfc3339("2026-06-11T12:58:25Z")
            .expect("test timestamp parses")
            .with_timezone(&Utc);

        let snapshot = codex_app_server_rate_limits_to_snapshot(&value, captured_at);

        assert!(snapshot.is_none());
    }

    #[test]
    fn extracts_codex_rate_limits_from_app_server_json_rpc_response() {
        let line = r#"{"id":6,"result":{"rateLimitsByLimitId":{"codex":{"limitId":"codex","planType":"plus","primary":{"usedPercent":21.0,"windowDurationMins":300,"resetsAt":1780750000},"secondary":{"usedPercent":34.0,"windowDurationMins":10080,"resetsAt":1781150000}}}}}"#;
        let captured_at = DateTime::parse_from_rfc3339("2026-06-11T12:58:25Z")
            .expect("test timestamp parses")
            .with_timezone(&Utc);

        let snapshot = codex_app_server_response_line_to_snapshot(line, 6, captured_at)
            .expect("json-rpc line parses")
            .expect("matching response returns snapshot");

        assert_eq!(snapshot.limit_id.as_deref(), Some("codex"));
        assert_eq!(snapshot.plan_type.as_deref(), Some("plus"));
        assert_eq!(snapshot.primary.used_percent, 21.0);
        assert_eq!(snapshot.secondary.used_percent, 34.0);
    }

    #[test]
    fn ignores_unrelated_app_server_json_rpc_messages() {
        let notification = r#"{"method":"account/rateLimits/updated","params":{"rateLimits":{"limitId":"codex","primary":{"usedPercent":99.0,"windowDurationMins":300,"resetsAt":1780750000}}}}"#;
        let captured_at = DateTime::parse_from_rfc3339("2026-06-11T12:58:25Z")
            .expect("test timestamp parses")
            .with_timezone(&Utc);

        let snapshot = codex_app_server_response_line_to_snapshot(notification, 6, captured_at)
            .expect("notification parses");

        assert!(snapshot.is_none());
    }

    #[cfg(windows)]
    #[test]
    fn reads_codex_subscription_rate_limits_from_app_server_process() {
        let script_path = std::env::temp_dir().join(format!(
            "tokenscope-codex-app-server-{}.ps1",
            Uuid::new_v4()
        ));
        fs::write(
            &script_path,
            r#"
$null = [Console]::In.ReadLine()
$null = [Console]::In.ReadLine()
$null = [Console]::In.ReadLine()
Write-Output '{"id":2,"result":{"rateLimitsByLimitId":{"codex":{"limitId":"codex","planType":"pro","primary":{"usedPercent":11.0,"windowDurationMins":300,"resetsAt":1780750000},"secondary":{"usedPercent":22.0,"windowDurationMins":10080,"resetsAt":1781150000}}}}}'
"#,
        )
        .expect("fake app-server script written");
        let args = vec![
            "-NoProfile".to_string(),
            "-ExecutionPolicy".to_string(),
            "Bypass".to_string(),
            "-File".to_string(),
            script_path.to_string_lossy().to_string(),
        ];

        let snapshot = latest_codex_app_server_usage_limits_with_command(
            "powershell.exe",
            &args,
            StdDuration::from_secs(5),
        )
        .expect("fake app-server process returns")
        .expect("app-server snapshot exists");

        assert_eq!(snapshot.source_path, "codex app-server");
        assert_eq!(snapshot.limit_id.as_deref(), Some("codex"));
        assert_eq!(snapshot.plan_type.as_deref(), Some("pro"));
        assert_eq!(snapshot.primary.used_percent, 11.0);
        assert_eq!(snapshot.secondary.used_percent, 22.0);

        fs::remove_file(script_path).expect("fake app-server script removed");
    }

    #[test]
    fn finds_latest_codex_usage_limit_snapshot_from_session_rollouts() {
        let sessions_path =
            std::env::temp_dir().join(format!("tokenscope-codex-sessions-{}", Uuid::new_v4()));
        let nested_path = sessions_path.join("2026").join("06");
        fs::create_dir_all(&nested_path).expect("session fixture directory created");
        write_rollout(
            &nested_path.join("old.jsonl"),
            r#"{"timestamp":"2026-06-06T07:20:00.000Z","type":"event_msg","payload":{"type":"token_count","rate_limits":{"limit_id":"codex","plan_type":"pro","primary":{"resets_at":1780746000,"used_percent":10.0,"window_minutes":300},"secondary":{"resets_at":1781141316,"used_percent":20.0,"window_minutes":10080}},"info":{"last_token_usage":{"input_tokens":1000,"output_tokens":200,"total_tokens":1200}}}}"#,
        );
        write_rollout(
            &nested_path.join("new.jsonl"),
            r#"{"timestamp":"2026-06-06T07:35:24.355Z","type":"event_msg","payload":{"type":"token_count","rate_limits":{"limit_id":"codex","plan_type":"pro","primary":{"resets_at":1780746229,"used_percent":5.0,"window_minutes":300},"secondary":{"resets_at":1781141316,"used_percent":18.0,"window_minutes":10080}},"info":{"last_token_usage":{"input_tokens":1200,"output_tokens":300,"total_tokens":1500}}}}"#,
        );

        let snapshot = latest_codex_usage_limits_from_sessions_path(&sessions_path)
            .expect("session rollouts are scanned")
            .expect("latest rate limit snapshot exists");

        assert_eq!(snapshot.captured_at, "2026-06-06T07:35:24.355Z");
        assert_eq!(snapshot.primary.remaining_percent, 95.0);
        assert_eq!(snapshot.secondary.remaining_percent, 82.0);
        assert!(snapshot.source_path.ends_with("new.jsonl"));

        fs::remove_dir_all(sessions_path).expect("session fixture directory removed");
    }

    #[test]
    fn prefers_general_codex_usage_limit_over_newer_model_specific_snapshot() {
        let sessions_path =
            std::env::temp_dir().join(format!("tokenscope-codex-sessions-{}", Uuid::new_v4()));
        let nested_path = sessions_path.join("2026").join("06");
        fs::create_dir_all(&nested_path).expect("session fixture directory created");
        write_rollout(
            &nested_path.join("general.jsonl"),
            r#"{"timestamp":"2026-06-08T02:59:58.072Z","type":"event_msg","payload":{"type":"token_count","rate_limits":{"limit_id":"codex","plan_type":"pro","primary":{"resets_at":1780893660,"used_percent":7.0,"window_minutes":300},"secondary":{"resets_at":1781141316,"used_percent":25.0,"window_minutes":10080}},"info":{"last_token_usage":{"input_tokens":1000,"output_tokens":200,"total_tokens":1200}}}}"#,
        );
        write_rollout(
            &nested_path.join("spark.jsonl"),
            r#"{"timestamp":"2026-06-08T03:00:23.644Z","type":"event_msg","payload":{"type":"token_count","rate_limits":{"limit_id":"codex_bengalfox","limit_name":"GPT-5.3-Codex-Spark","plan_type":"pro","primary":{"resets_at":1780905551,"used_percent":0.0,"window_minutes":300},"secondary":{"resets_at":1781492351,"used_percent":0.0,"window_minutes":10080}},"info":{"last_token_usage":{"input_tokens":1000,"output_tokens":200,"total_tokens":1200}}}}"#,
        );

        let snapshot = latest_codex_usage_limits_from_sessions_path(&sessions_path)
            .expect("session rollouts are scanned")
            .expect("rate limit snapshot exists");

        assert_eq!(snapshot.limit_id.as_deref(), Some("codex"));
        assert_eq!(snapshot.plan_type.as_deref(), Some("pro"));
        assert_eq!(snapshot.primary.used_percent, 7.0);
        assert_eq!(snapshot.secondary.used_percent, 25.0);
        assert!(snapshot.source_path.ends_with("general.jsonl"));

        fs::remove_dir_all(sessions_path).expect("session fixture directory removed");
    }

    #[test]
    fn prefers_general_codex_usage_limit_even_when_model_specific_is_much_newer() {
        let sessions_path =
            std::env::temp_dir().join(format!("tokenscope-codex-sessions-{}", Uuid::new_v4()));
        let nested_path = sessions_path.join("2026").join("06");
        fs::create_dir_all(&nested_path).expect("session fixture directory created");
        write_rollout(
            &nested_path.join("general.jsonl"),
            r#"{"timestamp":"2026-06-08T02:30:00.000Z","type":"event_msg","payload":{"type":"token_count","rate_limits":{"limit_id":"codex","plan_type":"pro","primary":{"resets_at":1780893660,"used_percent":17.0,"window_minutes":300},"secondary":{"resets_at":1781141316,"used_percent":43.0,"window_minutes":10080}},"info":{"last_token_usage":{"input_tokens":1000,"output_tokens":200,"total_tokens":1200}}}}"#,
        );
        write_rollout(
            &nested_path.join("spark.jsonl"),
            r#"{"timestamp":"2026-06-08T03:10:00.000Z","type":"event_msg","payload":{"type":"token_count","rate_limits":{"limit_id":"codex_bengalfox","limit_name":"GPT-5.3-Codex-Spark","plan_type":"pro","primary":{"resets_at":1780905551,"used_percent":0.0,"window_minutes":300},"secondary":{"resets_at":1781492351,"used_percent":0.0,"window_minutes":10080}},"info":{"last_token_usage":{"input_tokens":1000,"output_tokens":200,"total_tokens":1200}}}}"#,
        );

        let snapshot = latest_codex_usage_limits_from_sessions_path(&sessions_path)
            .expect("session rollouts are scanned")
            .expect("rate limit snapshot exists");

        assert_eq!(snapshot.limit_id.as_deref(), Some("codex"));
        assert_eq!(snapshot.plan_type.as_deref(), Some("pro"));
        assert_eq!(snapshot.primary.used_percent, 17.0);
        assert_eq!(snapshot.secondary.used_percent, 43.0);

        fs::remove_dir_all(sessions_path).expect("session fixture directory removed");
    }

    #[test]
    fn accepts_general_codex_usage_limit_for_any_plan_type() {
        let sessions_path =
            std::env::temp_dir().join(format!("tokenscope-codex-sessions-{}", Uuid::new_v4()));
        let nested_path = sessions_path.join("2026").join("06");
        fs::create_dir_all(&nested_path).expect("session fixture directory created");
        write_rollout(
            &nested_path.join("team.jsonl"),
            r#"{"timestamp":"2026-06-08T02:30:00.000Z","type":"event_msg","payload":{"type":"token_count","rate_limits":{"limit_id":"codex","plan_type":"team","primary":{"resets_at":1780893660,"used_percent":30.0,"window_minutes":300},"secondary":{"resets_at":1781141316,"used_percent":9.0,"window_minutes":10080}},"info":{"last_token_usage":{"input_tokens":1000,"output_tokens":200,"total_tokens":1200}}}}"#,
        );

        let snapshot = latest_codex_usage_limits_from_sessions_path(&sessions_path)
            .expect("session rollouts are scanned")
            .expect("general subscription rate limit snapshot exists");

        assert_eq!(snapshot.limit_id.as_deref(), Some("codex"));
        assert_eq!(snapshot.plan_type.as_deref(), Some("team"));
        assert_eq!(snapshot.primary.remaining_percent, 70.0);
        assert_eq!(snapshot.secondary.remaining_percent, 91.0);

        fs::remove_dir_all(sessions_path).expect("session fixture directory removed");
    }

    #[test]
    fn merges_fresh_general_codex_snapshots_conservatively() {
        let sessions_path =
            std::env::temp_dir().join(format!("tokenscope-codex-sessions-{}", Uuid::new_v4()));
        let nested_path = sessions_path.join("2026").join("06");
        fs::create_dir_all(&nested_path).expect("session fixture directory created");
        write_rollout(
            &nested_path.join("higher-used.jsonl"),
            r#"{"timestamp":"2026-06-10T03:01:53.000Z","type":"event_msg","payload":{"type":"token_count","rate_limits":{"limit_id":"codex","plan_type":"pro","primary":{"resets_at":1781064028,"used_percent":19.0,"window_minutes":300},"secondary":{"resets_at":1781141316,"used_percent":43.0,"window_minutes":10080}},"info":{"last_token_usage":{"input_tokens":1000,"output_tokens":200,"total_tokens":1200}}}}"#,
        );
        write_rollout(
            &nested_path.join("later-lower-used.jsonl"),
            r#"{"timestamp":"2026-06-10T03:02:26.000Z","type":"event_msg","payload":{"type":"token_count","rate_limits":{"limit_id":"codex","plan_type":"pro","primary":{"resets_at":1781064028,"used_percent":13.0,"window_minutes":300},"secondary":{"resets_at":1781141316,"used_percent":42.0,"window_minutes":10080}},"info":{"last_token_usage":{"input_tokens":1000,"output_tokens":200,"total_tokens":1200}}}}"#,
        );

        let snapshot = latest_codex_usage_limits_from_sessions_path(&sessions_path)
            .expect("session rollouts are scanned")
            .expect("rate limit snapshot exists");

        assert_eq!(snapshot.limit_id.as_deref(), Some("codex"));
        assert_eq!(snapshot.captured_at, "2026-06-10T03:02:26.000Z");
        assert_eq!(snapshot.primary.used_percent, 19.0);
        assert_eq!(snapshot.primary.remaining_percent, 81.0);
        assert_eq!(snapshot.secondary.used_percent, 43.0);
        assert_eq!(snapshot.secondary.remaining_percent, 57.0);

        fs::remove_dir_all(sessions_path).expect("session fixture directory removed");
    }

    #[test]
    fn merges_active_and_archived_codex_usage_limit_roots() {
        let root_path =
            std::env::temp_dir().join(format!("tokenscope-codex-roots-{}", Uuid::new_v4()));
        let active_path = root_path.join("sessions").join("2026").join("06");
        let archived_path = root_path.join("archived_sessions");
        fs::create_dir_all(&active_path).expect("active fixture directory created");
        fs::create_dir_all(&archived_path).expect("archived fixture directory created");
        write_rollout(
            &active_path.join("spark.jsonl"),
            r#"{"timestamp":"2026-06-10T03:02:26.000Z","type":"event_msg","payload":{"type":"token_count","rate_limits":{"limit_id":"codex_bengalfox","limit_name":"GPT-5.3-Codex-Spark","plan_type":"pro","primary":{"resets_at":1781064028,"used_percent":0.0,"window_minutes":300},"secondary":{"resets_at":1781141316,"used_percent":0.0,"window_minutes":10080}},"info":{"last_token_usage":{"input_tokens":1000,"output_tokens":200,"total_tokens":1200}}}}"#,
        );
        write_rollout(
            &archived_path.join("general.jsonl"),
            r#"{"timestamp":"2026-06-10T03:01:53.000Z","type":"event_msg","payload":{"type":"token_count","rate_limits":{"limit_id":"codex","plan_type":"pro","primary":{"resets_at":1781064028,"used_percent":19.0,"window_minutes":300},"secondary":{"resets_at":1781141316,"used_percent":43.0,"window_minutes":10080}},"info":{"last_token_usage":{"input_tokens":1000,"output_tokens":200,"total_tokens":1200}}}}"#,
        );

        let snapshot = latest_codex_usage_limits_from_roots_at(
            &[
                root_path.join("sessions"),
                root_path.join("archived_sessions"),
            ],
            false,
            codex_usage_limit_test_now(),
        )
        .expect("session roots are scanned")
        .expect("rate limit snapshot exists");

        assert_eq!(snapshot.limit_id.as_deref(), Some("codex"));
        assert_eq!(snapshot.primary.remaining_percent, 81.0);
        assert_eq!(snapshot.secondary.remaining_percent, 57.0);

        fs::remove_dir_all(root_path).expect("session fixture directory removed");
    }

    #[test]
    fn ignores_model_specific_codex_usage_limit_without_subscription_snapshot() {
        let sessions_path =
            std::env::temp_dir().join(format!("tokenscope-codex-sessions-{}", Uuid::new_v4()));
        let nested_path = sessions_path.join("2026").join("06");
        fs::create_dir_all(&nested_path).expect("session fixture directory created");
        write_rollout(
            &nested_path.join("spark.jsonl"),
            r#"{"timestamp":"2026-06-08T03:00:23.644Z","type":"event_msg","payload":{"type":"token_count","rate_limits":{"limit_id":"codex_bengalfox","limit_name":"GPT-5.3-Codex-Spark","plan_type":"pro","primary":{"resets_at":1780905551,"used_percent":0.0,"window_minutes":300},"secondary":{"resets_at":1781492351,"used_percent":0.0,"window_minutes":10080}},"info":{"last_token_usage":{"input_tokens":1000,"output_tokens":200,"total_tokens":1200}}}}"#,
        );

        let snapshot = latest_codex_usage_limits_from_sessions_path(&sessions_path)
            .expect("session rollouts are scanned");

        assert!(snapshot.is_none());

        fs::remove_dir_all(sessions_path).expect("session fixture directory removed");
    }

    #[test]
    fn ignores_expired_subscription_snapshot_when_only_model_specific_limits_are_fresh() {
        let sessions_path =
            std::env::temp_dir().join(format!("tokenscope-codex-sessions-{}", Uuid::new_v4()));
        let nested_path = sessions_path.join("2026").join("06");
        fs::create_dir_all(&nested_path).expect("session fixture directory created");
        write_rollout(
            &nested_path.join("expired-general.jsonl"),
            r#"{"timestamp":"2026-06-11T06:33:00.000Z","type":"event_msg","payload":{"type":"token_count","rate_limits":{"limit_id":"codex","plan_type":"pro","primary":{"resets_at":1,"used_percent":0.0,"window_minutes":300},"secondary":{"resets_at":1,"used_percent":0.0,"window_minutes":10080}},"info":{"last_token_usage":{"input_tokens":1000,"output_tokens":200,"total_tokens":1200}}}}"#,
        );
        write_rollout(
            &nested_path.join("fresh-spark.jsonl"),
            r#"{"timestamp":"2026-06-11T06:40:00.000Z","type":"event_msg","payload":{"type":"token_count","rate_limits":{"limit_id":"codex_bengalfox","limit_name":"GPT-5.3-Codex-Spark","plan_type":"pro","primary":{"resets_at":4102444800,"used_percent":0.0,"window_minutes":300},"secondary":{"resets_at":4102444800,"used_percent":0.0,"window_minutes":10080}},"info":{"last_token_usage":{"input_tokens":1000,"output_tokens":200,"total_tokens":1200}}}}"#,
        );

        let snapshot = latest_codex_usage_limits_from_roots(&[sessions_path.clone()], true)
            .expect("session rollouts are scanned");

        assert!(snapshot.is_none());

        fs::remove_dir_all(sessions_path).expect("session fixture directory removed");
    }

    #[test]
    fn reuses_unchanged_rollouts_and_reads_appended_usage_limit_lines() {
        let sessions_path =
            std::env::temp_dir().join(format!("tokenscope-codex-sessions-{}", Uuid::new_v4()));
        let nested_path = sessions_path.join("2026").join("06");
        let rollout_path = nested_path.join("rollout.jsonl");
        fs::create_dir_all(&nested_path).expect("session fixture directory created");
        write_rollout(
            &rollout_path,
            r#"{"timestamp":"2026-06-06T07:20:00.000Z","type":"event_msg","payload":{"type":"token_count","rate_limits":{"limit_id":"codex","plan_type":"pro","primary":{"resets_at":1780746000,"used_percent":10.0,"window_minutes":300},"secondary":{"resets_at":1781141316,"used_percent":20.0,"window_minutes":10080}},"info":{"last_token_usage":{"input_tokens":1000,"output_tokens":200,"total_tokens":1200}}}}"#,
        );

        let (snapshot, first_stats) =
            latest_codex_usage_limits_from_sessions_path_with_stats_force(&sessions_path)
                .expect("initial session rollouts are scanned");
        assert_eq!(
            snapshot
                .expect("initial snapshot exists")
                .primary
                .remaining_percent,
            90.0
        );
        assert_eq!(first_stats.read_files, 1);
        assert_eq!(first_stats.reused_files, 0);
        assert_eq!(first_stats.scanned_lines, 1);

        let (_, second_stats) =
            latest_codex_usage_limits_from_sessions_path_with_stats_force(&sessions_path)
                .expect("unchanged session rollouts are reused");
        assert_eq!(second_stats.read_files, 0);
        assert_eq!(second_stats.reused_files, 1);
        assert_eq!(second_stats.scanned_lines, 0);

        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&rollout_path)
            .expect("rollout fixture opens for append");
        writeln!(
            file,
            r#"{{"timestamp":"2026-06-06T07:35:24.355Z","type":"event_msg","payload":{{"type":"token_count","rate_limits":{{"limit_id":"codex","plan_type":"pro","primary":{{"resets_at":1780746229,"used_percent":5.0,"window_minutes":300}},"secondary":{{"resets_at":1781141316,"used_percent":18.0,"window_minutes":10080}}}},"info":{{"last_token_usage":{{"input_tokens":1200,"output_tokens":300,"total_tokens":1500}}}}}}}}"#
        )
        .expect("rollout fixture append succeeds");

        let (snapshot, third_stats) =
            latest_codex_usage_limits_from_sessions_path_with_stats_force(&sessions_path)
                .expect("appended session rollout line is scanned");
        let snapshot = snapshot.expect("appended snapshot exists");
        assert_eq!(snapshot.captured_at, "2026-06-06T07:35:24.355Z");
        assert_eq!(snapshot.line_number, 2);
        assert_eq!(snapshot.primary.remaining_percent, 95.0);
        assert_eq!(third_stats.read_files, 1);
        assert_eq!(third_stats.reused_files, 0);
        assert_eq!(third_stats.scanned_lines, 1);

        fs::remove_dir_all(sessions_path).expect("session fixture directory removed");
    }

    #[test]
    fn caches_recent_codex_usage_limit_scan_without_rewalking_files() {
        let sessions_path =
            std::env::temp_dir().join(format!("tokenscope-codex-sessions-{}", Uuid::new_v4()));
        let nested_path = sessions_path.join("2026").join("06");
        let rollout_path = nested_path.join("rollout.jsonl");
        fs::create_dir_all(&nested_path).expect("session fixture directory created");
        write_rollout(
            &rollout_path,
            r#"{"timestamp":"2026-06-06T07:20:00.000Z","type":"event_msg","payload":{"type":"token_count","rate_limits":{"limit_id":"codex","plan_type":"pro","primary":{"resets_at":1780746000,"used_percent":10.0,"window_minutes":300},"secondary":{"resets_at":1781141316,"used_percent":20.0,"window_minutes":10080}},"info":{"last_token_usage":{"input_tokens":1000,"output_tokens":200,"total_tokens":1200}}}}"#,
        );

        let (_, first_stats) =
            latest_codex_usage_limits_from_sessions_path_with_stats(&sessions_path)
                .expect("initial session rollouts are scanned");
        assert_eq!(first_stats.cache_hits, 0);
        assert_eq!(first_stats.read_files, 1);

        let (snapshot, second_stats) =
            latest_codex_usage_limits_from_sessions_path_with_stats(&sessions_path)
                .expect("recent session rollouts reuse cached root read");
        assert_eq!(second_stats.cache_hits, 1);
        assert_eq!(second_stats.files, 0);
        assert_eq!(second_stats.read_files, 0);
        assert_eq!(
            snapshot
                .expect("cached snapshot exists")
                .primary
                .remaining_percent,
            90.0
        );

        fs::remove_dir_all(sessions_path).expect("session fixture directory removed");
    }

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
    async fn imports_codex_threads_from_schema_without_ms_timestamp_columns() {
        let source_path = create_codex_state_db_without_ms_columns().await;
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("target repository connects");
        repository.migrate().await.expect("target migrations run");

        let result = import_codex_threads_from_path(&repository, &source_path)
            .await
            .expect("codex import supports older timestamp columns");

        assert_eq!(result.imported, 1);
        assert_eq!(result.skipped, 0);

        let row = query(
            r#"
      SELECT total_tokens, model_response, date_local
      FROM llm_call
      WHERE id = 'codex-thread-thread_legacy'
      "#,
        )
        .fetch_one(repository.pool())
        .await
        .expect("legacy timestamp call exists");

        assert_eq!(row.get::<i64, _>("total_tokens"), 2048);
        assert_eq!(row.get::<String, _>("model_response"), "gpt-5.3-codex");
        assert_eq!(row.get::<String, _>("date_local"), "2026-09-21");
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

    async fn create_codex_state_db_without_ms_columns() -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("tokenscope-codex-legacy-{}.sqlite", Uuid::new_v4()));
        let _ = fs::remove_file(&path);
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("legacy source db connects");

        query(
            r#"
      CREATE TABLE threads (
        id TEXT PRIMARY KEY,
        rollout_path TEXT,
        created_at INTEGER,
        updated_at INTEGER,
        model_provider TEXT,
        cwd TEXT,
        tokens_used INTEGER,
        model TEXT
      )
      "#,
        )
        .execute(&pool)
        .await
        .expect("legacy source schema created");

        query(
            r#"
      INSERT INTO threads (
        id,
        created_at,
        updated_at,
        model_provider,
        cwd,
        tokens_used,
        model
      ) VALUES (
        'thread_legacy',
        1790000000,
        1790000300,
        'openai',
        'D:\Project\legacy-project',
        2048,
        'gpt-5.3-codex'
      )
      "#,
        )
        .execute(&pool)
        .await
        .expect("legacy source thread inserted");
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

    fn write_rollout(path: &Path, content: &str) {
        fs::write(path, format!("{content}\n")).expect("rollout fixture written");
    }

    async fn set_codex_source_rollout_path(source_path: &PathBuf, rollout_path: &Path) {
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
