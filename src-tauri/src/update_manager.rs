// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Resumable update manager
//!
//! Supports断点续传 download, signature verification, and cross-restart recovery.
//! Adapted from lumina-note's update_manager.

use crate::config::{is_portable_mode, portable_aware_app_data_dir};
use base64::Engine as _;
use futures_util::StreamExt;
use minisign_verify::{PublicKey, Signature};
use reqwest::StatusCode;
use reqwest::header::{
    ACCEPT, ACCEPT_ENCODING, CONTENT_LENGTH, CONTENT_RANGE, ETAG, HeaderValue, IF_RANGE,
    IF_UNMODIFIED_SINCE, LAST_MODIFIED, RANGE,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_updater::{Update, UpdaterExt};
use thiserror::Error;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use uuid::Uuid;

// ── Error type ──────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum UpdateError {
    #[error("Update error: {0}")]
    General(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Integrity error: {0}")]
    Integrity(String),

    #[error("Install error: {0}")]
    Install(String),

    #[error("State error: {0}")]
    State(String),
}

impl Serialize for UpdateError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

fn error_code_for(error: &UpdateError) -> &'static str {
    match error {
        UpdateError::Network(_) => "network",
        UpdateError::Integrity(_) => "integrity",
        UpdateError::Install(_) => "install",
        UpdateError::State(_) => "state",
        UpdateError::General(_) => "update",
    }
}

// ── Constants ───────────────────────────────────────────────

const RESUMABLE_EVENT_NAME: &str = "update:resumable-event";
const STATE_FILE_NAME: &str = "state.json";
const PART_FILE_NAME: &str = "package.part";
const MAX_DOWNLOAD_ATTEMPTS: u32 = 3;
const BASE_RETRY_DELAY_MS: u64 = 1_500;
const MAX_RETRY_DELAY_MS: u64 = 12_000;
const DOWNLOAD_TIMEOUT_MS: u64 = 120_000;
const SAVE_STATE_INTERVAL_BYTES: u64 = 256 * 1024;
const LOCK_PROFILE_ENV: &str = "OXIDETERM_PROFILE_LOCKS";
const MAX_RETAINED_RESUMABLE_UPDATE_DIRS: usize = 2;

// ── Data types ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum UpdateStage {
    Downloading,
    Verifying,
    Installing,
    Ready,
    Error,
    Cancelled,
}

impl UpdateStage {
    fn is_terminal(self) -> bool {
        matches!(self, Self::Ready | Self::Error | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
enum ResumableEventType {
    Started,
    Resumed,
    Progress,
    Retrying,
    Verifying,
    Installing,
    Ready,
    Error,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumableUpdateStatus {
    pub task_id: String,
    pub version: String,
    pub attempt: u32,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub resumable: bool,
    pub stage: UpdateStage,
    pub status: UpdateStage,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub timestamp: i64,
    pub retry_delay_ms: Option<u64>,
    pub last_http_status: Option<u16>,
    pub can_resume_after_restart: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedUpdateState {
    status: ResumableUpdateStatus,
    download_url: String,
    signature: String,
    etag: Option<String>,
    last_modified: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResumableEventPayload {
    #[serde(rename = "type")]
    event_type: ResumableEventType,
    #[serde(flatten)]
    status: ResumableUpdateStatus,
}

// ── State ───────────────────────────────────────────────────

#[derive(Default)]
struct UpdateManagerRuntime {
    active_task_id: Option<String>,
    statuses: HashMap<String, ResumableUpdateStatus>,
    cancelled_tasks: HashSet<String>,
}

pub struct UpdateManagerState {
    inner: Arc<Mutex<UpdateManagerRuntime>>,
}

impl Default for UpdateManagerState {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(UpdateManagerRuntime::default())),
        }
    }
}

impl UpdateManagerState {
    async fn lock_runtime(
        &self,
        context: &'static str,
    ) -> tokio::sync::MutexGuard<'_, UpdateManagerRuntime> {
        lock_update_runtime(&self.inner, context).await
    }
}

fn update_lock_profiling_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| cfg!(debug_assertions) && std::env::var_os(LOCK_PROFILE_ENV).is_some())
}

async fn lock_update_runtime<'a>(
    runtime: &'a Arc<Mutex<UpdateManagerRuntime>>,
    context: &'static str,
) -> tokio::sync::MutexGuard<'a, UpdateManagerRuntime> {
    let started = Instant::now();
    let guard = runtime.lock().await;
    if update_lock_profiling_enabled() {
        tracing::debug!(
            "[UpdateManager] lock wait context={} waited_us={}",
            context,
            started.elapsed().as_micros()
        );
    }
    guard
}

// ── Update channel ──────────────────────────────────────────

const BETA_ENDPOINT: &str =
    "https://github.com/AnalyseDeCircuit/oxideterm/releases/download/updater-beta/latest.json";
const GPUI_PREVIEW_ENDPOINT: &str = "https://github.com/AnalyseDeCircuit/oxideterm/releases/download/updater-gpui-preview/latest.json";

/// Build an `Updater` for the given channel.
/// `"stable"` uses the config-default endpoint; `"beta"` and `"gpui-preview"`
/// override to channel-specific release assets. Keep GPUI preview separate so
/// Tauri beta users never receive native preview packages.
fn build_updater_for_channel(
    app: &AppHandle,
    channel: Option<&str>,
) -> Result<tauri_plugin_updater::Updater, UpdateError> {
    if is_portable_mode().map_err(|e| UpdateError::General(e.to_string()))? {
        return Err(UpdateError::General(
            "Updater is disabled in portable mode".to_string(),
        ));
    }

    let mut builder = app.updater_builder();
    let endpoint = match channel {
        Some("beta") => Some(BETA_ENDPOINT),
        Some("gpui-preview") => Some(GPUI_PREVIEW_ENDPOINT),
        Some("stable") | None => None,
        Some(other) => {
            return Err(UpdateError::General(format!(
                "unsupported update channel: {other}"
            )));
        }
    };
    if let Some(endpoint) = endpoint {
        builder = builder
            .endpoints(vec![
                endpoint.parse().expect("update endpoint is a valid URL"),
            ])
            .map_err(|e| UpdateError::General(format!("set update endpoint failed: {e}")))?;
    }
    builder
        .build()
        .map_err(|e| UpdateError::General(format!("build updater failed: {e}")))
}

// ── Commands ────────────────────────────────────────────────

/// Result of a channel-aware update check, serialized to JS.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResult {
    pub version: String,
    pub current_version: String,
    pub body: Option<String>,
    pub date: Option<String>,
}

#[tauri::command]
pub async fn update_check_with_channel(
    app: AppHandle,
    channel: Option<String>,
) -> Result<Option<UpdateCheckResult>, UpdateError> {
    let updater = build_updater_for_channel(&app, channel.as_deref())?;
    let update = updater
        .check()
        .await
        .map_err(|err| UpdateError::General(format!("check update failed: {err}")))?;

    Ok(update.map(|u| UpdateCheckResult {
        version: u.version.clone(),
        current_version: u.current_version.to_string(),
        body: u.body.clone(),
        date: u.date.as_ref().map(|d| d.to_string()),
    }))
}

#[tauri::command]
pub async fn update_start_resumable_install(
    app: AppHandle,
    manager_state: State<'_, UpdateManagerState>,
    expected_version: Option<String>,
    channel: Option<String>,
) -> Result<String, UpdateError> {
    let updater = build_updater_for_channel(&app, channel.as_deref())?;
    let update = updater
        .check()
        .await
        .map_err(|err| UpdateError::General(format!("check update failed: {err}")))?
        .ok_or_else(|| UpdateError::General("no update available".to_string()))?;

    if let Some(ref expected) = expected_version {
        if update.version != *expected {
            return Err(UpdateError::General(format!(
                "expected version {expected}, but latest is {}",
                update.version
            )));
        }
    }

    // Return existing task if already running for this version
    {
        let runtime = manager_state
            .lock_runtime("update_start_resumable_install:read_active")
            .await;
        if let Some(active_task_id) = runtime.active_task_id.as_ref() {
            if let Some(active_status) = runtime.statuses.get(active_task_id) {
                if active_status.version == update.version && !active_status.stage.is_terminal() {
                    return Ok(active_task_id.clone());
                }
            }
        }
    }

    let version_dir = version_dir(&app, &update.version)?;
    tokio::fs::create_dir_all(&version_dir)
        .await
        .map_err(|err| UpdateError::State(format!("create update dir failed: {err}")))?;

    let mut persisted_state = load_state_file(&version_dir)
        .await?
        .unwrap_or_else(|| new_persisted_state(&update, None));

    // Reset if terminal state, URL changed, or signature changed
    if persisted_state.status.stage.is_terminal()
        || persisted_state.download_url != update.download_url.as_str()
        || persisted_state.signature != update.signature
    {
        clear_version_cache(&version_dir).await?;
        persisted_state = new_persisted_state(&update, None);
    }

    if persisted_state.status.task_id.is_empty() {
        persisted_state.status.task_id = Uuid::new_v4().to_string();
    }
    if persisted_state.status.timestamp <= 0 {
        persisted_state.status.timestamp = now_millis();
    }

    let task_id = persisted_state.status.task_id.clone();
    let is_resumed = persisted_state.status.downloaded_bytes > 0;

    {
        let mut runtime = manager_state
            .lock_runtime("update_start_resumable_install:store_active")
            .await;
        runtime.active_task_id = Some(task_id.clone());
        runtime.cancelled_tasks.remove(&task_id);
        runtime
            .statuses
            .insert(task_id.clone(), persisted_state.status.clone());
    }

    save_state_file(&version_dir, &persisted_state).await?;

    let app_handle = app.clone();
    let runtime_state = manager_state.inner.clone();
    let task_id_for_task = task_id.clone();
    tauri::async_runtime::spawn(async move {
        let result = run_update_task(
            app_handle.clone(),
            runtime_state.clone(),
            version_dir,
            persisted_state,
            update,
            is_resumed,
        )
        .await;

        if let Err(err) = result {
            let _ = emit_terminal_error(app_handle, runtime_state, &task_id_for_task, err).await;
        }
    });

    Ok(task_id)
}

#[tauri::command]
pub async fn update_get_resumable_status(
    app: AppHandle,
    manager_state: State<'_, UpdateManagerState>,
    task_id: Option<String>,
) -> Result<Option<ResumableUpdateStatus>, UpdateError> {
    {
        let runtime = manager_state
            .lock_runtime("update_get_resumable_status")
            .await;
        if let Some(task_id) = task_id.as_ref() {
            if let Some(status) = runtime.statuses.get(task_id) {
                return Ok(Some(status.clone()));
            }
        } else if let Some(active_task_id) = runtime.active_task_id.as_ref() {
            if let Some(status) = runtime.statuses.get(active_task_id) {
                return Ok(Some(status.clone()));
            }
        }

        if task_id.is_none() {
            if let Some(status) = runtime.statuses.values().max_by_key(|s| s.timestamp) {
                return Ok(Some(status.clone()));
            }
        }
    }

    if let Some(task_id) = task_id {
        return load_status_by_task_id(&app, &task_id).await;
    }

    load_latest_persisted_status(&app).await
}

#[tauri::command]
pub async fn update_cancel_resumable_install(
    app: AppHandle,
    manager_state: State<'_, UpdateManagerState>,
    task_id: Option<String>,
) -> Result<(), UpdateError> {
    let mut status_to_emit: Option<ResumableUpdateStatus> = None;
    let target_task_id = {
        let mut runtime = manager_state
            .lock_runtime("update_cancel_resumable_install")
            .await;
        let target_task_id = task_id
            .or_else(|| runtime.active_task_id.clone())
            .ok_or_else(|| UpdateError::General("no active update task".to_string()))?;
        let current_stage = runtime.statuses.get(&target_task_id).map(|s| s.stage);
        if let Some(stage) = current_stage {
            if !can_cancel_from_stage(stage) {
                return Err(cancel_not_allowed_error(stage));
            }
            runtime.cancelled_tasks.insert(target_task_id.clone());
            let status = runtime
                .statuses
                .get_mut(&target_task_id)
                .ok_or_else(|| UpdateError::General("update status disappeared".to_string()))?;
            status.stage = UpdateStage::Cancelled;
            status.status = UpdateStage::Cancelled;
            status.error_code = Some("cancelled".to_string());
            status.error_message = Some("Update cancelled by user".to_string());
            status.timestamp = now_millis();
            status_to_emit = Some(status.clone());
        } else {
            runtime.cancelled_tasks.insert(target_task_id.clone());
        }
        if runtime.active_task_id.as_deref() == Some(target_task_id.as_str()) {
            runtime.active_task_id = None;
        }
        target_task_id
    };

    if let Some(status) = status_to_emit {
        let _ = app.emit(
            RESUMABLE_EVENT_NAME,
            ResumableEventPayload {
                event_type: ResumableEventType::Cancelled,
                status: status.clone(),
            },
        );
        let version_dir = version_dir(&app, &status.version)?;
        if let Some(mut persisted) = load_state_file(&version_dir).await? {
            persisted.status = status;
            save_state_file(&version_dir, &persisted).await?;
        }
    } else {
        let _ = target_task_id;
    }

    Ok(())
}

#[tauri::command]
pub async fn update_clear_resumable_cache(
    app: AppHandle,
    manager_state: State<'_, UpdateManagerState>,
    version: Option<String>,
) -> Result<(), UpdateError> {
    let root = updates_root(&app)?;
    if let Some(version) = version {
        let dir = root.join(sanitize_path_segment(&version));
        if dir.exists() {
            tokio::fs::remove_dir_all(&dir)
                .await
                .map_err(|err| UpdateError::State(format!("remove cache dir failed: {err}")))?;
        }
        let mut runtime = manager_state
            .lock_runtime("update_clear_resumable_cache:version")
            .await;
        runtime.statuses.retain(|_, s| s.version != version);
        if let Some(active) = runtime.active_task_id.clone() {
            if runtime
                .statuses
                .get(&active)
                .map(|s| s.version == version)
                .unwrap_or(false)
            {
                runtime.active_task_id = None;
            }
        }
        return Ok(());
    }

    if root.exists() {
        tokio::fs::remove_dir_all(&root)
            .await
            .map_err(|err| UpdateError::State(format!("clear cache failed: {err}")))?;
    }
    tokio::fs::create_dir_all(&root)
        .await
        .map_err(|err| UpdateError::State(format!("recreate cache root failed: {err}")))?;

    let mut runtime = manager_state
        .lock_runtime("update_clear_resumable_cache:all")
        .await;
    runtime.active_task_id = None;
    runtime.statuses.clear();
    runtime.cancelled_tasks.clear();

    Ok(())
}

// ── Core task ───────────────────────────────────────────────

async fn run_update_task(
    app: AppHandle,
    runtime_state: Arc<Mutex<UpdateManagerRuntime>>,
    version_dir: PathBuf,
    mut persisted: PersistedUpdateState,
    update: Update,
    is_resumed: bool,
) -> Result<(), UpdateError> {
    persisted.status.stage = UpdateStage::Downloading;
    persisted.status.status = UpdateStage::Downloading;
    persisted.status.error_code = None;
    persisted.status.error_message = None;
    persisted.status.timestamp = now_millis();
    save_and_emit_status(
        &app,
        runtime_state.clone(),
        &version_dir,
        &persisted,
        if is_resumed {
            ResumableEventType::Resumed
        } else {
            ResumableEventType::Started
        },
    )
    .await?;

    let package_bytes =
        download_with_retries(&app, runtime_state.clone(), &version_dir, &mut persisted).await?;

    ensure_task_not_cancelled(runtime_state.clone(), &persisted.status.task_id).await?;

    // Verify signature
    persisted.status.stage = UpdateStage::Verifying;
    persisted.status.status = UpdateStage::Verifying;
    persisted.status.timestamp = now_millis();
    save_and_emit_status(
        &app,
        runtime_state.clone(),
        &version_dir,
        &persisted,
        ResumableEventType::Verifying,
    )
    .await?;

    if let Some(total_bytes) = persisted.status.total_bytes {
        if package_bytes.len() as u64 != total_bytes {
            return Err(UpdateError::Integrity(format!(
                "size mismatch: got {}, expected {total_bytes}",
                package_bytes.len()
            )));
        }
    }

    let pub_key = updater_pubkey()?;
    verify_signature(&package_bytes, &persisted.signature, &pub_key)?;

    ensure_task_not_cancelled(runtime_state.clone(), &persisted.status.task_id).await?;

    // Install
    persisted.status.stage = UpdateStage::Installing;
    persisted.status.status = UpdateStage::Installing;
    persisted.status.timestamp = now_millis();
    save_and_emit_status(
        &app,
        runtime_state.clone(),
        &version_dir,
        &persisted,
        ResumableEventType::Installing,
    )
    .await?;

    ensure_task_not_cancelled(runtime_state.clone(), &persisted.status.task_id).await?;

    update
        .install(&package_bytes)
        .map_err(|err| UpdateError::Install(format!("install failed: {err}")))?;

    ensure_task_not_cancelled(runtime_state.clone(), &persisted.status.task_id).await?;

    // Ready
    persisted.status.stage = UpdateStage::Ready;
    persisted.status.status = UpdateStage::Ready;
    persisted.status.timestamp = now_millis();
    save_and_emit_status(
        &app,
        runtime_state.clone(),
        &version_dir,
        &persisted,
        ResumableEventType::Ready,
    )
    .await?;

    if let Err(err) = remove_part_file(&version_dir).await {
        tracing::warn!("Failed to remove completed update package cache: {err}");
    }
    if let Err(err) = prune_update_cache(&app, Some(&persisted.status.version)).await {
        tracing::warn!("Failed to prune stale update cache: {err}");
    }

    clear_active_task_if_needed(runtime_state, &persisted.status.task_id).await;
    Ok(())
}

// ── Download with retries + resume ──────────────────────────

async fn download_with_retries(
    app: &AppHandle,
    runtime_state: Arc<Mutex<UpdateManagerRuntime>>,
    version_dir: &Path,
    persisted: &mut PersistedUpdateState,
) -> Result<Vec<u8>, UpdateError> {
    let part_path = version_dir.join(PART_FILE_NAME);
    let mut next_attempt = persisted.status.attempt.max(1);

    while next_attempt <= MAX_DOWNLOAD_ATTEMPTS {
        if is_task_cancelled(runtime_state.clone(), &persisted.status.task_id).await {
            return Err(UpdateError::General("update cancelled".to_string()));
        }

        persisted.status.attempt = next_attempt;
        persisted.status.retry_delay_ms = None;
        persisted.status.stage = UpdateStage::Downloading;
        persisted.status.status = UpdateStage::Downloading;
        persisted.status.timestamp = now_millis();
        save_state_file(version_dir, persisted).await?;
        store_runtime_status(runtime_state.clone(), persisted.status.clone()).await;

        if next_attempt > 1 {
            let retry_delay_ms = compute_retry_delay(next_attempt - 1);
            persisted.status.retry_delay_ms = Some(retry_delay_ms);
            emit_status_event(app, ResumableEventType::Retrying, persisted.status.clone());
            tokio::time::sleep(Duration::from_millis(retry_delay_ms)).await;
        }

        let result = download_once(
            app,
            runtime_state.clone(),
            persisted,
            &part_path,
            version_dir,
        )
        .await;
        match result {
            Ok(()) => {
                let bytes = tokio::fs::read(&part_path).await.map_err(|err| {
                    UpdateError::State(format!("read downloaded package failed: {err}"))
                })?;
                return Ok(bytes);
            }
            Err(err) => {
                if is_task_cancelled(runtime_state.clone(), &persisted.status.task_id).await {
                    return Err(UpdateError::General("update cancelled".to_string()));
                }
                if next_attempt >= MAX_DOWNLOAD_ATTEMPTS {
                    return Err(err);
                }
                let delay_ms = compute_retry_delay(next_attempt);
                persisted.status.retry_delay_ms = Some(delay_ms);
                persisted.status.timestamp = now_millis();
                save_state_file(version_dir, persisted).await?;
                emit_status_event(app, ResumableEventType::Retrying, persisted.status.clone());
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                next_attempt += 1;
            }
        }
    }

    Err(UpdateError::Network("download retry exhausted".to_string()))
}

async fn download_once(
    app: &AppHandle,
    runtime_state: Arc<Mutex<UpdateManagerRuntime>>,
    persisted: &mut PersistedUpdateState,
    part_path: &Path,
    version_dir: &Path,
) -> Result<(), UpdateError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(DOWNLOAD_TIMEOUT_MS))
        .build()
        .map_err(|err| UpdateError::Network(format!("build http client failed: {err}")))?;

    let existing_len = tokio::fs::metadata(part_path)
        .await
        .map(|meta| meta.len())
        .unwrap_or(0);
    if existing_len != persisted.status.downloaded_bytes {
        persisted.status.downloaded_bytes = existing_len;
    }

    let mut range_requested = existing_len > 0;
    let mut request = client
        .get(&persisted.download_url)
        .header(ACCEPT, HeaderValue::from_static("application/octet-stream"))
        .header(ACCEPT_ENCODING, HeaderValue::from_static("identity"));

    if range_requested {
        request = request.header(RANGE, format!("bytes={existing_len}-"));
        if let Some(etag) = persisted.etag.as_ref() {
            request = request.header(IF_RANGE, etag);
        } else if let Some(last_modified) = persisted.last_modified.as_ref() {
            request = request.header(IF_UNMODIFIED_SINCE, last_modified);
        }
    }

    let response = request
        .send()
        .await
        .map_err(|err| UpdateError::Network(format!("download request failed: {err}")))?;
    let status = response.status();
    persisted.status.last_http_status = Some(status.as_u16());

    if should_restart_full_download(range_requested, status) {
        if status == StatusCode::OK {
            range_requested = false;
            persisted.status.resumable = false;
            persisted.status.downloaded_bytes = 0;
            tokio::fs::write(part_path, &[] as &[u8])
                .await
                .map_err(|err| {
                    UpdateError::State(format!("reset partial package failed: {err}"))
                })?;
        } else {
            persisted.status.downloaded_bytes = 0;
            persisted.status.resumable = false;
            tokio::fs::write(part_path, &[] as &[u8])
                .await
                .map_err(|err| {
                    UpdateError::State(format!("truncate partial package failed: {err}"))
                })?;
            save_state_file(version_dir, persisted).await?;
            return Err(UpdateError::Network(format!(
                "resume rejected by server: http {}",
                status.as_u16()
            )));
        }
    }

    if !status.is_success() {
        return Err(UpdateError::Network(format!(
            "download request failed with status {}",
            status.as_u16()
        )));
    }

    let response_headers = response.headers().clone();
    let current_etag = response_headers
        .get(ETAG)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_string());
    let current_last_modified = response_headers
        .get(LAST_MODIFIED)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_string());

    // ETag consistency check for range requests
    if range_requested {
        if let (Some(old), Some(new_val)) = (persisted.etag.as_ref(), current_etag.as_ref()) {
            if old != new_val {
                persisted.status.downloaded_bytes = 0;
                persisted.status.resumable = false;
                tokio::fs::write(part_path, &[] as &[u8])
                    .await
                    .map_err(|err| {
                        UpdateError::State(format!("truncate for etag reset failed: {err}"))
                    })?;
                save_state_file(version_dir, persisted).await?;
                return Err(UpdateError::Network(
                    "updater resource changed (etag)".to_string(),
                ));
            }
        }
    }

    if current_etag.is_some() {
        persisted.etag = current_etag;
    }
    if current_last_modified.is_some() {
        persisted.last_modified = current_last_modified;
    }

    let mut total_bytes = persisted.status.total_bytes;
    if status == StatusCode::PARTIAL_CONTENT {
        persisted.status.resumable = true;
        total_bytes = response_headers
            .get(CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(parse_content_range_total)
            .or(total_bytes);
    } else {
        let content_length = response_headers
            .get(CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());
        total_bytes = content_length;
        if range_requested {
            persisted.status.resumable = false;
        }
    }
    persisted.status.total_bytes = total_bytes;

    let mut downloaded = if status == StatusCode::PARTIAL_CONTENT {
        existing_len
    } else {
        0
    };

    let mut file = if status == StatusCode::PARTIAL_CONTENT {
        tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(part_path)
            .await
            .map_err(|err| UpdateError::State(format!("open part file failed: {err}")))?
    } else {
        tokio::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(part_path)
            .await
            .map_err(|err| UpdateError::State(format!("open package file failed: {err}")))?
    };

    let mut bytes_since_last_save = 0u64;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        if is_task_cancelled(runtime_state.clone(), &persisted.status.task_id).await {
            return Err(UpdateError::General("update cancelled".to_string()));
        }

        let chunk = chunk
            .map_err(|err| UpdateError::Network(format!("error decoding response body: {err}")))?;
        file.write_all(&chunk)
            .await
            .map_err(|err| UpdateError::State(format!("write update chunk failed: {err}")))?;
        downloaded = downloaded.saturating_add(chunk.len() as u64);
        bytes_since_last_save = bytes_since_last_save.saturating_add(chunk.len() as u64);
        persisted.status.downloaded_bytes = downloaded;
        persisted.status.timestamp = now_millis();
        persisted.status.error_code = None;
        persisted.status.error_message = None;

        store_runtime_status(runtime_state.clone(), persisted.status.clone()).await;
        emit_status_event(app, ResumableEventType::Progress, persisted.status.clone());

        if bytes_since_last_save >= SAVE_STATE_INTERVAL_BYTES {
            save_state_file(version_dir, persisted).await?;
            bytes_since_last_save = 0;
        }
    }

    file.flush()
        .await
        .map_err(|err| UpdateError::State(format!("flush update package failed: {err}")))?;
    save_state_file(version_dir, persisted).await?;

    if let Some(total) = persisted.status.total_bytes {
        if persisted.status.downloaded_bytes < total {
            return Err(UpdateError::Network(format!(
                "download incomplete: got {}, expected {total}",
                persisted.status.downloaded_bytes
            )));
        }
    }

    Ok(())
}

// ── Helpers ─────────────────────────────────────────────────

async fn save_and_emit_status(
    app: &AppHandle,
    runtime_state: Arc<Mutex<UpdateManagerRuntime>>,
    version_dir: &Path,
    persisted: &PersistedUpdateState,
    event_type: ResumableEventType,
) -> Result<(), UpdateError> {
    if persisted.status.stage != UpdateStage::Cancelled {
        ensure_task_not_cancelled(runtime_state.clone(), &persisted.status.task_id).await?;
    }
    save_state_file(version_dir, persisted).await?;
    store_runtime_status(runtime_state, persisted.status.clone()).await;
    emit_status_event(app, event_type, persisted.status.clone());
    Ok(())
}

async fn emit_terminal_error(
    app: AppHandle,
    runtime_state: Arc<Mutex<UpdateManagerRuntime>>,
    task_id: &str,
    error: UpdateError,
) -> Result<(), UpdateError> {
    let mut status: Option<ResumableUpdateStatus> = None;
    {
        let mut runtime = lock_update_runtime(&runtime_state, "emit_terminal_error").await;
        if let Some(existing) = runtime.statuses.get_mut(task_id) {
            existing.stage = if existing.error_code.as_deref() == Some("cancelled") {
                UpdateStage::Cancelled
            } else {
                UpdateStage::Error
            };
            existing.status = existing.stage;
            existing.timestamp = now_millis();
            if existing.stage == UpdateStage::Cancelled {
                existing.error_code = Some("cancelled".to_string());
                existing.error_message = Some("Update cancelled by user".to_string());
            } else {
                existing.error_code = Some(error_code_for(&error).to_string());
                existing.error_message = Some(error.to_string());
            }
            status = Some(existing.clone());
        }
        if runtime.active_task_id.as_deref() == Some(task_id) {
            runtime.active_task_id = None;
        }
    }

    if let Some(status) = status {
        let version_dir = version_dir(&app, &status.version)?;
        if let Some(mut persisted) = load_state_file(&version_dir).await? {
            persisted.status = status.clone();
            save_state_file(&version_dir, &persisted).await?;
        }
        emit_status_event(
            &app,
            if status.stage == UpdateStage::Cancelled {
                ResumableEventType::Cancelled
            } else {
                ResumableEventType::Error
            },
            status,
        );
    }

    Ok(())
}

async fn clear_active_task_if_needed(
    runtime_state: Arc<Mutex<UpdateManagerRuntime>>,
    task_id: &str,
) {
    let mut runtime = lock_update_runtime(&runtime_state, "clear_active_task_if_needed").await;
    if runtime.active_task_id.as_deref() == Some(task_id) {
        runtime.active_task_id = None;
    }
}

async fn store_runtime_status(
    runtime_state: Arc<Mutex<UpdateManagerRuntime>>,
    status: ResumableUpdateStatus,
) {
    let mut runtime = lock_update_runtime(&runtime_state, "store_runtime_status").await;
    runtime.statuses.insert(status.task_id.clone(), status);
}

async fn ensure_task_not_cancelled(
    runtime_state: Arc<Mutex<UpdateManagerRuntime>>,
    task_id: &str,
) -> Result<(), UpdateError> {
    if is_task_cancelled(runtime_state, task_id).await {
        return Err(UpdateError::General("update cancelled".to_string()));
    }
    Ok(())
}

async fn is_task_cancelled(runtime_state: Arc<Mutex<UpdateManagerRuntime>>, task_id: &str) -> bool {
    let runtime = lock_update_runtime(&runtime_state, "is_task_cancelled").await;
    runtime.cancelled_tasks.contains(task_id)
}

fn emit_status_event(
    app: &AppHandle,
    event_type: ResumableEventType,
    status: ResumableUpdateStatus,
) {
    let payload = ResumableEventPayload { event_type, status };
    let _ = app.emit(RESUMABLE_EVENT_NAME, payload);
}

fn new_persisted_state(update: &Update, task_id: Option<String>) -> PersistedUpdateState {
    let task_id = task_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let status = ResumableUpdateStatus {
        task_id,
        version: update.version.clone(),
        attempt: 1,
        downloaded_bytes: 0,
        total_bytes: None,
        resumable: true,
        stage: UpdateStage::Downloading,
        status: UpdateStage::Downloading,
        error_code: None,
        error_message: None,
        timestamp: now_millis(),
        retry_delay_ms: None,
        last_http_status: None,
        can_resume_after_restart: true,
    };

    PersistedUpdateState {
        status,
        download_url: update.download_url.to_string(),
        signature: update.signature.clone(),
        etag: None,
        last_modified: None,
    }
}

// ── File system helpers ─────────────────────────────────────

fn updates_root(app: &AppHandle) -> Result<PathBuf, UpdateError> {
    let app_data = portable_aware_app_data_dir(app)
        .map_err(|err| UpdateError::State(format!("get app data dir failed: {err}")))?;
    Ok(app_data.join("updates"))
}

fn sanitize_path_segment(segment: &str) -> String {
    segment
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn version_dir(app: &AppHandle, version: &str) -> Result<PathBuf, UpdateError> {
    Ok(updates_root(app)?.join(sanitize_path_segment(version)))
}

async fn load_state_file(version_dir: &Path) -> Result<Option<PersistedUpdateState>, UpdateError> {
    let path = version_dir.join(STATE_FILE_NAME);
    if !path.exists() {
        return Ok(None);
    }
    let raw = tokio::fs::read_to_string(&path)
        .await
        .map_err(|err| UpdateError::State(format!("read state file failed: {err}")))?;
    let state = serde_json::from_str::<PersistedUpdateState>(&raw)
        .map_err(|err| UpdateError::State(format!("parse state file failed: {err}")))?;
    Ok(Some(state))
}

async fn save_state_file(
    version_dir: &Path,
    state: &PersistedUpdateState,
) -> Result<(), UpdateError> {
    let path = version_dir.join(STATE_FILE_NAME);
    let body = serde_json::to_string_pretty(state)
        .map_err(|err| UpdateError::State(format!("serialize state failed: {err}")))?;
    tokio::fs::write(path, body)
        .await
        .map_err(|err| UpdateError::State(format!("write state file failed: {err}")))?;
    Ok(())
}

async fn clear_version_cache(version_dir: &Path) -> Result<(), UpdateError> {
    if !version_dir.exists() {
        return Ok(());
    }
    remove_part_file(version_dir).await?;
    if version_dir.join(STATE_FILE_NAME).exists() {
        tokio::fs::remove_file(version_dir.join(STATE_FILE_NAME))
            .await
            .map_err(|err| UpdateError::State(format!("remove state file failed: {err}")))?;
    }
    Ok(())
}

async fn remove_part_file(version_dir: &Path) -> Result<(), UpdateError> {
    let part_path = version_dir.join(PART_FILE_NAME);
    if part_path.exists() {
        tokio::fs::remove_file(part_path)
            .await
            .map_err(|err| UpdateError::State(format!("remove part file failed: {err}")))?;
    }
    Ok(())
}

async fn prune_update_cache(
    app: &AppHandle,
    keep_version: Option<&str>,
) -> Result<(), UpdateError> {
    let root = updates_root(app)?;
    if !root.exists() {
        return Ok(());
    }

    let keep_segment = keep_version.map(sanitize_path_segment);
    let mut resumable_dirs: Vec<(i64, PathBuf)> = Vec::new();
    let mut removable_dirs = Vec::new();
    let mut entries = tokio::fs::read_dir(&root)
        .await
        .map_err(|err| UpdateError::State(format!("read updates directory failed: {err}")))?;

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|err| UpdateError::State(format!("scan updates directory failed: {err}")))?
    {
        let file_type = entry.file_type().await.map_err(|err| {
            UpdateError::State(format!("read directory entry type failed: {err}"))
        })?;
        if !file_type.is_dir() {
            continue;
        }

        let path = entry.path();
        if keep_segment.as_ref().is_some_and(|segment| {
            path.file_name().and_then(|name| name.to_str()) == Some(segment.as_str())
        }) {
            continue;
        }

        // Only non-terminal downloads can be resumed after restart; terminal
        // states and no-state directories are stale update payload cache.
        match load_state_file(&path).await {
            Ok(Some(state)) if !state.status.stage.is_terminal() => {
                resumable_dirs.push((state.status.timestamp, path));
            }
            _ => removable_dirs.push(path),
        }
    }

    resumable_dirs.sort_by(|left, right| right.0.cmp(&left.0));
    removable_dirs.extend(
        resumable_dirs
            .into_iter()
            .skip(MAX_RETAINED_RESUMABLE_UPDATE_DIRS)
            .map(|(_, path)| path),
    );

    for dir in removable_dirs {
        tokio::fs::remove_dir_all(&dir)
            .await
            .map_err(|err| UpdateError::State(format!("remove cache dir failed: {err}")))?;
    }

    Ok(())
}

async fn load_status_by_task_id(
    app: &AppHandle,
    task_id: &str,
) -> Result<Option<ResumableUpdateStatus>, UpdateError> {
    let root = updates_root(app)?;
    if !root.exists() {
        return Ok(None);
    }
    let mut entries = tokio::fs::read_dir(root)
        .await
        .map_err(|err| UpdateError::State(format!("read updates directory failed: {err}")))?;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|err| UpdateError::State(format!("scan updates directory failed: {err}")))?
    {
        let file_type = entry.file_type().await.map_err(|err| {
            UpdateError::State(format!("read directory entry type failed: {err}"))
        })?;
        if !file_type.is_dir() {
            continue;
        }
        if let Some(state) = load_state_file(&entry.path()).await? {
            if state.status.task_id == task_id {
                return Ok(Some(state.status));
            }
        }
    }
    Ok(None)
}

async fn load_latest_persisted_status(
    app: &AppHandle,
) -> Result<Option<ResumableUpdateStatus>, UpdateError> {
    let root = updates_root(app)?;
    if !root.exists() {
        return Ok(None);
    }
    let mut latest: Option<ResumableUpdateStatus> = None;
    let mut entries = tokio::fs::read_dir(root)
        .await
        .map_err(|err| UpdateError::State(format!("read updates directory failed: {err}")))?;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|err| UpdateError::State(format!("scan updates directory failed: {err}")))?
    {
        let file_type = entry.file_type().await.map_err(|err| {
            UpdateError::State(format!("read directory entry type failed: {err}"))
        })?;
        if !file_type.is_dir() {
            continue;
        }
        if let Some(state) = load_state_file(&entry.path()).await? {
            match latest.as_ref() {
                Some(existing) if existing.timestamp >= state.status.timestamp => {}
                _ => latest = Some(state.status),
            }
        }
    }
    Ok(latest)
}

// ── Crypto helpers ──────────────────────────────────────────

fn updater_pubkey() -> Result<String, UpdateError> {
    let config_json: serde_json::Value =
        serde_json::from_str(include_str!("../tauri.conf.json"))
            .map_err(|err| UpdateError::Integrity(format!("parse tauri config failed: {err}")))?;
    let pub_key = config_json
        .get("plugins")
        .and_then(|p| p.get("updater"))
        .and_then(|u| u.get("pubkey"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            UpdateError::Integrity("updater pubkey missing in tauri config".to_string())
        })?;
    Ok(pub_key.to_string())
}

fn verify_signature(
    data: &[u8],
    release_signature: &str,
    pub_key: &str,
) -> Result<(), UpdateError> {
    let pub_key_decoded = base64_to_string(pub_key)?;
    let public_key = PublicKey::decode(&pub_key_decoded)
        .map_err(|err| UpdateError::Integrity(format!("decode public key failed: {err}")))?;
    let signature_decoded = base64_to_string(release_signature)?;
    let signature = Signature::decode(&signature_decoded)
        .map_err(|err| UpdateError::Integrity(format!("decode release signature failed: {err}")))?;
    public_key
        .verify(data, &signature, true)
        .map_err(|err| UpdateError::Integrity(format!("signature verification failed: {err}")))?;
    Ok(())
}

fn base64_to_string(b64: &str) -> Result<String, UpdateError> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|err| UpdateError::Integrity(format!("base64 decode failed: {err}")))?;
    std::str::from_utf8(&decoded)
        .map(|v| v.to_string())
        .map_err(|_| UpdateError::Integrity("invalid utf8 in signature".to_string()))
}

// ── Utility ─────────────────────────────────────────────────

fn parse_content_range_total(content_range: &str) -> Option<u64> {
    let (_, total_part) = content_range.split_once('/')?;
    if total_part == "*" {
        return None;
    }
    total_part.parse::<u64>().ok()
}

fn should_restart_full_download(range_requested: bool, status: StatusCode) -> bool {
    range_requested
        && matches!(
            status,
            StatusCode::OK | StatusCode::PRECONDITION_FAILED | StatusCode::RANGE_NOT_SATISFIABLE
        )
}

fn compute_retry_delay(attempt: u32) -> u64 {
    let exp = BASE_RETRY_DELAY_MS.saturating_mul(2u64.saturating_pow(attempt.saturating_sub(1)));
    exp.min(MAX_RETRY_DELAY_MS)
}

fn can_cancel_from_stage(stage: UpdateStage) -> bool {
    matches!(stage, UpdateStage::Downloading | UpdateStage::Verifying)
}

fn stage_label(stage: UpdateStage) -> &'static str {
    match stage {
        UpdateStage::Downloading => "downloading",
        UpdateStage::Verifying => "verifying",
        UpdateStage::Installing => "installing",
        UpdateStage::Ready => "ready",
        UpdateStage::Error => "error",
        UpdateStage::Cancelled => "cancelled",
    }
}

fn cancel_not_allowed_error(stage: UpdateStage) -> UpdateError {
    UpdateError::General(format!(
        "UPDATE_CANCEL_NOT_ALLOWED: stage={}",
        stage_label(stage)
    ))
}

fn now_millis() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
