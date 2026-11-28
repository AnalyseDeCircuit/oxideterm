use super::tabs::{TabAffinity, TabScope};
use super::*;
use oxideterm_workspace::session_retention::SessionRetention;

#[derive(Default)]
pub(super) struct RetentionRuntime {
    duration: Option<Duration>,
    // Freshly materialized SSH nodes also start disconnected. Only a close or restore admits one.
    closed_nodes: HashSet<NodeId>,
    clock: SessionRetention<TabAffinity>,
    wake: Option<(Instant, Task<()>)>,
}

impl WorkspaceApp {
    pub(super) fn configure_session_retention(&mut self, cx: &mut Context<Self>) {
        let seconds = self
            .settings_store
            .settings()
            .sidebar_ui
            .closed_session_retention_secs;
        self.session_retention.duration = oxideterm_settings::configured_timeout(seconds);
        if self.session_retention.duration.is_none() {
            self.session_retention.clock = SessionRetention::default();
            self.session_retention.wake = None;
            return;
        }
        self.sync_session_retention(cx);
    }

    pub(super) fn note_closed_ssh_session(&mut self, id: &NodeId) {
        self.session_retention.closed_nodes.insert(id.clone());
    }

    pub(super) fn forget_closed_ssh_session(&mut self, id: &NodeId) {
        if self.session_retention.closed_nodes.remove(id) {
            self.session_retention
                .clock
                .forget(&TabAffinity::Ssh(id.clone()));
        }
    }

    pub(super) fn begin_standalone_reconnect(
        &mut self,
        id: &str,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let attempt = self.standalone_connections.begin_reconnect(id)?;
        self.sync_session_retention(cx);
        Some(attempt)
    }

    /// Returns whether removing SSH nodes already requested a tree snapshot write.
    pub(super) fn sync_session_retention(&mut self, cx: &mut Context<Self>) -> bool {
        let duration = self.session_retention.duration;
        if duration.is_none() && self.session_retention.closed_nodes.is_empty() {
            return false;
        }
        let mut finished_discovery = HashSet::new();
        let mut eligible = HashSet::new();
        let form_owner = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .and_then(|form| form.standalone_connection_id.as_deref());
        for record in self.standalone_connections.closed_records() {
            if Some(record.id.as_str()) != form_owner {
                eligible.insert(TabAffinity::Standalone(record.id.clone()));
            }
        }
        if !self.session_retention.closed_nodes.is_empty() {
            let tabs = self
                .tab_host
                .read(cx)
                .tabs()
                .iter()
                .flat_map(|tab| self.tab_affinities(tab.id, cx))
                .collect::<HashSet<_>>();
            let runtime = self.workspace_runtime.read(cx);
            let leases = self.dedicated_sftp_connections.lock();
            for id in &self.session_retention.closed_nodes {
                if !self.ssh_nodes.get(id).is_some_and(|node| {
                    node.readiness == NodeReadiness::Disconnected && node.terminal_ids.is_empty()
                }) || runtime.has_session_connection_work(id)
                    || self.ssh_tunnels.has_pending_start(id)
                    || self.connections.flow.read(cx).proxy_chain_needs_node(id)
                    || leases
                        .get(id)
                        .and_then(|entry| entry.slot.upgrade())
                        .is_some_and(|slot| slot.try_lock().map_or(true, |lease| lease.is_some()))
                    || self
                        .forwarding_service
                        .manager_for_node(id)
                        .is_some_and(|manager| manager.active_forward_count() > 0)
                    || self
                        .sftp_transfer_manager
                        .has_unfinished_transfer_for_node(&id.0)
                {
                    continue;
                }
                let key = TabAffinity::Ssh(id.clone());
                if !tabs.contains(&key)
                    && self.node_router.node_metadata(id).is_some_and(|node| {
                        node.readiness == NodeReadiness::Disconnected
                            && node.connection_id.is_none()
                            && node.terminal_session_id.is_none()
                            && node.sftp_session_id.is_none()
                    })
                {
                    if self
                        .node_router
                        .runtime_store()
                        .discovery_origin(id)
                        .is_some()
                    {
                        finished_discovery.insert(key);
                    } else {
                        eligible.insert(key);
                    }
                }
            }
        }
        let removed_discovery = !finished_discovery.is_empty()
            && self.remove_expired_session_records(finished_discovery, cx);
        let Some(duration) = duration else {
            return removed_discovery;
        };
        let now = cx.background_executor().now();
        let (expired, next) = self
            .session_retention
            .clock
            .reconcile(eligible, duration, now);
        let snapshot_requested = (!expired.is_empty()
            && self.remove_expired_session_records(expired, cx))
            || removed_discovery;
        if self
            .session_retention
            .wake
            .as_ref()
            .map(|(deadline, _)| *deadline)
            != next
        {
            self.session_retention.wake = next.map(|deadline| {
                let timer = cx
                    .background_executor()
                    .timer(deadline.saturating_duration_since(now));
                (
                    deadline,
                    cx.spawn(async move |weak, cx| {
                        timer.await;
                        let _ = weak.update(cx, |workspace, cx| {
                            workspace.session_retention.wake = None;
                            workspace.sync_session_retention(cx);
                        });
                    }),
                )
            });
        }
        snapshot_requested
    }

    fn remove_expired_session_records(
        &mut self,
        expired: HashSet<TabAffinity>,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut nodes = Vec::new();
        let mut standalone = HashSet::new();
        for key in expired {
            match key {
                TabAffinity::Ssh(id) => {
                    if let Some(node) = self.node_router.node_metadata(&id) {
                        nodes.push((node.depth, id));
                    }
                }
                TabAffinity::Standalone(id) => {
                    standalone.insert(id);
                }
                _ => {}
            }
        }
        nodes.sort_unstable_by_key(|(depth, _)| std::cmp::Reverse(*depth));
        let mut removed_nodes = HashSet::new();
        for (_, id) in nodes {
            // An unexpired descendant keeps its entire route, even under a collapsed parent.
            if self
                .node_router
                .runtime_store()
                .remove_disconnected_leaf(&id)
            {
                self.remove_ssh_node_record(&id);
                removed_nodes.insert(id);
            }
        }
        let ssh_tree_changed = !removed_nodes.is_empty();
        if !ssh_tree_changed && standalone.is_empty() {
            return false;
        }
        if ssh_tree_changed {
            if self
                .active_ssh_node_id
                .as_ref()
                .is_some_and(|id| removed_nodes.contains(id))
            {
                self.active_ssh_node_id = None;
            }
            self.saved_ssh_nodes
                .retain(|_, id| !removed_nodes.contains(id));
            self.persist_session_tree_snapshot();
        }
        if !standalone.is_empty() {
            self.expanded_standalone_connections
                .retain(|id| !standalone.contains(id));
            self.standalone_connections
                .remove_closed_records(&standalone);
            for id in &standalone {
                self.session_retention
                    .clock
                    .forget(&TabAffinity::Standalone(id.clone()));
            }
        }
        // Retired records must not linger as sidebar selection targets; the
        // prune reads the live row list, which no longer contains them.
        self.prune_active_session_selection(cx);
        let removed_scope = match self.tab_host.read(cx).scope() {
            TabScope::Affinity(TabAffinity::Ssh(id)) => removed_nodes.contains(id),
            TabScope::Affinity(TabAffinity::Standalone(id)) => standalone.contains(id),
            _ => false,
        };
        if removed_scope {
            let previous = self
                .tab_host
                .update(cx, |host, _| host.switch_scope(TabScope::All));
            self.apply_main_window_active_tab_change(previous, self.active_tab_id(cx), cx);
            self.sync_active_tab_surface(cx);
        }
        cx.notify();
        ssh_tree_changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::session_manager::tests::isolated_workspace_fixture;
    use gpui::TestAppContext;
    use oxideterm_settings::ConnectionPoolSettings;

    fn config(host: &str) -> SshConfig {
        SshConfig {
            host: host.into(),
            username: "test".into(),
            auth: oxideterm_ssh::AuthMethod::Agent,
            ..Default::default()
        }
    }

    #[gpui::test]
    fn restored_records_expire_after_a_full_period_and_keep_an_occupied_jump_route(
        cx: &mut TestAppContext,
    ) {
        let Some(_dir) = isolated_workspace_fixture(cx, "OXIDETERM_RETENTION_RESTORE_DIR") else {
            return;
        };
        let mut settings = SettingsStore::load_from_path(default_settings_path()).unwrap();
        settings.settings_mut().onboarding_completed = true;
        settings.settings_mut().ssh_config.auto_load_hosts = false;
        settings
            .settings_mut()
            .sidebar_ui
            .closed_session_retention_secs = 5;
        settings.save().unwrap();
        let jump = NodeId::new("jump");
        let target = NodeId::new("target");
        let tree = NodeRouter::new(SshConnectionRegistry::default());
        tree.upsert_node(jump.clone(), config("jump.invalid"));
        tree.runtime_store()
            .upsert_child_node(jump.clone(), target.clone(), config("target.invalid"))
            .unwrap();
        let path = default_session_tree_path();
        write_session_tree_snapshot(&path, &tree.export_persistence_snapshot()).unwrap();
        let standalone_path = path.with_file_name("standalone_sessions.json");
        std::fs::write(&standalone_path, br#"[{"id":"console","title":"Console","kind":"Telnet","launch":{"type":"telnet","config":{"host":"console.invalid","port":23},"terminal":{}}}]"#).unwrap();
        cx.executor().allow_parking();
        let (shell, cx) = cx.add_window_view(|window, cx| {
            let ws = cx.new(|cx| WorkspaceApp::new(window, cx, None, None).unwrap());
            WorkspaceWindowShell::new(ws, window, cx)
        });
        let ws = shell.read_with(cx, |shell, _| shell.session_entity());
        let (fresh, transfers) = ws.update(cx, |ws, cx| {
            let fresh = ws.materialize_ssh_root_node(config("fresh.invalid"), "Fresh".into(), None);
            ws.sftp_transfer_manager
                .register_for_node("download", &target.0);
            ws.tab_host.update(cx, |host, _| {
                host.switch_scope(TabScope::Affinity(TabAffinity::Standalone(
                    "console".into(),
                )))
            });
            (fresh, ws.sftp_transfer_manager.clone())
        });
        cx.executor().advance_clock(Duration::from_secs(4));
        ws.read_with(cx, |ws, _| {
            assert_eq!(
                ws.standalone_connections.record("console").unwrap().title,
                "Console"
            )
        });
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        ws.read_with(cx, |ws, cx| {
            assert_eq!(ws.tab_host.read(cx).scope(), &TabScope::All);
            assert_eq!(
                ws.node_router.node_metadata(&jump).unwrap().children_ids,
                vec![target.clone()]
            );
            assert!(ws.standalone_connections.record("console").is_none());
        });
        // Hold the restored destination in the queue while the first hop awaits preflight.
        ws.update(cx, |ws, cx| {
            ws.connections.flow.update(cx, |flow, cx| {
                assert!(
                    flow.stage_existing_proxy_chain(
                        &ws.node_router,
                        target.clone(),
                        "Target".into(),
                        oxideterm_gpui_connections::launch::SshConnectionIntent::ConnectTemporary,
                        &mut None,
                        false,
                        |_| false,
                        |id| Some(oxideterm_ssh::NativeSessionTreeConnectEndpoint::new(
                            id.0.clone(),
                            22
                        )),
                        cx,
                    )
                    .unwrap()
                );
            });
        });
        transfers.unregister("download");
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_secs(6));
        cx.run_until_parked();
        ws.update(cx, |ws, cx| {
            assert!(ws.node_router.contains_node(&target));
            ws.cancel_active_proxy_connect_run(cx);
        });
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_secs(4));
        ws.read_with(cx, |ws, _| assert!(ws.node_router.contains_node(&target)));
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        ws.read_with(cx, |ws, _| {
            assert_eq!(ws.node_router.root_node_ids(), vec![fresh.clone()])
        });
        let saved: NodeTreePersistenceSnapshot =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(saved.root_ids, vec![fresh]);
        let standalone: serde_json::Value =
            serde_json::from_slice(&std::fs::read(standalone_path).unwrap()).unwrap();
        assert_eq!(standalone, serde_json::json!([]));
    }

    #[gpui::test]
    async fn failed_preconnect_preserves_a_peer_using_the_same_pool(cx: &mut TestAppContext) {
        let Some(_dir) = isolated_workspace_fixture(cx, "OXIDETERM_SHARED_GATE_DIR") else {
            return;
        };
        cx.executor().allow_parking();
        let (shell, cx) = cx.add_window_view(|window, cx| {
            let ws = cx.new(|cx| WorkspaceApp::new(window, cx, None, None).unwrap());
            WorkspaceWindowShell::new(ws, window, cx)
        });
        let ws = shell.read_with(cx, |shell, _| shell.session_entity());
        let (active, failed, transport, _terminal) = ws.update(cx, |ws, cx| {
            let config = config("shared-gate.invalid");
            let active = ws.materialize_ssh_root_node(config.clone(), "Active".into(), None);
            let transport = ws.ssh_registry.acquire(
                config.clone(),
                ConnectionConsumer::NodeRouter(active.0.clone()),
            );
            transport.set_physical(Arc::new(()));
            let id = transport.connection_id();
            ws.ssh_registry.mark_state(id, ConnectionState::Active);
            ws.node_router.bind_connection(&active, id).unwrap();
            let terminal = ws
                .ssh_registry
                .lease_consumer(id, ConnectionConsumer::Terminal("open-shell".into()))
                .unwrap();
            let failed = ws.materialize_ssh_root_node(
                SshConfig {
                    pre_connect_command: Some(
                        "oxideterm-test-nonexistent-preconnect-command".into(),
                    ),
                    ..config
                },
                "Failed gate".into(),
                None,
            );
            assert!(ws.ensure_node_connection_started_without_ancestors(&failed, cx));
            assert_eq!(
                ws.node_router
                    .resolve_connection_now(&active)
                    .unwrap()
                    .connection_id,
                id
            );
            assert!(ws.node_router.resolve_connection_now(&failed).is_err());
            (active, failed, transport, terminal)
        });
        cx.condition(&ws, |ws, _| {
            ws.ssh_nodes
                .get(&failed)
                .is_some_and(|node| node.readiness == NodeReadiness::Error)
        })
        .await;
        ws.read_with(cx, |ws, _| {
            assert_eq!(ws.node_router.connection_id_for_node(&failed), None);
            assert_eq!(
                ws.node_router
                    .resolve_connection_now(&active)
                    .unwrap()
                    .connection_id,
                transport.connection_id()
            );
            assert_eq!(transport.state(), ConnectionState::Active);
            assert_eq!(
                transport
                    .info()
                    .consumers
                    .into_iter()
                    .collect::<HashSet<_>>(),
                HashSet::from([
                    ConnectionConsumer::NodeRouter(active.0.clone()),
                    ConnectionConsumer::Terminal("open-shell".into())
                ])
            );
        });
    }

    #[gpui::test]
    fn closing_an_offline_forwarding_tab_restarts_the_full_retention_period(
        cx: &mut TestAppContext,
    ) {
        let Some(_dir) = isolated_workspace_fixture(cx, "OXIDETERM_RETENTION_FORWARD_TAB_DIR")
        else {
            return;
        };
        cx.executor().allow_parking();
        let (shell, cx) = cx.add_window_view(|window, cx| {
            let ws = cx.new(|cx| WorkspaceApp::new(window, cx, None, None).unwrap());
            WorkspaceWindowShell::new(ws, window, cx)
        });
        let ws = shell.read_with(cx, |shell, _| shell.session_entity());
        let node = cx.update(|window, cx| {
            ws.update(cx, |ws, cx| {
                ws.edit_settings(|s| s.sidebar_ui.closed_session_retention_secs = 60, cx);
                let node =
                    ws.materialize_ssh_root_node(config("offline.invalid"), "Offline".into(), None);
                ws.disconnect_ssh_node(&node, window, cx);
                node
            })
        });
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_secs(40));
        let tab = cx.update(|window, cx| {
            ws.update(cx, |ws, cx| {
                ws.open_forwards_tab(node.clone(), window, cx);
                ws.forwarding.read(cx).tab_for_node(&node).unwrap()
            })
        });
        cx.executor().advance_clock(Duration::from_secs(10));
        cx.update(|window, cx| ws.update(cx, |ws, cx| ws.close_tab_by_id(tab, window, cx)));
        cx.executor().advance_clock(Duration::from_secs(59));
        cx.run_until_parked();
        ws.read_with(cx, |ws, _| assert!(ws.node_router.contains_node(&node)));
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        ws.read_with(cx, |ws, _| assert!(!ws.node_router.contains_node(&node)));
    }

    #[gpui::test]
    fn removing_ssh_sessions_releases_node_histories_and_preserves_saved_history(
        cx: &mut TestAppContext,
    ) {
        let Some(_dir) = isolated_workspace_fixture(cx, "OXIDETERM_RETENTION_HISTORY_DIR") else {
            return;
        };
        cx.executor().allow_parking();
        let (shell, cx) = cx.add_window_view(|window, cx| {
            let ws = cx.new(|cx| WorkspaceApp::new(window, cx, None, None).unwrap());
            WorkspaceWindowShell::new(ws, window, cx)
        });
        let ws = shell.read_with(cx, |shell, _| shell.session_entity());
        cx.update(|window, cx| {
            ws.update(cx, |ws, cx| {
                ws.edit_settings(|s| s.sidebar_ui.closed_session_retention_secs = 0, cx);
                let temp = ws.materialize_ssh_root_node(
                    config("temporary.invalid"),
                    "Temporary".into(),
                    None,
                );
                let saved_config = config("saved.invalid");
                let saved = ws.materialize_ssh_root_node(
                    saved_config.clone(),
                    "Saved".into(),
                    Some("profile".into()),
                );
                let saved_history = ws.ssh_command_history(&saved).unwrap();
                // Seed the saved history through a terminal without opening a network connection.
                let pane = cx.new(|cx| {
                    TerminalPane::new_recording_playback(
                        80,
                        24,
                        TerminalUiPreferences {
                            command_history: SharedTerminalCommandHistory::from_commands(vec![
                                "pwd".into(),
                            ]),
                            ..Default::default()
                        },
                        window,
                        cx,
                    )
                    .unwrap()
                });
                pane.update(cx, |pane, cx| {
                    pane.bind_local_shell_history(saved_history.clone(), cx)
                });
                drop(pane);
                ws.ssh_terminal_command_histories
                    .insert(saved.clone(), saved_history);
                ws.ssh_terminal_command_histories.insert(
                    temp.clone(),
                    SharedTerminalCommandHistory::from_commands(vec!["hostname".into()]),
                );

                ws.disconnect_ssh_node(&temp, window, cx);
                assert!(!ws.ssh_terminal_command_histories.contains_key(&temp));
                assert_eq!(
                    ws.ssh_terminal_command_histories[&saved].records()[0].command,
                    "pwd"
                );
                let handle = ws.ssh_registry.acquire(
                    saved_config.clone(),
                    ConnectionConsumer::NodeRouter(saved.0.clone()),
                );
                ws.forwarding_service
                    .registry()
                    .register(ws.forwarding_session_id_for_node(&saved), handle);
                ws.remove_inactive_session_tree_node(&saved, window, cx);
                assert!(ws.forwarding_service.manager_for_node(&saved).is_none());
                assert!(!ws.ssh_terminal_command_histories.contains_key(&saved));
                let reopened = ws.materialize_ssh_root_node(
                    saved_config,
                    "Saved".into(),
                    Some("profile".into()),
                );
                assert_ne!(reopened, saved);
                assert_eq!(
                    ws.ssh_command_history(&reopened).unwrap().records()[0].command,
                    "pwd"
                );
            })
        });
    }

    #[gpui::test]
    async fn disabling_then_enabling_grants_a_new_period_to_manually_closed_sessions(
        cx: &mut TestAppContext,
    ) {
        let Some(_dir) = isolated_workspace_fixture(cx, "OXIDETERM_RETENTION_MANUAL_DIR") else {
            return;
        };
        cx.executor().allow_parking();
        let (shell, cx) = cx.add_window_view(|window, cx| {
            let ws = cx.new(|cx| WorkspaceApp::new(window, cx, None, None).unwrap());
            WorkspaceWindowShell::new(ws, window, cx)
        });
        let ws = shell.read_with(cx, |shell, _| shell.session_entity());
        let read_tree = || -> NodeTreePersistenceSnapshot {
            serde_json::from_slice(&std::fs::read(default_session_tree_path()).unwrap()).unwrap()
        };
        let (closed, failed) = cx.update(|window, cx| {
            ws.update(cx, |ws, cx| {
                ws.edit_settings(
                    |s| {
                        s.sidebar_ui.closed_session_retention_secs = 5;
                        s.connection_pool.idle_timeout_secs =
                            ConnectionPoolSettings::DISCONNECT_IMMEDIATELY_SECS;
                    },
                    cx,
                );
                let closed =
                    ws.materialize_ssh_root_node(config("closed.invalid"), "Closed".into(), None);
                let previous_generation = read_tree().nodes[0].generation;
                ws.disconnect_ssh_node(&closed, window, cx);
                let saved = read_tree();
                assert_eq!(saved.root_ids, vec![closed.clone()]);
                assert!(saved.nodes[0].generation > previous_generation);
                let failed = ws.materialize_ssh_root_node(
                    SshConfig {
                        proxy_command: Some(oxideterm_ssh::ProxyCommandConfig::Unavailable),
                        ..config("failed.invalid")
                    },
                    "Failed".into(),
                    None,
                );
                assert!(ws.ensure_node_connection_started_without_ancestors(&failed, cx));
                (closed, failed)
            })
        });
        cx.condition(&ws, |ws, _| {
            ws.ssh_nodes
                .get(&failed)
                .is_some_and(|node| node.readiness == NodeReadiness::Error)
        })
        .await;
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_secs(4));
        ws.update(cx, |ws, cx| {
            ws.edit_settings(|s| s.sidebar_ui.closed_session_retention_secs = -1, cx)
        });
        cx.executor().advance_clock(Duration::from_secs(6));
        ws.update(cx, |ws, cx| {
            assert!(ws.node_router.contains_node(&closed));
            ws.edit_settings(|s| s.sidebar_ui.closed_session_retention_secs = 5, cx);
        });
        cx.executor().advance_clock(Duration::from_secs(4));
        ws.read_with(cx, |ws, _| assert!(ws.node_router.contains_node(&closed)));
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        ws.read_with(cx, |ws, _| {
            assert_eq!(ws.node_router.root_node_ids(), vec![failed.clone()]);
            let node = ws.node_router.node_metadata(&failed).unwrap();
            assert_eq!(node.readiness, NodeReadiness::Error);
            assert!(node.error.as_deref().unwrap().contains("ProxyCommand"));
            assert_eq!(node.connection_id, None);
        });
        cx.update(|window, cx| {
            ws.update(cx, |ws, cx| {
                ws.edit_settings(|s| s.sidebar_ui.closed_session_retention_secs = -1, cx);
                let fresh = ws.materialize_ssh_root_node(config("new.invalid"), "New".into(), None);
                let closed =
                    ws.materialize_ssh_root_node(config("closed.invalid"), "Closed".into(), None);
                ws.disconnect_ssh_node(&closed, window, cx);
                ws.edit_settings(|s| s.sidebar_ui.closed_session_retention_secs = 0, cx);
                assert!(!ws.node_router.contains_node(&closed));
                assert!(ws.node_router.contains_node(&fresh));
                ws.disconnect_ssh_node(&fresh, window, cx);
                assert_eq!(read_tree().root_ids, vec![failed.clone()]);
            })
        });
    }

    #[gpui::test]
    fn immediate_removal_preserves_the_record_while_replacing_a_standalone_terminal(
        cx: &mut TestAppContext,
    ) {
        use standalone_connections::StandaloneConnectionSurface::Terminal;
        let Some(_dir) = isolated_workspace_fixture(cx, "OXIDETERM_RETENTION_RECONNECT_DIR") else {
            return;
        };
        cx.executor().allow_parking();
        let (shell, cx) = cx.add_window_view(|window, cx| {
            let ws = cx.new(|cx| WorkspaceApp::new(window, cx, None, None).unwrap());
            WorkspaceWindowShell::new(ws, window, cx)
        });
        let ws = shell.read_with(cx, |shell, _| shell.session_entity());
        cx.update(|window, cx| {
            ws.update(cx, |ws, cx| {
                ws.edit_settings(
                    |s| {
                        s.sidebar_ui.closed_session_retention_secs = 0;
                        s.terminal.crt_effect = false;
                    },
                    cx,
                );
                let old = ws
                    .create_telnet_terminal_tab(
                        TelnetSessionConfig {
                            host: "127.0.0.1".into(),
                            port: 0,
                            pre_connect_command: None,
                            line_endings: Default::default(),
                        },
                        Default::default(),
                        Default::default(),
                        window,
                        cx,
                    )
                    .unwrap();
                let id = ws
                    .standalone_connections
                    .connection_id_for_surface(Terminal(old))
                    .unwrap();
                ws.reconnect_standalone_connection(&id, window, cx);
                let Some(Terminal(new)) = ws.standalone_connections.record(&id).unwrap().surface
                else {
                    panic!("reconnect must mount a terminal");
                };
                assert_ne!(old, new);
                assert!(ws.tab_host.read(cx).terminal_location(old).is_none());
                assert!(ws.tab_host.read(cx).terminal_location(new).is_some());
                ws.close_terminal_session(new, window, cx);
                assert!(ws.standalone_connections.record(&id).is_none());
            })
        });
    }

    #[gpui::test]
    fn idle_retirement_starts_retention_and_late_delivery_preserves_a_rebound_node(
        cx: &mut TestAppContext,
    ) {
        let Some(_dir) = isolated_workspace_fixture(cx, "OXIDETERM_RETENTION_IDLE_DIR") else {
            return;
        };
        cx.executor().allow_parking();
        let (shell, cx) = cx.add_window_view(|window, cx| {
            let ws = cx.new(|cx| WorkspaceApp::new(window, cx, None, None).unwrap());
            WorkspaceWindowShell::new(ws, window, cx)
        });
        let ws = shell.read_with(cx, |shell, _| shell.session_entity());
        let ids = ws.update(cx, |ws, cx| {
            ws.edit_settings(|settings| {
                settings.sidebar_ui.closed_session_retention_secs = 5;
                settings.connection_pool.idle_timeout_secs =
                    ConnectionPoolSettings::DISCONNECT_IMMEDIATELY_SECS;
            }, cx);
            let (sender, events) = std::sync::mpsc::channel();
            ws.node_router.emitter().subscribe(sender);
            let mut ids = Vec::new();
            for host in ["expired.invalid", "rebound.invalid"] {
                let config = config(host);
                let id = ws.materialize_ssh_root_node(config.clone(), host.into(), None);
                let owner = ConnectionConsumer::NodeRouter(id.0.clone());
                let handle = ws.ssh_registry.acquire(config.clone(), owner.clone());
                handle.set_physical(Arc::new(()));
                ws.ssh_registry.mark_state(handle.connection_id(), ConnectionState::Active);
                ws.node_router.bind_connection(&id, handle.connection_id()).unwrap();
                // Two panes publish endpoint events on the route's own generation counter.
                for session in ["first", "second"] {
                    ws.ssh_registry.acquire_consumer_for_connection(
                        handle.connection_id(), ConnectionConsumer::Terminal(session.into()),
                    ).unwrap();
                    ws.node_router.bind_terminal_endpoint(&id, TerminalEndpoint {
                        session_id: session.into(),
                        ws_port: 0,
                        ws_token: Default::default(),
                    }).unwrap();
                }
                for session in ["first", "second"] {
                    ws.node_router.unbind_terminal_session(&id, session).unwrap();
                    ws.ssh_registry.release(
                        handle.connection_id(), &ConnectionConsumer::Terminal(session.into()),
                    );
                }
                loop {
                    let event = events.recv_timeout(Duration::from_secs(3)).expect("idle retirement");
                    if matches!(event, NodeStateEvent::ConnectionStateChanged { retired_connection_id: Some(retired), .. } if retired == handle.connection_id()) { break; }
                }
                if host == "rebound.invalid" {
                    let replacement = ws.ssh_registry.acquire(config, owner);
                    replacement.set_physical(Arc::new(()));
                    ws.ssh_registry.mark_state(replacement.connection_id(), ConnectionState::Active);
                    ws.node_router.bind_connection(&id, replacement.connection_id()).unwrap();
                }
                ids.push(id);
            }
            ids
        });
        cx.run_until_parked();
        ws.read_with(cx, |ws, _| {
            assert!(ws.node_router.contains_node(&ids[0]));
            assert_eq!(
                ws.node_router.node_metadata(&ids[0]).unwrap().connection_id,
                None
            );
            assert_eq!(
                ws.node_router.node_metadata(&ids[1]).unwrap().readiness,
                NodeReadiness::Ready
            );
        });
        cx.executor().advance_clock(Duration::from_secs(2));
        ws.update(cx, |ws, cx| {
            ws.edit_settings(|s| s.sidebar_ui.closed_session_retention_secs = 10, cx)
        });
        cx.executor().advance_clock(Duration::from_secs(1));
        ws.read_with(cx, |ws, _| assert!(ws.node_router.contains_node(&ids[0])));
        ws.update(cx, |ws, cx| {
            ws.active_ssh_node_id = Some(ids[0].clone());
            ws.edit_settings(|s| s.sidebar_ui.closed_session_retention_secs = 0, cx)
        });
        ws.read_with(cx, |ws, _| {
            assert_eq!(ws.active_ssh_node_id, None);
            assert_eq!(ws.node_router.root_node_ids(), vec![ids[1].clone()])
        });
    }
}
