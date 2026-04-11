// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Session-scoped ephemeral terminal history archive.
//!
//! This stores only lines evicted from the in-memory hot buffer. Archives live
//! for the lifetime of a backend session and are deleted when that session is
//! destroyed or during startup janitor cleanup after crashes.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    mpsc::{self, Receiver, SyncSender, TrySendError},
};
use std::thread;
use tracing::{debug, warn};

use super::scroll_buffer::TerminalLine;

const HISTORY_ARCHIVE_ROOT: &str = "terminal-history";
const MANIFEST_FILE_NAME: &str = "manifest.json";
const CHUNK_LINE_LIMIT: usize = 1_000;
const CHUNK_BYTE_LIMIT: usize = 512 * 1024;
const ARCHIVE_QUEUE_CAPACITY: usize = 64;

#[derive(Debug, thiserror::Error)]
pub enum TerminalHistoryArchiveError {
    #[error("failed to resolve archive root: {0}")]
    Path(String),

    #[error("archive I/O failed: {0}")]
    Io(#[from] std::io::Error),

    #[error("archive serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("archive compression failed: {0}")]
    Compression(String),

    #[error("archive worker unavailable")]
    WorkerUnavailable,

    #[error("archive record not found: {0}")]
    NotFound(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ArchiveHealthSnapshot {
    pub available: bool,
    pub degraded: bool,
    pub closing: bool,
    pub queued_commands: usize,
    pub max_queue_depth: usize,
    pub dropped_appends: u64,
    pub dropped_lines: u64,
    pub sealed_chunks: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ArchivedLineRecord {
    pub line_number: u64,
    pub timestamp: u64,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ansi_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchivedChunkMetadata {
    pub id: String,
    pub path: String,
    pub first_line: u64,
    pub last_line: u64,
    pub line_count: u64,
    pub compressed_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalHistoryManifest {
    pub version: u32,
    pub session_id: String,
    pub delete_on_close: bool,
    pub chunks: Vec<ArchivedChunkMetadata>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArchivedExcerptLine {
    pub line_number: u64,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ansi_text: Option<String>,
    pub is_match: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArchivedHistoryExcerpt {
    pub chunk_id: String,
    pub start_line_number: u64,
    pub end_line_number: u64,
    pub lines: Vec<ArchivedExcerptLine>,
}

enum ArchiveCommand {
    Append(Vec<TerminalLine>),
    Flush(mpsc::Sender<Result<(), String>>),
    Delete,
}

#[derive(Clone)]
pub struct TerminalHistoryArchive {
    session_dir: PathBuf,
    state: Arc<Mutex<ArchiveHandleState>>,
}

struct ArchiveHandleState {
    tx: SyncSender<ArchiveCommand>,
    closing: bool,
    telemetry: Arc<Mutex<ArchiveTelemetry>>,
}

#[derive(Debug, Default)]
struct ArchiveTelemetry {
    available: bool,
    degraded: bool,
    closing: bool,
    queued_commands: usize,
    max_queue_depth: usize,
    dropped_appends: u64,
    dropped_lines: u64,
    sealed_chunks: usize,
    last_error: Option<String>,
}

impl ArchiveTelemetry {
    fn snapshot(&self) -> ArchiveHealthSnapshot {
        ArchiveHealthSnapshot {
            available: self.available,
            degraded: self.degraded,
            closing: self.closing,
            queued_commands: self.queued_commands,
            max_queue_depth: self.max_queue_depth,
            dropped_appends: self.dropped_appends,
            dropped_lines: self.dropped_lines,
            sealed_chunks: self.sealed_chunks,
            last_error: self.last_error.clone(),
        }
    }

    fn mark_enqueued(&mut self) {
        self.queued_commands += 1;
        self.max_queue_depth = self.max_queue_depth.max(self.queued_commands);
    }

    fn mark_received(&mut self) {
        self.queued_commands = self.queued_commands.saturating_sub(1);
    }

    fn mark_chunk_sealed(&mut self, sealed_chunks: usize) {
        self.available = true;
        self.sealed_chunks = sealed_chunks;
    }

    fn mark_closing(&mut self) {
        self.closing = true;
    }

    fn record_drop(&mut self, lines: usize, message: &str) -> bool {
        let first_degrade = !self.degraded;
        self.degraded = true;
        self.dropped_appends += 1;
        self.dropped_lines += lines as u64;
        self.last_error = Some(message.to_string());
        first_degrade
    }

    fn record_error(&mut self, message: &str) {
        self.available = false;
        self.degraded = true;
        self.last_error = Some(message.to_string());
    }
}

impl TerminalHistoryArchive {
    pub fn new(session_id: &str) -> Result<Self, TerminalHistoryArchiveError> {
        let root = archive_root_dir()?;
        Self::new_in(root, session_id)
    }

    pub fn new_in(
        root_dir: PathBuf,
        session_id: &str,
    ) -> Result<Self, TerminalHistoryArchiveError> {
        Self::new_in_with_capacity(root_dir, session_id, ARCHIVE_QUEUE_CAPACITY)
    }

    fn new_in_with_capacity(
        root_dir: PathBuf,
        session_id: &str,
        queue_capacity: usize,
    ) -> Result<Self, TerminalHistoryArchiveError> {
        let session_dir = root_dir.join(session_id);
        fs::create_dir_all(&session_dir)?;

        let telemetry = Arc::new(Mutex::new(ArchiveTelemetry {
            available: true,
            ..ArchiveTelemetry::default()
        }));
        let (tx, rx) = mpsc::sync_channel::<ArchiveCommand>(queue_capacity);
        let state = ArchiveWriterState::new(
            session_dir.clone(),
            session_id.to_string(),
            telemetry.clone(),
        )?;

        thread::Builder::new()
            .name(format!("terminal-history-{session_id}"))
            .spawn(move || archive_worker_loop(rx, state))
            .map_err(TerminalHistoryArchiveError::Io)?;

        Ok(Self {
            session_dir,
            state: Arc::new(Mutex::new(ArchiveHandleState {
                tx,
                closing: false,
                telemetry,
            })),
        })
    }

    pub fn append_lines(&self, lines: Vec<TerminalLine>) {
        if lines.is_empty() {
            return;
        }

        let line_count = lines.len();
        let mut state = self.lock_state();
        if state.closing {
            return;
        }

        match state.tx.try_send(ArchiveCommand::Append(lines)) {
            Ok(()) => {
                lock_telemetry(&state.telemetry).mark_enqueued();
            }
            Err(TrySendError::Full(ArchiveCommand::Append(lines))) => {
                let should_warn = {
                    let mut telemetry = lock_telemetry(&state.telemetry);
                    telemetry.record_drop(lines.len(), "archive queue full")
                };
                if should_warn {
                    warn!(
                        "Terminal history archive queue reached capacity for {:?}; subsequent deep-history results may be partial",
                        self.session_dir
                    );
                }
            }
            Err(TrySendError::Disconnected(ArchiveCommand::Append(lines))) => {
                state.closing = true;
                let mut telemetry = lock_telemetry(&state.telemetry);
                telemetry.record_drop(lines.len(), "archive worker unavailable");
                telemetry.record_error("archive worker unavailable");
                warn!(
                    "Terminal history archive worker unavailable for {:?}; dropped {} evicted line(s)",
                    self.session_dir, line_count
                );
            }
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {}
        }
    }

    pub fn flush(&self) -> Result<(), TerminalHistoryArchiveError> {
        let (tx, rx) = mpsc::channel();
        let state = self.lock_state();
        if state.closing {
            return Err(TerminalHistoryArchiveError::WorkerUnavailable);
        }

        state
            .tx
            .send(ArchiveCommand::Flush(tx))
            .map_err(|_| TerminalHistoryArchiveError::WorkerUnavailable)?;
        lock_telemetry(&state.telemetry).mark_enqueued();
        drop(state);

        match rx.recv() {
            Ok(Ok(())) => Ok(()),
            Ok(Err(message)) => Err(TerminalHistoryArchiveError::Path(message)),
            Err(_) => Err(TerminalHistoryArchiveError::WorkerUnavailable),
        }
    }

    pub fn schedule_delete(&self) {
        let (tx, telemetry) = {
            let mut state = self.lock_state();
            if state.closing {
                return;
            }

            state.closing = true;
            let telemetry = state.telemetry.clone();
            lock_telemetry(&telemetry).mark_closing();
            (state.tx.clone(), telemetry)
        };

        thread::spawn(move || {
            if tx.send(ArchiveCommand::Delete).is_ok() {
                lock_telemetry(&telemetry).mark_enqueued();
            }
        });
    }

    pub fn session_dir(&self) -> PathBuf {
        self.session_dir.clone()
    }

    pub fn health_snapshot(&self) -> ArchiveHealthSnapshot {
        let state = self.lock_state();
        let mut snapshot = lock_telemetry(&state.telemetry).snapshot();
        snapshot.closing = snapshot.closing || state.closing;
        snapshot
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, ArchiveHandleState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

pub fn cleanup_stale_terminal_history_archives() -> Result<usize, TerminalHistoryArchiveError> {
    let root = archive_root_dir()?;
    cleanup_stale_archives_in(&root)
}

pub(crate) fn load_manifest(
    session_dir: &Path,
) -> Result<TerminalHistoryManifest, TerminalHistoryArchiveError> {
    let manifest_path = session_dir.join(MANIFEST_FILE_NAME);
    if !manifest_path.exists() {
        return Err(TerminalHistoryArchiveError::NotFound(format!(
            "manifest missing for {}",
            session_dir.display()
        )));
    }

    let manifest_json = fs::read_to_string(manifest_path)?;
    Ok(serde_json::from_str(&manifest_json)?)
}

pub(crate) fn read_chunk_records(
    session_dir: &Path,
    chunk: &ArchivedChunkMetadata,
) -> Result<Vec<ArchivedLineRecord>, TerminalHistoryArchiveError> {
    let chunk_path = session_dir.join(&chunk.path);
    let compressed = fs::read(chunk_path)?;
    let decoded = zstd::stream::decode_all(Cursor::new(compressed))
        .map_err(|error| TerminalHistoryArchiveError::Compression(error.to_string()))?;

    decoded
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).map_err(TerminalHistoryArchiveError::from))
        .collect()
}

pub(crate) fn get_archived_excerpt(
    session_dir: &Path,
    chunk_id: &str,
    line_number: u64,
    context_lines: usize,
) -> Result<ArchivedHistoryExcerpt, TerminalHistoryArchiveError> {
    let manifest = load_manifest(session_dir)?;
    let chunk = manifest
        .chunks
        .iter()
        .find(|chunk| chunk.id == chunk_id)
        .cloned()
        .ok_or_else(|| TerminalHistoryArchiveError::NotFound(format!("chunk {}", chunk_id)))?;
    let records = read_chunk_records(session_dir, &chunk)?;
    let match_index = records
        .iter()
        .position(|record| record.line_number == line_number)
        .ok_or_else(|| TerminalHistoryArchiveError::NotFound(format!("line {}", line_number)))?;

    let start = match_index.saturating_sub(context_lines);
    let end = (match_index + context_lines + 1).min(records.len());
    let excerpt_lines: Vec<ArchivedExcerptLine> = records[start..end]
        .iter()
        .map(|record| ArchivedExcerptLine {
            line_number: record.line_number,
            text: record.text.clone(),
            ansi_text: record.ansi_text.clone(),
            is_match: record.line_number == line_number,
        })
        .collect();

    Ok(ArchivedHistoryExcerpt {
        chunk_id: chunk.id,
        start_line_number: excerpt_lines
            .first()
            .map(|line| line.line_number)
            .unwrap_or(line_number),
        end_line_number: excerpt_lines
            .last()
            .map(|line| line.line_number)
            .unwrap_or(line_number),
        lines: excerpt_lines,
    })
}

fn cleanup_stale_archives_in(root: &Path) -> Result<usize, TerminalHistoryArchiveError> {
    if !root.exists() {
        return Ok(0);
    }

    let mut removed = 0;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            fs::remove_dir_all(&path)?;
            removed += 1;
        } else {
            fs::remove_file(&path)?;
        }
    }

    Ok(removed)
}

fn archive_root_dir() -> Result<PathBuf, TerminalHistoryArchiveError> {
    let base_dir = crate::config::storage::config_dir()
        .map_err(|error| TerminalHistoryArchiveError::Path(error.to_string()))?;
    Ok(base_dir.join(HISTORY_ARCHIVE_ROOT))
}

fn archive_worker_loop(rx: Receiver<ArchiveCommand>, mut state: ArchiveWriterState) {
    while let Ok(command) = rx.recv() {
        state.telemetry_mark_received();

        match command {
            ArchiveCommand::Append(lines) => {
                if let Err(error) = state.append_lines(lines) {
                    let message = error.to_string();
                    state.telemetry_record_error(&message);
                    warn!(
                        "Terminal history archive degraded for session {}: {}",
                        state.manifest.session_id, message
                    );
                    break;
                }
            }
            ArchiveCommand::Flush(response_tx) => {
                let result = state.flush().map_err(|error| error.to_string());
                if let Err(error) = &result {
                    state.telemetry_record_error(error);
                }
                let _ = response_tx.send(result);
            }
            ArchiveCommand::Delete => {
                let session_id = state.manifest.session_id.clone();
                if let Err(error) = state.delete_dir() {
                    state.telemetry_record_error(&error.to_string());
                    warn!(
                        "Failed to delete terminal history archive for session {}: {}",
                        session_id, error
                    );
                }
                return;
            }
        }
    }

    debug!(
        "Terminal history archive worker stopped for session {}",
        state.manifest.session_id
    );
}

struct ArchiveWriterState {
    session_dir: PathBuf,
    manifest_path: PathBuf,
    manifest: TerminalHistoryManifest,
    next_chunk_number: u64,
    next_line_number: u64,
    active_chunk_first_line: Option<u64>,
    active_chunk_line_count: usize,
    active_chunk_bytes: usize,
    active_chunk_payload: Vec<u8>,
    telemetry: Arc<Mutex<ArchiveTelemetry>>,
}

impl ArchiveWriterState {
    fn new(
        session_dir: PathBuf,
        session_id: String,
        telemetry: Arc<Mutex<ArchiveTelemetry>>,
    ) -> Result<Self, TerminalHistoryArchiveError> {
        let manifest = TerminalHistoryManifest {
            version: 1,
            session_id,
            delete_on_close: true,
            chunks: Vec::new(),
        };
        let manifest_path = session_dir.join(MANIFEST_FILE_NAME);
        write_json_atomic(&manifest_path, &manifest)?;

        Ok(Self {
            session_dir,
            manifest_path,
            manifest,
            next_chunk_number: 1,
            next_line_number: 0,
            active_chunk_first_line: None,
            active_chunk_line_count: 0,
            active_chunk_bytes: 0,
            active_chunk_payload: Vec::new(),
            telemetry,
        })
    }

    fn append_lines(
        &mut self,
        lines: Vec<TerminalLine>,
    ) -> Result<(), TerminalHistoryArchiveError> {
        for line in lines {
            self.append_line(line)?;
        }
        Ok(())
    }

    fn append_line(&mut self, line: TerminalLine) -> Result<(), TerminalHistoryArchiveError> {
        let line_number = self.next_line_number;
        self.next_line_number += 1;

        if self.active_chunk_first_line.is_none() {
            self.active_chunk_first_line = Some(line_number);
        }

        let record = ArchivedLineRecord {
            line_number,
            timestamp: line.timestamp,
            text: line.text,
            ansi_text: line.ansi_text,
        };

        let mut encoded = serde_json::to_vec(&record)?;
        encoded.push(b'\n');

        self.active_chunk_bytes += encoded.len();
        self.active_chunk_line_count += 1;
        self.active_chunk_payload.extend_from_slice(&encoded);

        if self.active_chunk_line_count >= CHUNK_LINE_LIMIT
            || self.active_chunk_bytes >= CHUNK_BYTE_LIMIT
        {
            self.flush()?;
        }

        Ok(())
    }

    fn flush(&mut self) -> Result<(), TerminalHistoryArchiveError> {
        if self.active_chunk_line_count == 0 {
            return Ok(());
        }

        let first_line = self
            .active_chunk_first_line
            .expect("active chunk line count implies first line");
        let last_line = self.next_line_number.saturating_sub(1);
        let chunk_id = format!("{:06}", self.next_chunk_number);
        let chunk_name = format!("{chunk_id}.ndjson.zst");
        let chunk_path = self.session_dir.join(&chunk_name);

        let compressed = zstd::stream::encode_all(Cursor::new(&self.active_chunk_payload), 3)
            .map_err(|error| TerminalHistoryArchiveError::Compression(error.to_string()))?;
        write_bytes_atomic(&chunk_path, &compressed)?;

        self.manifest.chunks.push(ArchivedChunkMetadata {
            id: chunk_id,
            path: chunk_name,
            first_line,
            last_line,
            line_count: self.active_chunk_line_count as u64,
            compressed_bytes: compressed.len() as u64,
        });
        write_json_atomic(&self.manifest_path, &self.manifest)?;

        self.next_chunk_number += 1;
        self.active_chunk_first_line = None;
        self.active_chunk_line_count = 0;
        self.active_chunk_bytes = 0;
        self.active_chunk_payload.clear();
        lock_telemetry(&self.telemetry).mark_chunk_sealed(self.manifest.chunks.len());

        Ok(())
    }

    fn delete_dir(&mut self) -> Result<(), TerminalHistoryArchiveError> {
        self.active_chunk_payload.clear();
        self.active_chunk_line_count = 0;
        self.active_chunk_bytes = 0;

        if self.session_dir.exists() {
            fs::remove_dir_all(&self.session_dir)?;
        }

        Ok(())
    }

    fn telemetry_mark_received(&self) {
        lock_telemetry(&self.telemetry).mark_received();
    }

    fn telemetry_record_error(&self, message: &str) {
        lock_telemetry(&self.telemetry).record_error(message);
    }
}

fn write_json_atomic<T: Serialize>(
    path: &Path,
    value: &T,
) -> Result<(), TerminalHistoryArchiveError> {
    let payload = serde_json::to_vec_pretty(value)?;
    write_bytes_atomic(path, &payload)
}

fn write_bytes_atomic(path: &Path, payload: &[u8]) -> Result<(), TerminalHistoryArchiveError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let temp_path = path.with_file_name(format!(
        "{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("archive")
    ));
    fs::write(&temp_path, payload)?;
    fs::rename(&temp_path, path)?;
    Ok(())
}

fn lock_telemetry(
    telemetry: &Arc<Mutex<ArchiveTelemetry>>,
) -> std::sync::MutexGuard<'_, ArchiveTelemetry> {
    telemetry
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_archive_flush_writes_manifest_and_chunk() {
        let temp_dir = TempDir::new().unwrap();
        let archive =
            TerminalHistoryArchive::new_in(temp_dir.path().to_path_buf(), "session-1").unwrap();

        archive.append_lines(vec![
            TerminalLine::with_ansi_timestamp("hello".to_string(), None, 1),
            TerminalLine::with_ansi_timestamp(
                "world".to_string(),
                Some("\x1b[31mworld\x1b[0m".to_string()),
                2,
            ),
        ]);
        archive.flush().unwrap();

        let session_dir = archive.session_dir();
        let manifest = load_manifest(&session_dir).unwrap();

        assert_eq!(manifest.session_id, "session-1");
        assert_eq!(manifest.chunks.len(), 1);

        let chunk_path = session_dir.join(&manifest.chunks[0].path);
        assert!(chunk_path.exists());

        let compressed = fs::read(chunk_path).unwrap();
        let decoded = zstd::stream::decode_all(Cursor::new(compressed)).unwrap();
        let content = String::from_utf8(decoded).unwrap();
        assert!(content.contains("\"text\":\"hello\""));
        assert!(content.contains("\"text\":\"world\""));
    }

    #[test]
    fn test_archive_delete_removes_session_directory() {
        let temp_dir = TempDir::new().unwrap();
        let archive =
            TerminalHistoryArchive::new_in(temp_dir.path().to_path_buf(), "session-2").unwrap();

        let session_dir = archive.session_dir();
        assert!(session_dir.exists());

        archive.schedule_delete();
        std::thread::sleep(std::time::Duration::from_millis(50));

        assert!(!session_dir.exists());
    }

    #[test]
    fn test_cleanup_stale_archives_removes_children() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().join(HISTORY_ARCHIVE_ROOT);
        fs::create_dir_all(root.join("a")).unwrap();
        fs::create_dir_all(root.join("b")).unwrap();

        let removed = cleanup_stale_archives_in(&root).unwrap();

        assert_eq!(removed, 2);
        assert!(!root.join("a").exists());
        assert!(!root.join("b").exists());
    }

    #[test]
    fn test_read_chunk_records_and_excerpt() {
        let temp_dir = TempDir::new().unwrap();
        let archive =
            TerminalHistoryArchive::new_in(temp_dir.path().to_path_buf(), "session-3").unwrap();

        archive.append_lines(vec![
            TerminalLine::with_ansi_timestamp("alpha".to_string(), None, 1),
            TerminalLine::with_ansi_timestamp("beta needle gamma".to_string(), None, 2),
            TerminalLine::with_ansi_timestamp("delta".to_string(), None, 3),
        ]);
        archive.flush().unwrap();

        let session_dir = archive.session_dir();
        let manifest = load_manifest(&session_dir).unwrap();
        let records = read_chunk_records(&session_dir, &manifest.chunks[0]).unwrap();
        assert_eq!(records.len(), 3);
        assert_eq!(records[1].line_number, 1);

        let excerpt = get_archived_excerpt(&session_dir, &manifest.chunks[0].id, 1, 1).unwrap();
        assert_eq!(excerpt.lines.len(), 3);
        assert!(excerpt.lines[1].is_match);
        assert_eq!(excerpt.lines[1].text, "beta needle gamma");
    }

    #[test]
    fn test_health_snapshot_reports_sealed_chunks() {
        let temp_dir = TempDir::new().unwrap();
        let archive =
            TerminalHistoryArchive::new_in(temp_dir.path().to_path_buf(), "session-4").unwrap();

        archive.append_lines(vec![TerminalLine::with_ansi_timestamp(
            "line".to_string(),
            None,
            1,
        )]);
        archive.flush().unwrap();

        let health = archive.health_snapshot();
        assert!(health.available);
        assert_eq!(health.sealed_chunks, 1);
    }
}
