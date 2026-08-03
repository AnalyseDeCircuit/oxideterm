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
            ws_token: Zeroizing::new("native-terminal-term-a".to_string()),
            session_id: "term-a".to_string(),
        };
        let event = router
            .bind_terminal_endpoint(&node, endpoint.clone())
            .unwrap();
        assert!(matches!(
            event,
            NodeStateEvent::TerminalEndpointChanged {
                available: true,
                ..
            }
        ));

        assert_eq!(router.terminal_url(&node).unwrap(), endpoint);

        router.unbind_terminal_session(&node, "term-a").unwrap();
        assert!(matches!(
            router.terminal_url(&node),
            Err(RouteError::NotConnected(_))
        ));
    }

    #[test]
    fn metadata_snapshot_omits_authentication_and_endpoint_tokens() {
        let store = NodeRuntimeStore::default();
        let node = NodeId::new("node-a");
        let config = SshConfig::password(
            "example.test",
            22,
            "deploy",
            "representative-password",
        );
        store.upsert_node(node.clone(), config);
        store
            .bind_terminal_endpoint(
                &node,
                TerminalEndpoint {
                    ws_port: 8022,
                    ws_token: Zeroizing::new("representative-endpoint-token".to_string()),
                    session_id: "term-a".to_string(),
                },
            )
            .unwrap();

        let metadata = store.metadata_snapshots();
        let debug_output = format!("{metadata:?}");

        assert_eq!(metadata[0].host, "example.test");
        assert_eq!(metadata[0].username, "deploy");
        assert_eq!(metadata[0].origin, NodeOrigin::Direct);
        assert!(!debug_output.contains("representative-password"));
        assert!(!debug_output.contains("representative-endpoint-token"));

        store
            .update_origin(
                &node,
                NodeOrigin::Restored {
                    saved_connection_id: "saved-connection".to_string(),
                },
            )
            .unwrap();
        assert_eq!(
            store.metadata_snapshot(&node).unwrap().origin,
            NodeOrigin::Restored {
                saved_connection_id: "saved-connection".to_string(),
            }
        );
    }

    #[test]
    fn persistence_snapshot_excludes_runtime_secrets_and_endpoints() {
        let store = NodeRuntimeStore::default();
        let secret_root = NodeId::new("secret-root");
        let mut secret_root_snapshot = snapshot_node("secret-root", None, 0, Vec::new());
        secret_root_snapshot.config = SshConfig::password(
            "secret.example.test",
            22,
            "deploy",
            "representative-password",
        );
        secret_root_snapshot.state.readiness = NodeReadiness::Ready;
        store
            .apply_snapshot(NodeTreeSnapshot {
                version: 1,
                exported_at_ms: now_ms(),
                root_ids: vec![secret_root.clone()],
                nodes: vec![secret_root_snapshot],
            })
            .unwrap();
        let secret_child = store
            .drill_down(
                secret_root,
                SshConfig {
                    host: "child.example.test".to_string(),
                    username: "deploy".to_string(),
                    auth: crate::AuthMethod::Agent,
                    ..SshConfig::default()
                },
            )
            .unwrap();

        let saved_node = NodeId::new("saved-node");
        store.upsert_node_with_origin(
            saved_node.clone(),
            SshConfig::password(
                "saved.example.test",
                22,
                "deploy",
                "saved-store-password",
            ),
            NodeOrigin::Restored {
                saved_connection_id: "saved-connection".to_string(),
            },
        );
        store
            .bind_terminal_endpoint(
                &saved_node,
                TerminalEndpoint {
                    ws_port: 8022,
                    ws_token: Zeroizing::new("representative-endpoint-token".to_string()),
                    session_id: "term-a".to_string(),
                },
            )
            .unwrap();

        let snapshot = store.export_persistence_snapshot();
        let debug_output = format!("{snapshot:?}");

        assert_eq!(snapshot.nodes.len(), 1);
        assert_eq!(snapshot.nodes[0].id, saved_node);
        assert!(snapshot.nodes[0].config.is_none());
        assert!(!snapshot.nodes.iter().any(|node| node.id == secret_child));
        assert!(!debug_output.contains("representative-password"));
        assert!(!debug_output.contains("saved-store-password"));
        assert!(!debug_output.contains("representative-endpoint-token"));
    }

    #[test]
    fn minimal_subtree_roots_drop_descendant_candidates_without_config_snapshots() {
        let store = NodeRuntimeStore::default();
        let root = NodeId::new("root");
        let child = NodeId::new("child");
        let sibling = NodeId::new("sibling");
        store.upsert_node(
            root.clone(),
            SshConfig {
                host: "root.example.test".to_string(),
                auth: crate::AuthMethod::Agent,
                ..SshConfig::default()
            },
        );
        store
            .upsert_child_node(
                root.clone(),
                child.clone(),
                SshConfig {
                    host: "child.example.test".to_string(),
                    auth: crate::AuthMethod::Agent,
                    ..SshConfig::default()
                },
            )
            .unwrap();
        store.upsert_node(
            sibling.clone(),
            SshConfig {
                host: "sibling.example.test".to_string(),
                auth: crate::AuthMethod::Agent,
                ..SshConfig::default()
            },
        );

        assert_eq!(
            store.minimal_subtree_roots([child, sibling.clone(), root.clone()]),
            vec![root, sibling]
        );
    }

    #[test]
    fn removing_primary_terminal_elects_another_endpoint() {
        let router = NodeRouter::new(SshConnectionRegistry::default());
        let node = NodeId::new("node-a");
        router.upsert_node(node.clone(), SshConfig::password("host", 22, "me", "pw"));
        let first = TerminalEndpoint {
            ws_port: 0,
            ws_token: Zeroizing::new("first-token".to_string()),
            session_id: "term-a".to_string(),
        };
        let second = TerminalEndpoint {
            ws_port: 0,
            ws_token: Zeroizing::new("second-token".to_string()),
            session_id: "term-b".to_string(),
        };

        router.bind_terminal_endpoint(&node, first.clone()).unwrap();
        router.bind_terminal_endpoint(&node, second.clone()).unwrap();
        assert_eq!(router.terminal_url(&node).unwrap(), first);

        router.unbind_terminal_session(&node, "term-a").unwrap();

        assert_eq!(router.terminal_url(&node).unwrap(), second);
        let snapshot = router.runtime_store().snapshot(&node).unwrap();
        assert_eq!(snapshot.terminal_session_id.as_deref(), Some("term-b"));
        let tree_snapshot = router.export_tree_snapshot();
        assert_eq!(tree_snapshot.nodes[0].terminal_endpoints.len(), 1);
    }

    #[test]
    fn snapshot_rejects_parent_cycles_without_mutating_existing_tree() {
        let store = NodeRuntimeStore::default();
        let existing = NodeId::new("existing");
        store.upsert_node(
            existing.clone(),
            SshConfig::password("existing", 22, "me", "pw"),
        );
        let node_a = snapshot_node("a", Some("b"), 99, vec![NodeId::new("b")]);
        let node_b = snapshot_node("b", Some("a"), 99, vec![NodeId::new("a")]);

        let result = store.apply_snapshot(NodeTreeSnapshot {
            version: 1,
            exported_at_ms: now_ms(),
            root_ids: Vec::new(),
            nodes: vec![node_a, node_b],
        });

        assert!(matches!(result, Err(RouteError::ConnectionError(_))));
        assert!(store.snapshot(&existing).is_some());
    }

    #[test]
    fn snapshot_derives_children_depth_and_roots_from_parent_links() {
        let store = NodeRuntimeStore::default();
        let child = snapshot_node("child", Some("root"), 42, Vec::new());
        let root = snapshot_node("root", None, 42, Vec::new());

        store
            .apply_snapshot(NodeTreeSnapshot {
                version: 1,
                exported_at_ms: now_ms(),
                root_ids: vec![NodeId::new("wrong")],
                nodes: vec![child, root],
            })
            .unwrap();

        let flat = store.flatten();
        assert_eq!(flat.iter().map(|node| node.id.as_str()).collect::<Vec<_>>(), vec!["root", "child"]);
        assert_eq!(flat[1].depth, 1);
    }

    #[test]
    fn snapshot_preserves_valid_root_and_sibling_order_hints() {
        let store = NodeRuntimeStore::default();
        let child_b = snapshot_node("child-b", Some("root-a"), 1, Vec::new());
        let root_b = snapshot_node("root-b", None, 0, Vec::new());
        let child_a = snapshot_node("child-a", Some("root-a"), 1, Vec::new());
        let root_a = snapshot_node(
            "root-a",
            None,
            0,
            vec![NodeId::new("child-a"), NodeId::new("child-b")],
        );

        store
            .apply_snapshot(NodeTreeSnapshot {
                version: 1,
                exported_at_ms: now_ms(),
                root_ids: vec![NodeId::new("root-a"), NodeId::new("root-b")],
                nodes: vec![child_b, root_b, child_a, root_a],
            })
            .unwrap();

        assert_eq!(
            store
                .flatten()
                .iter()
                .map(|node| node.id.as_str())
                .collect::<Vec<_>>(),
            vec!["root-a", "child-a", "child-b", "root-b"]
        );
    }

    fn snapshot_node(
        id: &str,
        parent_id: Option<&str>,
        depth: u32,
        children_ids: Vec<NodeId>,
    ) -> NodeTreeSnapshotNode {
        NodeTreeSnapshotNode {
            id: NodeId::new(id),
            parent_id: parent_id.map(NodeId::new),
            children_ids,
            depth,
            config: SshConfig::password(id, 22, "me", "pw"),
            origin: NodeOrigin::Direct,
            state: NodeState::default(),
            connection_id: None,
            terminal_session_id: None,
            terminal_endpoints: Vec::new(),
            sftp_session_id: None,
            created_at_ms: now_ms(),
            generation: 0,
        }
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
    fn saved_connection_x11_policy_updates_only_materialized_targets() {
        let store = NodeRuntimeStore::default();
        let restored_target = NodeId::new("restored-target");
        store.upsert_node_with_origin(
            restored_target.clone(),
            SshConfig::password("direct", 22, "me", "pw"),
            NodeOrigin::Restored {
                saved_connection_id: "saved-a".to_string(),
            },
        );
        let proxy_expansion = store
            .expand_manual_preset(
                "saved-a",
                vec![SshConfig::password("jump", 22, "me", "pw")],
                SshConfig::password("proxied-target", 22, "me", "pw"),
            )
            .unwrap();
        let parent = NodeId::new("unrelated-parent");
        store.upsert_node(parent.clone(), SshConfig::password("parent", 22, "me", "pw"));
        store
            .apply_node_readiness(&parent, NodeReadiness::Ready, "")
            .unwrap();
        let nested_expansion = store
            .expand_manual_preset_under_parent(
                parent,
                "saved-a",
                vec![SshConfig::password("nested-jump", 22, "me", "pw")],
                SshConfig::password("nested-target", 22, "me", "pw"),
            )
            .unwrap();
        let target_generation = store
            .snapshot(&proxy_expansion.target_node_id)
            .unwrap()
            .generation;
        let x11_forwarding = Some(X11ForwardPolicy::trusted());

        assert_eq!(
            store.update_saved_connection_x11_forwarding("saved-a", x11_forwarding),
            3
        );
        assert_eq!(store.x11_forwarding(&restored_target).unwrap(), x11_forwarding);
        assert_eq!(
            store
                .x11_forwarding(&proxy_expansion.target_node_id)
                .unwrap(),
            x11_forwarding
        );
        assert_eq!(
            store
                .x11_forwarding(&nested_expansion.target_node_id)
                .unwrap(),
            x11_forwarding
        );
        assert_eq!(
            store
                .x11_forwarding(&proxy_expansion.path_node_ids[0])
                .unwrap(),
            None
        );
        assert_eq!(
            store
                .x11_forwarding(&nested_expansion.path_node_ids[0])
                .unwrap(),
            None
        );
        assert_eq!(
            store
                .snapshot(&proxy_expansion.target_node_id)
                .unwrap()
                .generation,
            target_generation + 1
        );
        assert_eq!(
            store.update_saved_connection_x11_forwarding("saved-a", x11_forwarding),
            0
        );
        assert_eq!(
            store.update_saved_connection_x11_forwarding("saved-a", None),
            3
        );
        assert_eq!(store.x11_forwarding(&restored_target).unwrap(), None);
        assert_eq!(
            store
                .x11_forwarding(&proxy_expansion.target_node_id)
                .unwrap(),
            None
        );
        assert_eq!(
            store
                .x11_forwarding(&proxy_expansion.path_node_ids[0])
                .unwrap(),
            None
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
            let mut snapshot = store.export_snapshot();
            snapshot.nodes[0].state.readiness = NodeReadiness::Ready;
            store.apply_snapshot(snapshot).unwrap();
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
                            ws_token: Zeroizing::new("token".to_string()),
                            session_id: "term-a".to_string(),
                        }),
                    },
                    connection_id: Some("missing-connection".to_string()),
                    terminal_session_id: Some("term-a".to_string()),
                    terminal_endpoints: Vec::new(),
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
                    ws_token: Zeroizing::new("native-terminal-term-a".to_string()),
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
    fn connection_attempt_preparation_clears_runtime_without_disconnect_event() {
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
            .bind_sftp_session(&node, "sftp-a", Some("/home/me".to_string()))
            .unwrap();
        let (tx, rx) = mpsc::channel();
        router.emitter().subscribe(tx);

        router.prepare_node_connection_attempt(&node).unwrap();

        let snapshot = router.runtime_store().snapshot(&node).unwrap();
        assert_eq!(snapshot.state.readiness, NodeReadiness::Disconnected);
        assert!(snapshot.connection_id.is_none());
        assert!(snapshot.sftp_session_id.is_none());
        assert!(rx.try_iter().next().is_none());
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
    fn active_registry_state_without_physical_transport_is_not_ready() {
        let registry = SshConnectionRegistry::default();
        let router = NodeRouter::new(registry.clone());
        let node = NodeId::new("node-a");
        let config = SshConfig::password("host", 22, "me", "pw");
        router.upsert_node(node.clone(), config.clone());
        let handle = registry.acquire(config, ConnectionConsumer::NodeRouter("node-a".into()));
        registry.mark_state(handle.connection_id(), ConnectionState::Active);

        router
            .bind_connection(&node, handle.connection_id().to_string())
            .unwrap();

        assert_ne!(
            router.node_state(&node).unwrap().state.readiness,
            NodeReadiness::Ready
        );
    }

    #[test]
    fn closing_terminal_consumer_does_not_change_node_readiness() {
        let registry = SshConnectionRegistry::default();
        let router = NodeRouter::new(registry.clone());
        let node = NodeId::new("node-a");
        let config = SshConfig::password("host", 22, "me", "pw");
        router.upsert_node(node.clone(), config.clone());
        let handle = bind_active_node(&registry, &router, &node, config.clone());
        let terminal_consumer = ConnectionConsumer::Terminal("term-a".into());
        let terminal_handle = registry.acquire(config, terminal_consumer.clone());
        assert_eq!(terminal_handle.connection_id(), handle.connection_id());
        router
            .bind_terminal_session(&node, "term-a".to_string())
            .unwrap();

        registry.release(handle.connection_id(), &terminal_consumer);
        router.unbind_terminal_session(&node, "term-a").unwrap();

        assert_eq!(
            router.node_state(&node).unwrap().state.readiness,
            NodeReadiness::Ready
        );
        assert!(handle.has_physical());
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
                    ws_token: Zeroizing::new("native-terminal-term-target".to_string()),
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
