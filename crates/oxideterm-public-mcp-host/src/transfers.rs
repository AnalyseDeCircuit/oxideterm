use std::{
    collections::HashMap,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use oxideterm_public_mcp::{
    ArtifactStore, ClientRef, DomainRequest, OperationRef, PublicToolCall, StartTransferArgs,
    ToolEnvelope, ToolGroup, TransferRef,
};
use oxideterm_sftp::{
    FileType, SftpError, SftpSession, SftpTransferGuard, SftpTransferManager, TarDirectoryProfile,
    TarTransferOptions, TransferDirection, TransferProgress, TransferState,
};
use oxideterm_ssh::{NodeId, NodeRouter};
use serde_json::json;
use tempfile::NamedTempFile;

use super::{
    PublicMcpBackground, PublicMcpFileSessionRecord, PublicMcpOperationRecord,
    PublicMcpOperationTarget, PublicMcpRuntimeHandles, PublicMcpTransferRecord,
    PublicMcpTransferState, files, finish_serialized, remove_transfer_operations,
};

const TRANSFER_CAPACITY: usize = 128;
const TRANSFER_CAPACITY_PER_CLIENT: usize = 32;
const TRANSFER_RETENTION: Duration = Duration::from_secs(15 * 60);
const TRANSFER_MAXIMUM_BYTES: u64 = 64 * 1024 * 1024;

enum PublicMcpTransferJob {
    Upload {
        temporary_file: NamedTempFile,
        overwrite: bool,
    },
    Download {
        directory: tempfile::TempDir,
    },
    UploadLocal {
        local_path: PathBuf,
        overwrite: bool,
    },
    DownloadLocal {
        local_path: PathBuf,
        overwrite: bool,
    },
}

struct PublicMcpTransferFailure {
    state: PublicMcpTransferState,
    error_code: &'static str,
    remote_residue: Option<&'static str>,
}

impl PublicMcpTransferState {
    pub(super) fn is_finished(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Failed)
    }
}

impl PublicMcpBackground {
    pub(super) fn handle_public_mcp_transfer_start(&self, request: DomainRequest) {
        let PublicToolCall::TransferStart(args) = &request.call else {
            return;
        };
        let (file_session_ref, remote_path, local_path, direction, resume) = match args {
            StartTransferArgs::Upload {
                file_session_ref,
                remote_path,
                local_path,
                resume,
                ..
            } => (
                file_session_ref.clone(),
                remote_path.clone(),
                local_path.clone(),
                "upload",
                *resume,
            ),
            StartTransferArgs::Download {
                file_session_ref,
                remote_path,
                local_path,
                resume,
                ..
            } => (
                file_session_ref.clone(),
                remote_path.clone(),
                local_path.clone(),
                "download",
                *resume,
            ),
        };
        if resume {
            request.finish(ToolEnvelope::failed(
                "Transfer resume is not available in this version",
            ));
            return;
        }
        if local_path
            .as_ref()
            .is_some_and(|path| !Path::new(path).is_absolute())
        {
            request.finish(ToolEnvelope::failed("The local path must be absolute"));
            return;
        }
        let (job, initial_size) = match args {
            StartTransferArgs::Upload {
                local_path: Some(path),
                artifact_ref: None,
                overwrite,
                ..
            } => (
                PublicMcpTransferJob::UploadLocal {
                    local_path: PathBuf::from(path),
                    overwrite: *overwrite,
                },
                0,
            ),
            StartTransferArgs::Download {
                local_path: Some(path),
                overwrite,
                ..
            } => (
                PublicMcpTransferJob::DownloadLocal {
                    local_path: PathBuf::from(path),
                    overwrite: *overwrite,
                },
                0,
            ),
            StartTransferArgs::Upload {
                artifact_ref: Some(artifact_ref),
                local_path: None,
                overwrite,
                ..
            } => {
                let artifact = match self.state.artifacts.read_all(
                    &request.client_ref,
                    artifact_ref,
                    TRANSFER_MAXIMUM_BYTES,
                ) {
                    Ok(artifact) => artifact,
                    Err(_) => {
                        request.finish(ToolEnvelope::failed(
                            "The upload artifact is unavailable or too large",
                        ));
                        return;
                    }
                };
                let mut temporary_file = match NamedTempFile::new() {
                    Ok(file) => file,
                    Err(_) => {
                        request.finish(ToolEnvelope::failed(
                            "The private transfer workspace is unavailable",
                        ));
                        return;
                    }
                };
                if temporary_file.write_all(&artifact.bytes).is_err()
                    || temporary_file.flush().is_err()
                {
                    request.finish(ToolEnvelope::failed(
                        "The upload artifact could not be prepared",
                    ));
                    return;
                }
                (
                    PublicMcpTransferJob::Upload {
                        temporary_file,
                        overwrite: *overwrite,
                    },
                    artifact.projection.size,
                )
            }
            StartTransferArgs::Upload { .. } => {
                request.finish(ToolEnvelope::failed(
                    "Specify exactly one of artifact_ref and local_path",
                ));
                return;
            }
            StartTransferArgs::Download { .. } => {
                let directory = match private_download_directory(&std::env::temp_dir()) {
                    Ok(file) => file,
                    Err(_) => {
                        request.finish(ToolEnvelope::failed(
                            "The private transfer workspace is unavailable",
                        ));
                        return;
                    }
                };
                (PublicMcpTransferJob::Download { directory }, 0)
            }
        };

        let transfer_ref = TransferRef::new();
        let operation_ref = OperationRef::new();
        let internal_id = transfer_ref.to_string();
        let manager = self.sftp_transfer_manager.clone();
        let guard = {
            let mut handles = self.runtime_handles.lock();
            // An open grant can outlive its channel. The worker refreshes it on the current route.
            if !handles
                .file_sessions
                .get(&file_session_ref)
                .is_some_and(|record| {
                    record.client_ref == request.client_ref && record.root.is_some()
                })
            {
                drop(handles);
                request.finish(ToolEnvelope::failed("The SFTP handle is unavailable"));
                return;
            }
            expire_transfer_records(&mut handles);
            let client_count = handles
                .transfers
                .values()
                .filter(|record| record.client_ref == request.client_ref)
                .count();
            if handles.transfers.len() >= TRANSFER_CAPACITY
                || client_count >= TRANSFER_CAPACITY_PER_CLIENT
            {
                request.finish(ToolEnvelope::failed(
                    "The retained background transfer limit has been reached",
                ));
                return;
            }
            handles.transfers.insert(
                transfer_ref.clone(),
                PublicMcpTransferRecord {
                    client_ref: request.client_ref.clone(),
                    file_session_ref: file_session_ref.clone(),
                    internal_id: internal_id.clone(),
                    direction,
                    remote_path: remote_path.clone(),
                    entry_kind: local_path.is_none().then_some("file"),
                    strategy: local_path.is_none().then_some("sftp_file"),
                    local_path,
                    item_count: None,
                    state: PublicMcpTransferState::Pending,
                    total_bytes: initial_size,
                    transferred_bytes: 0,
                    speed_bytes_per_second: 0,
                    artifact: None,
                    error_code: None,
                    remote_residue: None,
                    finished_at: None,
                },
            );
            handles.operations.insert(
                operation_ref.clone(),
                PublicMcpOperationRecord {
                    client_ref: request.client_ref.clone(),
                    owner_group: ToolGroup::ArtifactTransfer,
                    target: PublicMcpOperationTarget::Transfer(transfer_ref.clone()),
                },
            );
            // Publish cancellation ownership before other callers can observe the pending record.
            let _control = manager.register(&internal_id);
            SftpTransferGuard::new(Some(&manager), internal_id)
        };
        let router = self.node_router.clone();
        let handles = self.runtime_handles.clone();
        let artifact_store = self.state.artifacts.clone();
        let client_ref = request.client_ref.clone();
        let worker_transfer_ref = transfer_ref.clone();
        let audit_context = request.audit_context.clone();
        self.forwarding_runtime
            .spawn(oxideterm_audit::AuditContext::scope_optional(
                audit_context,
                async move {
                    let _guard = guard;
                    if let Err(error) = manager.check_control(worker_transfer_ref.as_str()).await {
                        finish_transfer_failure(
                            &handles,
                            &worker_transfer_ref,
                            transfer_protocol_failure(error),
                        );
                        return;
                    }
                    let session = match files::refresh_file_session(
                        &router,
                        &handles,
                        &client_ref,
                        &file_session_ref,
                    )
                    .await
                    {
                        Ok(session) => session,
                        Err(_) => {
                            let failure =
                                match manager.check_control(worker_transfer_ref.as_str()).await {
                                    Err(error) => transfer_protocol_failure(error),
                                    Ok(()) => transfer_failure("session_unavailable"),
                                };
                            finish_transfer_failure(&handles, &worker_transfer_ref, failure);
                            return;
                        }
                    };
                    let result = run_transfer_job(
                        &router,
                        &handles,
                        &worker_transfer_ref,
                        &client_ref,
                        &remote_path,
                        session,
                        job,
                        manager,
                        artifact_store,
                    )
                    .await;
                    if let Err(error) = result {
                        finish_transfer_failure(&handles, &worker_transfer_ref, error);
                    }
                },
            ));

        finish_serialized(
            request,
            json!({
                "transfer_ref": transfer_ref,
                "operation_ref": operation_ref,
                "state": PublicMcpTransferState::Pending,
                "direction": direction,
                "resume_supported": false,
            }),
        );
    }

    pub(super) fn handle_public_mcp_transfer_status(&self, request: DomainRequest) {
        let PublicToolCall::TransferStatus(args) = &request.call else {
            return;
        };
        let projection = {
            let mut handles = self.runtime_handles.lock();
            expire_transfer_records(&mut handles);
            handles
                .transfers
                .get(&args.transfer_ref)
                .filter(|record| record.client_ref == request.client_ref)
                .map(|record| {
                    json!({
                        "transfer_ref": args.transfer_ref,
                        "file_session_ref": record.file_session_ref,
                        "direction": record.direction,
                        "remote_path": record.remote_path,
                        "local_path": record.local_path,
                        "entry_kind": record.entry_kind,
                        "strategy": record.strategy,
                        "item_count": record.item_count,
                        "state": record.state,
                        "total_bytes": record.total_bytes,
                        "transferred_bytes": record.transferred_bytes,
                        "speed_bytes_per_second": record.speed_bytes_per_second,
                        "artifact": record.artifact,
                        "error_code": record.error_code,
                        "remote_residue": record.remote_residue,
                        "resume_supported": false,
                    })
                })
        };
        match projection {
            Some(projection) => finish_serialized(request, projection),
            None => request.finish(ToolEnvelope::failed(
                "The background transfer handle is unavailable",
            )),
        }
    }

    pub(super) fn handle_public_mcp_transfer_cancel(&self, request: DomainRequest) {
        let PublicToolCall::TransferCancel(args) = &request.call else {
            return;
        };
        let internal_id = self
            .runtime_handles
            .lock()
            .transfers
            .get(&args.transfer_ref)
            .filter(|record| record.client_ref == request.client_ref)
            .filter(|record| !record.state.is_finished())
            .map(|record| record.internal_id.clone());
        let Some(internal_id) = internal_id else {
            request.finish(ToolEnvelope::failed(
                "The background transfer is unavailable or already finished",
            ));
            return;
        };
        let cancel_requested = self.sftp_transfer_manager.cancel(&internal_id);
        finish_serialized(request, json!({ "cancel_requested": cancel_requested }));
    }

    pub fn revoke_public_mcp_client_transfers(&self, client_ref: &ClientRef) {
        let transfer_ids = {
            let mut handles = self.runtime_handles.lock();
            let removed = handles
                .transfers
                .extract_if(|_, record| &record.client_ref == client_ref)
                .collect::<Vec<_>>();
            let transfer_refs = removed
                .iter()
                .map(|(transfer_ref, _)| transfer_ref.clone())
                .collect::<Vec<_>>();
            remove_transfer_operations(&mut handles, &transfer_refs);
            removed
                .into_iter()
                .map(|(_, record)| record.internal_id)
                .collect::<Vec<_>>()
        };
        for transfer_id in transfer_ids {
            self.sftp_transfer_manager.cancel(&transfer_id);
        }
    }

    pub(super) fn cancel_public_mcp_file_session_transfers(
        &self,
        file_session_ref: &oxideterm_public_mcp::FileSessionRef,
    ) {
        let transfer_ids = self
            .runtime_handles
            .lock()
            .transfers
            .values()
            .filter(|record| {
                &record.file_session_ref == file_session_ref && !record.state.is_finished()
            })
            .map(|record| record.internal_id.clone())
            .collect::<Vec<_>>();
        for transfer_id in transfer_ids {
            self.sftp_transfer_manager.cancel(&transfer_id);
        }
    }

    pub fn cancel_public_mcp_client_uploads(&self, client_ref: &ClientRef) {
        let transfer_ids = self
            .runtime_handles
            .lock()
            .transfers
            .values()
            .filter(|record| {
                &record.client_ref == client_ref
                    && record.direction == "upload"
                    && !record.state.is_finished()
            })
            .map(|record| record.internal_id.clone())
            .collect::<Vec<_>>();
        for transfer_id in transfer_ids {
            self.sftp_transfer_manager.cancel(&transfer_id);
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_transfer_job(
    router: &NodeRouter,
    handles: &Arc<parking_lot::Mutex<PublicMcpRuntimeHandles>>,
    transfer_ref: &TransferRef,
    client_ref: &ClientRef,
    remote_path: &str,
    file_session: PublicMcpFileSessionRecord,
    job: PublicMcpTransferJob,
    manager: Arc<SftpTransferManager>,
    artifact_store: Arc<ArtifactStore>,
) -> Result<(), PublicMcpTransferFailure> {
    let root =
        files::ready_root(&file_session).map_err(|_| transfer_failure("session_unavailable"))?;
    let routed_session = file_session
        .session
        .as_ref()
        .ok_or_else(|| transfer_failure("session_unavailable"))?;
    manager
        .check_control(transfer_ref.as_str())
        .await
        .map_err(transfer_protocol_failure)?;
    set_transfer_running(handles, transfer_ref);

    let path = files::path_from_root(root, remote_path);
    let (direction, local, overwrite) = match &job {
        PublicMcpTransferJob::Upload {
            temporary_file,
            overwrite,
        } => (
            TransferDirection::Upload,
            temporary_file.path().to_owned(),
            *overwrite,
        ),
        PublicMcpTransferJob::UploadLocal {
            local_path,
            overwrite,
        } => (
            TransferDirection::Upload,
            tokio::fs::canonicalize(local_path)
                .await
                .map_err(local_unavailable)?,
            *overwrite,
        ),
        PublicMcpTransferJob::Download { directory } => (
            TransferDirection::Download,
            directory.path().join("download"),
            false,
        ),
        PublicMcpTransferJob::DownloadLocal {
            local_path,
            overwrite,
        } => (TransferDirection::Download, local_path.clone(), *overwrite),
    };
    let artifact_backed = matches!(
        job,
        PublicMcpTransferJob::Upload { .. } | PublicMcpTransferJob::Download { .. }
    );
    let local_str = local.to_str().ok_or_else(|| {
        transfer_failure(if artifact_backed {
            "private_workspace_unavailable"
        } else {
            "local_unavailable"
        })
    })?;
    let (target, profile) = match direction {
        TransferDirection::Upload => {
            let metadata = tokio::fs::metadata(&local)
                .await
                .map_err(local_unavailable)?;
            let session = routed_session.lock().await;
            manager
                .check_control(transfer_ref.as_str())
                .await
                .map_err(transfer_protocol_failure)?;
            let target = session
                .canonicalize_write_target(&path)
                .await
                .map_err(|_| transfer_failure("invalid_remote_path"))?;
            files::require_mutable_path_within_root(root, &target)
                .map_err(|_| transfer_failure("path_outside_root"))?;
            if metadata.is_file() {
                set_transfer_details(
                    handles,
                    transfer_ref,
                    "file",
                    Some("sftp_file"),
                    metadata.len(),
                );
                if !overwrite && session.stat(&target).await.is_ok() {
                    return Err(transfer_failure("remote_exists"));
                }
                let (progress_tx, progress_task) = start_progress_delivery(handles, transfer_ref);
                let result = session
                    .upload_file(
                        local_str,
                        &target,
                        transfer_ref.as_str(),
                        Some(progress_tx),
                        Some(manager),
                    )
                    .await;
                let _ = progress_task.await;
                let total_bytes = result.map_err(upload_protocol_failure)?;
                finish_transfer_success(handles, transfer_ref, total_bytes, None, None);
                return Ok(());
            }
            if !metadata.is_dir() {
                return Err(transfer_failure("local_unsupported_type"));
            }
            set_transfer_details(handles, transfer_ref, "directory", None, 0);
            if let Ok(info) = session.stat(&target).await {
                if info.file_type != FileType::Directory {
                    return Err(transfer_failure("remote_not_directory"));
                }
                if !overwrite {
                    return Err(transfer_failure("remote_exists"));
                }
                validate_upload_merge(
                    &session,
                    root,
                    &local,
                    &target,
                    &manager,
                    transfer_ref.as_str(),
                )
                .await?;
            }
            for prefix in oxideterm_sftp::remote_directory_prefixes(&target) {
                if files::require_path_within_root(root, &prefix).is_ok() {
                    let _ = session.mkdir(&prefix).await;
                }
            }
            drop(session);
            (
                target,
                oxideterm_sftp::profile_local_directory(&local).await.ok(),
            )
        }
        TransferDirection::Download => {
            let session = routed_session.lock().await;
            manager
                .check_control(transfer_ref.as_str())
                .await
                .map_err(transfer_protocol_failure)?;
            let target = session
                .canonicalize(&path)
                .await
                .map_err(|_| transfer_failure("invalid_remote_path"))?;
            files::require_path_within_root(root, &target)
                .map_err(|_| transfer_failure("path_outside_root"))?;
            let info = session
                .stat(&target)
                .await
                .map_err(|_| transfer_failure("remote_unavailable"))?;
            if artifact_backed && info.file_type != FileType::File {
                return Err(transfer_failure("remote_not_file"));
            }
            if artifact_backed && info.size > TRANSFER_MAXIMUM_BYTES {
                return Err(transfer_failure("remote_too_large"));
            }
            if info.file_type == FileType::File {
                set_transfer_details(handles, transfer_ref, "file", Some("sftp_file"), info.size);
                let mut replacement = None;
                let mut destination = local.clone();
                if !artifact_backed {
                    let existing = local_metadata(&local).await?;
                    if let Some(meta) = &existing {
                        if meta.is_dir() {
                            return Err(transfer_failure("local_is_directory"));
                        }
                        if !overwrite {
                            return Err(transfer_failure("local_exists"));
                        }
                    }
                    if let Some(parent) = local.parent() {
                        tokio::fs::create_dir_all(parent)
                            .await
                            .map_err(local_unavailable)?;
                        if existing.is_some() {
                            // SFTP creates files exclusively. Replace the old entry only after success.
                            let dir = private_download_directory(parent)?;
                            destination = dir.path().join("download");
                            replacement = Some(dir);
                        }
                    }
                }
                let (progress_tx, progress_task) = start_progress_delivery(handles, transfer_ref);
                let result = session
                    .download_file(
                        &target,
                        local_path_str(&destination)?,
                        transfer_ref.as_str(),
                        Some(progress_tx),
                        Some(manager),
                    )
                    .await;
                let _ = progress_task.await;
                let total_bytes = result.map_err(transfer_protocol_failure)?;
                if !artifact_backed {
                    if replacement.is_some() {
                        tokio::fs::rename(&destination, &local)
                            .await
                            .map_err(local_unavailable)?;
                    }
                    finish_transfer_success(handles, transfer_ref, total_bytes, None, None);
                    return Ok(());
                }
                // The size can change between the authorization stat and the downloader's stat.
                if total_bytes > TRANSFER_MAXIMUM_BYTES {
                    return Err(transfer_failure("remote_too_large"));
                }
                let bytes = tokio::fs::read(&local)
                    .await
                    .map_err(|_| transfer_failure("private_workspace_unavailable"))?;
                if !transfer_is_live(handles, transfer_ref, client_ref) {
                    return Ok(());
                }
                let artifact = artifact_store
                    .stage(
                        client_ref.clone(),
                        &bytes,
                        "application/octet-stream".to_owned(),
                        files::safe_artifact_name(&target),
                    )
                    .map_err(|_| transfer_failure("artifact_capacity"))?;
                if !finish_transfer_success(
                    handles,
                    transfer_ref,
                    total_bytes,
                    Some(artifact.clone()),
                    None,
                ) {
                    artifact_store.revoke(client_ref, &artifact.artifact_ref);
                }
                return Ok(());
            }
            if info.file_type != FileType::Directory {
                return Err(transfer_failure("remote_unsupported_type"));
            }
            set_transfer_details(handles, transfer_ref, "directory", None, 0);
            if let Some(meta) = local_metadata(&local).await? {
                if !meta.is_dir() {
                    return Err(transfer_failure("local_not_directory"));
                }
                if !overwrite && dir_is_non_empty(&local).await? {
                    return Err(transfer_failure("local_exists"));
                }
            }
            let profile = match session
                .profile_remote_directory(&target, transfer_ref.as_str(), &Some(manager.clone()))
                .await
            {
                Ok(profile) => Some(profile),
                Err(error) if error.is_transfer_control() => {
                    return Err(transfer_protocol_failure(error));
                }
                Err(_) => None,
            };
            (target, profile)
        }
    };

    run_directory_job(
        router,
        handles,
        transfer_ref,
        &file_session,
        manager,
        direction,
        &local,
        &target,
        overwrite,
        profile,
    )
    .await
}

async fn validate_upload_merge(
    session: &SftpSession,
    root: &str,
    local: &Path,
    remote: &str,
    manager: &SftpTransferManager,
    id: &str,
) -> Result<(), PublicMcpTransferFailure> {
    // A canonical top-level directory alone cannot authorize writes through existing child links.
    let mut pending = vec![(local.to_owned(), remote.to_owned())];
    while let Some((local, remote)) = pending.pop() {
        manager
            .check_control(id)
            .await
            .map_err(transfer_protocol_failure)?;
        let entries = session
            .list_dir(&remote, None)
            .await
            .map_err(|_| transfer_failure("remote_unavailable"))?;
        for entry in entries {
            if !entry.is_symlink && entry.file_type != FileType::Directory {
                continue;
            }
            let child = local.join(entry.path.rsplit('/').next().unwrap_or(&entry.name));
            let Some(meta) = local_metadata(&child).await? else {
                continue;
            };
            if !meta.is_dir() && !meta.is_file() {
                continue;
            }
            let target = if entry.is_symlink {
                let canonical = session
                    .canonicalize(&entry.path)
                    .await
                    .map_err(|_| transfer_failure("invalid_remote_path"))?;
                files::require_mutable_path_within_root(root, &canonical)
                    .map_err(|_| transfer_failure("path_outside_root"))?;
                canonical
            } else {
                entry.path
            };
            if meta.is_dir() && entry.file_type == FileType::Directory {
                pending.push((child, target));
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run_directory_job(
    router: &NodeRouter,
    handles: &Arc<parking_lot::Mutex<PublicMcpRuntimeHandles>>,
    transfer_ref: &TransferRef,
    file_session: &PublicMcpFileSessionRecord,
    manager: Arc<SftpTransferManager>,
    direction: TransferDirection,
    local: &Path,
    target: &str,
    overwrite: bool,
    profile: Option<TarDirectoryProfile>,
) -> Result<(), PublicMcpTransferFailure> {
    let local_str = local_path_str(local)?;
    let total = profile.map_or(0, |p| p.total_bytes);
    set_transfer_details(handles, transfer_ref, "directory", None, total);
    let id = transfer_ref.as_str();
    manager
        .check_control(id)
        .await
        .map_err(transfer_protocol_failure)?;
    let failure = match direction {
        TransferDirection::Upload => upload_protocol_failure,
        TransferDirection::Download => transfer_protocol_failure,
    };
    // An exec channel may bypass the SFTP mutex, but only on the leased connection generation.
    if let Some(profile) = profile.filter(|p| p.prefers_tar())
        && let Some(route) = &file_session.route
        && let Ok(resolved) = router.resolve_connection(&file_session.node_id).await
        && resolved.connection_id == route.connection_id()
    {
        let caps = manager
            .tar_capabilities(&resolved.connection_id, &resolved.handle)
            .await;
        if caps.supports_tar {
            manager
                .check_control(id)
                .await
                .map_err(transfer_protocol_failure)?;
            let options = TarTransferOptions {
                profile,
                compression: profile.recommended_compression(caps.compression),
            };
            set_transfer_details(
                handles,
                transfer_ref,
                "directory",
                Some("tar_archive"),
                total,
            );
            let staging = DirectoryDownload::prepare(direction, local).await?;
            let (tx, delivery) = start_progress_delivery(handles, transfer_ref);
            let result = if let Some(stage) = &staging {
                oxideterm_sftp::tar_download_directory(
                    &resolved.handle,
                    target,
                    local_path_str(&stage.tree)?,
                    id,
                    Some(tx),
                    Some(manager.clone()),
                    options,
                )
                .await
            } else {
                oxideterm_sftp::tar_upload_directory(
                    &resolved.handle,
                    local_str,
                    target,
                    id,
                    Some(tx),
                    Some(manager.clone()),
                    options,
                )
                .await
            };
            let _ = delivery.await;
            match result {
                Ok(result) => {
                    if let Some(stage) = staging {
                        stage.install(local, overwrite, &manager, id).await?;
                    }
                    finish_transfer_success(
                        handles,
                        transfer_ref,
                        total.max(result.stream_bytes),
                        None,
                        Some(result.item_count),
                    );
                    return Ok(());
                }
                Err(error) if error.is_transfer_control() => return Err(failure(error)),
                Err(_) => {} // The private download attempt is discarded before retrying via SFTP.
            }
        }
    }

    set_transfer_details(
        handles,
        transfer_ref,
        "directory",
        Some("sftp_recursive"),
        total,
    );
    let session = file_session
        .session
        .as_ref()
        .ok_or_else(|| transfer_failure("session_unavailable"))?
        .lock()
        .await;
    manager.check_control(id).await.map_err(failure)?;
    // Recursive SFTP also creates files exclusively; merge replacements only after it succeeds.
    let staging = DirectoryDownload::prepare(direction, local).await?;
    let (tx, delivery) = start_progress_delivery(handles, transfer_ref);
    let result = match direction {
        TransferDirection::Upload => {
            session
                .upload_dir(local_str, target, id, Some(tx), Some(manager.clone()))
                .await
        }
        TransferDirection::Download => {
            session
                .download_dir(
                    target,
                    local_path_str(staging.as_ref().map_or(local, |stage| stage.tree.as_path()))?,
                    id,
                    Some(tx),
                    Some(manager.clone()),
                )
                .await
        }
    };
    drop(session);
    let transferred = delivery
        .await
        .map_err(|_| transfer_failure("transfer_failed"))?;
    let count = result.map_err(failure)?;
    if let Some(stage) = staging {
        stage.install(local, overwrite, &manager, id).await?;
    }
    finish_transfer_success(
        handles,
        transfer_ref,
        total.max(transferred),
        None,
        Some(count),
    );
    Ok(())
}

fn local_unavailable(_: std::io::Error) -> PublicMcpTransferFailure {
    transfer_failure("local_unavailable")
}

fn local_path_str(path: &Path) -> Result<&str, PublicMcpTransferFailure> {
    path.to_str()
        .ok_or_else(|| transfer_failure("local_unavailable"))
}

async fn local_metadata(
    path: &Path,
) -> Result<Option<std::fs::Metadata>, PublicMcpTransferFailure> {
    match tokio::fs::symlink_metadata(path).await {
        Ok(meta) => Ok(Some(meta)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(local_unavailable(e)),
    }
}

async fn dir_is_non_empty(path: &Path) -> Result<bool, PublicMcpTransferFailure> {
    Ok(tokio::fs::read_dir(path)
        .await
        .map_err(local_unavailable)?
        .next_entry()
        .await
        .map_err(local_unavailable)?
        .is_some())
}

fn private_download_directory(
    parent: &Path,
) -> Result<tempfile::TempDir, PublicMcpTransferFailure> {
    let mut builder = tempfile::Builder::new();
    builder.prefix("oxideterm-download-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(0o700));
    }
    builder.tempdir_in(parent).map_err(local_unavailable)
}

struct DirectoryDownload {
    workspace: tempfile::TempDir,
    tree: PathBuf,
}

impl DirectoryDownload {
    async fn prepare(
        direction: TransferDirection,
        destination: &Path,
    ) -> Result<Option<Self>, PublicMcpTransferFailure> {
        if direction == TransferDirection::Upload {
            return Ok(None);
        }
        tokio::fs::create_dir_all(destination)
            .await
            .map_err(local_unavailable)?;
        let workspace = private_download_directory(destination)?;
        // Tar may restore directory permissions; keep the extracted root inside the private workspace.
        let tree = workspace.path().join("tree");
        Ok(Some(Self { workspace, tree }))
    }

    async fn install(
        self,
        destination: &Path,
        overwrite: bool,
        manager: &SftpTransferManager,
        id: &str,
    ) -> Result<(), PublicMcpTransferFailure> {
        let mut pending = vec![(self.tree, destination.to_owned())];
        while let Some((from, to)) = pending.pop() {
            let mut entries = tokio::fs::read_dir(from).await.map_err(local_unavailable)?;
            while let Some(entry) = entries.next_entry().await.map_err(local_unavailable)? {
                manager
                    .check_control(id)
                    .await
                    .map_err(transfer_protocol_failure)?;
                let dest = to.join(entry.file_name());
                if dest == self.workspace.path() {
                    return Err(transfer_failure("local_exists"));
                }
                if entry.file_type().await.map_err(local_unavailable)?.is_dir() {
                    if let Some(meta) = local_metadata(&dest).await? {
                        // Never follow a destination link when merging an existing tree.
                        if !meta.is_dir() {
                            return Err(transfer_failure("local_not_directory"));
                        }
                        pending.push((entry.path(), dest));
                        continue;
                    }
                } else if !overwrite {
                    tempfile::TempPath::try_from_path(entry.path())
                        .map_err(local_unavailable)?
                        .persist_noclobber(&dest)
                        .map_err(|e| {
                            if e.error.kind() == std::io::ErrorKind::AlreadyExists {
                                transfer_failure("local_exists")
                            } else {
                                local_unavailable(e.error)
                            }
                        })?;
                    continue;
                }
                tokio::fs::rename(entry.path(), dest)
                    .await
                    .map_err(local_unavailable)?;
            }
        }
        Ok(())
    }
}

fn start_progress_delivery(
    handles: &Arc<parking_lot::Mutex<PublicMcpRuntimeHandles>>,
    transfer_ref: &TransferRef,
) -> (
    tokio::sync::mpsc::Sender<TransferProgress>,
    tokio::task::JoinHandle<u64>,
) {
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<TransferProgress>(16);
    let handles = handles.clone();
    let transfer_ref = transfer_ref.clone();
    let task = tokio::spawn(async move {
        let mut transferred = 0u64;
        let mut active_files = HashMap::new();
        let started = Instant::now();
        while let Some(progress) = progress_rx.recv().await {
            let mut handles = handles.lock();
            let Some(record) = handles.transfers.get_mut(&transfer_ref) else {
                continue;
            };
            if record.strategy == Some("sftp_recursive") {
                // Recursive workers report independent file offsets; retain only in-flight offsets.
                let previous = if progress.state == TransferState::Completed {
                    active_files.remove(&progress.remote_path)
                } else {
                    active_files.insert(progress.remote_path, progress.transferred_bytes)
                }
                .unwrap_or_default();
                transferred =
                    transferred.saturating_add(progress.transferred_bytes.saturating_sub(previous));
                record.total_bytes = record.total_bytes.max(transferred);
                let elapsed = started.elapsed().as_secs_f64();
                record.speed_bytes_per_second = if elapsed > 0.0 {
                    (transferred as f64 / elapsed) as u64
                } else {
                    0
                };
            } else {
                transferred = progress.transferred_bytes;
                record.total_bytes = progress.total_bytes;
                record.speed_bytes_per_second = progress.speed;
            }
            record.transferred_bytes = transferred;
            record.state = match progress.state {
                TransferState::Pending => PublicMcpTransferState::Pending,
                TransferState::InProgress
                | TransferState::Paused
                | TransferState::Completed
                | TransferState::Failed
                | TransferState::Cancelled => PublicMcpTransferState::Running,
            };
        }
        transferred
    });
    (progress_tx, task)
}

fn set_transfer_running(
    handles: &Arc<parking_lot::Mutex<PublicMcpRuntimeHandles>>,
    transfer_ref: &TransferRef,
) {
    if let Some(record) = handles.lock().transfers.get_mut(transfer_ref) {
        record.state = PublicMcpTransferState::Running;
    }
}

fn set_transfer_details(
    handles: &Arc<parking_lot::Mutex<PublicMcpRuntimeHandles>>,
    transfer_ref: &TransferRef,
    entry_kind: &'static str,
    strategy: Option<&'static str>,
    total_bytes: u64,
) {
    if let Some(record) = handles.lock().transfers.get_mut(transfer_ref) {
        record.entry_kind = Some(entry_kind);
        record.strategy = strategy;
        record.total_bytes = total_bytes;
        record.transferred_bytes = 0;
        record.speed_bytes_per_second = 0;
    }
}

fn finish_transfer_success(
    handles: &Arc<parking_lot::Mutex<PublicMcpRuntimeHandles>>,
    transfer_ref: &TransferRef,
    total_bytes: u64,
    artifact: Option<oxideterm_public_mcp::ArtifactProjection>,
    item_count: Option<u64>,
) -> bool {
    let mut handles = handles.lock();
    let Some(record) = handles.transfers.get_mut(transfer_ref) else {
        return false;
    };
    record.state = PublicMcpTransferState::Completed;
    record.total_bytes = total_bytes;
    record.transferred_bytes = total_bytes;
    record.speed_bytes_per_second = 0;
    record.artifact = artifact;
    record.item_count = item_count;
    record.error_code = None;
    record.remote_residue = None;
    record.finished_at = Some(Instant::now());
    true
}

fn finish_transfer_failure(
    handles: &Arc<parking_lot::Mutex<PublicMcpRuntimeHandles>>,
    transfer_ref: &TransferRef,
    failure: PublicMcpTransferFailure,
) {
    if let Some(record) = handles.lock().transfers.get_mut(transfer_ref) {
        record.state = failure.state;
        record.speed_bytes_per_second = 0;
        record.error_code = Some(failure.error_code);
        record.remote_residue = failure.remote_residue;
        record.finished_at = Some(Instant::now());
    }
}

fn transfer_is_live(
    handles: &Arc<parking_lot::Mutex<PublicMcpRuntimeHandles>>,
    transfer_ref: &TransferRef,
    client_ref: &ClientRef,
) -> bool {
    handles
        .lock()
        .transfers
        .get(transfer_ref)
        .is_some_and(|record| &record.client_ref == client_ref)
}

fn transfer_failure(error_code: &'static str) -> PublicMcpTransferFailure {
    PublicMcpTransferFailure {
        state: PublicMcpTransferState::Failed,
        error_code,
        remote_residue: None,
    }
}

fn transfer_protocol_failure(error: SftpError) -> PublicMcpTransferFailure {
    if matches!(error, SftpError::TransferCancelled) {
        PublicMcpTransferFailure {
            state: PublicMcpTransferState::Cancelled,
            error_code: "cancelled",
            remote_residue: None,
        }
    } else {
        transfer_failure("transfer_failed")
    }
}

fn upload_protocol_failure(error: SftpError) -> PublicMcpTransferFailure {
    let mut failure = transfer_protocol_failure(error);
    failure.remote_residue = Some("possible_partial_file");
    failure
}

pub(super) fn expire_transfer_records(handles: &mut PublicMcpRuntimeHandles) {
    let now = Instant::now();
    let expired = handles
        .transfers
        .iter()
        .filter_map(|(transfer_ref, record)| {
            (record.state.is_finished()
                && record.finished_at.is_none_or(|finished_at| {
                    now.saturating_duration_since(finished_at) > TRANSFER_RETENTION
                }))
            .then_some(transfer_ref.clone())
        })
        .collect::<Vec<_>>();
    handles.transfers.retain(|_, record| {
        !record.state.is_finished()
            || record.finished_at.is_some_and(|finished_at| {
                now.saturating_duration_since(finished_at) <= TRANSFER_RETENTION
            })
    });
    remove_transfer_operations(handles, &expired);
}

pub(super) fn invalidate_for_disconnected_nodes(
    handles: &mut PublicMcpRuntimeHandles,
    disconnected: &[NodeId],
) -> Vec<String> {
    let disconnected_file_sessions = handles
        .file_sessions
        .iter()
        .filter_map(|(file_session_ref, record)| {
            disconnected
                .contains(&record.node_id)
                .then_some(file_session_ref.clone())
        })
        .collect::<Vec<_>>();
    handles
        .transfers
        .values()
        .filter(|record| {
            !record.state.is_finished()
                && disconnected_file_sessions.contains(&record.file_session_ref)
        })
        .map(|record| record.internal_id.clone())
        .collect()
}
