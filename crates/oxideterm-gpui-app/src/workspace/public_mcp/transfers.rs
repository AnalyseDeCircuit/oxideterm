use std::{
    io::Write,
    sync::Arc,
    time::{Duration, Instant},
};

use oxideterm_public_mcp::{
    ArtifactStore, ClientRef, DomainRequest, OperationRef, PublicToolCall, StartTransferArgs,
    ToolEnvelope, ToolGroup, TransferRef,
};
use oxideterm_sftp::{
    FileType, SftpError, SftpTransferGuard, SftpTransferManager, TransferProgress, TransferState,
};
use oxideterm_ssh::NodeId;
use serde_json::json;
use tempfile::NamedTempFile;

use super::{
    PublicMcpFileSessionRecord, PublicMcpOperationRecord, PublicMcpOperationTarget,
    PublicMcpRuntimeHandles, PublicMcpTransferRecord, PublicMcpTransferState, WorkspaceApp, files,
    finish_serialized, remove_transfer_operations,
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
        temporary_file: NamedTempFile,
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

impl WorkspaceApp {
    /// Starts one bounded transfer without exposing caller-selected local paths.
    pub(super) fn handle_public_mcp_transfer_start(&self, request: DomainRequest) {
        let PublicToolCall::TransferStart(args) = &request.call else {
            return;
        };
        let (file_session_ref, remote_path, direction, resume) = match args {
            StartTransferArgs::Upload {
                file_session_ref,
                remote_path,
                resume,
                ..
            } => (
                file_session_ref.clone(),
                remote_path.clone(),
                "upload",
                *resume,
            ),
            StartTransferArgs::Download {
                file_session_ref,
                remote_path,
                resume,
            } => (
                file_session_ref.clone(),
                remote_path.clone(),
                "download",
                *resume,
            ),
        };
        if resume {
            request.finish(ToolEnvelope::failed(
                "Artifact-backed transfer resume is not available in this version",
            ));
            return;
        }
        let session_is_ready = self
            .public_mcp
            .runtime_handles
            .lock()
            .file_sessions
            .get(&file_session_ref)
            .is_some_and(|record| {
                record.client_ref == request.client_ref
                    && record.root.is_some()
                    && record.session.is_some()
            });
        if !session_is_ready {
            request.finish(ToolEnvelope::failed("The SFTP handle is unavailable"));
            return;
        }

        let (job, initial_size) = match args {
            StartTransferArgs::Upload {
                artifact_ref,
                overwrite,
                ..
            } => {
                let artifact = match self.public_mcp.state.artifacts.read_all(
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
            StartTransferArgs::Download { .. } => {
                let temporary_file = match NamedTempFile::new() {
                    Ok(file) => file,
                    Err(_) => {
                        request.finish(ToolEnvelope::failed(
                            "The private transfer workspace is unavailable",
                        ));
                        return;
                    }
                };
                (PublicMcpTransferJob::Download { temporary_file }, 0)
            }
        };

        let transfer_ref = TransferRef::new();
        let operation_ref = OperationRef::new();
        let internal_id = transfer_ref.to_string();
        {
            let mut handles = self.public_mcp.runtime_handles.lock();
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
        }

        let manager = self.sftp_transfer_manager.clone();
        let guard = SftpTransferGuard::new(Some(&manager), internal_id.clone());
        let router = self.node_router.clone();
        let handles = self.public_mcp.runtime_handles.clone();
        let artifact_store = self.public_mcp.state.artifacts.clone();
        let client_ref = request.client_ref.clone();
        let worker_transfer_ref = transfer_ref.clone();
        self.forwarding_runtime.spawn(async move {
            let _guard = guard;
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
                    finish_transfer_failure(
                        &handles,
                        &worker_transfer_ref,
                        PublicMcpTransferFailure {
                            state: PublicMcpTransferState::Failed,
                            error_code: "session_unavailable",
                            remote_residue: None,
                        },
                    );
                    return;
                }
            };
            let result = run_transfer_job(
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
        });

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
            let mut handles = self.public_mcp.runtime_handles.lock();
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
            .public_mcp
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

    pub(super) fn revoke_public_mcp_client_transfers(&self, client_ref: &ClientRef) {
        let transfer_ids = {
            let mut handles = self.public_mcp.runtime_handles.lock();
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
            .public_mcp
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

    pub(super) fn cancel_public_mcp_client_uploads(&self, client_ref: &ClientRef) {
        let transfer_ids = self
            .public_mcp
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
    let session = file_session
        .session
        .as_ref()
        .ok_or_else(|| transfer_failure("session_unavailable"))?
        .lock()
        .await;
    manager
        .check_control(&transfer_ref.to_string())
        .await
        .map_err(transfer_protocol_failure)?;
    set_transfer_running(handles, transfer_ref);

    match job {
        PublicMcpTransferJob::Upload {
            temporary_file,
            overwrite,
        } => {
            let target = session
                .canonicalize_write_target(&files::path_from_root(root, remote_path))
                .await
                .map_err(|_| transfer_failure("invalid_remote_path"))?;
            files::require_mutable_path_within_root(root, &target)
                .map_err(|_| transfer_failure("path_outside_root"))?;
            if !overwrite && session.stat(&target).await.is_ok() {
                return Err(transfer_failure("remote_exists"));
            }
            let local_path = temporary_file
                .path()
                .to_str()
                .ok_or_else(|| transfer_failure("private_workspace_unavailable"))?;
            let (progress_tx, progress_task) = start_progress_delivery(handles, transfer_ref);
            let result = session
                .upload_file(
                    local_path,
                    &target,
                    transfer_ref.as_str(),
                    Some(progress_tx),
                    Some(manager),
                )
                .await;
            let _ = progress_task.await;
            match result {
                Ok(total_bytes) => {
                    finish_transfer_success(handles, transfer_ref, total_bytes, None);
                    Ok(())
                }
                Err(error) => Err(upload_protocol_failure(error)),
            }
        }
        PublicMcpTransferJob::Download { temporary_file } => {
            let target = session
                .canonicalize(&files::path_from_root(root, remote_path))
                .await
                .map_err(|_| transfer_failure("invalid_remote_path"))?;
            files::require_path_within_root(root, &target)
                .map_err(|_| transfer_failure("path_outside_root"))?;
            let info = session
                .stat(&target)
                .await
                .map_err(|_| transfer_failure("remote_unavailable"))?;
            if info.file_type != FileType::File {
                return Err(transfer_failure("remote_not_file"));
            }
            if info.size > TRANSFER_MAXIMUM_BYTES {
                return Err(transfer_failure("remote_too_large"));
            }
            set_transfer_total(handles, transfer_ref, info.size);
            let local_path = temporary_file
                .path()
                .to_str()
                .ok_or_else(|| transfer_failure("private_workspace_unavailable"))?;
            let (progress_tx, progress_task) = start_progress_delivery(handles, transfer_ref);
            let result = session
                .download_file(
                    &target,
                    local_path,
                    transfer_ref.as_str(),
                    Some(progress_tx),
                    Some(manager),
                )
                .await;
            let _ = progress_task.await;
            let total_bytes = result.map_err(transfer_protocol_failure)?;
            let bytes = tokio::fs::read(temporary_file.path())
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
            if !finish_transfer_success(handles, transfer_ref, total_bytes, Some(artifact.clone()))
            {
                artifact_store.revoke(client_ref, &artifact.artifact_ref);
            }
            Ok(())
        }
    }
}

fn start_progress_delivery(
    handles: &Arc<parking_lot::Mutex<PublicMcpRuntimeHandles>>,
    transfer_ref: &TransferRef,
) -> (
    tokio::sync::mpsc::Sender<TransferProgress>,
    tokio::task::JoinHandle<()>,
) {
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<TransferProgress>(16);
    let handles = handles.clone();
    let transfer_ref = transfer_ref.clone();
    let task = tokio::spawn(async move {
        while let Some(progress) = progress_rx.recv().await {
            let mut handles = handles.lock();
            let Some(record) = handles.transfers.get_mut(&transfer_ref) else {
                continue;
            };
            record.total_bytes = progress.total_bytes;
            record.transferred_bytes = progress.transferred_bytes;
            record.speed_bytes_per_second = progress.speed;
            record.state = match progress.state {
                TransferState::Pending => PublicMcpTransferState::Pending,
                TransferState::InProgress
                | TransferState::Paused
                | TransferState::Completed
                | TransferState::Failed
                | TransferState::Cancelled => PublicMcpTransferState::Running,
            };
        }
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

fn set_transfer_total(
    handles: &Arc<parking_lot::Mutex<PublicMcpRuntimeHandles>>,
    transfer_ref: &TransferRef,
    total_bytes: u64,
) {
    if let Some(record) = handles.lock().transfers.get_mut(transfer_ref) {
        record.total_bytes = total_bytes;
    }
}

fn finish_transfer_success(
    handles: &Arc<parking_lot::Mutex<PublicMcpRuntimeHandles>>,
    transfer_ref: &TransferRef,
    total_bytes: u64,
    artifact: Option<oxideterm_public_mcp::ArtifactProjection>,
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
