// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! SSH Client implementation using russh

use std::net::ToSocketAddrs;
use std::sync::Arc;
use std::time::Duration;

use russh::keys::PublicKey;
use russh::*;
use tracing::{debug, info, warn};

use super::auth::{
    DEFAULT_AUTH_TIMEOUT_SECS, authenticate_certificate_best_algo, authenticate_password,
    authenticate_publickey_best_algo, build_client_config, ensure_auth_success,
    load_certificate_auth_material, load_private_key_material, try_kbi_auth_chain,
};
use super::config::{AuthMethod, SshConfig};
use super::error::SshError;
use super::known_hosts::{HostKeyVerification, get_known_hosts};
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
    ///
    /// If `app_handle` is provided, multi-step authentication is supported:
    /// when the primary auth method returns `partial_success` with
    /// `keyboard-interactive` in `remaining_methods`, a KBI prompt flow
    /// is automatically triggered via Tauri events.
    pub async fn connect(
        self,
        app_handle: Option<&tauri::AppHandle>,
    ) -> Result<SshSession, SshError> {
        let addr = format!("{}:{}", self.config.host, self.config.port);

        info!("Connecting to SSH server at {}", addr);

        // Resolve address
        let socket_addr = addr
            .to_socket_addrs()
            .map_err(|e| SshError::DnsResolution {
                address: addr.clone(),
                message: e.to_string(),
            })?
            .next()
            .ok_or_else(|| SshError::DnsResolution {
                address: addr.clone(),
                message: "No address found".to_string(),
            })?;

        // SSH keepalive config (defense-in-depth):
        // Layer 1 (here): russh native keepalive — safety net in case app heartbeat stalls
        // Layer 2: App-level heartbeat (15s) in connection_registry — provides granular
        //          LinkDown events, smart probe confirmation, and frontend state updates
        let ssh_config = build_client_config();

        // Create SSH client handler with host info for key verification
        let handler = ClientHandler::with_trust(
            self.config.host.clone(),
            self.config.port,
            self.config.strict_host_key_checking,
            self.config.trust_host_key,
            self.config.agent_forwarding,
            self.config.expected_host_key_fingerprint.clone(),
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
            AuthMethod::Password { password } => {
                authenticate_password(
                    &mut handle,
                    &self.config.username,
                    password,
                    DEFAULT_AUTH_TIMEOUT_SECS,
                    "Password authentication timed out",
                    "Password authentication timed out (retry)",
                    "Password auth",
                )
                .await?
            }
            AuthMethod::Key {
                key_path,
                passphrase,
            } => {
                let key =
                    load_private_key_material(key_path, passphrase.as_ref().map(|p| p.as_str()))?;
                authenticate_publickey_best_algo(&mut handle, &self.config.username, key).await?
            }
            AuthMethod::Agent => {
                // Connect to SSH Agent and authenticate
                let mut agent = crate::ssh::agent::SshAgentClient::connect().await?;
                agent
                    .authenticate(&mut handle, self.config.username.clone())
                    .await?;
                client::AuthResult::Success
            }
            AuthMethod::Certificate {
                key_path,
                cert_path,
                passphrase,
            } => {
                let (key, cert) = load_certificate_auth_material(
                    key_path,
                    cert_path,
                    passphrase.as_ref().map(|p| p.as_str()),
                )?;

                authenticate_certificate_best_algo(&mut handle, &self.config.username, key, cert)
                    .await?
            }
            AuthMethod::KeyboardInteractive => {
                // KeyboardInteractive is handled by the separate KBI flow (commands/kbi.rs)
                // This path should never be reached - KBI uses ssh_connect_kbi command
                return Err(SshError::AuthenticationFailed(
                    "KeyboardInteractive must be initiated via ssh_connect_kbi command".to_string(),
                ));
            }
        };

        // Multi-step auth chaining: if server returned partial_success with
        // keyboard-interactive in remaining_methods, automatically run KBI flow
        if !authenticated.success() {
            if let Some(app) = app_handle {
                if try_kbi_auth_chain(&authenticated, &mut handle, &self.config.username, app)
                    .await?
                {
                    info!("SSH authentication successful (multi-step: primary + KBI)");
                    return Ok(SshSession::new(
                        handle,
                        self.config.cols,
                        self.config.rows,
                        self.config.agent_forwarding,
                    ));
                }
            }
        }

        ensure_auth_success(&authenticated, "Authentication rejected by server")?;

        info!("SSH authentication successful");

        // Create session
        Ok(SshSession::new(
            handle,
            self.config.cols,
            self.config.rows,
            self.config.agent_forwarding,
        ))
    }
}

/// Client handler for russh callbacks
///
/// This handler processes server-initiated events, including:
/// - Host key verification against ~/.ssh/known_hosts
/// - Remote port forwarding (forwarded-tcpip channels)
/// - SSH agent forwarding relay
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
    /// Optional fingerprint captured during preflight to prevent TOCTOU drift.
    expected_host_key_fingerprint: Option<String>,
    /// Whether agent forwarding was requested by the client.
    /// Defense-in-depth: reject server-initiated agent channels if we never asked.
    agent_forwarding_requested: bool,
    /// Semaphore to limit concurrent agent forwarding channels (max 16)
    agent_forward_semaphore: std::sync::Arc<tokio::sync::Semaphore>,
}

impl ClientHandler {
    pub fn new(host: String, port: u16, strict: bool) -> Self {
        Self {
            host,
            port,
            strict,
            trust_host_key: None,
            expected_host_key_fingerprint: None,
            agent_forwarding_requested: false,
            agent_forward_semaphore: std::sync::Arc::new(tokio::sync::Semaphore::new(16)),
        }
    }

    pub fn with_trust(
        host: String,
        port: u16,
        strict: bool,
        trust_host_key: Option<bool>,
        agent_forwarding: bool,
        expected_host_key_fingerprint: Option<String>,
    ) -> Self {
        Self {
            host,
            port,
            strict,
            trust_host_key,
            expected_host_key_fingerprint,
            agent_forwarding_requested: agent_forwarding,
            agent_forward_semaphore: std::sync::Arc::new(tokio::sync::Semaphore::new(16)),
        }
    }

    /// Mark that agent forwarding was requested on this connection.
    /// Must be called after sending `auth-agent-req@openssh.com`.
    pub fn set_agent_forwarding_requested(&mut self, requested: bool) {
        self.agent_forwarding_requested = requested;
    }
}

impl client::Handler for ClientHandler {
    type Error = SshError;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> Result<bool, Self::Error> {
        let actual_fingerprint = super::known_hosts::KnownHostsStore::fingerprint(server_public_key);

        if let Some(expected_fingerprint) = self.expected_host_key_fingerprint.as_deref() {
            if actual_fingerprint != expected_fingerprint {
                warn!(
                    "Host key changed between preflight and connect for {}:{}: expected {}, got {}",
                    self.host, self.port, expected_fingerprint, actual_fingerprint
                );
                return Err(SshError::HostKeyChanged {
                    host: self.host.clone(),
                    port: self.port,
                    expected_fingerprint: expected_fingerprint.to_string(),
                    actual_fingerprint,
                });
            }
        }

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
                        if let Err(e) =
                            known_hosts.add_host(&self.host, self.port, server_public_key)
                        {
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
                    Err(SshError::HostKeyUnknown {
                        host: self.host.clone(),
                        port: self.port,
                        fingerprint,
                    })
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
                Err(SshError::HostKeyChanged {
                    host: self.host.clone(),
                    port: self.port,
                    expected_fingerprint,
                    actual_fingerprint,
                })
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

    /// Called when the server opens an agent forwarding channel.
    /// This happens when the remote side needs to use the local SSH agent
    /// (e.g., for `ssh` commands on the remote host).
    ///
    /// Defense-in-depth: rejects the channel if agent forwarding was not requested,
    /// and limits concurrent agent channels via semaphore.
    async fn server_channel_open_agent_forward(
        &mut self,
        channel: Channel<client::Msg>,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        // Defense-in-depth: reject unsolicited agent channels
        if !self.agent_forwarding_requested {
            warn!(
                "Server {}:{} tried to open agent channel but forwarding was not requested — rejecting",
                self.host, self.port
            );
            let _ = channel.eof().await;
            return Ok(());
        }

        info!("Server opened agent forwarding channel");

        use super::agent_forward::handle_agent_forward_channel;

        // Acquire semaphore permit to limit concurrent agent channels
        let semaphore = self.agent_forward_semaphore.clone();
        let permit = match semaphore.try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => {
                warn!("Too many concurrent agent forwarding channels — rejecting");
                let _ = channel.eof().await;
                return Ok(());
            }
        };

        // Spawn a task to handle this agent forwarding channel
        // We can't block here as this is called from the SSH event loop
        tokio::spawn(async move {
            handle_agent_forward_channel(channel).await;
            drop(permit); // Release semaphore when done
        });

        Ok(())
    }
}
