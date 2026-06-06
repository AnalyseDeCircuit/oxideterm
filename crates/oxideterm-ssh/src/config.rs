// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::fmt;

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::upstream_proxy::UpstreamProxyConfig;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshConfig {
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub username: String,
    pub auth: AuthMethod,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default = "default_cols")]
    pub cols: u32,
    #[serde(default = "default_rows")]
    pub rows: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_chain: Option<Vec<ProxyHopConfig>>,
    #[serde(default, skip)]
    pub upstream_proxy: Option<UpstreamProxyConfig>,
    #[serde(default)]
    pub strict_host_key_checking: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trust_host_key: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_host_key_fingerprint: Option<String>,
    #[serde(default)]
    pub agent_forwarding: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_connect_command: Option<String>,
}

impl SshConfig {
    pub fn password(
        host: impl Into<String>,
        port: u16,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        Self {
            host: host.into(),
            port,
            username: username.into(),
            auth: AuthMethod::password(password),
            ..Self::default()
        }
    }

    pub fn connection_key(&self) -> String {
        let proxy_key = self.proxy_chain.as_ref().map_or_else(String::new, |chain| {
            chain
                .iter()
                .map(|hop| format!("{}@{}:{}", hop.username, hop.host, hop.port))
                .collect::<Vec<_>>()
                .join(">")
        });
        let upstream_proxy_key = self
            .upstream_proxy
            .as_ref()
            .map_or_else(String::new, |proxy| {
                format!(
                    "|upstream={:?}:{}:{}:{}",
                    proxy.protocol, proxy.host, proxy.port, proxy.remote_dns
                )
            });
        format!(
            "{}@{}:{}|{}{}",
            self.username, self.host, self.port, proxy_key, upstream_proxy_key
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyHopConfig {
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub username: String,
    pub auth: AuthMethod,
    #[serde(default)]
    pub agent_forwarding: bool,
    #[serde(default = "default_proxy_strict_host_key_checking")]
    pub strict_host_key_checking: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trust_host_key: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_host_key_fingerprint: Option<String>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthMethod {
    Password {
        password: Zeroizing<String>,
    },
    Key {
        key_path: String,
        passphrase: Option<Zeroizing<String>>,
    },
    Agent,
    ManagedKey {
        key_id: String,
        passphrase: Option<Zeroizing<String>>,
    },
    Certificate {
        key_path: String,
        cert_path: String,
        passphrase: Option<Zeroizing<String>>,
    },
    KeyboardInteractive,
}

impl fmt::Debug for AuthMethod {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Password { .. } => formatter
                .debug_struct("Password")
                .field("password", &"[redacted secret]")
                .finish(),
            Self::Key {
                key_path,
                passphrase,
            } => formatter
                .debug_struct("Key")
                .field("key_path", key_path)
                .field(
                    "passphrase",
                    &passphrase.as_ref().map(|_| "[redacted secret]"),
                )
                .finish(),
            Self::Agent => formatter.write_str("Agent"),
            Self::ManagedKey { key_id, passphrase } => formatter
                .debug_struct("ManagedKey")
                .field("key_id", key_id)
                .field(
                    "passphrase",
                    &passphrase.as_ref().map(|_| "[redacted secret]"),
                )
                .finish(),
            Self::Certificate {
                key_path,
                cert_path,
                passphrase,
            } => formatter
                .debug_struct("Certificate")
                .field("key_path", key_path)
                .field("cert_path", cert_path)
                .field(
                    "passphrase",
                    &passphrase.as_ref().map(|_| "[redacted secret]"),
                )
                .finish(),
            Self::KeyboardInteractive => formatter.write_str("KeyboardInteractive"),
        }
    }
}

impl AuthMethod {
    pub fn password(password: impl Into<String>) -> Self {
        Self::Password {
            password: Zeroizing::new(password.into()),
        }
    }

    pub fn password_secret(password: Zeroizing<String>) -> Self {
        Self::Password { password }
    }

    pub fn key(key_path: impl Into<String>, passphrase: Option<String>) -> Self {
        Self::Key {
            key_path: key_path.into(),
            passphrase: passphrase.map(Zeroizing::new),
        }
    }

    pub fn key_secret(key_path: impl Into<String>, passphrase: Option<Zeroizing<String>>) -> Self {
        Self::Key {
            key_path: key_path.into(),
            passphrase,
        }
    }

    pub fn managed_key(key_id: impl Into<String>, passphrase: Option<String>) -> Self {
        Self::ManagedKey {
            key_id: key_id.into(),
            passphrase: passphrase.map(Zeroizing::new),
        }
    }

    pub fn managed_key_secret(
        key_id: impl Into<String>,
        passphrase: Option<Zeroizing<String>>,
    ) -> Self {
        Self::ManagedKey {
            key_id: key_id.into(),
            passphrase,
        }
    }

    pub fn certificate(
        key_path: impl Into<String>,
        cert_path: impl Into<String>,
        passphrase: Option<String>,
    ) -> Self {
        Self::Certificate {
            key_path: key_path.into(),
            cert_path: cert_path.into(),
            passphrase: passphrase.map(Zeroizing::new),
        }
    }

    pub fn certificate_secret(
        key_path: impl Into<String>,
        cert_path: impl Into<String>,
        passphrase: Option<Zeroizing<String>>,
    ) -> Self {
        Self::Certificate {
            key_path: key_path.into(),
            cert_path: cert_path.into(),
            passphrase,
        }
    }
}

impl Default for SshConfig {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: default_port(),
            username: String::new(),
            auth: AuthMethod::password(""),
            timeout_secs: default_timeout(),
            cols: default_cols(),
            rows: default_rows(),
            proxy_chain: None,
            upstream_proxy: None,
            strict_host_key_checking: false,
            trust_host_key: None,
            expected_host_key_fingerprint: None,
            agent_forwarding: false,
            post_connect_command: None,
        }
    }
}

const fn default_port() -> u16 {
    22
}

const fn default_timeout() -> u64 {
    30
}

const fn default_cols() -> u32 {
    80
}

const fn default_rows() -> u32 {
    24
}

const fn default_proxy_strict_host_key_checking() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_stable_connection_key() {
        let config = SshConfig::password("192.168.1.10", 22, "root", "pw");
        assert_eq!(config.connection_key(), "root@192.168.1.10:22|");
    }

    #[test]
    fn connection_key_includes_proxy_chain_order() {
        let mut config = SshConfig::password("target", 22, "app", "pw");
        config.proxy_chain = Some(vec![
            ProxyHopConfig {
                host: "jump-a".to_string(),
                port: 2222,
                username: "ops".to_string(),
                auth: AuthMethod::Agent,
                agent_forwarding: false,
                strict_host_key_checking: true,
                trust_host_key: None,
                expected_host_key_fingerprint: None,
            },
            ProxyHopConfig {
                host: "jump-b".to_string(),
                port: 22,
                username: "root".to_string(),
                auth: AuthMethod::Agent,
                agent_forwarding: true,
                strict_host_key_checking: true,
                trust_host_key: None,
                expected_host_key_fingerprint: None,
            },
        ]);

        assert_eq!(
            config.connection_key(),
            "app@target:22|ops@jump-a:2222>root@jump-b:22"
        );
    }

    #[test]
    fn proxy_hop_default_matches_tauri_non_strict_proxy_default() {
        assert!(!default_proxy_strict_host_key_checking());
    }
}
