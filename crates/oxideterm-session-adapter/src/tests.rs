// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use chrono::Utc;
use oxideterm_connections::{
    ConnectionOptions, ConnectionStore, SavedAuth, SavedConnection, SavedProxyHop,
    SavedUpstreamProxyAuth, SavedUpstreamProxyConfig, SavedUpstreamProxyPolicy,
    SavedUpstreamProxyProtocol, SecretString,
};
use oxideterm_settings::{
    PersistedSettings, SettingsUpstreamProxyAuth, SettingsUpstreamProxyConfig,
    SettingsUpstreamProxyProtocol,
};
use oxideterm_ssh::{AuthMethod, ProxyCommandConfig, UpstreamProxyAuth};

use crate::ssh::proxy_command_runtime_policy;
use crate::{
    reconnect_max_attempts_from_settings, reconnect_timing_from_settings,
    sftp_runtime_settings_from_settings, terminal_encoding_from_settings,
};
use crate::{
    ssh_config_for_saved_connection_hop, ssh_config_from_saved_connection,
    upstream_proxy_config_from_saved_policy,
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
        host: "target.example.com".to_string(),
        port: 22,
        username: "me".to_string(),
        auth,
        proxy_chain: Vec::new(),
        upstream_proxy: SavedUpstreamProxyPolicy::UseGlobal,
        options: ConnectionOptions::default(),
        created_at: now,
        last_used_at: None,
        updated_at: Some(now),
        color: None,
        icon: None,
        tags: Vec::new(),
        post_connect_command: None,
        privilege_credentials: Vec::new(),
    }
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
fn saved_proxy_chain_becomes_ssh_config_chain() {
    let (store, path) = temp_connection_store("proxy-chain");
    let mut conn = saved_connection(SavedAuth::Agent);
    conn.proxy_chain = vec![SavedProxyHop {
        host: "jump.example.com".to_string(),
        port: 2222,
        username: "ops".to_string(),
        auth: SavedAuth::Agent,
        agent_forwarding: true,
        legacy_ssh_compatibility: true,
    }];

    let settings = PersistedSettings::default();
    let config = ssh_config_from_saved_connection(&store, &settings, &conn).unwrap();

    assert!(config.strict_host_key_checking);
    let chain = config.proxy_chain.unwrap();
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].host, "jump.example.com");
    assert_eq!(chain[0].port, 2222);
    assert_eq!(chain[0].username, "ops");
    assert!(chain[0].agent_forwarding);
    assert!(chain[0].legacy_ssh_compatibility);
    let _ = std::fs::remove_file(path);
}

#[test]
fn saved_connection_hops_become_independent_runtime_configs() {
    let (store, path) = temp_connection_store("materialized-hops");
    let mut connection = saved_connection(SavedAuth::Agent);
    connection.proxy_chain = vec![SavedProxyHop {
        host: "jump.example.com".to_string(),
        port: 2222,
        username: "ops".to_string(),
        auth: SavedAuth::Agent,
        agent_forwarding: true,
        legacy_ssh_compatibility: true,
    }];
    let settings = PersistedSettings::default();

    let jump = ssh_config_for_saved_connection_hop(&store, &settings, &connection, 0)
        .expect("saved jump should become a runtime config");
    let target = ssh_config_for_saved_connection_hop(&store, &settings, &connection, 1)
        .expect("target should follow the materialized jumps");

    assert_eq!(jump.host, "jump.example.com");
    assert!(jump.proxy_chain.is_none());
    assert_eq!(target.host, "target.example.com");
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
fn custom_upstream_proxy_hydrates_plaintext_secret_without_keychain() {
    let (store, path) = temp_connection_store("custom-proxy");
    let settings = PersistedSettings::default();
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
    let _ = std::fs::remove_file(path);
}

#[test]
fn direct_upstream_proxy_policy_ignores_global_proxy() {
    let (store, path) = temp_connection_store("direct-proxy");
    let mut settings = PersistedSettings::default();
    settings.network.upstream_proxy = Some(SettingsUpstreamProxyConfig {
        protocol: SettingsUpstreamProxyProtocol::Socks5,
        host: "global-proxy.local".to_string(),
        port: 1080,
        auth: SettingsUpstreamProxyAuth::None,
        remote_dns: true,
        no_proxy: String::new(),
    });
    let policy = SavedUpstreamProxyPolicy::Direct;

    assert!(
        upstream_proxy_config_from_saved_policy(&store, &settings, &policy)
            .unwrap()
            .is_none()
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn use_global_upstream_proxy_prefers_global_settings_over_env_fallback() {
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
    let _ = std::fs::remove_file(path);
}

#[test]
fn use_global_upstream_proxy_fails_when_saved_password_is_missing() {
    let (store, path) = temp_connection_store("missing-global-proxy-password");
    let mut settings = PersistedSettings::default();
    settings.network.upstream_proxy = Some(SettingsUpstreamProxyConfig {
        protocol: SettingsUpstreamProxyProtocol::HttpConnect,
        host: "global-proxy.local".to_string(),
        port: 8080,
        auth: SettingsUpstreamProxyAuth::Password {
            username: "proxy-user".to_string(),
            keychain_id: None,
        },
        remote_dns: true,
        no_proxy: String::new(),
    });

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
