use chrono::Utc;
use gpui::{Keystroke, Modifiers};
use oxideterm_connections::{
    AuthType, ConnectionInfo, MoshIpFamily, MoshPredictionMode, MoshProfile, MoshUdpPortSelection,
    RemoteDesktopProfile, SavedAuth, SavedProxyHop, SavedUpstreamProxyPolicy,
    TelnetProfile,
};
use oxideterm_remote_desktop::{
    RemoteDesktopAudioOptions, RemoteDesktopClipboardOptions, RemoteDesktopDisplayOptions,
    RemoteDesktopProtocol, RemoteDesktopRdpNetworkProfile, RemoteDesktopRdpOptions,
};

use super::{
    NewConnectionField, NewConnectionForm, NewConnectionProxyHop, NewConnectionTransport,
    RemoteDesktopSessionOptions, RemoteDesktopVncCompression, RemoteDesktopVncImageQuality,
    RemoteDesktopVncOptions, RemoteDesktopVncSecurityPolicy, RemoteDesktopVncSessionMode,
    SshAuthFamily, SshAuthTab, SshKeyAuthSource, StandaloneSftpTransferMode, auth_family_from_tab,
    backspace_current_connection_field, connection_secret_field_visible, form_from_mosh_profile,
    form_from_remote_desktop_profile, form_from_telnet_profile,
    insert_text_into_current_connection_field, key_source_from_tab,
    select_current_connection_field, text_from_keystroke,
    toggle_connection_secret_field_visibility,
};

fn keystroke(key: &str, key_char: Option<&str>, modifiers: Modifiers) -> Keystroke {
    Keystroke {
        modifiers,
        key: key.to_string(),
        key_char: key_char.map(str::to_string),
    }
}

#[test]
fn primary_secret_visibility_is_hidden_and_toggles_independently() {
    let mut form = NewConnectionForm::default();

    assert_eq!(
        connection_secret_field_visible(&form, NewConnectionField::Password),
        Some(false)
    );
    assert_eq!(
        connection_secret_field_visible(&form, NewConnectionField::Passphrase),
        Some(false)
    );
    assert!(toggle_connection_secret_field_visibility(
        &mut form,
        NewConnectionField::Password
    ));
    assert_eq!(
        connection_secret_field_visible(&form, NewConnectionField::Password),
        Some(true)
    );
    assert_eq!(
        connection_secret_field_visible(&form, NewConnectionField::Passphrase),
        Some(false)
    );
    assert!(toggle_connection_secret_field_visibility(
        &mut form,
        NewConnectionField::Passphrase
    ));
    assert_eq!(
        connection_secret_field_visible(&form, NewConnectionField::Passphrase),
        Some(true)
    );
    assert!(!toggle_connection_secret_field_visibility(
        &mut form,
        NewConnectionField::Host
    ));
}

#[test]
fn visible_secret_drafts_remain_redacted_from_debug_output() {
    let mut form = NewConnectionForm::default();
    form.password = "password-value".to_string();
    form.password_visible = true;
    form.passphrase = "passphrase-value".to_string();
    form.passphrase_visible = true;

    let debug_output = format!("{form:?}");

    assert!(!debug_output.contains("password-value"));
    assert!(!debug_output.contains("passphrase-value"));
    assert!(debug_output.contains("[redacted secret]"));
}

#[test]
fn connection_secret_drafts_are_zeroized() {
    let mut form = NewConnectionForm::default();
    form.password = "password-value".to_string();
    form.passphrase = "passphrase-value".to_string();
    form.upstream_proxy_password = "proxy-password-value".to_string();
    form.standalone_sftp_transfer_mode = StandaloneSftpTransferMode::RemoteRemote;
    form.standalone_sftp_secondary.password = "secondary-password-value".to_string();
    form.standalone_sftp_secondary.passphrase = "secondary-passphrase-value".to_string();
    form.set_standalone_sftp_transfer_mode(StandaloneSftpTransferMode::LocalRemote);
    form.zeroize_secret_drafts();

    let mut proxy_hop = NewConnectionProxyHop::new();
    proxy_hop.password = "hop-password-value".to_string();
    proxy_hop.passphrase = "hop-passphrase-value".to_string();
    proxy_hop.zeroize_secret_drafts();

    assert!(form.password.is_empty());
    assert!(form.passphrase.is_empty());
    assert!(form.upstream_proxy_password.is_empty());
    assert!(form.standalone_sftp_secondary.password.is_empty());
    assert!(form.standalone_sftp_secondary.passphrase.is_empty());
    assert!(proxy_hop.password.is_empty());
    assert!(proxy_hop.passphrase.is_empty());
}

#[test]
fn remote_desktop_edit_form_restores_options_without_loading_secret() {
    let session_options = RemoteDesktopSessionOptions {
        clipboard: RemoteDesktopClipboardOptions {
            text: false,
            images: false,
            files: true,
        },
        audio: RemoteDesktopAudioOptions {
            playback: false,
            capture: true,
        },
        display: RemoteDesktopDisplayOptions {
            use_all_monitors: true,
        },
        rdp: RemoteDesktopRdpOptions {
            network_profile: RemoteDesktopRdpNetworkProfile::Broadband,
            disable_graphics_pipeline: true,
        },
        vnc: RemoteDesktopVncOptions {
            security_policy: RemoteDesktopVncSecurityPolicy::AllowLegacy,
            session_mode: RemoteDesktopVncSessionMode::Exclusive,
            image_quality: RemoteDesktopVncImageQuality::BestQuality,
            compression: RemoteDesktopVncCompression::High,
        },
    };
    let now = Utc::now();
    let profile = RemoteDesktopProfile {
        id: "remote-1".to_string(),
        name: "Lab desktop".to_string(),
        group: Some("Lab".to_string()),
        notes: Some("Shared display".to_string()),
        icon: Some("cloud".to_string()),
        color: Some("#7dd3fc".to_string()),
        icon_background_color: Some("#082f49".to_string()),
        protocol: RemoteDesktopProtocol::Rdp,
        host: "rdp.example.com".to_string(),
        port: 3389,
        username: Some("operator".to_string()),
        domain: Some("EXAMPLE".to_string()),
        credential_ref: Some("remote-desktop:remote-1".to_string()),
        ssh_gateway_connection_id: Some("gateway-1".to_string()),
        upstream_proxy: SavedUpstreamProxyPolicy::Custom {
            proxy: oxideterm_connections::SavedUpstreamProxyConfig {
                protocol: oxideterm_connections::SavedUpstreamProxyProtocol::Socks5,
                host: "proxy.test".into(),
                port: 1080,
                remote_dns: false,
                no_proxy: "*.internal".into(),
                auth: oxideterm_connections::SavedUpstreamProxyAuth::Password {
                    username: "proxy-user".into(),
                    keychain_id: Some("proxy-reference".into()),
                    plaintext_password: None,
                },
            },
        },
        read_only: true,
        session_options,
        created_at: now,
        updated_at: now,
        last_used_at: None,
    };

    let form = form_from_remote_desktop_profile(&profile, "Ungrouped".to_string());

    assert_eq!(form.remote_desktop_profile_id.as_deref(), Some("remote-1"));
    assert_eq!(form.transport, NewConnectionTransport::Rdp);
    assert_eq!(form.name, "Lab desktop");
    assert_eq!(form.host, "rdp.example.com");
    assert_eq!(form.port, "3389");
    assert_eq!(form.username, "operator");
    assert_eq!(form.group, "Lab");
    assert_eq!(form.notes, "Shared display");
    assert_eq!(form.icon, "cloud");
    assert_eq!(form.color, "#7dd3fc");
    assert_eq!(form.icon_background_color, "#082f49");
    assert_eq!(
        form.remote_desktop_ssh_gateway_connection_id.as_deref(),
        Some("gateway-1")
    );
    assert_eq!(form.remote_desktop_session_options, session_options);
    assert_eq!(
        form.saved_password_keychain_id.as_deref(),
        Some("remote-desktop:remote-1")
    );
    assert!(form.save_password);
    assert!(form.password.is_empty());
    assert_eq!(
        form.upstream_proxy_policy,
        super::NewConnectionUpstreamProxyPolicy::Custom
    );
    assert_eq!(form.upstream_proxy_host, "proxy.test");
    assert_eq!(form.upstream_proxy_port, "1080");
    assert_eq!(form.upstream_proxy_username, "proxy-user");
    assert_eq!(
        form.upstream_proxy_password_keychain_id.as_deref(),
        Some("proxy-reference")
    );
    assert!(form.upstream_proxy_password.is_empty());
    assert!(!form.upstream_proxy_remote_dns);
    assert_eq!(form.upstream_proxy_no_proxy, "*.internal");
}

#[test]
fn mosh_profile_form_preserves_edit_metadata_without_loading_credentials() {
    let mut profile = MoshProfile::new(
        "Roaming shell",
        "mosh.example.com",
        2222,
        "operator",
        SavedAuth::Password {
            empty_password: false,

            keychain_id: Some("mosh-password-owner".to_string()),
            plaintext_password: None,
        },
    );
    profile.id = "mosh-1".to_string();
    profile.group = Some("Mobile".to_string());
    profile.notes = Some("Intermittent link".to_string());
    profile.icon = Some("wifi".to_string());
    profile.color = Some("#93c5fd".to_string());
    profile.server_executable = "/opt/mosh/bin/mosh-server".to_string();
    profile.udp_host_override = Some("udp.example.com".to_string());
    profile.udp_port = MoshUdpPortSelection::Range {
        start: 60_000,
        end: 60_010,
    };
    profile.ip_family = MoshIpFamily::Ipv4;
    profile.prediction = MoshPredictionMode::Always;
    profile.locale = Some("en_US.UTF-8".to_string());
    profile.proxy_chain.push(SavedProxyHop {
        totp_credential_id: None,
        host: "jump.example.com".to_string(),
        port: 2200,
        username: "jump".to_string(),
        auth: SavedAuth::Agent,
        agent_forwarding: false,
        identity_agent: None,
        agent_forwarding_socket: None,
        legacy_ssh_compatibility: false,
        ssh_algorithms: oxideterm_connections::SshAlgorithmPreferences::default(),
    });

    let form = form_from_mosh_profile(&profile, "Ungrouped".to_string());

    assert_eq!(form.mosh_profile_id.as_deref(), Some("mosh-1"));
    assert_eq!(form.transport, NewConnectionTransport::Mosh);
    assert_eq!(form.name, "Roaming shell");
    assert_eq!(form.host, "mosh.example.com");
    assert_eq!(form.port, "2222");
    assert_eq!(form.username, "operator");
    assert_eq!(form.group, "Mobile");
    assert_eq!(form.notes, "Intermittent link");
    assert_eq!(form.icon, "wifi");
    assert_eq!(form.color, "#93c5fd");
    assert_eq!(form.mosh_server_executable, "/opt/mosh/bin/mosh-server");
    assert_eq!(form.mosh_udp_host, "udp.example.com");
    assert_eq!(form.mosh_udp_port, "60000:60010");
    assert_eq!(form.mosh_ip_family, MoshIpFamily::Ipv4);
    assert_eq!(form.mosh_prediction, MoshPredictionMode::Always);
    assert_eq!(form.mosh_locale, "en_US.UTF-8");
    assert_eq!(form.auth_tab, SshAuthTab::Password);
    assert_eq!(
        form.saved_password_keychain_id.as_deref(),
        Some("mosh-password-owner")
    );
    assert!(form.save_password);
    assert!(form.password.is_empty());
    assert!(form.proxy_chain_expanded);
    assert_eq!(form.proxy_hops.len(), 1);
    assert_eq!(form.proxy_hops[0].host, "jump.example.com");
    assert_eq!(form.proxy_hops[0].persisted_proxy_hop_index, Some(0));
}

#[test]
fn telnet_profile_form_restores_endpoint_and_terminal_settings_for_editing() {
    let mut profile = TelnetProfile::new("Router console", "router.example.com", 2323);
    profile.upstream_proxy = SavedUpstreamProxyPolicy::Custom {
        proxy: oxideterm_connections::SavedUpstreamProxyConfig {
            protocol: oxideterm_connections::SavedUpstreamProxyProtocol::HttpConnect,
            host: "proxy.test".into(),
            port: 8080,
            remote_dns: false,
            no_proxy: "router.local".into(),
            auth: oxideterm_connections::SavedUpstreamProxyAuth::Password {
                username: "proxy-user".into(),
                keychain_id: Some("protected-proxy".into()),
                plaintext_password: None,
            },
        },
    };
    profile.id = "telnet-1".to_string();
    profile.group = Some("Lab".to_string());
    profile.notes = Some("Legacy management plane".to_string());
    profile.icon = Some("network".to_string());
    profile.color = Some("#86efac".to_string());
    profile.icon_background_color = Some("#052e16".to_string());
    profile.terminal.encoding = Some(oxideterm_connections::ConnectionTerminalEncoding::Big5);
    profile.terminal.backspace_sequence =
        Some(oxideterm_connections::ConnectionTerminalBackspaceSequence::Delete);

    let form = form_from_telnet_profile(&profile, "Ungrouped".to_string());
    assert_eq!(
        form.upstream_proxy_policy,
        super::NewConnectionUpstreamProxyPolicy::Custom
    );
    assert_eq!(form.upstream_proxy_host, "proxy.test");
    assert_eq!(form.upstream_proxy_port, "8080");
    assert_eq!(form.upstream_proxy_no_proxy, "router.local");
    assert!(!form.upstream_proxy_remote_dns);
    assert_eq!(
        form.upstream_proxy_password_keychain_id.as_deref(),
        Some("protected-proxy")
    );

    assert_eq!(form.telnet_profile_id.as_deref(), Some("telnet-1"));
    assert_eq!(form.transport, NewConnectionTransport::Telnet);
    assert_eq!(form.telnet_profile_name, "Router console");
    assert_eq!(form.group, "Lab");
    assert_eq!(form.notes, "Legacy management plane");
    assert_eq!(form.icon, "network");
    assert_eq!(form.color, "#86efac");
    assert_eq!(form.icon_background_color, "#052e16");
    assert_eq!(form.host, "router.example.com");
    assert_eq!(form.port, "2323");
    assert_eq!(form.terminal, profile.terminal);
}

#[test]
fn text_input_uses_platform_text_and_ignores_shortcut_keystrokes() {
    let shifted = keystroke(
        "1",
        Some("!"),
        Modifiers {
            shift: true,
            ..Modifiers::default()
        },
    );
    let option_char = keystroke(
        "s",
        Some("ß"),
        Modifiers {
            alt: true,
            ..Modifiers::default()
        },
    );

    assert_eq!(text_from_keystroke(&shifted), Some("!"));
    assert_eq!(text_from_keystroke(&option_char), Some("ß"));
    let shortcut = keystroke(
        "v",
        None,
        Modifiers {
            platform: true,
            ..Modifiers::default()
        },
    );
    let control = keystroke(
        "a",
        Some("\u{1}"),
        Modifiers {
            control: true,
            ..Modifiers::default()
        },
    );

    assert_eq!(text_from_keystroke(&shortcut), None);
    assert_eq!(text_from_keystroke(&control), None);
}

#[test]
fn selected_text_is_replaced_by_committed_input() {
    let mut form = NewConnectionForm::default();
    form.host = "example.test".to_string();
    form.focused_field = NewConnectionField::Host;
    select_current_connection_field(&mut form);
    insert_text_into_current_connection_field(&mut form, "192.168.1.10");
    assert_eq!(form.host, "192.168.1.10");
    assert_eq!(form.selected_field, None);
}

#[test]
fn remote_desktop_form_uses_privacy_preserving_session_defaults() {
    let form = NewConnectionForm::default();

    assert!(form.remote_desktop_session_options.clipboard.text);
    assert!(form.remote_desktop_session_options.clipboard.images);
    assert!(!form.remote_desktop_session_options.clipboard.files);
    assert!(form.remote_desktop_session_options.audio.playback);
    assert!(!form.remote_desktop_session_options.audio.capture);
    assert!(!form.remote_desktop_session_options.display.use_all_monitors);
    assert_eq!(
        form.remote_desktop_session_options.vnc.security_policy,
        RemoteDesktopVncSecurityPolicy::RequireVerifiedEncryption
    );
    assert_eq!(
        form.remote_desktop_session_options.vnc.session_mode,
        RemoteDesktopVncSessionMode::Shared
    );
    assert_eq!(
        form.remote_desktop_session_options.vnc.image_quality,
        RemoteDesktopVncImageQuality::Balanced
    );
    assert_eq!(
        form.remote_desktop_session_options.vnc.compression,
        RemoteDesktopVncCompression::Balanced
    );
}

#[test]
fn backspace_handles_selection_and_empty_fields() {
    let mut form = NewConnectionForm::default();
    form.username = "root".to_string();
    form.focused_field = NewConnectionField::Username;
    select_current_connection_field(&mut form);
    assert!(backspace_current_connection_field(&mut form));
    assert!(form.username.is_empty());
    assert_eq!(form.selected_field, None);

    // Unselected text is edited one character at a time.
    form.username = "root".to_string();
    assert!(backspace_current_connection_field(&mut form));
    assert_eq!(form.username, "roo");
    assert_eq!(form.selected_field, None);

    // Empty fields report no change and stale selections are discarded.
    form.username.clear();
    form.focused_field = NewConnectionField::Name;
    assert!(!backspace_current_connection_field(&mut form));
    assert_eq!(form.selected_field, None);

    form.focused_field = NewConnectionField::Username;
    form.selected_field = Some(NewConnectionField::Host);
    assert!(backspace_current_connection_field(&mut form));
    assert_eq!(form.selected_field, None);
}

#[test]
fn jump_hop_uses_saved_connection_metadata_without_secrets() {
    let connection = ConnectionInfo {
        totp_credential_id: None,
        empty_password: false,
        id: "conn-1".to_string(),
        name: "Bastion".to_string(),
        group: Some("Prod".to_string()),
        notes: None,
        host: "bastion.example.com".to_string(),
        port: 2222,
        username: "jump".to_string(),
        auth_type: AuthType::Certificate,
        key_path: Some("~/.ssh/id_ed25519".to_string()),
        cert_path: Some("~/.ssh/id_ed25519-cert.pub".to_string()),
        managed_key_id: None,
        managed_key_name: None,
        gssapi_authentication: false,
        gssapi_server_identity: None,
        gssapi_delegate_credentials: false,
        proxy_chain: Vec::new(),
        upstream_proxy: SavedUpstreamProxyPolicy::UseGlobal,
        created_at: "2026-06-15T00:00:00Z".to_string(),
        last_used_at: None,
        color: None,
        icon_background_color: None,
        icon: None,
        tags: Vec::new(),
        agent_forwarding: true,
        identity_agent: None,
        agent_forwarding_socket: None,
        legacy_ssh_compatibility: true,
        ssh_algorithms: oxideterm_connections::SshAlgorithmPreferences::default(),
        post_connect_command: None,
    };
    let mut hop = NewConnectionProxyHop::new();
    hop.password = "old-password".to_string();
    hop.passphrase = "old-passphrase".to_string();

    hop.apply_saved_connection(&connection);

    assert_eq!(hop.saved_connection_id, "conn-1");
    assert_eq!(hop.host, "bastion.example.com");
    assert_eq!(hop.port, "2222");
    assert_eq!(hop.username, "jump");
    assert_eq!(hop.auth_tab, SshAuthTab::Certificate);
    assert_eq!(auth_family_from_tab(hop.auth_tab), SshAuthFamily::Key);
    assert_eq!(
        key_source_from_tab(hop.auth_tab),
        Some(SshKeyAuthSource::Certificate)
    );
    assert_eq!(hop.key_path, "~/.ssh/id_ed25519");
    assert_eq!(hop.cert_path, "~/.ssh/id_ed25519-cert.pub");
    assert!(hop.password.is_empty());
    assert!(hop.passphrase.is_empty());
    assert!(hop.agent_forwarding);
    assert!(hop.legacy_ssh_compatibility);
}
