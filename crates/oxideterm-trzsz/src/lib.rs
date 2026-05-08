// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Native trzsz backend state and local-file IO.
//!
//! This crate intentionally mirrors the Tauri command backend shape without
//! depending on GPUI. The terminal protocol/filter layer consumes these APIs
//! later, but owner-scoped handles, path guards, and cleanup live here.

pub mod buffer;
pub mod consumer;
pub mod controller;
pub mod download;
pub mod error;
pub mod escape;
pub mod filter;
pub mod path_guard;
pub mod progress;
pub mod transfer;
pub mod types;
pub mod upload;

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::path::PathBuf;
use std::sync::{Arc, Weak};
use std::thread;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::Serialize;
use uuid::Uuid;

pub use consumer::{TrzszConsumer, TrzszConsumerEvent};
pub use error::{TrzszError, TrzszErrorCode};
pub use progress::TextProgressBar;
pub use transfer::{
    DEFAULT_MAX_DATA_CHUNK_SIZE, TrzszFileReader, TrzszFileWriter, TrzszSaveParam, TrzszTransfer,
    TrzszTransferInput,
};
pub use types::{
    TRZSZ_PROTOCOL_VERSION, TrzszDetectedHandshake, TrzszHandshakeMode, TrzszTransferDirection,
    TrzszTransferEvent, TrzszTransferPolicy, TrzszTransferSelection,
};

use self::path_guard::validate_download_target_path;

pub const TRZSZ_API_VERSION: u32 = 1;
pub const MAX_TRANSFER_CHUNK_SIZE: usize = 1024 * 1024;
pub(crate) const MAX_HANDLES_PER_OWNER: usize = 256;
const DEFAULT_HANDLE_TTL: Duration = Duration::from_secs(15 * 60);
const DEFAULT_JANITOR_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrzszCapabilitiesDto {
    pub api_version: u32,
    pub provider: &'static str,
    pub features: TrzszFeaturesDto,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrzszFeaturesDto {
    pub directory: bool,
    pub atomic_directory_stage: bool,
}

impl Default for TrzszCapabilitiesDto {
    fn default() -> Self {
        Self {
            api_version: TRZSZ_API_VERSION,
            provider: "trzsz",
            features: TrzszFeaturesDto {
                directory: true,
                atomic_directory_stage: false,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrzszUploadEntryDto {
    pub path_id: u64,
    pub path: String,
    pub rel_path: Vec<String>,
    pub size: u64,
    pub is_dir: bool,
    pub is_symlink: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrzszUploadHandleDto {
    pub handle_id: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrzszPreparedDownloadRootDto {
    pub root_path: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrzszCreateDownloadDirectoryDto {
    pub created: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrzszDownloadOpenDto {
    pub writer_id: String,
    pub local_name: String,
    pub display_name: String,
    pub temp_path: String,
    pub final_path: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrzszOwnerCleanupDto {
    pub owner_id: String,
    pub upload_handles: usize,
    pub download_handles: usize,
    pub cleanup_errors: usize,
}

pub struct TrzszState {
    inner: Mutex<TrzszStateInner>,
    handle_ttl: Duration,
}

struct TrzszStateInner {
    owners: HashMap<String, OwnerState>,
    upload_handles: HashMap<String, UploadHandle>,
    download_handles: HashMap<String, DownloadHandle>,
}

struct OwnerState {
    authorized_upload_paths: HashSet<PathBuf>,
    prepared_download_root: Option<PathBuf>,
    upload_handles: HashSet<String>,
    download_handles: HashSet<String>,
    download_directories: HashSet<PathBuf>,
    touched_at: Instant,
}

struct UploadHandle {
    owner_id: String,
    size: u64,
    file: File,
    touched_at: Instant,
}

struct DownloadHandle {
    owner_id: String,
    root_path: PathBuf,
    final_path: PathBuf,
    temp_path: PathBuf,
    overwrite: bool,
    file: File,
    touched_at: Instant,
}

struct CleanupPlan {
    temp_paths: Vec<PathBuf>,
    directory_paths: Vec<PathBuf>,
}

impl TrzszState {
    pub fn new() -> Arc<Self> {
        Self::with_ttl(DEFAULT_HANDLE_TTL, true)
    }

    #[cfg(test)]
    pub fn new_for_tests(handle_ttl: Duration) -> Arc<Self> {
        Self::with_ttl(handle_ttl, false)
    }

    fn with_ttl(handle_ttl: Duration, spawn_janitor: bool) -> Arc<Self> {
        let state = Arc::new(Self {
            inner: Mutex::new(TrzszStateInner {
                owners: HashMap::new(),
                upload_handles: HashMap::new(),
                download_handles: HashMap::new(),
            }),
            handle_ttl,
        });

        if spawn_janitor {
            Self::spawn_janitor(&state);
        }

        state
    }

    fn spawn_janitor(state: &Arc<Self>) {
        let weak = Arc::downgrade(state);
        let _ = thread::Builder::new()
            .name("trzsz-janitor".to_string())
            .spawn(move || janitor_loop(weak));
    }

    pub fn set_authorized_upload_paths(&self, owner_id: &str, paths: HashSet<PathBuf>) {
        self.purge_expired();
        let now = Instant::now();
        let mut inner = self.inner.lock();
        let owner = inner
            .owners
            .entry(owner_id.to_string())
            .or_insert_with(|| OwnerState::new(now));
        owner.authorized_upload_paths = paths;
        owner.touched_at = now;
    }

    pub fn is_upload_path_authorized(&self, owner_id: &str, path: &PathBuf) -> bool {
        self.purge_expired();
        let mut inner = self.inner.lock();
        let Some(owner) = inner.owners.get_mut(owner_id) else {
            return false;
        };
        owner.touched_at = Instant::now();
        owner.authorized_upload_paths.contains(path)
    }

    pub fn register_upload_handle(
        &self,
        owner_id: &str,
        file: File,
        size: u64,
    ) -> Result<TrzszUploadHandleDto, TrzszError> {
        self.purge_expired();
        let handle_id = Uuid::new_v4().to_string();
        let now = Instant::now();
        let mut inner = self.inner.lock();
        let owner = inner
            .owners
            .entry(owner_id.to_string())
            .or_insert_with(|| OwnerState::new(now));
        if owner.upload_handles.len() >= MAX_HANDLES_PER_OWNER {
            return Err(TrzszError::InvalidState(format!(
                "Too many active upload handles for owner: {owner_id}"
            )));
        }
        owner.upload_handles.insert(handle_id.clone());
        owner.touched_at = now;
        inner.upload_handles.insert(
            handle_id.clone(),
            UploadHandle {
                owner_id: owner_id.to_string(),
                size,
                file,
                touched_at: now,
            },
        );
        Ok(TrzszUploadHandleDto { handle_id, size })
    }

    pub fn read_upload_chunk(
        &self,
        owner_id: &str,
        handle_id: &str,
        offset: u64,
        length: usize,
    ) -> Result<Vec<u8>, TrzszError> {
        use std::io::{Read, Seek};

        self.purge_expired();
        let now = Instant::now();
        let mut inner = self.inner.lock();
        if let Some(owner) = inner.owners.get_mut(owner_id) {
            owner.touched_at = now;
        }
        let Some(handle) = inner.upload_handles.get_mut(handle_id) else {
            return Err(TrzszError::HandleNotFound(handle_id.to_string()));
        };
        if handle.owner_id != owner_id {
            return Err(TrzszError::HandleOwnerMismatch);
        }
        if offset > handle.size {
            return Err(TrzszError::InvalidOffset {
                offset,
                size: handle.size,
            });
        }

        handle.touched_at = now;
        handle.file.seek(std::io::SeekFrom::Start(offset))?;
        let mut buffer = vec![0; length];
        let bytes_read = handle.file.read(&mut buffer)?;
        buffer.truncate(bytes_read);
        Ok(buffer)
    }

    pub fn close_upload_handle(&self, owner_id: &str, handle_id: &str) -> Result<(), TrzszError> {
        self.purge_expired();
        let mut inner = self.inner.lock();
        let Some(handle) = inner.upload_handles.remove(handle_id) else {
            return Err(TrzszError::HandleNotFound(handle_id.to_string()));
        };
        if handle.owner_id != owner_id {
            inner.upload_handles.insert(handle_id.to_string(), handle);
            return Err(TrzszError::HandleOwnerMismatch);
        }

        release_upload_handle(&mut inner, owner_id, handle_id);
        Ok(())
    }

    pub fn prepare_download_root(
        &self,
        owner_id: &str,
        root_path: PathBuf,
    ) -> TrzszPreparedDownloadRootDto {
        self.purge_expired();
        let now = Instant::now();
        let mut inner = self.inner.lock();
        let owner = inner
            .owners
            .entry(owner_id.to_string())
            .or_insert_with(|| OwnerState::new(now));
        owner.prepared_download_root = Some(root_path.clone());
        owner.touched_at = now;
        TrzszPreparedDownloadRootDto {
            root_path: root_path.to_string_lossy().to_string(),
        }
    }

    pub fn prepared_download_root(&self, owner_id: &str) -> Option<PathBuf> {
        self.purge_expired();
        let mut inner = self.inner.lock();
        let owner = inner.owners.get_mut(owner_id)?;
        owner.touched_at = Instant::now();
        owner.prepared_download_root.clone()
    }

    #[expect(clippy::too_many_arguments)]
    pub fn register_download_handle(
        &self,
        owner_id: &str,
        local_name: String,
        display_name: String,
        root_path: PathBuf,
        final_path: PathBuf,
        temp_path: PathBuf,
        overwrite: bool,
        file: File,
    ) -> Result<TrzszDownloadOpenDto, TrzszError> {
        self.purge_expired();
        let writer_id = Uuid::new_v4().to_string();
        let now = Instant::now();
        let mut inner = self.inner.lock();
        let owner = inner
            .owners
            .entry(owner_id.to_string())
            .or_insert_with(|| OwnerState::new(now));
        if owner.download_handles.len() >= MAX_HANDLES_PER_OWNER {
            return Err(TrzszError::InvalidState(format!(
                "Too many active download handles for owner: {owner_id}"
            )));
        }
        owner.download_handles.insert(writer_id.clone());
        owner.touched_at = now;
        inner.download_handles.insert(
            writer_id.clone(),
            DownloadHandle {
                owner_id: owner_id.to_string(),
                root_path,
                final_path: final_path.clone(),
                temp_path: temp_path.clone(),
                overwrite,
                file,
                touched_at: now,
            },
        );
        Ok(TrzszDownloadOpenDto {
            writer_id,
            local_name,
            display_name,
            temp_path: temp_path.to_string_lossy().to_string(),
            final_path: final_path.to_string_lossy().to_string(),
        })
    }

    pub fn register_download_directory(&self, owner_id: &str, directory_path: PathBuf) {
        self.purge_expired();
        let now = Instant::now();
        let mut inner = self.inner.lock();
        let owner = inner
            .owners
            .entry(owner_id.to_string())
            .or_insert_with(|| OwnerState::new(now));
        owner.download_directories.insert(directory_path);
        owner.touched_at = now;
    }

    pub fn commit_download_directory(&self, owner_id: &str, directory_path: &PathBuf) {
        self.purge_expired();
        let mut inner = self.inner.lock();
        if let Some(owner) = inner.owners.get_mut(owner_id) {
            owner.download_directories.remove(directory_path);
            owner.touched_at = Instant::now();
        }
    }

    pub fn write_download_chunk(
        &self,
        owner_id: &str,
        writer_id: &str,
        data: &[u8],
    ) -> Result<(), TrzszError> {
        use std::io::Write;

        self.purge_expired();
        let now = Instant::now();
        let mut inner = self.inner.lock();
        if let Some(owner) = inner.owners.get_mut(owner_id) {
            owner.touched_at = now;
        }
        let Some(handle) = inner.download_handles.get_mut(writer_id) else {
            return Err(TrzszError::HandleNotFound(writer_id.to_string()));
        };
        if handle.owner_id != owner_id {
            return Err(TrzszError::HandleOwnerMismatch);
        }

        handle.touched_at = now;
        handle.file.write_all(data)?;
        Ok(())
    }

    pub fn finish_download_handle(
        &self,
        owner_id: &str,
        writer_id: &str,
    ) -> Result<(), TrzszError> {
        use std::io::Write;

        self.purge_expired();
        let handle = {
            let mut inner = self.inner.lock();
            let Some(handle) = inner.download_handles.remove(writer_id) else {
                return Err(TrzszError::HandleNotFound(writer_id.to_string()));
            };
            if handle.owner_id != owner_id {
                inner.download_handles.insert(writer_id.to_string(), handle);
                return Err(TrzszError::HandleOwnerMismatch);
            }
            release_download_handle(&mut inner, owner_id, writer_id);
            handle
        };

        let mut handle = handle;
        let result = (|| -> Result<(), TrzszError> {
            handle.file.flush()?;
            handle.file.sync_all()?;
            drop(handle.file);

            validate_download_target_path(&handle.root_path, &handle.final_path)?;

            if let Ok(metadata) = std::fs::symlink_metadata(&handle.final_path) {
                if metadata.file_type().is_symlink() {
                    return Err(TrzszError::SymlinkNotAllowed(
                        handle.final_path.display().to_string(),
                    ));
                }
                if metadata.is_dir() {
                    return Err(TrzszError::InvalidPath(format!(
                        "Cannot replace existing directory: {}",
                        handle.final_path.display()
                    )));
                }
                if !handle.overwrite {
                    return Err(TrzszError::AlreadyExists(
                        handle.final_path.display().to_string(),
                    ));
                }
                std::fs::remove_file(&handle.final_path)?;
            }

            std::fs::rename(&handle.temp_path, &handle.final_path)?;
            Ok(())
        })();

        if result.is_err() {
            let _ = std::fs::remove_file(&handle.temp_path);
        }

        result
    }

    pub fn abort_download_handle(&self, owner_id: &str, writer_id: &str) -> Result<(), TrzszError> {
        let handle = {
            let mut inner = self.inner.lock();
            let Some(handle) = inner.download_handles.remove(writer_id) else {
                return Err(TrzszError::HandleNotFound(writer_id.to_string()));
            };
            if handle.owner_id != owner_id {
                inner.download_handles.insert(writer_id.to_string(), handle);
                return Err(TrzszError::HandleOwnerMismatch);
            }
            release_download_handle(&mut inner, owner_id, writer_id);
            handle
        };

        drop(handle.file);
        let _ = std::fs::remove_file(handle.temp_path);
        Ok(())
    }

    pub fn cleanup_owner(&self, owner_id: &str) -> TrzszOwnerCleanupDto {
        self.purge_expired();
        let (upload_count, download_count, cleanup_plan) = {
            let mut inner = self.inner.lock();
            let Some(owner) = inner.owners.remove(owner_id) else {
                return TrzszOwnerCleanupDto {
                    owner_id: owner_id.to_string(),
                    upload_handles: 0,
                    download_handles: 0,
                    cleanup_errors: 0,
                };
            };

            let mut cleanup_plan = CleanupPlan {
                temp_paths: Vec::new(),
                directory_paths: Vec::new(),
            };
            for handle_id in &owner.upload_handles {
                inner.upload_handles.remove(handle_id);
            }
            for handle_id in &owner.download_handles {
                if let Some(handle) = inner.download_handles.remove(handle_id) {
                    cleanup_plan.temp_paths.push(handle.temp_path);
                }
            }
            cleanup_plan
                .directory_paths
                .extend(owner.download_directories.iter().cloned());

            (
                owner.upload_handles.len(),
                owner.download_handles.len(),
                cleanup_plan,
            )
        };

        let cleanup_errors = cleanup_plan.execute();
        TrzszOwnerCleanupDto {
            owner_id: owner_id.to_string(),
            upload_handles: upload_count,
            download_handles: download_count,
            cleanup_errors,
        }
    }

    pub fn purge_expired(&self) {
        let cleanup = {
            let mut inner = self.inner.lock();
            collect_expired_cleanup(&mut inner, self.handle_ttl)
        };
        let _ = cleanup.execute();
    }
}

impl OwnerState {
    fn new(now: Instant) -> Self {
        Self {
            authorized_upload_paths: HashSet::new(),
            prepared_download_root: None,
            upload_handles: HashSet::new(),
            download_handles: HashSet::new(),
            download_directories: HashSet::new(),
            touched_at: now,
        }
    }
}

impl CleanupPlan {
    fn execute(mut self) -> usize {
        let mut errors = 0;
        for temp_path in self.temp_paths {
            if std::fs::remove_file(temp_path).is_err() {
                errors += 1;
            }
        }

        self.directory_paths.sort_by(|left, right| {
            right
                .components()
                .count()
                .cmp(&left.components().count())
                .then_with(|| right.cmp(left))
        });
        for directory_path in self.directory_paths {
            if std::fs::remove_dir(directory_path).is_err() {
                errors += 1;
            }
        }
        errors
    }
}

fn janitor_loop(state: Weak<TrzszState>) {
    while let Some(state) = state.upgrade() {
        thread::sleep(DEFAULT_JANITOR_INTERVAL);
        state.purge_expired();
    }
}

fn collect_expired_cleanup(inner: &mut TrzszStateInner, ttl: Duration) -> CleanupPlan {
    let now = Instant::now();
    let mut cleanup = CleanupPlan {
        temp_paths: Vec::new(),
        directory_paths: Vec::new(),
    };

    let expired_uploads = inner
        .upload_handles
        .iter()
        .filter(|(_, handle)| now.duration_since(handle.touched_at) >= ttl)
        .map(|(handle_id, handle)| (handle_id.clone(), handle.owner_id.clone()))
        .collect::<Vec<_>>();
    for (handle_id, owner_id) in expired_uploads {
        inner.upload_handles.remove(&handle_id);
        release_upload_handle(inner, &owner_id, &handle_id);
    }

    let expired_downloads = inner
        .download_handles
        .iter()
        .filter(|(_, handle)| now.duration_since(handle.touched_at) >= ttl)
        .map(|(handle_id, handle)| (handle_id.clone(), handle.owner_id.clone()))
        .collect::<Vec<_>>();
    for (handle_id, owner_id) in expired_downloads {
        if let Some(handle) = inner.download_handles.remove(&handle_id) {
            cleanup.temp_paths.push(handle.temp_path);
        }
        release_download_handle(inner, &owner_id, &handle_id);
    }

    let expired_owners = inner
        .owners
        .iter()
        .filter(|(_, owner)| {
            owner.upload_handles.is_empty()
                && owner.download_handles.is_empty()
                && now.duration_since(owner.touched_at) >= ttl
        })
        .map(|(owner_id, _)| owner_id.clone())
        .collect::<Vec<_>>();
    for owner_id in expired_owners {
        if let Some(owner) = inner.owners.remove(&owner_id) {
            cleanup
                .directory_paths
                .extend(owner.download_directories.into_iter());
        }
    }

    cleanup
}

fn release_upload_handle(inner: &mut TrzszStateInner, owner_id: &str, handle_id: &str) {
    if let Some(owner) = inner.owners.get_mut(owner_id) {
        owner.upload_handles.remove(handle_id);
        owner.touched_at = Instant::now();
    }
}

fn release_download_handle(inner: &mut TrzszStateInner, owner_id: &str, handle_id: &str) {
    if let Some(owner) = inner.owners.get_mut(owner_id) {
        owner.download_handles.remove(handle_id);
        owner.touched_at = Instant::now();
    }
}
