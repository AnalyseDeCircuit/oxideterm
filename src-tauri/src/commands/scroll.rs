// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Tauri commands for scroll buffer management

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, State};

use crate::session::history_archive::{get_archived_excerpt, load_manifest, read_chunk_records};
use crate::session::{
    ArchiveHealthSnapshot, ArchivedHistoryExcerpt, BufferStats, SearchOptions, SessionRegistry,
    TerminalLine, search_lines,
};

const TERMINAL_HISTORY_SEARCH_PROGRESS_EVENT: &str = "terminal-history-search-progress";
const SEARCH_JOB_RETENTION: Duration = Duration::from_secs(300);
const MAX_COMPLETED_SEARCH_JOBS: usize = 200;

fn search_jobs() -> &'static DashMap<String, Arc<SearchJobEntry>> {
    static SEARCH_JOBS: OnceLock<DashMap<String, Arc<SearchJobEntry>>> = OnceLock::new();
    SEARCH_JOBS.get_or_init(DashMap::new)
}

struct SearchJobEntry {
    cancel_flag: AtomicBool,
    state: std::sync::Mutex<SearchJobState>,
}

#[derive(Debug, Clone)]
struct SearchJobState {
    session_id: String,
    buffered_matches: Vec<HistorySearchMatch>,
    total_matches: usize,
    duration_ms: u64,
    searched_layers: Vec<HistorySearchSource>,
    searched_chunks: usize,
    total_chunks: Option<usize>,
    truncated: bool,
    partial_failure: bool,
    archive_status: ArchiveHealthSnapshot,
    error: Option<String>,
    done: bool,
    updated_at: Instant,
}

impl SearchJobEntry {
    fn new(session_id: String) -> Self {
        Self {
            cancel_flag: AtomicBool::new(false),
            state: std::sync::Mutex::new(SearchJobState {
                session_id,
                buffered_matches: Vec::new(),
                total_matches: 0,
                duration_ms: 0,
                searched_layers: Vec::new(),
                searched_chunks: 0,
                total_chunks: None,
                truncated: false,
                partial_failure: false,
                archive_status: unavailable_archive_status(),
                error: None,
                done: false,
                updated_at: Instant::now(),
            }),
        }
    }

    fn cancel(&self) {
        self.cancel_flag.store(true, Ordering::Relaxed);
        self.with_state(|state| {
            state.updated_at = Instant::now();
        });
    }

    fn is_cancelled(&self) -> bool {
        self.cancel_flag.load(Ordering::Relaxed)
    }

    fn update_from_progress(&self, progress: &TerminalHistorySearchProgress) {
        self.with_state(|state| {
            if !progress.matches.is_empty() {
                state.buffered_matches.extend(progress.matches.clone());
            }
            state.total_matches = progress.total_matches;
            state.duration_ms = progress.duration_ms;
            state.searched_layers = progress.searched_layers.clone();
            state.searched_chunks = progress.searched_chunks;
            state.total_chunks = progress.total_chunks;
            state.truncated = progress.truncated;
            state.partial_failure = progress.partial_failure;
            state.archive_status = progress.archive_status.clone();
            state.error = progress.error.clone();
            state.done = progress.done;
            state.updated_at = Instant::now();
        });
    }

    fn snapshot_range(
        &self,
        search_id: &str,
        cursor: usize,
    ) -> Result<TerminalHistorySearchResultsResponse, String> {
        self.with_state(|state| {
            state.updated_at = Instant::now();
            if cursor > state.buffered_matches.len() {
                return Err(format!(
                    "Cursor {} is out of range for search {} (buffered matches: {})",
                    cursor,
                    search_id,
                    state.buffered_matches.len()
                ));
            }

            let matches = state.buffered_matches[cursor..].to_vec();
            let next_cursor = cursor + matches.len();

            Ok(TerminalHistorySearchResultsResponse {
                search_id: search_id.to_string(),
                session_id: state.session_id.clone(),
                cursor,
                next_cursor,
                matches,
                total_buffered_matches: state.buffered_matches.len(),
                total_matches: state.total_matches,
                duration_ms: state.duration_ms,
                searched_layers: state.searched_layers.clone(),
                searched_chunks: state.searched_chunks,
                total_chunks: state.total_chunks,
                truncated: state.truncated,
                partial_failure: state.partial_failure,
                archive_status: state.archive_status.clone(),
                done: state.done,
                error: state.error.clone(),
            })
        })
    }

    fn is_stale(&self, now: Instant) -> bool {
        self.with_state(|state| {
            (state.done || self.is_cancelled())
                && now.duration_since(state.updated_at) > SEARCH_JOB_RETENTION
        })
    }

    fn with_state<T>(&self, f: impl FnOnce(&mut SearchJobState) -> T) -> T {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        f(&mut state)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HistorySearchSource {
    Hot,
    Cold,
}

#[derive(Debug, Clone, Serialize)]
pub struct HistorySearchMatch {
    pub source: HistorySearchSource,
    pub line_number: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buffer_line_number: Option<usize>,
    pub column_start: usize,
    pub column_end: usize,
    pub matched_text: String,
    pub line_content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TerminalHistorySearchProgress {
    pub search_id: String,
    pub session_id: String,
    pub done: bool,
    pub matches: Vec<HistorySearchMatch>,
    pub total_matches: usize,
    pub duration_ms: u64,
    pub searched_layers: Vec<HistorySearchSource>,
    pub searched_chunks: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_chunks: Option<usize>,
    pub truncated: bool,
    pub partial_failure: bool,
    pub archive_status: ArchiveHealthSnapshot,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StartTerminalHistorySearchResponse {
    pub search_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TerminalHistorySearchResultsResponse {
    pub search_id: String,
    pub session_id: String,
    pub cursor: usize,
    pub next_cursor: usize,
    pub matches: Vec<HistorySearchMatch>,
    pub total_buffered_matches: usize,
    pub total_matches: usize,
    pub duration_ms: u64,
    pub searched_layers: Vec<HistorySearchSource>,
    pub searched_chunks: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_chunks: Option<usize>,
    pub truncated: bool,
    pub partial_failure: bool,
    pub archive_status: ArchiveHealthSnapshot,
    pub done: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Response for get_all_buffer_lines with truncation metadata
#[derive(Debug, Clone, Serialize)]
pub struct BufferLinesResponse {
    /// The returned lines (may be a subset if truncated)
    pub lines: Vec<TerminalLine>,
    /// Total lines available in the buffer
    pub total_lines: usize,
    /// Number of lines actually returned
    pub returned_lines: usize,
    /// Whether the result was truncated due to the hard limit
    pub truncated: bool,
}

#[tauri::command]
pub async fn start_terminal_history_search(
    app_handle: AppHandle,
    session_id: String,
    options: SearchOptions,
    registry: State<'_, Arc<SessionRegistry>>,
) -> Result<StartTerminalHistorySearchResponse, String> {
    prune_stale_search_jobs();

    let (scroll_buffer, archive) = registry
        .with_session(&session_id, |entry| {
            (
                entry.scroll_buffer.clone(),
                entry.terminal_history_archive.clone(),
            )
        })
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    let search_id = uuid::Uuid::new_v4().to_string();
    let job = Arc::new(SearchJobEntry::new(session_id.clone()));
    search_jobs().insert(search_id.clone(), job.clone());

    let session_id_for_task = session_id.clone();
    let search_id_for_task = search_id.clone();
    tokio::spawn(async move {
        tokio::task::yield_now().await;

        let started_at = Instant::now();
        let limit = normalize_match_limit(options.max_matches);
        let mut emitted_matches = 0usize;
        let mut total_matches = 0usize;
        let mut searched_layers = Vec::new();
        let mut searched_chunks = 0usize;
        let mut total_chunks = None;
        let mut truncated;
        let mut partial_failure = false;
        let mut archive_status = archive
            .as_ref()
            .map(|archive| archive.health_snapshot())
            .unwrap_or_else(unavailable_archive_status);

        let hot_result = search_hot_layer(scroll_buffer, options.clone(), limit).await;
        match hot_result {
            Ok((hot_matches, hot_total, hot_truncated)) => {
                searched_layers.push(HistorySearchSource::Hot);
                total_matches += hot_total;
                emitted_matches += hot_matches.len();
                truncated = hot_truncated;

                publish_search_progress(
                    &app_handle,
                    &job,
                    TerminalHistorySearchProgress {
                        search_id: search_id_for_task.clone(),
                        session_id: session_id_for_task.clone(),
                        done: truncated || archive.is_none(),
                        matches: hot_matches,
                        total_matches,
                        duration_ms: started_at.elapsed().as_millis() as u64,
                        searched_layers: searched_layers.clone(),
                        searched_chunks,
                        total_chunks,
                        truncated,
                        partial_failure,
                        archive_status: archive_status.clone(),
                        error: None,
                    },
                );

                if truncated || archive.is_none() {
                    return;
                }
            }
            Err(error) => {
                publish_search_progress(
                    &app_handle,
                    &job,
                    TerminalHistorySearchProgress {
                        search_id: search_id_for_task.clone(),
                        session_id: session_id_for_task.clone(),
                        done: true,
                        matches: Vec::new(),
                        total_matches: 0,
                        duration_ms: started_at.elapsed().as_millis() as u64,
                        searched_layers,
                        searched_chunks,
                        total_chunks,
                        truncated: false,
                        partial_failure: false,
                        archive_status,
                        error: Some(error),
                    },
                );
                return;
            }
        }

        if let Some(archive) = archive {
            archive_status = archive.health_snapshot();
            let session_dir = archive.session_dir();

            match load_manifest(&session_dir) {
                Ok(manifest) => {
                    total_chunks = Some(manifest.chunks.len());
                    if !manifest.chunks.is_empty() {
                        searched_layers.push(HistorySearchSource::Cold);
                    }

                    for chunk in manifest.chunks.iter().rev() {
                        if job.is_cancelled() {
                            break;
                        }

                        if emitted_matches >= limit {
                            truncated = true;
                            break;
                        }

                        let remaining_limit = remaining_limit(limit, emitted_matches);
                        match search_cold_chunk(&session_dir, chunk, &options, remaining_limit) {
                            Ok((matches, found_total, chunk_truncated)) => {
                                searched_chunks += 1;
                                total_matches += found_total;
                                emitted_matches += matches.len();
                                truncated = truncated || chunk_truncated;

                                if !matches.is_empty() || chunk_truncated {
                                    publish_search_progress(
                                        &app_handle,
                                        &job,
                                        TerminalHistorySearchProgress {
                                            search_id: search_id_for_task.clone(),
                                            session_id: session_id_for_task.clone(),
                                            done: false,
                                            matches,
                                            total_matches,
                                            duration_ms: started_at.elapsed().as_millis() as u64,
                                            searched_layers: searched_layers.clone(),
                                            searched_chunks,
                                            total_chunks,
                                            truncated,
                                            partial_failure,
                                            archive_status: archive_status.clone(),
                                            error: None,
                                        },
                                    );
                                }

                                if truncated {
                                    break;
                                }
                            }
                            Err(error) => {
                                searched_chunks += 1;
                                partial_failure = true;
                                archive_status.degraded = true;
                                archive_status.last_error = Some(error.clone());
                            }
                        }
                    }
                }
                Err(error) => {
                    partial_failure = true;
                    archive_status.degraded = true;
                    archive_status.last_error = Some(error.to_string());
                }
            }
        }

        publish_search_progress(
            &app_handle,
            &job,
            TerminalHistorySearchProgress {
                search_id: search_id_for_task.clone(),
                session_id: session_id_for_task,
                done: true,
                matches: Vec::new(),
                total_matches,
                duration_ms: started_at.elapsed().as_millis() as u64,
                searched_layers,
                searched_chunks,
                total_chunks,
                truncated,
                partial_failure,
                archive_status,
                error: None,
            },
        );
    });

    Ok(StartTerminalHistorySearchResponse { search_id })
}

#[tauri::command]
pub async fn cancel_terminal_history_search(search_id: String) -> Result<(), String> {
    prune_stale_search_jobs();

    if let Some(job) = search_jobs().get(&search_id) {
        job.cancel();
    }
    Ok(())
}

#[tauri::command]
pub async fn get_terminal_history_search_results(
    search_id: String,
    cursor: usize,
) -> Result<TerminalHistorySearchResultsResponse, String> {
    prune_stale_search_jobs();

    let job = search_jobs()
        .get(&search_id)
        .ok_or_else(|| format!("Search {} not found", search_id))?;
    job.snapshot_range(&search_id, cursor)
}

#[tauri::command]
pub async fn get_archived_history_excerpt(
    session_id: String,
    chunk_id: String,
    line_number: u64,
    context_lines: usize,
    registry: State<'_, Arc<SessionRegistry>>,
) -> Result<ArchivedHistoryExcerpt, String> {
    let session_dir = registry
        .with_session(&session_id, |entry| {
            entry
                .terminal_history_archive
                .as_ref()
                .map(|archive| archive.session_dir())
        })
        .flatten()
        .ok_or_else(|| format!("Archived history unavailable for session {}", session_id))?;

    get_archived_excerpt(&session_dir, &chunk_id, line_number, context_lines)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn get_terminal_history_status(
    session_id: String,
    registry: State<'_, Arc<SessionRegistry>>,
) -> Result<ArchiveHealthSnapshot, String> {
    Ok(registry
        .with_session(&session_id, |entry| {
            entry
                .terminal_history_archive
                .as_ref()
                .map(|archive| archive.health_snapshot())
                .unwrap_or_else(unavailable_archive_status)
        })
        .ok_or_else(|| format!("Session {} not found", session_id))?)
}

/// Get scroll buffer contents for a session
#[tauri::command]
pub async fn get_scroll_buffer(
    session_id: String,
    start_line: usize,
    count: usize,
    registry: State<'_, Arc<SessionRegistry>>,
) -> Result<Vec<TerminalLine>, String> {
    let scroll_buffer = registry
        .with_session(&session_id, |entry| entry.scroll_buffer.clone())
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    Ok(scroll_buffer.get_range(start_line, count).await)
}

/// Get scroll buffer statistics
#[tauri::command]
pub async fn get_buffer_stats(
    session_id: String,
    registry: State<'_, Arc<SessionRegistry>>,
) -> Result<BufferStats, String> {
    let scroll_buffer = registry
        .with_session(&session_id, |entry| entry.scroll_buffer.clone())
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    Ok(scroll_buffer.stats().await)
}

/// Clear scroll buffer contents
#[tauri::command]
pub async fn clear_buffer(
    session_id: String,
    registry: State<'_, Arc<SessionRegistry>>,
) -> Result<(), String> {
    let scroll_buffer = registry
        .with_session(&session_id, |entry| entry.scroll_buffer.clone())
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    scroll_buffer.clear().await;
    Ok(())
}

/// Get all lines from scroll buffer (capped at 50,000 to prevent excessive memory use)
#[tauri::command]
pub async fn get_all_buffer_lines(
    session_id: String,
    registry: State<'_, Arc<SessionRegistry>>,
) -> Result<BufferLinesResponse, String> {
    let scroll_buffer = registry
        .with_session(&session_id, |entry| entry.scroll_buffer.clone())
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    // Single-lock cap-aware extraction: only clones up to HARD_LIMIT lines
    // and reads total atomically, avoiding both TOCTOU and full-buffer clone.
    const HARD_LIMIT: usize = 50_000;
    let (lines, total_lines) = scroll_buffer.get_capped(HARD_LIMIT).await;
    let returned_lines = lines.len();
    let truncated = total_lines > returned_lines;
    Ok(BufferLinesResponse {
        lines,
        total_lines,
        returned_lines,
        truncated,
    })
}

/// Scroll to specific line and get context
#[tauri::command]
pub async fn scroll_to_line(
    session_id: String,
    line_number: usize,
    context_lines: usize,
    registry: State<'_, Arc<SessionRegistry>>,
) -> Result<Vec<TerminalLine>, String> {
    let scroll_buffer = registry
        .with_session(&session_id, |entry| entry.scroll_buffer.clone())
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    // Calculate range: line_number ± context_lines
    let start = line_number.saturating_sub(context_lines);
    let count = context_lines * 2 + 1; // Before + target + after

    Ok(scroll_buffer.get_range(start, count).await)
}

fn unavailable_archive_status() -> ArchiveHealthSnapshot {
    ArchiveHealthSnapshot {
        available: false,
        degraded: false,
        closing: false,
        queued_commands: 0,
        max_queue_depth: 0,
        dropped_appends: 0,
        dropped_lines: 0,
        sealed_chunks: 0,
        last_error: None,
    }
}

fn normalize_match_limit(limit: usize) -> usize {
    if limit == 0 { usize::MAX } else { limit }
}

fn remaining_limit(limit: usize, emitted_matches: usize) -> usize {
    if limit == usize::MAX {
        0
    } else {
        limit.saturating_sub(emitted_matches)
    }
}

async fn search_hot_layer(
    scroll_buffer: Arc<crate::session::ScrollBuffer>,
    options: SearchOptions,
    limit: usize,
) -> Result<(Vec<HistorySearchMatch>, usize, bool), String> {
    let snapshot = scroll_buffer.get_all().await;
    let base_line = scroll_buffer
        .total_lines()
        .saturating_sub(snapshot.len() as u64);
    let search_options = SearchOptions {
        max_matches: if limit == usize::MAX { 0 } else { limit },
        ..options
    };

    let result = tokio::task::spawn_blocking(move || search_lines(&snapshot, search_options))
        .await
        .map_err(|_| "Search task failed".to_string())?;

    if let Some(error) = result.error {
        return Err(error);
    }

    let matches = result
        .matches
        .into_iter()
        .map(|search_match| HistorySearchMatch {
            source: HistorySearchSource::Hot,
            line_number: base_line + search_match.line_number as u64,
            buffer_line_number: Some(search_match.line_number),
            column_start: search_match.column_start,
            column_end: search_match.column_end,
            matched_text: search_match.matched_text,
            line_content: search_match.line_content,
            chunk_id: None,
        })
        .collect();

    Ok((matches, result.total_matches, result.truncated))
}

fn search_cold_chunk(
    session_dir: &std::path::Path,
    chunk: &crate::session::history_archive::ArchivedChunkMetadata,
    options: &SearchOptions,
    limit: usize,
) -> Result<(Vec<HistorySearchMatch>, usize, bool), String> {
    let records = read_chunk_records(session_dir, chunk).map_err(|error| error.to_string())?;
    let lines: Vec<TerminalLine> = records
        .iter()
        .map(|record| {
            TerminalLine::with_ansi_timestamp(
                record.text.clone(),
                record.ansi_text.clone(),
                record.timestamp,
            )
        })
        .collect();

    let search_options = SearchOptions {
        max_matches: if limit == usize::MAX { 0 } else { limit },
        ..options.clone()
    };
    let result = search_lines(&lines, search_options);
    if let Some(error) = result.error {
        return Err(error);
    }

    let matches = result
        .matches
        .into_iter()
        .map(|search_match| HistorySearchMatch {
            source: HistorySearchSource::Cold,
            line_number: records[search_match.line_number].line_number,
            buffer_line_number: None,
            column_start: search_match.column_start,
            column_end: search_match.column_end,
            matched_text: search_match.matched_text,
            line_content: search_match.line_content,
            chunk_id: Some(chunk.id.clone()),
        })
        .collect();

    Ok((matches, result.total_matches, result.truncated))
}

fn publish_search_progress(
    app_handle: &AppHandle,
    job: &Arc<SearchJobEntry>,
    payload: TerminalHistorySearchProgress,
) {
    job.update_from_progress(&payload);
    emit_search_progress(app_handle, payload);
}

fn emit_search_progress(app_handle: &AppHandle, payload: TerminalHistorySearchProgress) {
    let _ = app_handle.emit(TERMINAL_HISTORY_SEARCH_PROGRESS_EVENT, payload);
}

fn prune_stale_search_jobs() {
    let now = Instant::now();
    let jobs = search_jobs();

    let stale_ids: Vec<String> = jobs
        .iter()
        .filter_map(|entry| {
            if entry.value().is_stale(now) {
                Some(entry.key().clone())
            } else {
                None
            }
        })
        .collect();

    for search_id in stale_ids {
        jobs.remove(&search_id);
    }

    if jobs.len() <= MAX_COMPLETED_SEARCH_JOBS {
        return;
    }

    let mut completed_jobs: Vec<(String, Instant)> = jobs
        .iter()
        .filter_map(|entry| {
            let updated_at = entry.value().with_state(|state| {
                if state.done || entry.value().is_cancelled() {
                    Some(state.updated_at)
                } else {
                    None
                }
            });

            updated_at.map(|updated_at| (entry.key().clone(), updated_at))
        })
        .collect();

    if completed_jobs.is_empty() {
        return;
    }

    completed_jobs.sort_by_key(|(_, updated_at)| *updated_at);
    let excess = jobs.len().saturating_sub(MAX_COMPLETED_SEARCH_JOBS);
    for (search_id, _) in completed_jobs.into_iter().take(excess) {
        jobs.remove(&search_id);
    }
}

#[cfg(test)]
mod tests {
    // Tests will be added when integrating with registry
}
