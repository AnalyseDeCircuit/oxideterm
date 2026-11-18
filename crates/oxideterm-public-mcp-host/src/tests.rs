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
    ) -> Result<ForwardRule, String> {
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
        _created_by_client: bool,
        _persisted: bool,
    ) -> bool {
        unreachable!("the request must not reach forwarding")
    }
}

fn router() -> NodeRouter {
    NodeRouter::new(SshConnectionRegistry::new(ConnectionPoolConfig::default()))
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
    let lease = |client_ref: ClientRef, node: &str| PublicMcpNodeLease {
        client_ref,
        node_id: NodeId::new(node),
        saved_connection_id: None,
        physical_connection_id: None,
        consumer: ConnectionConsumer::PublicMcp(node.to_owned()),
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
    host.revoke_client_runtime(&alice, &router());
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

    let (_path, host, backend, client, mut requests) =
        dispatch_fixture(&[ToolGroup::DesktopObserve, ToolGroup::ArtifactTransfer]);
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
            desktop_ref: desktop,
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
