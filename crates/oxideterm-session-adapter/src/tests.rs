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
use oxideterm_ssh::{AuthMethod, UpstreamProxyAuth};

use crate::{ssh_config_from_saved_connection, upstream_proxy_config_from_saved_policy};

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

    let proxy = upstream_proxy_config_from_saved_policy(&store, &settings, &policy).unwrap();

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

    assert!(upstream_proxy_config_from_saved_policy(&store, &settings, &policy).is_none());
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

    let proxy = upstream_proxy_config_from_saved_policy(&store, &settings, &policy).unwrap();

    assert_eq!(proxy.host, "global-proxy.local");
    assert!(matches!(proxy.auth, UpstreamProxyAuth::None));
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
