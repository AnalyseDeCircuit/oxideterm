// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::fmt;

use oxideterm_connections::{SshAlgorithmPreferences, SshChannelStrategy};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::{
    agent_endpoint::{
        ssh_agent_endpoint_pool_identity, ssh_agent_forwarding_endpoint_pool_identity,
    },
    upstream_proxy::UpstreamProxyConfig,
};
use oxideterm_x11_forwarding::X11ForwardPolicy;

fn agent_endpoint_key_suffix(label: &str, endpoint: Option<&str>) -> String {
    let endpoint = ssh_agent_endpoint_pool_identity(endpoint);
    // Agent endpoint paths can reveal local account layout, so pool identity
    // retains only a one-way digest of the configured selector.
    let digest = Sha256::digest(endpoint.as_bytes());
    format!(":{label}={digest:x}")
}

fn agent_forwarding_endpoint_key_suffix(
    label: &str,
    forwarding_endpoint: Option<&str>,
    identity_endpoint: Option<&str>,
) -> String {
    let endpoint = forwarding_endpoint.map_or_else(
        || ssh_agent_endpoint_pool_identity(identity_endpoint),
        |endpoint| ssh_agent_forwarding_endpoint_pool_identity(Some(endpoint)),
    );
    // ForwardAgent endpoint paths receive the same redaction as IdentityAgent
    // while preserving their distinct OpenSSH option semantics.
    let digest = Sha256::digest(endpoint.as_bytes());
    format!(":{label}={digest:x}")
}

fn authentication_key_suffix(auth: &AuthMethod) -> String {
    let (server_identity, delegate_credentials) = match auth {
        AuthMethod::KerberosPreferred {
            server_identity,
            delegate_credentials,
            ..
        } => (server_identity, delegate_credentials),
        _ => return String::new(),
    };
    let identity = server_identity.as_deref().unwrap_or_default();
    // The configured service identity can reveal internal host naming, so the
    // registry key retains only a digest while still separating GSS contexts.
    let digest = Sha256::digest(identity.as_bytes());
    format!(":gssapi={digest:x}:delegate={delegate_credentials}")
}

fn algorithm_preferences_key_suffix(preferences: &SshAlgorithmPreferences) -> String {
    if preferences.is_default() {
        return String::new();
    }
    let mut hasher = Sha256::new();
    for (category, algorithms) in [
        (b'k', preferences.kex.as_slice()),
        (b'h', preferences.host_key.as_slice()),
        (b'c', preferences.cipher.as_slice()),
        (b'm', preferences.mac.as_slice()),
        (b'z', preferences.compression.as_slice()),
    ] {
        hasher.update([category]);
        for algorithm in algorithms {
            hasher.update(algorithm.as_bytes());
            hasher.update([0]);
        }
    }
    // Pool identity must separate physical connections with different offers,
    // while keeping arbitrary persisted names out of registry keys.
    format!(":algorithms={:x}", hasher.finalize())
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    #[serde(default, skip)]
    pub proxy_command: Option<ProxyCommandConfig>,
    #[serde(default)]
    pub strict_host_key_checking: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trust_host_key: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_host_key_fingerprint: Option<String>,
    #[serde(default)]
    pub agent_forwarding: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_forwarding_socket: Option<String>,
    #[serde(default)]
    pub legacy_ssh_compatibility: bool,
    #[serde(default)]
    pub ssh_channel_strategy: SshChannelStrategy,
    #[serde(default, skip_serializing_if = "SshAlgorithmPreferences::is_default")]
    pub ssh_algorithms: SshAlgorithmPreferences,
    /// X11 stores only non-secret policy; DISPLAY and cookies are resolved per shell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x11_forwarding: Option<X11ForwardPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_connect_command: Option<String>,
}

impl fmt::Debug for SshConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SshConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("auth", &self.auth)
            .field("timeout_secs", &self.timeout_secs)
            .field("cols", &self.cols)
            .field("rows", &self.rows)
            .field("proxy_chain", &self.proxy_chain)
            .field("upstream_proxy", &self.upstream_proxy)
            .field("proxy_command", &self.proxy_command)
            .field("strict_host_key_checking", &self.strict_host_key_checking)
            .field("trust_host_key", &self.trust_host_key)
            .field(
                "expected_host_key_fingerprint",
                &self.expected_host_key_fingerprint,
            )
            .field("agent_forwarding", &self.agent_forwarding)
            .field("identity_agent_configured", &self.identity_agent.is_some())
            .field(
                "agent_forwarding_socket_configured",
                &self.agent_forwarding_socket.is_some(),
            )
            .field("legacy_ssh_compatibility", &self.legacy_ssh_compatibility)
            .field("ssh_channel_strategy", &self.ssh_channel_strategy)
            .field(
                "ssh_algorithm_categories_customized",
                &[
                    !self.ssh_algorithms.kex.is_empty(),
                    !self.ssh_algorithms.host_key.is_empty(),
                    !self.ssh_algorithms.cipher.is_empty(),
                    !self.ssh_algorithms.mac.is_empty(),
                    !self.ssh_algorithms.compression.is_empty(),
                ]
                .into_iter()
                .filter(|customized| *customized)
                .count(),
            )
            .field("x11_forwarding", &self.x11_forwarding)
            .field(
                "post_connect_command_configured",
                &self.post_connect_command.is_some(),
            )
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum ProxyCommandConfig {
    AuthorizationRequired,
    Unavailable,
    Direct {
        program: Zeroizing<String>,
        args: Vec<Zeroizing<String>>,
    },
}

impl ProxyCommandConfig {
    pub fn direct(words: Vec<Zeroizing<String>>) -> Option<Self> {
        let mut words = words.into_iter();
        let program = words.next()?;
        Some(Self::Direct {
            program,
            args: words.collect(),
        })
    }

    fn connection_key_suffix(&self) -> String {
        match self {
            Self::AuthorizationRequired => "|proxy-command=authorization-required".to_string(),
            Self::Unavailable => "|proxy-command=unavailable".to_string(),
            Self::Direct { program, args } => {
                // Pool identity uses a one-way digest so command text and embedded tokens
                // never enter registry keys, diagnostics, or logs.
                let mut hasher = Sha256::new();
                hasher.update(program.as_bytes());
                for argument in args {
                    hasher.update([0]);
                    hasher.update(argument.as_bytes());
                }
                format!("|proxy-command={:x}", hasher.finalize())
            }
        }
    }
}

impl fmt::Debug for ProxyCommandConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthorizationRequired => formatter.write_str("AuthorizationRequired"),
            Self::Unavailable => formatter.write_str("Unavailable"),
            Self::Direct { args, .. } => formatter
                .debug_struct("Direct")
                .field("program", &"[redacted secret]")
                .field("argument_count", &args.len())
                .finish(),
        }
    }
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
                .map(|hop| {
                    let legacy_suffix = if hop.legacy_ssh_compatibility {
                        ":legacy"
                    } else {
                        ""
                    };
                    let agent_forwarding_suffix = if hop.agent_forwarding {
                        ":agent-forwarding"
                    } else {
                        ""
                    };
                    let identity_agent_suffix = hop
                        .identity_agent
                        .as_deref()
                        .map_or_else(String::new, |endpoint| {
                            agent_endpoint_key_suffix("identity-agent", Some(endpoint))
                        });
                    let forwarding_socket_suffix = if hop.agent_forwarding {
                        agent_forwarding_endpoint_key_suffix(
                            "forwarding-agent",
                            hop.agent_forwarding_socket.as_deref(),
                            hop.identity_agent.as_deref(),
                        )
                    } else {
                        String::new()
                    };
                    let authentication_suffix = authentication_key_suffix(&hop.auth);
                    let algorithm_suffix = algorithm_preferences_key_suffix(&hop.ssh_algorithms);
                    format!(
                        "{}@{}:{}{}{}{}{}{}{}",
                        hop.username,
                        hop.host,
                        hop.port,
                        legacy_suffix,
                        agent_forwarding_suffix,
                        identity_agent_suffix,
                        forwarding_socket_suffix,
                        authentication_suffix,
                        algorithm_suffix
                    )
                })
                .collect::<Vec<_>>()
                .join(">")
        });
        let legacy_key = if self.legacy_ssh_compatibility {
            "|legacy_ssh=true"
        } else {
            ""
        };
        let agent_forwarding_key = if self.agent_forwarding {
            "|agent_forwarding=true"
        } else {
            ""
        };
        let identity_agent_key = self
            .identity_agent
            .as_deref()
            .map_or_else(String::new, |endpoint| {
                agent_endpoint_key_suffix("identity_agent", Some(endpoint))
            });
        let agent_forwarding_socket_key = if self.agent_forwarding {
            agent_forwarding_endpoint_key_suffix(
                "agent_forwarding_socket",
                self.agent_forwarding_socket.as_deref(),
                self.identity_agent.as_deref(),
            )
        } else {
            String::new()
        };
        let upstream_proxy_key = self
            .upstream_proxy
            .as_ref()
            .map_or_else(String::new, |proxy| {
                format!(
                    "|upstream={:?}:{}:{}:{}",
                    proxy.protocol, proxy.host, proxy.port, proxy.remote_dns
                )
            });
        let proxy_command_key = self
            .proxy_command
            .as_ref()
            .map_or_else(String::new, ProxyCommandConfig::connection_key_suffix);
        let authentication_key = authentication_key_suffix(&self.auth);
        let algorithm_key = algorithm_preferences_key_suffix(&self.ssh_algorithms);
        let channel_strategy_key = if self.ssh_channel_strategy.requires_dedicated_consumers() {
            "|channel_strategy=dedicated_per_consumer"
        } else {
            ""
        };
        format!(
            "{}@{}:{}|{}{}{}{}{}{}{}{}{}{}",
            self.username,
            self.host,
            self.port,
            proxy_key,
            upstream_proxy_key,
            legacy_key,
            agent_forwarding_key,
            identity_agent_key,
            agent_forwarding_socket_key,
            proxy_command_key,
            authentication_key,
            algorithm_key,
            channel_strategy_key
        )
    }

    /// Runtime authentication material must never enter plain persisted snapshots.
    pub fn has_runtime_auth_secret(&self) -> bool {
        self.auth.has_runtime_secret()
            || self
                .proxy_chain
                .as_ref()
                .is_some_and(|chain| chain.iter().any(|hop| hop.auth.has_runtime_secret()))
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_forwarding_socket: Option<String>,
    #[serde(default)]
    pub legacy_ssh_compatibility: bool,
    #[serde(default, skip_serializing_if = "SshAlgorithmPreferences::is_default")]
    pub ssh_algorithms: SshAlgorithmPreferences,
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
    KerberosPreferred {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        server_identity: Option<String>,
        #[serde(default)]
        delegate_credentials: bool,
        fallback: Box<AuthMethod>,
    },
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
            Self::KerberosPreferred {
                server_identity,
                delegate_credentials,
                fallback,
            } => formatter
                .debug_struct("KerberosPreferred")
                .field("server_identity_configured", &server_identity.is_some())
                .field("delegate_credentials", delegate_credentials)
                .field("fallback", fallback)
                .finish(),
        }
    }
}

impl AuthMethod {
    /// Passwords and supplied passphrases are retained only for the active connection attempt.
    pub fn has_runtime_secret(&self) -> bool {
        match self {
            Self::Password { .. } => true,
            Self::Key { passphrase, .. }
            | Self::ManagedKey { passphrase, .. }
            | Self::Certificate { passphrase, .. } => passphrase.is_some(),
            Self::KerberosPreferred { fallback, .. } => fallback.has_runtime_secret(),
            Self::Agent | Self::KeyboardInteractive => false,
        }
    }

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

    pub fn kerberos_preferred(
        fallback: AuthMethod,
        server_identity: Option<String>,
        delegate_credentials: bool,
    ) -> Self {
        Self::KerberosPreferred {
            server_identity,
            delegate_credentials,
            fallback: Box::new(match fallback {
                Self::KerberosPreferred { fallback, .. } => *fallback,
                fallback => fallback,
            }),
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
            proxy_command: None,
            strict_host_key_checking: false,
            trust_host_key: None,
            expected_host_key_fingerprint: None,
            agent_forwarding: false,
            identity_agent: None,
            agent_forwarding_socket: None,
            legacy_ssh_compatibility: false,
            ssh_channel_strategy: SshChannelStrategy::default(),
            ssh_algorithms: SshAlgorithmPreferences::default(),
            x11_forwarding: None,
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
    fn algorithm_preferences_separate_physical_connection_pool_identity() {
        let mut config = SshConfig::password("host", 22, "operator", "pw");
        let default_key = config.connection_key();
        config.ssh_algorithms.cipher = vec!["aes256-gcm@openssh.com".to_string()];

        assert_ne!(default_key, config.connection_key());
    }

    #[test]
    fn channel_strategy_separates_physical_connection_pool_identity() {
        let mut config = SshConfig::password("host", 22, "operator", "pw");
        let multiplexed_key = config.connection_key();
        config.ssh_channel_strategy = SshChannelStrategy::DedicatedPerConsumer;

        assert_ne!(multiplexed_key, config.connection_key());
        assert!(
            config
                .connection_key()
                .contains("channel_strategy=dedicated_per_consumer")
        );
    }

    #[test]
    fn x11_policy_does_not_split_physical_connection_pool_identity() {
        let mut config = SshConfig::password("192.168.1.10", 22, "root", "pw");
        let without_x11 = config.connection_key();
        config.x11_forwarding = Some(X11ForwardPolicy::untrusted().with_timeout_millis(1_200_000));

        assert_eq!(config.connection_key(), without_x11);
        assert!(!format!("{config:?}").contains("MIT-MAGIC-COOKIE-1"));
    }

    #[test]
    fn proxy_command_is_redacted_and_only_a_digest_enters_the_pool_key() {
        let mut config = SshConfig::password("target", 22, "operator", "pw");
        config.proxy_command = ProxyCommandConfig::direct(vec![
            Zeroizing::new("helper-with-token".to_string()),
            Zeroizing::new("credential-value".to_string()),
        ]);

        let debug = format!("{config:?}");
        let pool_key = config.connection_key();

        assert!(!debug.contains("helper-with-token"));
        assert!(!debug.contains("credential-value"));
        assert!(!pool_key.contains("helper-with-token"));
        assert!(!pool_key.contains("credential-value"));
        assert!(pool_key.contains("proxy-command="));
    }

    #[test]
    fn gssapi_policy_separates_pool_identity_without_exposing_server_name() {
        let server_name = "kerberos.internal.example";
        let mut config = SshConfig {
            host: "target".to_string(),
            username: "operator".to_string(),
            auth: AuthMethod::kerberos_preferred(
                AuthMethod::Agent,
                Some(server_name.to_string()),
                false,
            ),
            ..SshConfig::default()
        };
        let non_delegated = config.connection_key();
        config.auth =
            AuthMethod::kerberos_preferred(AuthMethod::Agent, Some(server_name.to_string()), true);
        let delegated = config.connection_key();

        assert_ne!(non_delegated, delegated);
        assert!(!non_delegated.contains(server_name));
        assert!(!delegated.contains(server_name));
    }

    #[test]
    fn post_connect_command_is_redacted_from_runtime_debug_output() {
        let mut config = SshConfig::password("target", 22, "operator", "pw");
        config.post_connect_command = Some("command-with-private-token".to_string());

        let debug = format!("{config:?}");

        assert!(!debug.contains("command-with-private-token"));
        assert!(debug.contains("post_connect_command_configured: true"));
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
                identity_agent: None,
                agent_forwarding_socket: None,
                legacy_ssh_compatibility: false,
                ssh_algorithms: SshAlgorithmPreferences::default(),
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
                identity_agent: None,
                agent_forwarding_socket: None,
                legacy_ssh_compatibility: true,
                ssh_algorithms: SshAlgorithmPreferences::default(),
                strict_host_key_checking: true,
                trust_host_key: None,
                expected_host_key_fingerprint: None,
            },
        ]);

        assert!(config.connection_key().starts_with(
            "app@target:22|ops@jump-a:2222>root@jump-b:22:legacy:agent-forwarding:forwarding-agent="
        ));
    }

    #[test]
    fn connection_key_separates_target_and_proxy_agent_forwarding_policy() {
        let mut config = SshConfig {
            host: "target".to_string(),
            username: "operator".to_string(),
            auth: AuthMethod::Agent,
            ..SshConfig::default()
        };
        let without_target_forwarding = config.connection_key();
        config.agent_forwarding = true;
        let with_target_forwarding = config.connection_key();
        assert_ne!(without_target_forwarding, with_target_forwarding);

        config.agent_forwarding = false;
        config.proxy_chain = Some(vec![ProxyHopConfig {
            host: "jump".to_string(),
            port: 22,
            username: "operator".to_string(),
            auth: AuthMethod::Agent,
            agent_forwarding: false,
            identity_agent: None,
            agent_forwarding_socket: None,
            legacy_ssh_compatibility: false,
            ssh_algorithms: SshAlgorithmPreferences::default(),
            strict_host_key_checking: true,
            trust_host_key: None,
            expected_host_key_fingerprint: None,
        }]);
        let without_proxy_forwarding = config.connection_key();
        config.proxy_chain.as_mut().unwrap()[0].agent_forwarding = true;
        let with_proxy_forwarding = config.connection_key();
        assert_ne!(without_proxy_forwarding, with_proxy_forwarding);
    }

    #[test]
    fn connection_key_redacts_agent_endpoint_paths() {
        let private_endpoint = "/Users/private-account/.ssh/agent.sock";
        let config = SshConfig {
            host: "target".to_string(),
            username: "operator".to_string(),
            auth: AuthMethod::Agent,
            agent_forwarding: true,
            identity_agent: Some(private_endpoint.to_string()),
            ..SshConfig::default()
        };

        let key = config.connection_key();

        assert!(!key.contains(private_endpoint));
        assert!(key.contains("identity_agent="));
        assert!(key.contains("agent_forwarding_socket="));
    }

    #[test]
    fn runtime_auth_secret_detection_includes_target_and_proxy_hops() {
        let mut config = SshConfig {
            auth: AuthMethod::Agent,
            ..SshConfig::default()
        };
        assert!(!config.has_runtime_auth_secret());

        config.auth = AuthMethod::key("~/.ssh/id_ed25519", Some("passphrase".to_string()));
        assert!(config.has_runtime_auth_secret());

        config.auth = AuthMethod::Agent;
        config.proxy_chain = Some(vec![ProxyHopConfig {
            host: "jump.example.com".to_string(),
            port: 22,
            username: "operator".to_string(),
            auth: AuthMethod::password("proxy-password"),
            agent_forwarding: false,
            identity_agent: None,
            agent_forwarding_socket: None,
            legacy_ssh_compatibility: false,
            ssh_algorithms: SshAlgorithmPreferences::default(),
            strict_host_key_checking: false,
            trust_host_key: None,
            expected_host_key_fingerprint: None,
        }]);

        assert!(config.has_runtime_auth_secret());
    }
}
