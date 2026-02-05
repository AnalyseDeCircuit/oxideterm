//! Local Port Forwarding
//!
//! Forwards connections from a local port to a remote host:port through SSH.
//! Example: Forward local:8888 -> remote_jupyter:8888

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

/// Local port forwarding configuration
#[derive(Debug, Clone)]
pub struct LocalForward {
    /// Local address to bind to (e.g., "127.0.0.1:8888")
    pub local_addr: String,
    /// Remote host to connect to through SSH (e.g., "localhost")
    pub remote_host: String,
    /// Remote port to connect to
    pub remote_port: u16,
    /// Description for UI display
    pub description: Option<String>,
}

impl LocalForward {
    /// Create a new local port forward
    pub fn new(
        local_addr: impl Into<String>,
        remote_host: impl Into<String>,
        remote_port: u16,
    ) -> Self {
        Self {
            local_addr: local_addr.into(),
            remote_host: remote_host.into(),
            remote_port,
            description: None,
        }
    }

    /// Create a Jupyter notebook forward (common HPC use case)
    pub fn jupyter(local_port: u16, remote_port: u16) -> Self {
        Self {
            local_addr: format!("127.0.0.1:{}", local_port),
            remote_host: "localhost".into(),
            remote_port,
            description: Some(format!("Jupyter Notebook (port {})", remote_port)),
        }
    }

    /// Create a TensorBoard forward (common ML use case)
    pub fn tensorboard(local_port: u16, remote_port: u16) -> Self {
        Self {
            local_addr: format!("127.0.0.1:{}", local_port),
            remote_host: "localhost".into(),
            remote_port,
            description: Some(format!("TensorBoard (port {})", remote_port)),
        }
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }
}

/// Statistics for a port forward
#[derive(Debug, Clone, Default)]
pub struct ForwardStats {
    /// Total connections handled
    pub connection_count: u64,
    /// Active connections right now
    pub active_connections: u64,
    /// Total bytes sent (client -> server)
    pub bytes_sent: u64,
    /// Total bytes received (server -> client)
    pub bytes_received: u64,
}

/// Handle to a running local port forward
pub struct LocalForwardHandle {
    /// Forward configuration
    pub config: LocalForward,
    /// Actual bound address (may differ from requested if port was 0)
    pub bound_addr: SocketAddr,
    /// Flag to stop the forwarding loop
    running: Arc<AtomicBool>,
    /// Channel to signal stop
    stop_tx: mpsc::Sender<()>,
    /// Connection statistics
    stats: Arc<parking_lot::RwLock<ForwardStats>>,
}

impl LocalForwardHandle {
    /// Stop the port forwarding and wait for active connections to close
    pub async fn stop(&self) {
        info!("Stopping local port forward on {}", self.bound_addr);
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

    /// Check if the forward is still running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Get current statistics
    pub fn stats(&self) -> ForwardStats {
        self.stats.read().clone()
    }
}

/// Start local port forwarding
///
/// This function spawns a background task that:
/// 1. Listens on the local address
/// 2. For each incoming connection, opens a direct-tcpip channel through SSH
/// 3. Bridges data between the local socket and the SSH channel
///
/// Uses HandleController to communicate with Handle Owner Task for opening channels.
///
/// # Arguments
/// * `handle_controller` - Controller for SSH operations
/// * `config` - Forward configuration
/// * `disconnect_rx` - Receiver for SSH disconnect notification (optional for backward compat)
pub async fn start_local_forward(
    handle_controller: HandleController,
    config: LocalForward,
) -> Result<LocalForwardHandle, SshError> {
    // Subscribe to disconnect notifications
    let disconnect_rx = handle_controller.subscribe_disconnect();
    start_local_forward_with_disconnect(handle_controller, config, disconnect_rx, None, None).await
}

/// Start local port forwarding with explicit disconnect receiver
pub async fn start_local_forward_with_disconnect(
    handle_controller: HandleController,
    config: LocalForward,
    mut disconnect_rx: broadcast::Receiver<()>,
    forward_id: Option<String>,
    event_emitter: Option<ForwardEventEmitter>,
) -> Result<LocalForwardHandle, SshError> {
    // Bind to local address
    let listener = TcpListener::bind(&config.local_addr).await.map_err(|e| {
        match e.kind() {
            std::io::ErrorKind::AddrInUse => {
                SshError::ConnectionFailed(format!(
                    "Port already in use: {}. Another application may be using this port.",
                    config.local_addr
                ))
            }
            std::io::ErrorKind::PermissionDenied => {
                SshError::ConnectionFailed(format!(
                    "Permission denied binding to {}. Ports below 1024 require elevated privileges.",
                    config.local_addr
                ))
            }
            std::io::ErrorKind::AddrNotAvailable => {
                SshError::ConnectionFailed(format!(
                    "Address not available: {}. The specified address is not valid on this system.",
                    config.local_addr
                ))
            }
            _ => SshError::ConnectionFailed(format!("Failed to bind to {}: {}", config.local_addr, e)),
        }
    })?;

    let bound_addr = listener
        .local_addr()
        .map_err(|e| SshError::ConnectionFailed(format!("Failed to get bound address: {}", e)))?;

    info!(
        "Started local port forward: {} -> {}:{}",
        bound_addr, config.remote_host, config.remote_port
    );

    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();
    let (stop_tx, mut stop_rx) = mpsc::channel::<()>(1);
    let stats = Arc::new(parking_lot::RwLock::new(ForwardStats::default()));
    let stats_clone = stats.clone();

    let remote_host = config.remote_host.clone();
    let remote_port = config.remote_port;

    // Spawn the forwarding task
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
                    info!("Local port forward stopped: SSH disconnected");
                    break ExitReason::SshDisconnected;
                }

                // Handle stop signal
                _ = stop_rx.recv() => {
                    info!("Local port forward stopped by request");
                    break ExitReason::StopRequested;
                }

                // Accept new connections
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, peer_addr)) => {
                            if !running_clone.load(Ordering::SeqCst) {
                                break ExitReason::StopRequested;
                            }

                            // Disable Nagle's algorithm for low-latency forwarding
                            if let Err(e) = stream.set_nodelay(true) {
                                warn!("Failed to set TCP_NODELAY: {}", e);
                            }

                            debug!("Accepted connection from {} for forward", peer_addr);

                            // Update stats
                            {
                                let mut s = stats_clone.write();
                                s.connection_count += 1;
                                s.active_connections += 1;
                            }

                            // Clone for the connection handler
                            let controller = handle_controller.clone();
                            let remote_host_clone = remote_host.clone();
                            let stats_for_conn = stats_clone.clone();

                            // Spawn a task to handle this connection
                            tokio::spawn(async move {
                                let result = handle_forward_connection(
                                    controller,
                                    stream,
                                    &remote_host_clone,
                                    remote_port,
                                    stats_for_conn.clone(),
                                ).await;

                                // Decrement active connections when done
                                {
                                    let mut s = stats_for_conn.write();
                                    s.active_connections = s.active_connections.saturating_sub(1);
                                }

                                if let Err(e) = result {
                                    warn!("Forward connection error: {}", e);
                                }
                            });
                        }
                        Err(e) => {
                            error!("Accept error: {}", e);
                            // Small delay before retrying
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        }
                    }
                }
            }
        };

        running_clone.store(false, Ordering::SeqCst);
        
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
                        Some("Forward task error".into()),
                    );
                }
                ExitReason::StopRequested => {
                    // Stopped by user request, manager already handles this
                }
            }
        }
        
        info!("Local port forward task exited");
    });

    Ok(LocalForwardHandle {
        config,
        bound_addr,
        running,
        stop_tx,
        stats,
    })
}

/// Handle a single forwarded connection
/// Idle timeout for forwarded connections (5 minutes)
const FORWARD_IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

async fn handle_forward_connection(
    handle_controller: HandleController,
    mut local_stream: TcpStream,
    remote_host: &str,
    remote_port: u16,
    stats: Arc<parking_lot::RwLock<ForwardStats>>,
) -> Result<(), SshError> {
    // Open direct-tcpip channel to remote via Handle Owner Task
    let channel = handle_controller
        .open_direct_tcpip(remote_host, remote_port as u32, "127.0.0.1", 0)
        .await?;

    debug!(
        "Opened channel for forward to {}:{}",
        remote_host, remote_port
    );

    // Bridge the connection
    // We need to handle data in both directions
    let (mut local_read, mut local_write) = local_stream.split();

    // Create a wrapper to handle the channel I/O
    let channel = Arc::new(tokio::sync::Mutex::new(channel));
    let channel_for_read = channel.clone();
    let channel_for_write = channel.clone();

    let stats_for_send = stats.clone();
    let stats_for_recv = stats.clone();

    // Local -> Remote task with idle timeout
    let local_to_remote = async {
        let mut buf = vec![0u8; 32768];
        loop {
            // Add idle timeout to local read
            match tokio::time::timeout(FORWARD_IDLE_TIMEOUT, local_read.read(&mut buf)).await {
                Ok(Ok(0)) => break, // EOF
                Ok(Ok(n)) => {
                    let ch = channel_for_write.lock().await;
                    if let Err(e) = ch.data(&buf[..n]).await {
                        debug!("Channel write error: {}", e);
                        break;
                    }
                    // Update bytes sent
                    stats_for_send.write().bytes_sent += n as u64;
                }
                Ok(Err(e)) => {
                    debug!("Local read error: {}", e);
                    break;
                }
                Err(_) => {
                    debug!("Local read idle timeout ({}s), closing forward connection", FORWARD_IDLE_TIMEOUT.as_secs());
                    break;
                }
            }
        }
        // Signal EOF to remote
        let ch = channel_for_write.lock().await;
        let _ = ch.eof().await;
    };

    // Remote -> Local task with idle timeout
    let remote_to_local = async {
        loop {
            let mut ch = channel_for_read.lock().await;
            // Add idle timeout to channel wait
            match tokio::time::timeout(FORWARD_IDLE_TIMEOUT, ch.wait()).await {
                Ok(Some(russh::ChannelMsg::Data { data })) => {
                    let data_len = data.len();
                    drop(ch); // Release lock before writing
                    if let Err(e) = local_write.write_all(&data).await {
                        debug!("Local write error: {}", e);
                        break;
                    }
                    // Update bytes received
                    stats_for_recv.write().bytes_received += data_len as u64;
                }
                Ok(Some(russh::ChannelMsg::Eof)) => {
                    debug!("Channel EOF received");
                    break;
                }
                Ok(Some(russh::ChannelMsg::Close)) => {
                    debug!("Channel closed");
                    break;
                }
                Ok(None) => {
                    debug!("Channel ended");
                    break;
                }
                Ok(_) => continue,
                Err(_) => {
                    debug!("Remote read idle timeout ({}s), closing forward connection", FORWARD_IDLE_TIMEOUT.as_secs());
                    break;
                }
            }
        }
    };

    // Run both directions concurrently
    tokio::select! {
        _ = local_to_remote => {}
        _ = remote_to_local => {}
    }

    // Close the channel
    {
        let ch = channel.lock().await;
        let _ = ch.close().await;
    }

    debug!("Forward connection closed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jupyter_forward() {
        let forward = LocalForward::jupyter(8888, 8888);
        assert_eq!(forward.local_addr, "127.0.0.1:8888");
        assert_eq!(forward.remote_host, "localhost");
        assert_eq!(forward.remote_port, 8888);
        assert!(forward.description.unwrap().contains("Jupyter"));
    }

    #[test]
    fn test_tensorboard_forward() {
        let forward = LocalForward::tensorboard(6006, 6006);
        assert_eq!(forward.local_addr, "127.0.0.1:6006");
        assert_eq!(forward.remote_port, 6006);
        assert!(forward.description.unwrap().contains("TensorBoard"));
    }
}
