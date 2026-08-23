use super::*;
use oxideterm_connections::SavedProxyHop;

pub(super) fn base_form() -> NewConnectionForm {
    let mut form = NewConnectionForm::default();
    form.name = "Home".to_string();
    form.host = "192.168.1.2".to_string();
    form.port = "22".to_string();
    form.username = "me".to_string();
    form.group = "Ungrouped".to_string();
    form
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
        ssh_algorithms: oxideterm_connections::SshAlgorithmPreferences::default(),
    }];
    let form = form_from_saved_connection(&saved_connection, None);

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
        ssh_algorithms: oxideterm_connections::SshAlgorithmPreferences::default(),
    }];
    let mut form = form_from_saved_connection(&saved_connection, None);
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
        session_log_policy: oxideterm_connections::ConnectionTerminalSessionLogPolicy::Manual,
    };

    let mut form = form_from_saved_connection(&saved_connection, None);
    let request = save_request_from_form(&mut form, Some(saved_connection.id.clone())).unwrap();

    assert_eq!(form.terminal, saved_connection.options.terminal);
    assert_eq!(request.terminal, saved_connection.options.terminal);
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
            gssapi_enabled: false,
            gssapi_server_identity: String::new(),
            gssapi_delegate_credentials: false,
            agent_forwarding: true,
            identity_agent: "  /tmp/jump-agent.sock  ".to_string(),
            agent_forwarding_socket: Some("/tmp/jump-forward.sock".to_string()),
            legacy_ssh_compatibility: true,
            ssh_algorithms: oxideterm_connections::SshAlgorithmPreferences::default(),
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
            gssapi_enabled: false,
            gssapi_server_identity: String::new(),
            gssapi_delegate_credentials: false,
            agent_forwarding: false,
            identity_agent: String::new(),
            agent_forwarding_socket: None,
            legacy_ssh_compatibility: false,
            ssh_algorithms: oxideterm_connections::SshAlgorithmPreferences::default(),
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
