// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Tauri commands for scroll buffer management

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, State};

use crate::commands::local::LocalTerminalState;
use crate::session::history_archive::{get_archived_excerpt, load_manifest, read_chunk_records};
use crate::session::{
    ArchiveHealthSnapshot, ArchivedHistoryExcerpt, BufferStats, SearchOptions, SessionRegistry,
    TerminalLine, search_lines,
};

const TERMINAL_HISTORY_SEARCH_PROGRESS_EVENT: &str = "terminal-history-search-progress";
const TERMINAL_SEARCH_MODEL_UPDATED_EVENT: &str = "terminal-search-model-updated";
const SEARCH_JOB_RETENTION: Duration = Duration::from_secs(300);
const MAX_COMPLETED_SEARCH_JOBS: usize = 200;
const DEFAULT_SEARCH_MODEL_MATCH_LIMIT: usize = 100;

fn search_jobs() -> &'static DashMap<String, Arc<SearchJobEntry>> {
    static SEARCH_JOBS: OnceLock<DashMap<String, Arc<SearchJobEntry>>> = OnceLock::new();
    SEARCH_JOBS.get_or_init(DashMap::new)
}

fn search_models() -> &'static DashMap<String, Arc<TerminalSearchModelEntry>> {
    static SEARCH_MODELS: OnceLock<DashMap<String, Arc<TerminalSearchModelEntry>>> =
        OnceLock::new();
    SEARCH_MODELS.get_or_init(DashMap::new)
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

#[derive(Debug, Clone, Serialize)]
pub struct TerminalSearchModelUpdatedEvent {
    pub search_id: String,
    pub session_id: String,
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TerminalSearchModelMatch {
    pub match_index: usize,
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

impl TerminalSearchModelMatch {
    fn from_history(match_index: usize, search_match: HistorySearchMatch) -> Self {
        Self {
            match_index,
            source: search_match.source,
            line_number: search_match.line_number,
            buffer_line_number: search_match.buffer_line_number,
            column_start: search_match.column_start,
            column_end: search_match.column_end,
            matched_text: search_match.matched_text,
            line_content: search_match.line_content,
            chunk_id: search_match.chunk_id,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TerminalSearchModelSnapshot {
    pub search_id: String,
    pub session_id: String,
    pub query: String,
    pub options: SearchOptions,
    pub revision: u64,
    pub created_at: u64,
    pub updated_at: u64,
    pub loading: bool,
    pub done: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub matches: Vec<TerminalSearchModelMatch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_match_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_match: Option<TerminalSearchModelMatch>,
    pub max_matches: usize,
    pub total_matches: usize,
    pub total_buffered_matches: usize,
    pub duration_ms: u64,
    pub searched_layers: Vec<HistorySearchSource>,
    pub searched_chunks: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_chunks: Option<usize>,
    pub truncated: bool,
    pub partial_failure: bool,
    pub archive_status: ArchiveHealthSnapshot,
    pub hot_match_count: usize,
    pub cold_match_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<ArchivedHistoryExcerpt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excerpt_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StartTerminalSearchModelResponse {
    pub search_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TerminalSearchStepDirection {
    Next,
    Previous,
}

struct TerminalSearchModelEntry {
    state: std::sync::Mutex<TerminalSearchModelState>,
}

#[derive(Debug, Clone)]
struct TerminalSearchModelState {
    search_id: String,
    session_id: String,
    options: SearchOptions,
    revision: u64,
    created_at: u64,
    updated_at: u64,
    loading: bool,
    done: bool,
    error: Option<String>,
    matches: Vec<TerminalSearchModelMatch>,
    active_match_index: Option<usize>,
    max_matches: usize,
    total_matches: usize,
    duration_ms: u64,
    searched_layers: Vec<HistorySearchSource>,
    searched_chunks: usize,
    total_chunks: Option<usize>,
    truncated: bool,
    partial_failure: bool,
    archive_status: ArchiveHealthSnapshot,
    excerpt: Option<ArchivedHistoryExcerpt>,
    excerpt_error: Option<String>,
    updated_instant: Instant,
}

impl TerminalSearchModelEntry {
    fn new(search_id: String, session_id: String, options: SearchOptions) -> Self {
        let now = now_millis();
        let max_matches = model_match_limit(options.max_matches);
        Self {
            state: std::sync::Mutex::new(TerminalSearchModelState {
                search_id,
                session_id,
                options,
                revision: 0,
                created_at: now,
                updated_at: now,
                loading: true,
                done: false,
                error: None,
                matches: Vec::new(),
                active_match_index: None,
                max_matches,
                total_matches: 0,
                duration_ms: 0,
                searched_layers: Vec::new(),
                searched_chunks: 0,
                total_chunks: None,
                truncated: false,
                partial_failure: false,
                archive_status: unavailable_archive_status(),
                excerpt: None,
                excerpt_error: None,
                updated_instant: Instant::now(),
            }),
        }
    }

    fn apply_progress(
        &self,
        progress: &TerminalHistorySearchProgress,
    ) -> TerminalSearchModelUpdatedEvent {
        self.with_state(|state| {
            for search_match in &progress.matches {
                if state.matches.len() >= state.max_matches {
                    state.truncated = true;
                    break;
                }
                let match_index = state.matches.len();
                state.matches.push(TerminalSearchModelMatch::from_history(
                    match_index,
                    search_match.clone(),
                ));
            }

            state.loading = !progress.done;
            state.done = progress.done;
            state.error = progress.error.clone();
            state.total_matches = progress.total_matches;
            state.duration_ms = progress.duration_ms;
            state.searched_layers = progress.searched_layers.clone();
            state.searched_chunks = progress.searched_chunks;
            state.total_chunks = progress.total_chunks;
            state.truncated = state.truncated
                || progress.truncated
                || progress.total_matches > state.matches.len();
            state.partial_failure = progress.partial_failure;
            state.archive_status = progress.archive_status.clone();
            state.bump();
            state.update_event()
        })
    }

    fn snapshot(&self) -> TerminalSearchModelSnapshot {
        self.with_state(|state| state.snapshot())
    }

    fn select_match(
        &self,
        match_index: usize,
        excerpt: Option<Result<ArchivedHistoryExcerpt, String>>,
    ) -> Result<TerminalSearchModelSnapshot, String> {
        self.with_state(|state| {
            if match_index >= state.matches.len() {
                return Err(format!(
                    "Match {} is out of range for search {} (matches: {})",
                    match_index,
                    state.search_id,
                    state.matches.len()
                ));
            }

            state.active_match_index = Some(match_index);
            state.excerpt = None;
            state.excerpt_error = None;
            if let Some(result) = excerpt {
                match result {
                    Ok(excerpt) => state.excerpt = Some(excerpt),
                    Err(error) => state.excerpt_error = Some(error),
                }
            }
            state.bump();
            Ok(state.snapshot())
        })
    }

    fn step_match(
        &self,
        direction: TerminalSearchStepDirection,
    ) -> Option<(usize, TerminalSearchModelMatch)> {
        self.with_state(|state| {
            if state.matches.is_empty() {
                state.active_match_index = None;
                state.excerpt = None;
                state.excerpt_error = None;
                state.bump();
                return None;
            }

            let len = state.matches.len();
            let current = state.active_match_index.unwrap_or(match direction {
                TerminalSearchStepDirection::Next => len - 1,
                TerminalSearchStepDirection::Previous => 0,
            });
            let next = match direction {
                TerminalSearchStepDirection::Next => (current + 1) % len,
                TerminalSearchStepDirection::Previous => (current + len - 1) % len,
            };
            Some((next, state.matches[next].clone()))
        })
    }

    fn is_stale(&self, now: Instant) -> bool {
        self.with_state(|state| {
            state.done && now.duration_since(state.updated_instant) > SEARCH_JOB_RETENTION
        })
    }

    fn session_id(&self) -> String {
        self.with_state(|state| state.session_id.clone())
    }

    fn with_state<T>(&self, f: impl FnOnce(&mut TerminalSearchModelState) -> T) -> T {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        f(&mut state)
    }
}

impl TerminalSearchModelState {
    fn bump(&mut self) {
        self.revision = self.revision.saturating_add(1);
        self.updated_at = now_millis();
        self.updated_instant = Instant::now();
    }

    fn update_event(&self) -> TerminalSearchModelUpdatedEvent {
        TerminalSearchModelUpdatedEvent {
            search_id: self.search_id.clone(),
            session_id: self.session_id.clone(),
            revision: self.revision,
        }
    }

    fn snapshot(&self) -> TerminalSearchModelSnapshot {
        let active_match = self
            .active_match_index
            .and_then(|index| self.matches.get(index).cloned());
        let hot_match_count = self
            .matches
            .iter()
            .filter(|search_match| search_match.source == HistorySearchSource::Hot)
            .count();
        let cold_match_count = self.matches.len().saturating_sub(hot_match_count);

        TerminalSearchModelSnapshot {
            search_id: self.search_id.clone(),
            session_id: self.session_id.clone(),
            query: self.options.query.clone(),
            options: self.options.clone(),
            revision: self.revision,
            created_at: self.created_at,
            updated_at: self.updated_at,
            loading: self.loading,
            done: self.done,
            error: self.error.clone(),
            matches: self.matches.clone(),
            active_match_index: self.active_match_index,
            active_match,
            max_matches: self.max_matches,
            total_matches: self.total_matches,
            total_buffered_matches: self.matches.len(),
            duration_ms: self.duration_ms,
            searched_layers: self.searched_layers.clone(),
            searched_chunks: self.searched_chunks,
            total_chunks: self.total_chunks,
            truncated: self.truncated,
            partial_failure: self.partial_failure,
            archive_status: self.archive_status.clone(),
            hot_match_count,
            cold_match_count,
            excerpt: self.excerpt.clone(),
            excerpt_error: self.excerpt_error.clone(),
        }
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
pub async fn start_terminal_search_model(
    app_handle: AppHandle,
    session_id: String,
    mut options: SearchOptions,
    registry: State<'_, Arc<SessionRegistry>>,
) -> Result<StartTerminalSearchModelResponse, String> {
    prune_stale_search_jobs();
    prune_stale_search_models();

    // The UI-level search model must never become an unbounded memory bucket.
    // Low-level callers may still opt into unlimited polling, but model snapshots
    // keep a bounded match window and expose the full count through total_matches.
    options.max_matches = model_match_limit(options.max_matches);

    let response = start_terminal_history_search(
        app_handle.clone(),
        session_id.clone(),
        options.clone(),
        registry,
    )
    .await?;

    let model = Arc::new(TerminalSearchModelEntry::new(
        response.search_id.clone(),
        session_id.clone(),
        options,
    ));
    let event = model.with_state(|state| {
        state.bump();
        state.update_event()
    });
    search_models().insert(response.search_id.clone(), model);
    emit_search_model_update(&app_handle, event);

    Ok(StartTerminalSearchModelResponse {
        search_id: response.search_id,
    })
}

#[tauri::command]
pub async fn get_terminal_search_model_snapshot(
    search_id: String,
) -> Result<TerminalSearchModelSnapshot, String> {
    prune_stale_search_models();

    let model = search_models()
        .get(&search_id)
        .map(|entry| entry.value().clone())
        .ok_or_else(|| format!("Terminal search model {} not found", search_id))?;
    Ok(model.snapshot())
}

#[tauri::command]
pub async fn select_terminal_search_match(
    search_id: String,
    match_index: usize,
    registry: State<'_, Arc<SessionRegistry>>,
) -> Result<TerminalSearchModelSnapshot, String> {
    prune_stale_search_models();

    let model = search_models()
        .get(&search_id)
        .map(|entry| entry.value().clone())
        .ok_or_else(|| format!("Terminal search model {} not found", search_id))?;
    let selected_match = model
        .with_state(|state| state.matches.get(match_index).cloned())
        .ok_or_else(|| format!("Match {} not found for search {}", match_index, search_id))?;
    let excerpt = load_excerpt_for_model_match(&model, &selected_match, registry).await;
    model.select_match(match_index, excerpt)
}

#[tauri::command]
pub async fn step_terminal_search_match(
    search_id: String,
    direction: TerminalSearchStepDirection,
    registry: State<'_, Arc<SessionRegistry>>,
) -> Result<TerminalSearchModelSnapshot, String> {
    prune_stale_search_models();

    let model = search_models()
        .get(&search_id)
        .map(|entry| entry.value().clone())
        .ok_or_else(|| format!("Terminal search model {} not found", search_id))?;
    let Some((match_index, selected_match)) = model.step_match(direction) else {
        return Ok(model.snapshot());
    };
    let excerpt = load_excerpt_for_model_match(&model, &selected_match, registry).await;
    model.select_match(match_index, excerpt)
}

#[tauri::command]
pub async fn close_terminal_search_model(search_id: String) -> Result<(), String> {
    if let Some((_, model)) = search_models().remove(&search_id) {
        if let Some(job) = search_jobs().get(&search_id) {
            job.cancel();
        }
        model.with_state(|state| {
            state.loading = false;
            state.done = true;
            state.bump();
        });
    }
    Ok(())
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
    local_state: State<'_, Arc<LocalTerminalState>>,
) -> Result<Vec<TerminalLine>, String> {
    if let Some(scroll_buffer) =
        registry.with_session(&session_id, |entry| entry.scroll_buffer.clone())
    {
        return Ok(scroll_buffer.get_range(start_line, count).await);
    }

    local_state
        .registry
        .get_scroll_buffer(&session_id, start_line, count)
        .await
        .map_err(|_| format!("Session {} not found", session_id))
}

/// Get scroll buffer statistics
#[tauri::command]
pub async fn get_buffer_stats(
    session_id: String,
    registry: State<'_, Arc<SessionRegistry>>,
    local_state: State<'_, Arc<LocalTerminalState>>,
) -> Result<BufferStats, String> {
    if let Some(scroll_buffer) =
        registry.with_session(&session_id, |entry| entry.scroll_buffer.clone())
    {
        return Ok(scroll_buffer.stats().await);
    }

    local_state
        .registry
        .get_buffer_stats(&session_id)
        .await
        .map_err(|_| format!("Session {} not found", session_id))
}

/// Clear scroll buffer contents
#[tauri::command]
pub async fn clear_buffer(
    session_id: String,
    registry: State<'_, Arc<SessionRegistry>>,
) -> Result<(), String> {
    let (scroll_buffer, command_facts) = registry
        .with_session(&session_id, |entry| {
            (entry.scroll_buffer.clone(), entry.command_facts.clone())
        })
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    scroll_buffer.clear().await;
    command_facts
        .mark_open_facts_stale(
            "clear_buffer",
            crate::session::CommandFactClosedBy::TerminalReset,
        )
        .await;
    Ok(())
}

/// Get all lines from scroll buffer (capped at 50,000 to prevent excessive memory use)
#[tauri::command]
pub async fn get_all_buffer_lines(
    session_id: String,
    registry: State<'_, Arc<SessionRegistry>>,
    local_state: State<'_, Arc<LocalTerminalState>>,
) -> Result<BufferLinesResponse, String> {
    // Single-lock cap-aware extraction: only clones up to HARD_LIMIT lines
    // and reads total atomically, avoiding both TOCTOU and full-buffer clone.
    const HARD_LIMIT: usize = 50_000;
    let (lines, total_lines) = if let Some(scroll_buffer) =
        registry.with_session(&session_id, |entry| entry.scroll_buffer.clone())
    {
        scroll_buffer.get_capped(HARD_LIMIT).await
    } else {
        local_state
            .registry
            .get_capped_buffer(&session_id, HARD_LIMIT)
            .await
            .map_err(|_| format!("Session {} not found", session_id))?
    };
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

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn model_match_limit(limit: usize) -> usize {
    if limit == 0 {
        DEFAULT_SEARCH_MODEL_MATCH_LIMIT
    } else {
        limit
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
    if let Some(model) = search_models().get(&payload.search_id) {
        let event = model.apply_progress(&payload);
        emit_search_model_update(app_handle, event);
    }
    emit_search_progress(app_handle, payload);
}

fn emit_search_progress(app_handle: &AppHandle, payload: TerminalHistorySearchProgress) {
    let _ = app_handle.emit(TERMINAL_HISTORY_SEARCH_PROGRESS_EVENT, payload);
}

fn emit_search_model_update(app_handle: &AppHandle, payload: TerminalSearchModelUpdatedEvent) {
    let _ = app_handle.emit(TERMINAL_SEARCH_MODEL_UPDATED_EVENT, payload);
}

async fn load_excerpt_for_model_match(
    model: &Arc<TerminalSearchModelEntry>,
    selected_match: &TerminalSearchModelMatch,
    registry: State<'_, Arc<SessionRegistry>>,
) -> Option<Result<ArchivedHistoryExcerpt, String>> {
    if selected_match.source != HistorySearchSource::Cold {
        return None;
    }
    let Some(chunk_id) = selected_match.chunk_id.clone() else {
        return Some(Err("Archived match is missing chunk id".to_string()));
    };
    let session_id = model.session_id();
    let session_dir = match registry.with_session(&session_id, |entry| {
        entry
            .terminal_history_archive
            .as_ref()
            .map(|archive| archive.session_dir())
    }) {
        Some(Some(session_dir)) => session_dir,
        _ => {
            return Some(Err(format!(
                "Archived history unavailable for session {}",
                session_id
            )));
        }
    };

    Some(
        get_archived_excerpt(&session_dir, &chunk_id, selected_match.line_number, 6)
            .map_err(|error| error.to_string()),
    )
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

fn prune_stale_search_models() {
    let now = Instant::now();
    let stale_ids: Vec<String> = search_models()
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
        search_models().remove(&search_id);
    }
}

#[cfg(test)]
mod tests {
    // Tests will be added when integrating with registry
}
