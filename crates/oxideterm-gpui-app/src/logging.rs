use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result};
use oxideterm_settings::PersistedSettings;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

const LOG_FILE_NAME: &str = "oxideterm-native.log";
const LEGACY_LOG_FILE_PREFIX: &str = "oxideterm-native.";
const MAX_LOG_FILE_BYTES: u64 = 10 * 1024 * 1024;
const OVERSIZED_LOG_ENTRY_MARKER: &[u8] = b"[oversized log entry omitted]\n";
const DEFAULT_LOG_FILTER: &str = "warn,oxideterm_gpui_app=info,oxideterm_ssh=info";
const DEBUG_LOG_FILTER: &str = "warn,oxideterm_gpui_app=debug,oxideterm_ssh=debug,gpui=info";

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
        log_path = %log_path.display(),
        max_log_file_bytes = MAX_LOG_FILE_BYTES,
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
    fn size_limited_writer_keeps_latest_logs_in_one_bounded_file() {
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
    fn oversized_log_entry_is_replaced_without_exceeding_the_limit() {
        let directory = TestDirectory::new("oversized-entry");
        let log_path = directory.0.join(LOG_FILE_NAME);
        let mut writer = SizeLimitedLogWriter::open(&log_path, 64).expect("open bounded log");

        writer.write_all(&[b'x'; 128]).unwrap();
        writer.flush().unwrap();

        let contents = std::fs::read_to_string(log_path).unwrap();
        assert_eq!(
            contents,
            std::str::from_utf8(OVERSIZED_LOG_ENTRY_MARKER).unwrap()
        );
        assert!(contents.len() <= 64);
    }
}
