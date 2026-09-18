use super::*;
use oxideterm_ftp::{ConnectOptions, FtpSession};
use tokio_util::sync::CancellationToken;

impl WorkspaceApp {
    pub(super) fn transfer_protocol_for_remote(
        &self,
        remote_id: &SftpRemoteId,
    ) -> RemoteTransferProtocol {
        if let SftpRemoteId::Ftp(id) = remote_id {
            return if self.ftp_sessions.get(id).is_some_and(|runtime| {
                runtime.options.security == oxideterm_ftp::Security::ExplicitTls
            }) {
                RemoteTransferProtocol::Ftps
            } else {
                RemoteTransferProtocol::Ftp
            };
        }
        configured_transfer_protocol(self.settings_store.settings().sftp.transfer_protocol)
    }
}

pub(in crate::workspace) struct FtpRuntime {
    pub options: Arc<ConnectOptions>,
    pub cancel: CancellationToken,
    browse: tokio::sync::Mutex<Option<FtpSession>>,
}

impl FtpRuntime {
    pub async fn preview(&self, path: &str, offset: u64) -> Result<PreviewContent, String> {
        use base64::Engine;
        let mut session = self.transfer().await?;
        let limit = 10 * 1024 * 1024;
        let bytes = match session.read(path, limit, &self.cancel).await {
            Ok(bytes) => bytes,
            Err(oxideterm_ftp::Error::TooLarge) => {
                return Ok(PreviewContent::TooLarge {
                    size: limit as u64 + 1,
                    max_size: limit as u64,
                    recommend_download: true,
                });
            }
            Err(error) => return Err(error.to_string()),
        };
        let extension = Path::new(path)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if matches!(
            extension.as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp"
        ) {
            return Ok(PreviewContent::Image {
                data: base64::engine::general_purpose::STANDARD.encode(bytes),
                mime_type: format!(
                    "image/{}",
                    if extension == "jpg" {
                        "jpeg"
                    } else {
                        &extension
                    }
                ),
            });
        }
        if oxideterm_preview::is_likely_text_content(&bytes) {
            let (data, encoding, confidence, has_bom) =
                oxideterm_preview::detect_and_decode(&bytes);
            return Ok(PreviewContent::Text {
                data,
                mime_type: Some("text/plain".into()),
                language: oxideterm_preview::extension_to_language(&extension),
                encoding,
                confidence,
                has_bom,
            });
        }
        let start = usize::try_from(offset)
            .unwrap_or(usize::MAX)
            .min(bytes.len());
        let end = (start + 64 * 1024).min(bytes.len());
        Ok(PreviewContent::Hex {
            data: oxideterm_preview::generate_hex_dump(&bytes[start..end], start as u64),
            total_size: bytes.len() as u64,
            offset: start as u64,
            chunk_size: (end - start) as u64,
            has_more: end < bytes.len(),
        })
    }

    pub async fn save(
        &self,
        path: &str,
        bytes: &[u8],
        encoding: &str,
    ) -> Result<SftpPreviewSaveResult, String> {
        let mut session = self.transfer().await?;
        let result = session.write(path, bytes, &self.cancel).await;
        if result.is_err() {
            self.cleanup_upload(&mut session).await;
        }
        result.map_err(|error| error.to_string())?;
        Ok(SftpPreviewSaveResult {
            mtime: None,
            size: Some(bytes.len() as u64),
            encoding_used: encoding.to_owned(),
            atomic_write: false,
        })
    }
    pub async fn connect(
        options: ConnectOptions,
        attempt: CancellationToken,
    ) -> Result<Self, String> {
        let cancel = CancellationToken::new();
        let session = FtpSession::connect(&options, &attempt)
            .await
            .map_err(|error| error.to_string())?;
        Ok(Self {
            options: Arc::new(options),
            cancel,
            browse: tokio::sync::Mutex::new(Some(session)),
        })
    }

    async fn browsing(&self) -> Result<tokio::sync::MappedMutexGuard<'_, FtpSession>, String> {
        let mut slot = self.browse.lock().await;
        if !slot.as_ref().is_some_and(FtpSession::is_reusable) {
            *slot = Some(
                FtpSession::connect(&self.options, &self.cancel)
                    .await
                    .map_err(|error| error.to_string())?,
            );
        }
        Ok(tokio::sync::MutexGuard::map(slot, |slot| {
            slot.as_mut().expect("connected FTP session")
        }))
    }

    pub async fn transfer(&self) -> Result<FtpSession, String> {
        FtpSession::connect(&self.options, &self.cancel)
            .await
            .map_err(|error| error.to_string())
    }

    async fn cleanup_upload(&self, session: &mut FtpSession) {
        let Some(path) = session.take_pending_upload() else {
            return;
        };
        // A cancelled transfer socket may have an unread final reply. Cleanup
        // owns a separate, bounded connection and touches only its staging file.
        let _ = tokio::time::timeout(Duration::from_secs(5), async {
            let cancel = CancellationToken::new();
            let mut cleanup = FtpSession::connect(&self.options, &cancel).await?;
            cleanup.delete(&path, false, &cancel).await
        })
        .await;
    }

    pub async fn listing(&self, path: &str) -> Result<RemoteSftpListing, String> {
        let mut session = self.browsing().await?;
        let path = if path.is_empty() {
            session.home().to_owned()
        } else {
            path.to_owned()
        };
        let entries = session
            .list(&path, &self.cancel)
            .await
            .map_err(|error| error.to_string())?;
        let files = entries
            .into_iter()
            .map(|entry| {
                let is_dir = entry.kind == oxideterm_ftp::EntryKind::Directory;
                SftpFileEntry {
                    path: format!("{}/{}", path.trim_end_matches('/'), entry.name),
                    name: entry.name,
                    file_type: if is_dir {
                        SftpFileType::Directory
                    } else {
                        SftpFileType::File
                    },
                    size: entry.size.unwrap_or(0),
                    size_known: entry.size.is_some(),
                    modified: entry
                        .modified
                        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|duration| duration.as_secs() as i64),
                    permissions: entry.permissions.map(|mode| format!("{mode:04o}")),
                    owner: None,
                    group: None,
                    is_symlink: entry.kind == oxideterm_ftp::EntryKind::Symlink,
                    symlink_target: None,
                }
            })
            .collect();
        Ok(RemoteSftpListing { cwd: path, files })
    }
}

pub(super) enum MutationSession {
    Sftp(SftpSession),
    Ftp {
        session: tokio::sync::Mutex<FtpSession>,
        cancel: CancellationToken,
    },
}

impl SftpRemoteBackend {
    pub(super) async fn mutation_session(&self) -> Result<MutationSession, String> {
        match self {
            Self::Ftp { runtime } => Ok(MutationSession::Ftp {
                session: tokio::sync::Mutex::new(runtime.transfer().await?),
                cancel: runtime.cancel.clone(),
            }),
            _ => self
                .acquire_transfer_sftp()
                .await
                .map(MutationSession::Sftp),
        }
    }
}

impl MutationSession {
    pub(super) async fn mkdir(&self, path: &str) -> Result<(), String> {
        match self {
            Self::Sftp(session) => session.mkdir(path).await.map_err(|e| e.to_string()),
            Self::Ftp { session, cancel } => session
                .lock()
                .await
                .mkdir(path, cancel)
                .await
                .map_err(|e| e.to_string()),
        }
    }
    pub(super) async fn rename(&self, from: &str, to: &str) -> Result<(), String> {
        match self {
            Self::Sftp(session) => session.rename(from, to).await.map_err(|e| e.to_string()),
            Self::Ftp { session, cancel } => session
                .lock()
                .await
                .rename(from, to, cancel)
                .await
                .map_err(|e| e.to_string()),
        }
    }
    pub(super) async fn delete_recursive(&self, path: &str) -> Result<u64, String> {
        match self {
            Self::Sftp(session) => session
                .delete_recursive(path)
                .await
                .map_err(|e| e.to_string()),
            Self::Ftp { session, cancel } => session
                .lock()
                .await
                .delete_recursive(path, cancel)
                .await
                .map_err(|e| e.to_string()),
        }
    }
}

impl Drop for FtpRuntime {
    fn drop(&mut self) {
        // Transfers retain the runtime independently from the visible tab.
        self.cancel.cancel();
    }
}

pub(super) async fn run_transfer(
    runtime: &FtpRuntime,
    manager: &Arc<oxideterm_sftp::SftpTransferManager>,
    transfer_id: &str,
    id: u64,
    direction: SftpTransferDirection,
    directory: bool,
    local: &str,
    remote: &str,
    disposition: LocalDownloadDisposition,
    tx: &delivery::ActiveDeliverySender<SftpWorkerResult>,
    owner: &str,
) -> Result<(), String> {
    let protocol = if runtime.options.security == oxideterm_ftp::Security::ExplicitTls {
        RemoteTransferProtocol::Ftps
    } else {
        RemoteTransferProtocol::Ftp
    };
    let _ = tx.send(SftpWorkerResult::TransferProtocolResolved { id, protocol });
    let control = manager
        .get_control(transfer_id)
        .ok_or_else(|| "FTP transfer is no longer registered".to_owned())?;
    let token = runtime.cancel.child_token();
    let mut cancellation = control.subscribe_cancellation();
    let mut background = BackgroundTransferSnapshot::new(
        transfer_id.to_owned(),
        owner.to_owned(),
        Path::new(remote)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(remote)
            .to_owned(),
        local.to_owned(),
        remote.to_owned(),
        if direction == SftpTransferDirection::Upload {
            BackgroundTransferDirection::Upload
        } else {
            BackgroundTransferDirection::Download
        },
        if directory {
            BackgroundTransferKind::Directory
        } else {
            BackgroundTransferKind::File
        },
        if directory {
            RemoteTransferStrategy::DirectoryRecursive
        } else {
            RemoteTransferStrategy::File
        },
        0,
        0,
    );
    background.protocol = protocol;
    manager.register_background_transfer(background);
    let started = Instant::now();
    let last_delivery = parking_lot::Mutex::new(started);
    let last_delivery = &last_delivery;
    let progress = |progress: oxideterm_ftp::TransferProgress| async move {
        manager
            .check_control(transfer_id)
            .await
            .map_err(|_| oxideterm_ftp::Error::Cancelled)?;
        let limit = manager.speed_limit_bps();
        if limit > 0 {
            let expected = Duration::from_nanos(
                (u128::from(progress.completed) * 1_000_000_000 / limit as u128)
                    .min(u128::from(u64::MAX)) as u64,
            );
            if let Some(wait) = expected.checked_sub(started.elapsed()) {
                tokio::time::sleep(wait).await;
            }
        }
        let mut last = last_delivery.lock();
        if last.elapsed() >= Duration::from_millis(50) || progress.total == Some(progress.completed)
        {
            *last = Instant::now();
            let speed =
                (progress.completed as f64 / started.elapsed().as_secs_f64().max(0.001)) as u64;
            manager.update_background_transfer_progress(
                transfer_id,
                progress.completed,
                progress.total.unwrap_or(0),
                speed,
            );
            let _ = tx.send(SftpWorkerResult::TransferProgress {
                id,
                transferred: progress.completed,
                total: progress.total.unwrap_or(0),
                speed,
            });
        }
        Ok(())
    };
    let mut session = None;
    let result = {
        let operation = async {
            if direction == SftpTransferDirection::Download
                && !directory
                && disposition == LocalDownloadDisposition::CreateNew
                && tokio::fs::try_exists(local).await?
            {
                return Err(oxideterm_ftp::Error::Io(std::io::Error::from(
                    std::io::ErrorKind::AlreadyExists,
                )));
            }
            if disposition == LocalDownloadDisposition::ResumeVerified {
                return Err(oxideterm_ftp::Error::InvalidInput);
            }
            session = Some(FtpSession::connect(&runtime.options, &token).await?);
            let session = session.as_mut().expect("connected FTP transfer");
            match (direction, directory) {
                (SftpTransferDirection::Upload, false) => {
                    session
                        .upload(Path::new(local), remote, &token, progress)
                        .await
                }
                (SftpTransferDirection::Download, false) => {
                    session
                        .download(
                            remote,
                            Path::new(local),
                            disposition != LocalDownloadDisposition::CreateNew,
                            &token,
                            progress,
                        )
                        .await
                }
                (SftpTransferDirection::Upload, true) => {
                    session
                        .upload_directory(Path::new(local), remote, &token, progress)
                        .await
                }
                (SftpTransferDirection::Download, true) => {
                    session
                        .download_directory(remote, Path::new(local), &token, progress)
                        .await
                }
            }
        };
        tokio::pin!(operation);
        let interrupted = async {
            loop {
                if *cancellation.borrow_and_update() {
                    return;
                }
                if cancellation.changed().await.is_err() {
                    return;
                }
            }
        };
        tokio::select! {
            result=&mut operation=>result,
            _=interrupted=>{token.cancel();operation.await},
        }
    };
    if result.is_err()
        && let Some(session) = &mut session
    {
        runtime.cleanup_upload(session).await;
    }
    let (state, error) = match &result {
        Ok(bytes) => {
            manager.update_background_transfer_progress(transfer_id, *bytes, *bytes, 0);
            let _ = tx.send(SftpWorkerResult::TransferProgress {
                id,
                transferred: *bytes,
                total: *bytes,
                speed: 0,
            });
            (BackgroundTransferState::Completed, None)
        }
        Err(oxideterm_ftp::Error::Cancelled) => (BackgroundTransferState::Cancelled, None),
        Err(error) => (BackgroundTransferState::Error, Some(error.to_string())),
    };
    manager.finish_background_transfer(transfer_id, state, error, None);
    result.map(|_| ()).map_err(|error| error.to_string())
}
