use crate::{AuditError, AuditQuery, AuditStore, redact};
use serde::{Deserialize, Serialize};
use std::{
    cell::Cell,
    io::Write,
    path::Path,
    time::{Duration, Instant},
};

const EXPORT_MAX_WAL_GROWTH: u64 = 64 * 1024 * 1024;
const EXPORT_MAX_DURATION: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditExportFormat {
    Json,
    Csv,
}

impl AuditStore {
    /// Streams bounded pages. The caller owns the selected destination and any
    /// incomplete-file cleanup; no decrypted search index or temporary dump exists.
    pub fn export(
        &self,
        query: &AuditQuery,
        format: AuditExportFormat,
        details: bool,
        output: impl Write,
    ) -> Result<u64, AuditError> {
        self.export_until_cancelled(query, format, details, output, &|| false)
    }

    pub(crate) fn export_until_cancelled(
        &self,
        query: &AuditQuery,
        format: AuditExportFormat,
        details: bool,
        output: impl Write,
        cancelled: &impl Fn() -> bool,
    ) -> Result<u64, AuditError> {
        self.export_with_limits(
            query,
            format,
            details,
            output,
            cancelled,
            EXPORT_MAX_WAL_GROWTH,
            EXPORT_MAX_DURATION,
        )
    }

    fn export_with_limits(
        &self,
        query: &AuditQuery,
        format: AuditExportFormat,
        details: bool,
        mut output: impl Write,
        cancelled: &impl Fn() -> bool,
        max_wal_growth: u64,
        max_duration: Duration,
    ) -> Result<u64, AuditError> {
        if cancelled() {
            return Err(AuditError::Closed);
        }
        let wal = self.wal_path()?;
        let initial_wal_bytes = wal.as_deref().map(wal_bytes).transpose()?.unwrap_or(0);
        let started = Instant::now();
        let checks = Cell::new(0u32);
        let limit_error = Cell::new(None);
        let check_limit = |force: bool| {
            if cancelled() {
                return true;
            }
            let next = checks.get().wrapping_add(1);
            checks.set(next);
            if !force && !next.is_multiple_of(128) {
                return false;
            }
            if started.elapsed() >= max_duration {
                limit_error.set(Some(AuditError::ExportLimit));
                return true;
            }
            if let Some(path) = wal.as_deref() {
                match wal_bytes(path) {
                    Ok(current) if current.saturating_sub(initial_wal_bytes) > max_wal_growth => {
                        limit_error.set(Some(AuditError::ExportLimit));
                        return true;
                    }
                    Err(error) => {
                        limit_error.set(Some(error));
                        return true;
                    }
                    _ => {}
                }
            }
            false
        };
        let interrupted = || limit_error.get().unwrap_or(AuditError::Closed);
        // All pages see one database snapshot while the WAL writer continues independently.
        let _snapshot = self.read_snapshot()?;
        let mut query = query.clone();
        if let Some(before) = query.before_sequence {
            query.sequence_ceiling = Some(
                query
                    .sequence_ceiling
                    .map_or(before, |existing| existing.min(before)),
            );
        }
        query.limit = 100;
        let mut count = 0;
        match format {
            AuditExportFormat::Json => output.write_all(b"["),
            AuditExportFormat::Csv => output.write_all(
                b"time_utc_ms,category,severity,operation,source,actor,target,outcome,detail\r\n",
            ),
        }
        .map_err(|_| AuditError::Storage)?;
        loop {
            if check_limit(true) {
                return Err(interrupted());
            }
            let page = self
                .query_until_cancelled(&query, &|| check_limit(false))
                .map_err(|error| {
                    if error == AuditError::Closed {
                        interrupted()
                    } else {
                        error
                    }
                })?;
            for mut record in page.records {
                if check_limit(false) {
                    return Err(interrupted());
                }
                // Export repeats redaction at the plaintext boundary, including
                // records created by collectors that only supplied metadata.
                record.details.title = redact(&record.details.title);
                record.details.target = record.details.target.as_ref().map(|s| redact(s));
                record.details.detail = if details {
                    record.details.detail.as_ref().map(|s| redact(s))
                } else {
                    None
                };
                match format {
                    AuditExportFormat::Json => {
                        if count > 0 {
                            output.write_all(b",\n").map_err(|_| AuditError::Storage)?;
                        }
                        serde_json::to_writer(&mut output, &record)
                            .map_err(|_| AuditError::Storage)?;
                    }
                    AuditExportFormat::Csv => {
                        let operation = record.details.operation.as_ref();
                        let outcome = operation
                            .map(|op| format!("{:?}", op.outcome).to_lowercase())
                            .unwrap_or_default();
                        let time = record.occurred_at_ms.to_string();
                        let fields = [
                            time.as_str(),
                            record.category.key(),
                            record.severity.key(),
                            record.details.title.as_str(),
                            record.details.source.as_str(),
                            record.details.actor.as_str(),
                            record
                                .details
                                .target
                                .as_ref()
                                .map(|s| s.as_str())
                                .unwrap_or(""),
                            &outcome,
                            record
                                .details
                                .detail
                                .as_ref()
                                .map(|s| s.as_str())
                                .unwrap_or(""),
                        ];
                        for (index, field) in fields.into_iter().enumerate() {
                            if index > 0 {
                                output.write_all(b",").map_err(|_| AuditError::Storage)?;
                            }
                            write_csv_field(&mut output, field)?;
                        }
                        output.write_all(b"\r\n").map_err(|_| AuditError::Storage)?;
                    }
                }
                count += 1;
            }
            match page.next_cursor {
                Some(cursor) => query.before_sequence = Some(cursor),
                None => break,
            }
        }
        if check_limit(true) {
            return Err(interrupted());
        }
        if matches!(format, AuditExportFormat::Json) {
            output.write_all(b"]\n").map_err(|_| AuditError::Storage)?;
        }
        output.flush().map_err(|_| AuditError::Storage)?;
        Ok(count)
    }
}

fn wal_bytes(path: &Path) -> Result<u64, AuditError> {
    match std::fs::metadata(path) {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(_) => Err(AuditError::Storage),
    }
}

fn write_csv_field(output: &mut impl Write, value: &str) -> Result<(), AuditError> {
    output.write_all(b"\"").map_err(|_| AuditError::Storage)?;
    if value.trim_start().starts_with(['=', '+', '-', '@']) || value.starts_with(['\t', '\r', '\n'])
    {
        output.write_all(b"'").map_err(|_| AuditError::Storage)?;
    }
    for byte in value.bytes() {
        if byte == b'"' {
            output.write_all(b"\"").map_err(|_| AuditError::Storage)?;
        }
        output.write_all(&[byte]).map_err(|_| AuditError::Storage)?;
    }
    output.write_all(b"\"").map_err(|_| AuditError::Storage)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AuditKeyProvider;
    use zeroize::Zeroizing;

    struct Keys;
    impl AuditKeyProvider for Keys {
        fn load(&self, _: &str) -> Result<Zeroizing<Vec<u8>>, AuditError> {
            Ok(Zeroizing::new(vec![19; 32]))
        }
        fn create(&self, id: &str) -> Result<Zeroizing<Vec<u8>>, AuditError> {
            self.load(id)
        }
    }

    struct GrowingWriter {
        db: rusqlite::Connection,
        grown: bool,
    }
    impl Write for GrowingWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if !self.grown {
                self.db
                    .execute_batch(
                        "CREATE TABLE export_growth(value BLOB);
                    INSERT INTO export_growth VALUES (zeroblob(4096));",
                    )
                    .map_err(std::io::Error::other)?;
                self.grown = true;
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn export_limit_releases_consistent_snapshot_when_wal_grows() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audit.db");
        let store = AuditStore::open(&path, &Keys).unwrap();
        let writer = GrowingWriter {
            db: rusqlite::Connection::open(&path).unwrap(),
            grown: false,
        };
        assert_eq!(
            store.export_with_limits(
                &AuditQuery::default(),
                AuditExportFormat::Json,
                false,
                writer,
                &|| false,
                0,
                Duration::from_secs(60)
            ),
            Err(AuditError::ExportLimit)
        );
        let checkpoint = rusqlite::Connection::open(&path).unwrap();
        let busy: i64 = checkpoint
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))
            .unwrap();
        assert_eq!(busy, 0);
        assert_eq!(
            store.export_with_limits(
                &AuditQuery::default(),
                AuditExportFormat::Json,
                false,
                Vec::new(),
                &|| false,
                u64::MAX,
                Duration::ZERO
            ),
            Err(AuditError::ExportLimit)
        );
    }
}
