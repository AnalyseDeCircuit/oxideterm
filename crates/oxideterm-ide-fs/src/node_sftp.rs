// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{future::Future, pin::Pin, sync::Arc};

use futures_util::future::join_all;
use oxideterm_ide_core::{
    AsyncIdeFileSystem, FileKind, FileStat, FileSystemCapabilities, FileTreeEntry, IdeFileCheck,
    IdeFileData, IdeFileError, IdeFileErrorKind, IdeFsFuture, IdeLocation, IdePathStat,
    IdeProjectInfo, SavedFileVersion, WriteMode,
};
use oxideterm_sftp::{FileInfo, FileType, ListFilter, PreviewContent, SftpError, SftpSession};
use oxideterm_ssh::{NodeId, NodeRouter, RouteError};
use tokio::sync::Mutex;

type SharedSftp = Arc<Mutex<SftpSession>>;
type IdeOperationFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, IdeFileError>> + Send + 'a>>;

const MAX_EDITABLE_FILE_SIZE: u64 = 10 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct NodeSftpIdeFileSystem {
    router: NodeRouter,
}

impl NodeSftpIdeFileSystem {
    pub fn new(router: NodeRouter) -> Self {
        Self { router }
    }

    pub async fn open_project(
        &self,
        node_id: impl Into<String>,
        path: impl Into<String>,
    ) -> Result<IdeProjectInfo, IdeFileError> {
        let node_id = NodeId::new(node_id);
        let path = path.into();
        self.with_sftp_retry(&node_id, |sftp| {
            let path = path.clone();
            Box::pin(async move {
                let sftp = sftp.lock().await;
                let info = sftp.stat(&path).await.map_err(map_sftp_error)?;
                if info.file_type != FileType::Directory {
                    return Err(IdeFileError::new(
                        IdeFileErrorKind::Other,
                        "Path is not a directory",
                    ));
                }

                // Tauri's node_ide_open_project uses stat()'s canonical path so
                // IDE tabs survive cwd, symlink, and Windows OpenSSH separator
                // differences. Keep that as the project root persisted by IDE.
                let canonical_path = info.path.replace('\\', "/");
                let git_path = format!("{}/.git", canonical_path.trim_end_matches('/'));
                let is_git_repo = sftp.stat(&git_path).await.is_ok();
                let git_branch = if is_git_repo {
                    get_git_branch_inner(&sftp, &canonical_path).await.ok()
                } else {
                    None
                };
                let name = canonical_path
                    .rsplit('/')
                    .next()
                    .unwrap_or("project")
                    .to_string();

                Ok(IdeProjectInfo {
                    root_path: canonical_path,
                    name,
                    is_git_repo,
                    git_branch,
                    file_count: 0,
                })
            })
        })
        .await
    }

    pub async fn check_file(
        &self,
        node_id: impl Into<String>,
        path: impl Into<String>,
    ) -> Result<IdeFileCheck, IdeFileError> {
        let node_id = NodeId::new(node_id);
        let path = path.into();
        self.with_sftp_retry(&node_id, |sftp| {
            let path = path.clone();
            Box::pin(async move {
                let sftp = sftp.lock().await;
                let info = sftp.stat(&path).await.map_err(map_sftp_error)?;
                if info.file_type == FileType::Directory {
                    return Ok(IdeFileCheck::NotEditable {
                        reason: "Is a directory".to_string(),
                    });
                }
                if info.size > MAX_EDITABLE_FILE_SIZE {
                    return Ok(IdeFileCheck::TooLarge {
                        size: info.size,
                        limit: MAX_EDITABLE_FILE_SIZE,
                    });
                }

                match sftp.preview(&path).await.map_err(map_sftp_error)? {
                    PreviewContent::Text { .. } => Ok(IdeFileCheck::Editable {
                        size: info.size,
                        mtime: info.modified.max(0) as u64,
                    }),
                    PreviewContent::TooLarge { size, max_size, .. } => Ok(IdeFileCheck::TooLarge {
                        size,
                        limit: max_size,
                    }),
                    PreviewContent::Hex { .. } => Ok(IdeFileCheck::Binary),
                    _ => Ok(IdeFileCheck::NotEditable {
                        reason: "Unsupported file type".to_string(),
                    }),
                }
            })
        })
        .await
    }

    pub async fn batch_stat(
        &self,
        node_id: impl Into<String>,
        paths: Vec<String>,
    ) -> Result<Vec<Option<IdePathStat>>, IdeFileError> {
        let node_id = NodeId::new(node_id);
        self.with_sftp_retry(&node_id, |sftp| {
            let paths = paths.clone();
            Box::pin(async move {
                let sftp = sftp.lock().await;
                let stats = join_all(paths.iter().map(|path| sftp.stat(path))).await;
                Ok(stats
                    .into_iter()
                    .map(|stat| {
                        stat.ok().map(|info| IdePathStat {
                            size: info.size,
                            mtime: info.modified.max(0) as u64,
                            is_dir: info.file_type == FileType::Directory,
                        })
                    })
                    .collect())
            })
        })
        .await
    }

    pub async fn delete_item(
        &self,
        node_id: impl Into<String>,
        path: impl Into<String>,
        recursive: bool,
    ) -> Result<(), IdeFileError> {
        let node_id = NodeId::new(node_id);
        let path = path.into();
        self.with_sftp_retry(&node_id, |sftp| {
            let path = path.clone();
            Box::pin(async move {
                let sftp = sftp.lock().await;
                if recursive {
                    sftp.delete_recursive(&path).await.map(|_| ())
                } else {
                    sftp.delete(&path).await
                }
                .map_err(map_sftp_error)
            })
        })
        .await
    }

    pub async fn create_file(
        &self,
        node_id: impl Into<String>,
        path: impl Into<String>,
    ) -> Result<SavedFileVersion, IdeFileError> {
        let location = IdeLocation::remote(node_id.into(), path.into());
        self.write_file(&location, "", None, WriteMode::CreateNew)
            .await
    }

    pub async fn create_folder(
        &self,
        node_id: impl Into<String>,
        path: impl Into<String>,
    ) -> Result<(), IdeFileError> {
        let node_id = NodeId::new(node_id);
        let path = path.into();
        self.with_sftp_retry(&node_id, |sftp| {
            let path = path.clone();
            Box::pin(async move {
                let sftp = sftp.lock().await;
                sftp.mkdir(&path).await.map_err(map_sftp_error)
            })
        })
        .await
    }

    pub async fn rename_item(
        &self,
        node_id: impl Into<String>,
        old_path: impl Into<String>,
        new_path: impl Into<String>,
    ) -> Result<(), IdeFileError> {
        let node_id = NodeId::new(node_id);
        let old_path = old_path.into();
        let new_path = new_path.into();
        self.with_sftp_retry(&node_id, |sftp| {
            let old_path = old_path.clone();
            let new_path = new_path.clone();
            Box::pin(async move {
                let sftp = sftp.lock().await;
                if sftp.stat(&new_path).await.is_ok() {
                    return Err(IdeFileError::new(
                        IdeFileErrorKind::Conflict,
                        "ide.error.alreadyExists",
                    ));
                }
                sftp.rename(&old_path, &new_path)
                    .await
                    .map_err(map_sftp_error)
            })
        })
        .await
    }

    async fn with_sftp_retry<T, F>(&self, node_id: &NodeId, operation: F) -> Result<T, IdeFileError>
    where
        F: for<'a> Fn(&'a SharedSftp) -> IdeOperationFuture<'a, T>,
    {
        let sftp = self.acquire_sftp(node_id).await?;
        match operation(&sftp).await {
            Ok(value) => Ok(value),
            Err(error) if error.kind == IdeFileErrorKind::Disconnected => {
                // This is the IDE equivalent of Tauri's sftp_with_retry! macro:
                // the shared SFTP owner belongs to NodeRouter, and stale channel
                // failures are silently rebuilt once before the UI sees an error.
                let sftp = self.rebuild_sftp(node_id).await?;
                operation(&sftp).await
            }
            Err(error) => Err(error),
        }
    }

    async fn acquire_sftp(&self, node_id: &NodeId) -> Result<SharedSftp, IdeFileError> {
        self.router
            .acquire_sftp(node_id)
            .await
            .map_err(map_route_error)
    }

    async fn rebuild_sftp(&self, node_id: &NodeId) -> Result<SharedSftp, IdeFileError> {
        self.router
            .invalidate_and_reacquire_sftp(node_id)
            .await
            .map_err(map_route_error)
    }

    async fn write_file_once(
        &self,
        sftp: &SharedSftp,
        path: &str,
        text: &str,
        expected_version: Option<&SavedFileVersion>,
        mode: WriteMode,
    ) -> Result<SavedFileVersion, IdeFileError> {
        let sftp = sftp.lock().await;
        if mode == WriteMode::CreateNew && sftp.stat(path).await.is_ok() {
            return Err(IdeFileError::new(
                IdeFileErrorKind::Conflict,
                "File already exists",
            ));
        }
        if let Some(expected) = expected_version
            && let Ok(current_info) = sftp.stat(path).await
        {
            let current = version_from_remote(&current_info);
            if remote_versions_conflict(expected, &current) {
                return Err(IdeFileError::new(
                    IdeFileErrorKind::Conflict,
                    "Remote file changed",
                ));
            }
        }

        // Mirrors Tauri SFTP save fallback: write_content first attempts an
        // atomic swap+rename and internally falls back to direct overwrite when
        // the server rejects the swap path.
        sftp.write_content(path, text.as_bytes())
            .await
            .map_err(map_sftp_error)?;
        let info = sftp.stat(path).await.map_err(map_sftp_error)?;
        Ok(version_from_remote(&info))
    }
}

impl AsyncIdeFileSystem for NodeSftpIdeFileSystem {
    fn capabilities(&self) -> FileSystemCapabilities {
        FileSystemCapabilities {
            atomic_write: true,
            directory_listing: true,
            conflict_detection: true,
        }
    }

    fn read_file<'a>(&'a self, location: &'a IdeLocation) -> IdeFsFuture<'a, IdeFileData> {
        Box::pin(async move {
            let (node_id, path) = remote_location(location)?;
            self.with_sftp_retry(&node_id, |sftp| {
                let path = path.clone();
                Box::pin(async move {
                    let sftp = sftp.lock().await;
                    let info = sftp.stat(&path).await.map_err(map_sftp_error)?;
                    match sftp.preview(&path).await.map_err(map_sftp_error)? {
                        PreviewContent::Text { data, .. } => Ok(IdeFileData {
                            text: data,
                            version: version_from_remote(&info),
                        }),
                        PreviewContent::TooLarge { size, max_size, .. } => Err(IdeFileError::new(
                            IdeFileErrorKind::Unsupported,
                            format!("File is too large to edit ({size} > {max_size})"),
                        )),
                        PreviewContent::Hex { .. } => Err(IdeFileError::new(
                            IdeFileErrorKind::Unsupported,
                            "File is binary",
                        )),
                        _ => Err(IdeFileError::new(
                            IdeFileErrorKind::Unsupported,
                            "Unsupported file type",
                        )),
                    }
                })
            })
            .await
        })
    }

    fn stat<'a>(&'a self, location: &'a IdeLocation) -> IdeFsFuture<'a, FileStat> {
        Box::pin(async move {
            let (node_id, path) = remote_location(location)?;
            self.with_sftp_retry(&node_id, |sftp| {
                let path = path.clone();
                Box::pin(async move {
                    let sftp = sftp.lock().await;
                    let info = sftp.stat(&path).await.map_err(map_sftp_error)?;
                    Ok(FileStat {
                        version: version_from_remote(&info),
                        is_read_only: is_read_only(&info),
                    })
                })
            })
            .await
        })
    }

    fn list_dir<'a>(&'a self, location: &'a IdeLocation) -> IdeFsFuture<'a, Vec<FileTreeEntry>> {
        Box::pin(async move {
            let (node_id, path) = remote_location(location)?;
            self.with_sftp_retry(&node_id, |sftp| {
                let node_id = node_id.clone();
                let path = path.clone();
                Box::pin(async move {
                    let sftp = sftp.lock().await;
                    let entries = sftp
                        .list_dir(
                            &path,
                            Some(ListFilter {
                                show_hidden: true,
                                pattern: None,
                                sort: oxideterm_sftp::SortOrder::Name,
                            }),
                        )
                        .await
                        .map_err(map_sftp_error)?;
                    Ok(entries
                        .into_iter()
                        .map(|entry| file_tree_entry(&node_id, entry))
                        .collect())
                })
            })
            .await
        })
    }

    fn write_file<'a>(
        &'a self,
        location: &'a IdeLocation,
        text: &'a str,
        expected_version: Option<&'a SavedFileVersion>,
        mode: WriteMode,
    ) -> IdeFsFuture<'a, SavedFileVersion> {
        Box::pin(async move {
            let (node_id, path) = remote_location(location)?;
            let sftp = self.acquire_sftp(&node_id).await?;
            match self
                .write_file_once(&sftp, &path, text, expected_version, mode)
                .await
            {
                Ok(version) => Ok(version),
                Err(error) if error.kind == IdeFileErrorKind::Disconnected => {
                    let sftp = self.rebuild_sftp(&node_id).await?;
                    self.write_file_once(&sftp, &path, text, expected_version, mode)
                        .await
                }
                Err(error) => Err(error),
            }
        })
    }
}

fn remote_location(location: &IdeLocation) -> Result<(NodeId, String), IdeFileError> {
    match location {
        IdeLocation::Remote { node_id, path } => Ok((NodeId::new(node_id.clone()), path.clone())),
        IdeLocation::Local { .. } => Err(IdeFileError::new(
            IdeFileErrorKind::Unsupported,
            "Node SFTP IDE filesystem cannot read local locations",
        )),
    }
}

async fn get_git_branch_inner(
    sftp: &SftpSession,
    project_path: &str,
) -> Result<String, IdeFileError> {
    let head_path = format!("{}/.git/HEAD", project_path);
    let preview = sftp.preview(&head_path).await.map_err(map_sftp_error)?;
    let PreviewContent::Text { data, .. } = preview else {
        return Err(IdeFileError::new(
            IdeFileErrorKind::Unsupported,
            "HEAD is not a text file",
        ));
    };
    if let Some(branch) = data.strip_prefix("ref: refs/heads/") {
        Ok(branch.trim().to_string())
    } else {
        Ok(data.chars().take(7).collect())
    }
}

fn file_tree_entry(node_id: &NodeId, entry: FileInfo) -> FileTreeEntry {
    let version = version_from_remote(&entry);
    FileTreeEntry {
        location: IdeLocation::remote(node_id.0.clone(), entry.path.clone()),
        kind: match entry.file_type {
            FileType::File => FileKind::File,
            FileType::Directory => FileKind::Directory,
            FileType::Symlink => FileKind::Symlink,
            FileType::Unknown => FileKind::Other,
        },
        name: entry.name,
        version,
    }
}

fn version_from_remote(info: &FileInfo) -> SavedFileVersion {
    SavedFileVersion {
        size_bytes: Some(info.size),
        modified_millis: (info.modified > 0).then_some(info.modified * 1000),
        etag: None,
    }
}

fn is_read_only(info: &FileInfo) -> bool {
    u32::from_str_radix(&info.permissions, 8)
        .map(|mode| mode & 0o200 == 0)
        .unwrap_or(false)
}

fn remote_versions_conflict(expected: &SavedFileVersion, current: &SavedFileVersion) -> bool {
    // Tauri RemoteFileEditor compares the saved server mtime before writing.
    // Size is carried in snapshots, but mtime is the observable conflict gate.
    expected.modified_millis.is_some()
        && current.modified_millis.is_some()
        && expected.modified_millis != current.modified_millis
}

pub(crate) fn map_route_error(error: RouteError) -> IdeFileError {
    let message = error.to_string();
    let kind = match error {
        RouteError::ConnectionTimeout(_) => IdeFileErrorKind::Timeout,
        RouteError::NotConnected(_) | RouteError::ParentNotConnected(_) => {
            IdeFileErrorKind::Disconnected
        }
        RouteError::CapabilityUnavailable(_)
            if is_network_error_message(&message) || is_session_dead_error_message(&message) =>
        {
            IdeFileErrorKind::Disconnected
        }
        RouteError::CapabilityUnavailable(_) => IdeFileErrorKind::Unsupported,
        RouteError::NodeNotFound(_) => IdeFileErrorKind::NotFound,
        RouteError::ConnectionError(_) if is_network_error_message(&message) => {
            IdeFileErrorKind::Disconnected
        }
        RouteError::ConnectionError(_) | RouteError::MaxDepthExceeded(_) => IdeFileErrorKind::Other,
    };
    IdeFileError::new(kind, message)
}

fn map_sftp_error(error: SftpError) -> IdeFileError {
    let recoverable = error.is_channel_recoverable();
    let message = error.to_string();
    let kind = match &error {
        SftpError::PermissionDenied(_) => IdeFileErrorKind::PermissionDenied,
        SftpError::FileNotFound(_) | SftpError::DirectoryNotFound(_) => IdeFileErrorKind::NotFound,
        SftpError::IoError(io_error) if io_error.kind() == std::io::ErrorKind::TimedOut => {
            IdeFileErrorKind::Timeout
        }
        SftpError::IoError(_) if is_network_error_message(&message) => {
            IdeFileErrorKind::Disconnected
        }
        SftpError::ChannelError(_)
        | SftpError::ProtocolError(_)
        | SftpError::SubsystemNotAvailable(_)
            if is_network_error_message(&message) || recoverable =>
        {
            IdeFileErrorKind::Disconnected
        }
        SftpError::SubsystemNotAvailable(_) => IdeFileErrorKind::Unsupported,
        SftpError::InvalidPath(_) => IdeFileErrorKind::NotFound,
        SftpError::TransferCancelled => IdeFileErrorKind::Other,
        SftpError::TransferInterrupted(_) => IdeFileErrorKind::Disconnected,
        SftpError::NotInitialized(_) => IdeFileErrorKind::Disconnected,
        SftpError::TransferError(_) | SftpError::WriteError(_) | SftpError::StorageError(_) => {
            if is_network_error_message(&message) {
                IdeFileErrorKind::Disconnected
            } else {
                IdeFileErrorKind::Other
            }
        }
        SftpError::IoError(_) => IdeFileErrorKind::Other,
        SftpError::ChannelError(_) | SftpError::ProtocolError(_) => IdeFileErrorKind::Other,
    };
    IdeFileError::new(kind, message)
}

fn is_network_error_message(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    [
        "network",
        "connection",
        "timeout",
        "disconnected",
        "eof",
        "broken pipe",
        "reset by peer",
        "channel closed",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

fn is_session_dead_error_message(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    [
        "session not found",
        "not initialized",
        "no active ssh connection",
        "transport is closed",
        "transport is missing",
        "stale",
        "link_down",
        "link down",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_version_conflict_uses_mtime_like_tauri() {
        let expected = SavedFileVersion {
            size_bytes: Some(4),
            modified_millis: Some(10_000),
            etag: None,
        };
        let same_mtime_different_size = SavedFileVersion {
            size_bytes: Some(8),
            modified_millis: Some(10_000),
            etag: None,
        };
        let different_mtime = SavedFileVersion {
            size_bytes: Some(4),
            modified_millis: Some(11_000),
            etag: None,
        };

        assert!(!remote_versions_conflict(
            &expected,
            &same_mtime_different_size
        ));
        assert!(remote_versions_conflict(&expected, &different_mtime));
    }

    #[test]
    fn maps_channel_closed_to_disconnected() {
        let error = map_sftp_error(SftpError::ChannelError("channel closed".into()));
        assert_eq!(error.kind, IdeFileErrorKind::Disconnected);
    }

    #[test]
    fn maps_sftp_session_unavailable_route_to_disconnected() {
        let error = map_route_error(RouteError::CapabilityUnavailable(
            "Session not found: node-1".to_string(),
        ));
        assert_eq!(error.kind, IdeFileErrorKind::Disconnected);

        let error = map_route_error(RouteError::CapabilityUnavailable(
            "SFTP session not initialized for: node-1".to_string(),
        ));
        assert_eq!(error.kind, IdeFileErrorKind::Disconnected);
    }
}
