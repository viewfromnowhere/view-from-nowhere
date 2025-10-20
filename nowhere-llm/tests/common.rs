use std::sync::OnceLock;

use nowhere_common::observability::{LogConfig, LogFormat};

static INIT_PATH: OnceLock<std::path::PathBuf> = OnceLock::new();

pub fn init_test_tracing() {
    let _ = INIT_PATH.get_or_init(|| {
        let config = LogConfig {
            app_name: "nowhere-tests",
            emit_stderr: true,
            format: if std::env::var("NOWHERE_LOG_FORMAT")
                .map(|raw| raw.trim().eq_ignore_ascii_case("json"))
                .unwrap_or(false)
            {
                LogFormat::Json
            } else {
                LogFormat::Text
            },
            default_filter: "debug",
            ..LogConfig::default()
        };

        nowhere_common::observability::init_logging(config).unwrap_or_default()
    });
}
