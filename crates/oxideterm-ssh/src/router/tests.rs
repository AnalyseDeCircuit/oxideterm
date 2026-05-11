#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_node_to_shared_connection() {
        let registry = SshConnectionRegistry::default();
        let router = NodeRouter::new(registry.clone());
        let node = NodeId::new("node-a");
        let config = SshConfig::password("host", 22, "me", "pw");
        router.upsert_node(node.clone(), config.clone());
        let terminal = registry.acquire(config, ConnectionConsumer::Terminal("term-a".into()));
        terminal.set_physical(Arc::new(()));
        registry.mark_state(terminal.connection_id(), ConnectionState::Active);
        router
            .bind_connection(&node, terminal.connection_id().to_string())
            .unwrap();
        router
            .bind_terminal_session(&node, "term-a".to_string())
            .unwrap();

        let resolved = router
            .acquire_connection(&node, ConnectionConsumer::NodeRouter("node-a".into()))
            .unwrap();
        let state = router.node_state(&node).unwrap();

        assert_eq!(state.state.readiness, NodeReadiness::Ready);
        assert_eq!(resolved.terminal_session_id.as_deref(), Some("term-a"));
        assert!(!resolved.connection_id.is_empty());
    }

    #[test]
    fn terminal_url_tracks_bound_endpoint() {
        let registry = SshConnectionRegistry::default();
        let router = NodeRouter::new(registry);
        let node = NodeId::new("node-a");
        router.upsert_node(node.clone(), SshConfig::password("host", 22, "me", "pw"));

        let endpoint = TerminalEndpoint {
            ws_port: 0,
            ws_token: "native-terminal-term-a".to_string(),
            session_id: "term-a".to_string(),
        };
        router
            .bind_terminal_endpoint(&node, endpoint.clone())
            .unwrap();

        assert_eq!(router.terminal_url(&node).unwrap(), endpoint);

        router.unbind_terminal_session(&node, "term-a").unwrap();
        assert!(matches!(
            router.terminal_url(&node),
            Err(RouteError::NotConnected(_))
        ));
    }

    #[test]
    fn runtime_tree_snapshot_preserves_origin_and_topology() {
        let store = NodeRuntimeStore::default();
        let root = NodeId::new("root");
        let child = NodeId::new("child");
        store.upsert_node_with_origin(
            root.clone(),
            SshConfig::password("jump", 22, "me", "pw"),
            NodeOrigin::ManualPreset {
                saved_connection_id: "saved-a".to_string(),
                hop_index: 0,
            },
        );
        store
            .upsert_child_node_with_origin(
                root.clone(),
                child.clone(),
                SshConfig::password("target", 22, "me", "pw"),
                NodeOrigin::ManualPreset {
                    saved_connection_id: "saved-a".to_string(),
                    hop_index: 1,
                },
            )
            .unwrap();

        let snapshot = store.export_snapshot();
        let restored = NodeRuntimeStore::default();
        restored.apply_snapshot(snapshot).unwrap();

        let flat = restored.flatten();
        assert_eq!(flat.len(), 2);
        assert_eq!(flat[0].id, "root");
        assert_eq!(flat[0].origin_type, "manual_preset");
        assert_eq!(flat[1].id, "child");
        assert_eq!(flat[1].parent_id.as_deref(), Some("root"));
        assert_eq!(restored.summary().max_depth, 1);
    }

    #[test]
    fn expand_manual_preset_materializes_each_hop_as_own_node() {
        let store = NodeRuntimeStore::default();
        let expansion = store
            .expand_manual_preset(
                "saved-a",
                vec![
                    SshConfig::password("jump-a", 22, "me", "pw"),
                    SshConfig::password("jump-b", 22, "me", "pw"),
                ],
                SshConfig::password("target", 22, "me", "pw"),
            )
            .unwrap();

        assert_eq!(expansion.chain_depth, 3);
        assert_eq!(expansion.path_node_ids.len(), 3);
        assert_eq!(
            expansion.path_node_ids.last(),
            Some(&expansion.target_node_id)
        );

        let flat = store.flatten();
        assert_eq!(flat.len(), 3);
        assert_eq!(flat[0].origin_type, "manual_preset");
        assert_eq!(flat[1].parent_id.as_deref(), Some(flat[0].id.as_str()));
        assert_eq!(flat[2].parent_id.as_deref(), Some(flat[1].id.as_str()));

        let target = store.snapshot(&expansion.target_node_id).unwrap();
        assert_eq!(target.depth, 2);
        assert_eq!(
            target.origin,
            NodeOrigin::ManualPreset {
                saved_connection_id: "saved-a".to_string(),
                hop_index: 2,
            }
        );
    }

    #[test]
    fn drill_down_requires_ready_parent_like_tauri_tree() {
        let store = NodeRuntimeStore::default();
        let root = NodeId::new("root");
        store.upsert_node(root.clone(), SshConfig::password("jump", 22, "me", "pw"));

        assert!(matches!(
            store.drill_down(root.clone(), SshConfig::password("child", 22, "me", "pw")),
            Err(RouteError::ParentNotConnected(_))
        ));

        {
            let mut snapshot = store.snapshot(&root).unwrap();
            snapshot.state.readiness = NodeReadiness::Ready;
            store
                .apply_snapshot(NodeTreeSnapshot {
                    version: 1,
                    exported_at_ms: now_ms(),
                    root_ids: vec![root.clone()],
                    nodes: vec![NodeTreeSnapshotNode {
                        id: root.clone(),
                        parent_id: None,
                        children_ids: Vec::new(),
                        depth: 0,
                        config: snapshot.config,
                        origin: snapshot.origin,
                        state: snapshot.state,
                        connection_id: snapshot.connection_id,
                        terminal_session_id: snapshot.terminal_session_id,
                        sftp_session_id: snapshot.sftp_session_id,
                        created_at_ms: snapshot.created_at_ms,
                        generation: snapshot.generation,
                    }],
                })
                .unwrap();
        }

        let child = store
            .drill_down(root.clone(), SshConfig::password("child", 22, "me", "pw"))
            .unwrap();
        let path = store.path_to_node(&child).unwrap();
        assert_eq!(path, vec![root, child]);
    }

    #[test]
    fn reconcile_runtime_tree_clears_missing_runtime_connection() {
        let registry = SshConnectionRegistry::default();
        let router = NodeRouter::new(registry);
        let node = NodeId::new("node-a");
        router
            .apply_tree_snapshot(NodeTreeSnapshot {
                version: 1,
                exported_at_ms: now_ms(),
                root_ids: vec![node.clone()],
                nodes: vec![NodeTreeSnapshotNode {
                    id: node.clone(),
                    parent_id: None,
                    children_ids: Vec::new(),
                    depth: 0,
                    config: SshConfig::password("host", 22, "me", "pw"),
                    origin: NodeOrigin::Direct,
                    state: NodeState {
                        readiness: NodeReadiness::Ready,
                        error: None,
                        sftp_ready: true,
                        sftp_cwd: Some("/home/me".to_string()),
                        ws_endpoint: Some(TerminalEndpoint {
                            ws_port: 0,
                            ws_token: "token".to_string(),
                            session_id: "term-a".to_string(),
                        }),
                    },
                    connection_id: Some("missing-connection".to_string()),
                    terminal_session_id: Some("term-a".to_string()),
                    sftp_session_id: Some("sftp-a".to_string()),
                    created_at_ms: now_ms(),
                    generation: 1,
                }],
            })
            .unwrap();

        router.reconcile_runtime_tree();
        let state = router.node_state(&node).unwrap();
        let snapshot = router.runtime_store().snapshot(&node).unwrap();

        assert_eq!(state.state.readiness, NodeReadiness::Disconnected);
        assert!(snapshot.connection_id.is_none());
        assert!(snapshot.terminal_session_id.is_none());
        assert!(snapshot.state.ws_endpoint.is_none());
    }

    #[test]
    fn disconnect_node_runtime_clears_connection_and_session_metadata() {
        let registry = SshConnectionRegistry::default();
        let router = NodeRouter::new(registry.clone());
        let node = NodeId::new("node-a");
        let config = SshConfig::password("host", 22, "me", "pw");
        router.upsert_node(node.clone(), config.clone());
        let handle = registry.acquire(config, ConnectionConsumer::NodeRouter("node-a".into()));
        router
            .bind_connection(&node, handle.connection_id().to_string())
            .unwrap();
        router
            .bind_terminal_endpoint(
                &node,
                TerminalEndpoint {
                    ws_port: 0,
                    ws_token: "native-terminal-term-a".to_string(),
                    session_id: "term-a".to_string(),
                },
            )
            .unwrap();
        router.runtime_store().set_sftp_ready(&node, true, Some("/home/me".to_string())).unwrap();

        router
            .disconnect_node_runtime(&node, "explicit disconnect")
            .unwrap();
        let snapshot = router.runtime_store().snapshot(&node).unwrap();

        assert_eq!(snapshot.state.readiness, NodeReadiness::Disconnected);
        assert!(snapshot.connection_id.is_none());
        assert!(snapshot.terminal_session_id.is_none());
        assert!(snapshot.sftp_session_id.is_none());
        assert!(!snapshot.state.sftp_ready);
        assert!(snapshot.state.sftp_cwd.is_none());
        assert!(snapshot.state.ws_endpoint.is_none());
        assert!(matches!(
            router.acquire_connection(&node, ConnectionConsumer::Sftp("node-a:sftp".into())),
            Err(RouteError::NotConnected(_))
        ));
    }

    #[test]
    fn disconnect_node_runtime_emits_sftp_ready_false_before_disconnected() {
        let registry = SshConnectionRegistry::default();
        let router = NodeRouter::new(registry);
        let node = NodeId::new("node-a");
        router.upsert_node(node.clone(), SshConfig::password("host", 22, "me", "pw"));
        router
            .bind_sftp_session(&node, "sftp-a", Some("/home/me".to_string()))
            .unwrap();

        let (tx, rx) = mpsc::channel();
        router.emitter().subscribe(tx);

        router
            .disconnect_node_runtime(&node, "explicit disconnect")
            .unwrap();

        let events = rx.try_iter().collect::<Vec<_>>();
        assert!(matches!(
            events.first(),
            Some(NodeStateEvent::SftpReady {
                node_id,
                ready: false,
                cwd: None,
                ..
            }) if node_id == "node-a"
        ));
        assert!(matches!(
            events.get(1),
            Some(NodeStateEvent::ConnectionStateChanged {
                node_id,
                state: NodeReadiness::Disconnected,
                reason,
                ..
            }) if node_id == "node-a" && reason == "explicit disconnect"
        ));
    }

    #[test]
    fn acquiring_consumer_does_not_revive_link_down_connection() {
        let registry = SshConnectionRegistry::default();
        let router = NodeRouter::new(registry.clone());
        let node = NodeId::new("node-a");
        let config = SshConfig::password("host", 22, "me", "pw");
        router.upsert_node(node.clone(), config.clone());
        let terminal = registry.acquire(config, ConnectionConsumer::Terminal("term-a".into()));
        router
            .bind_connection(&node, terminal.connection_id().to_string())
            .unwrap();

        registry.mark_state(terminal.connection_id(), ConnectionState::LinkDown);

        assert!(matches!(
            router.acquire_connection(&node, ConnectionConsumer::PortForward("node:a".into())),
            Err(RouteError::NotConnected(_))
        ));
        assert_eq!(terminal.state(), ConnectionState::LinkDown);
    }

    #[test]
    fn acquire_wait_rejects_active_entry_without_transport() {
        let registry = SshConnectionRegistry::default();
        let router = NodeRouter::new(registry.clone());
        let node = NodeId::new("node-a");
        let config = SshConfig::password("host", 22, "me", "pw");
        router.upsert_node(node.clone(), config.clone());
        let handle = registry.acquire(config, ConnectionConsumer::NodeRouter("node-a".into()));
        router
            .bind_connection(&node, handle.connection_id().to_string())
            .unwrap();
        registry.mark_state(handle.connection_id(), ConnectionState::Active);

        assert!(matches!(
            router.acquire_connection(&node, ConnectionConsumer::Sftp("node-a:sftp".into())),
            Err(RouteError::NotConnected(_))
        ));
        registry.mark_state(handle.connection_id(), ConnectionState::Active);

        let result =
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(router.acquire_connection_wait(
                    &node,
                    ConnectionConsumer::Sftp("node-a:sftp".into()),
                    Duration::from_millis(20),
                ));

        assert!(matches!(result, Err(RouteError::NotConnected(_))));
        assert_eq!(handle.state(), ConnectionState::LinkDown);
    }
}
