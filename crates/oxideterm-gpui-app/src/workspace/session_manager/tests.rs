use super::*;

pub(super) fn base_form() -> NewConnectionForm {
    let mut form = NewConnectionForm::default();
    form.name = "Home".to_string();
    form.host = "192.168.1.2".to_string();
    form.port = "22".to_string();
    form.username = "me".to_string();
    form.group = "Ungrouped".to_string();
    form
}

pub(super) fn connection_info_fixture(icon: Option<&str>) -> ConnectionInfo {
    ConnectionInfo {
        id: "conn-1".to_string(),
        name: "Home".to_string(),
        group: Some("Ungrouped".to_string()),
        notes: None,
        host: "192.168.1.2".to_string(),
        port: 22,
        username: "me".to_string(),
        auth_type: AuthType::Agent,
        key_path: None,
        cert_path: None,
        managed_key_id: None,
        managed_key_name: None,
        proxy_chain: Vec::new(),
        upstream_proxy: SavedUpstreamProxyPolicy::UseGlobal,
        created_at: "2026-06-15T00:00:00Z".to_string(),
        last_used_at: None,
        color: None,
        icon_background_color: None,
        icon: icon.map(ToOwned::to_owned),
        tags: Vec::new(),
        agent_forwarding: false,
        identity_agent: None,
        agent_forwarding_socket: None,
        legacy_ssh_compatibility: false,
        post_connect_command: None,
    }
}

fn session_manager_display_fixture(
    id: &str,
    group: Option<&str>,
    last_used_at: Option<&str>,
) -> SessionManagerDisplayItem {
    SessionManagerDisplayItem::Connection(ConnectionInfo {
        id: id.to_string(),
        name: id.to_string(),
        group: group.map(ToOwned::to_owned),
        last_used_at: last_used_at.map(ToOwned::to_owned),
        ..connection_info_fixture(None)
    })
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

#[test]
pub(super) fn session_manager_grid_projection_virtualizes_cards_by_responsive_row() {
    let items = (0..7)
        .map(|index| {
            session_manager_display_fixture(
                &format!("connection-{index}"),
                None,
                (index < 3).then_some("2026-06-15T00:00:00Z"),
            )
        })
        .collect::<Vec<_>>();

    let rows =
        session_manager_grid_rows(&items, &[], "Recent".to_string(), "Hosts".to_string(), 3, 2);

    assert_eq!(
        rows,
        vec![
            SessionManagerGridRow::SectionHeader {
                title: "Recent".to_string(),
                item_count: 3,
            },
            SessionManagerGridRow::RecentItems {
                item_indices: vec![0, 1],
                is_last_in_section: false,
            },
            SessionManagerGridRow::RecentItems {
                item_indices: vec![2],
                is_last_in_section: true,
            },
            SessionManagerGridRow::SectionHeader {
                title: "Hosts".to_string(),
                item_count: 7,
            },
            SessionManagerGridRow::Cards {
                item_indices: vec![0, 1, 2],
            },
            SessionManagerGridRow::Cards {
                item_indices: vec![3, 4, 5],
            },
            SessionManagerGridRow::Cards {
                item_indices: vec![6],
            },
        ]
    );
}

#[test]
pub(super) fn session_manager_tree_projection_only_contains_visible_rows() {
    let items = vec![
        session_manager_display_fixture("parent-item", Some("parent"), None),
        session_manager_display_fixture("child-item", Some("parent/child"), None),
        session_manager_display_fixture("ungrouped-item", None, None),
    ];
    let roots = vec!["parent".to_string()];
    let children = HashMap::from([("parent".to_string(), vec!["parent/child".to_string()])]);

    assert_eq!(
        session_manager_tree_rows(&items, &roots, &children, &HashSet::new()),
        vec![
            SessionManagerTreeRow::Group {
                path: "parent".to_string(),
                depth: 0,
                expanded: false,
                has_children: true,
            },
            SessionManagerTreeRow::Item {
                item_index: 2,
                depth: 0,
            },
        ]
    );

    let expanded = HashSet::from(["parent".to_string(), "parent/child".to_string()]);
    assert_eq!(
        session_manager_tree_rows(&items, &roots, &children, &expanded),
        vec![
            SessionManagerTreeRow::Group {
                path: "parent".to_string(),
                depth: 0,
                expanded: true,
                has_children: true,
            },
            SessionManagerTreeRow::Group {
                path: "parent/child".to_string(),
                depth: 1,
                expanded: true,
                has_children: true,
            },
            SessionManagerTreeRow::Item {
                item_index: 1,
                depth: 2,
            },
            SessionManagerTreeRow::Item {
                item_index: 0,
                depth: 1,
            },
            SessionManagerTreeRow::Item {
                item_index: 2,
                depth: 0,
            },
        ]
    );
}

#[test]
pub(super) fn session_group_ui_state_rewrites_only_the_selected_subtree() {
    assert!(session_group_path_is_within(
        "Production/Core/Database",
        "Production"
    ));
    assert!(!session_group_path_is_within(
        "Production-Backup",
        "Production"
    ));
    assert_eq!(
        renamed_session_group_path("Production/Core", "Production", "Live"),
        Some("Live/Core".to_string())
    );
    assert_eq!(
        renamed_session_group_path("Unrelated", "Production", "Live"),
        None
    );
}

#[test]
pub(super) fn contextual_group_editors_compose_only_one_path_segment() {
    assert_eq!(
        split_session_group_path("Production/Core/Database"),
        (Some("Production/Core"), "Database")
    );
    assert_eq!(
        session_group_path_from_leaf(Some("Production/Core"), " Database "),
        Some("Production/Core/Database".to_string())
    );
    assert_eq!(
        session_group_path_from_leaf(Some("Production/Core"), "Cache"),
        Some("Production/Core/Cache".to_string())
    );
    assert_eq!(
        session_group_path_from_leaf(None, "Production"),
        Some("Production".to_string())
    );
    assert_eq!(
        session_group_path_from_leaf(Some("Production"), "Core/Database"),
        None
    );
    assert_eq!(session_group_path_from_leaf(None, "   "), None);
}

#[test]
pub(super) fn session_menu_dismissal_closes_all_manager_popovers() {
    let mut state = SessionManagerState {
        show_batch_move: true,
        row_action_menu: Some(SessionManagerRowActionMenu {
            target: SessionManagerRowActionTarget::Connection("connection-1".to_string()),
            origin: SessionManagerRowActionMenuOrigin::Pointer,
            x: 120.0,
            y: 80.0,
        }),
        ..SessionManagerState::default()
    };

    assert!(close_session_menu_state(&mut state));
    assert!(!state.show_batch_move);
    assert!(state.row_action_menu.is_none());
}

#[test]
pub(super) fn connection_display_item_uses_custom_icon_when_present() {
    let item = SessionManagerDisplayItem::Connection(connection_info_fixture(Some("cloud")));

    assert!(matches!(item.icon(), LucideIcon::Cloud));
}

#[test]
pub(super) fn connection_display_item_falls_back_to_server_icon() {
    let item = SessionManagerDisplayItem::Connection(connection_info_fixture(Some("missing")));

    assert!(matches!(item.icon(), LucideIcon::Server));
}

#[test]
pub(super) fn ssh_config_display_projection_never_copies_proxy_command_secrets() {
    let host = SshConfigHost {
        alias: "safe-alias".to_string(),
        hostname: Some("example.com".to_string()),
        proxy_command: Some(vec![SecretString::new("secret-proxy-token")]),
        ..SshConfigHost::default()
    };
    let item =
        SessionManagerDisplayItem::SshConfig(SessionManagerSshConfigDisplayItem::from(&host));

    let search_text = item.search_text();
    assert!(search_text.contains("safe-alias"));
    assert!(!search_text.contains("secret-proxy-token"));
}

#[test]
pub(super) fn saved_profile_selection_is_typed_separately_from_ssh_ids() {
    let now = Utc::now();
    let ssh = SessionManagerDisplayItem::Connection(ConnectionInfo {
        id: "shared-id".to_string(),
        ..connection_info_fixture(None)
    });
    let mut serial = SerialProfile::new("Serial console", "/dev/tty.test");
    serial.id = "shared-id".to_string();
    let serial = SessionManagerDisplayItem::Serial(serial);
    let mut telnet = TelnetProfile::new("Telnet console", "telnet.example.test", 23);
    telnet.id = "shared-id".to_string();
    let telnet = SessionManagerDisplayItem::Telnet(telnet);
    let mut mosh = MoshProfile::new(
        "Mosh console",
        "mosh.example.test",
        22,
        "operator",
        SavedAuth::Agent,
    );
    mosh.id = "shared-id".to_string();
    let mosh = SessionManagerDisplayItem::Mosh(mosh);
    let remote = SessionManagerDisplayItem::RemoteDesktop(RemoteDesktopProfile {
        id: "shared-id".to_string(),
        name: "Remote desktop".to_string(),
        group: None,
        notes: None,
        icon: None,
        color: None,
        icon_background_color: None,
        protocol: oxideterm_remote_desktop::RemoteDesktopProtocol::Rdp,
        host: "rdp.example.com".to_string(),
        port: 3389,
        username: Some("operator".to_string()),
        domain: None,
        credential_ref: None,
        ssh_gateway_connection_id: None,
        read_only: false,
        session_options: oxideterm_remote_desktop::RemoteDesktopSessionOptions::default(),
        created_at: now,
        updated_at: now,
        last_used_at: None,
    });

    assert_eq!(
        ssh.selection_target(),
        Some(SessionManagerSelectionTarget::Connection(
            "shared-id".to_string()
        ))
    );
    assert_eq!(
        serial.selection_target(),
        Some(SessionManagerSelectionTarget::Serial(
            "shared-id".to_string()
        ))
    );
    assert_eq!(
        telnet.selection_target(),
        Some(SessionManagerSelectionTarget::Telnet(
            "shared-id".to_string()
        ))
    );
    assert_eq!(
        mosh.selection_target(),
        Some(SessionManagerSelectionTarget::Mosh("shared-id".to_string()))
    );
    assert_eq!(
        remote.selection_target(),
        Some(SessionManagerSelectionTarget::RemoteDesktop(
            "shared-id".to_string()
        ))
    );
    assert_ne!(ssh.selection_target(), remote.selection_target());
}

#[test]
pub(super) fn save_request_from_form_preserves_custom_icon_and_independent_colors() {
    let mut form = base_form();
    form.icon = "cloud".to_string();
    form.color = "#7dd3fc".to_string();
    form.icon_background_color = "#082f49".to_string();
    let request = save_request_from_form(&mut form, Some("conn-1".to_string())).unwrap();

    assert_eq!(request.icon.as_deref(), Some("cloud"));
    assert_eq!(request.color.as_deref(), Some("#7dd3fc"));
    assert_eq!(request.icon_background_color.as_deref(), Some("#082f49"));
}

#[test]
pub(super) fn save_request_moves_manual_proxy_command_into_a_redacted_secret_owner() {
    let mut form = base_form();
    form.proxy_command_enabled = true;
    form.proxy_command = "helper --token proxy-command-secret".to_string();

    let request = save_request_from_form(&mut form, None).unwrap();
    let saved_command = request.proxy_command.unwrap();

    assert!(form.proxy_command.is_empty());
    assert_eq!(
        saved_command
            .plaintext_command
            .as_ref()
            .unwrap()
            .expose_secret(),
        "helper --token proxy-command-secret"
    );
    assert!(!format!("{saved_command:?}").contains("proxy-command-secret"));
}

#[test]
pub(super) fn oxide_export_logical_scroll_change_detects_inner_consumption() {
    // GPUI ListState owns measured row heights internally, so scroll-chain
    // decisions must compare actual logical movement instead of estimates.
    assert!(!oxide_export_logical_scroll_changed(0, 0.0, 0, 0.0));
    assert!(!oxide_export_logical_scroll_changed(0, 12.0, 0, 12.004));
    assert!(oxide_export_logical_scroll_changed(0, 0.0, 0, 24.0));
    assert!(oxide_export_logical_scroll_changed(0, 24.0, 1, 0.0));
}

#[test]
pub(super) fn oxide_export_native_i18n_keys_resolve_without_tauri_namespace() {
    // Native modals.json flattens the export dialog as `export.*`; using
    // Tauri's `modals.export.*` namespace renders raw keys in the dialog.
    let i18n = oxideterm_i18n::I18n::new(oxideterm_i18n::Locale::ZhCn);
    for key in [
        "export.select_connections",
        "export.select_all",
        "export.new_since_last_export",
        "export.badge_new",
        "export.credential_material",
        "export.content_summary_title",
        "export.app_settings_section_terminal_appearance",
    ] {
        assert_ne!(i18n.t(key), key, "unresolved export i18n key: {key}");
    }
    let tauri_namespace_key = ["modals", "export", "select_connections"].join(".");
    assert_eq!(i18n.t(&tauri_namespace_key), tauri_namespace_key);
}

#[test]
pub(super) fn oxide_dialog_inputs_are_active_outside_the_session_manager_tab() {
    let export_dialog = OxideExportDialogState::default();
    assert!(session_manager_input_is_active(
        SessionManagerInput::OxideExportPassword,
        false,
        None,
        Some(&export_dialog),
    ));

    let mut import_dialog = OxideImportDialogState::default();
    import_dialog.file_data = Some(vec![1].into());
    assert!(session_manager_input_is_active(
        SessionManagerInput::OxideImportPassword,
        false,
        Some(&import_dialog),
        None,
    ));

    assert!(!session_manager_input_is_active(
        SessionManagerInput::Search,
        false,
        None,
        None,
    ));
    assert!(session_manager_input_is_active(
        SessionManagerInput::Search,
        true,
        None,
        None,
    ));
}

#[test]
pub(super) fn busy_oxide_export_does_not_keep_a_stale_text_input_active() {
    let export_dialog = OxideExportDialogState {
        busy: true,
        ..OxideExportDialogState::default()
    };

    assert!(!session_manager_input_is_active(
        SessionManagerInput::OxideExportPassword,
        false,
        None,
        Some(&export_dialog),
    ));
}

#[test]
pub(super) fn new_connection_save_password_false_does_not_request_keychain_storage() {
    let mut form = base_form();
    form.password = "secret".to_string();
    form.save_password = false;

    let request = save_request_from_form(&mut form, None).unwrap();

    match request.auth {
        SavedAuth::Password {
            keychain_id: None,
            plaintext_password: None,
        } => {}
        other => panic!("unexpected auth: {other:?}"),
    }
}

#[test]
pub(super) fn new_connection_save_password_true_keeps_empty_password_as_submitted_secret() {
    let mut form = base_form();
    form.password = String::new();
    form.save_password = true;

    let request = save_request_from_form(&mut form, None).unwrap();

    match request.auth {
        SavedAuth::Password {
            keychain_id: None,
            plaintext_password: Some(password),
        } => assert_eq!(password, ""),
        other => panic!("unexpected auth: {other:?}"),
    }
}

#[test]
pub(super) fn edit_properties_unloaded_password_preserves_saved_keychain_id() {
    let existing = SavedAuth::Password {
        keychain_id: Some("kc-password".to_string()),
        plaintext_password: None,
    };
    let mut form = base_form();
    form.password = String::new();
    form.password_loaded = false;
    form.save_password = true;

    let request = save_request_from_form_with_existing_auth(
        &mut form,
        Some("conn-1".to_string()),
        Some(&existing),
    )
    .unwrap();

    match request.auth {
        SavedAuth::Password {
            keychain_id: Some(keychain_id),
            plaintext_password: None,
        } => assert_eq!(keychain_id, "kc-password"),
        other => panic!("unexpected auth: {other:?}"),
    }
}

#[test]
pub(super) fn edit_properties_switch_from_agent_to_password_submits_new_password() {
    let existing = SavedAuth::Agent;
    let connect_timeout_seconds = 120;
    let mut saved_connection = saved_connection_fixture(existing.clone());
    saved_connection.options.connect_timeout_seconds = Some(connect_timeout_seconds);
    let mut form = form_from_saved_connection(&saved_connection, None);
    form.auth_tab = SshAuthTab::Password;
    form.password = "new-secret".to_string();

    let request = save_request_from_form_with_existing_auth(
        &mut form,
        Some(saved_connection.id),
        Some(&existing),
    )
    .unwrap();
    assert_eq!(request.connect_timeout_seconds, connect_timeout_seconds);

    match request.auth {
        SavedAuth::Password {
            keychain_id: None,
            plaintext_password: Some(password),
        } => assert_eq!(password, "new-secret"),
        other => panic!("unexpected auth: {other:?}"),
    }
}

#[test]
pub(super) fn edit_properties_saved_keychain_password_starts_unloaded() {
    let saved_connection = saved_connection_fixture(SavedAuth::Password {
        keychain_id: Some("kc-password".to_string()),
        plaintext_password: None,
    });

    let form = form_from_saved_connection(&saved_connection, None);

    assert!(!form.password_loaded);
    assert_eq!(
        form.saved_password_keychain_id.as_deref(),
        Some("kc-password")
    );
}

#[test]
pub(super) fn edit_properties_restores_proxy_chain_without_loading_secrets() {
    let mut saved_connection = saved_connection_fixture(SavedAuth::Agent);
    saved_connection.proxy_chain = vec![SavedProxyHop {
        host: "jump.example.com".to_string(),
        port: 2222,
        username: "ops".to_string(),
        auth: SavedAuth::Password {
            keychain_id: Some("proxy-password-keychain-id".to_string()),
            plaintext_password: None,
        },
        agent_forwarding: true,
        identity_agent: Some("/tmp/proxy-agent.sock".to_string()),
        agent_forwarding_socket: Some("/tmp/proxy-forward.sock".to_string()),
        legacy_ssh_compatibility: true,
    }];
    let mut form = form_from_saved_connection(&saved_connection, None);

    restore_saved_proxy_chain_in_form(&mut form, &saved_connection);

    assert!(form.proxy_chain_expanded);
    assert_eq!(form.proxy_hops.len(), 1);
    let hop = &form.proxy_hops[0];
    assert_eq!(hop.persisted_proxy_hop_index, Some(0));
    assert_eq!(hop.host, "jump.example.com");
    assert_eq!(hop.port, "2222");
    assert_eq!(hop.username, "ops");
    assert_eq!(hop.auth_tab, SshAuthTab::Password);
    assert!(hop.password.is_empty());
    assert!(hop.passphrase.is_empty());
    assert!(hop.agent_forwarding);
    assert_eq!(hop.identity_agent, "/tmp/proxy-agent.sock");
    assert_eq!(
        hop.agent_forwarding_socket.as_deref(),
        Some("/tmp/proxy-forward.sock")
    );
    assert!(hop.legacy_ssh_compatibility);
}

#[test]
pub(super) fn edit_properties_can_remove_the_entire_proxy_chain() {
    let mut saved_connection = saved_connection_fixture(SavedAuth::Agent);
    saved_connection.proxy_chain = vec![SavedProxyHop {
        host: "jump.example.com".to_string(),
        port: 22,
        username: "ops".to_string(),
        auth: SavedAuth::Agent,
        agent_forwarding: false,
        identity_agent: None,
        agent_forwarding_socket: None,
        legacy_ssh_compatibility: false,
    }];
    let mut form = form_from_saved_connection(&saved_connection, None);
    restore_saved_proxy_chain_in_form(&mut form, &saved_connection);
    form.proxy_hops.clear();

    let request = save_request_from_form_with_existing_auth(
        &mut form,
        Some(saved_connection.id.clone()),
        Some(&saved_connection.auth),
    )
    .unwrap();

    assert!(request.proxy_chain.is_empty());
}

#[test]
pub(super) fn edit_properties_preserves_legacy_ssh_compatibility() {
    let mut saved_connection = saved_connection_fixture(SavedAuth::Agent);
    saved_connection.options.legacy_ssh_compatibility = true;
    saved_connection.options.dedicated_new_terminal_connection = true;

    // Editing and saving an existing connection must round-trip its transport policy.
    let mut form = form_from_saved_connection(&saved_connection, None);
    let request = save_request_from_form(&mut form, Some(saved_connection.id.clone())).unwrap();

    assert!(form.legacy_ssh_compatibility);
    assert!(request.legacy_ssh_compatibility);
    assert!(form.dedicated_new_terminal_connection);
    assert!(request.dedicated_new_terminal_connection);
}

#[test]
pub(super) fn edit_properties_round_trips_host_terminal_overrides() {
    let mut saved_connection = saved_connection_fixture(SavedAuth::Agent);
    saved_connection.options.terminal = ConnectionTerminalOptions {
        encoding: Some(oxideterm_connections::ConnectionTerminalEncoding::Gb18030),
        backspace_sequence: Some(
            oxideterm_connections::ConnectionTerminalBackspaceSequence::ControlH,
        ),
        delete_sequence: Some(oxideterm_connections::ConnectionTerminalDeleteSequence::Delete),
        semantic_scheme: Some("conservative".to_string()),
        highlight_rule_set: Some("network-devices".to_string()),
    };

    let mut form = form_from_saved_connection(&saved_connection, None);
    let request = save_request_from_form(&mut form, Some(saved_connection.id.clone())).unwrap();

    assert_eq!(form.terminal, saved_connection.options.terminal);
    assert_eq!(request.terminal, saved_connection.options.terminal);
}

#[test]
pub(super) fn edit_properties_initializes_saved_agent_availability() {
    let mut saved_connection = saved_connection_fixture(SavedAuth::Agent);
    saved_connection.options.identity_agent = Some("none".to_string());

    // IdentityAgent none is deterministic and proves the edit form replaces
    // Unknown with a real availability result.
    let form = form_from_saved_connection(&saved_connection, None);

    assert_eq!(form.agent_available, Some(false));
}

#[test]
pub(super) fn edit_properties_round_trips_custom_identity_agent() {
    let mut saved_connection = saved_connection_fixture(SavedAuth::Agent);
    saved_connection.options.identity_agent = Some("$YUBIKEY_AGENT".to_string());

    let mut form = form_from_saved_connection(&saved_connection, None);
    let request = save_request_from_form(&mut form, Some(saved_connection.id.clone())).unwrap();

    assert_eq!(form.identity_agent, "$YUBIKEY_AGENT");
    assert_eq!(request.identity_agent.as_deref(), Some("$YUBIKEY_AGENT"));
}

#[test]
pub(super) fn duplicate_template_name_uses_unique_tauri_copy_suffix() {
    let name = duplicate_connection_template_name(
        "Prod Copy",
        ["Prod", "Prod Copy", "Prod Copy 2"].into_iter(),
    );

    assert_eq!(name, "Prod Copy 3");
}

#[test]
pub(super) fn duplicate_template_name_falls_back_for_empty_source() {
    let name = duplicate_connection_template_name("", ["Connection Copy"].into_iter());

    assert_eq!(name, "Connection Copy 2");
}

#[test]
pub(super) fn edit_properties_same_key_empty_passphrase_submits_no_new_secret() {
    let existing = SavedAuth::Key {
        key_path: "/tmp/id_ed25519".to_string(),
        has_passphrase: true,
        passphrase_keychain_id: Some("kc-passphrase".to_string()),
        plaintext_passphrase: None,
    };
    let mut form = base_form();
    form.auth_tab = SshAuthTab::SshKey;
    form.key_path = "/tmp/id_ed25519".to_string();
    form.passphrase = String::new();

    let request = save_request_from_form_with_existing_auth(
        &mut form,
        Some("conn-1".to_string()),
        Some(&existing),
    )
    .unwrap();

    match request.auth {
        SavedAuth::Key {
            key_path,
            has_passphrase,
            passphrase_keychain_id: None,
            plaintext_passphrase: None,
        } => {
            assert_eq!(key_path, "/tmp/id_ed25519");
            assert!(!has_passphrase);
        }
        other => panic!("unexpected auth: {other:?}"),
    }
}

#[test]
pub(super) fn new_connection_request_carries_proxy_chain() {
    let mut form = base_form();
    form.auth_tab = SshAuthTab::Agent;
    form.identity_agent = "  /tmp/target-agent.sock  ".to_string();
    form.agent_forwarding_socket = Some("/tmp/target-forward.sock".to_string());
    form.proxy_hops
        .push(crate::workspace::new_connection::NewConnectionProxyHop {
            saved_connection_id: String::new(),
            persisted_proxy_hop_index: None,
            host: "jump.example.com".to_string(),
            port: "2222".to_string(),
            username: "ops".to_string(),
            auth_tab: SshAuthTab::Password,
            password: "jump-secret".to_string(),
            key_path: String::new(),
            managed_key_id: String::new(),
            cert_path: String::new(),
            passphrase: String::new(),
            agent_forwarding: true,
            identity_agent: "  /tmp/jump-agent.sock  ".to_string(),
            agent_forwarding_socket: Some("/tmp/jump-forward.sock".to_string()),
            legacy_ssh_compatibility: true,
        });

    let request = save_request_from_form(&mut form, None).unwrap();

    assert_eq!(
        request.identity_agent.as_deref(),
        Some("/tmp/target-agent.sock")
    );
    assert_eq!(
        request.agent_forwarding_socket.as_deref(),
        Some("/tmp/target-forward.sock")
    );
    assert_eq!(request.proxy_chain.len(), 1);
    let hop = &request.proxy_chain[0];
    assert_eq!(hop.host, "jump.example.com");
    assert_eq!(hop.port, 2222);
    assert_eq!(hop.username, "ops");
    assert!(hop.agent_forwarding);
    assert_eq!(hop.identity_agent.as_deref(), Some("/tmp/jump-agent.sock"));
    assert_eq!(
        hop.agent_forwarding_socket.as_deref(),
        Some("/tmp/jump-forward.sock")
    );
    assert!(hop.legacy_ssh_compatibility);
    match &hop.auth {
        SavedAuth::Password {
            keychain_id: None,
            plaintext_password: Some(password),
        } => assert_eq!(password, "jump-secret"),
        other => panic!("unexpected proxy auth: {other:?}"),
    }
}

#[test]
pub(super) fn save_request_moves_all_visible_password_allocations_and_redacts_debug() {
    let mut form = base_form();
    form.password = "target-secret-marker".to_string();
    form.save_password = true;
    let target_pointer = form.password.as_ptr();

    let mut hop = crate::workspace::new_connection::NewConnectionProxyHop::new();
    hop.host = "jump.example.com".to_string();
    hop.username = "ops".to_string();
    hop.auth_tab = SshAuthTab::Password;
    hop.password = "jump-secret-marker".to_string();
    let hop_pointer = hop.password.as_ptr();
    form.proxy_hops.push(hop);

    form.upstream_proxy_policy = NewConnectionUpstreamProxyPolicy::Custom;
    form.upstream_proxy_host = "proxy.example.com".to_string();
    form.upstream_proxy_port = "1080".to_string();
    form.upstream_proxy_auth = NewConnectionUpstreamProxyAuth::Password;
    form.upstream_proxy_username = "proxy-user".to_string();
    form.upstream_proxy_password = "upstream-secret-marker".to_string();
    let upstream_pointer = form.upstream_proxy_password.as_ptr();

    let request = save_request_from_form(&mut form, None).unwrap();

    assert!(form.password.is_empty());
    assert!(form.proxy_hops[0].password.is_empty());
    assert!(form.upstream_proxy_password.is_empty());
    match &request.auth {
        SavedAuth::Password {
            plaintext_password: Some(password),
            ..
        } => assert_eq!(password.expose_secret().as_ptr(), target_pointer),
        other => panic!("unexpected target auth: {other:?}"),
    }
    match &request.proxy_chain[0].auth {
        SavedAuth::Password {
            plaintext_password: Some(password),
            ..
        } => assert_eq!(password.expose_secret().as_ptr(), hop_pointer),
        other => panic!("unexpected proxy auth: {other:?}"),
    }
    match &request.upstream_proxy {
        SavedUpstreamProxyPolicy::Custom { proxy } => match &proxy.auth {
            oxideterm_connections::SavedUpstreamProxyAuth::Password {
                plaintext_password: Some(password),
                ..
            } => assert_eq!(password.expose_secret().as_ptr(), upstream_pointer),
            other => panic!("unexpected upstream auth: {other:?}"),
        },
        other => panic!("unexpected upstream policy: {other:?}"),
    }

    let debug = format!("{request:?}");
    for secret in [
        "target-secret-marker",
        "jump-secret-marker",
        "upstream-secret-marker",
    ] {
        assert!(!debug.contains(secret));
    }
}

#[test]
pub(super) fn upstream_proxy_test_handoff_preserves_visible_password() {
    let store = ConnectionStore::load_read_only(std::path::PathBuf::new()).unwrap();
    let mut form = base_form();
    form.upstream_proxy_policy = NewConnectionUpstreamProxyPolicy::Custom;
    form.upstream_proxy_host = "proxy.example.com".to_string();
    form.upstream_proxy_port = "1080".to_string();
    form.upstream_proxy_auth = NewConnectionUpstreamProxyAuth::Password;
    form.upstream_proxy_username = "proxy-user".to_string();
    form.upstream_proxy_password = "upstream-secret-marker".to_string();

    let config = runtime_upstream_proxy_config_from_form(
        &store,
        &mut form,
        RuntimeSecretHandoff::CopyForTest,
    )
    .unwrap();

    assert_eq!(form.upstream_proxy_password, "upstream-secret-marker");
    assert!(matches!(
        config.auth,
        UpstreamProxyAuth::Password { ref password, .. }
            if password.as_str() == "upstream-secret-marker"
    ));
}

#[test]
pub(super) fn save_request_moves_key_passphrase_allocation() {
    let mut form = base_form();
    form.auth_tab = SshAuthTab::SshKey;
    form.key_path = "/tmp/id_ed25519".to_string();
    form.passphrase = "passphrase-secret-marker".to_string();
    let passphrase_pointer = form.passphrase.as_ptr();

    let request = save_request_from_form(&mut form, None).unwrap();

    assert!(form.passphrase.is_empty());
    match request.auth {
        SavedAuth::Key {
            plaintext_passphrase: Some(passphrase),
            ..
        } => assert_eq!(passphrase.expose_secret().as_ptr(), passphrase_pointer),
        other => panic!("unexpected auth: {other:?}"),
    }
}

#[test]
pub(super) fn save_validation_failure_keeps_secret_allocations_in_the_form() {
    let mut form = base_form();
    form.host.clear();
    form.password = "validation-secret-marker".to_string();
    form.save_password = true;
    let password_pointer = form.password.as_ptr();

    let error = save_request_from_form(&mut form, None).unwrap_err();

    assert!(error.to_string().contains("Host is required"));
    assert_eq!(form.password, "validation-secret-marker");
    assert_eq!(form.password.as_ptr(), password_pointer);
}

#[test]
pub(super) fn proxy_hop_two_factor_is_saved_as_keyboard_interactive() {
    let mut form = base_form();
    form.auth_tab = SshAuthTab::Agent;
    form.proxy_hops
        .push(crate::workspace::new_connection::NewConnectionProxyHop {
            saved_connection_id: String::new(),
            persisted_proxy_hop_index: None,
            host: "jump.example.com".to_string(),
            port: "22".to_string(),
            username: "ops".to_string(),
            auth_tab: SshAuthTab::TwoFactor,
            password: String::new(),
            key_path: String::new(),
            managed_key_id: String::new(),
            cert_path: String::new(),
            passphrase: String::new(),
            agent_forwarding: false,
            identity_agent: String::new(),
            agent_forwarding_socket: None,
            legacy_ssh_compatibility: false,
        });

    let request = save_request_from_form(&mut form, None).unwrap();

    assert!(matches!(
        request.proxy_chain[0].auth,
        oxideterm_connections::SavedAuth::KeyboardInteractive
    ));
}

#[test]
pub(super) fn runtime_proxy_hops_are_prepended_without_cloning_the_connection_form() {
    let mut form = base_form();
    form.auth_tab = SshAuthTab::Agent;
    let mut form_hop = crate::workspace::new_connection::NewConnectionProxyHop::new();
    form_hop.host = "form-hop.example.com".to_string();
    form_hop.username = "form-user".to_string();
    form.proxy_hops.push(form_hop);

    let mut runtime_hop = crate::workspace::new_connection::NewConnectionProxyHop::new();
    runtime_hop.host = "runtime-hop.example.com".to_string();
    runtime_hop.username = "runtime-user".to_string();
    let request = save_request_from_form_with_proxy_hop_prefix(
        &mut form,
        std::slice::from_mut(&mut runtime_hop),
        None,
    )
    .unwrap();

    assert_eq!(request.proxy_chain.len(), 2);
    assert_eq!(request.proxy_chain[0].host, "runtime-hop.example.com");
    assert_eq!(request.proxy_chain[1].host, "form-hop.example.com");
}
