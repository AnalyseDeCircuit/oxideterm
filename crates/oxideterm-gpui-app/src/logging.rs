use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use oxideterm_settings::PersistedSettings;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

const LOG_FILE_PREFIX: &str = "oxideterm-native";
const DEFAULT_LOG_FILTER: &str = "warn,oxideterm_gpui_app=info,oxideterm_ssh=info";
const DEBUG_LOG_FILTER: &str = "warn,oxideterm_gpui_app=debug,oxideterm_ssh=debug,gpui=info";

pub(crate) fn init_file_logging(
    settings: &PersistedSettings,
    settings_path: Option<&Path>,
) -> Result<Option<WorkerGuard>> {
    let log_dir = log_directory_from_settings_path(settings_path);
    std::fs::create_dir_all(&log_dir)
        .with_context(|| format!("failed to create log directory at {}", log_dir.display()))?;

    let file_appender = tracing_appender::rolling::daily(&log_dir, LOG_FILE_PREFIX);
    let (writer, guard) = tracing_appender::non_blocking(file_appender);
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        if settings.diagnostics.debug_logging {
            EnvFilter::new(DEBUG_LOG_FILTER)
        } else {
            EnvFilter::new(DEFAULT_LOG_FILTER)
        }
    });

    let subscriber = tracing_subscriber::registry().with(
        fmt::layer()
            .with_writer(writer)
            .with_ansi(false)
            .with_target(true),
    );

    // Tests or embedding hosts may already have installed a global subscriber.
    // In that case OxideTerm should keep running and simply skip its file sink.
    if subscriber.with(filter).try_init().is_err() {
        return Ok(None);
    }

    tracing::info!(
        log_dir = %log_dir.display(),
        debug_logging = settings.diagnostics.debug_logging,
        "OxideTerm native file logging initialized"
    );
    Ok(Some(guard))
}

fn log_directory_from_settings_path(settings_path: Option<&Path>) -> PathBuf {
    settings_path
        .and_then(Path::parent)
        .map(|parent| parent.join("logs"))
        .unwrap_or_else(|| PathBuf::from("logs"))
}
