//! Handle Owner Task
//!
//! This module implements the "single owner" pattern for SSH Handle.
//!
//! # Architecture
//!
//! Only one task owns the `Handle<ClientHandler>`. All other components
//! communicate with it via `HandleController` which sends commands through
//! an mpsc channel.
//!
//! This avoids:
//! - `Arc<Mutex<Handle>>` lock contention
//! - Deadlocks from holding locks across `.await`
//! - Protocol violations from concurrent Handle access
//!
//! # Usage
//!
//! ```ignore
//! let controller = spawn_handle_owner_task(handle, session_id);
//!
//! // Open a channel
//! let channel = controller.open_session_channel().await?;
//!
//! // Request remote forward
//! let bound_port = controller.tcpip_forward("0.0.0.0", 8080).await?;
//! ```

use russh::client::{Handle, Msg};
use russh::Channel;
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::{debug, info, warn};

use super::client::ClientHandler;
use super::error::SshError;

/// Ping 结果类型，区分不同的失败原因
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PingResult {
    /// 连接正常
    Ok,
    /// 超时（可能是网络延迟，可重试）
    Timeout,
    /// IO 错误（物理连接断开，应立即重连）
    IoError,
}

/// Commands sent to the Handle Owner Task
pub enum HandleCommand {
    /// Open a session channel (for PTY/shell)
    ChannelOpenSession {
        reply_tx: oneshot::Sender<Result<Channel<Msg>, russh::Error>>,
    },

    /// Open a direct-tcpip channel (for local forward / dynamic forward)
    ChannelOpenDirectTcpip {
        host: String,
        port: u32,
        originator_host: String,
        originator_port: u32,
        reply_tx: oneshot::Sender<Result<Channel<Msg>, russh::Error>>,
    },

    /// Request remote forward (tcpip-forward)
    TcpipForward {
        address: String,
        port: u32,
        reply_tx: oneshot::Sender<Result<u32, russh::Error>>,
    },

    /// Cancel remote forward
    CancelTcpipForward {
        address: String,
        port: u32,
        reply_tx: oneshot::Sender<Result<(), russh::Error>>,
    },

    /// Ping the connection (for keepalive check)
    Ping {
        reply_tx: oneshot::Sender<PingResult>,
    },

    /// Disconnect the SSH connection
    Disconnect,
}

/// Controller for sending commands to the Handle Owner Task
///
/// # Clone Semantics
///
/// `HandleController` implements `Clone`. This means:
/// - Any module holding a `HandleController` has **full SSH control**
/// - Can open any channel, create any forward, or disconnect
///
/// # Design Decision
///
/// This is **intentional**:
/// 1. **Simple passing**: No Arc needed, clone cost is low (just copies Sender)
/// 2. **Trust boundary**: Only in-process Rust code can obtain a Controller
/// 3. **Full capability**: SFTP, Forwarding, Shell all need full control
///
/// # Security Considerations
///
/// - **Do not** expose `HandleController` to untrusted code
/// - **Do not** serialize or pass across process boundaries
/// - Fine-grained permission control should be at upper layers (e.g., Tauri commands)
#[derive(Clone)]
pub struct HandleController {
    cmd_tx: mpsc::Sender<HandleCommand>,
    /// Broadcast sender for SSH disconnect notification.
    /// Subscribers (like port forwards) can listen for disconnection.
    disconnect_tx: broadcast::Sender<()>,
}

impl HandleController {
    /// Create a new HandleController with the given sender
    /// 
    /// This is primarily used for testing. In production, use `spawn_handle_owner_task`.
    pub fn new(cmd_tx: mpsc::Sender<HandleCommand>) -> Self {
        let (disconnect_tx, _) = broadcast::channel(1);
        Self { cmd_tx, disconnect_tx }
    }

    /// Subscribe to SSH disconnect notifications.
    /// 
    /// Returns a receiver that will receive `()` when the SSH connection is closed.
    /// Use this in `tokio::select!` to detect SSH disconnection.
    /// 
    /// # Example
    /// ```ignore
    /// let mut disconnect_rx = controller.subscribe_disconnect();
    /// tokio::select! {
    ///     _ = disconnect_rx.recv() => {
    ///         info!("SSH disconnected, stopping forward");
    ///         break;
    ///     }
    ///     // ... other branches
    /// }
    /// ```
    pub fn subscribe_disconnect(&self) -> broadcast::Receiver<()> {
        self.disconnect_tx.subscribe()
    }

    /// Open a session channel (for PTY/shell)
    pub async fn open_session_channel(&self) -> Result<Channel<Msg>, SshError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(HandleCommand::ChannelOpenSession { reply_tx })
            .await
            .map_err(|_| SshError::Disconnected)?;
        reply_rx
            .await
            .map_err(|_| SshError::Disconnected)?
            .map_err(|e| SshError::ChannelError(e.to_string()))
    }

    /// Open a direct-tcpip channel (for local forward / dynamic forward)
    pub async fn open_direct_tcpip(
        &self,
        host: &str,
        port: u32,
        originator_host: &str,
        originator_port: u32,
    ) -> Result<Channel<Msg>, SshError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(HandleCommand::ChannelOpenDirectTcpip {
                host: host.to_string(),
                port,
                originator_host: originator_host.to_string(),
                originator_port,
                reply_tx,
            })
            .await
            .map_err(|_| SshError::Disconnected)?;
        reply_rx
            .await
            .map_err(|_| SshError::Disconnected)?
            .map_err(|e| SshError::ChannelError(e.to_string()))
    }

    /// Request remote port forward (tcpip-forward)
    ///
    /// Returns the actual bound port (may differ if requested port was 0)
    pub async fn tcpip_forward(&self, address: &str, port: u32) -> Result<u32, SshError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(HandleCommand::TcpipForward {
                address: address.to_string(),
                port,
                reply_tx,
            })
            .await
            .map_err(|_| SshError::Disconnected)?;
        reply_rx
            .await
            .map_err(|_| SshError::Disconnected)?
            .map_err(|e| SshError::ConnectionFailed(e.to_string()))
    }

    /// Cancel a remote port forward
    pub async fn cancel_tcpip_forward(&self, address: &str, port: u32) -> Result<(), SshError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(HandleCommand::CancelTcpipForward {
                address: address.to_string(),
                port,
                reply_tx,
            })
            .await
            .map_err(|_| SshError::Disconnected)?;
        reply_rx
            .await
            .map_err(|_| SshError::Disconnected)?
            .map_err(|e| SshError::ConnectionFailed(e.to_string()))
    }

    /// Disconnect the SSH connection
    pub async fn disconnect(&self) {
        let _ = self.cmd_tx.send(HandleCommand::Disconnect).await;
    }

    /// Ping the connection (for keepalive check)
    /// Returns PingResult indicating connection status
    pub async fn ping(&self) -> PingResult {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self.cmd_tx.send(HandleCommand::Ping { reply_tx }).await.is_err() {
            return PingResult::IoError;
        }
        reply_rx.await.unwrap_or(PingResult::IoError)
    }

    /// Check if the Handle Owner Task is still running
    pub fn is_connected(&self) -> bool {
        !self.cmd_tx.is_closed()
    }
}

/// Spawn the Handle Owner Task
///
/// Consumes ownership of the Handle and returns a HandleController for sending commands.
///
/// # Arguments
///
/// * `handle` - The SSH Handle (ownership transferred to the task)
/// * `session_id` - Session ID for logging
///
/// # Returns
///
/// A `HandleController` that can be cloned and used to send commands.
pub fn spawn_handle_owner_task(
    handle: Handle<ClientHandler>,
    session_id: String,
) -> HandleController {
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<HandleCommand>(64);
    let (disconnect_tx, _) = broadcast::channel::<()>(1);
    let disconnect_tx_clone = disconnect_tx.clone();

    tokio::spawn(async move {
        let mut handle = handle; // Move into task, becomes sole owner

        info!("Handle owner task started for session {}", session_id);

        loop {
            match cmd_rx.recv().await {
                Some(cmd) => {
                    match cmd {
                        HandleCommand::ChannelOpenSession { reply_tx } => {
                            let result = handle.channel_open_session().await;
                            if reply_tx.send(result).is_err() {
                                warn!(
                                    "Caller dropped before receiving channel_open_session result"
                                );
                                // Channel will be dropped, SSH server will close it
                            }
                        }

                        HandleCommand::ChannelOpenDirectTcpip {
                            host,
                            port,
                            originator_host,
                            originator_port,
                            reply_tx,
                        } => {
                            let result = handle
                                .channel_open_direct_tcpip(
                                    &host,
                                    port,
                                    &originator_host,
                                    originator_port,
                                )
                                .await;
                            if reply_tx.send(result).is_err() {
                                warn!("Caller dropped before receiving direct_tcpip result");
                                // Channel will be dropped, SSH server will close it
                            }
                        }

                        HandleCommand::TcpipForward {
                            address,
                            port,
                            reply_tx,
                        } => {
                            let result = handle.tcpip_forward(&address, port).await;
                            match &result {
                                Ok(bound_port) => {
                                    let bound_port = *bound_port;
                                    if reply_tx.send(result).is_err() {
                                        // CRITICAL: Caller disappeared, but forward was established
                                        // Must cancel immediately to avoid "ghost forward"
                                        warn!(
                                            "Caller dropped after tcpip_forward succeeded. \
                                             Cancelling orphaned forward {}:{}",
                                            address, bound_port
                                        );
                                        let _ =
                                            handle.cancel_tcpip_forward(&address, bound_port).await;
                                    }
                                }
                                Err(_) => {
                                    // Forward failed, no cleanup needed
                                    let _ = reply_tx.send(result);
                                }
                            }
                        }

                        HandleCommand::CancelTcpipForward {
                            address,
                            port,
                            reply_tx,
                        } => {
                            let result = handle.cancel_tcpip_forward(&address, port).await;
                            if reply_tx.send(result).is_err() {
                                warn!(
                                    "Caller dropped before receiving cancel_tcpip_forward result"
                                );
                                // Cancel already executed, no rollback needed
                            }
                        }

                        HandleCommand::Ping { reply_tx } => {
                            // Use send_keepalive(true) — sends SSH_MSG_GLOBAL_REQUEST
                            // "keepalive@openssh.com" with want_reply=true.
                            // This is the proper SSH heartbeat mechanism, avoiding the
                            // channel_open_session hack which leaked channels on the server.
                            debug!("Keepalive probe for session {}", session_id);
                            let result = match tokio::time::timeout(
                                std::time::Duration::from_secs(5),
                                handle.send_keepalive(true),
                            )
                            .await
                            {
                                Ok(Ok(())) => {
                                    debug!("Keepalive OK for session {}", session_id);
                                    PingResult::Ok
                                }
                                Ok(Err(e)) => {
                                    let error_str = format!("{:?}", e);
                                    if error_str.contains("Disconnect") || error_str.contains("disconnect") {
                                        warn!("Keepalive SSH disconnect for session {}: {:?}", session_id, e);
                                        PingResult::IoError
                                    } else {
                                        warn!("Keepalive SSH error for session {} (treating as soft failure): {:?}", session_id, e);
                                        PingResult::Timeout
                                    }
                                }
                                Err(_) => {
                                    warn!("Keepalive timeout for session {} (5s)", session_id);
                                    PingResult::Timeout
                                }
                            };
                            let _ = reply_tx.send(result);
                        }

                        HandleCommand::Disconnect => {
                            info!("Disconnect requested for session {}", session_id);
                            break;
                        }
                    }
                }
                None => {
                    // All senders dropped
                    info!("All controllers dropped for session {}", session_id);
                    break;
                }
            }
        }

        // === Cleanup phase ===
        // Notify all disconnect subscribers (port forwards, etc.)
        // The send() may fail if no subscribers, which is fine
        let _ = disconnect_tx_clone.send(());
        
        // Drain all pending commands, notify callers that connection is closed
        drain_pending_commands(&mut cmd_rx);

        // Disconnect SSH properly with reason
        let _ = handle
            .disconnect(russh::Disconnect::ByApplication, "Session closed", "en")
            .await;
        info!("Handle owner task terminated for session {}", session_id);
    });

    HandleController { cmd_tx, disconnect_tx }
}

/// Drain all pending commands, returning Disconnected error to each
fn drain_pending_commands(cmd_rx: &mut mpsc::Receiver<HandleCommand>) {
    // Close receiver first, prevent new messages
    cmd_rx.close();

    // Drain all messages already in queue
    while let Ok(cmd) = cmd_rx.try_recv() {
        match cmd {
            HandleCommand::ChannelOpenSession { reply_tx } => {
                let _ = reply_tx.send(Err(russh::Error::Disconnect));
            }
            HandleCommand::ChannelOpenDirectTcpip { reply_tx, .. } => {
                let _ = reply_tx.send(Err(russh::Error::Disconnect));
            }
            HandleCommand::TcpipForward { reply_tx, .. } => {
                let _ = reply_tx.send(Err(russh::Error::Disconnect));
            }
            HandleCommand::CancelTcpipForward { reply_tx, .. } => {
                let _ = reply_tx.send(Err(russh::Error::Disconnect));
            }
            HandleCommand::Ping { reply_tx } => {
                let _ = reply_tx.send(PingResult::IoError);
            }
            HandleCommand::Disconnect => {
                // Already disconnecting, ignore
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // TODO: Add unit tests
    // - HandleController sends commands and receives replies
    // - HandleController drop causes task exit
    // - Disconnect drains pending commands
    // - tcpip_forward cleanup on reply loss
}
