//! Connection Commands with Session Registry Integration
//!
//! This module provides the main connection commands for establishing SSH sessions.
//! 
//! ## Architecture Overview
//! 
//! There are two configuration types with distinct responsibilities:
//! - **`SessionConfig`** (session/types.rs): UI-facing configuration with display properties
//!   (name, color). Used for session registry and frontend state.
//! - **`SshConfig`** (ssh/config.rs): Low-level network configuration with connection properties
//!   (timeout, proxy_chain, strict_host_key_checking). Used for actual SSH handshake.
//!
//! ## Connection Flow
//!
//! ```text
//! Frontend Request (ConnectRequest)
//!        │
//!        ▼
//! ┌──────────────────┐
//! │ convert_auth()   │  ← Convert AuthRequest to AuthMethod
//! └────────┬─────────┘
//!          │
//!          ▼
//! ┌──────────────────┐     ┌───────────────────────┐
//! │ Proxy Chain?     │─Yes─▶│ connect_via_proxy()   │
//! └────────┬─────────┘     └───────────┬───────────┘
//!          │No                         │
//!          ▼                           │
//! ┌──────────────────┐                 │
//! │ SshClient::      │                 │
//! │ connect()        │                 │
//! └────────┬─────────┘                 │
//!          │                           │
//!          ▼                           ▼
//! ┌──────────────────────────────────────┐
//! │ start_session_and_bridge()           │  ← Common path for both
//! │ - Request shell                      │
//! │ - Create WebSocket bridge            │
//! │ - Update registry                    │
//! └────────────────┬─────────────────────┘
//!                  │
//!                  ▼
//! ┌──────────────────────────────────────┐
//! │ register_session_services()          │  ← Register to pools
//! │ - ForwardingManager                  │
//! │ - ConnectionRegistry (heartbeat)     │
//! └──────────────────────────────────────┘
//! ```

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, State};
use tokio::sync::oneshot;
use tokio::time::timeout;
use tracing::{info, warn};

use super::ForwardingRegistry;
use crate::bridge::{BridgeManager, WsBridge};
use crate::forwarding::ForwardingManager;
use crate::session::{
    AuthMethod, KeyAuth, SessionConfig, SessionInfo, SessionRegistry, SessionStats,
};
use crate::sftp::session::SftpRegistry;
use crate::ssh::{
    AuthMethod as SshAuthMethod, HandleController, SshClient, SshConfig,
    SshConnectionRegistry, SshSession,
};

/// Connection timeout settings
const HANDSHAKE_TIMEOUT_SECS: u64 = 30;
const AUTH_TIMEOUT_SECS: u64 = 60;

/// Response returned when a connection is established
#[derive(Debug, Serialize)]
pub struct ConnectResponseV2 {
    /// Session ID
    pub session_id: String,
    /// WebSocket URL to connect to
    pub ws_url: String,
    /// Port number
    pub port: u16,
    /// Session information
    pub session: SessionInfo,
    /// WebSocket authentication token (sent as first message after connection)
    pub ws_token: String,
}

/// Connect request from frontend
#[derive(Debug, Deserialize)]
pub struct ConnectRequest {
    pub host: String,
    pub port: u16,
    pub username: String,
    #[serde(flatten)]
    pub auth: AuthRequest,
    #[serde(default = "default_cols")]
    pub cols: u32,
    #[serde(default = "default_rows")]
    pub rows: u32,
    pub name: Option<String>,
    pub proxy_chain: Option<Vec<ProxyChainRequest>>,
    #[serde(default)]
    pub buffer_config: Option<BufferConfigRequest>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BufferConfigRequest {
    pub max_lines: usize,
    pub save_on_disconnect: bool,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "auth_type", rename_all = "snake_case")]
pub enum AuthRequest {
    Password {
        password: String,
    },
    Key {
        key_path: String,
        passphrase: Option<String>,
    },
    DefaultKey {
        passphrase: Option<String>,
    },
    Agent,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ProxyChainRequest {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthRequest,
}

fn default_cols() -> u32 {
    80
}
fn default_rows() -> u32 {
    24
}

// ═══════════════════════════════════════════════════════════════════════════════════
// Helper Functions - Extracted to reduce duplication between proxy and direct paths
// ═══════════════════════════════════════════════════════════════════════════════════

/// Convert frontend `AuthRequest` to internal `AuthMethod`.
///
/// This handles the `DefaultKey` variant by searching for SSH keys in standard locations.
/// Returns the unified `AuthMethod` type used throughout the backend.
fn convert_auth_request(auth: &AuthRequest) -> Result<AuthMethod, String> {
    match auth {
        AuthRequest::Password { password } => Ok(AuthMethod::Password {
            password: password.clone(),
        }),
        AuthRequest::Key {
            key_path,
            passphrase,
        } => Ok(AuthMethod::Key {
            key_path: key_path.clone(),
            passphrase: passphrase.clone(),
        }),
        AuthRequest::DefaultKey { passphrase } => {
            let key_auth = KeyAuth::from_default_locations(passphrase.as_deref())
                .map_err(|e| format!("No SSH key found: {}", e))?;
            Ok(AuthMethod::Key {
                key_path: key_auth.key_path.to_string_lossy().to_string(),
                passphrase: passphrase.clone(),
            })
        }
        AuthRequest::Agent => Err("SSH Agent not yet supported".to_string()),
    }
}

/// Convert `AuthRequest` to SSH-layer `AuthMethod` (for ProxyChain use).
///
/// Similar to `convert_auth_request` but returns `SshAuthMethod` directly
/// for use with the SSH proxy layer.
fn convert_auth_request_to_ssh(auth: &AuthRequest) -> Result<SshAuthMethod, String> {
    match auth {
        AuthRequest::Password { password } => Ok(SshAuthMethod::Password {
            password: password.clone(),
        }),
        AuthRequest::Key {
            key_path,
            passphrase,
        } => Ok(SshAuthMethod::Key {
            key_path: key_path.clone(),
            passphrase: passphrase.clone(),
        }),
        AuthRequest::DefaultKey { passphrase } => {
            let key_auth = KeyAuth::from_default_locations(passphrase.as_deref())
                .map_err(|e| format!("No SSH key found: {}", e))?;
            Ok(SshAuthMethod::Key {
                key_path: key_auth.key_path.to_string_lossy().to_string(),
                passphrase: passphrase.clone(),
            })
        }
        AuthRequest::Agent => Err("SSH Agent not yet supported".to_string()),
    }
}

/// Build `SessionConfig` from connection request parameters.
///
/// `SessionConfig` is the UI-facing configuration stored in the session registry.
/// It contains display properties (name, color) but not network-specific options.
fn build_session_config(request: &ConnectRequest, auth: AuthMethod) -> SessionConfig {
    SessionConfig {
        host: request.host.clone(),
        port: request.port,
        username: request.username.clone(),
        auth,
        name: request.name.clone(),
        color: None,
        cols: request.cols,
        rows: request.rows,
    }
}

/// Convert `SessionConfig` to `SshConfig` for SSH client.
///
/// `SshConfig` is the low-level network configuration used by `SshClient`.
/// It includes timeout settings and host key checking options.
///
/// # Errors
/// Returns an error if the auth method is `KeyboardInteractive`, which must
/// use the dedicated `ssh_connect_kbi` command instead.
impl TryFrom<&SessionConfig> for SshConfig {
    type Error = &'static str;
    
    fn try_from(config: &SessionConfig) -> Result<Self, Self::Error> {
        let ssh_auth = match &config.auth {
            AuthMethod::Password { password } => SshAuthMethod::Password {
                password: password.clone(),
            },
            AuthMethod::Key {
                key_path,
                passphrase,
            } => SshAuthMethod::Key {
                key_path: key_path.clone(),
                passphrase: passphrase.clone(),
            },
            AuthMethod::Certificate {
                key_path,
                cert_path,
                passphrase,
            } => SshAuthMethod::Certificate {
                key_path: key_path.clone(),
                cert_path: cert_path.clone(),
                passphrase: passphrase.clone(),
            },
            AuthMethod::Agent => SshAuthMethod::Agent,
            AuthMethod::KeyboardInteractive => {
                // KeyboardInteractive sessions must use the dedicated ssh_connect_kbi command.
                // Return error instead of panic for robustness.
                return Err("KeyboardInteractive must use ssh_connect_kbi command, not connect_v2");
            }
        };

        Ok(SshConfig {
            host: config.host.clone(),
            port: config.port,
            username: config.username.clone(),
            auth: ssh_auth,
            timeout_secs: HANDSHAKE_TIMEOUT_SECS,
            cols: config.cols,
            rows: config.rows,
            proxy_chain: None,
            strict_host_key_checking: false, // Auto-accept unknown hosts for UX
            trust_host_key: None, // TODO: Integrate with TOFU flow
        })
    }
}

/// Result of starting a session and WebSocket bridge.
struct SessionStartResult {
    /// WebSocket port for frontend connection
    ws_port: u16,
    /// Authentication token for WebSocket handshake
    ws_token: String,
    /// Handle controller for forwarding and heartbeat
    handle_controller: HandleController,
    /// Receiver for disconnect notification
    disconnect_rx: oneshot::Receiver<crate::bridge::DisconnectReason>,
}

/// Start a session: request shell, create WebSocket bridge, update registry.
///
/// This is the common path for both proxy and direct connections after
/// the SSH handshake is complete. It:
/// 1. Requests a shell on the SSH session
/// 2. Creates a WebSocket bridge for the frontend
/// 3. Updates the session registry with connection success
///
/// # Arguments
/// * `session` - The established SSH session (post-handshake)
/// * `sid` - Session ID in the registry
/// * `registry` - Session registry for state management
/// * `cols`, `rows` - Terminal dimensions
async fn start_session_and_bridge(
    session: SshSession,
    sid: &str,
    registry: &Arc<SessionRegistry>,
) -> Result<SessionStartResult, String> {
    // Request shell with auth timeout
    let shell_future = session.request_shell_extended();

    let (session_handle, handle_controller) =
        timeout(Duration::from_secs(AUTH_TIMEOUT_SECS), shell_future)
            .await
            .map_err(|_| {
                registry.remove(sid);
                format!("Authentication timeout after {}s", AUTH_TIMEOUT_SECS)
            })?
            .map_err(|e| {
                registry.remove(sid);
                format!("Shell request failed: {}", e)
            })?;

    // Get command sender for resize support
    let cmd_tx = session_handle.cmd_tx.clone();

    // Get scroll buffer for this session
    let scroll_buffer = registry
        .with_session(sid, |entry| entry.scroll_buffer.clone())
        .ok_or_else(|| "Session not found in registry".to_string())?;

    // Start WebSocket bridge with disconnect tracking
    let (_, ws_port, ws_token, disconnect_rx) =
        WsBridge::start_extended_with_disconnect(session_handle, scroll_buffer, false)
            .await
            .map_err(|e| {
                registry.remove(sid);
                format!("Failed to start WebSocket bridge: {}", e)
            })?;

    // Update registry with success
    let controller_for_registry = handle_controller.clone();
    registry
        .connect_success(sid, ws_port, cmd_tx, controller_for_registry)
        .map_err(|e| {
            registry.remove(sid);
            format!("Failed to update session state: {}", e)
        })?;

    Ok(SessionStartResult {
        ws_port,
        ws_token,
        handle_controller,
        disconnect_rx,
    })
}

/// Register session with supporting services (forwarding, connection pool).
///
/// This sets up:
/// - `ForwardingManager` for port forwarding support
/// - Connection pool registration for visibility in connection panel
/// - Heartbeat monitoring for connection health
async fn register_session_services(
    sid: &str,
    config: SessionConfig,
    handle_controller: HandleController,
    disconnect_rx: oneshot::Receiver<crate::bridge::DisconnectReason>,
    registry: &Arc<SessionRegistry>,
    forwarding_registry: &Arc<ForwardingRegistry>,
    connection_registry: &Arc<SshConnectionRegistry>,
) {
    // Spawn task to handle WebSocket bridge disconnect
    let sid_clone = sid.to_string();
    let registry_clone = registry.clone();
    let conn_registry_clone = connection_registry.clone();
    tokio::spawn(async move {
        if let Ok(reason) = disconnect_rx.await {
            warn!(
                "Session {} WebSocket bridge disconnected: {:?}",
                sid_clone, reason
            );

            if reason.is_recoverable() {
                let _ = registry_clone.mark_ws_detached(&sid_clone, std::time::Duration::from_secs(300));
            } else {
                // AcceptTimeout: 清理会话（在 connect_v2 中 session_id == connection_id）
                if matches!(reason, crate::bridge::DisconnectReason::AcceptTimeout) {
                    warn!("Session {} WS accept timeout, removing from registries", sid_clone);
                    // 在 connect_v2 模式中，connection_id == session_id
                    let _ = conn_registry_clone.remove_terminal(&sid_clone, &sid_clone).await;
                    let _ = registry_clone.disconnect_complete(&sid_clone, true);
                } else {
                    let _ = registry_clone.disconnect_complete(&sid_clone, false);
                }
            }
        }
    });

    // Register ForwardingManager for port forwarding support
    let forwarding_controller = handle_controller.clone();
    let forwarding_manager = ForwardingManager::new(forwarding_controller, sid.to_string());
    forwarding_registry
        .register(sid.to_string(), forwarding_manager)
        .await;
    info!("ForwardingManager registered for session {}", sid);

    // Register to SSH connection pool for visibility in connection panel
    let pool_controller = handle_controller;
    connection_registry
        .register_existing(sid.to_string(), config, pool_controller, sid.to_string())
        .await;
    info!("Connection registered to pool for session {}", sid);

    // Start heartbeat monitoring for this connection
    connection_registry.start_heartbeat(sid);
    info!("Heartbeat started for session {}", sid);
}

// ═══════════════════════════════════════════════════════════════════════════════════
// Main Commands
// ═══════════════════════════════════════════════════════════════════════════════════

/// Connect to SSH server (v2 with registry)
///
/// Supports two connection modes:
/// 1. **Direct connection**: Standard SSH connection to target host
/// 2. **Proxy chain**: Multi-hop connection through jump hosts (ProxyJump)
///
/// Both modes share the same session lifecycle:
/// - Create session in registry
/// - Establish SSH connection (direct or via proxy)
/// - Request shell and create WebSocket bridge
/// - Register supporting services (forwarding, heartbeat)
///
/// # ⚠️ DEPRECATED
/// 
/// 此命令已被弃用。请使用 `connect_tree_node` + `expand_manual_preset` 组合作为唯一的 SSH 连接入口。
/// 
/// **原因**：
/// - OxideTerm 采用"前端驱动、后端执行"架构
/// - 所有连接必须通过 SessionTree 管理，以确保锁机制生效
/// - `connect_v2` 的 proxy_chain 处理绕过了 SessionTree，可能导致状态不一致
/// 
/// **迁移指南**：
/// 1. 使用 `expand_manual_preset` 在 SessionTree 中展开跳板链
/// 2. 前端使用 `connectNodeWithAncestors` 线性连接每个节点
/// 3. 每个节点通过 `connect_tree_node(node_id)` 建立连接
#[tauri::command]
#[deprecated(since = "0.8.0", note = "Use connect_tree_node + expand_manual_preset instead")]
pub async fn connect_v2(
    _app_handle: AppHandle,
    request: ConnectRequest,
    registry: State<'_, Arc<SessionRegistry>>,
    forwarding_registry: State<'_, Arc<ForwardingRegistry>>,
    connection_registry: State<'_, Arc<SshConnectionRegistry>>,
) -> Result<ConnectResponseV2, String> {
    warn!(
        "⚠️ DEPRECATED: connect_v2 called for {}@{}:{} - please migrate to connect_tree_node + expand_manual_preset",
        request.username, request.host, request.port
    );
    info!(
        "Connect request: {}@{}:{}",
        request.username, request.host, request.port
    );

    // Convert auth and build session config
    let auth = convert_auth_request(&request.auth)?;
    let config = build_session_config(&request, auth.clone());

    // Create session in registry (checks connection limit)
    let sid = if let Some(buf_cfg) = &request.buffer_config {
        registry
            .create_session_with_buffer(config.clone(), buf_cfg.max_lines)
            .map_err(|e| format!("Failed to create session: {}", e))?
    } else {
        registry
            .create_session(config.clone())
            .map_err(|e| format!("Failed to create session: {}", e))?
    };

    // Start connecting state
    if let Err(e) = registry.start_connecting(&sid) {
        registry.remove(&sid);
        return Err(format!("Failed to start connection: {}", e));
    }

    // Establish connection and start session
    let (ws_port, ws_token, handle_controller, disconnect_rx) =
        if let Some(proxy_chain_req) = &request.proxy_chain {
            // === Multi-hop proxy connection ===
            connect_via_proxy_chain(
                &request,
                proxy_chain_req,
                &sid,
                registry.inner(),
            )
            .await?
        } else {
            // === Direct connection ===
            connect_direct(&config, &sid, registry.inner()).await?
        };

    // Register supporting services (common path)
    register_session_services(
        &sid,
        config,
        handle_controller,
        disconnect_rx,
        registry.inner(),
        forwarding_registry.inner(),
        connection_registry.inner(),
    )
    .await;

    info!("Connection established: session={}, ws_port={}", sid, ws_port);

    // Build response
    let ws_url = format!("ws://localhost:{}", ws_port);
    let session_info = registry
        .get(&sid)
        .ok_or_else(|| "Session disappeared from registry".to_string())?;

    Ok(ConnectResponseV2 {
        session_id: sid,
        ws_url,
        port: ws_port,
        session: session_info,
        ws_token,
    })
}

/// Establish direct SSH connection (no proxy chain).
async fn connect_direct(
    config: &SessionConfig,
    sid: &str,
    registry: &Arc<SessionRegistry>,
) -> Result<(u16, String, HandleController, oneshot::Receiver<crate::bridge::DisconnectReason>), String>
{
    // Convert SessionConfig to SshConfig
    let ssh_config: SshConfig = config.try_into()
        .map_err(|e: &str| e.to_string())?;

    // Connect with handshake timeout
    let client = SshClient::new(ssh_config);
    let connect_future = client.connect();

    let session = timeout(Duration::from_secs(HANDSHAKE_TIMEOUT_SECS), connect_future)
        .await
        .map_err(|_| {
            registry.remove(sid);
            format!("Connection timeout after {}s", HANDSHAKE_TIMEOUT_SECS)
        })?
        .map_err(|e| {
            registry.remove(sid);
            format!("Connection failed: {}", e)
        })?;

    // Start session and bridge (common path)
    let result = start_session_and_bridge(session, sid, registry).await?;

    Ok((
        result.ws_port,
        result.ws_token,
        result.handle_controller,
        result.disconnect_rx,
    ))
}

/// Establish multi-hop SSH connection via proxy chain.
async fn connect_via_proxy_chain(
    request: &ConnectRequest,
    proxy_chain_req: &[ProxyChainRequest],
    sid: &str,
    registry: &Arc<SessionRegistry>,
) -> Result<(u16, String, HandleController, oneshot::Receiver<crate::bridge::DisconnectReason>), String>
{
    info!("Using proxy chain with {} hops", proxy_chain_req.len());

    // Convert target auth to SSH layer type
    let target_auth = convert_auth_request_to_ssh(&request.auth)?;

    // Build proxy chain
    let mut chain = crate::ssh::ProxyChain::new();
    for hop_req in proxy_chain_req {
        let hop_auth = convert_auth_request_to_ssh(&hop_req.auth)
            .map_err(|e| format!("Proxy hop auth error: {}", e))?;

        chain = chain.add_hop(crate::ssh::ProxyHop {
            host: hop_req.host.clone(),
            port: hop_req.port,
            username: hop_req.username.clone(),
            auth: hop_auth,
        });
    }

    // Establish multi-hop SSH connection
    let proxy_conn = crate::ssh::connect_via_proxy(
        &chain,
        &request.host,
        request.port,
        &request.username,
        &target_auth,
        HANDSHAKE_TIMEOUT_SECS,
    )
    .await
    .map_err(|e| format!("Proxy connection failed: {}", e))?;

    info!(
        "Multi-hop connection established: {} proxy handles",
        proxy_conn.jump_handles.len()
    );

    // Extract target handle and create session
    let target_handle = proxy_conn.into_target_handle();
    let session = SshSession::new(target_handle, request.cols, request.rows);

    // Start session and bridge (common path)
    let result = start_session_and_bridge(session, sid, registry).await?;

    Ok((
        result.ws_port,
        result.ws_token,
        result.handle_controller,
        result.disconnect_rx,
    ))
}

/// Disconnect a session (v2 with registry)
#[tauri::command]
pub async fn disconnect_v2(
    session_id: String,
    registry: State<'_, Arc<SessionRegistry>>,
    bridge_manager: State<'_, BridgeManager>,
    sftp_registry: State<'_, Arc<SftpRegistry>>,
    forwarding_registry: State<'_, Arc<ForwardingRegistry>>,
    connection_registry: State<'_, Arc<SshConnectionRegistry>>,
) -> Result<bool, String> {
    info!("Disconnecting session: {}", session_id);

    // Save terminal buffer before disconnecting
    if let Err(e) = registry.persist_session_with_buffer(&session_id).await {
        tracing::warn!("Failed to persist session buffer: {}", e);
        // Don't fail the disconnect if persistence fails
    }

    // Stop and remove all port forwards for this session
    forwarding_registry.remove(&session_id).await;

    // Close via registry (sends close command)
    registry.close_session(&session_id).await?;

    // Complete disconnection and remove
    let _ = registry.disconnect_complete(&session_id, true);

    // Also unregister from bridge manager
    bridge_manager.unregister(&session_id);

    // Drop any cached SFTP handle tied to this session
    sftp_registry.remove(&session_id);

    // Release connection from pool (using session_id as connection_id)
    // This will decrement ref_count and potentially start idle timer
    if let Err(e) = connection_registry.release(&session_id).await {
        warn!("Failed to release connection from pool: {}", e);
        // Not a fatal error - the connection might not have been in the pool
    } else {
        info!("Connection released from pool for session {}", session_id);
    }

    Ok(true)
}

/// List all sessions (v2)
#[tauri::command]
pub async fn list_sessions_v2(
    registry: State<'_, Arc<SessionRegistry>>,
) -> Result<Vec<SessionInfo>, String> {
    Ok(registry.list())
}

/// Get session statistics
#[tauri::command]
pub async fn get_session_stats(
    registry: State<'_, Arc<SessionRegistry>>,
) -> Result<SessionStats, String> {
    Ok(registry.stats())
}

/// Get single session info
#[tauri::command]
pub async fn get_session(
    session_id: String,
    registry: State<'_, Arc<SessionRegistry>>,
) -> Result<SessionInfo, String> {
    registry
        .get(&session_id)
        .ok_or_else(|| format!("Session not found: {}", session_id))
}

/// Resize session PTY (v2)
#[tauri::command]
pub async fn resize_session_v2(
    session_id: String,
    cols: u16,
    rows: u16,
    registry: State<'_, Arc<SessionRegistry>>,
) -> Result<(), String> {
    registry.resize(&session_id, cols, rows).await
}

/// Reorder sessions (for tab drag and drop)
#[tauri::command]
pub async fn reorder_sessions(
    ordered_ids: Vec<String>,
    registry: State<'_, Arc<SessionRegistry>>,
) -> Result<(), String> {
    registry
        .reorder(&ordered_ids)
        .map_err(|e| format!("Failed to reorder: {}", e))
}

/// Check if default SSH keys are available
#[tauri::command]
pub async fn check_ssh_keys() -> Result<Vec<String>, String> {
    let keys = crate::session::auth::list_available_keys();
    Ok(keys
        .into_iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect())
}

/// Restore persisted sessions (returns session metadata for selective restoration)
#[tauri::command]
pub async fn restore_sessions(
    registry: State<'_, Arc<SessionRegistry>>,
) -> Result<Vec<PersistedSessionDto>, String> {
    let sessions = registry
        .restore_sessions()
        .map_err(|e| format!("Failed to restore sessions: {:?}", e))?;

    Ok(sessions
        .into_iter()
        .map(|s| PersistedSessionDto {
            id: s.id,
            host: s.config.host,
            port: s.config.port,
            username: s.config.username,
            name: s.config.name,
            created_at: s.created_at.to_rfc3339(),
            order: s.order,
        })
        .collect())
}

/// List persisted session IDs
#[tauri::command]
pub async fn list_persisted_sessions(
    registry: State<'_, Arc<SessionRegistry>>,
) -> Result<Vec<String>, String> {
    registry
        .list_persisted_sessions()
        .map_err(|e| format!("Failed to list persisted sessions: {:?}", e))
}

/// Delete a persisted session
#[tauri::command]
pub async fn delete_persisted_session(
    registry: State<'_, Arc<SessionRegistry>>,
    session_id: String,
) -> Result<(), String> {
    registry
        .delete_persisted_session(&session_id)
        .map_err(|e| format!("Failed to delete persisted session: {:?}", e))
}

/// DTO for persisted session info (without sensitive data)
#[derive(Debug, Serialize)]
pub struct PersistedSessionDto {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub name: Option<String>,
    pub created_at: String,
    pub order: usize,
}

// ═══════════════════════════════════════════════════════════════════════════
// Connection Pool Commands (建立连接，不创建终端)
// ═══════════════════════════════════════════════════════════════════════════

/// 建立连接响应
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EstablishConnectionResponse {
    /// 连接 ID
    pub connection_id: String,
    /// 是否复用了已有连接
    pub reused: bool,
    /// 连接信息
    pub connection: crate::ssh::ConnectionInfo,
}

/// 建立 SSH 连接（不创建终端）
/// 
/// 如果已有相同配置的活跃连接，则复用；否则建立新连接。
/// 连接加入连接池，用户可以稍后从连接池创建终端。
#[tauri::command]
pub async fn establish_connection(
    request: ConnectRequest,
    connection_registry: State<'_, Arc<SshConnectionRegistry>>,
) -> Result<EstablishConnectionResponse, String> {
    info!(
        "Establish connection request: {}@{}:{}",
        request.username, request.host, request.port
    );

    // 构建配置用于查找/创建
    let auth = match request.auth {
        AuthRequest::Password { password } => AuthMethod::Password { password },
        AuthRequest::Key {
            key_path,
            passphrase,
        } => AuthMethod::Key {
            key_path,
            passphrase,
        },
        AuthRequest::DefaultKey { passphrase } => {
            let key_auth = KeyAuth::from_default_locations(passphrase.as_deref())
                .map_err(|e| format!("No SSH key found: {}", e))?;
            AuthMethod::Key {
                key_path: key_auth.key_path.to_string_lossy().to_string(),
                passphrase,
            }
        }
        AuthRequest::Agent => {
            return Err("SSH Agent not yet supported".to_string());
        }
    };

    let config = SessionConfig {
        host: request.host.clone(),
        port: request.port,
        username: request.username.clone(),
        auth,
        name: request.name.clone(),
        color: None,
        cols: request.cols,
        rows: request.rows,
    };

    // 检查是否有可复用的连接
    if let Some(existing_id) = connection_registry.find_by_config(&config) {
        info!("Reusing existing connection: {}", existing_id);
        
        let connection_info = connection_registry
            .get_info(&existing_id)
            .await
            .ok_or_else(|| "Connection disappeared".to_string())?;

        return Ok(EstablishConnectionResponse {
            connection_id: existing_id,
            reused: true,
            connection: connection_info,
        });
    }

    // 建立新连接
    // TODO: 支持 proxy_chain
    if request.proxy_chain.is_some() {
        return Err("Proxy chain not yet supported in establish_connection. Use connect_v2 for proxy connections.".to_string());
    }

    let connection_id = connection_registry
        .connect(config)
        .await
        .map_err(|e| format!("Connection failed: {}", e))?;

    let connection_info = connection_registry
        .get_info(&connection_id)
        .await
        .ok_or_else(|| "Connection disappeared after creation".to_string())?;

    info!("New connection established: {}", connection_id);

    Ok(EstablishConnectionResponse {
        connection_id,
        reused: false,
        connection: connection_info,
    })
}

/// 获取连接池中所有连接
#[tauri::command]
pub async fn list_connections(
    connection_registry: State<'_, Arc<SshConnectionRegistry>>,
) -> Result<Vec<crate::ssh::ConnectionInfo>, String> {
    Ok(connection_registry.inner().list_connections().await)
}

/// 断开连接池中的连接
#[tauri::command]
pub async fn disconnect_connection(
    connection_id: String,
    connection_registry: State<'_, Arc<SshConnectionRegistry>>,
) -> Result<(), String> {
    connection_registry
        .inner()
        .disconnect(&connection_id)
        .await
        .map_err(|e| format!("Failed to disconnect: {}", e))
}
