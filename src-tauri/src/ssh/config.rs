// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! SSH Configuration

use serde::{Deserialize, Serialize};

/// SSH connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshConfig {
    /// Remote host address
    pub host: String,

    /// SSH port (default: 22)
    #[serde(default = "default_port")]
    pub port: u16,

    /// Username for authentication
    pub username: String,

    /// Authentication method
    pub auth: AuthMethod,

    /// Connection timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,

    /// Terminal columns
    #[serde(default = "default_cols")]
    pub cols: u32,

    /// Terminal rows
    #[serde(default = "default_rows")]
    pub rows: u32,

    /// Optional proxy chain for jump hosts (ProxyJump)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_chain: Option<Vec<ProxyHopConfig>>,

    /// Strict host key checking (default: false for user-friendly behavior)
    /// - true: reject connections to unknown hosts
    /// - false: auto-accept unknown hosts, still reject changed keys
    #[serde(default)]
    pub strict_host_key_checking: bool,

    /// Trust host key mode for TOFU (Trust On First Use)
    /// - None: use strict_host_key_checking behavior
    /// - Some(true): trust and save unknown keys to known_hosts
    /// - Some(false): trust for this session only (don't save)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trust_host_key: Option<bool>,
}

/// Configuration for a single proxy hop
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyHopConfig {
    /// Jump host address
    pub host: String,

    /// Jump host port (default: 22)
    #[serde(default = "default_port")]
    pub port: u16,

    /// Username for the jump host
    pub username: String,

    /// Authentication method for the jump host
    pub auth: AuthMethod,
}

/// Authentication methods supported
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthMethod {
    /// Password authentication
    Password { password: String },

    /// SSH key authentication
    Key {
        /// Path to private key file
        key_path: String,
        /// Optional passphrase for encrypted keys
        passphrase: Option<String>,
    },

    /// SSH agent authentication
    Agent,

    /// SSH certificate authentication (OpenSSH certificates)
    Certificate {
        /// Path to private key file
        key_path: String,
        /// Path to certificate file (*-cert.pub)
        cert_path: String,
        /// Optional passphrase for encrypted keys
        passphrase: Option<String>,
    },

    /// Keyboard-Interactive authentication (2FA/TOTP)
    /// Note: This requires frontend interaction during authentication
    KeyboardInteractive,
}

impl AuthMethod {
    pub fn password(password: impl Into<String>) -> Self {
        Self::Password {
            password: password.into(),
        }
    }

    pub fn key(key_path: impl Into<String>, passphrase: Option<String>) -> Self {
        Self::Key {
            key_path: key_path.into(),
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
            passphrase,
        }
    }
}

fn default_port() -> u16 {
    22
}

fn default_timeout() -> u64 {
    30
}

fn default_cols() -> u32 {
    80
}

fn default_rows() -> u32 {
    24
}

impl Default for SshConfig {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: 22,
            username: String::new(),
            auth: AuthMethod::Password {
                password: String::new(),
            },
            timeout_secs: 30,
            cols: 80,
            rows: 24,
            proxy_chain: None,
            strict_host_key_checking: false,
            trust_host_key: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_method_helper_builders() {
        assert!(matches!(
            AuthMethod::password("secret"),
            AuthMethod::Password { password } if password == "secret"
        ));
        assert!(matches!(
            AuthMethod::key("/tmp/id_ed25519", Some("pp".to_string())),
            AuthMethod::Key { key_path, passphrase }
                if key_path == "/tmp/id_ed25519" && passphrase.as_deref() == Some("pp")
        ));
        assert!(matches!(
            AuthMethod::certificate("/tmp/id_ed25519", "/tmp/id_ed25519-cert.pub", None),
            AuthMethod::Certificate { key_path, cert_path, passphrase }
                if key_path == "/tmp/id_ed25519"
                    && cert_path == "/tmp/id_ed25519-cert.pub"
                    && passphrase.is_none()
        ));
    }

    #[test]
    fn test_auth_method_serde_round_trips_all_variants() {
        let variants = vec![
            AuthMethod::Password {
                password: "secret".to_string(),
            },
            AuthMethod::Key {
                key_path: "/tmp/id_ed25519".to_string(),
                passphrase: Some("pp".to_string()),
            },
            AuthMethod::Agent,
            AuthMethod::Certificate {
                key_path: "/tmp/id_ed25519".to_string(),
                cert_path: "/tmp/id_ed25519-cert.pub".to_string(),
                passphrase: None,
            },
            AuthMethod::KeyboardInteractive,
        ];

        for variant in variants {
            let json = serde_json::to_string(&variant).unwrap();
            let round_trip: AuthMethod = serde_json::from_str(&json).unwrap();

            match (variant, round_trip) {
                (
                    AuthMethod::Password { password: left },
                    AuthMethod::Password { password: right },
                ) => {
                    assert_eq!(left, right);
                }
                (
                    AuthMethod::Key {
                        key_path: left_key,
                        passphrase: left_pass,
                    },
                    AuthMethod::Key {
                        key_path: right_key,
                        passphrase: right_pass,
                    },
                ) => {
                    assert_eq!(left_key, right_key);
                    assert_eq!(left_pass, right_pass);
                }
                (AuthMethod::Agent, AuthMethod::Agent) => {}
                (
                    AuthMethod::Certificate {
                        key_path: left_key,
                        cert_path: left_cert,
                        passphrase: left_pass,
                    },
                    AuthMethod::Certificate {
                        key_path: right_key,
                        cert_path: right_cert,
                        passphrase: right_pass,
                    },
                ) => {
                    assert_eq!(left_key, right_key);
                    assert_eq!(left_cert, right_cert);
                    assert_eq!(left_pass, right_pass);
                }
                (AuthMethod::KeyboardInteractive, AuthMethod::KeyboardInteractive) => {}
                other => panic!("unexpected round trip mismatch: {:?}", other),
            }
        }
    }

    #[test]
    fn test_ssh_config_default_values_are_stable() {
        let config = SshConfig::default();

        assert_eq!(config.port, 22);
        assert_eq!(config.timeout_secs, 30);
        assert_eq!(config.cols, 80);
        assert_eq!(config.rows, 24);
        assert!(!config.strict_host_key_checking);
        assert!(config.proxy_chain.is_none());
        assert!(config.trust_host_key.is_none());
    }

    #[test]
    fn test_proxy_hop_config_round_trips_with_agent_auth() {
        let hop = ProxyHopConfig {
            host: "jump.example.com".to_string(),
            port: 2222,
            username: "alice".to_string(),
            auth: AuthMethod::Agent,
        };

        let json = serde_json::to_string(&hop).unwrap();
        let round_trip: ProxyHopConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(round_trip.host, "jump.example.com");
        assert_eq!(round_trip.port, 2222);
        assert_eq!(round_trip.username, "alice");
        assert!(matches!(round_trip.auth, AuthMethod::Agent));
    }
}
