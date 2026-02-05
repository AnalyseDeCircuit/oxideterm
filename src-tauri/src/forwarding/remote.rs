//! Remote Port Forwarding
//!
//! Forwards connections from a remote port back to a local host:port through SSH.
//! Example: Remote server:9000 -> local:3000 (expose local service to remote)
//!
//! ## Architecture
//!
//! Remote forwarding requires coordination between:
//! 1. The SSH client (sends `tcpip-forward` request via HandleController)
//! 2. The SSH server (listens on remote port)
//! 3. The ClientHandler callback (handles incoming `forwarded-tcpip` channels)
//!
//! We use a global registry to store the mapping from (address, port) -> local target,
//! which the ClientHandler can look up when it receives a forwarded connection.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use once_cell::sync::Lazy;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{debug, info, warn};

use crate::ssh::{HandleController, SshError};
use super::events::ForwardEventEmitter;
use super::manager::ForwardStatus;

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

/// Remote port forwarding configuration
#[derive(Debug, Clone)]
pub struct RemoteForward {
    /// Remote bind address (e.g., "0.0.0.0" or "localhost")
    pub remote_addr: String,
    /// Remote port to bind on the server
    pub remote_port: u16,
    /// Local host to connect to (e.g., "localhost")
    pub local_host: String,
    /// Local port to connect to
    pub local_port: u16,
    /// Description for UI display
    pub description: Option<String>,
}

impl RemoteForward {
    /// Create a new remote port forward
    pub fn new(
        remote_addr: impl Into<String>,
        remote_port: u16,
        local_host: impl Into<String>,
        local_port: u16,
    ) -> Self {
        Self {
            remote_addr: remote_addr.into(),
            remote_port,
            local_host: local_host.into(),
            local_port,
            description: None,
        }
    }

    /// Create a simple remote forward from remote port to local port
    pub fn simple(remote_port: u16, local_port: u16) -> Self {
        Self {
            remote_addr: "localhost".into(),
            remote_port,
            local_host: "localhost".into(),
            local_port,
            description: None,
        }
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }
}

/// Target configuration for a remote forward
#[derive(Debug, Clone)]
pub struct RemoteForwardTarget {
    pub local_host: String,
    pub local_port: u16,
    /// Stats tracking using atomics for lock-free updates from async handlers
    pub stats: Arc<RemoteForwardStatsAtomic>,
}

/// Atomic stats for remote forwards (used for thread-safe updates from callbacks)
#[derive(Debug, Default)]
pub struct RemoteForwardStatsAtomic {
    pub connection_count: AtomicU64,
    pub active_connections: AtomicU64,
    pub bytes_sent: AtomicU64,
    pub bytes_received: AtomicU64,
}

impl RemoteForwardStatsAtomic {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn to_stats(&self) -> ForwardStats {
        ForwardStats {
            connection_count: self.connection_count.load(Ordering::Relaxed),
            active_connections: self.active_connections.load(Ordering::Relaxed),
            bytes_sent: self.bytes_sent.load(Ordering::Relaxed),
            bytes_received: self.bytes_received.load(Ordering::Relaxed),
        }
    }
}

/// Global registry for remote forward configurations
///
/// This registry maps (remote_address, remote_port) -> local target.
/// It's used by the ClientHandler to look up where to connect when
/// a forwarded-tcpip channel is opened by the server.
pub struct RemoteForwardRegistry {
    /// Map from (address, port) to local target
    forwards: RwLock<HashMap<(String, u16), RemoteForwardTarget>>,
}

impl RemoteForwardRegistry {
    /// Create a new registry
    pub fn new() -> Self {
        Self {
            forwards: RwLock::new(HashMap::new()),
        }
    }

    /// Register a remote forward
    pub async fn register(
        &self,
        remote_addr: String,
        remote_port: u16,
        local_host: String,
        local_port: u16,
    ) -> Arc<RemoteForwardStatsAtomic> {
        let key = (remote_addr.clone(), remote_port);
        let stats = Arc::new(RemoteForwardStatsAtomic::new());
        let target = RemoteForwardTarget {
            local_host,
            local_port,
            stats: stats.clone(),
        };
        self.forwards.write().await.insert(key, target);
        debug!(
            "Registered remote forward: {}:{} -> target",
            remote_addr, remote_port
        );
        stats
    }

    /// Unregister a remote forward
    pub async fn unregister(&self, remote_addr: &str, remote_port: u16) {
        let key = (remote_addr.to_string(), remote_port);
        self.forwards.write().await.remove(&key);
        debug!(
            "Unregistered remote forward: {}:{}",
            remote_addr, remote_port
        );
    }

    /// Look up a remote forward target
    pub async fn lookup(&self, remote_addr: &str, remote_port: u16) -> Option<RemoteForwardTarget> {
        let key = (remote_addr.to_string(), remote_port);
        self.forwards.read().await.get(&key).cloned()
    }

    /// Check if a forward exists
    #[allow(dead_code)]
    pub async fn exists(&self, remote_addr: &str, remote_port: u16) -> bool {
        let key = (remote_addr.to_string(), remote_port);
        self.forwards.read().await.contains_key(&key)
    }
}

impl Default for RemoteForwardRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Global instance of the remote forward registry
pub static REMOTE_FORWARD_REGISTRY: Lazy<RemoteForwardRegistry> =
    Lazy::new(RemoteForwardRegistry::new);

/// Handle to a running remote port forward
pub struct RemoteForwardHandle {
    /// Forward configuration
    pub config: RemoteForward,
    /// Actual bound port on the server (may differ if original was 0)
    pub bound_port: u16,
    /// Flag to indicate if running
    running: Arc<AtomicBool>,
    /// Channel to signal stop
    stop_tx: mpsc::Sender<()>,
    /// Handle controller for cancellation
    handle_controller: HandleController,
    /// Stats tracking
    stats: Arc<RemoteForwardStatsAtomic>,
}

impl RemoteForwardHandle {
    /// Stop the port forwarding and wait for cleanup
    pub async fn stop(&self) {
        info!(
            "Stopping remote port forward {}:{}",
            self.config.remote_addr, self.bound_port
        );
        self.running.store(false, Ordering::SeqCst);

        // Cancel the forward on the server via Handle Owner Task
        if let Err(e) = self
            .handle_controller
            .cancel_tcpip_forward(&self.config.remote_addr, self.bound_port as u32)
            .await
        {
            warn!("Failed to cancel remote forward: {:?}", e);
        }

        // Unregister from the global registry
        REMOTE_FORWARD_REGISTRY
            .unregister(&self.config.remote_addr, self.bound_port)
            .await;

        let _ = self.stop_tx.send(()).await;
        
        // 等待活跃连接关闭（最多等待 5 秒）
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(5);
        while self.stats.active_connections.load(Ordering::SeqCst) > 0 {
            if start.elapsed() > timeout {
                warn!(
                    "Timeout waiting for {} active connections to close on {}:{}",
                    self.stats.active_connections.load(Ordering::SeqCst),
                    self.config.remote_addr,
                    self.bound_port
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

    /// Get current stats
    pub fn stats(&self) -> ForwardStats {
        self.stats.to_stats()
    }
}

/// Start remote port forwarding
///
/// This function:
/// 1. Sends a tcpip-forward request to the SSH server via Handle Owner Task
/// 2. Registers the forward in the global registry
/// 3. Returns a handle that can be used to stop the forward
///
/// The actual forwarding happens in the ClientHandler callback when
/// the server opens a forwarded-tcpip channel.
pub async fn start_remote_forward(
    handle_controller: HandleController,
    config: RemoteForward,
) -> Result<RemoteForwardHandle, SshError> {
    // Subscribe to disconnect notifications
    let disconnect_rx = handle_controller.subscribe_disconnect();
    start_remote_forward_with_disconnect(handle_controller, config, disconnect_rx, None, None).await
}

/// Start remote forward with explicit disconnect receiver and optional event emitter
pub async fn start_remote_forward_with_disconnect(
    handle_controller: HandleController,
    config: RemoteForward,
    mut disconnect_rx: broadcast::Receiver<()>,
    forward_id: Option<String>,
    event_emitter: Option<ForwardEventEmitter>,
) -> Result<RemoteForwardHandle, SshError> {
    info!(
        "Requesting remote port forward: {}:{} -> {}:{}",
        config.remote_addr, config.remote_port, config.local_host, config.local_port
    );

    // Request the server to listen on the remote port via Handle Owner Task
    // The tcpip_forward is called through message passing, avoiding &mut Handle issues
    let actual_port = handle_controller
        .tcpip_forward(&config.remote_addr, config.remote_port as u32)
        .await?;

    info!(
        "Remote forward established: {}:{} (requested {}) -> {}:{}",
        config.remote_addr, actual_port, config.remote_port, config.local_host, config.local_port
    );

    // Register in the global registry so ClientHandler can find the target
    // This also returns the stats Arc for tracking
    let stats = REMOTE_FORWARD_REGISTRY
        .register(
            config.remote_addr.clone(),
            actual_port as u16,
            config.local_host.clone(),
            config.local_port,
        )
        .await;

    let running = Arc::new(AtomicBool::new(true));
    let (stop_tx, mut stop_rx) = mpsc::channel::<()>(1);
    let running_clone = running.clone();
    let remote_addr_clone = config.remote_addr.clone();
    let bound_port_clone = actual_port as u16;

    // Spawn a monitoring task that listens for stop signal or SSH disconnect
    tokio::spawn(async move {
        enum ExitReason {
            StopRequested,
            SshDisconnected,
        }
        
        let exit_reason = tokio::select! {
            _ = stop_rx.recv() => {
                info!("Remote port forward stopped by request");
                ExitReason::StopRequested
            }
            _ = disconnect_rx.recv() => {
                info!("Remote port forward stopped: SSH disconnected");
                ExitReason::SshDisconnected
            }
        };
        running_clone.store(false, Ordering::SeqCst);
        
        // Unregister from registry on exit
        REMOTE_FORWARD_REGISTRY
            .unregister(&remote_addr_clone, bound_port_clone)
            .await;
        
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
                ExitReason::StopRequested => {
                    // Stopped by user request, manager already handles this
                }
            }
        }
        
        info!("Remote port forward monitor task exited");
    });

    Ok(RemoteForwardHandle {
        config,
        bound_port: actual_port as u16,
        running,
        stop_tx,
        handle_controller,
        stats,
    })
}

/// Handle a forwarded connection from the remote server.
///
/// This is called by the ClientHandler when the server opens a forwarded-tcpip channel.
/// It looks up the target in the global registry and bridges the connection.
pub async fn handle_forwarded_connection(
    channel: russh::Channel<russh::client::Msg>,
    connected_address: &str,
    connected_port: u32,
    originator_address: &str,
    originator_port: u32,
) -> Result<(), SshError> {
    debug!(
        "Handling forwarded connection: {}:{} from {}:{}",
        connected_address, connected_port, originator_address, originator_port
    );

    // Look up the target from the registry
    let target = REMOTE_FORWARD_REGISTRY
        .lookup(connected_address, connected_port as u16)
        .await
        .ok_or_else(|| {
            SshError::ConnectionFailed(format!(
                "No registered forward for {}:{}",
                connected_address, connected_port
            ))
        })?;

    // Update connection stats
    target
        .stats
        .connection_count
        .fetch_add(1, Ordering::Relaxed);
    target
        .stats
        .active_connections
        .fetch_add(1, Ordering::Relaxed);
    let stats = target.stats.clone();

    // Connect to local service
    let local_addr = format!("{}:{}", target.local_host, target.local_port);
    let local_stream = TcpStream::connect(&local_addr).await.map_err(|e| {
        // Decrement active connections on connection failure
        stats.active_connections.fetch_sub(1, Ordering::Relaxed);
        SshError::ConnectionFailed(format!("Failed to connect to {}: {}", local_addr, e))
    })?;

    // Disable Nagle's algorithm for low-latency forwarding
    if let Err(e) = local_stream.set_nodelay(true) {
        warn!("Failed to set TCP_NODELAY: {}", e);
    }

    info!(
        "Bridging forwarded connection {}:{} -> {}",
        connected_address, connected_port, local_addr
    );

    // Bridge the connection
    let result = bridge_forwarded_connection(local_stream, channel, stats.clone()).await;

    // Decrement active connections when done
    stats.active_connections.fetch_sub(1, Ordering::Relaxed);

    result
}

/// Idle timeout for remote forwarded connections (5 minutes)
const REMOTE_FORWARD_IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// Bridge data between local socket and SSH channel
/// 
/// # Architecture: Lock-Free Channel I/O with Timeout Protection
/// 
/// Uses the same message-passing pattern as local.rs to avoid lock contention.
/// A single task owns the SSH Channel, communicating with read/write tasks via mpsc.
/// 
/// Key improvements over the original Arc<Mutex<Channel>> approach:
/// 1. No lock contention between concurrent read/write operations
/// 2. Explicit timeout on all I/O operations (protects against zombie connections)
/// 3. Clean shutdown propagation via broadcast channel
async fn bridge_forwarded_connection(
    mut local_stream: TcpStream,
    mut channel: russh::Channel<russh::client::Msg>,
    stats: Arc<RemoteForwardStatsAtomic>,
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
                    debug!("Remote forward local reader: received close signal");
                    break;
                }
                
                result = tokio::time::timeout(REMOTE_FORWARD_IDLE_TIMEOUT, local_read.read(&mut buf)) => {
                    match result {
                        Ok(Ok(0)) => {
                            debug!("Remote forward local reader: EOF");
                            break;
                        }
                        Ok(Ok(n)) => {
                            stats_for_send.bytes_sent.fetch_add(n as u64, Ordering::Relaxed);
                            if local_to_ssh_tx.send(buf[..n].to_vec()).await.is_err() {
                                debug!("Remote forward local reader: channel closed");
                                break;
                            }
                        }
                        Ok(Err(e)) => {
                            debug!("Remote forward local reader: error {}", e);
                            break;
                        }
                        Err(_) => {
                            debug!("Remote forward local reader: idle timeout ({}s)", REMOTE_FORWARD_IDLE_TIMEOUT.as_secs());
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
                    debug!("Remote forward local writer: received close signal");
                    break;
                }
                
                data = ssh_to_local_rx.recv() => {
                    match data {
                        Some(data) => {
                            if let Err(e) = local_write.write_all(&data).await {
                                debug!("Remote forward local writer: error {}", e);
                                break;
                            }
                        }
                        None => {
                            debug!("Remote forward local writer: channel closed");
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
                                debug!("Remote forward SSH I/O: send error {}", e);
                                break;
                            }
                        }
                        None => {
                            debug!("Remote forward SSH I/O: local reader closed, sending EOF");
                            let _ = channel.eof().await;
                            break;
                        }
                    }
                }
                
                // Priority 2: Receive data from SSH channel (with timeout)
                result = tokio::time::timeout(REMOTE_FORWARD_IDLE_TIMEOUT, channel.wait()) => {
                    match result {
                        Ok(Some(russh::ChannelMsg::Data { data })) => {
                            let data_len = data.len();
                            stats_for_recv.bytes_received.fetch_add(data_len as u64, Ordering::Relaxed);
                            if ssh_to_local_tx.send(data.to_vec()).await.is_err() {
                                debug!("Remote forward SSH I/O: local writer closed");
                                break;
                            }
                        }
                        Ok(Some(russh::ChannelMsg::Eof)) => {
                            debug!("Remote forward SSH I/O: received EOF");
                            break;
                        }
                        Ok(Some(russh::ChannelMsg::Close)) => {
                            debug!("Remote forward SSH I/O: channel closed by remote");
                            break;
                        }
                        Ok(None) => {
                            debug!("Remote forward SSH I/O: channel ended");
                            break;
                        }
                        Ok(_) => continue,
                        Err(_) => {
                            debug!("Remote forward SSH I/O: idle timeout ({}s)", REMOTE_FORWARD_IDLE_TIMEOUT.as_secs());
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

    debug!("Remote forward connection closed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remote_forward_simple() {
        let forward = RemoteForward::simple(9000, 3000);
        assert_eq!(forward.remote_port, 9000);
        assert_eq!(forward.local_port, 3000);
        assert_eq!(forward.local_host, "localhost");
    }

    #[test]
    fn test_remote_forward_with_description() {
        let forward = RemoteForward::simple(8080, 8080).with_description("Web Server");
        assert!(forward.description.unwrap().contains("Web Server"));
    }

    #[tokio::test]
    async fn test_registry() {
        let registry = RemoteForwardRegistry::new();

        // Register
        registry
            .register("0.0.0.0".to_string(), 9000, "localhost".to_string(), 3000)
            .await;

        // Lookup
        let target = registry.lookup("0.0.0.0", 9000).await;
        assert!(target.is_some());
        let target = target.unwrap();
        assert_eq!(target.local_host, "localhost");
        assert_eq!(target.local_port, 3000);

        // Unregister
        registry.unregister("0.0.0.0", 9000).await;
        assert!(registry.lookup("0.0.0.0", 9000).await.is_none());
    }
}
