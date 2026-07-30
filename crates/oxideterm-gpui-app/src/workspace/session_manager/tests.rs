use super::*;
use oxideterm_gpui_ui::TauriTableMetrics;

const MANAGER_COL_CHECKBOX: f32 = 32.0;
const MANAGER_COL_NAME_BASIS: f32 = 140.0;
const MANAGER_COL_HOST: f32 = 130.0;
const MANAGER_COL_PORT: f32 = 50.0;
const MANAGER_COL_USERNAME: f32 = 90.0;
const MANAGER_COL_AUTH: f32 = 72.0;
const MANAGER_COL_GROUP: f32 = 100.0;
const MANAGER_COL_LAST_USED: f32 = 90.0;
const MANAGER_COL_ACTIONS: f32 = 84.0;

pub(super) fn manager_table_min_width_for_metrics(metrics: TauriTableMetrics) -> f32 {
    // Tauri ConnectionTable columns: px-2 wrapper plus w-8, w-[140px],
    // w-[130px], w-[50px], w-[90px], w-[72px], w-[100px], w-[90px],
    // and sticky w-[84px] actions.
    metrics.padding_x * 2.0
        + MANAGER_COL_CHECKBOX
        + MANAGER_COL_NAME_BASIS
        + MANAGER_COL_HOST
        + MANAGER_COL_PORT
        + MANAGER_COL_USERNAME
        + MANAGER_COL_AUTH
        + MANAGER_COL_GROUP
        + MANAGER_COL_LAST_USED
        + MANAGER_COL_ACTIONS
}

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
        host: "192.168.1.2".to_string(),
        port: 22,
        username: "me".to_string(),
        auth,
        proxy_chain: Vec::new(),
        upstream_proxy: SavedUpstreamProxyPolicy::UseGlobal,
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
pub(super) fn session_manager_table_width_matches_tauri_connection_table_columns() {
    // This locks the Tauri ConnectionTable min-w-fit contract that keeps
    // horizontal scrolling, row dividers, and the sticky actions column aligned.
    assert_eq!(
        manager_table_min_width_for_metrics(TauriTableMetrics::default()),
        804.0
    );
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
pub(super) fn session_manager_main_views_keep_independent_empty_list_states() {
    let state = SessionManagerState::default();

    assert_eq!(state.main_grid_list_state.item_count(), 0);
    assert_eq!(state.main_list_state.item_count(), 0);
    assert_eq!(state.main_tree_list_state.item_count(), 0);
}

#[test]
pub(super) fn session_manager_main_views_use_virtual_lists_as_scroll_owners() {
    let source = include_str!("views.rs");
    for (function_name, next_function_name) in [
        (
            "pub(super) fn render_session_manager_grid_view",
            "pub(super) fn render_session_manager_grid_row",
        ),
        (
            "pub(super) fn render_session_manager_list_view",
            "pub(super) fn render_session_manager_tree_view",
        ),
        (
            "pub(super) fn render_session_manager_tree_view",
            "pub(super) fn render_session_manager_view_actions",
        ),
    ] {
        let function_start = source.find(function_name).expect("main view function");
        let function_tail = &source[function_start + function_name.len()..];
        let function_end = function_tail
            .find(next_function_name)
            .expect("next main view function");
        let function_source = &function_tail[..function_end];
        assert!(function_source.contains("tauri_virtual_list("));
        assert!(!function_source.contains("overflow_y_scrollbar"));
    }
}

#[test]
pub(super) fn session_manager_virtual_rows_claim_the_available_list_width() {
    let source = include_str!("views.rs");
    // Every top-level virtual row must stretch independently of its content width.
    for (function_name, next_function_name) in [
        (
            "pub(super) fn render_session_manager_section_header",
            "pub(super) fn render_session_manager_item_card",
        ),
        (
            "pub(super) fn render_session_manager_tree_group_row",
            "pub(super) fn render_session_manager_display_item_row",
        ),
        (
            "pub(super) fn render_session_manager_display_item_row",
            "pub(super) fn render_session_manager_item_icon",
        ),
    ] {
        let function_start = source.find(function_name).expect("row function");
        let function_tail = &source[function_start + function_name.len()..];
        let function_end = function_tail
            .find(next_function_name)
            .expect("next row function");
        let function_source = &function_tail[..function_end];
        assert!(function_source.contains(".w_full()"));
        assert!(function_source.contains(".min_w(px(0.0))"));
    }

    let grid_row_start = source
        .find("pub(super) fn render_session_manager_grid_row")
        .expect("grid row function");
    let grid_row_tail =
        &source[grid_row_start + "pub(super) fn render_session_manager_grid_row".len()..];
    let grid_row_end = grid_row_tail
        .find("pub(super) fn render_session_manager_recent_item")
        .expect("next grid function");
    let grid_row_source = &grid_row_tail[..grid_row_end];
    assert_eq!(grid_row_source.matches(".w_full()").count(), 2);
    assert_eq!(grid_row_source.matches(".min_w(px(0.0))").count(), 2);
}

#[test]
pub(super) fn session_manager_grid_rows_preserve_symmetric_outer_gutters() {
    let source = include_str!("views.rs");
    let grid_view_start = source
        .find("pub(super) fn render_session_manager_grid_view")
        .expect("grid view function");
    let grid_view_tail =
        &source[grid_view_start + "pub(super) fn render_session_manager_grid_view".len()..];
    let grid_view_end = grid_view_tail
        .find("pub(super) fn render_session_manager_grid_row")
        .expect("grid row function");
    let grid_view_source = &grid_view_tail[..grid_view_end];
    assert!(grid_view_source.contains(".pt(px(self.tokens.spacing.three))"));
    assert!(!grid_view_source.contains(".p(px(self.tokens.spacing.three))"));

    let grid_row_tail = &grid_view_tail[grid_view_end..];
    let grid_row_end = grid_row_tail
        .find("pub(super) fn render_session_manager_recent_item")
        .expect("recent item function");
    let grid_row_source = &grid_row_tail[..grid_row_end];
    assert_eq!(
        grid_row_source
            .matches(".px(px(self.tokens.spacing.three))")
            .count(),
        2
    );

    let header_start = source
        .find("pub(super) fn render_session_manager_section_header")
        .expect("section header function");
    let header_tail =
        &source[header_start + "pub(super) fn render_session_manager_section_header".len()..];
    let header_end = header_tail
        .find("pub(super) fn render_session_manager_item_card")
        .expect("item card function");
    assert!(
        header_tail[..header_end].contains(".px(px(self.tokens.spacing.three))"),
        "grid section headers must align with card rows"
    );
}

#[test]
pub(super) fn session_menu_dismissal_closes_all_manager_popovers() {
    let mut state = SessionManagerState {
        show_batch_move: true,
        row_action_menu: Some(SessionManagerRowActionMenu {
            target: SessionManagerRowActionTarget::Connection("connection-1".to_string()),
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
pub(super) fn remote_desktop_selection_is_typed_separately_from_ssh_ids() {
    let now = Utc::now();
    let ssh = SessionManagerDisplayItem::Connection(ConnectionInfo {
        id: "shared-id".to_string(),
        ..connection_info_fixture(None)
    });
    let remote = SessionManagerDisplayItem::RemoteDesktop(RemoteDesktopProfile {
        id: "shared-id".to_string(),
        name: "Remote desktop".to_string(),
        group: None,
        icon: None,
        color: None,
        icon_background_color: None,
        protocol: oxideterm_remote_desktop::RemoteDesktopProtocol::Rdp,
        host: "rdp.example.com".to_string(),
        port: 3389,
        username: Some("operator".to_string()),
        domain: None,
        credential_ref: None,
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
pub(super) fn oxide_export_logical_scroll_change_detects_inner_consumption() {
    // GPUI ListState owns measured row heights internally, so scroll-chain
    // decisions must compare actual logical movement instead of estimates.
    assert!(!oxide_export_logical_scroll_changed(0, 0.0, 0, 0.0));
    assert!(!oxide_export_logical_scroll_changed(0, 12.0, 0, 12.004));
    assert!(oxide_export_logical_scroll_changed(0, 0.0, 0, 24.0));
    assert!(oxide_export_logical_scroll_changed(0, 24.0, 1, 0.0));
}

#[test]
pub(super) fn oxide_export_selection_count_label_uses_locale_placeholders() {
    assert_eq!(
        oxide_export_selection_count_label(
            "Select Connections to Export ({{selected}}/{{total}})".to_string(),
            2,
            5,
        ),
        "Select Connections to Export (2/5)"
    );
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
        false,
        None,
        Some(&export_dialog),
    ));

    let mut import_dialog = OxideImportDialogState::default();
    import_dialog.file_data = Some(vec![1].into());
    assert!(session_manager_input_is_active(
        SessionManagerInput::OxideImportPassword,
        false,
        false,
        Some(&import_dialog),
        None,
    ));

    assert!(!session_manager_input_is_active(
        SessionManagerInput::Search,
        false,
        false,
        None,
        None,
    ));
    assert!(session_manager_input_is_active(
        SessionManagerInput::Search,
        true,
        false,
        None,
        None,
    ));
}

#[test]
pub(super) fn saved_sidebar_search_is_active_only_while_its_sidebar_is_visible() {
    assert!(session_manager_input_is_active(
        SessionManagerInput::SavedSearch,
        false,
        true,
        None,
        None,
    ));
    assert!(!session_manager_input_is_active(
        SessionManagerInput::SavedSearch,
        true,
        false,
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
    let saved_connection = saved_connection_fixture(existing.clone());
    let mut form = form_from_saved_connection(&saved_connection, None);
    form.auth_tab = SshAuthTab::Password;
    form.password = "new-secret".to_string();

    let request = save_request_from_form_with_existing_auth(
        &mut form,
        Some(saved_connection.id),
        Some(&existing),
    )
    .unwrap();

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
pub(super) fn edit_properties_preserves_legacy_ssh_compatibility() {
    let mut saved_connection = saved_connection_fixture(SavedAuth::Agent);
    saved_connection.options.legacy_ssh_compatibility = true;

    // Editing and saving an existing connection must round-trip its transport policy.
    let mut form = form_from_saved_connection(&saved_connection, None);
    let request = save_request_from_form(&mut form, Some(saved_connection.id.clone())).unwrap();

    assert!(form.legacy_ssh_compatibility);
    assert!(request.legacy_ssh_compatibility);
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

#[test]
pub(super) fn basic_dialog_tab_order_wraps_through_text_input_like_radix_dialog() {
    assert_eq!(
        browser_behavior::modal_footer_input_key_action(
            "tab",
            false,
            &SESSION_MANAGER_BASIC_DIALOG_FOOTER_ACTIONS,
            true,
            true,
            None,
            SessionManagerBasicDialogFooterAction::Cancel,
            None,
        ),
        Some(browser_behavior::ModalFooterInputKeyAction::FocusFooter(
            SessionManagerBasicDialogFooterAction::Cancel
        ))
    );

    assert_eq!(
        browser_behavior::modal_footer_input_key_action(
            "tab",
            false,
            &SESSION_MANAGER_BASIC_DIALOG_FOOTER_ACTIONS,
            true,
            false,
            Some(SessionManagerBasicDialogFooterAction::Primary),
            SessionManagerBasicDialogFooterAction::Cancel,
            None,
        ),
        Some(browser_behavior::ModalFooterInputKeyAction::FocusInput)
    );

    assert_eq!(
        browser_behavior::modal_footer_input_key_action(
            "tab",
            true,
            &SESSION_MANAGER_BASIC_DIALOG_FOOTER_ACTIONS,
            true,
            false,
            Some(SessionManagerBasicDialogFooterAction::Cancel),
            SessionManagerBasicDialogFooterAction::Cancel,
            None,
        ),
        Some(browser_behavior::ModalFooterInputKeyAction::FocusInput)
    );
}

#[test]
pub(super) fn basic_dialog_footer_arrows_stay_inside_footer_actions() {
    assert_eq!(
        browser_behavior::modal_footer_input_key_action(
            "arrowleft",
            false,
            &SESSION_MANAGER_BASIC_DIALOG_FOOTER_ACTIONS,
            false,
            false,
            Some(SessionManagerBasicDialogFooterAction::Cancel),
            SessionManagerBasicDialogFooterAction::Cancel,
            None,
        ),
        Some(browser_behavior::ModalFooterInputKeyAction::FocusFooter(
            SessionManagerBasicDialogFooterAction::Primary
        ))
    );

    assert_eq!(
        browser_behavior::modal_footer_input_key_action(
            "arrowright",
            false,
            &SESSION_MANAGER_BASIC_DIALOG_FOOTER_ACTIONS,
            false,
            false,
            Some(SessionManagerBasicDialogFooterAction::Primary),
            SessionManagerBasicDialogFooterAction::Cancel,
            None,
        ),
        Some(browser_behavior::ModalFooterInputKeyAction::FocusFooter(
            SessionManagerBasicDialogFooterAction::Cancel
        ))
    );
}
