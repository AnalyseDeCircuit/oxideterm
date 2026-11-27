use super::*;
use oxideterm_forwarding::{ForwardRule, ForwardStats, ForwardUpdate, PortDetectionSnapshot};
use oxideterm_ssh::{ConnectionPoolConfig, SshConnectionRegistry};

struct NoForwards;

#[async_trait::async_trait]
impl PublicMcpForwardPort for NoForwards {
    fn public_mcp_rules_for_node(&self, _node_id: &NodeId) -> Vec<ForwardRule> {
        unreachable!("the request must not reach forwarding")
    }

    fn public_mcp_forward_is_persisted(&self, _forward_id: &str) -> bool {
        unreachable!("the request must not reach forwarding")
    }

    fn public_mcp_forward_owner_connection_id(&self, _forward_id: &str) -> Option<String> {
        unreachable!("the request must not reach forwarding")
    }

    async fn public_mcp_open_forward(
        &self,
        _node_id: &NodeId,
        _owner_connection_id: Option<&str>,
        _rule: ForwardRule,
        _check_health: bool,
        _persist: bool,
    ) -> Result<PublicMcpForwardMutation, String> {
        unreachable!("the request must not reach forwarding")
    }

    async fn public_mcp_change_forward(
        &self,
        _node_id: &NodeId,
        _owner_connection_id: Option<&str>,
        _forward_id: &str,
        _expected_rule: &ForwardRule,
        _update: ForwardUpdate,
        _persist: bool,
    ) -> Result<PublicMcpForwardMutation, String> {
        unreachable!("the request must not reach forwarding")
    }

    async fn public_mcp_stop_forward(
        &self,
        _node_id: &NodeId,
        _owner_connection_id: Option<&str>,
        _forward_id: &str,
    ) -> Result<(ForwardRule, PublicMcpForwardStopHandoff), String> {
        unreachable!("the request must not reach forwarding")
    }

    async fn public_mcp_restart_forward(
        &self,
        _node_id: &NodeId,
        _owner_connection_id: Option<&str>,
        _forward_id: &str,
        _persist: bool,
    ) -> Result<PublicMcpForwardMutation, String> {
        unreachable!("the request must not reach forwarding")
    }

    async fn public_mcp_remove_forward(
        &self,
        _node_id: &NodeId,
        _owner_connection_id: Option<&str>,
        _forward_id: &str,
        _remove_saved: bool,
    ) -> Result<bool, String> {
        unreachable!("the request must not reach forwarding")
    }

    fn public_mcp_forward_stats(
        &self,
        _node_id: &NodeId,
        _forward_id: &str,
    ) -> Result<ForwardStats, String> {
        unreachable!("the request must not reach forwarding")
    }

    async fn public_mcp_discover_ports(
        &self,
        _node_id: &NodeId,
        _owner_connection_id: Option<&str>,
    ) -> Result<PortDetectionSnapshot, String> {
        unreachable!("the request must not reach forwarding")
    }

    async fn public_mcp_revoke_forward(
        &self,
        _node_id: &NodeId,
        _forward_id: &str,
        _persisted: bool,
    ) {
        unreachable!("the request must not reach forwarding")
    }

    async fn public_mcp_restore_forward_after_revocation(
        &self,
        _node_id: &NodeId,
        _owner_connection_id: Option<&str>,
        _original_rule: &ForwardRule,
        _revoked_rule: &ForwardRule,
        _stop_handoff: Option<&PublicMcpForwardStopHandoff>,
        _created_by_client: bool,
        _persisted: bool,
    ) -> bool {
        unreachable!("the request must not reach forwarding")
    }
}

fn router() -> NodeRouter {
    NodeRouter::new(SshConnectionRegistry::new(ConnectionPoolConfig::default()))
}

#[tokio::test]
async fn cold_jump_chain_returns_only_the_ready_target_lease() {
    use oxideterm_ssh::{
        ConnectionChainProgress, ConnectionState, ConnectionTraceStage, ConnectionTraceStatus,
        SshConfig,
    };

    let (_dir, host, mut backend, client, mut requests) =
        dispatch_fixture(&[ToolGroup::NodeSession]);
    let registry = SshConnectionRegistry::default();
    registry.set_idle_timeout(Some(Duration::ZERO));
    backend.node_router = NodeRouter::new(registry.clone());
    let jump = NodeId::new("jump");
    let target = NodeId::new("target");
    let config = |host: &str| SshConfig {
        host: host.into(),
        ..Default::default()
    };
    let router = &backend.node_router;
    router.upsert_node(jump.clone(), config("jump.invalid"));
    router
        .runtime_store()
        .upsert_child_node(jump.clone(), target.clone(), config("target.invalid"))
        .unwrap();
    let call = || {
        PublicToolCall::ConnectNode(oxideterm_public_mcp::calls::ConnectNodeArgs {
            connection_ref: ConnectionRef::new(),
        })
    };
    let (request, mut response) = queued_request(&host, &mut requests, &client, call()).await;
    let (tx, rx) = tokio::sync::watch::channel(ConnectionChainProgress {
        stage: ConnectionTraceStage::SshHandshake,
        ..ConnectionChainProgress::queued(1)
    });
    backend.acquire_node_lease(request, target.clone(), "saved".into(), Some(rx));
    // The first hop is held at its handshake; the child has no physical id yet.
    let pending = Duration::from_millis(30);
    assert!(tokio::time::timeout(pending, &mut response).await.is_err());
    let bind = |node: &NodeId, name| {
        let handle = registry.acquire(config(name), ConnectionConsumer::NodeRouter(node.0.clone()));
        handle.set_physical(Arc::new(()));
        registry.mark_state(handle.connection_id(), ConnectionState::Active);
        router
            .bind_connection(node, handle.connection_id())
            .unwrap();
        handle
    };
    let parent_handle = bind(&jump, "jump.invalid");
    tx.send_replace(ConnectionChainProgress::queued(2));
    assert!(tokio::time::timeout(pending, &mut response).await.is_err());
    let target_handle = bind(&target, "target.invalid");
    let delivery = router
        .lease_connection_handoff(target_handle.connection_id())
        .unwrap();
    backend.rebind_node_leases(&target, target_handle.connection_id());
    drop(delivery);
    assert_eq!(target_handle.state(), ConnectionState::Active);
    tx.send_replace(ConnectionChainProgress {
        hop: 2,
        stage: ConnectionTraceStage::Ready,
        status: ConnectionTraceStatus::Ready,
    });
    let result = response.await.unwrap();
    let node_ref: NodeRef =
        serde_json::from_value(result.data.unwrap()["node_ref"].clone()).unwrap();
    let lease = host.node_lease(&client, &node_ref).unwrap();
    assert_eq!(lease.node_id, target);
    assert_eq!(
        lease.physical_connection_id.as_deref(),
        Some(target_handle.connection_id())
    );
    assert_eq!(
        parent_handle.info().consumers,
        vec![ConnectionConsumer::NodeRouter(jump.0)]
    );
    let retained = vec![
        ConnectionConsumer::NodeRouter(target.0.clone()),
        lease.consumer.clone(),
    ];
    assert_eq!(target_handle.info().consumers, retained);

    let (request, response) = queued_request(&host, &mut requests, &client, call()).await;
    let (tx, rx) = tokio::sync::watch::channel(ConnectionChainProgress::queued(1));
    backend.acquire_node_lease(request, target.clone(), "saved".into(), Some(rx));
    // An already-ready target also needs protection while the new request waits on its chain.
    assert_eq!(target_handle.info().ref_count, 3);
    tx.send_replace(ConnectionChainProgress {
        hop: 1,
        stage: ConnectionTraceStage::Authentication,
        status: ConnectionTraceStatus::Failed,
    });
    assert_eq!(
        response.await.unwrap().error.as_deref(),
        Some("SSH hop 1 failed during Authentication")
    );
    assert_eq!(host.runtime_handles.lock().nodes.len(), 1);
    assert_eq!(target_handle.info().consumers, retained);

    let replacement = bind(&target, "reconnected.invalid");
    let delivery = router
        .lease_connection_handoff(replacement.connection_id())
        .unwrap();
    backend.rebind_node_leases(&target, replacement.connection_id());
    backend.rebind_node_leases(&target, target_handle.connection_id());
    drop(delivery);
    assert_eq!(replacement.info().consumers, retained);
    assert!(!target_handle.info().consumers.contains(&lease.consumer));
    let (request, response) = queued_request(
        &host,
        &mut requests,
        &client,
        PublicToolCall::ReleaseNode(oxideterm_public_mcp::calls::ReleaseNodeArgs { node_ref }),
    )
    .await;
    assert!(backend.dispatch(request).is_ok());
    assert_eq!(response.await.unwrap().data.unwrap()["released"], true);
    assert_eq!(
        replacement.info().consumers,
        [ConnectionConsumer::NodeRouter(target.0)]
    );
}

#[tokio::test]
async fn disconnecting_one_shared_node_releases_its_mcp_and_sftp_consumers() {
    use oxideterm_ssh::{ConnectionState, SshConfig};
    let (_dir, host, mut backend, client, mut receiver) = dispatch_fixture(&[
        ToolGroup::NodeSession,
        ToolGroup::FileRead,
        ToolGroup::CommandExecute,
    ]);
    let registry = SshConnectionRegistry::default();
    registry.set_idle_timeout(Some(Duration::ZERO));
    backend.node_router = NodeRouter::new(registry.clone());
    let router = &backend.node_router;
    let a = NodeId::new("disconnect");
    let b = NodeId::new("keep");
    let config = SshConfig::default();
    let handle = registry.acquire(config.clone(), ConnectionConsumer::NodeRouter(a.0.clone()));
    let id = handle.connection_id();
    handle.set_physical(Arc::new(()));
    registry.mark_state(id, ConnectionState::Active);
    registry.acquire(config.clone(), ConnectionConsumer::NodeRouter(b.0.clone()));
    for node in [&a, &b] {
        router.upsert_node(node.clone(), config.clone());
        router.bind_connection(node, id).unwrap();
    }
    let terminal = ConnectionConsumer::Terminal("other-shell".into());
    let sibling = registry.lease_consumer(id, terminal.clone()).unwrap();
    let (request, response) = queued_request(
        &host,
        &mut receiver,
        &client,
        PublicToolCall::ConnectNode(oxideterm_public_mcp::calls::ConnectNodeArgs {
            connection_ref: ConnectionRef::new(),
        }),
    )
    .await;
    backend.acquire_node_lease(request, a.clone(), "saved-a".into(), None);
    let node_ref: NodeRef =
        serde_json::from_value(response.await.unwrap().data.unwrap()["node_ref"].clone()).unwrap();
    let file_ref = FileSessionRef::new();
    let consumer = ConnectionConsumer::Sftp(file_ref.to_string());
    let route = router.lease_connection_now(&a, consumer).unwrap();
    host.runtime_handles.lock().file_sessions.insert(
        file_ref.clone(),
        PublicMcpFileSessionRecord {
            client_ref: client.clone(),
            node_id: a.clone(),
            root: None,
            session: None,
            route: Some(Arc::new(route)),
        },
    );
    assert_eq!(registry.monitor_stats().total_sftp_sessions, 1);

    // Both requests have entered the broker before the desktop invalidates the node.
    let mut pending = Vec::new();
    for call in [
        PublicToolCall::FilesOpen(oxideterm_public_mcp::calls::FilesOpenArgs {
            node_ref: node_ref.clone(),
            root: None,
        }),
        PublicToolCall::StartCommand(oxideterm_public_mcp::calls::StartCommandArgs {
            node_ref: node_ref.clone(),
            command: Zeroizing::new("pwd".into()),
            working_directory: None,
        }),
    ] {
        pending.push(queued_request(&host, &mut receiver, &client, call).await);
    }
    router
        .disconnect_node_runtime(&a, "explicit disconnect")
        .unwrap();
    router.release_consumer(id, &ConnectionConsumer::NodeRouter(a.0.clone()));
    assert_eq!(backend.invalidate_disconnected_nodes(&[a]), 1);
    for (request, response) in pending {
        assert!(backend.dispatch(request).is_ok());
        assert_eq!(
            response.await.unwrap().error.as_deref(),
            Some("The node handle is unavailable")
        );
    }
    assert!(host.node_lease(&client, &node_ref).is_none());
    assert_eq!(router.resolve_connection_now(&b).unwrap().connection_id, id);
    assert_eq!(
        handle.info().consumers,
        [ConnectionConsumer::NodeRouter(b.0), terminal]
    );
    assert_eq!(registry.monitor_stats().total_sftp_sessions, 0);
    assert!(
        !host
            .runtime_handles
            .lock()
            .file_sessions
            .contains_key(&file_ref)
    );
    drop(sibling);
    tokio::time::timeout(Duration::from_secs(2), async {
        while registry.get(id).is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the final terminal must permit idle retirement");
}

#[allow(
    dead_code,
    reason = "Only the SSH protocol tests inspect home-resolution events."
)]
#[path = "../../oxideterm-ssh/tests/support/peer.rs"]
mod ssh_peer;

#[tokio::test]
async fn file_handle_survives_node_release_and_immediate_idle_reconnect() {
    use oxideterm_public_mcp::calls::{
        ConnectNodeArgs, FilesCloseArgs, FilesOpenArgs, ReleaseNodeArgs, StartTransferArgs,
        TransferHandleArgs,
    };
    use oxideterm_ssh::{ConnectionState, SshTransportClient};

    let (_dir, host, mut backend, client, mut receiver) = dispatch_fixture(&[
        ToolGroup::NodeSession,
        ToolGroup::FileRead,
        ToolGroup::ArtifactTransfer,
    ]);
    let (config, _, server, _) = ssh_peer::start_peer(false).await;
    let registry = SshConnectionRegistry::default();
    registry.set_idle_timeout(Some(Duration::ZERO));
    backend.node_router = NodeRouter::new(registry.clone());
    let router = &backend.node_router;
    let node = NodeId::new("files");
    router.upsert_node(node.clone(), config.clone());
    let connect = || {
        SshTransportClient::new(config.clone()).connect_node_with_registry(
            registry.clone(),
            ConnectionConsumer::NodeRouter(node.0.clone()),
        )
    };
    let first = connect().await.unwrap();
    let delivery = router
        .bind_established_connection(&node, first.connection_id())
        .unwrap();
    let (request, response) = queued_request(
        &host,
        &mut receiver,
        &client,
        PublicToolCall::ConnectNode(ConnectNodeArgs {
            connection_ref: ConnectionRef::new(),
        }),
    )
    .await;
    backend.acquire_node_lease(request, node.clone(), "saved".into(), None);
    let node_ref: NodeRef =
        serde_json::from_value(response.await.unwrap().data.unwrap()["node_ref"].clone()).unwrap();
    drop(delivery);
    async fn dispatch(
        backend: &PublicMcpBackground,
        host: &PublicMcpHost,
        receiver: &mut DomainRequestReceiver,
        client: &ClientRef,
        call: PublicToolCall,
    ) -> ToolEnvelope {
        let (request, response) = queued_request(host, receiver, client, call).await;
        assert!(backend.dispatch(request).is_ok());
        response.await.unwrap()
    }
    let open = PublicToolCall::FilesOpen(FilesOpenArgs {
        node_ref: node_ref.clone(),
        root: Some("/fixture".into()),
    });
    let files = dispatch(&backend, &host, &mut receiver, &client, open).await;
    assert_eq!(files.error, None);
    let file_ref: FileSessionRef =
        serde_json::from_value(files.data.expect("open SFTP")["file_session_ref"].clone()).unwrap();
    let released = dispatch(
        &backend,
        &host,
        &mut receiver,
        &client,
        PublicToolCall::ReleaseNode(ReleaseNodeArgs {
            node_ref: node_ref.clone(),
        }),
    )
    .await;
    assert_eq!(released.data.unwrap()["released"], true);
    registry.mark_state_without_event(first.connection_id(), ConnectionState::LinkDown);
    let second = connect().await.unwrap();
    let delivery = router
        .bind_established_connection(&node, second.connection_id())
        .unwrap();
    backend.rebind_node_leases(&node, second.connection_id());
    drop(delivery);
    tokio::task::yield_now().await;
    assert!(
        second
            .info()
            .consumers
            .iter()
            .any(|consumer| matches!(consumer, ConnectionConsumer::Sftp(_)))
    );
    let started = dispatch(
        &backend,
        &host,
        &mut receiver,
        &client,
        PublicToolCall::TransferStart(StartTransferArgs::Download {
            file_session_ref: file_ref.clone(),
            remote_path: "sample".into(),
            local_path: None,
            overwrite: false,
            resume: false,
        }),
    )
    .await;
    let transfer: TransferRef = serde_json::from_value(
        started.data.expect("download after reconnect")["transfer_ref"].clone(),
    )
    .unwrap();
    let artifact = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let status = dispatch(
                &backend,
                &host,
                &mut receiver,
                &client,
                PublicToolCall::TransferStatus(TransferHandleArgs {
                    transfer_ref: transfer.clone(),
                }),
            )
            .await
            .data
            .unwrap();
            match status["state"].as_str() {
                Some("completed") => {
                    break serde_json::from_value(status["artifact"]["artifact_ref"].clone())
                        .unwrap();
                }
                Some("pending" | "running") => tokio::task::yield_now().await,
                _ => panic!("download failed: {status}"),
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(
        host.state
            .artifacts
            .read_all(&client, &artifact, 2)
            .unwrap()
            .bytes
            .as_slice(),
        &[2, 2]
    );
    let closed = dispatch(
        &backend,
        &host,
        &mut receiver,
        &client,
        PublicToolCall::FilesClose(FilesCloseArgs {
            file_session_ref: file_ref,
        }),
    )
    .await;
    assert_eq!(closed.data.unwrap()["closed"], true);
    tokio::time::timeout(Duration::from_secs(2), async {
        while registry.get(second.connection_id()).is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("closing the last SFTP consumer retires the connection");
    server.abort();
}

fn dispatch_fixture(
    groups: &[ToolGroup],
) -> (
    tempfile::TempDir,
    PublicMcpHost,
    PublicMcpBackground,
    ClientRef,
    DomainRequestReceiver,
) {
    let path = tempfile::tempdir().unwrap();
    let runtime = tokio::runtime::Handle::current();
    let mut host = PublicMcpHost::start(&path.path().join("settings.json"), &runtime);
    host.create_client("fixture".into(), ClientApprovalMode::Standard)
        .unwrap();
    let client = host.clients().pop().unwrap().client_ref;
    host.set_client_groups(
        &client,
        groups.iter().copied().chain([ToolGroup::Basic]).collect(),
    )
    .unwrap();
    host.set_suspended(false);
    let receiver = host.take_receiver().unwrap();
    let (effects, _) = tokio::sync::mpsc::unbounded_channel();
    let backend = PublicMcpBackground::new(
        &host,
        router(),
        Arc::new(oxideterm_sftp::SftpTransferManager::new()),
        runtime,
        Arc::new(NoForwards),
        effects,
    );
    (path, host, backend, client, receiver)
}

async fn queued_request(
    host: &PublicMcpHost,
    receiver: &mut DomainRequestReceiver,
    client: &ClientRef,
    call: PublicToolCall,
) -> (DomainRequest, tokio::task::JoinHandle<ToolEnvelope>) {
    let broker = host.state.broker.clone();
    let clients = host.state.clients.clone();
    let client = client.clone();
    let response = tokio::spawn(async move {
        broker
            .execute(&clients, ClientApprovalMode::Standard, client, call, None)
            .await
            .unwrap()
    });
    loop {
        if let oxideterm_public_mcp::DomainMessage::Request(request) =
            receiver.recv().await.unwrap()
        {
            return (*request, response);
        }
    }
}

#[tokio::test]
async fn node_lease_quota_and_revocation_are_client_scoped() {
    let path = tempfile::tempdir().unwrap();
    let host = PublicMcpHost::start(
        &path.path().join("settings.json"),
        &tokio::runtime::Handle::current(),
    );
    let alice = ClientRef::new();
    let bob = ClientRef::new();
    let lease = |client_ref: ClientRef, node: &str| PublicMcpNodeRegistration {
        client_ref,
        node_id: NodeId::new(node),
        saved_connection_id: None,
        connection: None,
    };
    let first = NodeRef::new();
    {
        let mut handles = host.runtime_handles.lock();
        assert!(handles.reserve_node(first.clone(), lease(alice.clone(), "alice-first")));
        for _ in 1..PUBLIC_MCP_NODE_CAPACITY_PER_CLIENT {
            assert!(handles.reserve_node(NodeRef::new(), lease(alice.clone(), "alice-next")));
        }
        assert!(!handles.reserve_node(NodeRef::new(), lease(alice.clone(), "overflow")));
        assert!(handles.reserve_node(NodeRef::new(), lease(bob.clone(), "bob")));
    }
    assert_eq!(
        host.node_lease(&alice, &first).unwrap().node_id,
        NodeId::new("alice-first")
    );
    assert!(host.node_lease(&bob, &first).is_none());
    host.revoke_client_runtime(&alice);
    assert!(host.node_lease(&alice, &first).is_none());
    let handles = host.runtime_handles.lock();
    assert_eq!(
        handles
            .nodes
            .values()
            .map(|lease| &lease.client_ref)
            .collect::<Vec<_>>(),
        [&bob]
    );
    assert!(handles.node_capacity_available(&alice));
}

#[tokio::test]
async fn dispatch_handles_node_reads_and_rejects_requests_while_locked() {
    let (_path, host, backend, client, mut receiver) = dispatch_fixture(&[ToolGroup::NodeSession]);
    for (suspended, expected) in [
        (false, "The node handle is unavailable"),
        (true, "The OxideTerm workspace is locked"),
    ] {
        host.set_suspended(suspended);
        let (request, response) = queued_request(
            &host,
            &mut receiver,
            &client,
            PublicToolCall::InspectNode(oxideterm_public_mcp::calls::InspectNodeArgs {
                node_ref: NodeRef::new(),
            }),
        )
        .await;
        assert!(backend.dispatch(request).is_ok());
        let response = serde_json::to_value(response.await.unwrap()).unwrap();
        assert_eq!(response["outcome"], "failed");
        assert_eq!(response["error"], expected);
    }
}

#[test]
fn canonical_path_boundary_distinguishes_children_from_shared_prefixes() {
    assert!(files::require_path_within_root("/srv/project", "/srv/project/file.txt").is_ok());
    assert!(files::require_path_within_root("/srv/project", "/srv/projects/file.txt").is_err());
    assert!(files::require_path_within_root("/srv/project", "/srv/other/file.txt").is_err());
}

#[tokio::test]
async fn recording_status_routes_only_live_recordings_to_the_window() {
    use oxideterm_public_mcp::{
        RecordingStatusTarget, RecordingsControlArgs, calls::RecordingsStatusArgs,
    };
    use oxideterm_terminal_recording::{TerminalRecordingState, TerminalRecordingStatus};

    let (_path, host, backend, client, mut receiver) =
        dispatch_fixture(&[ToolGroup::RecordingControl, ToolGroup::RecordingContent]);
    let terminal = TerminalRef::new();
    let runtime = tokio::runtime::Handle::current();
    let (request, response) = queued_request(
        &host,
        &mut receiver,
        &client,
        PublicToolCall::RecordingsControl(RecordingsControlArgs::Start {
            terminal_ref: terminal.clone(),
            title: Some("demo".into()),
            capture_input: false,
        }),
    )
    .await;
    host.recording_request(request, &runtime, |target, action| {
        assert_eq!(target, &terminal);
        let RecordingPaneAction::Start(title) = action else {
            panic!("expected recorder start");
        };
        assert_eq!(title.as_deref(), Some("demo"));
        Ok(RecordingPaneSnapshot {
            status: TerminalRecordingStatus {
                state: TerminalRecordingState::Recording,
                ..Default::default()
            },
            size: (80, 24),
            stopped_content: None,
        })
    });
    let started = response.await.unwrap().data.unwrap();
    assert_eq!(started["recording"]["state"], "recording");
    let recording = started["recording"]["recording_ref"]
        .as_str()
        .unwrap()
        .parse::<RecordingRef>()
        .unwrap();
    let call = || {
        PublicToolCall::RecordingsStatus(RecordingsStatusArgs {
            target: RecordingStatusTarget::Recording {
                recording_ref: recording.clone(),
            },
        })
    };
    let (request, response) = queued_request(&host, &mut receiver, &client, call()).await;
    let Err(PublicMcpWindowRequest::Call(request)) = backend.dispatch(request) else {
        panic!("an active recording needs the live pane");
    };
    request.finish(ToolEnvelope::failed("fixture has no pane"));
    assert_eq!(
        response.await.unwrap().error.as_deref(),
        Some("fixture has no pane")
    );

    for expected_pane_call in [true, false] {
        let (request, response) = queued_request(
            &host,
            &mut receiver,
            &client,
            PublicToolCall::RecordingsControl(RecordingsControlArgs::Stop {
                recording_ref: recording.clone(),
            }),
        )
        .await;
        let mut stopped = false;
        host.recording_request(request, &runtime, |target, action| {
            assert_eq!(target, &terminal);
            assert!(matches!(action, RecordingPaneAction::Stop));
            stopped = true;
            Ok(RecordingPaneSnapshot {
                status: TerminalRecordingStatus {
                    state: TerminalRecordingState::Recording,
                    elapsed: Duration::from_millis(2700),
                    event_count: 4,
                },
                size: (90, 30),
                stopped_content: Some(Zeroizing::new("sample terminal output".into())),
            })
        });
        assert_eq!(
            stopped, expected_pane_call,
            "repeated stop must not ask the pane for content again"
        );
        assert_eq!(
            response.await.unwrap().data.unwrap()["recording"]["elapsed_ms"],
            2700
        );
    }
    for content_available in [true, false] {
        if !content_available {
            host.revoke_public_mcp_client_recording_content(&client);
        }
        let (request, response) = queued_request(&host, &mut receiver, &client, call()).await;
        assert!(
            backend.dispatch(request).is_ok(),
            "a stopped recording has no pane dependency"
        );
        assert_eq!(
            response.await.unwrap().data.unwrap()["recording"],
            json!({
                "recording_ref": recording,
                "terminal_ref": terminal,
                "state": "stopped",
                "elapsed_ms": 2700,
                "event_count": 4,
                "cols": 90,
                "rows": 30,
                "managed_by_client": true,
                "capture_input": false,
                "content_available": content_available,
                "truncated": false,
            })
        );
    }
}

#[tokio::test]
async fn addon_dispatch_prepares_only_the_callers_validated_artifact() {
    use oxideterm_public_mcp::calls::AddonsInstallArgs;

    let (_path, host, backend, client, mut receiver) =
        dispatch_fixture(&[ToolGroup::ArtifactTransfer, ToolGroup::AddonManage]);
    let bytes = b"fixture package content";
    let owned = host
        .state
        .artifacts
        .stage(client.clone(), bytes, "application/zip".into(), None)
        .unwrap();
    let foreign = host
        .state
        .artifacts
        .stage(ClientRef::new(), bytes, "application/zip".into(), None)
        .unwrap();
    let args = |artifact_ref, checksum| {
        PublicToolCall::AddonsInstall(AddonsInstallArgs {
            artifact_ref,
            expected_identity: "com.example.fixture".into(),
            checksum,
            replace_existing: false,
        })
    };
    for (artifact_ref, checksum, expected) in [
        (
            foreign.artifact_ref,
            foreign.digest,
            "the artifact is not available to this client",
        ),
        (
            owned.artifact_ref.clone(),
            "wrong-checksum".into(),
            "The addon package checksum does not match the artifact",
        ),
    ] {
        let (request, response) =
            queued_request(&host, &mut receiver, &client, args(artifact_ref, checksum)).await;
        assert!(
            backend.dispatch(request).is_ok(),
            "invalid packages must never reach the plugin owner"
        );
        assert_eq!(response.await.unwrap().error.as_deref(), Some(expected));
    }
    let (request, response) = queued_request(
        &host,
        &mut receiver,
        &client,
        args(
            owned.artifact_ref,
            format!("sha256:{}", owned.digest.to_uppercase()),
        ),
    )
    .await;
    let Err(PublicMcpWindowRequest::InstallAddon { request, package }) = backend.dispatch(request)
    else {
        panic!("a validated package should reach the plugin owner");
    };
    assert_eq!(package.bytes.as_slice(), bytes);
    assert_eq!(package.checksum, owned.digest);
    request.finish(ToolEnvelope::failed("fixture has no plugin owner"));
    assert_eq!(
        response.await.unwrap().error.as_deref(),
        Some("fixture has no plugin owner")
    );
}

#[tokio::test]
async fn sync_undo_rejects_new_local_edits_then_retries_and_consumes_the_handle() {
    use oxideterm_cloud_sync::state::CloudSyncPersistedState;
    use oxideterm_quick_commands::{load_snapshot, save_snapshot};
    use oxideterm_settings::SettingsStore;
    use sync_protocol::{PublicMcpSyncEffect, PublicMcpSyncOwner, PublicMcpSyncStores};

    struct Owner<'a> {
        connections: &'a mut ConnectionStore,
        settings: &'a mut SettingsStore,
        forwards: &'a oxideterm_forwarding::ForwardingRegistry,
        state: CloudSyncPersistedState,
    }
    impl PublicMcpSyncOwner for Owner<'_> {
        fn stores(&mut self) -> PublicMcpSyncStores<'_> {
            PublicMcpSyncStores {
                connections: self.connections,
                settings: self.settings,
                forwards: self.forwards,
                cloud: &self.state,
            }
        }
        fn service(&self) -> oxideterm_cloud_sync::operation::CloudSyncOperationService {
            panic!("local undo must not start a Cloud Sync service")
        }
        fn busy(&self) -> bool {
            false
        }
        fn effect(&mut self, effect: PublicMcpSyncEffect<'_>) {
            let PublicMcpSyncEffect::Restored(state) = effect else {
                panic!("local undo must only publish a restore effect");
            };
            self.state = state;
        }
    }

    let (dir, host, backend, client, mut receiver) = dispatch_fixture(&[ToolGroup::CloudSync]);
    let mut connections = ConnectionStore::load(dir.path().join("connections.json")).unwrap();
    let mut settings = SettingsStore::load_from_path(dir.path().join("settings.json")).unwrap();
    let forwards = oxideterm_forwarding::ForwardingRegistry::new();
    let state = CloudSyncPersistedState::default();
    let mut commands = load_snapshot(settings.path()).unwrap();
    commands.commands[0].name = "Before applying sync".into();
    save_snapshot(settings.path(), &commands).unwrap();
    let checkpoint = sync_protocol::capture_undo_checkpoint(
        &[oxideterm_public_mcp::PublicSyncSection::QuickCommands],
        &connections,
        &forwards,
        &settings,
        &state,
    )
    .unwrap()
    .unwrap();

    commands.commands[0].name = "Received from sync".into();
    save_snapshot(settings.path(), &commands).unwrap();
    let undo = PublicMcpSyncUndo {
        client_ref: client.clone(),
        created_at: Instant::now(),
        post_apply_state: sync_protocol::full_local_state(&connections, &forwards, &settings)
            .unwrap(),
        checkpoint,
    };
    let reference = backend.sync_handles.lock().insert_sync_undo(undo);
    let mut owner = Owner {
        connections: &mut connections,
        settings: &mut settings,
        forwards: &forwards,
        state,
    };
    for (name, expected_name, expected_error) in [
        (
            "New local edit",
            "New local edit",
            Some("Local synchronized data changed after the undo handle was created"),
        ),
        ("Received from sync", "Before applying sync", None),
        (
            "Before applying sync",
            "Before applying sync",
            Some("The Cloud Sync undo handle is unavailable, expired, or already used"),
        ),
    ] {
        commands.commands[0].name = name.into();
        save_snapshot(owner.settings.path(), &commands).unwrap();
        let (request, response) = queued_request(
            &host,
            &mut receiver,
            &client,
            PublicToolCall::SyncRestore(oxideterm_public_mcp::SyncRestoreArgs {
                undo_ref: reference.clone(),
            }),
        )
        .await;
        assert!(backend.sync_request(request, &mut owner).is_none());
        let response = response.await.unwrap();
        assert_eq!(response.error.as_deref(), expected_error);
        if expected_error.is_none() {
            assert_eq!(response.data, Some(json!({ "restored": true })));
        }
        assert_eq!(
            load_snapshot(owner.settings.path()).unwrap().commands[0].name,
            expected_name
        );
    }
}

#[tokio::test]
async fn history_pages_and_release_are_served_without_a_window() {
    use oxideterm_public_mcp::history::*;
    let (_path, host, backend, client, mut requests) =
        dispatch_fixture(&[ToolGroup::TerminalObserve]);
    let terminal = TerminalRef::new();
    host.register_terminal(
        terminal.clone(),
        PublicMcpTerminalRecord {
            client_ref: client.clone(),
            session_id: TerminalSessionId(42),
            title: "fixture".into(),
            transport: "local",
            node_ref: None,
        },
    );
    let now = Instant::now();
    let initial_args = HistoryArgs {
        request: HistoryRequest::Snapshot {
            terminal_ref: terminal.clone(),
            buffer: HistoryBuffer::Primary,
            selection: HistorySelection::All {},
            slot: 0,
            after_revision: None,
        },
        line_limit: Some(1),
        byte_limit: None,
    };
    let initial = {
        let mut cache = host.history.lock();
        let reservation = cache
            .reserve(
                &client,
                0,
                HistoryTarget {
                    terminal: terminal.clone(),
                    session: "42".into(),
                },
                HistoryBuffer::Primary,
                HistorySelection::All {},
                None,
                now,
            )
            .unwrap();
        cache
            .publish(
                reservation,
                HistoryCapture {
                    stamp: HistoryStamp {
                        identity: 1,
                        content: 1,
                        switches: 0,
                        cleared: 0,
                        evicted: 0,
                    },
                    captured: now,
                    wall_time: std::time::SystemTime::UNIX_EPOCH,
                    screen_rows: 2,
                    retained_rows: 2,
                    active_buffer: HistoryBuffer::Primary,
                    rows: Some(
                        ["first row", "second row"]
                            .into_iter()
                            .map(|value| HistoryRow {
                                text: Zeroizing::new(value.into()),
                                soft_wrapped: false,
                            })
                            .collect(),
                    ),
                },
                &initial_args,
                now,
            )
            .unwrap()
    };
    let cursor = initial["next_cursor"].as_str().unwrap().to_owned();
    let page = || HistoryRequest::Page {
        cursor: cursor.clone(),
    };
    for (action, expected) in [
        (page(), "second row"),
        (HistoryRequest::ReleaseSlot { slot: 0 }, "released"),
        (page(), "failed"),
    ] {
        let (request, response) = queued_request(
            &host,
            &mut requests,
            &client,
            PublicToolCall::TerminalHistory(HistoryArgs {
                request: action,
                line_limit: None,
                byte_limit: None,
            }),
        )
        .await;
        assert!(backend.dispatch(request).is_ok());
        let result = serde_json::to_value(response.await.unwrap()).unwrap();
        match expected {
            "released" => assert_eq!(result["data"]["status"], "released"),
            "failed" => assert_eq!(result["outcome"], "failed"),
            row => assert_eq!(result["data"]["rows"][0]["text"], row),
        }
    }
}

#[tokio::test]
async fn desktop_frame_generation_and_artifact_lifetime_follow_the_tab() {
    use oxideterm_public_mcp::calls::DesktopFrameArgs;
    use oxideterm_remote_desktop::RemoteDesktopSize;

    let (_path, host, backend, client, mut requests) = dispatch_fixture(&[
        ToolGroup::DesktopObserve,
        ToolGroup::DesktopInput,
        ToolGroup::ArtifactTransfer,
    ]);
    let desktop = DesktopRef::new();
    let tab = TabId(17);
    host.register_desktop(desktop.clone(), client.clone(), tab, "fixture".into(), true);
    let (request, response) = queued_request(
        &host,
        &mut requests,
        &client,
        PublicToolCall::DesktopFrame(DesktopFrameArgs {
            desktop_ref: desktop.clone(),
            after_generation: Some(5),
        }),
    )
    .await;
    let frame = || PublicMcpDesktopFrame {
        size: RemoteDesktopSize {
            width: 1,
            height: 1,
        },
        graphics_epoch: 3,
        generation: 5,
        bgra_bytes: Zeroizing::new(vec![10, 20, 30, 255]),
    };
    backend.desktop_frame(request, tab, frame(), None);
    assert_eq!(
        response.await.unwrap().data.unwrap(),
        json!({
            "desktop_ref": desktop, "graphics_epoch": 3, "generation": 5, "unchanged": true,
        })
    );

    let (request, response) = queued_request(
        &host,
        &mut requests,
        &client,
        PublicToolCall::DesktopFrame(DesktopFrameArgs {
            desktop_ref: desktop.clone(),
            after_generation: Some(4),
        }),
    )
    .await;
    backend.desktop_frame(request, tab, frame(), None);
    let result = response.await.unwrap().data.unwrap();
    let artifact: ArtifactRef = result["artifact"]["artifact_ref"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let encoded = host
        .state
        .artifacts
        .read_all(&client, &artifact, 4096)
        .unwrap();
    assert_eq!(encoded.projection.media_type, "image/png");
    let image = image::load_from_memory_with_format(&encoded.bytes, image::ImageFormat::Png)
        .unwrap()
        .to_rgba8();
    assert_eq!(image.get_pixel(0, 0).0, [30, 20, 10, 255]);
    use oxideterm_public_mcp::calls::{DesktopActArgs, DesktopGesture, PublicDesktopMouseButton};
    let (request, response) = queued_request(
        &host,
        &mut requests,
        &client,
        PublicToolCall::DesktopAct(DesktopActArgs {
            desktop_ref: desktop,
            graphics_epoch: 3,
            action: DesktopGesture::Click {
                x: 0,
                y: 0,
                button: PublicDesktopMouseButton::Left,
            },
        }),
    )
    .await;
    backend.desktop_frame(request, tab, frame(), None);
    let result = response.await.unwrap();
    let png = result.inline_png.unwrap();
    let inline = image::load_from_memory_with_format(&png.0, image::ImageFormat::Png).unwrap();
    assert_eq!(inline.to_rgba8().get_pixel(0, 0).0, [30, 20, 10, 255]);
    host.remove_closed_desktop(tab);
    assert!(!host.state.artifacts.is_available(&client, &artifact));
}

#[tokio::test]
async fn revoking_observation_cancels_only_calls_requiring_that_group() {
    use oxideterm_public_mcp::history::{HistoryArgs, HistoryRequest};
    let (_path, mut host, backend, client, mut requests) =
        dispatch_fixture(&[ToolGroup::TerminalObserve, ToolGroup::NodeSession]);
    let (request, answer) = queued_request(
        &host,
        &mut requests,
        &client,
        PublicToolCall::TerminalHistory(HistoryArgs {
            request: HistoryRequest::ReleaseSlot { slot: 0 },
            line_limit: None,
            byte_limit: None,
        }),
    )
    .await;
    let (other, other_answer) = queued_request(
        &host,
        &mut requests,
        &client,
        PublicToolCall::InspectNode(oxideterm_public_mcp::calls::InspectNodeArgs {
            node_ref: NodeRef::new(),
        }),
    )
    .await;
    let admission = host.admission();
    let verify = admission.lock();
    let effects = host
        .change_client(
            &backend,
            &client,
            PublicMcpClientChange::ToolGroup(ToolGroup::TerminalObserve, false),
        )
        .unwrap();
    assert!(effects.is_empty());
    assert!(request.is_cancelled());
    assert!(!other.is_cancelled());
    let retained = host.state.clients.get(&client).unwrap().tool_groups;
    assert!(!retained.contains(&ToolGroup::TerminalObserve));
    assert!(retained.contains(&ToolGroup::NodeSession));
    drop(verify);
    assert!(backend.dispatch(other).is_ok());
    assert_eq!(
        other_answer.await.unwrap().error.as_deref(),
        Some("The node handle is unavailable")
    );
    drop(request);
    answer.abort();
}

#[tokio::test]
async fn terminal_read_preserves_unchanged_cursor_and_bounds_whole_lines() {
    use oxideterm_public_mcp::calls::ReadTerminalArgs;
    let (_path, host, _, client, mut requests) = dispatch_fixture(&[ToolGroup::TerminalObserve]);
    let terminal_ref = TerminalRef::new();
    for cursor in [Some(4), Some(3)] {
        let (request, response) = queued_request(
            &host,
            &mut requests,
            &client,
            PublicToolCall::ReadTerminal(ReadTerminalArgs {
                terminal_ref: terminal_ref.clone(),
                cursor,
                line_limit: 2,
                tail: true,
            }),
        )
        .await;
        finish_terminal_read(request, 4, (80, 24), 2, true, || {
            assert_eq!(
                cursor,
                Some(3),
                "an unchanged cursor must not read terminal text"
            );
            format!("outside tail\nselected\n{}", "界".repeat(100_000))
        });
        let data = response.await.unwrap().data.unwrap();
        if cursor == Some(4) {
            assert_eq!(
                data,
                json!({
                    "terminal_ref": terminal_ref, "cursor": 4, "unchanged": true, "lines": [], "truncated": false,
                })
            );
        } else {
            assert_eq!(
                data,
                json!({
                    "terminal_ref": terminal_ref, "cursor": 4, "unchanged": false, "cols": 80, "rows": 24,
                    "display_offset": 2, "alternate_buffer": true, "lines": ["selected"], "truncated": true,
                })
            );
        }
    }
}

#[test]
fn cloud_sync_errors_keep_the_safe_code_but_drop_backend_details() {
    let error = sync_protocol::PublicMcpSyncFailure::from_error(
        "remote_changed_after_preview: private backend path".into(),
    );
    assert_eq!(error.display_error, "remote_changed_after_preview");
    assert_eq!(
        error.message,
        "The remote Cloud Sync revision changed after the plan was created"
    );
    let unknown =
        sync_protocol::PublicMcpSyncFailure::from_error("unexpected server detail".into());
    assert_eq!(unknown.display_error, "The Cloud Sync operation failed");
    assert_eq!(unknown.message, "The Cloud Sync operation failed");
}

#[tokio::test]
async fn local_path_file_transfer_streams_to_disk_and_enforces_overwrite() {
    use oxideterm_public_mcp::calls::{
        ConnectNodeArgs, FilesOpenArgs, StartTransferArgs, TransferHandleArgs,
    };
    use oxideterm_ssh::SshTransportClient;

    let (work_dir, host, mut backend, client, mut receiver) = dispatch_fixture(&[
        ToolGroup::NodeSession,
        ToolGroup::FileRead,
        ToolGroup::FileWrite,
        ToolGroup::ArtifactTransfer,
    ]);
    let (config, _, server, _) = ssh_peer::start_peer(false).await;
    let registry = SshConnectionRegistry::default();
    backend.node_router = NodeRouter::new(registry.clone());
    let node = NodeId::new("local-file-transfer");
    backend
        .node_router
        .upsert_node(node.clone(), config.clone());
    let conn = SshTransportClient::new(config)
        .connect_node_with_registry(registry, ConnectionConsumer::NodeRouter(node.0.clone()))
        .await
        .unwrap();
    let delivery = backend
        .node_router
        .bind_established_connection(&node, conn.connection_id())
        .unwrap();
    let (request, response) = queued_request(
        &host,
        &mut receiver,
        &client,
        PublicToolCall::ConnectNode(ConnectNodeArgs {
            connection_ref: ConnectionRef::new(),
        }),
    )
    .await;
    backend.acquire_node_lease(request, node.clone(), "saved".into(), None);
    let node_ref: NodeRef =
        serde_json::from_value(response.await.unwrap().data.unwrap()["node_ref"].clone()).unwrap();
    drop(delivery);

    async fn dispatch(
        backend: &PublicMcpBackground,
        host: &PublicMcpHost,
        receiver: &mut DomainRequestReceiver,
        client: &ClientRef,
        call: PublicToolCall,
    ) -> ToolEnvelope {
        let (request, response) = queued_request(host, receiver, client, call).await;
        assert!(backend.dispatch(request).is_ok());
        response.await.unwrap()
    }

    async fn wait_transfer(
        backend: &PublicMcpBackground,
        host: &PublicMcpHost,
        receiver: &mut DomainRequestReceiver,
        client: &ClientRef,
        transfer_ref: &TransferRef,
    ) -> serde_json::Value {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let status = dispatch(
                    backend,
                    host,
                    receiver,
                    client,
                    PublicToolCall::TransferStatus(TransferHandleArgs {
                        transfer_ref: transfer_ref.clone(),
                    }),
                )
                .await
                .data
                .unwrap();
                match status["state"].as_str() {
                    Some("completed" | "failed" | "cancelled") => return status,
                    _ => tokio::task::yield_now().await,
                }
            }
        })
        .await
        .unwrap()
    }

    let files = dispatch(
        &backend,
        &host,
        &mut receiver,
        &client,
        PublicToolCall::FilesOpen(FilesOpenArgs {
            node_ref,
            root: Some("/fixture".into()),
        }),
    )
    .await;
    let file_ref: FileSessionRef =
        serde_json::from_value(files.data.unwrap()["file_session_ref"].clone()).unwrap();

    // Download directly to a local file path (bypassing artifact staging).
    let local_file = work_dir.path().join("nested").join("sample.bin");
    let local_file_str = local_file.to_string_lossy().into_owned();
    let started = dispatch(
        &backend,
        &host,
        &mut receiver,
        &client,
        PublicToolCall::TransferStart(StartTransferArgs::Download {
            file_session_ref: file_ref.clone(),
            remote_path: "sample".into(),
            local_path: Some(local_file_str.clone()),
            overwrite: false,
            resume: false,
        }),
    )
    .await;
    let transfer_ref: TransferRef =
        serde_json::from_value(started.data.unwrap()["transfer_ref"].clone()).unwrap();
    let status = wait_transfer(&backend, &host, &mut receiver, &client, &transfer_ref).await;
    assert_eq!(status["state"], "completed", "{status}");
    assert_eq!(status["entry_kind"], "file");
    assert_eq!(status["strategy"], "sftp_file");
    assert_eq!(status["local_path"], local_file_str);
    assert!(status["artifact"].is_null());
    assert_eq!(std::fs::read(&local_file).unwrap(), &[1]);

    // Overwrite=false must fail with local_exists and leave existing local bytes intact.
    std::fs::write(&local_file, b"sentinel").unwrap();
    let blocked = dispatch(
        &backend,
        &host,
        &mut receiver,
        &client,
        PublicToolCall::TransferStart(StartTransferArgs::Download {
            file_session_ref: file_ref.clone(),
            remote_path: "sample".into(),
            local_path: Some(local_file_str.clone()),
            overwrite: false,
            resume: false,
        }),
    )
    .await;
    let blocked_ref: TransferRef =
        serde_json::from_value(blocked.data.unwrap()["transfer_ref"].clone()).unwrap();
    let blocked_status = wait_transfer(&backend, &host, &mut receiver, &client, &blocked_ref).await;
    assert_eq!(blocked_status["state"], "failed", "{blocked_status}");
    assert_eq!(blocked_status["error_code"], "local_exists");
    assert_eq!(std::fs::read(&local_file).unwrap(), b"sentinel");

    // Overwrite=true must atomically replace the existing local file.
    let replaced = dispatch(
        &backend,
        &host,
        &mut receiver,
        &client,
        PublicToolCall::TransferStart(StartTransferArgs::Download {
            file_session_ref: file_ref.clone(),
            remote_path: "sample".into(),
            local_path: Some(local_file_str),
            overwrite: true,
            resume: false,
        }),
    )
    .await;
    let replaced_ref: TransferRef =
        serde_json::from_value(replaced.data.unwrap()["transfer_ref"].clone()).unwrap();
    let replaced_status =
        wait_transfer(&backend, &host, &mut receiver, &client, &replaced_ref).await;
    assert_eq!(replaced_status["state"], "completed", "{replaced_status}");
    assert_eq!(std::fs::read(&local_file).unwrap(), &[1]);

    // Missing local upload path fails with local_unavailable; relative path is rejected upfront.
    let missing_upload = dispatch(
        &backend,
        &host,
        &mut receiver,
        &client,
        PublicToolCall::TransferStart(StartTransferArgs::Upload {
            file_session_ref: file_ref.clone(),
            remote_path: "out.bin".into(),
            artifact_ref: None,
            local_path: Some(
                work_dir
                    .path()
                    .join("does-not-exist.bin")
                    .to_string_lossy()
                    .into_owned(),
            ),
            overwrite: true,
            resume: false,
        }),
    )
    .await;
    let missing_ref: TransferRef =
        serde_json::from_value(missing_upload.data.unwrap()["transfer_ref"].clone()).unwrap();
    let missing_status = wait_transfer(&backend, &host, &mut receiver, &client, &missing_ref).await;
    assert_eq!(missing_status["state"], "failed");
    assert_eq!(missing_status["error_code"], "local_unavailable");

    let relative_upload = dispatch(
        &backend,
        &host,
        &mut receiver,
        &client,
        PublicToolCall::TransferStart(StartTransferArgs::Upload {
            file_session_ref: file_ref,
            remote_path: "out.bin".into(),
            artifact_ref: None,
            local_path: Some("relative/out.bin".into()),
            overwrite: true,
            resume: false,
        }),
    )
    .await;
    assert!(relative_upload.error.is_some());

    server.abort();
}

#[cfg(unix)]
#[tokio::test]
async fn local_path_directory_transfer_streams_tar_archive_both_directions() {
    use oxideterm_public_mcp::calls::{
        ConnectNodeArgs, FilesOpenArgs, StartTransferArgs, TransferHandleArgs,
    };
    use oxideterm_ssh::{SshConfig, SshTransportClient};
    use russh::{keys, server};
    use std::path::PathBuf;

    struct TarPeer {
        root: PathBuf,
        channels: std::collections::HashMap<russh::ChannelId, russh::Channel<server::Msg>>,
        tasks: tokio::task::JoinSet<()>,
    }

    struct LocalFsSftp {
        root: PathBuf,
        dirs: std::collections::HashMap<String, Vec<russh_sftp::protocol::File>>,
    }

    impl LocalFsSftp {
        fn attrs_for(
            path: &std::path::Path,
        ) -> Result<russh_sftp::protocol::FileAttributes, russh_sftp::protocol::StatusCode>
        {
            use std::os::unix::fs::MetadataExt;
            let meta = std::fs::symlink_metadata(path)
                .map_err(|_| russh_sftp::protocol::StatusCode::NoSuchFile)?;
            Ok(russh_sftp::protocol::FileAttributes {
                size: Some(meta.len()),
                permissions: Some(meta.mode()),
                ..Default::default()
            })
        }
    }

    impl russh_sftp::server::Handler for LocalFsSftp {
        type Error = russh_sftp::protocol::StatusCode;
        fn unimplemented(&self) -> Self::Error {
            russh_sftp::protocol::StatusCode::OpUnsupported
        }
        async fn realpath(
            &mut self,
            id: u32,
            path: String,
        ) -> Result<russh_sftp::protocol::Name, Self::Error> {
            let resolved = if path == "." {
                self.root.to_string_lossy().into_owned()
            } else {
                path
            };
            Ok(russh_sftp::protocol::Name {
                id,
                files: vec![russh_sftp::protocol::File::dummy(resolved)],
            })
        }
        async fn stat(
            &mut self,
            id: u32,
            path: String,
        ) -> Result<russh_sftp::protocol::Attrs, Self::Error> {
            Ok(russh_sftp::protocol::Attrs {
                id,
                attrs: Self::attrs_for(std::path::Path::new(&path))?,
            })
        }
        async fn lstat(
            &mut self,
            id: u32,
            path: String,
        ) -> Result<russh_sftp::protocol::Attrs, Self::Error> {
            self.stat(id, path).await
        }
        async fn mkdir(
            &mut self,
            id: u32,
            path: String,
            _: russh_sftp::protocol::FileAttributes,
        ) -> Result<russh_sftp::protocol::Status, Self::Error> {
            let _ = std::fs::create_dir_all(&path);
            Ok(russh_sftp::protocol::Status {
                id,
                status_code: russh_sftp::protocol::StatusCode::Ok,
                error_message: String::new(),
                language_tag: String::new(),
            })
        }
        async fn opendir(
            &mut self,
            id: u32,
            path: String,
        ) -> Result<russh_sftp::protocol::Handle, Self::Error> {
            let mut entries = Vec::new();
            let dir = std::fs::read_dir(&path)
                .map_err(|_| russh_sftp::protocol::StatusCode::NoSuchFile)?;
            for entry in dir.flatten() {
                let attrs = Self::attrs_for(&entry.path()).unwrap_or_default();
                let name = entry.file_name().to_string_lossy().into_owned();
                entries.push(russh_sftp::protocol::File::new(name, attrs));
            }
            let handle = format!("dir-{id}");
            self.dirs.insert(handle.clone(), entries);
            Ok(russh_sftp::protocol::Handle { id, handle })
        }
        async fn readdir(
            &mut self,
            id: u32,
            handle: String,
        ) -> Result<russh_sftp::protocol::Name, Self::Error> {
            let Some(entries) = self.dirs.get_mut(&handle) else {
                return Err(russh_sftp::protocol::StatusCode::NoSuchFile);
            };
            if entries.is_empty() {
                return Err(russh_sftp::protocol::StatusCode::Eof);
            }
            let batch = std::mem::take(entries);
            Ok(russh_sftp::protocol::Name { id, files: batch })
        }
        async fn close(
            &mut self,
            id: u32,
            handle: String,
        ) -> Result<russh_sftp::protocol::Status, Self::Error> {
            self.dirs.remove(&handle);
            Ok(russh_sftp::protocol::Status {
                id,
                status_code: russh_sftp::protocol::StatusCode::Ok,
                error_message: String::new(),
                language_tag: String::new(),
            })
        }
    }

    impl server::Handler for TarPeer {
        type Error = russh::Error;
        async fn auth_password(
            &mut self,
            user: &str,
            password: &str,
        ) -> Result<server::Auth, Self::Error> {
            Ok(if user == "tar-user" && password == "tar-secret" {
                server::Auth::Accept
            } else {
                server::Auth::reject()
            })
        }
        async fn channel_open_session(
            &mut self,
            channel: russh::Channel<server::Msg>,
            reply: server::ChannelOpenHandle,
            _: &mut server::Session,
        ) -> Result<(), Self::Error> {
            self.channels.insert(channel.id(), channel);
            reply.accept().await;
            Ok(())
        }
        async fn subsystem_request(
            &mut self,
            channel: russh::ChannelId,
            name: &str,
            session: &mut server::Session,
        ) -> Result<(), Self::Error> {
            if name != "sftp" {
                return session.channel_failure(channel);
            }
            session.channel_success(channel)?;
            let stream = self.channels.remove(&channel).unwrap().into_stream();
            let root = self.root.clone();
            self.tasks.spawn(async move {
                russh_sftp::server::run(
                    stream,
                    LocalFsSftp {
                        root,
                        dirs: Default::default(),
                    },
                )
                .await;
            });
            Ok(())
        }
        async fn exec_request(
            &mut self,
            channel: russh::ChannelId,
            command: &[u8],
            session: &mut server::Session,
        ) -> Result<(), Self::Error> {
            let cmd = String::from_utf8_lossy(command).into_owned();
            session.channel_success(channel)?;
            let mut ch = self.channels.remove(&channel).unwrap();
            self.tasks.spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut child = tokio::process::Command::new("/bin/sh")
                    .args(["-c", &cmd])
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()
                    .unwrap();
                let mut child_stdin = Some(child.stdin.take().unwrap());
                let mut child_stdout = child.stdout.take().unwrap();
                let mut child_stderr = child.stderr.take().unwrap();
                let writer = ch.make_writer();
                let stdout_task = tokio::spawn(async move {
                    let mut out = writer;
                    let mut buf = [0u8; 16 * 1024];
                    while let Ok(n) = child_stdout.read(&mut buf).await {
                        if n == 0 {
                            break;
                        }
                        if out.write_all(&buf[..n]).await.is_err() {
                            break;
                        }
                    }
                    let _ = out.flush().await;
                });
                let err_writer = ch.make_writer_ext(Some(1));
                let stderr_task = tokio::spawn(async move {
                    let mut err = err_writer;
                    let mut buf = [0u8; 4096];
                    while let Ok(n) = child_stderr.read(&mut buf).await {
                        if n == 0 {
                            break;
                        }
                        let _ = err.write_all(&buf[..n]).await;
                    }
                });
                let status = loop {
                    tokio::select! {
                        status = child.wait() => break status.unwrap(),
                        msg = ch.wait(), if child_stdin.is_some() => match msg {
                            Some(russh::ChannelMsg::Data { data }) => {
                                if let Some(stdin) = child_stdin.as_mut() {
                                    let _ = stdin.write_all(&data).await;
                                }
                            }
                            Some(russh::ChannelMsg::Eof | russh::ChannelMsg::Close) | None => {
                                child_stdin.take();
                            }
                            _ => {}
                        },
                    }
                };
                drop(child_stdin);
                let _ = stdout_task.await;
                let _ = stderr_task.await;
                let _ = ch.exit_status(status.code().unwrap_or(1) as u32).await;
                let _ = ch.eof().await;
                let _ = ch.close().await;
            });
            Ok(())
        }
    }

    let remote_root = tempfile::tempdir().unwrap();
    let remote_root_path = std::fs::canonicalize(remote_root.path()).unwrap();
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let key = keys::PrivateKey::random(&mut rand10::rng(), keys::Algorithm::Ed25519).unwrap();
    let fingerprint = key
        .public_key()
        .fingerprint(keys::HashAlg::Sha256)
        .to_string();
    let server_root = remote_root_path.clone();
    let server = tokio::spawn(async move {
        let config = Arc::new(server::Config {
            keys: vec![key],
            auth_rejection_time: Duration::ZERO,
            ..Default::default()
        });
        let mut conns = tokio::task::JoinSet::new();
        loop {
            let (socket, _) = listener.accept().await.unwrap();
            let cfg = config.clone();
            let root = server_root.clone();
            conns.spawn(async move {
                if let Ok(connection) = server::run_stream(
                    cfg,
                    socket,
                    TarPeer {
                        root,
                        channels: Default::default(),
                        tasks: Default::default(),
                    },
                )
                .await
                {
                    let _ = connection.await;
                }
            });
        }
    });

    let mut ssh_config = SshConfig::password("127.0.0.1", port, "tar-user", "tar-secret");
    ssh_config.trust_host_key = Some(false);
    ssh_config.expected_host_key_fingerprint = Some(fingerprint);
    let (work_dir, host, mut backend, client, mut receiver) = dispatch_fixture(&[
        ToolGroup::NodeSession,
        ToolGroup::FileRead,
        ToolGroup::FileWrite,
        ToolGroup::ArtifactTransfer,
    ]);
    let registry = SshConnectionRegistry::default();
    backend.node_router = NodeRouter::new(registry.clone());
    let node = NodeId::new("tar-dir-transfer");
    backend
        .node_router
        .upsert_node(node.clone(), ssh_config.clone());
    let conn = SshTransportClient::new(ssh_config)
        .connect_node_with_registry(registry, ConnectionConsumer::NodeRouter(node.0.clone()))
        .await
        .unwrap();
    let delivery = backend
        .node_router
        .bind_established_connection(&node, conn.connection_id())
        .unwrap();

    async fn dispatch(
        backend: &PublicMcpBackground,
        host: &PublicMcpHost,
        receiver: &mut DomainRequestReceiver,
        client: &ClientRef,
        call: PublicToolCall,
    ) -> ToolEnvelope {
        let (request, response) = queued_request(host, receiver, client, call).await;
        assert!(backend.dispatch(request).is_ok());
        response.await.unwrap()
    }

    async fn wait_transfer(
        backend: &PublicMcpBackground,
        host: &PublicMcpHost,
        receiver: &mut DomainRequestReceiver,
        client: &ClientRef,
        transfer_ref: &TransferRef,
    ) -> serde_json::Value {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let status = dispatch(
                    backend,
                    host,
                    receiver,
                    client,
                    PublicToolCall::TransferStatus(TransferHandleArgs {
                        transfer_ref: transfer_ref.clone(),
                    }),
                )
                .await
                .data
                .unwrap();
                match status["state"].as_str() {
                    Some("completed" | "failed" | "cancelled") => return status,
                    _ => tokio::task::yield_now().await,
                }
            }
        })
        .await
        .unwrap()
    }

    let (request, response) = queued_request(
        &host,
        &mut receiver,
        &client,
        PublicToolCall::ConnectNode(ConnectNodeArgs {
            connection_ref: ConnectionRef::new(),
        }),
    )
    .await;
    backend.acquire_node_lease(request, node.clone(), "saved".into(), None);
    let node_ref: NodeRef =
        serde_json::from_value(response.await.unwrap().data.unwrap()["node_ref"].clone()).unwrap();
    drop(delivery);

    let files = dispatch(
        &backend,
        &host,
        &mut receiver,
        &client,
        PublicToolCall::FilesOpen(FilesOpenArgs {
            node_ref,
            root: Some(remote_root_path.to_string_lossy().into_owned()),
        }),
    )
    .await;
    let file_ref: FileSessionRef =
        serde_json::from_value(files.data.unwrap()["file_session_ref"].clone()).unwrap();

    // Build a local multi-file folder (>= 5 files) so tar streaming is selected.
    let src_dir = work_dir.path().join("release_bundle");
    std::fs::create_dir_all(src_dir.join("sub")).unwrap();
    for idx in 0..5 {
        std::fs::write(
            src_dir.join(format!("file_{idx}.txt")),
            format!("payload-{idx}"),
        )
        .unwrap();
    }
    std::fs::write(src_dir.join("sub").join("nested.txt"), "nested-content").unwrap();

    // Upload folder via local_path -> auto-packed and unpacked remotely via tar_archive.
    let upload = dispatch(
        &backend,
        &host,
        &mut receiver,
        &client,
        PublicToolCall::TransferStart(StartTransferArgs::Upload {
            file_session_ref: file_ref.clone(),
            remote_path: "deployed".into(),
            artifact_ref: None,
            local_path: Some(src_dir.to_string_lossy().into_owned()),
            overwrite: false,
            resume: false,
        }),
    )
    .await;
    let upload_ref: TransferRef =
        serde_json::from_value(upload.data.unwrap()["transfer_ref"].clone()).unwrap();
    let upload_status = wait_transfer(&backend, &host, &mut receiver, &client, &upload_ref).await;
    assert_eq!(upload_status["state"], "completed", "{upload_status}");
    assert_eq!(upload_status["entry_kind"], "directory");
    assert_eq!(upload_status["strategy"], "tar_archive");
    assert!(upload_status["item_count"].as_u64().unwrap_or(0) >= 6);
    assert_eq!(
        std::fs::read_to_string(remote_root_path.join("deployed").join("file_3.txt")).unwrap(),
        "payload-3"
    );
    assert_eq!(
        std::fs::read_to_string(
            remote_root_path
                .join("deployed")
                .join("sub")
                .join("nested.txt")
        )
        .unwrap(),
        "nested-content"
    );

    // Download the remote folder back to a fresh local path -> auto-packed remotely and unpacked locally.
    let dst_dir = work_dir.path().join("downloaded_bundle");
    let download = dispatch(
        &backend,
        &host,
        &mut receiver,
        &client,
        PublicToolCall::TransferStart(StartTransferArgs::Download {
            file_session_ref: file_ref,
            remote_path: "deployed".into(),
            local_path: Some(dst_dir.to_string_lossy().into_owned()),
            overwrite: false,
            resume: false,
        }),
    )
    .await;
    let download_ref: TransferRef =
        serde_json::from_value(download.data.unwrap()["transfer_ref"].clone()).unwrap();
    let download_status =
        wait_transfer(&backend, &host, &mut receiver, &client, &download_ref).await;
    assert_eq!(download_status["state"], "completed", "{download_status}");
    assert_eq!(download_status["entry_kind"], "directory");
    assert_eq!(download_status["strategy"], "tar_archive");
    assert!(download_status["item_count"].as_u64().unwrap_or(0) >= 6);
    assert_eq!(
        std::fs::read_to_string(dst_dir.join("file_0.txt")).unwrap(),
        "payload-0"
    );
    assert_eq!(
        std::fs::read_to_string(dst_dir.join("sub").join("nested.txt")).unwrap(),
        "nested-content"
    );

    server.abort();
}
