// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::node_sftp::{map_route_error, map_sftp_error};
use oxideterm_sftp::{SftpTransferGuard, SftpTransferManager, TransferDirection};
use serde_json::{Value, json};
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use zeroize::Zeroizing;

pub(super) const CHUNK: usize = 32 * 1024;

fn error(message: impl Into<String>) -> IdeFileError {
    IdeFileError::new(IdeFileErrorKind::Other, message)
}

fn io_error(cause: std::io::Error) -> IdeFileError {
    error(cause.to_string())
}

fn check_cancelled(cancel: &watch::Receiver<bool>) -> Result<(), IdeFileError> {
    if *cancel.borrow() || cancel.has_changed().is_err() {
        return Err(cancelled());
    }
    Ok(())
}

fn cancelled() -> IdeFileError {
    IdeFileError::new(IdeFileErrorKind::Cancelled, "Transfer cancelled")
}

impl NodeAgentIdeFileSystem {
    pub async fn transfer_file(
        &self,
        location: IdeLocation,
        local: PathBuf,
        direction: TransferDirection,
        overwrite: bool,
        mut cancel: watch::Receiver<bool>,
        progress: watch::Sender<(u64, u64)>,
    ) -> Result<(), IdeFileError> {
        let (node, path) = remote_location(&location)?;
        let audit_context = self.file_audit_context(&node);
        let audit = oxideterm_audit::AuditOperation::in_context(
            audit_context.as_ref(),
            oxideterm_audit::AuditCategory::File,
            "file_transfer",
            Some(&path),
        );
        let result = self
            .transfer_to_path(
                &node,
                &path,
                &local,
                direction,
                overwrite,
                &mut cancel,
                progress,
            )
            .await;
        use oxideterm_audit::{AuditEvidence, AuditOutcome};
        let outcome = match &result {
            Ok(()) => AuditOutcome::Succeeded,
            Err(e) if e.kind == IdeFileErrorKind::Cancelled => AuditOutcome::Cancelled,
            Err(e) if e.kind == IdeFileErrorKind::Disconnected => AuditOutcome::Interrupted,
            Err(_) => AuditOutcome::Failed,
        };
        audit.finish(outcome, AuditEvidence::Protocol, None, None);
        result
    }

    async fn transfer_to_path(
        &self,
        node: &NodeId,
        path: &str,
        local: &Path,
        direction: TransferDirection,
        overwrite: bool,
        cancel: &mut watch::Receiver<bool>,
        progress: watch::Sender<(u64, u64)>,
    ) -> Result<(), IdeFileError> {
        check_cancelled(cancel)?;
        self.ensure_ide_session_for_node(node).await?;
        let session = self.agent_session(node).await?;
        if session
            .as_ref()
            .is_some_and(|session| !session.supports_capability("single-file-transfer"))
        {
            return Err(IdeFileError::new(
                IdeFileErrorKind::Unsupported,
                "Agent update required",
            ));
        }
        check_cancelled(cancel)?;
        let download = direction == TransferDirection::Download;
        // A private sibling directory lets the native SFTP downloader open with CREATE_NEW.
        let temp = if download {
            let parent = local
                .parent()
                .ok_or_else(|| error("Invalid download destination"))?;
            let mut builder = tempfile::Builder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                builder.permissions(std::fs::Permissions::from_mode(0o700));
            }
            Some(
                builder
                    .prefix(".oxideterm-")
                    .tempdir_in(parent)
                    .map_err(io_error)?,
            )
        } else {
            None
        };
        let staging = temp.as_ref().map(|dir| dir.path().join("file"));
        let file_path = staging.as_deref().unwrap_or(local);
        let id = uuid::Uuid::new_v4().simple().to_string();
        if let Some(session) = session {
            if download {
                session
                    .download_single(path, file_path, cancel, &progress)
                    .await?;
            } else {
                session
                    .upload_single(path, file_path, &id, overwrite, cancel, &progress)
                    .await?;
            }
        } else {
            let routed = tokio::select! {
                _ = cancel.changed() => return Err(cancelled()),
                result = self.router.acquire_sftp(node) => result.map_err(map_route_error)?,
            };
            let sftp = tokio::select! {
                _ = cancel.changed() => return Err(cancelled()),
                session = routed.lock() => session.share(),
            };
            check_cancelled(cancel)?;
            let manager = Arc::new(SftpTransferManager::new());
            let control = manager.register_for_node(&id, node.0.clone());
            let _owner = SftpTransferGuard::new(Some(&manager), &id);
            let local = file_path
                .to_str()
                .ok_or_else(|| error("Invalid local file name"))?;
            let (tx, mut rx) = mpsc::channel(8);
            let work = async {
                if download {
                    let expected = sftp
                        .download_into_staging(path, local, &id, Some(tx), Some(manager.clone()))
                        .await?;
                    if tokio::fs::metadata(local).await?.len() != expected {
                        return Err(SftpError::TransferError("Incomplete download".into()));
                    }
                } else {
                    sftp.upload_staged_file(local, path, &id, overwrite, tx, manager.clone())
                        .await?;
                }
                Ok(())
            };
            tokio::pin!(work);
            let mut cancelling = false;
            let mut receiving = true;
            loop {
                tokio::select! {
                    biased;
                    _ = cancel.changed(), if !cancelling => {
                        control.cancel();
                        cancelling = true;
                    }
                    result = &mut work => {
                        result.map_err(map_sftp_error)?;
                        break;
                    }
                    value = rx.recv(), if receiving => match value {
                        Some(p) => { progress.send_replace((p.transferred_bytes, p.total_bytes)); }
                        None => receiving = false,
                    }
                }
            }
        }
        if let Some(stage) = staging {
            check_cancelled(cancel)?;
            if overwrite {
                match tokio::fs::symlink_metadata(local).await {
                    Ok(metadata) if metadata.is_file() => {
                        let mut permissions = metadata.permissions();
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            permissions.set_mode(permissions.mode() & 0o777);
                        }
                        tokio::fs::set_permissions(&stage, permissions)
                            .await
                            .map_err(io_error)?;
                    }
                    Ok(_) => return Err(error("Download destination is not a regular file")),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(io_error(e)),
                }
                check_cancelled(cancel)?;
                tokio::fs::rename(stage, local).await.map_err(io_error)?;
            } else {
                tempfile::TempPath::try_from_path(stage)
                    .map_err(io_error)?
                    .persist_noclobber(local)
                    .map_err(|cause| io_error(cause.error))?;
            }
        }
        Ok(())
    }
}

impl AgentSession {
    async fn transfer_rpc(&self, method: &str, params: Value) -> Result<Value, IdeFileError> {
        self.transport
            .call_with_timeout(method, params, 60)
            .await
            .map_err(ide_error_from_agent_error)
    }

    async fn download_single(
        &self,
        path: &str,
        local: &Path,
        cancel: &watch::Receiver<bool>,
        progress: &watch::Sender<(u64, u64)>,
    ) -> Result<(), IdeFileError> {
        let mut file = tokio::fs::File::create_new(local).await.map_err(io_error)?;
        let mut offset = 0;
        let mut first_version = None;
        loop {
            check_cancelled(cancel)?;
            let mut value = self
                .transfer_rpc("transfer/read", json!({"path":path,"offset":offset}))
                .await?;
            let invalid_response = || error("Invalid transfer response");
            let Some(Value::String(content)) = value.get_mut("content").map(Value::take) else {
                return Err(invalid_response());
            };
            let content = Zeroizing::new(content);
            let total = value["size"].as_u64().ok_or_else(invalid_response)?;
            let version = value["version"].as_str().ok_or_else(invalid_response)?;
            if let Some((first_total, first)) = &first_version {
                if *first_total != total || first != version {
                    return Err(error("Remote file changed during download"));
                }
            } else {
                first_version = Some((total, version.to_owned()));
            }
            if content.len() > CHUNK.div_ceil(3) * 4 {
                return Err(error("Invalid transfer size"));
            }
            let bytes = Zeroizing::new(
                base64::engine::general_purpose::STANDARD
                    .decode(content.as_bytes())
                    .map_err(|_| error("Invalid transfer data"))?,
            );
            if bytes.len() as u64 != (total - offset).min(CHUNK as u64) {
                return Err(error("Remote file changed during download"));
            }
            file.write_all(&bytes).await.map_err(io_error)?;
            offset += bytes.len() as u64;
            progress.send_replace((offset, total));
            if offset == total {
                break;
            }
        }
        file.sync_all().await.map_err(io_error)
    }

    async fn upload_single(
        &self,
        path: &str,
        local: &Path,
        id: &str,
        overwrite: bool,
        cancel: &watch::Receiver<bool>,
        progress: &watch::Sender<(u64, u64)>,
    ) -> Result<(), IdeFileError> {
        let mut file = tokio::fs::File::open(local).await.map_err(io_error)?;
        let meta = file.metadata().await.map_err(io_error)?;
        if !meta.is_file() {
            return Err(error("Expected a regular file"));
        }
        let total = meta.len();
        let result = async {
            self.transfer_rpc("transfer/begin", json!({"id":id,"path":path}))
                .await?;
            let mut offset = 0;
            let mut buffer = Zeroizing::new(vec![0; CHUNK]);
            loop {
                check_cancelled(cancel)?;
                let n = file.read(&mut buffer).await.map_err(io_error)?;
                if n == 0 {
                    break;
                }
                if offset + n as u64 > total {
                    return Err(error("Local file changed during upload"));
                }
                let content = base64::engine::general_purpose::STANDARD.encode(&buffer[..n]);
                let mut params = json!({"id":id,"offset":offset});
                params["content"] = Value::String(content);
                self.transfer_rpc("transfer/append", params).await?;
                offset += n as u64;
                progress.send_replace((offset, total));
            }
            if offset != total {
                return Err(error("Local file changed during upload"));
            }
            check_cancelled(cancel)?;
            self.transfer_rpc(
                "transfer/commit",
                json!({"id":id,"path":path,"size":total,"overwrite":overwrite}),
            )
            .await?;
            Ok(())
        }
        .await;
        if result.is_err() {
            let _ = self.transfer_rpc("transfer/abort", json!({"id":id})).await;
        }
        result
    }
}
