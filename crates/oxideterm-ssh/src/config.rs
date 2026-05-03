// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    #[serde(default)]
    pub strict_host_key_checking: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trust_host_key: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_host_key_fingerprint: Option<String>,
    #[serde(default)]
    pub agent_forwarding: bool,
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
        format!(
            "{}@{}:{}|{}",
            self.username, self.host, self.port, proxy_key
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyHopConfig {
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub username: String,
    pub auth: AuthMethod,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    Certificate {
        key_path: String,
        cert_path: String,
        passphrase: Option<Zeroizing<String>>,
    },
    KeyboardInteractive,
}

impl AuthMethod {
    pub fn password(password: impl Into<String>) -> Self {
        Self::Password {
            password: Zeroizing::new(password.into()),
        }
    }

    pub fn key(key_path: impl Into<String>, passphrase: Option<String>) -> Self {
        Self::Key {
            key_path: key_path.into(),
            passphrase: passphrase.map(Zeroizing::new),
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
            strict_host_key_checking: false,
            trust_host_key: None,
            expected_host_key_fingerprint: None,
            agent_forwarding: false,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_stable_connection_key() {
        let config = SshConfig::password("192.168.1.10", 22, "root", "pw");
        assert_eq!(config.connection_key(), "root@192.168.1.10:22|");
    }
}
