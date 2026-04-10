// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! ProxyJump Implementation for SSH
//!
//! Implements SSH connection through jump hosts (bastion hosts).
//! Supports unlimited multi-hop proxy with SSH-over-SSH.
//!
//! # Algorithm
//!
//! Multi-hop connection uses `direct-tcpip` channels to establish SSH-over-SSH tunnels:
//! ```text
//! Client --SSH--> [Jump1] --direct-tcpip--> [Jump2] --direct-tcpip--> ... --> [JumpN] --direct-tcpip--> [Target]
//! ```
//!
//! # Key APIs
//!
//! - `russh::client::connect_stream()` - Connect via custom AsyncRead + AsyncWrite transport
//! - `russh::ChannelStream` - Wrap channel as AsyncRead + AsyncWrite for nested SSH
//! - `Handle::channel_open_direct_tcpip()` - Open TCP tunnel through SSH
//!
//! # Performance
//!
//! - Zero-copy: `ChannelStream` wraps channels directly without buffering
//! - Non-blocking: All operations async with tokio
//! - Memory efficient: No extra buffers, channels used as transports

use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use russh::client::{self, Handle};
use tracing::{debug, info};

use super::auth::{
    DEFAULT_AUTH_TIMEOUT_SECS, authenticate_certificate_best_algo, authenticate_password,
    authenticate_publickey_best_algo, build_client_config, ensure_auth_success,
    load_certificate_auth_material, load_private_key_material,
};
use super::client::ClientHandler;
use super::config::AuthMethod;
use super::error::SshError;
use zeroize::Zeroizing;

use crate::session::tree::MAX_CHAIN_DEPTH;

/// Proxy hop configuration
#[derive(Debug, Clone)]
pub struct ProxyHop {
    /// Hostname of the jump host
    pub host: String,
    /// Port of the jump host (default: 22)
    pub port: u16,
    /// Username for authentication
    pub username: String,
    /// Authentication method
    pub auth: AuthMethod,
}

impl ProxyHop {
    /// Create a new proxy hop with password authentication
    pub fn with_password(
        host: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        Self {
            host: host.into(),
            port: 22,
            username: username.into(),
            auth: AuthMethod::Password {
                password: Zeroizing::new(password.into()),
            },
        }
    }

    /// Create a new proxy hop with key authentication
    pub fn with_key(
        host: impl Into<String>,
        username: impl Into<String>,
        key_path: impl Into<String>,
    ) -> Self {
        Self {
            host: host.into(),
            port: 22,
            username: username.into(),
            auth: AuthMethod::Key {
                key_path: key_path.into(),
                passphrase: None,
            },
        }
    }

    /// Set custom port
    pub fn port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }
}

/// Proxy chain for multi-hop SSH connections
#[derive(Debug, Clone, Default)]
pub struct ProxyChain {
    /// List of jump hosts (in order)
    pub hops: Vec<ProxyHop>,
}

impl ProxyChain {
    /// Create an empty proxy chain
    pub fn new() -> Self {
        Self { hops: Vec::new() }
    }

    /// Add a hop to the chain
    pub fn add_hop(mut self, hop: ProxyHop) -> Self {
        self.hops.push(hop);
        self
    }

    /// Check if the chain is empty
    pub fn is_empty(&self) -> bool {
        self.hops.is_empty()
    }

    /// Get the number of hops
    pub fn len(&self) -> usize {
        self.hops.len()
    }

    /// Get the first hop
    pub fn first(&self) -> Option<&ProxyHop> {
        self.hops.first()
    }
}

/// Result of a multi-hop proxy connection
///
/// Contains handles to all intermediate proxy jump hosts and a SSH handle
/// on the final target host (for PTY, SFTP, port forwarding, etc.).
pub struct ProxyConnection {
    /// Handles to all proxy jump hosts (for cleanup)
    /// Order: [jump1, jump2, ..., jumpN]
    /// All handles will be dropped automatically when ProxyConnection is dropped,
    /// triggering proper SSH disconnection for all intermediate hops.
    pub jump_handles: Vec<Handle<ClientHandler>>,

    /// SSH handle on the final target host
    /// This handle is used for PTY, SFTP, port forwarding, etc.
    pub target_handle: Handle<ClientHandler>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyConnectOperation {
    ResolveAddress,
    EstablishTransport,
    Authenticate,
    OpenTunnel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyConnectEndpointKind {
    JumpHost,
    Target,
}

#[derive(Debug, Clone)]
pub struct ProxyConnectEndpoint {
    pub kind: ProxyConnectEndpointKind,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub hop_index: Option<usize>,
    pub total_hops: usize,
    pub via_hop_index: Option<usize>,
}

impl ProxyConnectEndpoint {
    fn with_via_hop_index(mut self, via_hop_index: usize) -> Self {
        self.via_hop_index = Some(via_hop_index);
        self
    }
}

#[derive(Debug)]
pub enum ProxyConnectError {
    InvalidChain {
        detail: String,
        total_hops: usize,
    },
    Step {
        operation: ProxyConnectOperation,
        endpoint: ProxyConnectEndpoint,
        source: SshError,
    },
}

impl ProxyConnectError {
    fn step(
        operation: ProxyConnectOperation,
        endpoint: ProxyConnectEndpoint,
        source: SshError,
    ) -> Self {
        Self::Step {
            operation,
            endpoint,
            source,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ProxyAuthPath {
    Direct,
    Stream,
}

fn jump_host_endpoint(hop: &ProxyHop, hop_index: usize, total_hops: usize) -> ProxyConnectEndpoint {
    ProxyConnectEndpoint {
        kind: ProxyConnectEndpointKind::JumpHost,
        host: hop.host.clone(),
        port: hop.port,
        username: hop.username.clone(),
        hop_index: Some(hop_index),
        total_hops,
        via_hop_index: None,
    }
}

fn target_endpoint(
    target_host: &str,
    target_port: u16,
    target_username: &str,
    total_hops: usize,
) -> ProxyConnectEndpoint {
    ProxyConnectEndpoint {
        kind: ProxyConnectEndpointKind::Target,
        host: target_host.to_string(),
        port: target_port,
        username: target_username.to_string(),
        hop_index: None,
        total_hops,
        via_hop_index: None,
    }
}

fn resolve_socket_addr(addr: &str) -> Result<SocketAddr, SshError> {
    addr.to_socket_addrs()
        .map_err(|e| SshError::DnsResolution {
            address: addr.to_string(),
            message: e.to_string(),
        })?
        .next()
        .ok_or_else(|| SshError::DnsResolution {
            address: addr.to_string(),
            message: "No address found".to_string(),
        })
}

async fn authenticate_proxy_hop(
    handle: &mut Handle<ClientHandler>,
    hop: &ProxyHop,
    auth_path: ProxyAuthPath,
) -> Result<(), SshError> {
    let authenticated = match &hop.auth {
        AuthMethod::Password { password } => {
            let (timeout_message, retry_timeout_message, retry_debug_label) = match auth_path {
                ProxyAuthPath::Direct => (
                    format!("Password auth to {} timed out", hop.host),
                    format!("Password auth to {} timed out (retry)", hop.host),
                    format!("Jump host password auth to {}", hop.host),
                ),
                ProxyAuthPath::Stream => (
                    format!("Password auth to {} timed out", hop.host),
                    format!("Password auth to {} timed out (retry)", hop.host),
                    format!("Stream password auth to {}", hop.host),
                ),
            };

            authenticate_password(
                handle,
                &hop.username,
                password,
                DEFAULT_AUTH_TIMEOUT_SECS,
                &timeout_message,
                &retry_timeout_message,
                &retry_debug_label,
            )
            .await?
        }
        AuthMethod::Key {
            key_path,
            passphrase,
        } => {
            let key = load_private_key_material(key_path, passphrase.as_ref().map(|p| p.as_str()))?;
            authenticate_publickey_best_algo(handle, &hop.username, key).await?
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

            authenticate_certificate_best_algo(handle, &hop.username, key, cert).await?
        }
        AuthMethod::Agent => {
            let mut agent = crate::ssh::agent::SshAgentClient::connect().await?;
            agent.authenticate(handle, hop.username.clone()).await?;
            client::AuthResult::Success
        }
        AuthMethod::KeyboardInteractive => {
            return Err(SshError::AuthenticationFailed(
                "KeyboardInteractive authentication not supported for proxy chain hops"
                    .to_string(),
            ));
        }
    };

    ensure_auth_success(
        &authenticated,
        format!("Authentication to {} rejected", hop.host),
    )
}

impl ProxyConnection {
    /// Extract the target handle, leaving only jump handles.
    /// This is needed because ProxyConnection implements Drop.
    #[must_use = "into_target_handle transfers ownership - ignoring the result will leak the SSH connection"]
    pub fn into_target_handle(self) -> Handle<ClientHandler> {
        use std::mem::ManuallyDrop;

        let this = ManuallyDrop::new(self);
        // Safety: We're taking ownership of target_handle and will not use it again
        // The jump_handles will be properly dropped when `this` is dropped
        unsafe { std::ptr::read(&this.target_handle) }
    }
}

impl Drop for ProxyConnection {
    fn drop(&mut self) {
        info!(
            "Dropping ProxyConnection: {} intermediate jump handles",
            self.jump_handles.len()
        );
        // All jump handles will be dropped automatically,
        // which triggers proper SSH disconnection.
        // Note: target_handle is managed separately by the session system.
    }
}

fn build_proxy_client_handler(
    host: String,
    port: u16,
    trust_unknown_host: Option<bool>,
) -> ClientHandler {
    if let Some(trust) = trust_unknown_host {
        ClientHandler::with_trust(host, port, false, Some(trust), false, None)
    } else {
        ClientHandler::new(host, port, false)
    }
}

/// Connect directly to a single SSH host (internal helper)
async fn direct_connect(
    hop: &ProxyHop,
    timeout_secs: u64,
    trust_unknown_host: Option<bool>,
) -> Result<Handle<ClientHandler>, SshError> {
    let addr = format!("{}:{}", hop.host, hop.port);
    let socket_addr = resolve_socket_addr(&addr)?;

    info!("Connecting to jump host at {}", addr);

    // Create SSH config with keepalive
    let ssh_config = build_client_config();

    // Use non-strict mode for jump hosts (auto-accept unknown)
    let handler = build_proxy_client_handler(hop.host.clone(), hop.port, trust_unknown_host);

    // Connect with timeout
    let mut handle = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        client::connect(Arc::new(ssh_config), socket_addr, handler),
    )
    .await
    .map_err(|_| SshError::Timeout(format!("Connection to {} timed out", addr)))?
    .map_err(|e| SshError::ConnectionFailed(e.to_string()))?;

    debug!("SSH handshake with jump host completed");

    // Authenticate
    authenticate_proxy_hop(&mut handle, hop, ProxyAuthPath::Direct).await?;

    info!("Authenticated to jump host {}", hop.host);
    Ok(handle)
}

/// Connect to a jump host using a custom stream (SSH-over-SSH)
///
/// This enables multi-hop connections by establishing SSH over a
/// direct-tcpip channel from the previous hop.
///
/// # Arguments
///
/// * `hop` - The jump host configuration
/// * `stream` - The transport stream (usually a ChannelStream from previous hop)
/// * `timeout_secs` - Connection timeout in seconds
///
/// # Returns
///
/// A Handle to the SSH session on the jump host
async fn connect_via_stream(
    hop: &ProxyHop,
    stream: russh::ChannelStream<russh::client::Msg>,
    timeout_secs: u64,
    trust_unknown_host: Option<bool>,
) -> Result<Handle<ClientHandler>, SshError> {
    use russh::client;

    info!(
        "Connecting via stream to {}:{} (SSH-over-SSH)",
        hop.host, hop.port
    );

    // Create SSH config with keepalive
    let ssh_config = build_client_config();

    // Use non-strict mode for tunnel hosts (auto-accept unknown)
    let handler = build_proxy_client_handler(hop.host.clone(), hop.port, trust_unknown_host);
    let config = Arc::new(ssh_config);

    // Use russh::connect_stream() to connect over our custom stream!
    // This is the key API for SSH-over-SSH support.
    let mut handle = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        client::connect_stream(config, stream, handler),
    )
    .await
    .map_err(|_| {
        SshError::Timeout(format!(
            "Connection to {}:{} via stream timed out",
            hop.host, hop.port
        ))
    })?
    .map_err(|e| {
        SshError::ConnectionFailed(format!(
            "Failed to connect via stream to {}:{}: {}",
            hop.host, hop.port, e
        ))
    })?;

    debug!("SSH handshake via stream completed");

    // Authenticate
    authenticate_proxy_hop(&mut handle, hop, ProxyAuthPath::Stream).await?;

    info!("Authenticated via stream to {}", hop.host);
    Ok(handle)
}

/// Connect to a target host through a single jump host (ProxyJump)
///
/// This is a main entry point for single-hop proxy connections.
/// It returns a ProxyConnection that contains:
/// - The handle to jump host (for cleanup)
/// - The target SSH session handle (for PTY, SFTP, forwarding, etc.)
///
/// This is a convenience wrapper around `connect_via_proxy()` for single-hop scenarios.
pub async fn connect_via_single_hop(
    jump_host: &ProxyHop,
    target_host: &str,
    target_port: u16,
    timeout_secs: u64,
) -> Result<ProxyConnection, SshError> {
    info!(
        "Connecting to {}:{} via jump host {}@{}:{}",
        target_host, target_port, jump_host.username, jump_host.host, jump_host.port
    );

    // Build single-hop chain and use generic multi-hop function
    let chain = ProxyChain::new().add_hop(jump_host.clone());

    // Use the same auth as jump host for the target connection
    // This maintains compatibility with the original behavior.
    let result = connect_via_proxy(
        &chain,
        target_host,
        target_port,
        &jump_host.username,
        &jump_host.auth,
        timeout_secs,
    )
    .await?;

    // Validate: For single-hop, we should have 1 jump handle
    // and target_handle should be usable for PTY/shell
    if result.jump_handles.len() != 1 {
        return Err(SshError::ConnectionFailed(format!(
            "Expected 1 jump handle, got {}",
            result.jump_handles.len()
        )));
    }

    Ok(result)
}

/// Connect through a proxy chain (true multi-hop with SSH-over-SSH)
///
/// # Algorithm
///
/// ```text
/// Client --SSH--> [Jump1] --direct-tcpip--> [Jump2] --direct-tcpip--> ... --> [JumpN] --direct-tcpip--> [Target]
/// ```
///
/// # Steps
///
/// 1. Connect to Jump1 via SSH → Handle1
/// 2. Open direct-tcpip to Jump2 → Channel1
/// 3. Wrap Channel1 as ChannelStream → Stream1
/// 4. Connect to Jump2 via SSH over Stream1 → Handle2
/// 5. Repeat steps 2-4 until JumpN
/// 6. Open direct-tcpip from JumpN to Target → FinalChannel
/// 7. Wrap FinalChannel as ChannelStream → FinalStream
/// 8. Connect to Target via SSH over FinalStream → TargetHandle
///
/// # Performance
///
/// - Zero-copy: `ChannelStream` wraps channels directly
/// - Non-blocking: All operations async with tokio
/// - Memory efficient: No extra buffering, channels used directly
///
/// # Requirements
///
/// - russh >= 0.48 (for `connect_stream()` and `ChannelStream`)
/// - All intermediate hops must support direct-tcpip
pub async fn connect_via_proxy(
    chain: &ProxyChain,
    target_host: &str,
    target_port: u16,
    target_username: &str,
    target_auth: &AuthMethod,
    timeout_secs: u64,
) -> Result<ProxyConnection, SshError> {
    connect_via_proxy_internal(
        chain,
        target_host,
        target_port,
        target_username,
        target_auth,
        timeout_secs,
        None,
    )
    .await
}

pub async fn connect_via_proxy_for_test(
    chain: &ProxyChain,
    target_host: &str,
    target_port: u16,
    target_username: &str,
    target_auth: &AuthMethod,
    timeout_secs: u64,
) -> Result<ProxyConnection, ProxyConnectError> {
    if chain.is_empty() {
        return Err(ProxyConnectError::InvalidChain {
            detail: "Proxy chain is empty".into(),
            total_hops: 0,
        });
    }

    let num_hops = chain.hops.len();
    if num_hops > MAX_CHAIN_DEPTH as usize {
        return Err(ProxyConnectError::InvalidChain {
            detail: format!("Proxy chain too long: {} hops (max {})", num_hops, MAX_CHAIN_DEPTH),
            total_hops: num_hops,
        });
    }

    let mut current_stream: Option<russh::ChannelStream<russh::client::Msg>> = None;
    let mut jump_handles: Vec<Handle<ClientHandler>> = Vec::with_capacity(num_hops);

    for (index, hop) in chain.hops.iter().enumerate() {
        let hop_number = index + 1;
        let endpoint = jump_host_endpoint(hop, hop_number, num_hops);

        let handle = if let Some(stream) = current_stream.take() {
            connect_via_stream_for_test(hop, stream, timeout_secs, endpoint.clone()).await?
        } else {
            direct_connect_for_test(hop, timeout_secs, endpoint.clone()).await?
        };

        if index < num_hops - 1 {
            let next_hop = &chain.hops[index + 1];
            let next_endpoint = jump_host_endpoint(next_hop, hop_number + 1, num_hops)
                .with_via_hop_index(hop_number);

            let channel = handle
                .channel_open_direct_tcpip(&next_hop.host, next_hop.port as u32, "127.0.0.1", 0)
                .await
                .map_err(|e| {
                    ProxyConnectError::step(
                        ProxyConnectOperation::OpenTunnel,
                        next_endpoint,
                        SshError::ConnectionFailed(format!(
                            "Failed to open tunnel to {}@{}:{}: {}",
                            next_hop.username, next_hop.host, next_hop.port, e
                        )),
                    )
                })?;

            current_stream = Some(channel.into_stream());
        } else {
            let destination = target_endpoint(target_host, target_port, target_username, num_hops)
                .with_via_hop_index(hop_number);

            let channel = handle
                .channel_open_direct_tcpip(target_host, target_port as u32, "127.0.0.1", 0)
                .await
                .map_err(|e| {
                    ProxyConnectError::step(
                        ProxyConnectOperation::OpenTunnel,
                        destination,
                        SshError::ConnectionFailed(format!(
                            "Failed to open tunnel to target {}@{}:{}: {}",
                            target_username, target_host, target_port, e
                        )),
                    )
                })?;

            current_stream = Some(channel.into_stream());
        }

        jump_handles.push(handle);
    }

    let target_hop = ProxyHop {
        host: target_host.to_string(),
        port: target_port,
        username: target_username.to_string(),
        auth: target_auth.clone(),
    };

    let stream = current_stream.ok_or_else(|| ProxyConnectError::step(
        ProxyConnectOperation::OpenTunnel,
        target_endpoint(target_host, target_port, target_username, num_hops),
        SshError::ConnectionFailed("No stream available for target connection".into()),
    ))?;

    let target_handle = connect_via_stream_for_test(
        &target_hop,
        stream,
        timeout_secs,
        target_endpoint(target_host, target_port, target_username, num_hops),
    )
    .await?;

    Ok(ProxyConnection {
        jump_handles,
        target_handle,
    })
}

async fn direct_connect_for_test(
    hop: &ProxyHop,
    timeout_secs: u64,
    endpoint: ProxyConnectEndpoint,
) -> Result<Handle<ClientHandler>, ProxyConnectError> {
    let addr = format!("{}:{}", hop.host, hop.port);
    let socket_addr = resolve_socket_addr(&addr).map_err(|source| {
        ProxyConnectError::step(ProxyConnectOperation::ResolveAddress, endpoint.clone(), source)
    })?;

    let ssh_config = build_client_config();
    let handler = build_proxy_client_handler(hop.host.clone(), hop.port, Some(false));

    let mut handle = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        client::connect(Arc::new(ssh_config), socket_addr, handler),
    )
    .await
    .map_err(|_| {
        ProxyConnectError::step(
            ProxyConnectOperation::EstablishTransport,
            endpoint.clone(),
            SshError::Timeout(format!("Connection to {} timed out", addr)),
        )
    })?
    .map_err(|e| {
        ProxyConnectError::step(
            ProxyConnectOperation::EstablishTransport,
            endpoint.clone(),
            SshError::ConnectionFailed(e.to_string()),
        )
    })?;

    authenticate_proxy_hop(&mut handle, hop, ProxyAuthPath::Direct)
        .await
        .map_err(|source| ProxyConnectError::step(ProxyConnectOperation::Authenticate, endpoint, source))?;

    Ok(handle)
}

async fn connect_via_stream_for_test(
    hop: &ProxyHop,
    stream: russh::ChannelStream<russh::client::Msg>,
    timeout_secs: u64,
    endpoint: ProxyConnectEndpoint,
) -> Result<Handle<ClientHandler>, ProxyConnectError> {
    let ssh_config = build_client_config();
    let handler = build_proxy_client_handler(hop.host.clone(), hop.port, Some(false));
    let config = Arc::new(ssh_config);

    let mut handle = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        client::connect_stream(config, stream, handler),
    )
    .await
    .map_err(|_| {
        ProxyConnectError::step(
            ProxyConnectOperation::EstablishTransport,
            endpoint.clone(),
            SshError::Timeout(format!("Connection to {}:{} via stream timed out", hop.host, hop.port)),
        )
    })?
    .map_err(|e| {
        ProxyConnectError::step(
            ProxyConnectOperation::EstablishTransport,
            endpoint.clone(),
            SshError::ConnectionFailed(format!(
                "Failed to connect via stream to {}:{}: {}",
                hop.host, hop.port, e
            )),
        )
    })?;

    authenticate_proxy_hop(&mut handle, hop, ProxyAuthPath::Stream)
        .await
        .map_err(|source| ProxyConnectError::step(ProxyConnectOperation::Authenticate, endpoint, source))?;

    Ok(handle)
}

async fn connect_via_proxy_internal(
    chain: &ProxyChain,
    target_host: &str,
    target_port: u16,
    target_username: &str,
    target_auth: &AuthMethod,
    timeout_secs: u64,
    trust_unknown_host: Option<bool>,
) -> Result<ProxyConnection, SshError> {
    if chain.is_empty() {
        return Err(SshError::ConnectionFailed("Proxy chain is empty".into()));
    }

    let num_hops = chain.hops.len();
    if num_hops > MAX_CHAIN_DEPTH as usize {
        return Err(SshError::ConnectionFailed(format!(
            "Proxy chain too long: {} hops (max {})",
            num_hops, MAX_CHAIN_DEPTH
        )));
    }
    info!(
        "Establishing multi-hop SSH: {} proxy hops to {}@{}:{}",
        num_hops, target_username, target_host, target_port
    );

    let mut current_stream: Option<russh::ChannelStream<russh::client::Msg>> = None;
    let mut jump_handles: Vec<Handle<ClientHandler>> = Vec::with_capacity(num_hops);

    for (i, hop) in chain.hops.iter().enumerate() {
        info!(
            "Proxy hop {}: connecting to {}@{}:{}",
            i + 1,
            hop.username,
            hop.host,
            hop.port
        );

        let handle = if let Some(stream) = current_stream.take() {
            connect_via_stream(hop, stream, timeout_secs, trust_unknown_host).await?
        } else {
            direct_connect(hop, timeout_secs, trust_unknown_host).await?
        };

        if i < num_hops - 1 {
            let next_hop = &chain.hops[i + 1];
            info!(
                "Proxy hop {}: opening tunnel to next hop {}@{}:{}",
                i + 1,
                next_hop.username,
                next_hop.host,
                next_hop.port
            );

            let channel = handle
                .channel_open_direct_tcpip(&next_hop.host, next_hop.port as u32, "127.0.0.1", 0)
                .await
                .map_err(|e| {
                    SshError::ConnectionFailed(format!(
                        "Failed to open tunnel to {}@{}:{}: {}",
                        next_hop.username, next_hop.host, next_hop.port, e
                    ))
                })?;

            current_stream = Some(channel.into_stream());
        } else {
            info!(
                "Last proxy hop ({}): opening tunnel to target {}@{}:{}",
                i + 1,
                target_username,
                target_host,
                target_port
            );

            let channel = handle
                .channel_open_direct_tcpip(target_host, target_port as u32, "127.0.0.1", 0)
                .await
                .map_err(|e| {
                    SshError::ConnectionFailed(format!(
                        "Failed to open tunnel to target {}@{}:{}: {}",
                        target_username, target_host, target_port, e
                    ))
                })?;

            current_stream = Some(channel.into_stream());
        }

        jump_handles.push(handle);
    }

    let target_hop = ProxyHop {
        host: target_host.to_string(),
        port: target_port,
        username: target_username.to_string(),
        auth: target_auth.clone(),
    };

    info!(
        "Connecting to target {}@{}:{} through final tunnel",
        target_username, target_host, target_port
    );

    let stream = current_stream.ok_or_else(|| {
        SshError::ConnectionFailed("No stream available for target connection".into())
    })?;

    let target_handle = connect_via_stream(&target_hop, stream, timeout_secs, trust_unknown_host)
        .await?;

    info!("Target connection established");

    Ok(ProxyConnection {
        jump_handles,
        target_handle,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proxy_chain_builder() {
        let chain = ProxyChain::new()
            .add_hop(ProxyHop::with_password(
                "jump1.example.com",
                "user1",
                "pass1",
            ))
            .add_hop(ProxyHop::with_key("jump2.example.com", "user2", "~/.ssh/id_rsa").port(2222));

        assert_eq!(chain.len(), 2);
        assert_eq!(chain.hops[0].host, "jump1.example.com");
        assert_eq!(chain.hops[1].port, 2222);
    }

    #[test]
    fn test_proxy_hop_with_password() {
        let hop = ProxyHop::with_password("bastion.example.com", "admin", "secret123");

        assert_eq!(hop.host, "bastion.example.com");
        assert_eq!(hop.username, "admin");
        assert_eq!(hop.port, 22);
        match hop.auth {
            AuthMethod::Password { password } => assert_eq!(&*password, "secret123"),
            _ => panic!("Expected password auth"),
        }
    }

    #[test]
    fn test_proxy_hop_with_key() {
        let hop = ProxyHop::with_key("bastion.example.com", "admin", "~/.ssh/id_ed25519").port(22);

        assert_eq!(hop.host, "bastion.example.com");
        assert_eq!(hop.username, "admin");
        assert_eq!(hop.port, 22);
        match hop.auth {
            AuthMethod::Key {
                key_path,
                passphrase,
            } => {
                assert_eq!(key_path, "~/.ssh/id_ed25519");
                assert!(passphrase.is_none());
            }
            _ => panic!("Expected key auth"),
        }
    }

    #[test]
    fn test_empty_proxy_chain() {
        let chain = ProxyChain::new();
        assert!(chain.is_empty());
        assert_eq!(chain.len(), 0);
        assert!(chain.first().is_none());
    }
}
