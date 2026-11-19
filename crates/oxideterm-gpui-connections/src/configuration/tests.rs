use super::*;
use crate::form_state::password_draft_mut;
use oxideterm_connections::{ConnectionTerminalOptions, SavedProxyHop};

fn save_request_from_form(
    form: &mut NewConnectionForm,
    id: Option<String>,
) -> anyhow::Result<SaveConnectionRequest> {
    save_request_from_form_with_proxy_hop_prefix(form, &mut [], id)
}

fn base_form() -> NewConnectionForm {
    let mut form = NewConnectionForm::default();
    form.name = "Home".to_string();
    form.host = "192.168.1.2".to_string();
    form.port = "22".to_string();
    form.username = "me".to_string();
    form.group = "Ungrouped".to_string();
    form
}

fn saved_connection_fixture(auth: SavedAuth) -> SavedConnection {
    let mut connection: SavedConnection = serde_json::from_value(serde_json::json!({
        "id": "conn-1",
        "name": "Home",
        "group": "Ungrouped",
        "host": "192.168.1.2",
        "port": 22,
        "username": "me",
        "auth": {"type": "agent"},
        "created_at": "2026-01-01T00:00:00Z",
    }))
    .unwrap();
    connection.auth = auth;
    connection
}

#[test]
fn save_request_moves_manual_proxy_command_into_a_redacted_secret_owner() {
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
fn new_connection_save_password_false_does_not_request_keychain_storage() {
    let mut form = base_form();
    form.password = "secret".to_string();
    form.save_password = false;

    let request = save_request_from_form(&mut form, None).unwrap();

    match request.auth {
        SavedAuth::Password {
            keychain_id: None,
            plaintext_password: None,
            ..
        } => {}
        other => panic!("unexpected auth: {other:?}"),
    }
}

#[test]
fn password_form_distinguishes_missing_from_explicit_empty_password() {
    for empty_password in [false, true] {
        let mut form = base_form();
        form.password.clear();
        form.save_password = true;
        form.empty_password = empty_password;
        let request = save_request_from_form(&mut form, None).unwrap();
        assert_eq!(request.auth.uses_empty_password(), empty_password);
        assert!(matches!(
            request.auth,
            SavedAuth::Password {
                keychain_id: None,
                plaintext_password: None,
                ..
            }
        ));
    }
}

#[test]
fn edit_properties_optional_name_preserves_connection_identity_and_saved_password() {
    let existing = SavedAuth::Password {
        empty_password: false,

        keychain_id: Some("kc-password".to_string()),
        plaintext_password: None,
    };
    let saved_connection = saved_connection_fixture(existing.clone());
    for (name, expected) in [
        ("Home", "Home"),
        ("", "deploy@server.example.com"),
        (" \t ", "deploy@server.example.com"),
    ] {
        let mut form = form_from_saved_connection(&saved_connection, None);
        assert!(!form.password_loaded);
        assert_eq!(
            form.saved_password_keychain_id.as_deref(),
            Some("kc-password")
        );
        form.name = name.to_string();
        form.host = " server.example.com ".to_string();
        form.username = " deploy ".to_string();
        form.save_password = true;

        let request = save_request_from_form_with_existing_auth(
            &mut form,
            Some("conn-1".to_string()),
            Some(&existing),
        )
        .unwrap();
        assert_eq!(request.name, expected);
        assert_eq!(request.id.as_deref(), Some("conn-1"));
        assert_eq!(request.host, "server.example.com");
        assert_eq!(request.username, "deploy");
        match request.auth {
            SavedAuth::Password {
                keychain_id: Some(keychain_id),
                plaintext_password: None,
                ..
            } => {
                assert_eq!(keychain_id, "kc-password");
            }
            other => panic!("unexpected auth: {other:?}"),
        }
    }
}

#[test]
fn edit_properties_switch_from_agent_to_password_submits_new_password() {
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
            ..
        } => assert_eq!(password, "new-secret"),
        other => panic!("unexpected auth: {other:?}"),
    }
}

#[test]
fn edit_properties_restores_proxy_metadata_and_can_remove_the_chain() {
    let mut saved_connection = saved_connection_fixture(SavedAuth::Agent);
    saved_connection.proxy_chain = vec![SavedProxyHop {
        totp_credential_id: None,
        host: "jump.example.com".to_string(),
        port: 2222,
        username: "ops".to_string(),
        auth: SavedAuth::Password {
            empty_password: false,

            keychain_id: Some("proxy-password-keychain-id".to_string()),
            plaintext_password: None,
        },
        agent_forwarding: true,
        identity_agent: Some("/tmp/proxy-agent.sock".to_string()),
        agent_forwarding_socket: Some("/tmp/proxy-forward.sock".to_string()),
        legacy_ssh_compatibility: true,
        ssh_algorithms: oxideterm_connections::SshAlgorithmPreferences::default(),
    }];
    let mut form = form_from_saved_connection(&saved_connection, None);

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
fn edit_properties_preserves_transport_policy_and_terminal_overrides() {
    let mut saved_connection = saved_connection_fixture(SavedAuth::Agent);
    saved_connection.options.legacy_ssh_compatibility = true;
    saved_connection.options.ssh_algorithms.mac = vec!["hmac-sha1".to_string()];
    saved_connection.options.dedicated_new_terminal_connection = true;
    saved_connection.options.open_sftp_on_connect = true;

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

    assert!(form.legacy_ssh_compatibility);
    assert!(request.legacy_ssh_compatibility);
    assert_eq!(form.ssh_algorithms.mac, ["hmac-sha1"]);
    assert_eq!(request.ssh_algorithms.mac, ["hmac-sha1"]);
    assert!(form.dedicated_new_terminal_connection);
    assert!(request.dedicated_new_terminal_connection);
    assert!(form.open_sftp_on_connect);
    assert!(request.open_sftp_on_connect);
    assert_eq!(form.terminal, saved_connection.options.terminal);
    assert_eq!(request.terminal, saved_connection.options.terminal);
}

#[test]
fn edit_properties_same_key_empty_passphrase_submits_no_new_secret() {
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
fn save_request_moves_all_visible_password_allocations_and_redacts_debug() {
    let mut form = base_form();
    form.identity_agent = "  /tmp/target-agent.sock  ".to_string();
    form.agent_forwarding_socket = Some("/tmp/target-forward.sock".to_string());
    form.password = "target-secret-marker".to_string();
    form.save_password = true;
    let target_pointer = form.password.as_ptr();

    let mut hop = NewConnectionProxyHop::new();
    hop.host = "jump.example.com".to_string();
    hop.port = "2222".to_string();
    hop.username = "ops".to_string();
    hop.agent_forwarding = true;
    hop.identity_agent = "  /tmp/jump-agent.sock  ".to_string();
    hop.agent_forwarding_socket = Some("/tmp/jump-forward.sock".to_string());
    hop.legacy_ssh_compatibility = true;
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
    assert_eq!(
        request.identity_agent.as_deref(),
        Some("/tmp/target-agent.sock")
    );
    assert_eq!(
        request.agent_forwarding_socket.as_deref(),
        Some("/tmp/target-forward.sock")
    );
    assert_eq!(request.proxy_chain.len(), 1);
    let saved_hop = &request.proxy_chain[0];
    assert_eq!(
        (
            saved_hop.host.as_str(),
            saved_hop.port,
            saved_hop.username.as_str()
        ),
        ("jump.example.com", 2222, "ops")
    );
    assert!(saved_hop.agent_forwarding);
    assert!(saved_hop.legacy_ssh_compatibility);
    assert_eq!(
        saved_hop.identity_agent.as_deref(),
        Some("/tmp/jump-agent.sock")
    );
    assert_eq!(
        saved_hop.agent_forwarding_socket.as_deref(),
        Some("/tmp/jump-forward.sock")
    );
    match &request.auth {
        SavedAuth::Password {
            plaintext_password: Some(password),
            ..
        } => assert_eq!(password.expose_secret().as_ptr(), target_pointer),
        other => panic!("unexpected target auth: {other:?}"),
    }
    match &request.proxy_chain[0].auth {
        SavedAuth::Password {
            keychain_id: None,
            plaintext_password: Some(password),
            ..
        } => {
            assert_eq!(password.expose_secret().as_ptr(), hop_pointer);
            assert_eq!(password.expose_secret(), "jump-secret-marker");
        }
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
fn upstream_proxy_test_handoff_preserves_visible_password() {
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
fn save_request_moves_key_passphrase_allocation() {
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
fn save_validation_failure_keeps_secret_allocations_in_the_form() {
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
fn runtime_proxy_hops_are_prepended_without_cloning_the_connection_form() {
    let mut form = base_form();
    form.auth_tab = SshAuthTab::Agent;
    let mut form_hop = NewConnectionProxyHop::new();
    form_hop.host = "form-hop.example.com".to_string();
    form_hop.username = "form-user".to_string();
    form.proxy_hops.push(form_hop);

    let mut runtime_hop = NewConnectionProxyHop::new();
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
fn viewing_saved_password_does_not_replace_stored_credential() {
    let existing = SavedAuth::Password {
        empty_password: false,

        keychain_id: Some("stored-owner".into()),
        plaintext_password: None,
    };
    let mut form = base_form();
    form.saved_password_keychain_id = Some("stored-owner".into());
    form.password = "revealed-test-value".into();
    form.password_from_store = true;
    form.password_loaded = true;
    let request = save_request_from_form_with_existing_auth(
        &mut form,
        Some("connection".into()),
        Some(&existing),
    )
    .unwrap();
    assert!(
        matches!(request.auth, SavedAuth::Password { keychain_id: Some(ref id), plaintext_password: None ,
            ..
} if id == "stored-owner")
    );
    password_draft_mut(&mut form).push_str("-edited");
    let edited = save_request_from_form_with_existing_auth(
        &mut form,
        Some("connection".into()),
        Some(&existing),
    )
    .unwrap();
    assert!(
        matches!(edited.auth, SavedAuth::Password { plaintext_password: Some(ref password), .. } if password.expose_secret() == "revealed-test-value-edited")
    );
    assert!(form.password.is_empty());
}

#[test]
fn standalone_sftp_empty_password_roundtrip_keeps_both_endpoints_independent() {
    for empty in [false, true] {
        let auth = |empty_password| SavedAuth::Password {
            empty_password,
            keychain_id: None,
            plaintext_password: None,
        };
        let mut profile = oxideterm_connections::StandaloneSftpProfile::new(
            "pair",
            "first.test",
            22,
            "first",
            auth(!empty),
        );
        profile.transfer_mode = oxideterm_connections::StandaloneSftpTransferMode::RemoteRemote;
        profile.secondary_endpoint = Some(oxideterm_connections::StandaloneSftpEndpoint::new(
            "second.test",
            22,
            "second",
            auth(empty),
        ));
        let form = form_from_standalone_sftp_profile(&profile);
        assert_eq!(form.empty_password, !empty);
        assert_eq!(form.standalone_sftp_secondary.empty_password, empty);
    }
}
