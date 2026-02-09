//! SSH Client implementation using russh

use std::net::ToSocketAddrs;
use std::sync::Arc;
use std::time::Duration;

use russh::*;
use russh::keys::key::PrivateKeyWithHashAlg;
use russh::keys::PublicKey;
use tracing::{debug, info, warn};

use super::config::{AuthMethod, SshConfig};
use super::error::SshError;
use super::known_hosts::{get_known_hosts, HostKeyVerification};
use super::session::SshSession;

/// SSH Client handler for russh
pub struct SshClient {
    config: SshConfig,
}

impl SshClient {
    pub fn new(config: SshConfig) -> Self {
        Self { config }
    }

    /// Connect to the SSH server and return a session
    pub async fn connect(self) -> Result<SshSession, SshError> {
        let addr = format!("{}:{}", self.config.host, self.config.port);

        info!("Connecting to SSH server at {}", addr);

        // Resolve address
        let socket_addr = addr
            .to_socket_addrs()
            .map_err(|e| SshError::ConnectionFailed(format!("Failed to resolve address: {}", e)))?
            .next()
            .ok_or_else(|| SshError::ConnectionFailed("No address found".to_string()))?;

        // Configure SSH client with keepalive
        let ssh_config = client::Config {
            inactivity_timeout: None, // Disabled: app-level heartbeat (15s) handles liveness
            keepalive_interval: Some(Duration::from_secs(30)),  // Send keepalive every 30s
            keepalive_max: 3, // Disconnect after 3 missed keepalives (90s total)
            ..Default::default()
        };

        // Create SSH client handler with host info for key verification
        let handler = ClientHandler::with_trust(
            self.config.host.clone(),
            self.config.port,
            self.config.strict_host_key_checking,
            self.config.trust_host_key,
        );

        // Connect with timeout
        let mut handle = tokio::time::timeout(
            Duration::from_secs(self.config.timeout_secs),
            client::connect(Arc::new(ssh_config), socket_addr, handler),
        )
        .await
        .map_err(|_| SshError::Timeout("Connection timed out".to_string()))?
        .map_err(|e| SshError::ConnectionFailed(e.to_string()))?;

        debug!("SSH handshake completed");

        // Authenticate
        let authenticated = match &self.config.auth {
            AuthMethod::Password { password } => handle
                .authenticate_password(&self.config.username, password)
                .await
                .map_err(|e| SshError::AuthenticationFailed(e.to_string()))?,
            AuthMethod::Key {
                key_path,
                passphrase,
            } => {
                let key = if let Some(pass) = passphrase {
                    russh::keys::load_secret_key(key_path, Some(pass))
                        .map_err(|e| SshError::KeyError(e.to_string()))?
                } else {
                    russh::keys::load_secret_key(key_path, None)
                        .map_err(|e| SshError::KeyError(e.to_string()))?
                };

                let key_with_hash = PrivateKeyWithHashAlg::new(Arc::new(key), None);

                handle
                    .authenticate_publickey(&self.config.username, key_with_hash)
                    .await
                    .map_err(|e| SshError::AuthenticationFailed(e.to_string()))?
            }
            AuthMethod::Agent => {
                // Connect to SSH Agent and authenticate
                let mut agent =
                    crate::ssh::agent::SshAgentClient::connect()
                        .await
                        .map_err(|e| {
                            SshError::AuthenticationFailed(format!(
                                "Failed to connect to SSH agent: {}",
                                e
                            ))
                        })?;

                agent
                    .authenticate(&handle, &self.config.username)
                    .await
                    .map_err(|e| SshError::AuthenticationFailed(e.to_string()))?;

                // Agent authentication returns () on success, set flag manually
                client::AuthResult::Success
            }
            AuthMethod::Certificate {
                key_path,
                cert_path,
                passphrase,
            } => {
                // Load private key
                let key = if let Some(pass) = passphrase {
                    russh::keys::load_secret_key(key_path, Some(pass))
                        .map_err(|e| SshError::KeyError(e.to_string()))?
                } else {
                    russh::keys::load_secret_key(key_path, None)
                        .map_err(|e| SshError::KeyError(e.to_string()))?
                };

                // Load and parse OpenSSH certificate
                let cert = russh::keys::load_openssh_certificate(cert_path)
                    .map_err(|e| SshError::CertificateParseError(format!("Failed to load certificate: {}", e)))?;

                // Authenticate with certificate
                handle
                    .authenticate_openssh_cert(&self.config.username, Arc::new(key), cert)
                    .await
                    .map_err(|e| SshError::AuthenticationFailed(format!("Certificate authentication failed: {}", e)))?
            }
            AuthMethod::KeyboardInteractive => {
                // KeyboardInteractive is handled by the separate KBI flow (commands/kbi.rs)
                // This path should never be reached - KBI uses ssh_connect_kbi command
                return Err(SshError::AuthenticationFailed(
                    "KeyboardInteractive must be initiated via ssh_connect_kbi command".to_string(),
                ));
            }
        };

        if !authenticated.success() {
            return Err(SshError::AuthenticationFailed(
                "Authentication rejected by server".to_string(),
            ));
        }

        info!("SSH authentication successful");

        // Create session
        Ok(SshSession::new(handle, self.config.cols, self.config.rows))
    }
}

/// Client handler for russh callbacks
///
/// This handler processes server-initiated events, including:
/// - Host key verification against ~/.ssh/known_hosts
/// - Remote port forwarding (forwarded-tcpip channels)
pub struct ClientHandler {
    /// Target host for key verification
    host: String,
    /// Target port
    port: u16,
    /// Strict host key checking mode
    /// - true: reject unknown/changed keys
    /// - false: auto-accept unknown keys (still reject changed)
    strict: bool,
    /// Trust host key mode for TOFU
    /// - None: use strict behavior
    /// - Some(true): trust and save unknown keys
    /// - Some(false): trust for session only (don't save)
    trust_host_key: Option<bool>,
}

impl ClientHandler {
    pub fn new(host: String, port: u16, strict: bool) -> Self {
        Self { host, port, strict, trust_host_key: None }
    }

    pub fn with_trust(host: String, port: u16, strict: bool, trust_host_key: Option<bool>) -> Self {
        Self { host, port, strict, trust_host_key }
    }
}

impl client::Handler for ClientHandler {
    type Error = SshError;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> Result<bool, Self::Error> {
        let known_hosts = get_known_hosts();
        let verification = known_hosts.verify(&self.host, self.port, server_public_key);

        match verification {
            HostKeyVerification::Verified => {
                info!("Host key verified for {}:{}", self.host, self.port);
                Ok(true)
            }
            HostKeyVerification::Unknown { fingerprint } => {
                // Check if user has pre-approved this key via TOFU flow
                if let Some(trust) = self.trust_host_key {
                    if trust {
                        // Trust and save to known_hosts
                        info!(
                            "TOFU: Trusting and saving host key for {}:{} (fingerprint: {})",
                            self.host, self.port, fingerprint
                        );
                        if let Err(e) = known_hosts.add_host(&self.host, self.port, server_public_key) {
                            warn!("Failed to save host key: {}", e);
                        }
                    } else {
                        // Trust for this session only (don't save)
                        info!(
                            "TOFU: Trusting host key for session only {}:{} (fingerprint: {})",
                            self.host, self.port, fingerprint
                        );
                    }
                    return Ok(true);
                }

                if self.strict {
                    // Strict mode: reject unknown hosts
                    warn!(
                        "Unknown host key for {}:{} (fingerprint: {}). Strict mode enabled, rejecting.",
                        self.host, self.port, fingerprint
                    );
                    Err(SshError::ConnectionFailed(format!(
                        "Host key verification failed: unknown host {}:{}. Fingerprint: {}. \
                         Add to known_hosts or disable strict mode.",
                        self.host, self.port, fingerprint
                    )))
                } else {
                    // Non-strict mode: auto-accept and save unknown keys
                    // NOTE: This is the legacy behavior, kept for backward compatibility
                    info!(
                        "New host {}:{}, auto-adding to known_hosts (fingerprint: {})",
                        self.host, self.port, fingerprint
                    );
                    if let Err(e) = known_hosts.add_host(&self.host, self.port, server_public_key) {
                        warn!("Failed to save host key: {}", e);
                    }
                    Ok(true)
                }
            }
            HostKeyVerification::Changed {
                expected_fingerprint,
                actual_fingerprint,
            } => {
                // ALWAYS reject changed keys - potential MITM attack
                warn!(
                    "HOST KEY CHANGED for {}:{}! Expected {}, got {}. POSSIBLE MITM ATTACK!",
                    self.host, self.port, expected_fingerprint, actual_fingerprint
                );
                Err(SshError::ConnectionFailed(format!(
                    "HOST KEY VERIFICATION FAILED: Key for {}:{} has changed! \
                     Expected: {}, Actual: {}. \
                     This could indicate a man-in-the-middle attack. \
                     If the key change is legitimate, remove the old key from ~/.ssh/known_hosts",
                    self.host, self.port, expected_fingerprint, actual_fingerprint
                )))
            }
        }
    }

    /// Called when the server opens a channel for a new remote port forwarding connection.
    /// This happens when someone connects to the forwarded port on the remote server.
    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: Channel<client::Msg>,
        connected_address: &str,
        connected_port: u32,
        originator_address: &str,
        originator_port: u32,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        info!(
            "Server opened forwarded-tcpip channel: {}:{} from {}:{}",
            connected_address, connected_port, originator_address, originator_port
        );

        // Import the handler function from forwarding module
        use crate::forwarding::remote::handle_forwarded_connection;

        let connected_address = connected_address.to_string();
        let originator_address = originator_address.to_string();

        // Spawn a task to handle this forwarded connection
        // We can't block here as this is called from the SSH event loop
        tokio::spawn(async move {
            if let Err(e) = handle_forwarded_connection(
                channel,
                &connected_address,
                connected_port,
                &originator_address,
                originator_port,
            )
            .await
            {
                warn!(
                    "Failed to handle forwarded connection {}:{}: {}",
                    connected_address, connected_port, e
                );
            }
        });

        Ok(())
    }
}
