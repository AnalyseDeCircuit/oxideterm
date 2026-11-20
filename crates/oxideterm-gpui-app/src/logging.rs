use std::{
    backtrace::Backtrace,
    cell::Cell,
    fs::{File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    panic::PanicHookInfo,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use anyhow::{Context as _, Result};
use oxideterm_settings::PersistedSettings;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{
    EnvFilter, Registry, fmt, layer::SubscriberExt, reload, util::SubscriberInitExt,
};

const LOG_FILE_NAME: &str = "oxideterm-native.log";
const CRASH_LOG_FILE_NAME: &str = "oxideterm-native-crash.log";
const LEGACY_LOG_FILE_PREFIX: &str = "oxideterm-native.";
const MAX_LOG_FILE_BYTES: u64 = 10 * 1024 * 1024;
const MAX_CRASH_LOG_BYTES: u64 = 2 * 1024 * 1024;
const OVERSIZED_LOG_ENTRY_MARKER: &[u8] = b"[oversized log entry omitted]\n";
const DEFAULT_LOG_FILTER: &str = "warn,\
oxideterm_gpui_app=info,\
oxideterm_ssh=info,\
oxideterm_terminal=info,\
oxideterm_gpui_connections=info,\
oxideterm_update=info";
const DEBUG_LOG_FILTER: &str = "warn,\
oxideterm_gpui_app=debug,\
oxideterm_gpui_platform=debug,\
oxideterm_gpui_terminal=debug,\
oxideterm_gpui_connections=debug,\
oxideterm_gpui_sftp=debug,\
oxideterm_gpui_remote_desktop=debug,\
oxideterm_gpui_file_manager=debug,\
oxideterm_gpui_forwards=debug,\
oxideterm_gpui_editor=debug,\
oxideterm_gpui_ide=debug,\
oxideterm_gpui_ai=debug,\
oxideterm_gpui_cloud_sync=debug,\
oxideterm_gpui_background=debug,\
oxideterm_gpui_settings_view=debug,\
oxideterm_gpui_ui=debug,\
oxideterm_ssh=debug,\
oxideterm_ssh_launch=debug,\
oxideterm_terminal=debug,\
oxideterm_forwarding=debug,\
oxideterm_wsl_graphics=debug,\
oxideterm_sftp=debug,\
oxideterm_ftp=debug,\
oxideterm_mosh=debug,\
oxideterm_ble=debug,\
oxideterm_remote_desktop=debug,\
oxideterm_connections=debug,\
oxideterm_network_proxy=debug,\
oxideterm_ai=debug,\
oxideterm_public_mcp=debug,\
oxideterm_public_mcp_host=debug,\
oxideterm_plugin_registry=debug,\
oxideterm_plugin_host_api=debug,\
oxideterm_cloud_sync=debug,\
oxideterm_update=debug,\
gpui_pre=info,\
gpui_pre_apple=info,\
gpui_pre_macos=info,\
gpui_pre_windows=info,\
gpui_pre_linux=info,\
gpui_pre_wgpu=info,\
zed_xim=info";

static CRASH_LOG_DIR: OnceLock<PathBuf> = OnceLock::new();
static CRASH_LOG_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static PANIC_HOOK_INSTALLED: OnceLock<()> = OnceLock::new();
static FILTER_RELOAD_HANDLE: OnceLock<reload::Handle<EnvFilter, Registry>> = OnceLock::new();
static ENV_FILTER_OVERRIDDEN: OnceLock<bool> = OnceLock::new();

thread_local! {
    static IN_PANIC_HOOK: Cell<bool> = const { Cell::new(false) };
}

struct PanicHookReentrancyGuard;

impl Drop for PanicHookReentrancyGuard {
    fn drop(&mut self) {
        IN_PANIC_HOOK.with(|flag| flag.set(false));
    }
}

struct SizeLimitedLogWriter {
    file: File,
    current_len: u64,
    max_len: u64,
}

impl SizeLimitedLogWriter {
    fn open(path: &Path, max_len: u64) -> io::Result<Self> {
        if max_len == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "log file size limit must be greater than zero",
            ));
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
        let current_len = file.metadata()?.len();
        let mut writer = Self {
            file,
            current_len,
            max_len,
        };
        if current_len > max_len {
            writer.compact_for(0)?;
        }
        writer.file.seek(SeekFrom::End(0))?;
        Ok(writer)
    }

    fn compact_for(&mut self, incoming_len: u64) -> io::Result<()> {
        let history_budget = (self.max_len / 2).min(self.max_len.saturating_sub(incoming_len));
        let tail_start = self.current_len.saturating_sub(history_budget);
        self.file.seek(SeekFrom::Start(tail_start))?;

        let mut recent_log = Vec::with_capacity(history_budget as usize);
        self.file.read_to_end(&mut recent_log)?;
        if tail_start > 0 {
            // Drop the partial line at the compaction boundary so the retained
            // file always begins with a complete UTF-8 log record.
            match recent_log.iter().position(|byte| *byte == b'\n') {
                Some(first_line_end) => {
                    recent_log.drain(..=first_line_end);
                }
                None => recent_log.clear(),
            }
        }

        self.file.set_len(0)?;
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&recent_log)?;
        self.current_len = recent_log.len() as u64;
        Ok(())
    }
}

impl Write for SizeLimitedLogWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }

        let original_len = bytes.len();
        let bytes = if bytes.len() as u64 > self.max_len {
            let marker_start = OVERSIZED_LOG_ENTRY_MARKER
                .len()
                .saturating_sub(self.max_len as usize);
            &OVERSIZED_LOG_ENTRY_MARKER[marker_start..]
        } else {
            bytes
        };
        let incoming_len = bytes.len() as u64;
        if self.current_len.saturating_add(incoming_len) > self.max_len {
            self.compact_for(incoming_len)?;
        }
        self.file.write_all(bytes)?;
        self.current_len += incoming_len;
        Ok(original_len)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

pub(crate) fn init_file_logging(
    settings: &PersistedSettings,
    settings_path: Option<&Path>,
) -> Result<Option<WorkerGuard>> {
    let log_dir = log_directory_from_settings_path(settings_path);
    std::fs::create_dir_all(&log_dir)
        .with_context(|| format!("failed to create log directory at {}", log_dir.display()))?;
    if let Err(error) = remove_legacy_daily_logs(&log_dir) {
        // A stale file cleanup failure must not disable current-session logs.
        eprintln!("failed to remove legacy OxideTerm daily logs: {error}");
    }

    let log_path = log_dir.join(LOG_FILE_NAME);
    let file_writer = SizeLimitedLogWriter::open(&log_path, MAX_LOG_FILE_BYTES)
        .with_context(|| format!("failed to open log file at {}", log_path.display()))?;
    let (writer, guard) = tracing_appender::non_blocking(file_writer);
    let env_filter = EnvFilter::try_from_default_env().ok();
    let _ = ENV_FILTER_OVERRIDDEN.set(env_filter.is_some());
    let filter = env_filter.unwrap_or_else(|| build_log_filter(settings.diagnostics.debug_logging));
    let (filter_layer, reload_handle) = reload::Layer::new(filter);

    let subscriber = tracing_subscriber::registry().with(filter_layer).with(
        fmt::layer()
            .with_writer(writer)
            .with_ansi(false)
            .with_target(true),
    );

    // Tests or embedding hosts may already have installed a global subscriber.
    // In that case OxideTerm should keep running and simply skip its file sink.
    if subscriber.try_init().is_err() {
        return Ok(None);
    }
    let _ = FILTER_RELOAD_HANDLE.set(reload_handle);

    tracing::info!(
        log_path = %log_path.display(),
        max_log_file_bytes = MAX_LOG_FILE_BYTES,
        debug_logging = settings.diagnostics.debug_logging,
        "OxideTerm native file logging initialized"
    );
    Ok(Some(guard))
}

fn build_log_filter(debug_logging: bool) -> EnvFilter {
    EnvFilter::new(if debug_logging {
        DEBUG_LOG_FILTER
    } else {
        DEFAULT_LOG_FILTER
    })
}

pub(crate) fn set_debug_logging_enabled(debug_logging: bool) {
    // Explicit RUST_LOG takes precedence over settings-driven filter toggles.
    if ENV_FILTER_OVERRIDDEN.get().copied().unwrap_or(false) {
        return;
    }
    let Some(handle) = FILTER_RELOAD_HANDLE.get() else {
        return;
    };
    let next_filter = build_log_filter(debug_logging);
    if handle.reload(next_filter).is_ok() {
        tracing::info!(debug_logging, "Updated OxideTerm native log filter");
    }
}

fn log_directory_from_settings_path(settings_path: Option<&Path>) -> PathBuf {
    settings_path
        .and_then(Path::parent)
        .map(|parent| parent.join("logs"))
        .unwrap_or_else(|| PathBuf::from("logs"))
}

pub(crate) fn install_panic_hook(settings_path: Option<&Path>) {
    let log_dir = log_directory_from_settings_path(settings_path);
    let _ = CRASH_LOG_DIR.set(log_dir.clone());
    let _ = std::fs::create_dir_all(&log_dir);
    let _ = CRASH_LOG_WRITE_LOCK.get_or_init(|| Mutex::new(()));

    PANIC_HOOK_INSTALLED.get_or_init(|| {
        let previous_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let already_active = IN_PANIC_HOOK.with(|flag| flag.replace(true));
            if !already_active {
                let _guard = PanicHookReentrancyGuard;
                record_panic_to_logs(info);
            }
            previous_hook(info);
        }));
    });
}

fn panic_payload_message(info: &PanicHookInfo<'_>) -> String {
    if let Some(text) = info.payload().downcast_ref::<&str>() {
        (*text).to_owned()
    } else if let Some(text) = info.payload().downcast_ref::<String>() {
        text.clone()
    } else {
        "<non-string panic payload>".to_owned()
    }
}

fn format_panic_report(
    timestamp: &str,
    thread_name: &str,
    location: &str,
    raw_payload: &str,
    raw_backtrace: &str,
) -> String {
    let redacted_payload = oxideterm_audit::redact(raw_payload);
    let redacted_backtrace = oxideterm_audit::redact(raw_backtrace);
    format!(
        "=== OxideTerm Panic Report ===\n\
         timestamp: {timestamp}\n\
         version: {}\n\
         target: {}/{}\n\
         thread: {thread_name}\n\
         location: {location}\n\
         message: {}\n\
         backtrace:\n{}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        redacted_payload.as_str(),
        redacted_backtrace.as_str().trim_end(),
    )
}

fn write_panic_report_to_dir(log_dir: &Path, report: &str, max_len: u64) -> io::Result<()> {
    std::fs::create_dir_all(log_dir)?;
    let crash_log_path = log_dir.join(CRASH_LOG_FILE_NAME);
    let lock = CRASH_LOG_WRITE_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut writer = SizeLimitedLogWriter::open(&crash_log_path, max_len)?;
    writer.write_all(report.as_bytes())?;
    writer.flush()
}

fn record_panic_to_logs(info: &PanicHookInfo<'_>) {
    let timestamp = chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, false);
    let current_thread = std::thread::current();
    let thread_name = current_thread.name().unwrap_or("<unnamed>");
    let location = info.location().map_or_else(
        || "<unknown>".to_owned(),
        |loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()),
    );
    let raw_payload = panic_payload_message(info);
    let redacted_payload = oxideterm_audit::redact(&raw_payload);
    let raw_backtrace = Backtrace::force_capture().to_string();
    let report = format_panic_report(
        &timestamp,
        thread_name,
        &location,
        &raw_payload,
        &raw_backtrace,
    );

    if let Some(log_dir) = CRASH_LOG_DIR.get() {
        let _ = write_panic_report_to_dir(log_dir, &report, MAX_CRASH_LOG_BYTES);
    }

    tracing::error!(
        thread = thread_name,
        location = %location,
        message = %redacted_payload.as_str(),
        "Unhandled Rust panic recorded"
    );
}

fn remove_legacy_daily_logs(log_dir: &Path) -> io::Result<()> {
    for entry in std::fs::read_dir(log_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && entry
                .file_name()
                .to_str()
                .is_some_and(is_legacy_daily_log_name)
        {
            std::fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

fn is_legacy_daily_log_name(file_name: &str) -> bool {
    let Some(date) = file_name.strip_prefix(LEGACY_LOG_FILE_PREFIX) else {
        return false;
    };
    let bytes = date.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(test_name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "oxideterm-logging-{test_name}-{}",
                uuid::Uuid::new_v4()
            ));
            std::fs::create_dir_all(&path).expect("create test directory");
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn size_limited_writer_compacts_history_and_replaces_oversized_entries() {
        let directory = TestDirectory::new("bounded-file");
        let log_path = directory.0.join(LOG_FILE_NAME);
        let mut writer = SizeLimitedLogWriter::open(&log_path, 64).expect("open bounded log");

        writer
            .write_all(b"old-entry-one\nold-entry-two\nold-entry-three\n")
            .expect("write old entries");
        writer
            .write_all(b"new-entry-that-must-remain-after-size-compaction\n")
            .expect("write newest entry");
        writer.flush().expect("flush bounded log");

        let contents = std::fs::read_to_string(&log_path).expect("read bounded log");
        assert!(contents.len() <= 64);
        assert!(contents.contains("new-entry-that-must-remain-after-size-compaction"));
        assert!(!contents.contains("old-entry-one"));
        assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 1);

        writer.write_all(&[b'x'; 128]).unwrap();
        writer.flush().unwrap();
        let contents = std::fs::read_to_string(log_path).unwrap();
        assert_eq!(contents, "[oversized log entry omitted]\n");
        assert!(contents.len() <= 64);
    }

    #[test]
    fn legacy_cleanup_only_removes_daily_log_files() {
        let directory = TestDirectory::new("legacy-cleanup");
        let legacy_log = directory.0.join("oxideterm-native.2026-07-17");
        let current_log = directory.0.join(LOG_FILE_NAME);
        let unrelated_log = directory.0.join("oxideterm-native.backup");
        std::fs::write(&legacy_log, "legacy").unwrap();
        std::fs::write(&current_log, "current").unwrap();
        std::fs::write(&unrelated_log, "unrelated").unwrap();

        remove_legacy_daily_logs(&directory.0).expect("remove legacy logs");

        assert!(!legacy_log.exists());
        assert!(current_log.exists());
        assert!(unrelated_log.exists());
    }

    #[test]
    fn panic_report_redacts_secrets_and_writes_bounded_crash_log() {
        let directory = TestDirectory::new("panic-report");
        let report = format_panic_report(
            "2026-11-20T21:20:00.000+08:00",
            "tokio-worker",
            "crates/example/src/lib.rs:42:9",
            "dial failed for ssh://alice:super-secret@host.example:22 with password=topsecret",
            "   0: std::panicking::begin_panic",
        );

        assert!(report.contains("thread: tokio-worker"));
        assert!(report.contains("location: crates/example/src/lib.rs:42:9"));
        assert!(!report.contains("super-secret"));
        assert!(!report.contains("topsecret"));
        assert!(report.contains("ssh://alice:[REDACTED]@host.example:22"));

        write_panic_report_to_dir(&directory.0, &report, MAX_CRASH_LOG_BYTES)
            .expect("write crash report");
        let crash_path = directory.0.join(CRASH_LOG_FILE_NAME);
        let written = std::fs::read_to_string(&crash_path).expect("read crash report");
        assert_eq!(written, report);
        assert!(written.len() as u64 <= MAX_CRASH_LOG_BYTES);
    }

    #[test]
    fn default_and_debug_filters_parse_cleanly_with_platform_and_runtime_targets() {
        let default_directives = build_log_filter(false).to_string();
        assert!(default_directives.contains("oxideterm_ssh=info"));
        assert!(default_directives.contains("oxideterm_terminal=info"));

        let debug_directives = build_log_filter(true).to_string();
        assert!(debug_directives.contains("oxideterm_gpui_app=debug"));
        assert!(debug_directives.contains("oxideterm_forwarding=debug"));
        assert!(debug_directives.contains("gpui_pre=info"));
        assert!(debug_directives.contains("gpui_pre_wgpu=info"));
        assert!(debug_directives.contains("gpui_pre_windows=info"));
        assert!(!debug_directives.contains(",gpui="));
    }
}
