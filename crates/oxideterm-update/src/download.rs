// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use futures_util::StreamExt as _;
use oxideterm_settings::UpdateChannel;
use reqwest::{
    StatusCode,
    header::{
        ACCEPT, ACCEPT_ENCODING, CONTENT_LENGTH, CONTENT_RANGE, ETAG, HeaderValue, IF_RANGE,
        IF_UNMODIFIED_SINCE, LAST_MODIFIED, RANGE,
    },
};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::{
    NativeUpdateManifest, NativeUpdatePackage, NativeUpdateStage, PersistedUpdateState,
    PlatformTarget, ResumableUpdateStatus, TauriUpdaterEvent, current_platform_target,
    endpoint_for_channel, integrity::verify_minisign_signature, state::now_millis,
};

const STATE_FILE_NAME: &str = "state.json";
const PART_FILE_NAME: &str = "package.part";
const MAX_DOWNLOAD_ATTEMPTS: u32 = 3;
const BASE_RETRY_DELAY_MS: u64 = 1_500;
const MAX_RETRY_DELAY_MS: u64 = 12_000;
const DOWNLOAD_TIMEOUT_MS: u64 = 120_000;
const SAVE_STATE_INTERVAL_BYTES: u64 = 256 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum NativeUpdateError {
    #[error("Update error: {0}")]
    General(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Integrity error: {0}")]
    Integrity(String),

    #[error("State error: {0}")]
    State(String),

    #[error("Failed to build update HTTP client: {0}")]
    Client(#[source] reqwest::Error),

    #[error("Failed to fetch update manifest: {0}")]
    ManifestFetch(#[source] reqwest::Error),

    #[error("Update manifest returned HTTP {status}: {url}")]
    ManifestStatus {
        status: reqwest::StatusCode,
        url: String,
    },

    #[error("Failed to parse update manifest: {0}")]
    ManifestJson(#[source] serde_json::Error),

    #[error("No update package found for platform {os}/{arch}")]
    UnsupportedPlatform {
        os: &'static str,
        arch: &'static str,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeUpdateRequest {
    pub channel: UpdateChannel,
    pub current_version: String,
    pub target: PlatformTarget,
}

impl NativeUpdateRequest {
    pub fn current(channel: UpdateChannel, current_version: impl Into<String>) -> Self {
        Self {
            channel,
            current_version: current_version.into(),
            target: current_platform_target(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeUpdateStatus {
    UpToDate,
    Available(NativeUpdatePackage),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeUpdateDownload {
    pub package: NativeUpdatePackage,
    pub path: PathBuf,
    pub bytes: u64,
    pub sha256: String,
    pub status: ResumableUpdateStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DownloadProgress {
    pub event: TauriUpdaterEvent,
    pub status: ResumableUpdateStatus,
}

#[derive(Clone)]
pub struct NativeUpdateClient {
    http: reqwest::Client,
}

impl NativeUpdateClient {
    pub fn new() -> Result<Self, NativeUpdateError> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_millis(DOWNLOAD_TIMEOUT_MS))
            .user_agent(format!("OxideTerm/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(NativeUpdateError::Client)?;
        Ok(Self { http })
    }

    pub async fn check(
        &self,
        request: NativeUpdateRequest,
    ) -> Result<NativeUpdateStatus, NativeUpdateError> {
        let endpoint = endpoint_for_channel(request.channel);
        let response = self
            .http
            .get(endpoint.url)
            .send()
            .await
            .map_err(NativeUpdateError::ManifestFetch)?;
        if !response.status().is_success() {
            return Err(NativeUpdateError::ManifestStatus {
                status: response.status(),
                url: endpoint.url.to_string(),
            });
        }

        let bytes = response
            .bytes()
            .await
            .map_err(NativeUpdateError::ManifestFetch)?;
        let manifest: NativeUpdateManifest =
            serde_json::from_slice(&bytes).map_err(NativeUpdateError::ManifestJson)?;

        match manifest.select_package(&request.current_version, &request.target) {
            Some(package) => Ok(NativeUpdateStatus::Available(package)),
            None if manifest.platforms.is_empty() => Ok(NativeUpdateStatus::UpToDate),
            None if crate::is_update_newer(&manifest.version, &request.current_version) => {
                Err(NativeUpdateError::UnsupportedPlatform {
                    os: request.target.os(),
                    arch: request.target.arch(),
                })
            }
            None => Ok(NativeUpdateStatus::UpToDate),
        }
    }

    pub async fn download_package<F>(
        &self,
        package: NativeUpdatePackage,
        directory: &Path,
        progress: F,
    ) -> Result<NativeUpdateDownload, NativeUpdateError>
    where
        F: FnMut(DownloadProgress) + Send,
    {
        self.download_resumable_package(
            package,
            directory,
            Arc::new(AtomicBool::new(false)),
            progress,
        )
        .await
    }

    pub async fn download_resumable_package<F>(
        &self,
        package: NativeUpdatePackage,
        cache_root: &Path,
        cancel: Arc<AtomicBool>,
        mut progress: F,
    ) -> Result<NativeUpdateDownload, NativeUpdateError>
    where
        F: FnMut(DownloadProgress) + Send,
    {
        tokio::fs::create_dir_all(cache_root)
            .await
            .map_err(|error| {
                NativeUpdateError::State(format!("create update root failed: {error}"))
            })?;
        let version_dir = cache_root.join(sanitize_path_segment(&package.version));
        tokio::fs::create_dir_all(&version_dir)
            .await
            .map_err(|error| {
                NativeUpdateError::State(format!("create update dir failed: {error}"))
            })?;

        let mut persisted = load_state_file(&version_dir)
            .await?
            .unwrap_or_else(|| new_persisted_state(&package));

        if persisted.status.stage.is_terminal()
            || persisted.download_url != package.url
            || persisted.signature != package.signature
        {
            clear_version_cache(&version_dir).await?;
            persisted = new_persisted_state(&package);
        }

        let is_resumed = persisted.status.downloaded_bytes > 0;
        set_stage(&mut persisted.status, NativeUpdateStage::Downloading);
        save_state_file(&version_dir, &persisted).await?;
        emit_progress(
            &mut progress,
            if is_resumed {
                TauriUpdaterEvent::Resumed
            } else {
                TauriUpdaterEvent::Started
            },
            &persisted.status,
        );

        let package_bytes = self
            .download_with_retries(&version_dir, &mut persisted, &cancel, &mut progress)
            .await?;

        ensure_not_cancelled(&cancel)?;
        set_stage(&mut persisted.status, NativeUpdateStage::Verifying);
        save_state_file(&version_dir, &persisted).await?;
        emit_progress(
            &mut progress,
            TauriUpdaterEvent::Verifying,
            &persisted.status,
        );

        if let Some(total_bytes) = persisted.status.total_bytes {
            if package_bytes.len() as u64 != total_bytes {
                return Err(NativeUpdateError::Integrity(format!(
                    "size mismatch: got {}, expected {total_bytes}",
                    package_bytes.len()
                )));
            }
        }
        let signature = package
            .signature
            .as_deref()
            .ok_or_else(|| NativeUpdateError::Integrity("release signature missing".to_string()))?;
        verify_minisign_signature(&package_bytes, signature)?;

        let final_path = version_dir.join(package_file_name(&package));
        tokio::fs::rename(version_dir.join(PART_FILE_NAME), &final_path)
            .await
            .map_err(|error| {
                NativeUpdateError::State(format!("finalize update package failed: {error}"))
            })?;

        let mut hasher = Sha256::new();
        hasher.update(&package_bytes);
        set_stage(&mut persisted.status, NativeUpdateStage::Ready);
        save_state_file(&version_dir, &persisted).await?;
        emit_progress(&mut progress, TauriUpdaterEvent::Ready, &persisted.status);

        Ok(NativeUpdateDownload {
            package,
            path: final_path,
            bytes: package_bytes.len() as u64,
            sha256: format!("{:x}", hasher.finalize()),
            status: persisted.status,
        })
    }

    async fn download_with_retries<F>(
        &self,
        version_dir: &Path,
        persisted: &mut PersistedUpdateState,
        cancel: &Arc<AtomicBool>,
        progress: &mut F,
    ) -> Result<Vec<u8>, NativeUpdateError>
    where
        F: FnMut(DownloadProgress) + Send,
    {
        let part_path = version_dir.join(PART_FILE_NAME);
        let mut next_attempt = persisted.status.attempt.max(1);

        while next_attempt <= MAX_DOWNLOAD_ATTEMPTS {
            ensure_not_cancelled(cancel)?;
            persisted.status.attempt = next_attempt;
            persisted.status.retry_delay_ms = None;
            set_stage(&mut persisted.status, NativeUpdateStage::Downloading);
            save_state_file(version_dir, persisted).await?;

            if next_attempt > 1 {
                let retry_delay_ms = compute_retry_delay(next_attempt - 1);
                persisted.status.retry_delay_ms = Some(retry_delay_ms);
                save_state_file(version_dir, persisted).await?;
                emit_progress(progress, TauriUpdaterEvent::Retrying, &persisted.status);
                tokio::time::sleep(Duration::from_millis(retry_delay_ms)).await;
            }

            match self
                .download_once(version_dir, persisted, &part_path, cancel, progress)
                .await
            {
                Ok(()) => {
                    return tokio::fs::read(&part_path).await.map_err(|error| {
                        NativeUpdateError::State(format!("read downloaded package failed: {error}"))
                    });
                }
                Err(error) => {
                    ensure_not_cancelled(cancel)?;
                    if next_attempt >= MAX_DOWNLOAD_ATTEMPTS {
                        return Err(error);
                    }
                    let delay_ms = compute_retry_delay(next_attempt);
                    persisted.status.retry_delay_ms = Some(delay_ms);
                    persisted.status.timestamp = now_millis();
                    save_state_file(version_dir, persisted).await?;
                    emit_progress(progress, TauriUpdaterEvent::Retrying, &persisted.status);
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    next_attempt += 1;
                }
            }
        }

        Err(NativeUpdateError::Network(
            "download retry exhausted".to_string(),
        ))
    }

    async fn download_once<F>(
        &self,
        version_dir: &Path,
        persisted: &mut PersistedUpdateState,
        part_path: &Path,
        cancel: &Arc<AtomicBool>,
        progress: &mut F,
    ) -> Result<(), NativeUpdateError>
    where
        F: FnMut(DownloadProgress) + Send,
    {
        let existing_len = tokio::fs::metadata(part_path)
            .await
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        if existing_len != persisted.status.downloaded_bytes {
            persisted.status.downloaded_bytes = existing_len;
        }

        let mut range_requested = existing_len > 0;
        let mut request = self
            .http
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

        let response = request.send().await.map_err(|error| {
            NativeUpdateError::Network(format!("download request failed: {error}"))
        })?;
        let status = response.status();
        persisted.status.last_http_status = Some(status.as_u16());

        if should_restart_full_download(range_requested, status) {
            if status == StatusCode::OK {
                range_requested = false;
                persisted.status.resumable = false;
                persisted.status.downloaded_bytes = 0;
                tokio::fs::write(part_path, &[] as &[u8])
                    .await
                    .map_err(|error| {
                        NativeUpdateError::State(format!("reset partial package failed: {error}"))
                    })?;
            } else {
                persisted.status.downloaded_bytes = 0;
                persisted.status.resumable = false;
                tokio::fs::write(part_path, &[] as &[u8])
                    .await
                    .map_err(|error| {
                        NativeUpdateError::State(format!(
                            "truncate partial package failed: {error}"
                        ))
                    })?;
                save_state_file(version_dir, persisted).await?;
                return Err(NativeUpdateError::Network(format!(
                    "resume rejected by server: http {}",
                    status.as_u16()
                )));
            }
        }

        if !status.is_success() {
            return Err(NativeUpdateError::Network(format!(
                "download request failed with status {}",
                status.as_u16()
            )));
        }

        let headers = response.headers().clone();
        let current_etag = headers
            .get(ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let current_last_modified = headers
            .get(LAST_MODIFIED)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);

        if range_requested
            && let (Some(old), Some(new_value)) = (persisted.etag.as_ref(), current_etag.as_ref())
            && old != new_value
        {
            persisted.status.downloaded_bytes = 0;
            persisted.status.resumable = false;
            tokio::fs::write(part_path, &[] as &[u8])
                .await
                .map_err(|error| {
                    NativeUpdateError::State(format!("truncate for etag reset failed: {error}"))
                })?;
            save_state_file(version_dir, persisted).await?;
            return Err(NativeUpdateError::Network(
                "updater resource changed (etag)".to_string(),
            ));
        }

        if current_etag.is_some() {
            persisted.etag = current_etag;
        }
        if current_last_modified.is_some() {
            persisted.last_modified = current_last_modified;
        }

        let total_bytes = if status == StatusCode::PARTIAL_CONTENT {
            persisted.status.resumable = true;
            headers
                .get(CONTENT_RANGE)
                .and_then(|value| value.to_str().ok())
                .and_then(parse_content_range_total)
                .or(persisted.status.total_bytes)
        } else {
            if range_requested {
                persisted.status.resumable = false;
            }
            headers
                .get(CONTENT_LENGTH)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok())
        };
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
                .map_err(|error| {
                    NativeUpdateError::State(format!("open part file failed: {error}"))
                })?
        } else {
            tokio::fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(part_path)
                .await
                .map_err(|error| {
                    NativeUpdateError::State(format!("open package file failed: {error}"))
                })?
        };

        let mut bytes_since_last_save = 0_u64;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            ensure_not_cancelled(cancel)?;
            let chunk = chunk.map_err(|error| {
                NativeUpdateError::Network(format!("error decoding response body: {error}"))
            })?;
            file.write_all(&chunk).await.map_err(|error| {
                NativeUpdateError::State(format!("write update chunk failed: {error}"))
            })?;
            downloaded = downloaded.saturating_add(chunk.len() as u64);
            bytes_since_last_save = bytes_since_last_save.saturating_add(chunk.len() as u64);
            persisted.status.downloaded_bytes = downloaded;
            persisted.status.timestamp = now_millis();
            persisted.status.error_code = None;
            persisted.status.error_message = None;

            emit_progress(progress, TauriUpdaterEvent::Progress, &persisted.status);

            if bytes_since_last_save >= SAVE_STATE_INTERVAL_BYTES {
                save_state_file(version_dir, persisted).await?;
                bytes_since_last_save = 0;
            }
        }

        file.flush().await.map_err(|error| {
            NativeUpdateError::State(format!("flush update package failed: {error}"))
        })?;
        save_state_file(version_dir, persisted).await?;

        if let Some(total) = persisted.status.total_bytes
            && persisted.status.downloaded_bytes < total
        {
            return Err(NativeUpdateError::Network(format!(
                "download incomplete: got {}, expected {total}",
                persisted.status.downloaded_bytes
            )));
        }

        Ok(())
    }
}

fn emit_progress(
    progress: &mut impl FnMut(DownloadProgress),
    event: TauriUpdaterEvent,
    status: &ResumableUpdateStatus,
) {
    progress(DownloadProgress {
        event,
        status: status.clone(),
    });
}

fn new_persisted_state(package: &NativeUpdatePackage) -> PersistedUpdateState {
    let status = ResumableUpdateStatus {
        task_id: Uuid::new_v4().to_string(),
        version: package.version.clone(),
        attempt: 1,
        downloaded_bytes: 0,
        total_bytes: None,
        resumable: true,
        stage: NativeUpdateStage::Downloading,
        status: NativeUpdateStage::Downloading,
        error_code: None,
        error_message: None,
        timestamp: now_millis(),
        retry_delay_ms: None,
        last_http_status: None,
        can_resume_after_restart: true,
    };

    PersistedUpdateState {
        status,
        download_url: package.url.clone(),
        signature: package.signature.clone(),
        etag: None,
        last_modified: None,
    }
}

fn set_stage(status: &mut ResumableUpdateStatus, stage: NativeUpdateStage) {
    status.stage = stage;
    status.status = stage;
    status.timestamp = now_millis();
    status.error_code = None;
    status.error_message = None;
}

fn ensure_not_cancelled(cancel: &Arc<AtomicBool>) -> Result<(), NativeUpdateError> {
    if cancel.load(Ordering::Relaxed) {
        Err(NativeUpdateError::General("update cancelled".to_string()))
    } else {
        Ok(())
    }
}

async fn load_state_file(
    version_dir: &Path,
) -> Result<Option<PersistedUpdateState>, NativeUpdateError> {
    let path = version_dir.join(STATE_FILE_NAME);
    if !path.exists() {
        return Ok(None);
    }
    let raw = tokio::fs::read_to_string(&path)
        .await
        .map_err(|error| NativeUpdateError::State(format!("read state file failed: {error}")))?;
    let state = serde_json::from_str::<PersistedUpdateState>(&raw)
        .map_err(|error| NativeUpdateError::State(format!("parse state file failed: {error}")))?;
    Ok(Some(state))
}

async fn save_state_file(
    version_dir: &Path,
    state: &PersistedUpdateState,
) -> Result<(), NativeUpdateError> {
    let path = version_dir.join(STATE_FILE_NAME);
    let body = serde_json::to_string_pretty(state)
        .map_err(|error| NativeUpdateError::State(format!("serialize state failed: {error}")))?;
    tokio::fs::write(path, body)
        .await
        .map_err(|error| NativeUpdateError::State(format!("write state file failed: {error}")))?;
    Ok(())
}

async fn clear_version_cache(version_dir: &Path) -> Result<(), NativeUpdateError> {
    if !version_dir.exists() {
        return Ok(());
    }
    let part_path = version_dir.join(PART_FILE_NAME);
    if part_path.exists() {
        tokio::fs::remove_file(&part_path).await.map_err(|error| {
            NativeUpdateError::State(format!("remove part file failed: {error}"))
        })?;
    }
    let state_path = version_dir.join(STATE_FILE_NAME);
    if state_path.exists() {
        tokio::fs::remove_file(&state_path).await.map_err(|error| {
            NativeUpdateError::State(format!("remove state file failed: {error}"))
        })?;
    }
    Ok(())
}

fn package_file_name(package: &NativeUpdatePackage) -> String {
    let source_name = package
        .url
        .rsplit('/')
        .next()
        .map(|name| name.split(['?', '#']).next().unwrap_or(name))
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("oxideterm-update");
    let sanitized = source_name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("{}-{}", package.version, sanitized)
}

fn sanitize_path_segment(segment: &str) -> String {
    segment
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

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
    let exp = BASE_RETRY_DELAY_MS.saturating_mul(2_u64.saturating_pow(attempt.saturating_sub(1)));
    exp.min(MAX_RETRY_DELAY_MS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_file_name_keeps_version_and_removes_path_unsafe_chars() {
        let name = package_file_name(&NativeUpdatePackage {
            version: "1.2.0-gpui-preview.1".into(),
            current_version: "1.2.0-gpui-preview.0".into(),
            body: None,
            date: None,
            platform_key: "darwin-aarch64".into(),
            url: "https://example.invalid/download/OxideTerm Preview.dmg?token=secret".into(),
            signature: None,
        });

        assert!(name.starts_with("1.2.0-gpui-preview.1-"));
        assert!(!name.contains('/'));
        assert!(!name.contains('?'));
    }

    #[test]
    fn retry_delay_matches_tauri_bounds() {
        assert_eq!(compute_retry_delay(1), 1_500);
        assert_eq!(compute_retry_delay(2), 3_000);
        assert_eq!(compute_retry_delay(8), 12_000);
    }

    #[test]
    fn parses_content_range_total_like_tauri_backend() {
        assert_eq!(parse_content_range_total("bytes 10-19/42"), Some(42));
        assert_eq!(parse_content_range_total("bytes 10-19/*"), None);
    }
}
