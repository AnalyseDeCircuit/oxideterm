// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::{HashSet, VecDeque},
    fmt,
    future::Future,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use futures_util::stream::{self, StreamExt, TryStreamExt};
use russh_sftp::{
    client::{SftpSession as RusshSftpSession, error::Error as SftpErrorInner},
    protocol::{FileAttributes, OpenFlags},
};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tracing::{debug, info, warn};

use super::{
    error::SftpError,
    path_utils::{is_absolute_remote_path, join_local_path, join_remote_path},
    types::{
        AdaptiveChunkSizer, AssetFileKind, FileInfo, FileType, ListFilter, PreviewContent,
        SortOrder, TransferDirection, TransferProgress, TransferState, constants,
        detect_and_decode, extension_to_language, generate_hex_dump, is_likely_text_content,
        is_office_extension, is_text_extension,
    },
};
use crate::{
    ProgressStore, SftpTransferGuard, SftpTransferManager, StoredTransferProgress, TransferType,
};

pub trait SftpChannelOpener: Clone + Send + Sync + 'static {
    fn open_sftp_channel(
        &self,
    ) -> impl Future<Output = Result<russh::Channel<russh::client::Msg>, SftpError>> + Send;
}

pub struct WriteContentResult {
    pub atomic_write: bool,
}

pub struct SftpSession {
    sftp: RusshSftpSession,
    session_id: String,
    cwd: String,
}

#[derive(Clone)]
struct DownloadFileJob {
    remote_path: String,
    local_path: String,
    total_bytes: u64,
}

#[derive(Clone)]
struct UploadFileJob {
    local_path: String,
    remote_path: String,
    total_bytes: u64,
}

impl fmt::Debug for SftpSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SftpSession")
            .field("session_id", &self.session_id)
            .field("cwd", &self.cwd)
            .finish_non_exhaustive()
    }
}

impl SftpSession {
    pub async fn new<O>(connection: O, session_id: String) -> Result<Self, SftpError>
    where
        O: SftpChannelOpener,
    {
        info!("Opening SFTP subsystem for session {session_id}");
        let channel = connection.open_sftp_channel().await?;
        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(|error| {
                SftpError::SubsystemNotAvailable(format!(
                    "Failed to request SFTP subsystem: {error}"
                ))
            })?;
        let sftp = RusshSftpSession::new(channel.into_stream())
            .await
            .map_err(|error| SftpError::SubsystemNotAvailable(error.to_string()))?;
        let cwd = sftp
            .canonicalize(".")
            .await
            .map_err(|error| SftpError::ProtocolError(error.to_string()))?;
        info!("SFTP subsystem opened for session {session_id}");
        Ok(Self {
            sftp,
            session_id,
            cwd,
        })
    }

    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    pub fn set_cwd(&mut self, path: String) {
        self.cwd = path;
    }

    pub async fn canonicalize(&self, path: &str) -> Result<String, SftpError> {
        self.resolve_path(path).await
    }

    pub async fn list_dir(
        &self,
        path: &str,
        filter: Option<ListFilter>,
    ) -> Result<Vec<FileInfo>, SftpError> {
        let canonical_path = self.resolve_path(path).await?;
        debug!("Listing SFTP directory: {canonical_path}");
        let read_dir = self
            .sftp
            .read_dir(&canonical_path)
            .await
            .map_err(|error| self.map_sftp_error(error, &canonical_path))?;
        let mut entries = Vec::new();

        for entry in read_dir {
            let name = entry.file_name();
            if name == "." || name == ".." {
                continue;
            }
            if filter.as_ref().is_some_and(|f| !f.show_hidden) && name.starts_with('.') {
                continue;
            }

            let full_path = join_remote_path(&canonical_path, &name);
            let metadata = entry.metadata();
            let entry_file_type = file_type_from_attrs(&metadata);
            let (symlink_target, target_file_type) = if entry_file_type == FileType::Symlink {
                let symlink_target = self.sftp.read_link(&full_path).await.ok();
                let target_file_type = self
                    .sftp
                    .metadata(&full_path)
                    .await
                    .ok()
                    .map(|target_metadata| file_type_from_attrs(&target_metadata));
                (symlink_target, target_file_type)
            } else {
                (None, None)
            };
            let file_type = classify_list_entry_file_type(entry_file_type, target_file_type);
            entries.push(FileInfo {
                name,
                path: full_path,
                file_type,
                size: metadata.size.unwrap_or(0),
                modified: metadata.mtime.map(|mtime| mtime as i64).unwrap_or(0),
                permissions: metadata
                    .permissions
                    .map(|permissions| format!("{:o}", permissions & 0o777))
                    .unwrap_or_else(|| "000".to_string()),
                owner: metadata.uid.map(|uid| uid.to_string()),
                group: metadata.gid.map(|gid| gid.to_string()),
                is_symlink: entry_file_type == FileType::Symlink,
                symlink_target,
            });
        }

        if let Some(pattern) = filter.as_ref().and_then(|filter| filter.pattern.as_ref())
            && let Ok(glob_pattern) = glob::Pattern::new(pattern)
        {
            entries.retain(|entry| glob_pattern.matches(&entry.name));
        }

        let sort_order = filter
            .as_ref()
            .map(|filter| filter.sort)
            .unwrap_or_default();
        sort_entries(&mut entries, sort_order);
        Ok(entries)
    }

    pub async fn stat(&self, path: &str) -> Result<FileInfo, SftpError> {
        let canonical_path = self.resolve_path(path).await?;
        let metadata = self
            .sftp
            .metadata(&canonical_path)
            .await
            .map_err(|error| self.map_sftp_error(error, &canonical_path))?;
        let name = Path::new(&canonical_path)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        let file_type = file_type_from_attrs(&metadata);
        let symlink_target = if file_type == FileType::Symlink {
            self.sftp.read_link(&canonical_path).await.ok()
        } else {
            None
        };
        Ok(FileInfo {
            name,
            path: canonical_path,
            file_type,
            size: metadata.size.unwrap_or(0),
            modified: metadata.mtime.map(|mtime| mtime as i64).unwrap_or(0),
            permissions: metadata
                .permissions
                .map(|permissions| format!("{:o}", permissions & 0o777))
                .unwrap_or_else(|| "000".to_string()),
            owner: metadata.uid.map(|uid| uid.to_string()),
            group: metadata.gid.map(|gid| gid.to_string()),
            is_symlink: file_type == FileType::Symlink,
            symlink_target,
        })
    }

    pub async fn write_content(
        &self,
        path: &str,
        content: &[u8],
    ) -> Result<WriteContentResult, SftpError> {
        let canonical_path = match self.resolve_path(path).await {
            Ok(path) => path,
            Err(_) => self.resolve_new_file_path(path).await?,
        };
        let swap_path = swap_path(&canonical_path);
        match self
            .write_to_swap_and_rename(&canonical_path, &swap_path, content)
            .await
        {
            Ok(()) => Ok(WriteContentResult { atomic_write: true }),
            Err(error) => {
                let error_string = error.to_string();
                let recoverable = matches!(error, SftpError::PermissionDenied(_))
                    || error_string.contains(".oxswp")
                    || error_string.contains("Atomic rename failed");
                if !recoverable {
                    return Err(error);
                }
                warn!(
                    "Atomic SFTP write failed for {canonical_path} ({error_string}), falling back to direct overwrite"
                );
                self.write_direct(&canonical_path, content).await?;
                Ok(WriteContentResult {
                    atomic_write: false,
                })
            }
        }
    }

    pub async fn preview(&self, path: &str) -> Result<PreviewContent, SftpError> {
        self.preview_with_offset(path, 0).await
    }

    pub async fn preview_with_offset(
        &self,
        path: &str,
        offset: u64,
    ) -> Result<PreviewContent, SftpError> {
        let canonical_path = self.resolve_path(path).await?;
        let metadata = self
            .sftp
            .metadata(&canonical_path)
            .await
            .map_err(|error| self.map_sftp_error(error, &canonical_path))?;
        let file_size = metadata.size.unwrap_or(0);
        let file_name = Path::new(&canonical_path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        let extension = Path::new(&canonical_path)
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let mime_type = mime_guess::from_path(&canonical_path)
            .first_or_octet_stream()
            .to_string();

        if is_text_extension(&extension) {
            return self
                .preview_text(&canonical_path, &extension, &mime_type, file_size)
                .await;
        }
        if file_name.starts_with('.') && extension.is_empty() {
            return self
                .preview_text(&canonical_path, "conf", &mime_type, file_size)
                .await;
        }
        if extension == "pdf" || mime_type == "application/pdf" {
            return self
                .preview_asset(&canonical_path, file_size, &mime_type, AssetFileKind::Pdf)
                .await;
        }
        if is_office_extension(&extension) {
            return self
                .preview_asset(
                    &canonical_path,
                    file_size,
                    &mime_type,
                    AssetFileKind::Office,
                )
                .await;
        }
        if mime_type.starts_with("image/") {
            return self
                .preview_image(&canonical_path, file_size, &mime_type)
                .await;
        }
        if mime_type.starts_with("video/")
            || matches!(
                extension.as_str(),
                "mp4" | "webm" | "ogg" | "mov" | "mkv" | "avi"
            )
        {
            return self
                .preview_asset(&canonical_path, file_size, &mime_type, AssetFileKind::Video)
                .await;
        }
        if mime_type.starts_with("audio/")
            || matches!(
                extension.as_str(),
                "mp3" | "wav" | "ogg" | "flac" | "aac" | "m4a"
            )
        {
            return self
                .preview_asset(&canonical_path, file_size, &mime_type, AssetFileKind::Audio)
                .await;
        }

        let is_text_mime = mime_type.starts_with("text/")
            || matches!(
                mime_type.as_str(),
                "application/json"
                    | "application/xml"
                    | "application/javascript"
                    | "application/toml"
                    | "application/yaml"
            );
        if is_text_mime {
            return self
                .preview_text(&canonical_path, &extension, &mime_type, file_size)
                .await;
        }
        if (extension.is_empty() || mime_type == "application/octet-stream")
            && file_size <= constants::MAX_TEXT_PREVIEW_SIZE
        {
            let sample = self
                .read_file_limited(&canonical_path, file_size.min(8192) as usize)
                .await?;
            if is_likely_text_content(&sample) {
                return self
                    .preview_text(&canonical_path, "txt", "text/plain", file_size)
                    .await;
            }
        }

        self.preview_hex(&canonical_path, file_size, offset).await
    }

    pub async fn delete(&self, path: &str) -> Result<(), SftpError> {
        let canonical_path = self.resolve_path(path).await?;
        let metadata = self
            .sftp
            .symlink_metadata(&canonical_path)
            .await
            .map_err(|error| self.map_sftp_error(error, &canonical_path))?;
        if metadata.is_dir() && !metadata.is_symlink() {
            self.sftp
                .remove_dir(&canonical_path)
                .await
                .map_err(|error| self.map_sftp_error(error, &canonical_path))
        } else {
            self.sftp
                .remove_file(&canonical_path)
                .await
                .map_err(|error| self.map_sftp_error(error, &canonical_path))
        }
    }

    pub async fn delete_recursive(&self, path: &str) -> Result<u64, SftpError> {
        let canonical_path = self.resolve_path(path).await?;
        self.delete_recursive_inner(&canonical_path).await
    }

    pub async fn mkdir(&self, path: &str) -> Result<(), SftpError> {
        let canonical_path = if is_absolute_remote_path(path) {
            path.to_string()
        } else {
            join_remote_path(&self.cwd, path)
        };
        self.sftp
            .create_dir(&canonical_path)
            .await
            .map_err(|error| self.map_sftp_error(error, &canonical_path))
    }

    pub async fn rename(&self, old_path: &str, new_path: &str) -> Result<(), SftpError> {
        let old_canonical = self.resolve_path(old_path).await?;
        let new_canonical = if is_absolute_remote_path(new_path) {
            new_path.to_string()
        } else {
            let parent = old_canonical
                .rsplit_once('/')
                .map(|(parent, _)| parent)
                .filter(|parent| !parent.is_empty())
                .unwrap_or("/");
            join_remote_path(parent, new_path)
        };
        self.sftp
            .rename(&old_canonical, &new_canonical)
            .await
            .map_err(|error| self.map_sftp_error(error, &old_canonical))
    }

    async fn delete_recursive_inner(&self, path: &str) -> Result<u64, SftpError> {
        let metadata = self
            .sftp
            .symlink_metadata(path)
            .await
            .map_err(|error| self.map_sftp_error(error, path))?;
        if !metadata.is_dir() || metadata.is_symlink() {
            self.sftp
                .remove_file(path)
                .await
                .map_err(|error| self.map_sftp_error(error, path))?;
            return Ok(1);
        }

        let mut deleted_count = 0;
        let entries = self
            .list_dir(
                path,
                Some(ListFilter {
                    show_hidden: true,
                    pattern: None,
                    sort: SortOrder::Name,
                }),
            )
            .await?;
        for entry in entries {
            deleted_count += Box::pin(self.delete_recursive_inner(&entry.path)).await?;
        }
        self.sftp
            .remove_dir(path)
            .await
            .map_err(|error| self.map_sftp_error(error, path))?;
        Ok(deleted_count + 1)
    }

    pub async fn download_file(
        &self,
        remote_path: &str,
        local_path: &str,
        transfer_id: &str,
        progress_tx: Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        transfer_manager: Option<Arc<SftpTransferManager>>,
    ) -> Result<u64, SftpError> {
        let _control = transfer_manager
            .as_ref()
            .map(|manager| manager.register(transfer_id));
        let _guard = SftpTransferGuard::new(transfer_manager.as_ref(), transfer_id);
        let canonical_remote = self.resolve_path(remote_path).await?;
        let remote_info = self.stat(&canonical_remote).await?;
        self.download_file_inner(
            &DownloadFileJob {
                remote_path: canonical_remote,
                local_path: local_path.to_string(),
                total_bytes: remote_info.size,
            },
            transfer_id,
            &progress_tx,
            &transfer_manager,
        )
        .await?;
        Ok(remote_info.size)
    }

    pub async fn upload_file(
        &self,
        local_path: &str,
        remote_path: &str,
        transfer_id: &str,
        progress_tx: Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        transfer_manager: Option<Arc<SftpTransferManager>>,
    ) -> Result<u64, SftpError> {
        let _control = transfer_manager
            .as_ref()
            .map(|manager| manager.register(transfer_id));
        let _guard = SftpTransferGuard::new(transfer_manager.as_ref(), transfer_id);
        let metadata = tokio::fs::metadata(local_path)
            .await
            .map_err(SftpError::IoError)?;
        let canonical_remote = self.resolve_new_file_path(remote_path).await?;
        self.upload_file_inner(
            &UploadFileJob {
                local_path: local_path.to_string(),
                remote_path: canonical_remote,
                total_bytes: metadata.len(),
            },
            transfer_id,
            &progress_tx,
            &transfer_manager,
        )
        .await?;
        Ok(metadata.len())
    }

    pub async fn download_with_resume(
        &self,
        remote_path: &str,
        local_path: &str,
        progress_store: Arc<dyn ProgressStore>,
        progress_tx: Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        transfer_manager: Option<Arc<SftpTransferManager>>,
        transfer_id: Option<String>,
    ) -> Result<u64, SftpError> {
        let transfer_id = transfer_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let _control = transfer_manager
            .as_ref()
            .map(|manager| manager.register(&transfer_id));
        let _guard = SftpTransferGuard::new(transfer_manager.as_ref(), transfer_id.clone());
        let canonical_remote = self.resolve_path(remote_path).await?;
        let remote_info = self.stat(&canonical_remote).await?;
        let total_bytes = remote_info.size;
        let mut offset = match tokio::fs::metadata(local_path).await {
            Ok(metadata) => metadata.len(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
            Err(error) => return Err(SftpError::IoError(error)),
        };

        let stored = progress_store.load(&transfer_id).await?;
        if stored
            .as_ref()
            .is_some_and(|progress| progress.total_bytes != total_bytes)
            || offset > total_bytes
        {
            progress_store.delete(&transfer_id).await?;
            tokio::fs::File::create(local_path)
                .await
                .map_err(SftpError::IoError)?;
            offset = 0;
        }

        let mut stored_progress = StoredTransferProgress::new(
            transfer_id.clone(),
            TransferType::Download,
            PathBuf::from(&canonical_remote),
            PathBuf::from(local_path),
            total_bytes,
            self.session_id.clone(),
        );
        stored_progress.transferred_bytes = offset;
        progress_store.save(&stored_progress).await?;

        let result = self
            .download_file_resume_inner(
                &DownloadFileJob {
                    remote_path: canonical_remote.clone(),
                    local_path: local_path.to_string(),
                    total_bytes,
                },
                &transfer_id,
                offset,
                &progress_tx,
                &transfer_manager,
                progress_store.clone(),
                stored_progress,
            )
            .await;

        match result {
            Ok(transferred) => {
                progress_store.delete(&transfer_id).await?;
                Ok(transferred)
            }
            Err(SftpError::TransferCancelled) => {
                progress_store.delete(&transfer_id).await?;
                Err(SftpError::TransferCancelled)
            }
            Err(error) => {
                if let Ok(Some(mut progress)) = progress_store.load(&transfer_id).await {
                    progress.mark_failed(error.to_string());
                    let _ = progress_store.save(&progress).await;
                }
                Err(error)
            }
        }
    }

    pub async fn upload_with_resume(
        &self,
        local_path: &str,
        remote_path: &str,
        progress_store: Arc<dyn ProgressStore>,
        progress_tx: Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        transfer_manager: Option<Arc<SftpTransferManager>>,
        transfer_id: Option<String>,
    ) -> Result<u64, SftpError> {
        let transfer_id = transfer_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let _control = transfer_manager
            .as_ref()
            .map(|manager| manager.register(&transfer_id));
        let _guard = SftpTransferGuard::new(transfer_manager.as_ref(), transfer_id.clone());
        let canonical_remote = self.resolve_new_file_path(remote_path).await?;
        let temp_remote = format!("{canonical_remote}.oxide-part");
        let metadata = tokio::fs::metadata(local_path)
            .await
            .map_err(SftpError::IoError)?;
        let total_bytes = metadata.len();

        let stored = progress_store
            .list_incomplete(&self.session_id)
            .await?
            .into_iter()
            .find(|progress| {
                progress.transfer_type == TransferType::Upload
                    && progress.source_path == PathBuf::from(local_path)
                    && progress.destination_path == PathBuf::from(&canonical_remote)
            });
        if let Some(progress) = stored.as_ref()
            && progress.total_bytes != total_bytes
        {
            progress_store.delete(&progress.transfer_id).await?;
            let _ = self.delete(&temp_remote).await;
        }

        let offset = match self.stat(&temp_remote).await {
            Ok(info) if info.size >= total_bytes => {
                self.replace_remote_file(&temp_remote, &canonical_remote)
                    .await?;
                progress_store.delete(&transfer_id).await?;
                return Ok(total_bytes);
            }
            Ok(info) => info.size,
            Err(_) => 0,
        };

        let mut stored_progress = StoredTransferProgress::new(
            transfer_id.clone(),
            TransferType::Upload,
            PathBuf::from(local_path),
            PathBuf::from(&canonical_remote),
            total_bytes,
            self.session_id.clone(),
        );
        stored_progress.transferred_bytes = offset;
        progress_store.save(&stored_progress).await?;

        let result = self
            .upload_file_resume_inner(
                &UploadFileJob {
                    local_path: local_path.to_string(),
                    remote_path: temp_remote.clone(),
                    total_bytes,
                },
                &transfer_id,
                offset,
                &progress_tx,
                &transfer_manager,
                progress_store.clone(),
                stored_progress,
            )
            .await;

        match result {
            Ok(transferred) => {
                self.replace_remote_file(&temp_remote, &canonical_remote)
                    .await?;
                progress_store.delete(&transfer_id).await?;
                Ok(transferred)
            }
            Err(SftpError::TransferCancelled) => {
                let _ = self.delete(&temp_remote).await;
                progress_store.delete(&transfer_id).await?;
                Err(SftpError::TransferCancelled)
            }
            Err(error) => {
                if let Ok(Some(mut progress)) = progress_store.load(&transfer_id).await {
                    progress.mark_failed(error.to_string());
                    let _ = progress_store.save(&progress).await;
                }
                Err(error)
            }
        }
    }

    pub async fn download_dir(
        &self,
        remote_path: &str,
        local_path: &str,
        transfer_id: &str,
        progress_tx: Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        transfer_manager: Option<Arc<SftpTransferManager>>,
    ) -> Result<u64, SftpError> {
        let _control = transfer_manager
            .as_ref()
            .map(|manager| manager.register(transfer_id));
        let _guard = SftpTransferGuard::new(transfer_manager.as_ref(), transfer_id);
        let canonical_remote = self.resolve_path(remote_path).await?;
        tokio::fs::create_dir_all(local_path)
            .await
            .map_err(SftpError::IoError)?;
        let mut jobs = Vec::new();
        self.collect_download_jobs_depth(&canonical_remote, local_path, 0, &mut jobs)
            .await?;
        self.run_download_jobs(jobs, transfer_id, &progress_tx, &transfer_manager)
            .await
    }

    pub async fn upload_dir(
        &self,
        local_path: &str,
        remote_path: &str,
        transfer_id: &str,
        progress_tx: Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        transfer_manager: Option<Arc<SftpTransferManager>>,
    ) -> Result<u64, SftpError> {
        let _control = transfer_manager
            .as_ref()
            .map(|manager| manager.register(transfer_id));
        let _guard = SftpTransferGuard::new(transfer_manager.as_ref(), transfer_id);
        let canonical_remote = if is_absolute_remote_path(remote_path) {
            remote_path.to_string()
        } else {
            join_remote_path(&self.cwd, remote_path)
        };
        let mut jobs = Vec::new();
        let mut dirs = vec![canonical_remote.clone()];
        self.collect_upload_jobs_depth(local_path, &canonical_remote, 0, &mut dirs, &mut jobs)
            .await?;

        let mut seen = HashSet::new();
        for dir in dirs {
            if seen.insert(dir.clone()) {
                let _ = self.mkdir(&dir).await;
            }
        }

        self.run_upload_jobs(jobs, transfer_id, &progress_tx, &transfer_manager)
            .await
    }

    async fn collect_download_jobs_depth(
        &self,
        remote_path: &str,
        local_path: &str,
        depth: u32,
        jobs: &mut Vec<DownloadFileJob>,
    ) -> Result<(), SftpError> {
        const MAX_DEPTH: u32 = 64;
        if depth >= MAX_DEPTH {
            return Err(SftpError::TransferError(format!(
                "download recursion depth {MAX_DEPTH} reached at {remote_path}"
            )));
        }
        let mut stack = VecDeque::from([(remote_path.to_string(), local_path.to_string(), depth)]);
        while let Some((remote_dir, local_dir, current_depth)) = stack.pop_back() {
            if current_depth >= MAX_DEPTH {
                return Err(SftpError::TransferError(format!(
                    "download recursion depth {MAX_DEPTH} reached at {remote_dir}"
                )));
            }
            let entries = self
                .list_dir(
                    &remote_dir,
                    Some(ListFilter {
                        show_hidden: true,
                        pattern: None,
                        sort: SortOrder::Name,
                    }),
                )
                .await?;
            for entry in entries {
                let local_entry = join_local_path(&local_dir, &entry.name);
                if entry.file_type == FileType::Directory {
                    tokio::fs::create_dir_all(&local_entry)
                        .await
                        .map_err(SftpError::IoError)?;
                    stack.push_back((entry.path, local_entry, current_depth + 1));
                } else {
                    jobs.push(DownloadFileJob {
                        remote_path: entry.path,
                        local_path: local_entry,
                        total_bytes: entry.size,
                    });
                }
            }
        }
        Ok(())
    }

    async fn collect_upload_jobs_depth(
        &self,
        local_path: &str,
        remote_path: &str,
        depth: u32,
        all_remote_dirs: &mut Vec<String>,
        jobs: &mut Vec<UploadFileJob>,
    ) -> Result<(), SftpError> {
        const MAX_DEPTH: u32 = 64;
        if depth >= MAX_DEPTH {
            return Err(SftpError::TransferError(format!(
                "upload recursion depth {MAX_DEPTH} reached at {local_path}"
            )));
        }
        let mut stack =
            VecDeque::from([(PathBuf::from(local_path), remote_path.to_string(), depth)]);
        while let Some((local_dir, remote_dir, current_depth)) = stack.pop_back() {
            if current_depth >= MAX_DEPTH {
                return Err(SftpError::TransferError(format!(
                    "upload recursion depth {MAX_DEPTH} reached at {}",
                    local_dir.display()
                )));
            }
            let mut entries = tokio::fs::read_dir(&local_dir)
                .await
                .map_err(SftpError::IoError)?;
            while let Some(entry) = entries.next_entry().await.map_err(SftpError::IoError)? {
                let name = entry.file_name().to_string_lossy().to_string();
                let local_entry = entry.path();
                let remote_entry = join_remote_path(&remote_dir, &name);
                let metadata = match tokio::fs::symlink_metadata(&local_entry).await {
                    Ok(metadata) => metadata,
                    Err(error) => {
                        warn!(
                            "Skipping inaccessible local entry {:?}: {error}",
                            local_entry
                        );
                        continue;
                    }
                };
                if metadata.file_type().is_symlink() {
                    warn!(
                        "Skipping local symlink during SFTP upload: {:?}",
                        local_entry
                    );
                    continue;
                }
                if metadata.is_dir() {
                    all_remote_dirs.push(remote_entry.clone());
                    stack.push_back((local_entry, remote_entry, current_depth + 1));
                } else if metadata.is_file() {
                    jobs.push(UploadFileJob {
                        local_path: local_entry.to_string_lossy().to_string(),
                        remote_path: remote_entry,
                        total_bytes: metadata.len(),
                    });
                } else {
                    warn!(
                        "Skipping special local entry during SFTP upload: {:?}",
                        local_entry
                    );
                }
            }
        }
        Ok(())
    }

    async fn run_download_jobs(
        &self,
        jobs: Vec<DownloadFileJob>,
        transfer_id: &str,
        progress_tx: &Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        transfer_manager: &Option<Arc<SftpTransferManager>>,
    ) -> Result<u64, SftpError> {
        let parallelism = transfer_manager
            .as_ref()
            .map(|manager| manager.directory_parallelism())
            .unwrap_or(1)
            .clamp(1, crate::MAX_SFTP_DIRECTORY_PARALLELISM);
        if parallelism <= 1
            || transfer_manager
                .as_ref()
                .is_some_and(|m| m.speed_limit_bps() > 0)
        {
            for job in &jobs {
                self.download_file_inner(job, transfer_id, progress_tx, transfer_manager)
                    .await?;
            }
            return Ok(jobs.len() as u64);
        }
        stream::iter(jobs)
            .map(|job| async move {
                self.download_file_inner(&job, transfer_id, progress_tx, transfer_manager)
                    .await?;
                Ok::<u64, SftpError>(1)
            })
            .buffer_unordered(parallelism)
            .try_fold(0, |sum, count| async move { Ok(sum + count) })
            .await
    }

    async fn run_upload_jobs(
        &self,
        jobs: Vec<UploadFileJob>,
        transfer_id: &str,
        progress_tx: &Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        transfer_manager: &Option<Arc<SftpTransferManager>>,
    ) -> Result<u64, SftpError> {
        let parallelism = transfer_manager
            .as_ref()
            .map(|manager| manager.directory_parallelism())
            .unwrap_or(1)
            .clamp(1, crate::MAX_SFTP_DIRECTORY_PARALLELISM);
        if parallelism <= 1
            || transfer_manager
                .as_ref()
                .is_some_and(|m| m.speed_limit_bps() > 0)
        {
            for job in &jobs {
                self.upload_file_inner(job, transfer_id, progress_tx, transfer_manager)
                    .await?;
            }
            return Ok(jobs.len() as u64);
        }
        stream::iter(jobs)
            .map(|job| async move {
                self.upload_file_inner(&job, transfer_id, progress_tx, transfer_manager)
                    .await?;
                Ok::<u64, SftpError>(1)
            })
            .buffer_unordered(parallelism)
            .try_fold(0, |sum, count| async move { Ok(sum + count) })
            .await
    }

    async fn download_file_inner(
        &self,
        job: &DownloadFileJob,
        transfer_id: &str,
        progress_tx: &Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        transfer_manager: &Option<Arc<SftpTransferManager>>,
    ) -> Result<(), SftpError> {
        let mut remote_file = self
            .sftp
            .open(&job.remote_path)
            .await
            .map_err(|error| self.map_sftp_error(error, &job.remote_path))?;
        if let Some(parent) = Path::new(&job.local_path).parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(SftpError::IoError)?;
        }
        let mut local_file = tokio::fs::File::create(&job.local_path)
            .await
            .map_err(SftpError::IoError)?;
        let mut buffer = vec![0u8; AdaptiveChunkSizer::MAX_CHUNK];
        let mut chunk_sizer = AdaptiveChunkSizer::new();
        let started = Instant::now();
        let mut transferred = 0u64;
        let mut last_progress = Instant::now();
        loop {
            check_transfer_control(transfer_manager, transfer_id).await?;
            let chunk_size = chunk_sizer.chunk_size();
            let read = remote_file
                .read(&mut buffer[..chunk_size])
                .await
                .map_err(SftpError::IoError)?;
            if read == 0 {
                break;
            }
            local_file
                .write_all(&buffer[..read])
                .await
                .map_err(SftpError::IoError)?;
            transferred += read as u64;
            chunk_sizer.record(read);
            throttle_transfer(transferred, started, transfer_manager).await;
            if last_progress.elapsed().as_millis() >= 200 {
                send_transfer_progress(
                    progress_tx,
                    transfer_id,
                    &job.remote_path,
                    &job.local_path,
                    TransferDirection::Download,
                    job.total_bytes,
                    transferred,
                    started,
                    TransferState::InProgress,
                    None,
                )
                .await;
                last_progress = Instant::now();
            }
        }
        local_file.flush().await.map_err(SftpError::IoError)?;
        send_transfer_progress(
            progress_tx,
            transfer_id,
            &job.remote_path,
            &job.local_path,
            TransferDirection::Download,
            job.total_bytes,
            transferred,
            started,
            TransferState::Completed,
            None,
        )
        .await;
        Ok(())
    }

    async fn upload_file_inner(
        &self,
        job: &UploadFileJob,
        transfer_id: &str,
        progress_tx: &Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        transfer_manager: &Option<Arc<SftpTransferManager>>,
    ) -> Result<(), SftpError> {
        let mut local_file = tokio::fs::File::open(&job.local_path)
            .await
            .map_err(SftpError::IoError)?;
        let mut remote_file = self
            .sftp
            .open_with_flags(
                &job.remote_path,
                OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE,
            )
            .await
            .map_err(|error| self.map_sftp_error(error, &job.remote_path))?;
        let mut buffer = vec![0u8; AdaptiveChunkSizer::MAX_CHUNK];
        let mut chunk_sizer = AdaptiveChunkSizer::new();
        let started = Instant::now();
        let mut transferred = 0u64;
        let mut last_progress = Instant::now();
        loop {
            check_transfer_control(transfer_manager, transfer_id).await?;
            let chunk_size = chunk_sizer.chunk_size();
            let read = local_file
                .read(&mut buffer[..chunk_size])
                .await
                .map_err(SftpError::IoError)?;
            if read == 0 {
                break;
            }
            remote_file
                .write_all(&buffer[..read])
                .await
                .map_err(SftpError::IoError)?;
            transferred += read as u64;
            chunk_sizer.record(read);
            throttle_transfer(transferred, started, transfer_manager).await;
            if last_progress.elapsed().as_millis() >= 200 {
                send_transfer_progress(
                    progress_tx,
                    transfer_id,
                    &job.remote_path,
                    &job.local_path,
                    TransferDirection::Upload,
                    job.total_bytes,
                    transferred,
                    started,
                    TransferState::InProgress,
                    None,
                )
                .await;
                last_progress = Instant::now();
            }
        }
        remote_file.flush().await.map_err(SftpError::IoError)?;
        send_transfer_progress(
            progress_tx,
            transfer_id,
            &job.remote_path,
            &job.local_path,
            TransferDirection::Upload,
            job.total_bytes,
            transferred,
            started,
            TransferState::Completed,
            None,
        )
        .await;
        Ok(())
    }

    async fn download_file_resume_inner(
        &self,
        job: &DownloadFileJob,
        transfer_id: &str,
        offset: u64,
        progress_tx: &Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        transfer_manager: &Option<Arc<SftpTransferManager>>,
        progress_store: Arc<dyn ProgressStore>,
        mut stored_progress: StoredTransferProgress,
    ) -> Result<u64, SftpError> {
        let mut remote_file = self
            .sftp
            .open(&job.remote_path)
            .await
            .map_err(|error| self.map_sftp_error(error, &job.remote_path))?;
        if offset > 0 {
            remote_file
                .seek(std::io::SeekFrom::Start(offset))
                .await
                .map_err(SftpError::IoError)?;
        }
        if let Some(parent) = Path::new(&job.local_path).parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(SftpError::IoError)?;
        }
        let mut local_file = if offset > 0 {
            tokio::fs::OpenOptions::new()
                .write(true)
                .open(&job.local_path)
                .await
                .map_err(SftpError::IoError)?
        } else {
            tokio::fs::File::create(&job.local_path)
                .await
                .map_err(SftpError::IoError)?
        };
        if offset > 0 {
            local_file
                .seek(std::io::SeekFrom::End(0))
                .await
                .map_err(SftpError::IoError)?;
        }
        let mut buffer = vec![0u8; AdaptiveChunkSizer::MAX_CHUNK];
        let mut chunk_sizer = AdaptiveChunkSizer::new();
        let started = Instant::now();
        let mut transferred = offset;
        let mut last_progress = Instant::now();
        loop {
            check_transfer_control(transfer_manager, transfer_id).await?;
            let chunk_size = chunk_sizer.chunk_size();
            let read = remote_file
                .read(&mut buffer[..chunk_size])
                .await
                .map_err(SftpError::IoError)?;
            if read == 0 {
                break;
            }
            local_file
                .write_all(&buffer[..read])
                .await
                .map_err(SftpError::IoError)?;
            transferred += read as u64;
            chunk_sizer.record(read);
            throttle_transfer(
                transferred.saturating_sub(offset),
                started,
                transfer_manager,
            )
            .await;
            if last_progress.elapsed().as_millis() >= 200 {
                stored_progress.update_progress(transferred);
                progress_store.save(&stored_progress).await?;
                send_transfer_progress(
                    progress_tx,
                    transfer_id,
                    &job.remote_path,
                    &job.local_path,
                    TransferDirection::Download,
                    job.total_bytes,
                    transferred,
                    started,
                    TransferState::InProgress,
                    None,
                )
                .await;
                last_progress = Instant::now();
            }
        }
        local_file.flush().await.map_err(SftpError::IoError)?;
        stored_progress.mark_completed();
        progress_store.save(&stored_progress).await?;
        send_transfer_progress(
            progress_tx,
            transfer_id,
            &job.remote_path,
            &job.local_path,
            TransferDirection::Download,
            job.total_bytes,
            transferred,
            started,
            TransferState::Completed,
            None,
        )
        .await;
        Ok(transferred)
    }

    async fn upload_file_resume_inner(
        &self,
        job: &UploadFileJob,
        transfer_id: &str,
        offset: u64,
        progress_tx: &Option<tokio::sync::mpsc::Sender<TransferProgress>>,
        transfer_manager: &Option<Arc<SftpTransferManager>>,
        progress_store: Arc<dyn ProgressStore>,
        mut stored_progress: StoredTransferProgress,
    ) -> Result<u64, SftpError> {
        let mut local_file = tokio::fs::File::open(&job.local_path)
            .await
            .map_err(SftpError::IoError)?;
        if offset > 0 {
            local_file
                .seek(std::io::SeekFrom::Start(offset))
                .await
                .map_err(SftpError::IoError)?;
        }
        let mut remote_file = if offset > 0 {
            self.sftp
                .open_with_flags(&job.remote_path, OpenFlags::WRITE | OpenFlags::APPEND)
                .await
                .map_err(|error| self.map_sftp_error(error, &job.remote_path))?
        } else {
            self.sftp
                .open_with_flags(
                    &job.remote_path,
                    OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE,
                )
                .await
                .map_err(|error| self.map_sftp_error(error, &job.remote_path))?
        };
        let mut buffer = vec![0u8; AdaptiveChunkSizer::MAX_CHUNK];
        let mut chunk_sizer = AdaptiveChunkSizer::new();
        let started = Instant::now();
        let mut transferred = offset;
        let mut last_progress = Instant::now();
        loop {
            check_transfer_control(transfer_manager, transfer_id).await?;
            let chunk_size = chunk_sizer.chunk_size();
            let read = local_file
                .read(&mut buffer[..chunk_size])
                .await
                .map_err(SftpError::IoError)?;
            if read == 0 {
                break;
            }
            remote_file
                .write_all(&buffer[..read])
                .await
                .map_err(SftpError::IoError)?;
            transferred += read as u64;
            chunk_sizer.record(read);
            throttle_transfer(
                transferred.saturating_sub(offset),
                started,
                transfer_manager,
            )
            .await;
            if last_progress.elapsed().as_millis() >= 200 {
                stored_progress.update_progress(transferred);
                progress_store.save(&stored_progress).await?;
                send_transfer_progress(
                    progress_tx,
                    transfer_id,
                    &job.remote_path,
                    &job.local_path,
                    TransferDirection::Upload,
                    job.total_bytes,
                    transferred,
                    started,
                    TransferState::InProgress,
                    None,
                )
                .await;
                last_progress = Instant::now();
            }
        }
        remote_file.flush().await.map_err(SftpError::IoError)?;
        stored_progress.mark_completed();
        progress_store.save(&stored_progress).await?;
        send_transfer_progress(
            progress_tx,
            transfer_id,
            &job.remote_path,
            &job.local_path,
            TransferDirection::Upload,
            job.total_bytes,
            transferred,
            started,
            TransferState::Completed,
            None,
        )
        .await;
        Ok(transferred)
    }

    async fn replace_remote_file(
        &self,
        source_path: &str,
        target_path: &str,
    ) -> Result<(), SftpError> {
        if let Err(error) = self.sftp.remove_file(target_path).await
            && !is_missing_file_error_message(&error.to_string())
        {
            return Err(self.map_sftp_error(error, target_path));
        }
        self.sftp
            .rename(source_path, target_path)
            .await
            .map_err(|error| self.map_sftp_error(error, target_path))
    }

    async fn preview_text(
        &self,
        path: &str,
        extension: &str,
        mime_type: &str,
        file_size: u64,
    ) -> Result<PreviewContent, SftpError> {
        if file_size > constants::MAX_TEXT_PREVIEW_SIZE {
            return Ok(PreviewContent::TooLarge {
                size: file_size,
                max_size: constants::MAX_TEXT_PREVIEW_SIZE,
                recommend_download: true,
            });
        }
        let content = self.read_file_limited(path, file_size as usize).await?;
        let (data, encoding, confidence, has_bom) = detect_and_decode(&content);
        Ok(PreviewContent::Text {
            data,
            mime_type: Some(mime_type.to_string()),
            language: extension_to_language(extension),
            encoding,
            confidence,
            has_bom,
        })
    }

    async fn preview_image(
        &self,
        path: &str,
        size: u64,
        mime_type: &str,
    ) -> Result<PreviewContent, SftpError> {
        if size > constants::MAX_PREVIEW_SIZE {
            return Ok(PreviewContent::TooLarge {
                size,
                max_size: constants::MAX_PREVIEW_SIZE,
                recommend_download: true,
            });
        }
        const INLINE_THRESHOLD: u64 = 512 * 1024;
        if size <= INLINE_THRESHOLD {
            use base64::Engine;
            let bytes = self.read_file_limited(path, size as usize).await?;
            return Ok(PreviewContent::Image {
                data: base64::engine::general_purpose::STANDARD.encode(bytes),
                mime_type: mime_type.to_string(),
            });
        }
        self.preview_asset(path, size, mime_type, AssetFileKind::Image)
            .await
    }

    async fn preview_asset(
        &self,
        path: &str,
        size: u64,
        mime_type: &str,
        kind: AssetFileKind,
    ) -> Result<PreviewContent, SftpError> {
        let max_size = match kind {
            AssetFileKind::Audio | AssetFileKind::Video => constants::MAX_MEDIA_PREVIEW_SIZE,
            AssetFileKind::Office => 50 * 1024 * 1024,
            AssetFileKind::Image | AssetFileKind::Pdf => constants::MAX_PREVIEW_SIZE,
        };
        if size > max_size {
            return Ok(PreviewContent::TooLarge {
                size,
                max_size,
                recommend_download: true,
            });
        }
        let path = self.download_to_temp(path).await?;
        Ok(PreviewContent::AssetFile {
            path: path.to_string_lossy().to_string(),
            mime_type: mime_type.to_string(),
            kind,
        })
    }

    async fn preview_hex(
        &self,
        path: &str,
        total_size: u64,
        offset: u64,
    ) -> Result<PreviewContent, SftpError> {
        if offset >= total_size {
            return Ok(PreviewContent::Hex {
                data: String::new(),
                total_size,
                offset,
                chunk_size: 0,
                has_more: false,
            });
        }
        let bytes_to_read = constants::HEX_CHUNK_SIZE.min(total_size - offset) as usize;
        let mut file = self
            .sftp
            .open(path)
            .await
            .map_err(|error| self.map_sftp_error(error, path))?;
        if offset > 0 {
            file.seek(std::io::SeekFrom::Start(offset))
                .await
                .map_err(SftpError::IoError)?;
        }
        let mut buffer = vec![0u8; bytes_to_read];
        let read = file.read(&mut buffer).await.map_err(SftpError::IoError)?;
        buffer.truncate(read);
        Ok(PreviewContent::Hex {
            data: generate_hex_dump(&buffer, offset),
            total_size,
            offset,
            chunk_size: read as u64,
            has_more: offset + (read as u64) < total_size,
        })
    }

    async fn read_file_limited(&self, path: &str, max_bytes: usize) -> Result<Vec<u8>, SftpError> {
        let mut file = self
            .sftp
            .open(path)
            .await
            .map_err(|error| self.map_sftp_error(error, path))?;
        let mut content =
            Vec::with_capacity(max_bytes.min(constants::STREAMING_PREVIEW_CHUNK_SIZE));
        let mut remaining = max_bytes;
        let mut buffer = vec![0u8; constants::STREAMING_PREVIEW_CHUNK_SIZE.min(max_bytes.max(1))];
        while remaining > 0 {
            let read_len = remaining.min(buffer.len());
            let read = file
                .read(&mut buffer[..read_len])
                .await
                .map_err(SftpError::IoError)?;
            if read == 0 {
                break;
            }
            content.extend_from_slice(&buffer[..read]);
            remaining -= read;
        }
        Ok(content)
    }

    async fn download_to_temp(&self, remote_path: &str) -> Result<PathBuf, SftpError> {
        let extension = Path::new(remote_path)
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("bin");
        let temp_dir = std::env::temp_dir().join("oxideterm-sftp-preview");
        tokio::fs::create_dir_all(&temp_dir)
            .await
            .map_err(SftpError::IoError)?;
        let temp_path = temp_dir.join(format!("{}.{}", uuid::Uuid::new_v4(), extension));
        let mut remote_file = self
            .sftp
            .open(remote_path)
            .await
            .map_err(|error| self.map_sftp_error(error, remote_path))?;
        let mut local_file = tokio::fs::File::create(&temp_path)
            .await
            .map_err(SftpError::IoError)?;
        let mut buffer = vec![0u8; constants::STREAMING_PREVIEW_CHUNK_SIZE];
        loop {
            let read = remote_file
                .read(&mut buffer)
                .await
                .map_err(SftpError::IoError)?;
            if read == 0 {
                break;
            }
            local_file
                .write_all(&buffer[..read])
                .await
                .map_err(SftpError::IoError)?;
        }
        local_file.flush().await.map_err(SftpError::IoError)?;
        std::fs::canonicalize(&temp_path).map_err(SftpError::IoError)
    }

    async fn write_to_swap_and_rename(
        &self,
        canonical_path: &str,
        swap_path: &str,
        content: &[u8],
    ) -> Result<(), SftpError> {
        let mut file = self
            .sftp
            .open_with_flags(
                swap_path,
                OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE,
            )
            .await
            .map_err(|error| self.map_sftp_error(error, swap_path))?;
        file.write_all(content).await.map_err(|error| {
            SftpError::WriteError(format!("Failed to write swap file: {error}"))
        })?;
        file.flush().await.map_err(|error| {
            SftpError::WriteError(format!("Failed to flush swap file: {error}"))
        })?;
        drop(file);
        if let Err(error) = self.sftp.remove_file(canonical_path).await
            && !is_missing_file_error_message(&error.to_string())
        {
            warn!("Failed to remove existing target before SFTP rename: {error}");
        }
        match self.sftp.rename(swap_path, canonical_path).await {
            Ok(()) => Ok(()),
            Err(error) => {
                let _ = self.sftp.remove_file(swap_path).await;
                Err(SftpError::WriteError(format!(
                    "Atomic rename failed: {error}"
                )))
            }
        }
    }

    async fn write_direct(&self, canonical_path: &str, content: &[u8]) -> Result<(), SftpError> {
        let mut file = self
            .sftp
            .open_with_flags(
                canonical_path,
                OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE,
            )
            .await
            .map_err(|error| self.map_sftp_error(error, canonical_path))?;
        file.write_all(content)
            .await
            .map_err(|error| SftpError::WriteError(format!("Failed to write content: {error}")))?;
        file.flush()
            .await
            .map_err(|error| SftpError::WriteError(format!("Failed to flush file: {error}")))?;
        Ok(())
    }

    async fn resolve_path(&self, path: &str) -> Result<String, SftpError> {
        if path.is_empty() {
            return Ok(self.cwd.clone());
        }
        let path_to_resolve = if is_absolute_remote_path(path) {
            path.to_string()
        } else {
            join_remote_path(&self.cwd, path)
        };
        self.sftp
            .canonicalize(&path_to_resolve)
            .await
            .map_err(|error| self.map_sftp_error(error, &path_to_resolve))
    }

    async fn resolve_new_file_path(&self, path: &str) -> Result<String, SftpError> {
        let (parent, filename) = if let Some((parent, filename)) = path.rsplit_once('/') {
            let parent = if parent.is_empty() { "/" } else { parent };
            (parent, filename)
        } else {
            (self.cwd.as_str(), path)
        };
        if filename.is_empty() {
            return Err(SftpError::InvalidPath(format!(
                "missing file name in path: {path}"
            )));
        }
        let canonical_parent = self.resolve_path(parent).await?;
        Ok(join_remote_path(&canonical_parent, filename))
    }

    fn map_sftp_error(&self, error: SftpErrorInner, path: &str) -> SftpError {
        let message = error.to_string();
        let lower = message.to_lowercase();
        if lower.contains("permission denied") {
            SftpError::PermissionDenied(path.to_string())
        } else if is_missing_file_error_message(&lower) {
            SftpError::FileNotFound(path.to_string())
        } else if lower.contains("no such directory") {
            SftpError::DirectoryNotFound(path.to_string())
        } else {
            SftpError::ProtocolError(message)
        }
    }
}

fn classify_list_entry_file_type(
    entry_file_type: FileType,
    target_file_type: Option<FileType>,
) -> FileType {
    match entry_file_type {
        FileType::Symlink => match target_file_type {
            Some(FileType::Directory) => FileType::Directory,
            _ => FileType::Symlink,
        },
        other => other,
    }
}

fn file_type_from_attrs(metadata: &FileAttributes) -> FileType {
    if metadata.is_dir() {
        FileType::Directory
    } else if metadata.is_symlink() {
        FileType::Symlink
    } else if metadata.is_regular() {
        FileType::File
    } else {
        FileType::Unknown
    }
}

fn sort_entries(entries: &mut [FileInfo], order: SortOrder) {
    entries.sort_by(|a, b| {
        let a_is_dir = a.file_type == FileType::Directory;
        let b_is_dir = b.file_type == FileType::Directory;
        if a_is_dir != b_is_dir {
            return b_is_dir.cmp(&a_is_dir);
        }
        match order {
            SortOrder::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            SortOrder::NameDesc => b.name.to_lowercase().cmp(&a.name.to_lowercase()),
            SortOrder::Size => a.size.cmp(&b.size),
            SortOrder::SizeDesc => b.size.cmp(&a.size),
            SortOrder::Modified => a.modified.cmp(&b.modified),
            SortOrder::ModifiedDesc => b.modified.cmp(&a.modified),
            SortOrder::Type => a.name.cmp(&b.name),
            SortOrder::TypeDesc => b.name.cmp(&a.name),
        }
    });
}

fn swap_path(canonical_path: &str) -> String {
    if let Some(slash_pos) = canonical_path.rfind('/') {
        let dir = &canonical_path[..=slash_pos];
        let name = &canonical_path[slash_pos + 1..];
        format!("{dir}.{name}.oxswp")
    } else {
        format!(".{canonical_path}.oxswp")
    }
}

async fn throttle_transfer(
    transferred: u64,
    started: Instant,
    transfer_manager: &Option<Arc<SftpTransferManager>>,
) {
    let Some(manager) = transfer_manager else {
        return;
    };
    let limit = manager.speed_limit_bps();
    if limit == 0 {
        return;
    }
    let elapsed = started.elapsed().as_secs_f64();
    let expected = transferred as f64 / limit as f64;
    if expected > elapsed {
        tokio::time::sleep(std::time::Duration::from_secs_f64(expected - elapsed)).await;
    }
}

async fn check_transfer_control(
    transfer_manager: &Option<Arc<SftpTransferManager>>,
    transfer_id: &str,
) -> Result<(), SftpError> {
    if let Some(manager) = transfer_manager {
        manager.check_control(transfer_id).await?;
    }
    Ok(())
}

async fn send_transfer_progress(
    progress_tx: &Option<tokio::sync::mpsc::Sender<TransferProgress>>,
    transfer_id: &str,
    remote_path: &str,
    local_path: &str,
    direction: TransferDirection,
    total_bytes: u64,
    transferred_bytes: u64,
    started: Instant,
    state: TransferState,
    error: Option<String>,
) {
    let Some(tx) = progress_tx else {
        return;
    };
    let elapsed = started.elapsed().as_secs_f64();
    let speed = if elapsed > 0.0 {
        (transferred_bytes as f64 / elapsed) as u64
    } else {
        0
    };
    let eta_seconds = if speed > 0 && total_bytes > transferred_bytes {
        Some(((total_bytes - transferred_bytes) as f64 / speed as f64) as u64)
    } else {
        None
    };
    let _ = tx
        .send(TransferProgress {
            id: transfer_id.to_string(),
            remote_path: remote_path.to_string(),
            local_path: local_path.to_string(),
            direction,
            state,
            total_bytes,
            transferred_bytes,
            speed,
            eta_seconds,
            error,
        })
        .await;
}

fn is_missing_file_error_message(message: &str) -> bool {
    let lower = message.to_lowercase();
    lower.contains("no such file")
        || lower.contains("not found")
        || lower.contains("does not exist")
}
