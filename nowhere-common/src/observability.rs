//! Shared observability helpers for binaries and integration tests.
//!
//! The logging initializer centralises our `tracing` setup so that every
//! binary emits into the same rolling file sink. Call [`init_logging`] once
//! near process start and reuse its defaults—additional callers are treated
//! as no-ops and simply receive the resolved log file path.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::Context;
use chrono::Local;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

static LOG_GUARD: OnceLock<WorkerGuard> = OnceLock::new();
static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Output encoding for structured logs.
#[derive(Debug, Clone, Copy)]
pub enum LogFormat {
    Text,
    Json,
}

/// Configuration passed to [`init_logging`].
#[derive(Debug, Clone)]
pub struct LogConfig {
    /// Logical name of the component (used for defaults and file names).
    pub app_name: &'static str,
    /// Optional explicit directory for log output. If `None`, we consult
    /// `NOWHERE_LOG_DIR` and finally fall back to `~/.local/share/<app_name>`.
    pub log_dir: Option<PathBuf>,
    /// Whether to duplicate events to `stderr` in addition to the file sink.
    pub emit_stderr: bool,
    /// Preferred log encoding.
    pub format: LogFormat,
    /// Default filter applied when `RUST_LOG` is unset.
    pub default_filter: &'static str,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            app_name: "nowhere",
            log_dir: None,
            emit_stderr: false,
            format: LogFormat::Text,
            default_filter: "info",
        }
    }
}

/// Initialise the global `tracing` subscriber.
///
/// Returns the concrete log file path for the current day. Subsequent calls
/// are cheap and simply hand back the originally resolved location.
pub fn init_logging(config: LogConfig) -> anyhow::Result<PathBuf> {
    if let Some(path) = LOG_PATH.get() {
        return Ok(path.clone());
    }

    let resolved_dir = resolve_log_dir(config.app_name, config.log_dir.as_deref());
    std::fs::create_dir_all(&resolved_dir)
        .with_context(|| format!("failed to create log directory: {}", resolved_dir.display()))?;

    let log_filename = format!("{}.log", config.app_name);
    let today = Local::now().format("%Y-%m-%d").to_string();
    let full_path = resolved_dir.join(&today).join(&log_filename);

    let appender = rolling::daily(resolved_dir, log_filename);
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let _ = LOG_GUARD.set(guard);

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(config.default_filter));

    match (config.format, config.emit_stderr) {
        (LogFormat::Text, false) => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt::layer().with_writer(writer).with_ansi(false))
                .try_init()
                .map_err(|e| anyhow::anyhow!("tracing setup failed: {e}"))?;
        }
        (LogFormat::Text, true) => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt::layer().with_writer(writer).with_ansi(false))
                .with(fmt::layer().with_writer(std::io::stderr))
                .try_init()
                .map_err(|e| anyhow::anyhow!("tracing setup failed: {e}"))?;
        }
        (LogFormat::Json, false) => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt::layer().json().with_writer(writer))
                .try_init()
                .map_err(|e| anyhow::anyhow!("tracing setup failed: {e}"))?;
        }
        (LogFormat::Json, true) => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt::layer().json().with_writer(writer))
                .with(fmt::layer().json().with_writer(std::io::stderr))
                .try_init()
                .map_err(|e| anyhow::anyhow!("tracing setup failed: {e}"))?;
        }
    }

    let _ = LOG_PATH.set(full_path.clone());
    Ok(full_path)
}

fn resolve_log_dir(app_name: &str, explicit: Option<&Path>) -> PathBuf {
    if let Some(dir) = explicit {
        return expand_home(dir);
    }

    if let Ok(env_dir) = std::env::var("NOWHERE_LOG_DIR") {
        return expand_home(Path::new(&env_dir));
    }

    default_data_dir(app_name)
}

fn expand_home(path: &Path) -> PathBuf {
    if let Some(rest) = path.to_str().and_then(|s| s.strip_prefix("~/")) {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    path.to_path_buf()
}

fn default_data_dir(app_name: &str) -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home)
            .join(".local")
            .join("share")
            .join(app_name)
    } else {
        PathBuf::from(".").join(app_name)
    }
}
