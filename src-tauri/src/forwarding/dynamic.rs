//! Dynamic SOCKS5 Proxy Forwarding
//!
//! Implements a local SOCKS5 proxy server that tunnels connections through SSH.
//! Example: Local SOCKS5 proxy on 127.0.0.1:1080 -> SSH tunnel -> any destination

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, info, warn};

use super::events::ForwardEventEmitter;
use super::manager::ForwardStatus;
use crate::ssh::{HandleController, SshError};

/// Forward statistics
#[derive(Debug, Clone, Default)]
pub struct ForwardStats {
    /// Total connection count
    pub connection_count: u64,
    /// Currently active connections
    pub active_connections: u64,
    /// Total bytes sent (to remote)
    pub bytes_sent: u64,
    /// Total bytes received (from remote)
    pub bytes_received: u64,
}

/// SOCKS5 protocol constants
#[allow(dead_code)]
mod socks5 {
    pub const VERSION: u8 = 0x05;
    pub const AUTH_NONE: u8 = 0x00;
    pub const CMD_CONNECT: u8 = 0x01;
    pub const ATYP_IPV4: u8 = 0x01;
    pub const ATYP_DOMAIN: u8 = 0x03;
    pub const ATYP_IPV6: u8 = 0x04;
    pub const REP_SUCCESS: u8 = 0x00;
    pub const REP_GENERAL_FAILURE: u8 = 0x01;
    pub const REP_CONN_NOT_ALLOWED: u8 = 0x02;
    pub const REP_NETWORK_UNREACHABLE: u8 = 0x03;
    pub const REP_HOST_UNREACHABLE: u8 = 0x04;
    pub const REP_CONN_REFUSED: u8 = 0x05;
    pub const REP_CMD_NOT_SUPPORTED: u8 = 0x07;
    pub const REP_ADDR_NOT_SUPPORTED: u8 = 0x08;
}

/// Dynamic (SOCKS5) port forwarding configuration
#[derive(Debug, Clone)]
pub struct DynamicForward {
    /// Local address to bind SOCKS5 proxy (e.g., "127.0.0.1:1080")
    pub local_addr: String,
    /// Description for UI display
    pub description: Option<String>,
}

impl DynamicForward {
    /// Create a new dynamic forward
    pub fn new(local_addr: impl Into<String>) -> Self {
        Self {
            local_addr: local_addr.into(),
            description: None,
        }
    }

    /// Create with default port 1080
    pub fn default_port() -> Self {
        Self {
            local_addr: "127.0.0.1:1080".into(),
            description: Some("SOCKS5 Proxy".into()),
        }
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }
}

/// Handle to a running dynamic forward (SOCKS5 proxy)
pub struct DynamicForwardHandle {
    /// Forward configuration
    pub config: DynamicForward,
    /// Actual bound address
    pub bound_addr: SocketAddr,
    /// Flag to indicate if running
    running: Arc<AtomicBool>,
    /// Channel to signal stop
    stop_tx: mpsc::Sender<()>,
    /// Stats tracking
    stats: Arc<parking_lot::RwLock<ForwardStats>>,
}

impl DynamicForwardHandle {
    /// Stop the SOCKS5 proxy and wait for active connections to close
    pub async fn stop(&self) {
        info!("Stopping SOCKS5 proxy on {}", self.bound_addr);
        self.running.store(false, Ordering::SeqCst);
        let _ = self.stop_tx.send(()).await;

        // 等待所有活跃连接关闭（最多等待 5 秒）
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(5);
        while self.stats.read().active_connections > 0 {
            if start.elapsed() > timeout {
                warn!(
                    "Timeout waiting for {} active connections to close on {}",
                    self.stats.read().active_connections,
                    self.bound_addr
                );
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    /// Check if the proxy is still running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Get current stats
    pub fn stats(&self) -> ForwardStats {
        self.stats.read().clone()
    }
}

/// Start dynamic (SOCKS5) port forwarding
///
/// This function:
/// 1. Starts a local SOCKS5 proxy server
/// 2. For each incoming connection, performs SOCKS5 handshake
/// 3. Opens direct-tcpip channel through SSH to the requested destination
/// 4. Bridges data between the local socket and the SSH channel
pub async fn start_dynamic_forward(
    handle_controller: HandleController,
    config: DynamicForward,
) -> Result<DynamicForwardHandle, SshError> {
    // Subscribe to disconnect notifications
    let disconnect_rx = handle_controller.subscribe_disconnect();
    start_dynamic_forward_with_disconnect(handle_controller, config, disconnect_rx, None, None)
        .await
}

/// Start dynamic forward with explicit disconnect receiver
pub async fn start_dynamic_forward_with_disconnect(
    handle_controller: HandleController,
    config: DynamicForward,
    mut disconnect_rx: broadcast::Receiver<()>,
    forward_id: Option<String>,
    event_emitter: Option<ForwardEventEmitter>,
) -> Result<DynamicForwardHandle, SshError> {
    // Bind to local address
    let listener = TcpListener::bind(&config.local_addr)
        .await
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::AddrInUse => SshError::ConnectionFailed(format!(
                "Port already in use: {}. Another application may be using this port.",
                config.local_addr
            )),
            std::io::ErrorKind::PermissionDenied => SshError::ConnectionFailed(format!(
                "Permission denied binding to {}. Ports below 1024 require elevated privileges.",
                config.local_addr
            )),
            std::io::ErrorKind::AddrNotAvailable => SshError::ConnectionFailed(format!(
                "Address not available: {}. The specified address is not valid on this system.",
                config.local_addr
            )),
            _ => SshError::ConnectionFailed(format!(
                "Failed to bind SOCKS5 proxy to {}: {}",
                config.local_addr, e
            )),
        })?;

    let bound_addr = listener
        .local_addr()
        .map_err(|e| SshError::ConnectionFailed(format!("Failed to get bound address: {}", e)))?;

    info!("Started SOCKS5 proxy on {}", bound_addr);

    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();
    let (stop_tx, mut stop_rx) = mpsc::channel::<()>(1);
    let stats = Arc::new(parking_lot::RwLock::new(ForwardStats::default()));
    let stats_clone = stats.clone();

    // Create a broadcast channel for notifying child tasks of shutdown
    // This propagates disconnect/stop signals to all spawned SOCKS5 connection handlers
    let (child_shutdown_tx, _) = broadcast::channel::<()>(16);
    let child_shutdown_tx_clone = child_shutdown_tx.clone();

    // Spawn the proxy task
    tokio::spawn(async move {
        // Track exit reason for event emission
        #[allow(dead_code)]
        enum ExitReason {
            SshDisconnected,
            StopRequested,
            Error, // Reserved for future error handling
        }

        let exit_reason = loop {
            tokio::select! {
                // Handle SSH disconnect signal
                _ = disconnect_rx.recv() => {
                    info!("SOCKS5 proxy stopped: SSH disconnected");
                    break ExitReason::SshDisconnected;
                }

                // Handle stop signal
                _ = stop_rx.recv() => {
                    info!("SOCKS5 proxy stopped by request");
                    break ExitReason::StopRequested;
                }

                // Accept new connections
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, peer_addr)) => {
                            if !running_clone.load(Ordering::SeqCst) {
                                break ExitReason::StopRequested;
                            }

                            // Disable Nagle's algorithm for low-latency SOCKS5 proxy
                            if let Err(e) = stream.set_nodelay(true) {
                                warn!("Failed to set TCP_NODELAY: {}", e);
                            }

                            debug!("SOCKS5: Accepted connection from {}", peer_addr);

                            // Update stats
                            {
                                let mut s = stats_clone.write();
                                s.connection_count += 1;
                                s.active_connections += 1;
                            }

                            let controller = handle_controller.clone();
                            let stats_for_conn = stats_clone.clone();
                            // Subscribe to shutdown signal for this child task
                            let mut child_shutdown_rx = child_shutdown_tx_clone.subscribe();

                            // Spawn a task to handle this SOCKS5 connection
                            tokio::spawn(async move {
                                let result = handle_socks5_connection(
                                    controller,
                                    stream,
                                    stats_for_conn.clone(),
                                    &mut child_shutdown_rx,
                                ).await;

                                // Decrement active connections when done
                                {
                                    let mut s = stats_for_conn.write();
                                    s.active_connections = s.active_connections.saturating_sub(1);
                                }

                                if let Err(e) = result {
                                    warn!("SOCKS5 connection error from {}: {}", peer_addr, e);
                                }
                            });
                        }
                        Err(e) => {
                            error!("SOCKS5 accept error: {}", e);
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        }
                    }
                }
            }
        };

        running_clone.store(false, Ordering::SeqCst);

        // Signal all child tasks to shutdown
        // Ignore error if no receivers (all connections already closed)
        let _ = child_shutdown_tx.send(());

        // Emit status event based on exit reason
        if let (Some(ref emitter), Some(ref fwd_id)) = (&event_emitter, &forward_id) {
            match exit_reason {
                ExitReason::SshDisconnected => {
                    emitter.emit_status_changed(
                        fwd_id,
                        ForwardStatus::Suspended,
                        Some("SSH connection lost".into()),
                    );
                }
                ExitReason::Error => {
                    emitter.emit_status_changed(
                        fwd_id,
                        ForwardStatus::Error,
                        Some("SOCKS5 proxy error".into()),
                    );
                }
                ExitReason::StopRequested => {
                    // Stopped by user request, manager already handles this
                }
            }
        }

        info!("SOCKS5 proxy task exited");
    });

    Ok(DynamicForwardHandle {
        config,
        bound_addr,
        running,
        stop_tx,
        stats,
    })
}

/// Handle a single SOCKS5 connection
async fn handle_socks5_connection(
    handle_controller: HandleController,
    mut stream: TcpStream,
    stats: Arc<parking_lot::RwLock<ForwardStats>>,
    shutdown_rx: &mut broadcast::Receiver<()>,
) -> Result<(), SshError> {
    // Phase 1: Authentication negotiation
    let mut buf = [0u8; 258];

    // Read version and auth method count
    stream.read_exact(&mut buf[..2]).await.map_err(|e| {
        SshError::ConnectionFailed(format!("Failed to read SOCKS5 greeting: {}", e))
    })?;

    let version = buf[0];
    let nmethods = buf[1] as usize;

    if version != socks5::VERSION {
        return Err(SshError::ConnectionFailed(format!(
            "Unsupported SOCKS version: {}",
            version
        )));
    }

    // Read auth methods
    stream
        .read_exact(&mut buf[..nmethods])
        .await
        .map_err(|e| SshError::ConnectionFailed(format!("Failed to read auth methods: {}", e)))?;

    // Check if NO AUTH is supported
    let no_auth_supported = buf[..nmethods].contains(&socks5::AUTH_NONE);
    if !no_auth_supported {
        // Send auth failure
        stream.write_all(&[socks5::VERSION, 0xFF]).await.ok();
        return Err(SshError::ConnectionFailed(
            "Client doesn't support NO AUTH method".into(),
        ));
    }

    // Send auth success (no auth required)
    stream
        .write_all(&[socks5::VERSION, socks5::AUTH_NONE])
        .await
        .map_err(|e| SshError::ConnectionFailed(format!("Failed to send auth response: {}", e)))?;

    // Phase 2: Connection request
    stream
        .read_exact(&mut buf[..4])
        .await
        .map_err(|e| SshError::ConnectionFailed(format!("Failed to read SOCKS5 request: {}", e)))?;

    let version = buf[0];
    let cmd = buf[1];
    // buf[2] is reserved
    let atyp = buf[3];

    if version != socks5::VERSION {
        return Err(SshError::ConnectionFailed(
            "Invalid SOCKS5 version in request".into(),
        ));
    }

    if cmd != socks5::CMD_CONNECT {
        // Only CONNECT is supported
        send_socks5_reply(&mut stream, socks5::REP_CMD_NOT_SUPPORTED).await?;
        return Err(SshError::ConnectionFailed(format!(
            "Unsupported SOCKS5 command: {}",
            cmd
        )));
    }

    // Parse destination address
    let (dest_host, dest_port) = match atyp {
        socks5::ATYP_IPV4 => {
            stream.read_exact(&mut buf[..6]).await.map_err(|e| {
                SshError::ConnectionFailed(format!("Failed to read IPv4 address: {}", e))
            })?;
            let ip = std::net::Ipv4Addr::new(buf[0], buf[1], buf[2], buf[3]);
            let port = u16::from_be_bytes([buf[4], buf[5]]);
            (ip.to_string(), port)
        }
        socks5::ATYP_DOMAIN => {
            stream.read_exact(&mut buf[..1]).await.map_err(|e| {
                SshError::ConnectionFailed(format!("Failed to read domain length: {}", e))
            })?;
            let domain_len = buf[0] as usize;
            stream
                .read_exact(&mut buf[..domain_len + 2])
                .await
                .map_err(|e| SshError::ConnectionFailed(format!("Failed to read domain: {}", e)))?;
            let domain = String::from_utf8_lossy(&buf[..domain_len]).to_string();
            let port = u16::from_be_bytes([buf[domain_len], buf[domain_len + 1]]);
            (domain, port)
        }
        socks5::ATYP_IPV6 => {
            stream.read_exact(&mut buf[..18]).await.map_err(|e| {
                SshError::ConnectionFailed(format!("Failed to read IPv6 address: {}", e))
            })?;
            let ip = std::net::Ipv6Addr::new(
                u16::from_be_bytes([buf[0], buf[1]]),
                u16::from_be_bytes([buf[2], buf[3]]),
                u16::from_be_bytes([buf[4], buf[5]]),
                u16::from_be_bytes([buf[6], buf[7]]),
                u16::from_be_bytes([buf[8], buf[9]]),
                u16::from_be_bytes([buf[10], buf[11]]),
                u16::from_be_bytes([buf[12], buf[13]]),
                u16::from_be_bytes([buf[14], buf[15]]),
            );
            let port = u16::from_be_bytes([buf[16], buf[17]]);
            (ip.to_string(), port)
        }
        _ => {
            send_socks5_reply(&mut stream, socks5::REP_ADDR_NOT_SUPPORTED).await?;
            return Err(SshError::ConnectionFailed(format!(
                "Unsupported address type: {}",
                atyp
            )));
        }
    };

    debug!("SOCKS5: Connecting to {}:{}", dest_host, dest_port);

    // Open SSH direct-tcpip channel to destination via Handle Owner Task
    let peer_addr = stream
        .peer_addr()
        .map_or("127.0.0.1".to_string(), |a| a.ip().to_string());
    let peer_port = stream.peer_addr().map_or(0, |a| a.port() as u32);

    let channel = match handle_controller
        .open_direct_tcpip(&dest_host, dest_port as u32, &peer_addr, peer_port)
        .await
    {
        Ok(ch) => ch,
        Err(e) => {
            warn!(
                "Failed to open SSH channel to {}:{}: {:?}",
                dest_host, dest_port, e
            );
            send_socks5_reply(&mut stream, socks5::REP_HOST_UNREACHABLE).await?;
            return Err(e);
        }
    };

    // Send success reply
    send_socks5_reply(&mut stream, socks5::REP_SUCCESS).await?;

    debug!("SOCKS5: Tunnel established to {}:{}", dest_host, dest_port);

    // Bridge the connection
    bridge_socks5_connection(stream, channel, stats, shutdown_rx).await
}

/// Send a SOCKS5 reply
async fn send_socks5_reply(stream: &mut TcpStream, status: u8) -> Result<(), SshError> {
    // VER | REP | RSV | ATYP | BND.ADDR | BND.PORT
    // For simplicity, we always return 0.0.0.0:0 as bound address
    let reply = [
        socks5::VERSION,
        status,
        0x00, // Reserved
        socks5::ATYP_IPV4,
        0,
        0,
        0,
        0, // 0.0.0.0
        0,
        0, // Port 0
    ];

    stream
        .write_all(&reply)
        .await
        .map_err(|e| SshError::ConnectionFailed(format!("Failed to send SOCKS5 reply: {}", e)))
}

/// Idle timeout for SOCKS5 connections (5 minutes)
const SOCKS5_IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// Bridge data between SOCKS5 client and SSH channel
///
/// # Architecture: Lock-Free Channel I/O
///
/// Uses the same message-passing pattern as local.rs and remote.rs to avoid lock contention.
/// A single task owns the SSH Channel, communicating with read/write tasks via mpsc.
async fn bridge_socks5_connection(
    mut local_stream: TcpStream,
    mut channel: russh::Channel<russh::client::Msg>,
    stats: Arc<parking_lot::RwLock<ForwardStats>>,
    shutdown_rx: &mut broadcast::Receiver<()>,
) -> Result<(), SshError> {
    let (mut local_read, mut local_write) = local_stream.split();

    // Create internal channels for lock-free data flow
    let (local_to_ssh_tx, mut local_to_ssh_rx) = mpsc::channel::<Vec<u8>>(32);
    let (ssh_to_local_tx, mut ssh_to_local_rx) = mpsc::channel::<Vec<u8>>(32);

    // Control signals for clean shutdown
    let (close_tx, _) = broadcast::channel::<()>(1);
    let mut close_rx1 = close_tx.subscribe();
    let mut close_rx2 = close_tx.subscribe();

    let stats_for_send = stats.clone();
    let stats_for_recv = stats.clone();

    // Task 1: Read from local socket, send to mpsc channel
    let local_reader = async move {
        let mut buf = vec![0u8; 32768];
        loop {
            tokio::select! {
                biased;

                _ = close_rx1.recv() => {
                    debug!("SOCKS5 local reader: received close signal");
                    break;
                }

                result = tokio::time::timeout(SOCKS5_IDLE_TIMEOUT, local_read.read(&mut buf)) => {
                    match result {
                        Ok(Ok(0)) => {
                            debug!("SOCKS5 local reader: EOF");
                            break;
                        }
                        Ok(Ok(n)) => {
                            stats_for_send.write().bytes_sent += n as u64;
                            if local_to_ssh_tx.send(buf[..n].to_vec()).await.is_err() {
                                debug!("SOCKS5 local reader: channel closed");
                                break;
                            }
                        }
                        Ok(Err(e)) => {
                            debug!("SOCKS5 local reader: error {}", e);
                            break;
                        }
                        Err(_) => {
                            debug!("SOCKS5 local reader: idle timeout ({}s)", SOCKS5_IDLE_TIMEOUT.as_secs());
                            break;
                        }
                    }
                }
            }
        }
    };

    // Task 2: Read from mpsc channel, write to local socket
    let local_writer = async move {
        loop {
            tokio::select! {
                biased;

                _ = close_rx2.recv() => {
                    debug!("SOCKS5 local writer: received close signal");
                    break;
                }

                data = ssh_to_local_rx.recv() => {
                    match data {
                        Some(data) => {
                            if let Err(e) = local_write.write_all(&data).await {
                                debug!("SOCKS5 local writer: error {}", e);
                                break;
                            }
                        }
                        None => {
                            debug!("SOCKS5 local writer: channel closed");
                            break;
                        }
                    }
                }
            }
        }
    };

    // Task 3: SSH channel I/O loop (single owner of Channel, no mutex needed)
    let ssh_io = async move {
        loop {
            tokio::select! {
                biased;

                // Priority 1: Send data to SSH channel
                data = local_to_ssh_rx.recv() => {
                    match data {
                        Some(data) => {
                            if let Err(e) = channel.data(&data[..]).await {
                                debug!("SOCKS5 SSH I/O: send error {}", e);
                                break;
                            }
                        }
                        None => {
                            debug!("SOCKS5 SSH I/O: local reader closed, sending EOF");
                            let _ = channel.eof().await;
                            break;
                        }
                    }
                }

                // Priority 2: Receive data from SSH channel (with timeout)
                result = tokio::time::timeout(SOCKS5_IDLE_TIMEOUT, channel.wait()) => {
                    match result {
                        Ok(Some(russh::ChannelMsg::Data { data })) => {
                            let data_len = data.len();
                            stats_for_recv.write().bytes_received += data_len as u64;
                            if ssh_to_local_tx.send(data.to_vec()).await.is_err() {
                                debug!("SOCKS5 SSH I/O: local writer closed");
                                break;
                            }
                        }
                        Ok(Some(russh::ChannelMsg::Eof)) => {
                            debug!("SOCKS5 SSH I/O: received EOF");
                            break;
                        }
                        Ok(Some(russh::ChannelMsg::Close)) => {
                            debug!("SOCKS5 SSH I/O: channel closed by remote");
                            break;
                        }
                        Ok(None) => {
                            debug!("SOCKS5 SSH I/O: channel ended");
                            break;
                        }
                        Ok(_) => continue,
                        Err(_) => {
                            debug!("SOCKS5 SSH I/O: idle timeout ({}s)", SOCKS5_IDLE_TIMEOUT.as_secs());
                            break;
                        }
                    }
                }
            }
        }

        // Cleanup: close the channel
        let _ = channel.close().await;
    };

    // Resubscribe to get a fresh receiver for this select
    let mut shutdown_rx_clone = shutdown_rx.resubscribe();

    // Run all tasks concurrently, exit when any completes (including parent shutdown)
    tokio::select! {
        _ = local_reader => {}
        _ = local_writer => {}
        _ = ssh_io => {}
        _ = shutdown_rx_clone.recv() => {
            debug!("SOCKS5 bridge: received shutdown signal from parent");
        }
    }

    // Signal all tasks to close
    let _ = close_tx.send(());

    debug!("SOCKS5 connection closed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dynamic_forward_config() {
        let forward = DynamicForward::new("127.0.0.1:1080");
        assert_eq!(forward.local_addr, "127.0.0.1:1080");
        assert!(forward.description.is_none());
    }

    #[test]
    fn test_dynamic_forward_default() {
        let forward = DynamicForward::default_port();
        assert_eq!(forward.local_addr, "127.0.0.1:1080");
        assert!(forward.description.is_some());
    }

    #[test]
    fn test_dynamic_forward_with_description() {
        let forward = DynamicForward::new("127.0.0.1:9050").with_description("Tor-like proxy");
        assert!(forward.description.unwrap().contains("Tor"));
    }
}
