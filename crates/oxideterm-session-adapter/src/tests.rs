// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use chrono::Utc;
use oxideterm_connections::{
    ConnectionOptions, ConnectionStore, ConnectionTerminalBackspaceSequence,
    ConnectionTerminalDeleteSequence, ConnectionTerminalEncoding, SavedAuth, SavedConnection,
    SavedConnectionRuntimeSecrets, SavedProxyCommand, SavedProxyHop, SavedUpstreamProxyAuth,
    SavedUpstreamProxyConfig, SavedUpstreamProxyPolicy, SavedUpstreamProxyProtocol, SecretString,
};
use oxideterm_settings::{
    PersistedSettings, SettingsUpstreamProxyAuth, SettingsUpstreamProxyConfig,
    SettingsUpstreamProxyProtocol,
};
use oxideterm_ssh::{AuthMethod, ProxyCommandConfig, UpstreamProxyAuth};

use crate::ssh::proxy_command_runtime_policy;
use crate::{
    reconnect_max_attempts_from_settings, reconnect_timing_from_settings,
    sftp_runtime_settings_from_settings, terminal_backspace_sequence_from_connection,
    terminal_delete_sequence_from_connection, terminal_encoding_from_connection,
    terminal_encoding_from_settings,
};
use crate::{
    ssh_config_for_saved_connection_hop, ssh_config_from_saved_connection,
    ssh_config_from_saved_connection_with_runtime_secrets, upstream_proxy_config_from_saved_policy,
};

fn temp_connection_store(name: &str) -> (ConnectionStore, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!(
        "oxideterm-session-adapter-{name}-{}-connections.json",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    (ConnectionStore::load(&path).unwrap(), path)
}

fn saved_connection(auth: SavedAuth) -> SavedConnection {
    let now = Utc::now();
    SavedConnection {
        id: "conn-1".to_string(),
        version: oxideterm_connections::CONFIG_VERSION,
        name: "Home".to_string(),
        group: None,
        notes: None,
        host: "target.example.com".to_string(),
        port: 22,
        username: "me".to_string(),
        auth,
        proxy_chain: Vec::new(),
        upstream_proxy: SavedUpstreamProxyPolicy::UseGlobal,
        proxy_command: None,
        options: ConnectionOptions::default(),
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
fn missing_referenced_password_uses_runtime_prompt_with_kerberos_and_proxy_hops_preserved() {
    // This is the only test in this binary that reads a keychain reference. Use an
    // empty store so headless hosts exercise a missing entry, not an unavailable service.
    #[cfg(not(target_os = "macos"))]
    keyring::set_default_credential_builder(keyring::mock::default_credential_builder());

    let (store, path) = temp_connection_store("missing-referenced-password");
    let password = SavedAuth::Password {
        empty_password: false,
        keychain_id: Some(format!(
            "oxideterm-missing-password-regression-{}",
            std::process::id()
        )),
        plaintext_password: None,
    };
    let mut connection = saved_connection(SavedAuth::KerberosPreferred {
        server_identity: Some("host/target.example.com".into()),
        delegate_credentials: false,
        fallback: Box::new(password.clone()),
    });
    connection.proxy_chain.push(SavedProxyHop {
        totp_credential_id: None,
        host: "jump.example.com".into(),
        port: 2222,
        username: "jump-user".into(),
        auth: password,
        agent_forwarding: false,
        identity_agent: None,
        agent_forwarding_socket: None,
        legacy_ssh_compatibility: false,
        ssh_algorithms: Default::default(),
    });
    let config =
        ssh_config_from_saved_connection(&store, &PersistedSettings::default(), &connection)
            .unwrap();
    assert_eq!(
        config.auth,
        AuthMethod::kerberos_preferred(
            AuthMethod::password_prompt(),
            Some("host/target.example.com".into()),
            false
        )
    );
    let hop = &config.proxy_chain.as_ref().unwrap()[0];
    assert_eq!(
        (&*hop.host, hop.port, &*hop.username, &hop.auth),
        (
            "jump.example.com",
            2222,
            "jump-user",
            &AuthMethod::password_prompt()
        )
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn runtime_settings_conversion_clamps_persisted_values() {
    let mut settings = PersistedSettings::default();
    settings.sftp.max_concurrent_transfers = 0;
    settings.sftp.directory_parallelism = 0;
    settings.sftp.speed_limit_enabled = false;
    settings.sftp.speed_limit_kbps = 4096;
    settings.reconnect.base_delay_ms = 0;
    settings.reconnect.max_delay_ms = 0;
    settings.reconnect.max_attempts = 0;

    let sftp = sftp_runtime_settings_from_settings(&settings);
    assert_eq!(sftp.max_concurrent_transfers, 1);
    assert_eq!(sftp.directory_parallelism, 1);
    assert_eq!(sftp.speed_limit_kbps, 0);
    let reconnect = reconnect_timing_from_settings(&settings);
    assert_eq!(reconnect.retry_base_delay.as_millis(), 1);
    assert_eq!(reconnect.retry_max_delay.as_millis(), 1);
    assert_eq!(reconnect_max_attempts_from_settings(&settings), 1);
    assert_eq!(
        terminal_encoding_from_settings(oxideterm_settings::TerminalEncoding::Gb18030),
        oxideterm_terminal::TerminalEncoding::Gb18030
    );
}

#[test]
fn saved_connection_terminal_options_map_to_runtime_sequences() {
    assert_eq!(
        terminal_encoding_from_connection(ConnectionTerminalEncoding::Windows1252),
        oxideterm_terminal::TerminalEncoding::Windows1252
    );
    assert_eq!(
        terminal_backspace_sequence_from_connection(ConnectionTerminalBackspaceSequence::ControlH),
        oxideterm_settings::TerminalBackspaceSequence::ControlH
    );
    assert_eq!(
        terminal_delete_sequence_from_connection(ConnectionTerminalDeleteSequence::Delete),
        oxideterm_settings::TerminalDeleteSequence::Delete
    );
}

#[test]
fn password_auth_prompts_only_when_no_explicit_empty_value_is_saved() {
    let (store, path) = temp_connection_store("empty-password-auth");
    for (empty_password, expected_prompt) in [(false, true), (true, false)] {
        let saved = SavedAuth::Password {
            empty_password,
            keychain_id: None,
            plaintext_password: None,
        };
        let auth = crate::auth_method_from_saved_auth(&store, &saved).unwrap();
        let AuthMethod::Password { password, prompt } = auth else {
            panic!("expected password authentication");
        };
        assert_eq!(prompt, expected_prompt);
        assert_eq!(password.as_str(), "");
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn proxy_command_requires_authorization_before_runtime_hydration() {
    let words = || {
        Some(vec![
            SecretString::new("helper-with-token"),
            SecretString::new("credential-value"),
        ])
    };

    let denied = proxy_command_runtime_policy(false, words()).unwrap();
    assert_eq!(denied, ProxyCommandConfig::AuthorizationRequired);
    assert!(!format!("{denied:?}").contains("credential-value"));

    let authorized = proxy_command_runtime_policy(true, words()).unwrap();
    assert!(matches!(authorized, ProxyCommandConfig::Direct { .. }));
    assert!(!format!("{authorized:?}").contains("credential-value"));
}

#[test]
fn manual_proxy_command_uses_runtime_secret_and_overrides_other_routes() {
    let (store, path) = temp_connection_store("manual-proxy-command");
    let mut connection = saved_connection(SavedAuth::Agent);
    connection.proxy_command = Some(SavedProxyCommand {
        keychain_id: Some("proxy-command-owner".to_string()),
        plaintext_command: None,
    });
    connection.proxy_chain.push(SavedProxyHop {
        totp_credential_id: None,
        host: "unused-jump.example.com".to_string(),
        port: 22,
        username: "jump".to_string(),
        auth: SavedAuth::Password {
            empty_password: false,

            keychain_id: None,
            plaintext_password: None,
        },
        agent_forwarding: false,
        identity_agent: None,
        agent_forwarding_socket: None,
        legacy_ssh_compatibility: false,
        ssh_algorithms: oxideterm_connections::SshAlgorithmPreferences::default(),
    });
    let mut settings = PersistedSettings::default();
    settings.ssh_config.allow_proxy_command = true;

    let config = ssh_config_from_saved_connection_with_runtime_secrets(
        &store,
        &settings,
        &connection,
        SavedConnectionRuntimeSecrets {
            auth: None,
            proxy_chain: vec![None],
            upstream_proxy: None,
            proxy_command: Some(SecretString::from(
                "helper --token secret-value --target %h:%p",
            )),
        },
        None,
    )
    .unwrap();

    assert!(config.proxy_chain.is_none());
    assert!(!format!("{config:?}").contains("secret-value"));
    let ProxyCommandConfig::Direct { args, .. } = config.proxy_command.unwrap() else {
        panic!("manual ProxyCommand should hydrate a direct runtime command");
    };
    assert!(
        args.iter()
            .any(|argument| argument.as_str() == "target.example.com:22")
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn legacy_jump_host_becomes_runtime_proxy_chain() {
    let (mut store, path) = temp_connection_store("legacy-jump-host");
    let mut jump = saved_connection(SavedAuth::Agent);
    jump.id = "jump-1".to_string();
    jump.host = "jump.example.com".to_string();
    jump.port = 2200;
    jump.username = "jump".to_string();
    store.upsert_imported_connection(jump).unwrap();

    let mut target = saved_connection(SavedAuth::Agent);
    target.options.jump_host = Some("jump-1".to_string());
    let config = ssh_config_from_saved_connection(&store, &PersistedSettings::default(), &target)
        .expect("legacy jump host should resolve");

    let chain = config.proxy_chain.expect("one legacy jump host");
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].host, "jump.example.com");
    assert_eq!(chain[0].port, 2200);
    assert_eq!(chain[0].username, "jump");
    let _ = std::fs::remove_file(path);
}

#[test]
fn saved_proxy_chain_maps_to_shared_and_independent_runtime_configs() {
    let (store, path) = temp_connection_store("materialized-hops");
    let mut connection = saved_connection(SavedAuth::Agent);
    connection.options.connect_timeout_seconds = Some(180);
    connection.options.totp_credential_id = Some("target-mfa".into());
    connection.proxy_chain = vec![SavedProxyHop {
        totp_credential_id: Some("jump-mfa".into()),
        host: "jump.example.com".to_string(),
        port: 2222,
        username: "ops".to_string(),
        auth: SavedAuth::Agent,
        agent_forwarding: true,
        identity_agent: Some("/tmp/jump-agent.sock".to_string()),
        agent_forwarding_socket: Some("/tmp/jump-forward.sock".to_string()),
        legacy_ssh_compatibility: true,
        ssh_algorithms: oxideterm_connections::SshAlgorithmPreferences::default(),
    }];
    let settings = PersistedSettings::default();

    let config = ssh_config_from_saved_connection(&store, &settings, &connection).unwrap();

    assert!(config.strict_host_key_checking);
    let chain = config.proxy_chain.unwrap();
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].host, "jump.example.com");
    assert_eq!(chain[0].port, 2222);
    assert_eq!(chain[0].username, "ops");
    assert!(chain[0].agent_forwarding);
    assert_eq!(
        chain[0].identity_agent.as_deref(),
        Some("/tmp/jump-agent.sock")
    );
    assert_eq!(
        chain[0].agent_forwarding_socket.as_deref(),
        Some("/tmp/jump-forward.sock")
    );
    assert!(chain[0].legacy_ssh_compatibility);

    let jump = ssh_config_for_saved_connection_hop(&store, &settings, &connection, 0)
        .expect("saved jump should become a runtime config");
    let target = ssh_config_for_saved_connection_hop(&store, &settings, &connection, 1)
        .expect("target should follow the materialized jumps");

    assert_eq!(jump.host, "jump.example.com");
    assert_eq!(
        jump.totp.as_ref().map(|p| p.credential_id.as_str()),
        Some("jump-mfa")
    );
    assert_eq!(jump.timeout_secs, 180);
    assert!(jump.proxy_chain.is_none());
    assert_eq!(jump.identity_agent.as_deref(), Some("/tmp/jump-agent.sock"));
    assert_eq!(
        jump.agent_forwarding_socket.as_deref(),
        Some("/tmp/jump-forward.sock")
    );
    assert_eq!(target.host, "target.example.com");
    assert_eq!(
        target.totp.as_ref().map(|p| p.credential_id.as_str()),
        Some("target-mfa")
    );
    let bound_key = target.connection_key();
    let mut unbound = target.clone();
    unbound.totp = None;
    assert_ne!(
        bound_key,
        unbound.connection_key(),
        "different MFA bindings cannot share a pooled transport"
    );
    assert_eq!(target.timeout_secs, 180);
    assert!(target.proxy_chain.is_none());
    assert!(ssh_config_for_saved_connection_hop(&store, &settings, &connection, 2).is_none());
    let _ = std::fs::remove_file(path);
}

#[test]
fn saved_managed_key_becomes_reference_only_ssh_config() {
    let (store, path) = temp_connection_store("managed-key");
    let conn = saved_connection(SavedAuth::ManagedKey {
        key_id: "managed-key-1".to_string(),
        passphrase_keychain_id: None,
        plaintext_passphrase: None,
    });

    let settings = PersistedSettings::default();
    let config = ssh_config_from_saved_connection(&store, &settings, &conn).unwrap();

    assert!(matches!(
        config.auth,
        AuthMethod::ManagedKey { key_id, passphrase }
            if key_id == "managed-key-1" && passphrase.is_none()
    ));
    let _ = std::fs::remove_file(path);
}

#[test]
fn saved_upstream_proxy_policy_overrides_global_proxy_and_hydrates_custom_secret() {
    let (store, path) = temp_connection_store("custom-proxy");
    let mut settings = PersistedSettings::default();
    settings.network.upstream_proxy = Some(SettingsUpstreamProxyConfig {
        protocol: SettingsUpstreamProxyProtocol::Socks5,
        host: "global-proxy.local".to_string(),
        port: 1080,
        auth: SettingsUpstreamProxyAuth::None,
        remote_dns: true,
        no_proxy: String::new(),
    });
    let policy = SavedUpstreamProxyPolicy::Custom {
        proxy: SavedUpstreamProxyConfig {
            protocol: SavedUpstreamProxyProtocol::Socks5,
            host: "custom-proxy.local".to_string(),
            port: 1080,
            auth: SavedUpstreamProxyAuth::Password {
                username: "custom-user".to_string(),
                keychain_id: None,
                plaintext_password: Some(SecretString::new("custom-secret")),
            },
            remote_dns: true,
            no_proxy: "localhost".to_string(),
        },
    };

    let proxy = upstream_proxy_config_from_saved_policy(&store, &settings, &policy)
        .unwrap()
        .unwrap();

    assert_eq!(proxy.host, "custom-proxy.local");
    assert_eq!(proxy.no_proxy, "localhost");
    match proxy.auth {
        UpstreamProxyAuth::Password { username, password } => {
            assert_eq!(username, "custom-user");
            assert_eq!(password.as_str(), "custom-secret");
        }
        UpstreamProxyAuth::None => panic!("expected password auth"),
    }
    assert!(
        upstream_proxy_config_from_saved_policy(
            &store,
            &settings,
            &SavedUpstreamProxyPolicy::Direct
        )
        .unwrap()
        .is_none()
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn global_upstream_proxy_overrides_environment_and_requires_saved_credentials() {
    let _socks_env = EnvVarGuard::set("OXIDETERM_SOCKS5_PROXY", "env-proxy.local:1080");
    let _http_env = EnvVarGuard::set("OXIDETERM_HTTP_PROXY", "http://env-http.local:8080");
    let (store, path) = temp_connection_store("global-proxy-priority");
    let mut settings = PersistedSettings::default();
    settings.network.upstream_proxy = Some(SettingsUpstreamProxyConfig {
        protocol: SettingsUpstreamProxyProtocol::Socks5,
        host: "global-proxy.local".to_string(),
        port: 1080,
        auth: SettingsUpstreamProxyAuth::None,
        remote_dns: true,
        no_proxy: String::new(),
    });
    let policy = SavedUpstreamProxyPolicy::UseGlobal;

    let proxy = upstream_proxy_config_from_saved_policy(&store, &settings, &policy)
        .unwrap()
        .unwrap();

    assert_eq!(proxy.host, "global-proxy.local");
    assert!(matches!(proxy.auth, UpstreamProxyAuth::None));
    let global_proxy = settings.network.upstream_proxy.as_mut().unwrap();
    global_proxy.protocol = SettingsUpstreamProxyProtocol::HttpConnect;
    global_proxy.port = 8080;
    global_proxy.auth = SettingsUpstreamProxyAuth::Password {
        username: "proxy-user".to_string(),
        keychain_id: None,
    };

    let error = upstream_proxy_config_from_saved_policy(
        &store,
        &settings,
        &SavedUpstreamProxyPolicy::UseGlobal,
    )
    .expect_err("missing proxy credentials must not silently select a direct route");

    assert!(error.contains("password is not saved"));
    let _ = std::fs::remove_file(path);
}

struct EnvVarGuard {
    key: &'static str,
    previous: Option<String>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var(key).ok();
        // These resolver tests run in-process and temporarily control proxy
        // environment variables to verify fallback precedence.
        unsafe {
            std::env::set_var(key, value);
        }
        Self { key, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        // Restore the caller's environment after the focused resolver test.
        unsafe {
            match &self.previous {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }
}

#[test]
fn rdp_proxy_policy_respects_gateway_and_rejects_http() {
    use crate::rdp_socks_proxy_from_saved_policy;
    let (store, path) = temp_connection_store("rdp-proxy");
    let mut settings = PersistedSettings::default();
    settings.network.upstream_proxy = Some(SettingsUpstreamProxyConfig {
        protocol: SettingsUpstreamProxyProtocol::HttpConnect,
        host: "global.test".into(),
        port: 8080,
        auth: SettingsUpstreamProxyAuth::None,
        remote_dns: true,
        no_proxy: String::new(),
    });
    assert!(
        rdp_socks_proxy_from_saved_policy(
            &store,
            &settings,
            &SavedUpstreamProxyPolicy::Direct,
            false
        )
        .unwrap()
        .is_none()
    );
    assert!(
        rdp_socks_proxy_from_saved_policy(
            &store,
            &settings,
            &SavedUpstreamProxyPolicy::UseGlobal,
            false
        )
        .is_err()
    );
    let mut proxy = SavedUpstreamProxyConfig {
        protocol: SavedUpstreamProxyProtocol::Socks5,
        host: "proxy.test".into(),
        port: 1080,
        remote_dns: true,
        no_proxy: "*.internal".into(),
        auth: SavedUpstreamProxyAuth::Password {
            username: "proxy-user".into(),
            keychain_id: None,
            plaintext_password: None,
        },
    };
    let missing_password = SavedUpstreamProxyPolicy::Custom {
        proxy: proxy.clone(),
    };
    assert!(
        rdp_socks_proxy_from_saved_policy(&store, &settings, &missing_password, true)
            .unwrap()
            .is_none()
    );
    assert!(
        rdp_socks_proxy_from_saved_policy(&store, &settings, &missing_password, false).is_err()
    );
    proxy.auth = SavedUpstreamProxyAuth::Password {
        username: "proxy-user".into(),
        keychain_id: None,
        plaintext_password: Some(SecretString::from("proxy-secret")),
    };
    let runtime = rdp_socks_proxy_from_saved_policy(
        &store,
        &settings,
        &SavedUpstreamProxyPolicy::Custom { proxy },
        false,
    )
    .unwrap()
    .unwrap();
    assert_eq!(runtime.host, "proxy.test");
    assert_eq!(runtime.port, 1080);
    assert_eq!(runtime.no_proxy, "*.internal");
    assert!(runtime.remote_dns);
    assert_eq!(
        runtime.auth.as_ref().unwrap().password.expose_secret(),
        "proxy-secret"
    );
    assert!(!format!("{runtime:?}").contains("proxy-secret"));
    let _ = std::fs::remove_file(path);
}
