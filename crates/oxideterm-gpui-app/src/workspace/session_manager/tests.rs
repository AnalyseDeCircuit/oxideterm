use super::*;
use gpui::TestAppContext;
use oxideterm_connections::{
    ConnectionStore, ConnectionStoreData, MoshProfile, SavedAuth, SavedConnection, SavedProxyHop,
    SerialProfile, TelnetProfile,
};

pub(in crate::workspace) fn isolated_workspace_fixture(
    cx: &mut TestAppContext,
    fixture_key: &str,
) -> Option<std::path::PathBuf> {
    let executable = std::env::current_exe().unwrap();
    if let Some(fixture_dir) = std::env::var_os(fixture_key) {
        let fixture_dir = std::path::PathBuf::from(fixture_dir);
        assert!(default_settings_path().starts_with(&fixture_dir));
        return Some(fixture_dir);
    }

    // Workspace storage is process-wide; keep settings and connection fixtures isolated.
    let directory = tempfile::tempdir_in(executable.parent().unwrap()).unwrap();
    let child = directory.path().join(executable.file_name().unwrap());
    std::fs::hard_link(&executable, &child).unwrap();
    std::fs::write(directory.path().join("portable"), []).unwrap();
    let output = std::process::Command::new(child)
        .arg(cx.test_function_name().unwrap())
        .arg("--nocapture")
        .env(fixture_key, directory.path())
        .env_remove("APPIMAGE")
        .current_dir(directory.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    None
}
fn workspace_with_console_profiles<'a>(
    cx: &'a mut TestAppContext,
    fixture_dir: &std::path::Path,
) -> (Entity<WorkspaceApp>, &'a mut TestAppContext) {
    let settings_path = default_settings_path();
    let mut settings = SettingsStore::load_from_path(settings_path).unwrap();
    settings.settings_mut().ssh_config.auto_load_hosts = false;
    settings.settings_mut().onboarding_completed = true;
    settings.save().unwrap();

    let mut telnet = TelnetProfile::new("Telnet", "telnet.test", 23);
    let mut mosh = MoshProfile::new("Mosh", "mosh.test", 22, "deploy", SavedAuth::Agent);
    let mut serial = SerialProfile::new("Serial", "COM3");
    for id in [&mut telnet.id, &mut mosh.id, &mut serial.id] {
        *id = "shared-profile-id".into();
    }
    let connections_path = fixture_dir.join("console-history-connections.json");
    std::fs::write(
        &connections_path,
        serde_json::to_vec(&ConnectionStoreData {
            telnet_profiles: vec![telnet],
            mosh_profiles: vec![mosh],
            serial_profiles: vec![serial],
            ..ConnectionStoreData::default()
        })
        .unwrap(),
    )
    .unwrap();
    let connections = ConnectionStore::load(connections_path).unwrap();

    let (shell, cx) = cx.add_window_view(|window, cx| {
        let workspace = cx.new(|cx| WorkspaceApp::new(window, cx, None, None).unwrap());
        WorkspaceWindowShell::new(workspace, window, cx)
    });
    let workspace = shell.read_with(cx, |shell, _| shell.session_entity());
    workspace.update(cx, |workspace, _cx| {
        workspace.connection_store = connections;
    });
    (workspace, cx)
}

fn history_scopes(workspace: &WorkspaceApp) -> HashSet<String> {
    workspace
        .terminal_history_store
        .histories()
        .into_iter()
        .map(|(scope, _, _)| scope)
        .collect()
}

pub(super) fn saved_connection_fixture(auth: SavedAuth) -> SavedConnection {
    let now = Utc::now();
    SavedConnection {
        id: "conn-1".to_string(),
        version: 1,
        name: "Home".to_string(),
        group: Some("Ungrouped".to_string()),
        notes: None,
        host: "192.168.1.2".to_string(),
        port: 22,
        username: "me".to_string(),
        auth,
        proxy_chain: Vec::new(),
        upstream_proxy: SavedUpstreamProxyPolicy::UseGlobal,
        proxy_command: None,
        options: oxideterm_connections::ConnectionOptions::default(),
        created_at: now,
        last_used_at: None,
        updated_at: Some(now),
        color: None,
        icon_background_color: None,
        icon: None,
        tags: Vec::new(),
        post_connect_command: None,
        privilege_credentials: Vec::new(),
    }
}

#[gpui::test]
fn single_console_profile_delete_retires_its_history_scope(cx: &mut TestAppContext) {
    let Some(fixture_dir) =
        isolated_workspace_fixture(cx, "OXIDETERM_SESSION_DELETE_SINGLE_TEST_DIR")
    else {
        return;
    };
    let (workspace, cx) = workspace_with_console_profiles(cx, &fixture_dir);
    let serial_scope = serde_json::json!(["serial", "shared-profile-id", "COM3"]).to_string();
    let telnet_scope =
        serde_json::json!(["telnet", "shared-profile-id", "telnet.test", 23]).to_string();

    workspace.update(cx, |workspace, cx| {
        let _serial_history = workspace
            .terminal_history_store
            .history(serial_scope.clone(), "Serial".into());
        let _telnet_history = workspace
            .terminal_history_store
            .history(telnet_scope.clone(), "Telnet".into());

        workspace.request_session_manager_row_delete(
            SessionManagerRowActionTarget::Serial("shared-profile-id".to_owned()),
            cx,
        );
        workspace.confirm_session_manager_delete(cx);

        assert_eq!(history_scopes(workspace), HashSet::from([telnet_scope]));
        assert!(
            workspace
                .connection_store
                .serial_profiles()
                .iter()
                .all(|profile| profile.id != "shared-profile-id")
        );
    });
}

#[gpui::test]
fn batch_console_profile_delete_retires_its_history_scope(cx: &mut TestAppContext) {
    let Some(fixture_dir) =
        isolated_workspace_fixture(cx, "OXIDETERM_SESSION_DELETE_BATCH_TEST_DIR")
    else {
        return;
    };
    let (workspace, cx) = workspace_with_console_profiles(cx, &fixture_dir);
    let telnet_scope =
        serde_json::json!(["telnet", "shared-profile-id", "telnet.test", 23]).to_string();
    let serial_scope = serde_json::json!(["serial", "shared-profile-id", "COM3"]).to_string();

    workspace.update(cx, |workspace, cx| {
        let _telnet_history = workspace
            .terminal_history_store
            .history(telnet_scope.clone(), "Telnet".into());
        let _serial_history = workspace
            .terminal_history_store
            .history(serial_scope.clone(), "Serial".into());

        workspace.delete_selected_session_items(
            vec![SessionManagerSelectionTarget::Telnet(
                "shared-profile-id".into(),
            )],
            cx,
        );

        assert_eq!(history_scopes(workspace), HashSet::from([serial_scope]));
        assert!(
            workspace
                .connection_store
                .telnet_profiles()
                .iter()
                .all(|profile| profile.id != "shared-profile-id")
        );
    });
}

#[gpui::test]
fn selected_tunnels_share_host_auth_and_leave_manager_active(cx: &mut TestAppContext) {
    let Some(dir) = isolated_workspace_fixture(cx, "OXIDETERM_TUNNEL_BATCH_TEST_DIR") else {
        return;
    };
    cx.executor().allow_parking();
    let (workspace, cx) = workspace_with_console_profiles(cx, &dir);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let mut host = saved_connection_fixture(SavedAuth::Password {
        empty_password: false,
        keychain_id: None,
        plaintext_password: None,
    });
    host.host = "127.0.0.1".into();
    host.port = listener.local_addr().unwrap().port();
    let mut other = host.clone();
    other.id = "other".into();
    other.username = "other".into();
    let path = dir.join("tunnel-hosts.json");
    std::fs::write(
        &path,
        serde_json::to_vec(&ConnectionStoreData {
            connections: vec![host, other],
            ..Default::default()
        })
        .unwrap(),
    )
    .unwrap();

    let window = cx.windows()[0];
    window
        .update(cx, |_, window, cx| {
            workspace.update(cx, |ws, cx| {
                ws.connection_store = ConnectionStore::load(path).unwrap();
                ws.open_session_manager_tab(window, cx);
                let manager = ws.active_tab_id(cx).unwrap();
                let store = ws.forwarding_service.registry().saved_store().unwrap();
                for (id, owner) in [
                    ("dev", "conn-1"),
                    ("other-dev", "other"),
                    ("uat", "conn-1"),
                    ("prod", "conn-1"),
                ] {
                    let mut rule = oxideterm_forwarding::ForwardRule::local(
                        "127.0.0.1",
                        3307,
                        "localhost",
                        3306,
                    );
                    rule.id = id.into();
                    let mut saved = oxideterm_forwarding::PersistedForward::new(
                        id.into(),
                        String::new(),
                        Some(owner.into()),
                        rule.forward_type,
                        rule,
                        false,
                    );
                    saved.tunnel = Some(oxideterm_forwarding::SshTunnelProfile {
                        name: id.into(),
                        group: None,
                        connect_on_startup: false,
                    });
                    store.persist_forward(saved).unwrap();
                }
                ws.start_ssh_tunnels(
                    &["dev".into(), "uat".into(), "other-dev".into()],
                    false,
                    window,
                    cx,
                );
                let first = ws.saved_ssh_nodes["conn-1"].clone();
                let other = ws.saved_ssh_nodes["other"].clone();
                assert_ne!(first, other);
                assert_eq!(
                    ws.ssh_tunnel_status("dev"),
                    oxideterm_forwarding::ForwardStatus::Starting
                );
                assert_eq!(
                    ws.ssh_tunnel_status("uat"),
                    oxideterm_forwarding::ForwardStatus::Starting
                );
                assert_eq!(
                    ws.ssh_tunnel_status("prod"),
                    oxideterm_forwarding::ForwardStatus::Stopped
                );
                assert_eq!(
                    ws.ssh_tunnel_status("other-dev"),
                    oxideterm_forwarding::ForwardStatus::Starting
                );
                // A failed transport clears exactly the rules sharing that host's authentication.
                ws.resume_ssh_tunnel_starts(&first, &NodeReadiness::Error, cx);
                assert_eq!(
                    ws.ssh_tunnel_status("dev"),
                    oxideterm_forwarding::ForwardStatus::Stopped
                );
                assert_eq!(
                    ws.ssh_tunnel_status("uat"),
                    oxideterm_forwarding::ForwardStatus::Stopped
                );
                assert_eq!(
                    ws.ssh_tunnel_status("other-dev"),
                    oxideterm_forwarding::ForwardStatus::Starting
                );
                assert_eq!(ws.active_tab_id(cx), Some(manager));
                assert!(ws.tabs(cx).iter().all(|tab| tab.kind != TabKind::Forwards));
            })
        })
        .unwrap();
}

#[gpui::test]
fn ide_access_prompt_defers_tab_creation_until_selection(cx: &mut TestAppContext) {
    let Some(fixture_dir) = isolated_workspace_fixture(cx, "OXIDETERM_IDE_ACCESS_PROMPT_TEST_DIR")
    else {
        return;
    };
    // Confirming access starts a Tokio backend task that can wake GPUI from another thread.
    cx.executor().allow_parking();
    let (workspace, cx) = workspace_with_console_profiles(cx, &fixture_dir);
    let window_handle = cx.windows()[0];
    let node_id = NodeId::new("ide-choice-host");
    workspace.update(cx, |workspace, _cx| {
        workspace.ssh_nodes.insert(
            node_id.clone(),
            super::super::WorkspaceSshNode::new(
                None,
                &SshConfig::default(),
                "Choice Host".into(),
                Vec::new(),
                oxideterm_ssh::NodeReadiness::Ready,
            ),
        );
    });
    let tab_ids = |cx: &TestAppContext| {
        workspace.read_with(cx, |workspace, cx| {
            workspace
                .tabs(cx)
                .iter()
                .map(|tab| tab.id)
                .collect::<Vec<_>>()
        })
    };
    let original_tabs = tab_ids(cx);
    let prompt_open = |cx: &mut TestAppContext| {
        window_handle
            .update(cx, |_, window, cx| {
                // Keyboard routing uses the rendered custom prompt's focus tree.
                window.draw(cx).clear(cx);
                window.has_active_prompt()
            })
            .unwrap()
    };
    for choose_ssh in [false, true] {
        window_handle
            .update(cx, |_, window, cx| {
                workspace.update(cx, |workspace, cx| {
                    workspace.open_ide_folder_picker_tab(node_id.clone(), window, cx);
                });
            })
            .unwrap();
        assert!(prompt_open(cx));
        assert_eq!(tab_ids(cx), original_tabs);
        if choose_ssh {
            cx.simulate_keystrokes(window_handle, "down");
            assert_eq!(tab_ids(cx), original_tabs);
            cx.simulate_keystrokes(window_handle, "enter");
            assert!(prompt_open(cx));
            assert_eq!(tab_ids(cx), original_tabs);
        }
        cx.simulate_keystrokes(window_handle, "escape");
        assert!(!prompt_open(cx));
        assert_eq!(tab_ids(cx), original_tabs);
    }
    window_handle
        .update(cx, |_, window, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace.open_ide_folder_picker_tab(node_id.clone(), window, cx);
            });
        })
        .unwrap();
    assert!(prompt_open(cx));
    cx.simulate_keystrokes(window_handle, "enter");
    assert!(!prompt_open(cx));
    workspace.read_with(cx, |workspace, cx| {
        let ide_tabs: Vec<_> = workspace
            .tabs(cx)
            .iter()
            .filter(|tab| tab.kind == TabKind::Ide)
            .collect();
        assert_eq!(ide_tabs.len(), 1);
        assert_eq!(ide_tabs[0].title, "IDE · Choice Host");
        assert_eq!(workspace.active_tab_id(cx), Some(ide_tabs[0].id));
    });
}

#[gpui::test]
fn sidebar_search_owns_keys_only_while_the_disclosure_is_visible(cx: &mut TestAppContext) {
    let Some(fixture_dir) =
        isolated_workspace_fixture(cx, "OXIDETERM_SIDEBAR_SEARCH_GATE_TEST_DIR")
    else {
        return;
    };
    let (workspace, cx) = workspace_with_console_profiles(cx, &fixture_dir);

    workspace.update(cx, |workspace, cx| {
        workspace.set_sidebar_section(SidebarSection::Connections, cx);
        workspace
            .connections
            .session_manager
            .update(cx, |manager, _cx| {
                manager.search_query = "web".into();
                manager.focused_input = Some(SessionManagerInput::Search);
            });

        // With the disclosure closed the field must not capture keys, so the
        // active terminal keeps its keyboard model.
        assert_eq!(workspace.active_session_manager_input(cx), None);

        workspace.session_manager_search_open = true;
        assert_eq!(
            workspace.active_session_manager_input(cx),
            Some(SessionManagerInput::Search)
        );

        // The header sort menu owns the panel keyboard while it is open.
        workspace.session_manager_sidebar_sort_open = true;
        assert_eq!(workspace.active_session_manager_input(cx), None);
        workspace.session_manager_sidebar_sort_open = false;

        // Escape collapses the disclosure and drops the draft, matching the
        // active-sessions search.
        let escape = gpui::KeyDownEvent {
            keystroke: gpui::Keystroke {
                modifiers: gpui::Modifiers::default(),
                key: "escape".into(),
                key_char: None,
            },
            is_held: false,
            prefer_character_input: false,
        };
        assert!(workspace.handle_session_manager_key(&escape, cx));
        assert!(!workspace.session_manager_search_open);
        assert!(
            workspace
                .connections
                .session_manager
                .read(cx)
                .search_query
                .is_empty()
        );
        assert_eq!(
            workspace
                .connections
                .session_manager
                .read(cx)
                .focused_input(),
            None
        );
    });

    workspace.update(cx, |workspace, cx| {
        let visible_count = workspace.session_manager_display_items(cx).len();
        assert!(visible_count > 1);
        workspace.session_manager_search_open = true;
        workspace
            .connections
            .session_manager
            .update(cx, |manager, _| {
                manager.search_query = "telnet".into();
                manager.focused_input = Some(SessionManagerInput::Search);
            });
        assert_eq!(workspace.session_manager_display_items(cx).len(), 1);
        workspace.set_sidebar_section(SidebarSection::Sessions, cx);
        workspace.set_sidebar_section(SidebarSection::Connections, cx);
        assert!(!workspace.session_manager_search_open);
        assert_eq!(
            workspace.session_manager_display_items(cx).len(),
            visible_count
        );
        assert_eq!(
            workspace
                .connections
                .session_manager
                .read(cx)
                .focused_input(),
            None
        );
    });
}

#[gpui::test]
fn group_dialog_name_field_owns_keys_without_the_session_manager_tab(cx: &mut TestAppContext) {
    let Some(fixture_dir) = isolated_workspace_fixture(cx, "OXIDETERM_GROUP_DIALOG_GATE_TEST_DIR")
    else {
        return;
    };
    let (workspace, cx) = workspace_with_console_profiles(cx, &fixture_dir);

    workspace.update(cx, |workspace, cx| {
        workspace.set_sidebar_section(SidebarSection::Connections, cx);
        workspace
            .connections
            .session_manager
            .update(cx, |manager, _cx| {
                manager.focused_input = Some(SessionManagerInput::GroupName);
            });

        // While no dialog is open the draft field must not capture keys, so
        // the active terminal keeps its keyboard model.
        assert_eq!(workspace.active_session_manager_input(cx), None);

        // The root mounts the group manager dialog for the sidebar form, so
        // its field owns keys while the dialog is open even though no
        // Session Manager tab exists underneath.
        workspace
            .connections
            .session_manager
            .update(cx, |manager, _cx| {
                manager.show_group_manager = true;
            });
        assert_eq!(
            workspace.active_session_manager_input(cx),
            Some(SessionManagerInput::GroupName)
        );
    });

    cx.windows()[0]
        .update(cx, |_, window, cx| {
            workspace.update(cx, |workspace, cx| {
                assert!(
                    workspace
                        .tabs(cx)
                        .iter()
                        .all(|tab| tab.kind != TabKind::SessionManager)
                );
                let escape = KeyDownEvent {
                    keystroke: gpui::Keystroke::parse("escape").unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                };
                // Keep the group manager open behind a valid batch confirmation.
                workspace
                    .connections
                    .session_manager
                    .update(cx, |manager, _| {
                        manager
                            .selected_items
                            .insert(SessionManagerSelectionTarget::Telnet(
                                "shared-profile-id".into(),
                            ));
                    });
                workspace.request_delete_selected_connections(cx);
                assert!(
                    workspace
                        .connections
                        .session_manager
                        .read(cx)
                        .delete_confirm
                        .is_some()
                );
                assert!(workspace.capture_active_window_modal_key(&escape, window, cx));
                assert!(
                    workspace
                        .connections
                        .session_manager
                        .read(cx)
                        .delete_confirm
                        .is_none()
                );
                assert!(
                    workspace
                        .connections
                        .session_manager
                        .read(cx)
                        .show_group_manager
                );
                assert!(workspace.capture_active_window_modal_key(&escape, window, cx));
                assert!(
                    !workspace
                        .connections
                        .session_manager
                        .read(cx)
                        .show_group_manager
                );
                assert_eq!(
                    workspace.connection_store.telnet_profiles()[0].name,
                    "Telnet"
                );
            })
        })
        .unwrap();
}

#[gpui::test]
fn session_manager_sidebar_loads_ssh_config_without_opening_a_tab(cx: &mut TestAppContext) {
    let Some(fixture_dir) = isolated_workspace_fixture(cx, "OXIDETERM_SIDEBAR_SSH_CONFIG_TEST_DIR")
    else {
        return;
    };
    let ssh_dir = oxideterm_portable_runtime::portable_ssh_dir()
        .unwrap()
        .unwrap();
    assert!(ssh_dir.starts_with(&fixture_dir));
    std::fs::create_dir_all(&ssh_dir).unwrap();
    std::fs::write(
        ssh_dir.join("config"),
        "Host sidebar-fixture\n    HostName server.invalid\n    User demo\n",
    )
    .unwrap();
    cx.executor().allow_parking();
    let (workspace, cx) = workspace_with_console_profiles(cx, &fixture_dir);
    let load = workspace.update(cx, |workspace, cx| {
        workspace
            .settings_store
            .settings_mut()
            .ssh_config
            .auto_load_hosts = true;
        workspace.set_sidebar_section(SidebarSection::Connections, cx);
        workspace
            .connections
            .session_manager
            .update(cx, |manager, _| {
                manager
                    .ssh_config_load_task
                    .take()
                    .expect("sidebar starts SSH config discovery")
            })
    });
    cx.foreground_executor().block_test(load);
    workspace.read_with(cx, |workspace, cx| {
        assert!(
            workspace
                .tabs(cx)
                .iter()
                .all(|tab| tab.kind != TabKind::SessionManager)
        );
        let aliases = workspace
            .session_manager_display_items(cx)
            .into_iter()
            .filter_map(|item| match item {
                item @ SessionManagerDisplayItem::SshConfig(_) => Some(item.name().to_owned()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(aliases, ["sidebar-fixture"]);
    });
}

#[gpui::test]
fn sftp_dialog_below_the_session_manager_is_visible_and_owns_escape(cx: &mut TestAppContext) {
    use crate::workspace::sftp::{SftpDialog, SftpPane, SftpRemoteId, SftpSurfaceId};
    use oxideterm_gpui_ui::motion::ExitPhase;

    let Some(fixture_dir) = isolated_workspace_fixture(cx, "OXIDETERM_SIDEBAR_SFTP_MODAL_TEST_DIR")
    else {
        return;
    };
    let (workspace, cx) = workspace_with_console_profiles(cx, &fixture_dir);
    cx.windows()[0]
        .update(cx, |_, window, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace.set_sidebar_section(SidebarSection::Connections, cx);
                let node = NodeId::new("sidebar-sftp-fixture");
                workspace.embedded_sftp_node_id = Some(node.clone());
                workspace.sftp_view.update(cx, |sftp, _| {
                    sftp.activate_view(SftpSurfaceId::Sidebar, SftpRemoteId::Node(node));
                    sftp.set_dialog(SftpDialog::Delete {
                        pane: SftpPane::Remote,
                        files: vec!["sample.txt".into()],
                        paths: Vec::new(),
                    });
                });
                assert!(matches!(
                    workspace.embedded_sftp_sidebar_dialog_view(cx).and_then(|view| view.dialog()),
                    Some(SftpDialog::Delete { files, .. }) if files == ["sample.txt"]
                ));
                let escape = KeyDownEvent {
                    keystroke: gpui::Keystroke::parse("escape").unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                };
                assert!(workspace.capture_active_window_modal_key(&escape, window, cx));
                assert!(
                    workspace
                        .embedded_sftp_sidebar_dialog_view(cx)
                        .is_none_or(|view| view.dialog_phase() == ExitPhase::Exiting)
                );
            })
        })
        .unwrap();
}
