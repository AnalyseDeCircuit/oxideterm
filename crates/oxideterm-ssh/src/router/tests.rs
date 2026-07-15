#[cfg(test)]
mod tests {
    use super::*;

    fn bind_active_node(
        registry: &SshConnectionRegistry,
        router: &NodeRouter,
        node_id: &NodeId,
        config: SshConfig,
    ) -> SshConnectionHandle {
        let handle = registry.acquire(
            config,
            ConnectionConsumer::NodeRouter(node_id.0.clone()),
        );
        handle.set_physical(Arc::new(()));
        registry.mark_state(handle.connection_id(), ConnectionState::Active);
        router
            .bind_connection(node_id, handle.connection_id().to_string())
            .unwrap();
        handle
    }

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
                root,
                child,
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
    fn runtime_tree_snapshot_restores_legacy_auto_route_origin() {
        let store = NodeRuntimeStore::default();
        let node_id = NodeId::new("legacy-auto-route");
        store.upsert_node_with_origin(
            node_id.clone(),
            SshConfig::password("target", 22, "me", "pw"),
            NodeOrigin::AutoRoute {
                target_host: "target".to_string(),
                route_id: "legacy-route".to_string(),
                hop_index: 0,
            },
        );

        let restored = NodeRuntimeStore::default();
        restored.apply_snapshot(store.export_snapshot()).unwrap();

        let snapshot = restored.snapshot(&node_id).unwrap();
        assert!(matches!(
            snapshot.origin,
            NodeOrigin::AutoRoute {
                target_host,
                route_id,
                hop_index: 0,
            } if target_host == "target" && route_id == "legacy-route"
        ));
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
    fn expand_manual_preset_under_parent_materializes_chain_below_ready_parent() {
        let store = NodeRuntimeStore::default();
        let parent = NodeId::new("root");
        store.upsert_node(parent.clone(), SshConfig::password("root", 22, "me", "pw"));
        store
            .apply_node_readiness(&parent, NodeReadiness::Ready, "")
            .unwrap();

        let expansion = store
            .expand_manual_preset_under_parent(
                parent.clone(),
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
        let path = store.path_to_node(&expansion.target_node_id).unwrap();
        assert_eq!(path.first(), Some(&parent));
        assert_eq!(&path[1..], expansion.path_node_ids.as_slice());

        let flat = store.flatten();
        assert_eq!(flat.len(), 4);
        assert_eq!(flat[0].id, "root");
        assert_eq!(flat[1].parent_id.as_deref(), Some("root"));
        assert_eq!(flat[2].parent_id.as_deref(), Some(flat[1].id.as_str()));
        assert_eq!(flat[3].parent_id.as_deref(), Some(flat[2].id.as_str()));

        let target = store.snapshot(&expansion.target_node_id).unwrap();
        assert_eq!(target.depth, 3);
        assert_eq!(
            target.origin,
            NodeOrigin::ManualPreset {
                saved_connection_id: "saved-a".to_string(),
                hop_index: 2,
            }
        );
    }

    #[test]
    fn expand_manual_preset_under_parent_requires_ready_parent() {
        let store = NodeRuntimeStore::default();
        let parent = NodeId::new("root");
        store.upsert_node(parent.clone(), SshConfig::password("root", 22, "me", "pw"));

        assert!(matches!(
            store.expand_manual_preset_under_parent(
                parent,
                "saved-a",
                Vec::new(),
                SshConfig::password("target", 22, "me", "pw"),
            ),
            Err(RouteError::ParentNotConnected(_))
        ));
    }

    #[test]
    fn remove_subtree_detaches_parent_child_links() {
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

        let removed = store.remove_subtree(&expansion.path_node_ids[0]);

        assert_eq!(removed.len(), 3);
        assert!(store.flatten().is_empty());
        assert!(store.snapshot(&expansion.target_node_id).is_none());
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

    #[test]
    fn acquire_wait_follows_runtime_rebind_during_reconnect() {
        let registry = SshConnectionRegistry::default();
        let router = NodeRouter::new(registry.clone());
        let node = NodeId::new("node-a");
        let config = SshConfig::password("host", 22, "me", "pw");
        router.upsert_node(node.clone(), config.clone());
        let old = registry.acquire(
            config.clone(),
            ConnectionConsumer::NodeRouter("node-a".into()),
        );
        router
            .bind_connection(&node, old.connection_id().to_string())
            .unwrap();
        registry.mark_state(old.connection_id(), ConnectionState::LinkDown);
        let old_connection_id = old.connection_id().to_string();

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let rebound_router = router.clone();
        let rebound_registry = registry;
        let rebound_node = node.clone();
        runtime.spawn(async move {
            sleep(Duration::from_millis(50)).await;
            let _ = rebound_registry.retire_connection(&old_connection_id);
            let new = rebound_registry.acquire(
                config,
                ConnectionConsumer::NodeRouter("node-a".into()),
            );
            new.set_physical(Arc::new(()));
            rebound_registry.mark_state(new.connection_id(), ConnectionState::Active);
            rebound_router
                .bind_connection(&rebound_node, new.connection_id().to_string())
                .unwrap();
        });

        let resolved = runtime
            .block_on(router.acquire_connection_wait(
                &node,
                ConnectionConsumer::PortForward("node:a".into()),
                Duration::from_millis(500),
            ))
            .unwrap();

        assert_eq!(resolved.handle.state(), ConnectionState::Active);
        assert_eq!(
            resolved.handle.info().consumers,
            vec![
                ConnectionConsumer::NodeRouter("node-a".into()),
                ConnectionConsumer::PortForward("node:a".into()),
            ]
        );
    }

    #[test]
    fn ssh_matrix_proxy_child_terminal_close_keeps_node_owned_liveness() {
        let registry = SshConnectionRegistry::default();
        let router = NodeRouter::new(registry.clone());
        let parent_id = NodeId::new("jump");
        let child_id = NodeId::new("target");
        let parent_config = SshConfig::password("jump", 22, "me", "pw");
        let child_config = SshConfig::password("target", 22, "me", "pw");
        router.upsert_node(parent_id.clone(), parent_config.clone());
        router
            .runtime_store()
            .upsert_child_node(parent_id.clone(), child_id.clone(), child_config.clone())
            .unwrap();

        let parent = bind_active_node(&registry, &router, &parent_id, parent_config);
        let child = bind_active_node(&registry, &router, &child_id, child_config.clone());
        registry.set_parent_connection_id(
            child.connection_id(),
            Some(parent.connection_id().to_string()),
        );

        let terminal_consumer = ConnectionConsumer::Terminal("term-target".to_string());
        let terminal = registry.acquire(child_config, terminal_consumer.clone());
        assert_eq!(terminal.connection_id(), child.connection_id());
        router
            .bind_terminal_endpoint(
                &child_id,
                TerminalEndpoint {
                    ws_port: 0,
                    ws_token: "native-terminal-term-target".to_string(),
                    session_id: "term-target".to_string(),
                },
            )
            .unwrap();

        router
            .unbind_terminal_session(&child_id, "term-target")
            .unwrap();
        registry.release(child.connection_id(), &terminal_consumer);

        let sftp = router
            .acquire_connection(&child_id, ConnectionConsumer::Sftp("target:sftp".to_string()))
            .unwrap();
        let forward = router
            .acquire_connection(
                &child_id,
                ConnectionConsumer::PortForward("target:forward".to_string()),
            )
            .unwrap();

        assert_eq!(sftp.connection_id, child.connection_id());
        assert_eq!(forward.connection_id, child.connection_id());
        assert!(parent.info().consumers.contains(&ConnectionConsumer::NodeRouter(
            "jump".to_string()
        )));
        assert!(!parent
            .info()
            .consumers
            .contains(&ConnectionConsumer::Sftp("target:sftp".to_string())));
    }

    #[test]
    fn ssh_matrix_parent_link_down_blocks_child_consumers_and_emits_affected_children() {
        let registry = SshConnectionRegistry::default();
        let router = NodeRouter::new(registry.clone());
        let parent_id = NodeId::new("jump");
        let child_id = NodeId::new("target");
        let parent_config = SshConfig::password("jump", 22, "me", "pw");
        let child_config = SshConfig::password("target", 22, "me", "pw");
        router.upsert_node(parent_id.clone(), parent_config.clone());
        router
            .runtime_store()
            .upsert_child_node(parent_id.clone(), child_id.clone(), child_config.clone())
            .unwrap();

        let parent = bind_active_node(&registry, &router, &parent_id, parent_config);
        let child = bind_active_node(&registry, &router, &child_id, child_config);
        registry.set_parent_connection_id(
            child.connection_id(),
            Some(parent.connection_id().to_string()),
        );
        let (tx, rx) = mpsc::channel();
        router.emitter().subscribe(tx);

        registry.mark_link_down_cascade(parent.connection_id());

        assert_eq!(parent.state(), ConnectionState::LinkDown);
        assert_eq!(child.state(), ConnectionState::LinkDown);
        assert!(matches!(
            router.acquire_connection(
                &child_id,
                ConnectionConsumer::PortForward("target:forward".to_string())
            ),
            Err(RouteError::NotConnected(_))
        ));
        assert!(!child
            .info()
            .consumers
            .contains(&ConnectionConsumer::PortForward("target:forward".to_string())));

        let events = rx.try_iter().collect::<Vec<_>>();
        assert!(events.iter().any(|event| matches!(
            event,
            NodeStateEvent::ConnectionStatusChanged {
                connection_id,
                status,
                affected_children,
                ..
            } if connection_id == parent.connection_id()
                && status == "link_down"
                && affected_children == &vec![child.connection_id().to_string()]
        )));
    }

    #[test]
    fn ssh_matrix_manual_disconnect_subtree_prevents_reconnect_restore_acquire() {
        let registry = SshConnectionRegistry::default();
        let router = NodeRouter::new(registry.clone());
        let parent_id = NodeId::new("jump");
        let child_id = NodeId::new("target");
        let parent_config = SshConfig::password("jump", 22, "me", "pw");
        let child_config = SshConfig::password("target", 22, "me", "pw");
        router.upsert_node(parent_id.clone(), parent_config.clone());
        router
            .runtime_store()
            .upsert_child_node(parent_id.clone(), child_id.clone(), child_config.clone())
            .unwrap();

        let parent = bind_active_node(&registry, &router, &parent_id, parent_config);
        let child = bind_active_node(&registry, &router, &child_id, child_config);
        registry.set_parent_connection_id(
            child.connection_id(),
            Some(parent.connection_id().to_string()),
        );
        let affected = router.runtime_store().subtree_postorder(&parent_id);
        assert_eq!(affected, vec![child_id.clone(), parent_id.clone()]);

        for node_id in affected {
            router
                .disconnect_node_runtime(&node_id, "manual disconnect")
                .unwrap();
        }

        assert!(matches!(
            router.acquire_connection(&parent_id, ConnectionConsumer::Sftp("jump:sftp".into())),
            Err(RouteError::NotConnected(_))
        ));
        assert!(matches!(
            router.acquire_connection(
                &child_id,
                ConnectionConsumer::PortForward("target:forward".into())
            ),
            Err(RouteError::NotConnected(_))
        ));
        assert!(router.connection_id_for_node(&parent_id).is_none());
        assert!(router.connection_id_for_node(&child_id).is_none());
    }

    #[test]
    fn ssh_matrix_reconnect_restore_acquire_follows_proxy_child_rebind() {
        let registry = SshConnectionRegistry::default();
        let router = NodeRouter::new(registry.clone());
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let parent_id = NodeId::new("jump");
        let child_id = NodeId::new("target");
        let parent_config = SshConfig::password("jump", 22, "me", "pw");
        let child_config = SshConfig::password("target", 22, "me", "pw");
        router.upsert_node(parent_id.clone(), parent_config.clone());
        router
            .runtime_store()
            .upsert_child_node(parent_id.clone(), child_id.clone(), child_config.clone())
            .unwrap();

        let parent = bind_active_node(&registry, &router, &parent_id, parent_config);
        let old_child = bind_active_node(&registry, &router, &child_id, child_config.clone());
        registry.set_parent_connection_id(
            old_child.connection_id(),
            Some(parent.connection_id().to_string()),
        );
        registry.mark_state(old_child.connection_id(), ConnectionState::LinkDown);
        let old_child_connection_id = old_child.connection_id().to_string();

        let rebound_router = router.clone();
        let rebound_registry = registry;
        let rebound_child_id = child_id.clone();
        let parent_connection_id = parent.connection_id().to_string();
        runtime.spawn(async move {
            sleep(Duration::from_millis(50)).await;
            let _ = rebound_registry.retire_connection(&old_child_connection_id);
            let new_child = rebound_registry.acquire(
                child_config,
                ConnectionConsumer::NodeRouter("target".to_string()),
            );
            new_child.set_physical(Arc::new(()));
            rebound_registry.mark_state(new_child.connection_id(), ConnectionState::Active);
            rebound_registry
                .set_parent_connection_id(new_child.connection_id(), Some(parent_connection_id));
            rebound_router
                .bind_connection(&rebound_child_id, new_child.connection_id().to_string())
                .unwrap();
        });

        let resolved = runtime
            .block_on(router.acquire_connection_wait(
                &child_id,
                ConnectionConsumer::PortForward("target:forward".into()),
                Duration::from_millis(500),
            ))
            .unwrap();

        assert_ne!(resolved.connection_id, old_child.connection_id());
        assert_eq!(resolved.handle.state(), ConnectionState::Active);
        assert_eq!(
            resolved.handle.info().parent_connection_id.as_deref(),
            Some(parent.connection_id())
        );
        assert!(resolved
            .handle
            .info()
            .consumers
            .contains(&ConnectionConsumer::PortForward("target:forward".into())));
    }
}
