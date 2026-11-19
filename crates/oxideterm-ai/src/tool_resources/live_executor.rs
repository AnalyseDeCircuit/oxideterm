use oxideterm_sftp::{BackgroundTransferDirection, BackgroundTransferState, SftpTransferManager};
use oxideterm_ssh::{NodeId, NodeRouter};

use super::{
    AiActionResultLite, AiFileRead, AiFileTransfer, AiFileWrite, AiLiveResourceOperation,
    AiLiveResourceRequest, AiOrchestratorRuntimeSnapshot, AiRemoteFileWriteError, AiSftpOperation,
    AiSftpWrittenFile, AiTransferDirection, AiTransferOutcome, validate_sftp_write_precondition,
};

/// Concrete application adapters used only after the GPUI broker validates a
/// live capability handle. This type must never enter a provider task.
#[derive(Clone)]
pub struct AiLiveToolServices {
    pub node_router: NodeRouter,
    pub sftp_transfer_manager: std::sync::Arc<SftpTransferManager>,
    pub backend_runtime: std::sync::Arc<tokio::runtime::Runtime>,
}

/// Separates a stale SFTP authority from an operation failure without retaining
/// raw transfer arguments in an error's debug representation.
enum AiSftpTransferError {
    OwnerReplaced,
    Operation(String),
}

pub enum AiLiveFileOwner {
    Sftp(crate::AiSftpRuntimeOwner<NodeId>),
    Ide {
        node_id: NodeId,
        file_system: oxideterm_ide_fs::NodeAgentIdeFileSystem,
    },
}

async fn execute_live_resource(
    snapshot: &AiOrchestratorRuntimeSnapshot,
    services: &AiLiveToolServices,
    owner: AiLiveFileOwner,
    request: AiLiveResourceRequest<'_>,
    post_user_approval: bool,
) -> AiActionResultLite {
    use AiLiveFileOwner::{Ide, Sftp};
    use AiLiveResourceRequest::{Read, Transfer, Write};
    match (owner, request) {
        (
            Ide {
                node_id,
                file_system,
            },
            Read(request),
        ) => {
            let path = request.path;
            match file_system.node_agent_read_file(node_id.0, path).await {
                Ok(file) => snapshot.ide_file_read(
                    path,
                    file.content,
                    file.hash,
                    file.size,
                    file.mtime,
                    file.encoding,
                ),
                Err(_) => snapshot.ide_file_owner_unavailable(false),
            }
        }
        (
            Ide {
                node_id,
                file_system,
            },
            Write(request),
        ) => {
            match file_system
                .node_agent_write_file(
                    node_id.0,
                    request.path,
                    request.content,
                    request.expected_hash,
                )
                .await
            {
                Ok(file) => snapshot.ide_file_written(
                    request.path,
                    file.hash,
                    file.size,
                    file.mtime,
                    file.atomic,
                ),
                Err(_) => snapshot.ide_file_owner_unavailable(true),
            }
        }
        (Sftp(owner), Read(request)) => {
            read_live_resource(snapshot, services, owner, request, post_user_approval).await
        }
        (Sftp(owner), Write(request)) => {
            write_live_resource(snapshot, services, owner, request, post_user_approval).await
        }
        (Sftp(owner), Transfer(request)) => {
            transfer_live_resource(snapshot, services, owner, request, post_user_approval).await
        }
        // Transfer handles are issued only for SFTP, never for the IDE.
        (Ide { .. }, Transfer(_)) => snapshot.missing_sftp_transfer_owner(),
    }
}

/// Executes a v2 live read after the UI broker resolved an opaque owner
/// handle to this exact node. No target identifier crosses this boundary.
async fn read_live_resource(
    snapshot: &AiOrchestratorRuntimeSnapshot,
    services: &AiLiveToolServices,
    owner: crate::AiSftpRuntimeOwner<NodeId>,
    request: AiFileRead<'_>,
    post_user_approval: bool,
) -> AiActionResultLite {
    let path = request.path;
    let Ok(shared) = services
        .node_router
        .acquire_existing_sftp_generation(
            &owner.node_id,
            &owner.connection_id,
            owner.session_generation,
        )
        .await
    else {
        return snapshot.sftp_session_changed(AiSftpOperation::Read, false, post_user_approval);
    };
    let result = {
        let sftp = shared.lock().await;
        if request.directory {
            sftp.list_dir(
                path,
                Some(oxideterm_sftp::ListFilter {
                    show_hidden: true,
                    pattern: None,
                    sort: oxideterm_sftp::SortOrder::Name,
                }),
            )
            .await
            .map(|entries| serde_json::json!(entries))
        } else {
            sftp.preview(path)
                .await
                .map(|preview| serde_json::json!(preview))
        }
    };
    match result {
        Ok(data) => snapshot.sftp_file_read(request, data),
        Err(error) if error.is_channel_recoverable() => {
            // A replacement channel is for later requests, never this authorization.
            let _ = services
                .node_router
                .invalidate_and_reacquire_sftp(&owner.node_id)
                .await;
            snapshot.sftp_session_changed(AiSftpOperation::Read, true, post_user_approval)
        }
        Err(error) => snapshot.sftp_read_failed(error),
    }
}

/// Executes a v2 live write after validating the exact SFTP or IDE owner.
async fn write_live_resource(
    snapshot: &AiOrchestratorRuntimeSnapshot,
    services: &AiLiveToolServices,
    owner: crate::AiSftpRuntimeOwner<NodeId>,
    request: AiFileWrite<'_>,
    post_user_approval: bool,
) -> AiActionResultLite {
    match write_remote_file(
        services,
        &owner,
        request.path,
        request.content,
        request.expected_hash,
    )
    .await
    {
        Ok(file) => snapshot.sftp_file_written(request.path, file, request.content),
        Err(AiRemoteFileWriteError::Sftp(error)) if error.is_channel_recoverable() => {
            let _ = services
                .node_router
                .invalidate_and_reacquire_sftp(&owner.node_id)
                .await;
            snapshot.sftp_session_changed(AiSftpOperation::Write, true, post_user_approval)
        }
        Err(error) => snapshot.sftp_write_failed(error, post_user_approval),
    }
}

/// Starts a v2 SFTP transfer from an already validated concrete session.
async fn transfer_live_resource(
    snapshot: &AiOrchestratorRuntimeSnapshot,
    services: &AiLiveToolServices,
    sftp_owner: crate::AiSftpRuntimeOwner<NodeId>,
    request: AiFileTransfer<'_>,
    post_user_approval: bool,
) -> AiActionResultLite {
    let Ok(session) = services
        .node_router
        .acquire_existing_sftp_generation(
            &sftp_owner.node_id,
            &sftp_owner.connection_id,
            sftp_owner.session_generation,
        )
        .await
    else {
        return snapshot.sftp_session_changed(AiSftpOperation::Transfer, false, post_user_approval);
    };
    match run_sftp_transfer(services, &sftp_owner, session, &request).await {
        Ok(result) => snapshot.sftp_transfer_result(&request, result),
        Err(AiSftpTransferError::OwnerReplaced) => {
            snapshot.sftp_session_changed(AiSftpOperation::Transfer, false, post_user_approval)
        }
        Err(AiSftpTransferError::Operation(error)) => snapshot.sftp_transfer_failed(error),
    }
}

async fn write_remote_file(
    services: &AiLiveToolServices,
    owner: &crate::AiSftpRuntimeOwner<NodeId>,
    path: &str,
    content: &str,
    expected_hash: Option<&str>,
) -> Result<AiSftpWrittenFile, AiRemoteFileWriteError<oxideterm_ssh::SftpError>> {
    // File content may contain credentials even when it is not itself a
    // credential field, so the backend copy is wiped after the write.
    let bytes = zeroize::Zeroizing::new(content.as_bytes().to_vec());
    let shared = services
        .node_router
        .acquire_existing_sftp_generation(
            &owner.node_id,
            &owner.connection_id,
            owner.session_generation,
        )
        .await
        .map_err(|_| AiRemoteFileWriteError::OwnerReplaced)?;
    let sftp = shared.lock().await;
    if let Some(expected) = expected_hash {
        let current_bytes = sftp
            .read_file_bytes(path)
            .await
            .map_err(|error| match error {
                oxideterm_ssh::SftpError::FileNotFound(_) => {
                    AiRemoteFileWriteError::ExpectedFileMissing {
                        path: path.to_string(),
                    }
                }
                other => AiRemoteFileWriteError::Sftp(other),
            })?;
        validate_sftp_write_precondition(path, zeroize::Zeroizing::new(current_bytes), expected)?;
    }
    let write = sftp
        .write_content(path, &bytes)
        .await
        .map_err(AiRemoteFileWriteError::Sftp)?;
    let info = sftp
        .stat(path)
        .await
        .map_err(AiRemoteFileWriteError::Sftp)?;
    Ok(AiSftpWrittenFile {
        path: info.path,
        size: info.size,
        mtime: info.modified,
        atomic: write.atomic_write,
    })
}

async fn run_sftp_transfer(
    services: &AiLiveToolServices,
    owner: &crate::AiSftpRuntimeOwner<NodeId>,
    owner_session: std::sync::Arc<tokio::sync::Mutex<oxideterm_ssh::SftpSession>>,
    request: &AiFileTransfer<'_>,
) -> Result<AiTransferOutcome, AiSftpTransferError> {
    if request.directory {
        return start_sftp_directory_transfer(services, owner, owner_session, request).await;
    }
    // Never substitute the channel validated by the window broker.
    let sftp = owner_session.lock().await;
    let manager = Some(services.sftp_transfer_manager.clone());
    let bytes = match request.direction {
        AiTransferDirection::Upload => {
            sftp.upload_file(
                request.source,
                request.destination,
                &request.id,
                None,
                manager,
            )
            .await
        }
        AiTransferDirection::Download => {
            sftp.download_file(
                request.source,
                request.destination,
                &request.id,
                None,
                manager,
            )
            .await
        }
    }
    .map_err(|error| AiSftpTransferError::Operation(error.to_string()))?;
    Ok(AiTransferOutcome::File { bytes })
}

async fn start_sftp_directory_transfer(
    services: &AiLiveToolServices,
    owner: &crate::AiSftpRuntimeOwner<NodeId>,
    owner_session: std::sync::Arc<tokio::sync::Mutex<oxideterm_ssh::SftpSession>>,
    request: &AiFileTransfer<'_>,
) -> Result<AiTransferOutcome, AiSftpTransferError> {
    let (local_path, remote_path, direction_enum) = match request.direction {
        AiTransferDirection::Upload => (
            request.source,
            request.destination,
            BackgroundTransferDirection::Upload,
        ),
        AiTransferDirection::Download => (
            request.destination,
            request.source,
            BackgroundTransferDirection::Download,
        ),
    };
    let resolved = services
        .node_router
        .resolve_connection(&owner.node_id)
        .await
        .map_err(|_| AiSftpTransferError::OwnerReplaced)?;
    if resolved.connection_id != owner.connection_id {
        return Err(AiSftpTransferError::OwnerReplaced);
    }
    let transfer_audit = services
        .node_router
        .audit_context(&owner.node_id)
        .and_then(|node| {
            resolved
                .handle
                .audit_context()
                .map(|runtime| runtime.with_request(&node))
        })
        .or_else(|| {
            resolved
                .handle
                .audit_context()
                .map(|runtime| runtime.for_request())
        });
    let snapshot = services
        .sftp_transfer_manager
        .start_owned_directory_transfer(
            &services.backend_runtime,
            owner_session,
            resolved.handle,
            &resolved.connection_id,
            &owner.node_id.0,
            direction_enum,
            local_path,
            remote_path,
            &request.id,
            transfer_audit.as_ref(),
        )
        .await;

    Ok(AiTransferOutcome::Directory {
        strategy: serde_json::json!(snapshot.strategy),
        transfer: serde_json::json!(snapshot),
    })
}

impl AiLiveToolServices {
    /// Runs an authorized request on its original owner, never on a replacement session.
    pub fn start(
        self,
        owner: AiLiveFileOwner,
        operation: AiLiveResourceOperation,
        snapshot: AiOrchestratorRuntimeSnapshot,
        args: serde_json::Value,
        post_user_approval: bool,
        tool_call_id: String,
        tool_name: String,
        started: std::time::Instant,
        sender: tokio::sync::oneshot::Sender<crate::AiExecutedToolResult>,
    ) {
        let runtime = self.backend_runtime.clone();
        let audit = oxideterm_audit::AuditContext::current_request();
        runtime.spawn(oxideterm_audit::AuditContext::scope_optional(
            audit,
            async move {
                let execution = async {
                    let request = match snapshot.decode_live_resource(operation, &args) {
                        Ok(request) => request,
                        Err(result) => return result,
                    };
                    execute_live_resource(&snapshot, &self, owner, request, post_user_approval)
                        .await
                };
                snapshot
                    .complete_live_resource(tool_call_id, tool_name, started, sender, execution)
                    .await;
            },
        ));
    }
}

pub fn capture_live_transfers(manager: &SftpTransferManager) -> serde_json::Value {
    use super::{AiTransferCapture, AiTransferState};
    let transfers = manager.list_background_transfers(None);
    let facts = transfers
        .iter()
        .map(|transfer| AiTransferCapture {
            id: &transfer.id,
            direction: match transfer.direction {
                BackgroundTransferDirection::Upload => "upload",
                BackgroundTransferDirection::Download => "download",
            },
            state: match transfer.state {
                BackgroundTransferState::Pending => AiTransferState::Pending,
                BackgroundTransferState::Active => AiTransferState::Active,
                BackgroundTransferState::Paused => AiTransferState::Paused,
                BackgroundTransferState::Completed => AiTransferState::Completed,
                BackgroundTransferState::Cancelled => AiTransferState::Cancelled,
                BackgroundTransferState::Error => AiTransferState::Error,
            },
            size: transfer.size,
            transferred: transfer.transferred,
            has_error: transfer.error.is_some(),
            start_time: transfer.start_time,
            end_time: transfer.end_time,
        })
        .collect::<Vec<_>>();
    super::ai_transfers_state(&facts)
}
