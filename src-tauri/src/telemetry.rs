use std::sync::OnceLock;

static PERF_LOGGING_ENABLED: OnceLock<bool> = OnceLock::new();

pub fn perf_logging_enabled() -> bool {
    *PERF_LOGGING_ENABLED.get_or_init(|| {
        cfg!(debug_assertions)
            || std::env::var("TOKENSCOPE_PERF_LOG")
                .map(|value| matches!(value.trim(), "1" | "true" | "TRUE" | "on" | "ON"))
                .unwrap_or(false)
    })
}

#[macro_export]
macro_rules! perf_log {
    ($($arg:tt)*) => {{
        if $crate::telemetry::perf_logging_enabled() {
            eprintln!($($arg)*);
        }
    }};
}
