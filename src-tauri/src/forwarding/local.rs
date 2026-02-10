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
                "Failed to bind to {}: {}",
                config.local_addr, e
            )),
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

    // Create a broadcast channel for notifying child tasks of shutdown
    // This propagates the disconnect signal to all spawned connection handlers
    let (child_shutdown_tx, _) = broadcast::channel::<()>(16);
    let child_shutdown_tx_clone = child_shutdown_tx.clone();

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
                            // Subscribe to shutdown signal for this child task
                            let mut child_shutdown_rx = child_shutdown_tx_clone.subscribe();

                            // Spawn a task to handle this connection
                            tokio::spawn(async move {
                                let result = handle_forward_connection(
                                    controller,
                                    stream,
                                    &remote_host_clone,
                                    remote_port,
                                    stats_for_conn.clone(),
                                    &mut child_shutdown_rx,
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

/// Idle timeout for forwarded connections (5 minutes)
const FORWARD_IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// Handle a single forwarded connection
///
/// # Architecture: Lock-Free Channel I/O
///
/// Instead of wrapping the russh Channel in `Arc<Mutex<Channel>>` (which causes lock contention
/// when both read and write tasks compete for the mutex), we use a message-passing approach:
///
/// 1. A dedicated "channel reader" task owns the Channel and reads from SSH
/// 2. Data flows through mpsc channels: local_read -> SSH, SSH -> local_write
/// 3. The shutdown signal propagates to all tasks via broadcast channel
///
/// This eliminates:
/// - Lock contention between read/write paths
/// - Potential deadlocks from holding locks across `.await`
/// - The need to manually manage lock ordering
async fn handle_forward_connection(
    handle_controller: HandleController,
    mut local_stream: TcpStream,
    remote_host: &str,
    remote_port: u16,
    stats: Arc<parking_lot::RwLock<ForwardStats>>,
    shutdown_rx: &mut broadcast::Receiver<()>,
) -> Result<(), SshError> {
    // Open direct-tcpip channel to remote via Handle Owner Task
    let mut channel = handle_controller
        .open_direct_tcpip(remote_host, remote_port as u32, "127.0.0.1", 0)
        .await?;

    debug!(
        "Opened channel for forward to {}:{}",
        remote_host, remote_port
    );

    // Split local stream for concurrent read/write
    let (mut local_read, mut local_write) = local_stream.split();

    // Create internal channels for lock-free data flow
    // local_to_ssh_tx: data read from local socket, to be sent to SSH
    // ssh_to_local_tx: data read from SSH channel, to be sent to local socket
    let (local_to_ssh_tx, mut local_to_ssh_rx) = mpsc::channel::<Vec<u8>>(32);
    let (ssh_to_local_tx, mut ssh_to_local_rx) = mpsc::channel::<Vec<u8>>(32);

    // Control signals
    let (close_tx, _) = broadcast::channel::<()>(1);
    let mut close_rx1 = close_tx.subscribe();
    let mut close_rx2 = close_tx.subscribe();
    let mut shutdown_rx_clone = shutdown_rx.resubscribe();

    let stats_for_send = stats.clone();
    let stats_for_recv = stats.clone();

    // Task 1: Read from local socket, send to mpsc channel
    let local_reader = async move {
        let mut buf = vec![0u8; 32768];
        loop {
            tokio::select! {
                biased;

                _ = close_rx1.recv() => {
                    debug!("Local reader: received close signal");
                    break;
                }

                result = tokio::time::timeout(FORWARD_IDLE_TIMEOUT, local_read.read(&mut buf)) => {
                    match result {
                        Ok(Ok(0)) => {
                            debug!("Local reader: EOF");
                            break;
                        }
                        Ok(Ok(n)) => {
                            stats_for_send.write().bytes_sent += n as u64;
                            if local_to_ssh_tx.send(buf[..n].to_vec()).await.is_err() {
                                debug!("Local reader: channel closed");
                                break;
                            }
                        }
                        Ok(Err(e)) => {
                            debug!("Local reader: error {}", e);
                            break;
                        }
                        Err(_) => {
                            debug!("Local reader: idle timeout ({}s)", FORWARD_IDLE_TIMEOUT.as_secs());
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
                    debug!("Local writer: received close signal");
                    break;
                }

                data = ssh_to_local_rx.recv() => {
                    match data {
                        Some(data) => {
                            if let Err(e) = local_write.write_all(&data).await {
                                debug!("Local writer: error {}", e);
                                break;
                            }
                        }
                        None => {
                            debug!("Local writer: channel closed");
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

                // Priority 1: Check for shutdown signal from parent
                _ = shutdown_rx_clone.recv() => {
                    debug!("SSH I/O: received shutdown signal");
                    break;
                }

                // Priority 2: Send data to SSH channel
                data = local_to_ssh_rx.recv() => {
                    match data {
                        Some(data) => {
                            if let Err(e) = channel.data(&data[..]).await {
                                debug!("SSH I/O: send error {}", e);
                                break;
                            }
                        }
                        None => {
                            debug!("SSH I/O: local reader closed, sending EOF");
                            let _ = channel.eof().await;
                            break;
                        }
                    }
                }

                // Priority 3: Receive data from SSH channel (with timeout)
                result = tokio::time::timeout(FORWARD_IDLE_TIMEOUT, channel.wait()) => {
                    match result {
                        Ok(Some(russh::ChannelMsg::Data { data })) => {
                            let data_len = data.len();
                            stats_for_recv.write().bytes_received += data_len as u64;
                            if ssh_to_local_tx.send(data.to_vec()).await.is_err() {
                                debug!("SSH I/O: local writer closed");
                                break;
                            }
                        }
                        Ok(Some(russh::ChannelMsg::Eof)) => {
                            debug!("SSH I/O: received EOF");
                            break;
                        }
                        Ok(Some(russh::ChannelMsg::Close)) => {
                            debug!("SSH I/O: channel closed by remote");
                            break;
                        }
                        Ok(None) => {
                            debug!("SSH I/O: channel ended");
                            break;
                        }
                        Ok(_) => continue,
                        Err(_) => {
                            debug!("SSH I/O: idle timeout ({}s)", FORWARD_IDLE_TIMEOUT.as_secs());
                            break;
                        }
                    }
                }
            }
        }

        // Cleanup: close the channel
        let _ = channel.close().await;
    };

    // Run all tasks concurrently, exit when any completes
    tokio::select! {
        _ = local_reader => {}
        _ = local_writer => {}
        _ = ssh_io => {}
    }

    // Signal all tasks to close
    let _ = close_tx.send(());

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
