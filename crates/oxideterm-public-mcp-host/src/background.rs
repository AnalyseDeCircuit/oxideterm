use super::*;
use oxideterm_public_mcp::{DomainMessage, RecordingStatusTarget};

async fn acquire_ready_target(
    router: &NodeRouter,
    target: &NodeId,
    consumer: ConnectionConsumer,
    chain: Option<tokio::sync::watch::Receiver<oxideterm_ssh::ConnectionChainProgress>>,
) -> Result<oxideterm_ssh::ConnectionConsumerLease, String> {
    use oxideterm_ssh::ConnectionTraceStatus;
    let budget = Duration::from_secs(30);
    let Some(mut progress) = chain else {
        return router
            .lease_connection_wait(target, consumer, budget)
            .await
            .map_err(|_| "The SSH node did not become ready".to_owned());
    };
    let deadline = tokio::time::Instant::now() + budget;
    loop {
        let step = *progress.borrow_and_update();
        // Only the ordinal and the typed stage may cross the public boundary.
        let failure = |reason| format!("SSH hop {} {reason} during {:?}", step.hop, step.stage);
        match step.status {
            ConnectionTraceStatus::Ready => {
                return tokio::time::timeout_at(
                    deadline,
                    router.lease_connection_wait(
                        target,
                        consumer,
                        deadline.saturating_duration_since(tokio::time::Instant::now()),
                    ),
                )
                .await
                .map_err(|_| format!("SSH hop {} timed out acquiring the target lease", step.hop))?
                .map_err(|_| format!("SSH hop {} failed acquiring the target lease", step.hop));
            }
            ConnectionTraceStatus::Failed => return Err(failure("failed")),
            ConnectionTraceStatus::Cancelled => return Err(failure("was cancelled")),
            ConnectionTraceStatus::Running => {}
        }
        tokio::time::timeout_at(deadline, progress.changed())
            .await
            .map_err(|_| failure("timed out"))?
            .map_err(|_| failure("was interrupted"))?;
    }
}

pub enum PublicMcpWindowRequest {
    Call(Box<DomainRequest>),
    ConnectionDirectory(Box<DomainRequest>),
    InstallAddon {
        request: Box<DomainRequest>,
        package: PublicMcpAddonPackage,
    },
}

impl PublicMcpWindowRequest {
    pub fn request(&self) -> &DomainRequest {
        match self {
            Self::Call(request)
            | Self::ConnectionDirectory(request)
            | Self::InstallAddon { request, .. } => request,
        }
    }

    pub fn into_request(self) -> DomainRequest {
        match self {
            Self::Call(request)
            | Self::ConnectionDirectory(request)
            | Self::InstallAddon { request, .. } => *request,
        }
    }
}

pub enum PublicMcpWindowDelivery {
    Request {
        request: PublicMcpWindowRequest,
        accepted: Option<tokio::sync::oneshot::Sender<()>>,
    },
    StateChanged,
}

#[derive(Clone)]
pub struct PublicMcpBackground {
    pub(crate) state: Arc<PublicMcpState>,
    pub(crate) quick_commands: Arc<Mutex<quick_commands::QuickCommandDirectory>>,
    pub(crate) settings_path: PathBuf,
    pub(crate) recordings: Arc<Mutex<HashMap<RecordingRef, PublicMcpRecordingRecord>>>,
    pub(crate) runtime_handles: Arc<Mutex<PublicMcpRuntimeHandles>>,
    pub(crate) history: Arc<Mutex<oxideterm_public_mcp::history::HistoryStore>>,
    pub(crate) desktops: Arc<Mutex<HashMap<DesktopRef, PublicMcpDesktopRecord>>>,
    pub(crate) terminals: Arc<Mutex<HashMap<TerminalRef, PublicMcpTerminalRecord>>>,
    pub(crate) sync_handles: Arc<Mutex<sync_handles::SyncHandles>>,
    pub(crate) admission: Arc<Mutex<bool>>,
    pub(crate) node_router: NodeRouter,
    pub(crate) sftp_transfer_manager: Arc<oxideterm_sftp::SftpTransferManager>,
    pub(crate) forwarding_runtime: tokio::runtime::Handle,
    pub(crate) forwarding_service: Arc<dyn PublicMcpForwardPort>,
    pub(crate) effects: tokio::sync::mpsc::UnboundedSender<PublicMcpHostEffect>,
}

impl PublicMcpBackground {
    pub fn new(
        host: &PublicMcpHost,
        node_router: NodeRouter,
        sftp_transfer_manager: Arc<oxideterm_sftp::SftpTransferManager>,
        forwarding_runtime: tokio::runtime::Handle,
        forwarding_service: Arc<dyn PublicMcpForwardPort>,
        effects: tokio::sync::mpsc::UnboundedSender<PublicMcpHostEffect>,
    ) -> Self {
        Self {
            state: host.state.clone(),
            recordings: host.recordings.clone(),
            quick_commands: host.quick_commands.clone(),
            settings_path: host.settings_path.clone(),
            runtime_handles: host.runtime_handles.clone(),
            admission: host.admission.clone(),
            sync_handles: host.sync_handles.clone(),
            history: host.history.clone(),
            terminals: host.terminals.clone(),
            desktops: host.desktops.clone(),
            node_router,
            sftp_transfer_manager,
            forwarding_runtime,
            forwarding_service,
            effects,
        }
    }

    pub fn start_delivery(
        &self,
        mut receiver: DomainRequestReceiver,
    ) -> (
        tokio::task::JoinHandle<()>,
        tokio::sync::mpsc::Receiver<PublicMcpWindowDelivery>,
    ) {
        let (window_tx, window_rx) = tokio::sync::mpsc::channel(PUBLIC_MCP_BROKER_CAPACITY);
        let backend = self.clone();
        let task = self.forwarding_runtime.spawn(async move {
            while let Some(message) = receiver.recv().await {
                let (message, accepted) = match message {
                    DomainMessage::Request(request) => {
                        let backend = backend.clone();
                        let request =
                            match tokio::task::spawn_blocking(move || backend.dispatch(*request))
                                .await
                            {
                                Ok(Err(request)) => request,
                                Ok(Ok(())) | Err(_) => continue,
                            };
                        // A prepared addon can be tens of MiB. Only buffer one until the
                        // window has accepted or discarded it.
                        let (accepted, wait) =
                            if matches!(request, PublicMcpWindowRequest::InstallAddon { .. }) {
                                let (tx, rx) = tokio::sync::oneshot::channel();
                                (Some(tx), Some(rx))
                            } else {
                                (None, None)
                            };
                        (PublicMcpWindowDelivery::Request { request, accepted }, wait)
                    }
                    DomainMessage::StateChanged => (PublicMcpWindowDelivery::StateChanged, None),
                };
                if window_tx.send(message).await.is_err() {
                    break;
                }
                if let Some(accepted) = accepted
                    && accepted.await.is_err()
                {
                    break;
                }
            }
        });
        (task, window_rx)
    }

    /// Serializes admission with policy changes, not the lifetime of the remote job.
    pub fn dispatch(&self, request: DomainRequest) -> Result<(), PublicMcpWindowRequest> {
        let suspended = self.admission.lock();
        if request.is_cancelled() {
            return Ok(());
        }
        if *suspended {
            request.finish(ToolEnvelope::failed("The OxideTerm workspace is locked"));
            return Ok(());
        }
        if let PublicToolCall::AddonsInstall(args) = &request.call {
            // Preparation only reads a client-owned artifact. The window rechecks cancellation
            // and the application lock before retiring any plugin sessions.
            drop(suspended);
            let result = self.prepare_addon_package(&request, args);
            if request.is_cancelled() {
                return Ok(());
            }
            return match result {
                Ok(package) => Err(PublicMcpWindowRequest::InstallAddon {
                    request: Box::new(request),
                    package,
                }),
                Err(error) => {
                    request.finish(ToolEnvelope::failed(error));
                    Ok(())
                }
            };
        }
        let request = if let PublicToolCall::Revert(args) = &request.call {
            let call = PublicToolCall::SyncRestore(oxideterm_public_mcp::SyncRestoreArgs {
                undo_ref: args.undo_ref.clone(),
            });
            request.with_call(call)
        } else {
            request
        };
        let audit = request.audit_context.clone();
        oxideterm_audit::AuditContext::with_sync_request(audit.as_ref(), || {
            match &request.call {
                PublicToolCall::BrowseConnections(_) | PublicToolCall::DescribeConnection(_) => {
                    return Err(PublicMcpWindowRequest::ConnectionDirectory(Box::new(
                        request,
                    )));
                }
                PublicToolCall::TerminalHistory(args)
                    if !matches!(
                        args.request,
                        oxideterm_public_mcp::history::HistoryRequest::Snapshot { .. }
                    ) =>
                {
                    self.terminal_history(request, None)
                }
                PublicToolCall::OperationState(_) => self.handle_public_mcp_operation(request),
                PublicToolCall::CancelOperation(_) => {
                    self.handle_public_mcp_cancel_operation(request)
                }
                PublicToolCall::InspectNode(_) => self.handle_public_mcp_inspect_node(request),
                PublicToolCall::ReleaseNode(_) => self.handle_public_mcp_release_node(request),
                PublicToolCall::StartCommand(_) => self.handle_public_mcp_start_command(request),
                PublicToolCall::CommandState(_) => self.handle_public_mcp_command_state(request),
                PublicToolCall::CommandOutput(_) => self.handle_public_mcp_command_output(request),
                PublicToolCall::CancelCommand(_) => self.handle_public_mcp_cancel_command(request),
                PublicToolCall::StageArtifact(_) => self.handle_public_mcp_stage_artifact(request),
                PublicToolCall::ReadArtifact(_) => self.handle_public_mcp_read_artifact(request),
                PublicToolCall::AuditSearch(_) => self.handle_public_mcp_audit_search(request),
                PublicToolCall::HostToolsCatalog(_) => {
                    self.handle_public_mcp_host_tools_catalog(request)
                }
                PublicToolCall::HostToolsCapture(_) => {
                    self.handle_public_mcp_host_tools_capture(request)
                }
                PublicToolCall::HostToolsOperate(_) => {
                    self.handle_public_mcp_host_tools_operate(request)
                }
                PublicToolCall::PreparedQuickCommandRun(_) => {
                    self.handle_public_mcp_prepared_quick_command_run(request)
                }
                PublicToolCall::ForwardsList(_) => self.handle_public_mcp_forwards_list(request),
                PublicToolCall::ForwardsOpen(_) => self.handle_public_mcp_forwards_open(request),
                PublicToolCall::ForwardsChange(_) => {
                    self.handle_public_mcp_forwards_change(request)
                }
                PublicToolCall::ForwardsStop(_) => self.handle_public_mcp_forwards_stop(request),
                PublicToolCall::ForwardsRestart(_) => {
                    self.handle_public_mcp_forwards_restart(request)
                }
                PublicToolCall::ForwardsRemove(_) => {
                    self.handle_public_mcp_forwards_remove(request)
                }
                PublicToolCall::ForwardsMetrics(_) => {
                    self.handle_public_mcp_forwards_metrics(request)
                }
                PublicToolCall::ForwardsDiscoverPorts(_) => {
                    self.handle_public_mcp_forwards_discover_ports(request)
                }
                PublicToolCall::FilesOpen(_) => self.handle_public_mcp_files_open(request),
                PublicToolCall::FilesClose(_) => self.handle_public_mcp_files_close(request),
                PublicToolCall::FilesList(_) => self.handle_public_mcp_files_list(request),
                PublicToolCall::FilesStat(_) => self.handle_public_mcp_files_stat(request),
                PublicToolCall::FilesRead(_) => self.handle_public_mcp_files_read(request),
                PublicToolCall::FilesCompare(_) => self.handle_public_mcp_files_compare(request),
                PublicToolCall::FilesWrite(_) => self.handle_public_mcp_files_write(request),
                PublicToolCall::FilesMove(_) => self.handle_public_mcp_files_move(request),
                PublicToolCall::FilesRemove(_) => self.handle_public_mcp_files_remove(request),
                PublicToolCall::TransferStart(_) => self.handle_public_mcp_transfer_start(request),
                PublicToolCall::TransferStatus(_) => {
                    self.handle_public_mcp_transfer_status(request)
                }
                PublicToolCall::TransferCancel(_) => {
                    self.handle_public_mcp_transfer_cancel(request)
                }
                PublicToolCall::WorkspaceTree(_) => self.handle_public_mcp_workspace_tree(request),
                PublicToolCall::WorkspaceRead(_) => self.handle_public_mcp_workspace_read(request),
                PublicToolCall::WorkspaceApplyEdits(_) => {
                    self.handle_public_mcp_workspace_apply_edits(request)
                }
                PublicToolCall::WorkspaceSearch(_) => {
                    self.handle_public_mcp_workspace_search(request)
                }
                PublicToolCall::WorkspaceClose(_) => {
                    self.handle_public_mcp_workspace_close(request)
                }
                PublicToolCall::QuickCommandsList(_) => {
                    self.handle_public_mcp_quick_commands_list(request)
                }
                PublicToolCall::QuickCommandsDescribe(_) => {
                    self.handle_public_mcp_quick_commands_describe(request)
                }
                PublicToolCall::RecordingsSearch(_) => {
                    self.handle_public_mcp_recordings_search(request)
                }
                PublicToolCall::RecordingsExport(_) => {
                    self.handle_public_mcp_recordings_export(request)
                }
                PublicToolCall::RecordingsStatus(args) => {
                    if let RecordingStatusTarget::Recording { recording_ref } = &args.target {
                        let recording_ref = recording_ref.clone();
                        self.stopped_recording_status(request, recording_ref)
                            .map_err(|request| PublicMcpWindowRequest::Call(Box::new(request)))?;
                    } else {
                        return Err(PublicMcpWindowRequest::Call(Box::new(request)));
                    }
                }
                _ => return Err(PublicMcpWindowRequest::Call(Box::new(request))),
            }
            Ok(())
        })
    }

    pub(super) fn handle_public_mcp_prepared_quick_command_run(&self, request: DomainRequest) {
        let PublicToolCall::PreparedQuickCommandRun(args) = &request.call else {
            return;
        };
        let node_ref = args.node_ref.clone();
        // The approval store owns the frozen source; this bounded copy transfers it to the task.
        let command = Zeroizing::new(args.command.to_string());
        self.start_public_mcp_node_command(
            request,
            node_ref,
            command,
            ToolGroup::QuickCommandExecute,
        );
    }

    pub fn acquire_node_lease(
        &self,
        request: DomainRequest,
        node_id: NodeId,
        connection_id: String,
        chain: Option<tokio::sync::watch::Receiver<oxideterm_ssh::ConnectionChainProgress>>,
    ) {
        let node_ref = NodeRef::new();
        let lease = PublicMcpNodeRegistration {
            client_ref: request.client_ref.clone(),
            node_id: node_id.clone(),
            saved_connection_id: Some(connection_id),
            connection: None,
        };
        if !self
            .runtime_handles
            .lock()
            .reserve_node(node_ref.clone(), lease)
        {
            request.finish(ToolEnvelope::failed(
                "The retained SSH node lease limit has been reached",
            ));
            return;
        }
        if let Ok(ready) = self.node_router.resolve_connection_now(&node_id) {
            self.rebind_node_leases(&node_id, &ready.connection_id);
        }
        let router = self.node_router.clone();
        let handles = self.runtime_handles.clone();
        let request_cancellation = request.cancellation_token();
        let audit_context = request.audit_context.clone();
        self.forwarding_runtime
            .spawn(oxideterm_audit::AuditContext::scope_optional(
                audit_context,
                async move {
                    let acquired = tokio::select! {
                        _ = request_cancellation.cancelled() => {
                            handles.lock().nodes.remove(&node_ref);
                            return;
                        }
                        result = acquire_ready_target(&router, &node_id, ConnectionConsumer::PublicMcp(format!("connecting:{node_ref}")), chain) => result,
                    };
                    match acquired {
                        Ok(handoff) => {
                            if request_cancellation.is_cancelled() {
                                handles.lock().nodes.remove(&node_ref);
                                return;
                            }
                            let mut handles = handles.lock();
                            let Some(registration) = handles.nodes.get_mut(&node_ref) else {
                                request.finish(ToolEnvelope::failed(
                                    "The MCP client was revoked while connecting",
                                ));
                                return;
                            };
                            // A reconnect may have completed while this worker was waiting for the chain.
                            if registration.bind_ready_connection(&router, &node_ref).is_err() {
                                handles.nodes.remove(&node_ref);
                                request.finish(ToolEnvelope::failed("The SSH node did not become ready"));
                                return;
                            }
                            drop(handles);
                            drop(handoff);
                            finish_serialized(
                                request,
                                json!({ "node_ref": node_ref, "state": "ready" }),
                            );
                        }
                        Err(message) => {
                            handles.lock().nodes.remove(&node_ref);
                            request.finish(ToolEnvelope::failed(message));
                        }
                    }
                },
            ));
    }

    pub fn rebind_node_leases(&self, node: &NodeId, connection_id: &str) {
        let mut handles = self.runtime_handles.lock();
        if self.node_router.connection_id_for_node(node).as_deref() != Some(connection_id) {
            return;
        }
        for (node_ref, registration) in &mut handles.nodes {
            if &registration.node_id == node {
                let _ = registration.bind_ready_connection(&self.node_router, node_ref);
            }
        }
        for record in handles
            .file_sessions
            .values_mut()
            .filter(|record| &record.node_id == node && record.root.is_some())
        {
            if record
                .route
                .as_ref()
                .is_some_and(|lease| lease.connection_id() == connection_id)
            {
                continue;
            }
            if let Ok(lease) = self.node_router.lease_connection_now(
                node,
                ConnectionConsumer::Sftp(FileSessionRef::new().to_string()),
            ) {
                // The public file handle survives its node_ref and reopens its channel on demand.
                record.route = Some(Arc::new(lease));
                record.session = None;
            }
        }
    }

    pub fn invalidate_disconnected_nodes(&self, disconnected: &[NodeId]) -> usize {
        let mut handles = self.runtime_handles.lock();
        let disconnected_node_refs = handles
            .nodes
            .iter()
            .filter_map(|(node_ref, candidate)| {
                disconnected
                    .contains(&candidate.node_id)
                    .then_some(node_ref.clone())
            })
            .collect::<HashSet<_>>();
        for node_ref in &disconnected_node_refs {
            handles.nodes.remove(node_ref);
        }
        let command_refs = handles
            .commands
            .iter()
            .filter_map(|(command_ref, record)| {
                disconnected_node_refs
                    .contains(&record.node_ref)
                    .then_some(command_ref.clone())
            })
            .collect::<Vec<_>>();
        remove_command_operations(&mut handles, &command_refs);
        let cancellations = command_refs
            .into_iter()
            .filter_map(|command_ref| handles.commands.remove(&command_ref))
            .map(|record| record.cancellation)
            .collect::<Vec<_>>();
        let interrupted_transfers =
            transfers::invalidate_for_disconnected_nodes(&mut handles, disconnected);
        forwards::invalidate_for_disconnected_nodes(&mut handles, disconnected);
        let file_sessions = files::take_disconnected_file_sessions(&mut handles, disconnected);
        let disconnected_workspaces =
            workspaces::take_disconnected_workspaces(&mut handles, disconnected);
        drop(handles);
        drop(file_sessions);
        for cancellation in cancellations {
            cancellation.cancel();
        }
        for transfer_id in interrupted_transfers {
            self.sftp_transfer_manager.cancel(&transfer_id);
        }
        for record in disconnected_workspaces {
            record.revoke();
        }
        disconnected_node_refs.len()
    }

    fn handle_public_mcp_operation(&self, request: DomainRequest) {
        let PublicToolCall::OperationState(args) = &request.call else {
            return;
        };
        let operation_ref = args.operation_ref.clone();
        let mut handles = self.runtime_handles.lock();
        let Some(operation) = handles
            .operations
            .get(&operation_ref)
            .filter(|operation| operation.client_ref == request.client_ref)
            .cloned()
        else {
            request.finish(ToolEnvelope::failed(
                "The background operation handle is unavailable",
            ));
            return;
        };
        if matches!(&operation.target, PublicMcpOperationTarget::Transfer(_)) {
            transfers::expire_transfer_records(&mut handles);
        }
        // A Basic-group operation handle never bypasses the group that created the operation.
        let group_enabled = self
            .state
            .clients
            .get(&request.client_ref)
            .is_some_and(|client| client.tool_groups.contains(&operation.owner_group));
        if !group_enabled {
            request.finish(ToolEnvelope::failed(
                "The operation's tool group is disabled",
            ));
            return;
        }
        let projection = match operation.target {
            PublicMcpOperationTarget::Command(command_ref) => {
                handles.commands.get(&command_ref).map(|record| {
                    let stage = match record.state {
                        PublicMcpCommandState::Running => "running",
                        PublicMcpCommandState::Succeeded => "completed",
                        PublicMcpCommandState::Failed => "failed",
                        PublicMcpCommandState::Cancelled => "cancelled",
                    };
                    json!({
                        "operation_ref": operation_ref,
                        "kind": "command",
                        "stage": stage,
                        "cancellable": record.state == PublicMcpCommandState::Running,
                        "command_ref": command_ref,
                        "exit_code": record.exit_code,
                        "truncated": record.truncated,
                        "error": record.error,
                    })
                })
            }
            PublicMcpOperationTarget::Transfer(transfer_ref) => {
                handles.transfers.get(&transfer_ref).map(|record| {
                    json!({
                        "operation_ref": operation_ref,
                        "kind": "transfer",
                        "stage": record.state,
                        "cancellable": !record.state.is_finished(),
                        "transfer_ref": transfer_ref,
                        "progress": {
                            "completed_bytes": record.transferred_bytes,
                            "total_bytes": record.total_bytes,
                            "speed_bytes_per_second": record.speed_bytes_per_second,
                        },
                        "artifact": record.artifact,
                        "error_code": record.error_code,
                        "remote_residue": record.remote_residue,
                    })
                })
            }
        };
        match projection {
            Some(projection) => finish_serialized(request, projection),
            None => {
                handles.operations.remove(&operation_ref);
                request.finish(ToolEnvelope::failed(
                    "The background operation result has expired",
                ));
            }
        }
    }

    fn handle_public_mcp_cancel_operation(&self, request: DomainRequest) {
        let PublicToolCall::CancelOperation(args) = &request.call else {
            return;
        };
        let operation_ref = args.operation_ref.clone();
        let mut handles = self.runtime_handles.lock();
        let Some(operation) = handles
            .operations
            .get(&operation_ref)
            .filter(|operation| operation.client_ref == request.client_ref)
            .cloned()
        else {
            request.finish(ToolEnvelope::failed(
                "The background operation handle is unavailable",
            ));
            return;
        };
        if matches!(&operation.target, PublicMcpOperationTarget::Transfer(_)) {
            transfers::expire_transfer_records(&mut handles);
        }
        let group_enabled = self
            .state
            .clients
            .get(&request.client_ref)
            .is_some_and(|client| client.tool_groups.contains(&operation.owner_group));
        if !group_enabled {
            request.finish(ToolEnvelope::failed(
                "The operation's tool group is disabled",
            ));
            return;
        }
        match operation.target {
            PublicMcpOperationTarget::Command(command_ref) => {
                let Some(record) = handles.commands.get_mut(&command_ref) else {
                    handles.operations.remove(&operation_ref);
                    request.finish(ToolEnvelope::failed(
                        "The background operation result has expired",
                    ));
                    return;
                };
                let cancel_requested = record.state == PublicMcpCommandState::Running;
                if cancel_requested {
                    record.cancellation.cancel();
                    record.state = PublicMcpCommandState::Cancelled;
                }
                drop(handles);
                if cancel_requested {
                    self.schedule_public_mcp_command_expiry(command_ref);
                }
                finish_serialized(
                    request,
                    json!({
                        "operation_ref": operation_ref,
                        "cancel_requested": cancel_requested,
                        "side_effects_may_remain": cancel_requested,
                        "undo_ref": null,
                    }),
                );
            }
            PublicMcpOperationTarget::Transfer(transfer_ref) => {
                let Some(record) = handles.transfers.get(&transfer_ref) else {
                    handles.operations.remove(&operation_ref);
                    request.finish(ToolEnvelope::failed(
                        "The background operation result has expired",
                    ));
                    return;
                };
                let internal_id = (!record.state.is_finished()).then(|| record.internal_id.clone());
                let side_effects_may_remain =
                    record.direction == "upload" && record.transferred_bytes > 0;
                drop(handles);
                let cancel_requested = internal_id
                    .as_deref()
                    .is_some_and(|internal_id| self.sftp_transfer_manager.cancel(internal_id));
                finish_serialized(
                    request,
                    json!({
                        "operation_ref": operation_ref,
                        "cancel_requested": cancel_requested,
                        "side_effects_may_remain": side_effects_may_remain,
                        "undo_ref": null,
                    }),
                );
            }
        }
    }

    fn handle_public_mcp_inspect_node(&self, request: DomainRequest) {
        let PublicToolCall::InspectNode(args) = &request.call else {
            return;
        };
        let Some(lease) =
            node_lease_for_client(&self.runtime_handles, &request.client_ref, &args.node_ref)
        else {
            request.finish(ToolEnvelope::failed("The node handle is unavailable"));
            return;
        };
        let state = self.node_router.node_state(&lease.node_id);
        let metadata = self.node_router.node_metadata(&lease.node_id);
        let node_ref = args.node_ref.clone();
        match (state, metadata) {
            (Ok(state), Some(metadata)) => finish_serialized(
                request,
                json!({
                    "node_ref": node_ref,
                    "readiness": state.state.readiness,
                    "host": metadata.host,
                    "port": metadata.port,
                    "username": metadata.username,
                }),
            ),
            (Err(_), _) => request.finish(ToolEnvelope::failed("The node state is unavailable")),
            (_, None) => request.finish(ToolEnvelope::failed("The node no longer exists")),
        }
    }

    fn handle_public_mcp_release_node(&self, request: DomainRequest) {
        let PublicToolCall::ReleaseNode(args) = &request.call else {
            return;
        };
        let (lease, cancellations) = {
            let mut handles = self.runtime_handles.lock();
            let owned = handles
                .nodes
                .get(&args.node_ref)
                .is_some_and(|lease| lease.client_ref == request.client_ref);
            if !owned {
                drop(handles);
                request.finish(ToolEnvelope::failed("The node handle is unavailable"));
                return;
            }
            let lease = handles
                .nodes
                .remove(&args.node_ref)
                .expect("node lease ownership was checked");
            let command_refs = handles
                .commands
                .iter()
                .filter_map(|(command_ref, record)| {
                    (record.client_ref == request.client_ref && record.node_ref == args.node_ref)
                        .then_some(command_ref.clone())
                })
                .collect::<Vec<_>>();
            remove_command_operations(&mut handles, &command_refs);
            let cancellations = command_refs
                .into_iter()
                .filter_map(|command_ref| handles.commands.remove(&command_ref))
                .map(|record| record.cancellation)
                .collect::<Vec<_>>();
            (lease, cancellations)
        };
        for cancellation in cancellations {
            cancellation.cancel();
        }
        drop(lease);
        finish_serialized(
            request,
            json!({ "released": true, "physical_node_disconnected": false }),
        );
    }

    fn handle_public_mcp_start_command(&self, request: DomainRequest) {
        let PublicToolCall::StartCommand(args) = &request.call else {
            return;
        };
        let node_ref = args.node_ref.clone();
        let command = command_for_working_directory(
            &args.command,
            args.working_directory
                .as_ref()
                .map(|directory| directory.as_str()),
        );
        self.start_public_mcp_node_command(request, node_ref, command, ToolGroup::CommandExecute);
    }

    pub fn start_public_mcp_node_command(
        &self,
        request: DomainRequest,
        node_ref: NodeRef,
        command: Zeroizing<String>,
        owner_group: ToolGroup,
    ) {
        let mut handles = self.runtime_handles.lock();
        let Some(node_id) = handles
            .nodes
            .get(&node_ref)
            .filter(|node| node.client_ref == request.client_ref)
            .map(|node| node.node_id.clone())
        else {
            drop(handles);
            request.finish(ToolEnvelope::failed("The node handle is unavailable"));
            return;
        };
        let command_ref = CommandRef::new();
        let operation_ref = OperationRef::new();
        let cancellation = CancellationToken::new();
        let client_command_count = handles
            .commands
            .values()
            .filter(|record| record.client_ref == request.client_ref)
            .count();
        if handles.commands.len() >= PUBLIC_MCP_COMMAND_CAPACITY
            || client_command_count >= PUBLIC_MCP_COMMAND_CAPACITY_PER_CLIENT
        {
            drop(handles);
            request.finish(ToolEnvelope::failed(
                "The retained command limit was reached; wait for old results to expire or release an unused node lease",
            ));
            return;
        }
        handles.commands.insert(
            command_ref.clone(),
            PublicMcpCommandRecord {
                client_ref: request.client_ref.clone(),
                node_ref,
                owner_group,
                state: PublicMcpCommandState::Running,
                stdout: Zeroizing::new(Vec::new()),
                stderr: Zeroizing::new(Vec::new()),
                exit_code: None,
                truncated: false,
                error: None,
                cancellation: cancellation.clone(),
            },
        );
        handles.operations.insert(
            operation_ref.clone(),
            PublicMcpOperationRecord {
                client_ref: request.client_ref.clone(),
                owner_group,
                target: PublicMcpOperationTarget::Command(command_ref.clone()),
            },
        );
        drop(handles);

        let router = self.node_router.clone();
        let handles = self.runtime_handles.clone();
        let command_ref_for_task = command_ref.clone();
        let audit_context = request.audit_context.clone();
        self.forwarding_runtime
            .spawn(oxideterm_audit::AuditContext::scope_optional(
                audit_context,
                async move {
                    let resolved = tokio::select! {
                        _ = cancellation.cancelled() => return,
                        result = router.resolve_connection(&node_id) => result,
                    };
                    let result = match resolved {
                        Ok(resolved) => {
                            tokio::select! {
                                _ = cancellation.cancelled() => None,
                                result = resolved.handle.run_secret_command_capture(
                                    command.as_str(),
                                    PUBLIC_MCP_COMMAND_TIMEOUT,
                                    PUBLIC_MCP_COMMAND_OUTPUT_LIMIT,
                                ) => Some(result.map_err(public_command_error)),
                            }
                        }
                        Err(_) => Some(Err("The SSH node is no longer ready".to_owned())),
                    };
                    let Some(result) = result else {
                        return;
                    };
                    // The retained result never needs the submitted command text.
                    drop(command);
                    {
                        let mut runtime_handles = handles.lock();
                        let Some(record) = runtime_handles.commands.get_mut(&command_ref_for_task)
                        else {
                            return;
                        };
                        if record.state != PublicMcpCommandState::Running {
                            return;
                        }
                        match result {
                            Ok(output) => {
                                record.stdout = output.stdout;
                                record.stderr = output.stderr;
                                record.exit_code = output.exit_code;
                                record.truncated = output.truncated;
                                if output.exit_code == Some(0) {
                                    record.state = PublicMcpCommandState::Succeeded;
                                } else {
                                    record.state = PublicMcpCommandState::Failed;
                                    record.error = Some(match output.exit_code {
                                        Some(exit_code) => {
                                            format!("Remote command exited with status {exit_code}")
                                        }
                                        None => {
                                            "Remote command ended without an exit status".to_owned()
                                        }
                                    });
                                }
                            }
                            Err(error) => {
                                record.error = Some(error);
                                record.state = PublicMcpCommandState::Failed;
                            }
                        }
                    }
                    expire_public_mcp_command_after_retention(handles, command_ref_for_task).await;
                },
            ));
        finish_serialized(
            request,
            json!({
                "command_ref": command_ref,
                "operation_ref": operation_ref,
                "state": "running",
            }),
        );
    }

    fn handle_public_mcp_command_state(&self, request: DomainRequest) {
        let PublicToolCall::CommandState(args) = &request.call else {
            return;
        };
        let handles = self.runtime_handles.lock();
        let Some(record) = handles
            .commands
            .get(&args.command_ref)
            .filter(|record| record.client_ref == request.client_ref)
        else {
            request.finish(ToolEnvelope::failed("The command handle is unavailable"));
            return;
        };
        let command_ref = args.command_ref.clone();
        let error = record.error.clone();
        finish_serialized(
            request,
            json!({
                "command_ref": command_ref,
                "state": record.state,
                "exit_code": record.exit_code,
                "truncated": record.truncated,
                "error": error,
            }),
        );
    }

    fn handle_public_mcp_command_output(&self, request: DomainRequest) {
        let PublicToolCall::CommandOutput(args) = &request.call else {
            return;
        };
        let handles = self.runtime_handles.lock();
        let Some(record) = handles
            .commands
            .get(&args.command_ref)
            .filter(|record| record.client_ref == request.client_ref)
        else {
            request.finish(ToolEnvelope::failed("The command handle is unavailable"));
            return;
        };
        let offset = usize::try_from(args.offset).unwrap_or(usize::MAX);
        let limit = usize::try_from(args.limit)
            .unwrap_or(PUBLIC_MCP_OUTPUT_PAGE_LIMIT)
            .min(PUBLIC_MCP_OUTPUT_PAGE_LIMIT);
        let stdout = output_page(&record.stdout, offset, limit);
        let stderr = output_page(&record.stderr, offset, limit);
        let command_ref = args.command_ref.clone();
        finish_serialized(
            request,
            json!({
                "command_ref": command_ref,
                "state": record.state,
                "offset": offset,
                "stdout": stdout,
                "stderr": stderr,
                "stdout_size": record.stdout.len(),
                "stderr_size": record.stderr.len(),
                "truncated": record.truncated,
            }),
        );
    }

    fn handle_public_mcp_cancel_command(&self, request: DomainRequest) {
        let PublicToolCall::CancelCommand(args) = &request.call else {
            return;
        };
        let command_ref = args.command_ref.clone();
        let mut handles = self.runtime_handles.lock();
        let Some(record) = handles
            .commands
            .get_mut(&command_ref)
            .filter(|record| record.client_ref == request.client_ref)
        else {
            request.finish(ToolEnvelope::failed("The command handle is unavailable"));
            return;
        };
        let cancelled = record.state == PublicMcpCommandState::Running;
        if cancelled {
            record.cancellation.cancel();
            record.state = PublicMcpCommandState::Cancelled;
        }
        drop(handles);
        if cancelled {
            self.schedule_public_mcp_command_expiry(command_ref);
        }
        finish_serialized(request, json!({ "cancelled": cancelled }));
    }

    fn schedule_public_mcp_command_expiry(&self, command_ref: CommandRef) {
        let handles = self.runtime_handles.clone();
        self.forwarding_runtime
            .spawn(expire_public_mcp_command_after_retention(
                handles,
                command_ref,
            ));
    }

    fn handle_public_mcp_stage_artifact(&self, request: DomainRequest) {
        let PublicToolCall::StageArtifact(args) = &request.call else {
            return;
        };
        match self.state.artifacts.stage(
            request.client_ref.clone(),
            &args.bytes,
            args.media_type.clone(),
            args.name.clone(),
        ) {
            Ok(artifact) => finish_serialized(request, json!({ "artifact": artifact })),
            Err(error) => request.finish(ToolEnvelope::failed(error.to_string())),
        }
    }

    fn handle_public_mcp_read_artifact(&self, request: DomainRequest) {
        let PublicToolCall::ReadArtifact(args) = &request.call else {
            return;
        };
        match self.state.artifacts.read(
            &request.client_ref,
            &args.artifact_ref,
            args.offset,
            args.length,
        ) {
            Ok(page) => {
                // Only the requested bounded page crosses the protocol boundary.
                let bytes_base64 =
                    base64::engine::general_purpose::STANDARD.encode(page.bytes.as_slice());
                finish_serialized(
                    request,
                    json!({
                        "artifact": page.projection,
                        "offset": page.offset,
                        "bytes_base64": bytes_base64,
                        "next_offset": page.next_offset,
                    }),
                );
            }
            Err(error) => request.finish(ToolEnvelope::failed(error.to_string())),
        }
    }

    fn handle_public_mcp_audit_search(&self, request: DomainRequest) {
        let PublicToolCall::AuditSearch(args) = &request.call else {
            return;
        };
        let page = self.state.audit.search(
            &request.client_ref,
            AuditQuery {
                after_ms: args.after_ms,
                before_ms: args.before_ms,
                tool_name: args.tool.as_deref(),
                target: args.target_ref.as_deref(),
                cursor: args.cursor.as_ref(),
                limit: args.limit as usize,
            },
        );
        finish_serialized(request, json!(page));
    }

    pub fn revoke_public_mcp_client_file_sessions(&self, client_ref: &ClientRef) {
        self.revoke_public_mcp_client_workspaces(client_ref);
        self.revoke_public_mcp_client_transfers(client_ref);
        drop(files::take_client_file_sessions(
            &self.runtime_handles,
            client_ref,
        ));
    }

    pub fn revoke_public_mcp_client_workspaces(&self, client_ref: &ClientRef) {
        for record in workspaces::take_client_workspaces(&self.runtime_handles, client_ref) {
            record.revoke();
        }
    }

    pub fn cancel_public_mcp_client_workspace_edits(&self, client_ref: &ClientRef) {
        for record in self
            .runtime_handles
            .lock()
            .workspaces
            .values()
            .filter(|record| record.client_ref == *client_ref)
        {
            record.edit_cancellation.cancel();
        }
    }

    pub fn reset_public_mcp_client_workspace_edit_cancellation(&self, client_ref: &ClientRef) {
        for record in self
            .runtime_handles
            .lock()
            .workspaces
            .values_mut()
            .filter(|record| record.client_ref == *client_ref)
        {
            record.edit_cancellation = CancellationToken::new();
        }
    }
}

pub(crate) fn remove_command_operations(
    handles: &mut PublicMcpRuntimeHandles,
    command_refs: &[CommandRef],
) {
    // Generic handles cannot outlive their typed command records.
    handles.operations.retain(|_, operation| {
        !matches!(
            &operation.target,
            PublicMcpOperationTarget::Command(command_ref)
                if command_refs.contains(command_ref)
        )
    });
}

pub(crate) async fn expire_public_mcp_command_after_retention(
    handles: Arc<Mutex<PublicMcpRuntimeHandles>>,
    command_ref: CommandRef,
) {
    tokio::time::sleep(PUBLIC_MCP_COMMAND_RETENTION).await;
    let mut handles = handles.lock();
    let is_finished = handles
        .commands
        .get(&command_ref)
        .is_some_and(|record| record.state != PublicMcpCommandState::Running);
    if !is_finished {
        return;
    }
    remove_command_operations(&mut handles, std::slice::from_ref(&command_ref));
    handles.commands.remove(&command_ref);
}

pub(crate) fn remove_transfer_operations(
    handles: &mut PublicMcpRuntimeHandles,
    transfer_refs: &[TransferRef],
) {
    // Generic handles cannot outlive their typed transfer records.
    handles.operations.retain(|_, operation| {
        !matches!(
            &operation.target,
            PublicMcpOperationTarget::Transfer(transfer_ref)
                if transfer_refs.contains(transfer_ref)
        )
    });
}

pub(crate) fn node_lease_for_client(
    handles: &Arc<Mutex<PublicMcpRuntimeHandles>>,
    client_ref: &ClientRef,
    node_ref: &NodeRef,
) -> Option<PublicMcpNodeLease> {
    handles
        .lock()
        .nodes
        .get(node_ref)
        .filter(|lease| &lease.client_ref == client_ref)
        .map(|lease| PublicMcpNodeLease {
            client_ref: lease.client_ref.clone(),
            node_id: lease.node_id.clone(),
            saved_connection_id: lease.saved_connection_id.clone(),
            physical_connection_id: lease
                .connection
                .as_ref()
                .map(|owner| owner.connection_id().to_owned()),
            consumer: ConnectionConsumer::PublicMcp(node_ref.to_string()),
        })
}

pub(crate) fn connection_directory_matches_query(connection: &ConnectionInfo, query: &str) -> bool {
    query.is_empty()
        || connection.name.to_lowercase().contains(query)
        || connection.host.to_lowercase().contains(query)
        || connection.username.to_lowercase().contains(query)
        || connection
            .group
            .as_deref()
            .is_some_and(|group| group.to_lowercase().contains(query))
        || connection
            .tags
            .iter()
            .any(|tag| tag.to_lowercase().contains(query))
}

pub(crate) fn public_profile_matches_query(
    name: &str,
    group: Option<&str>,
    searchable_fields: &[&str],
    query: &str,
) -> bool {
    query.is_empty()
        || name.to_lowercase().contains(query)
        || group.is_some_and(|group| group.to_lowercase().contains(query))
        || searchable_fields
            .iter()
            .any(|value| value.to_lowercase().contains(query))
}

pub(crate) fn command_for_working_directory(
    command: &Zeroizing<String>,
    working_directory: Option<&str>,
) -> Zeroizing<String> {
    let Some(working_directory) = working_directory.filter(|directory| !directory.is_empty())
    else {
        return Zeroizing::new(command.to_string());
    };
    let quoted_directory = shell_single_quote(working_directory);
    Zeroizing::new(format!(
        "cd -- {} && {}",
        quoted_directory.as_str(),
        command.as_str()
    ))
}

pub(crate) fn shell_single_quote(value: &str) -> Zeroizing<String> {
    // POSIX shells represent one literal quote by ending, escaping, and reopening the quote.
    let mut quoted = String::with_capacity(value.len().saturating_add(2));
    quoted.push('\'');
    for character in value.chars() {
        if character == '\'' {
            quoted.push_str("'\\''");
        } else {
            quoted.push(character);
        }
    }
    quoted.push('\'');
    Zeroizing::new(quoted)
}

pub(crate) fn output_page(bytes: &[u8], offset: usize, limit: usize) -> String {
    let start = offset.min(bytes.len());
    let end = start.saturating_add(limit).min(bytes.len());
    String::from_utf8_lossy(&bytes[start..end]).into_owned()
}

pub(crate) fn public_command_error(error: SshTransportError) -> String {
    // Public errors expose an actionable category without forwarding transport internals.
    match error {
        SshTransportError::CommandChannelUnavailable => {
            "No verified SSH command channel is available".to_owned()
        }
        SshTransportError::CommandRejected => {
            "The server rejected the command request".to_owned()
        }
        SshTransportError::CommandOutcomeUnknown => {
            "The command may have executed, but its result could not be confirmed; check the remote state before retrying".to_owned()
        }
        SshTransportError::Timeout => "The remote command timed out".to_owned(),
        SshTransportError::SampleOutputLimit { .. } => "Sample output exceeded the size limit".to_owned(),
        SshTransportError::DnsResolution { .. } => {
            "The SSH host name could not be resolved".to_owned()
        }
        SshTransportError::AuthenticationFailed(_) | SshTransportError::UnsupportedAuth(_) => {
            "SSH authentication is unavailable for this command".to_owned()
        }
        SshTransportError::HostKeyUnknown { .. }
        | SshTransportError::HostKeyChanged { .. }
        | SshTransportError::HostKeyCheckFailed(_) => {
            "SSH host key verification requires attention in OxideTerm".to_owned()
        }
        SshTransportError::AlgorithmNegotiationFailed { .. } => {
            "SSH algorithm negotiation failed".to_owned()
        }
        SshTransportError::ConnectionFailed(_)
        | SshTransportError::Protocol(_)
        | SshTransportError::PreflightComplete
        | SshTransportError::Channel(_) => "The remote command could not be completed".to_owned(),
    }
}
