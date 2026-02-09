//! Session and Connection Management Commands
//!
//! This module provides commands for managing SSH sessions and the connection pool.
//! 
//! ## Active Commands
//! 
//! - `disconnect_v2` - Disconnect a session
//! - `list_sessions_v2` / `get_session` / `get_session_stats` - Session queries
//! - `resize_session_v2` / `reorder_sessions` - Session management
//! - `restore_sessions` / `list_persisted_sessions` / `delete_persisted_session` - Persistence
//! - `establish_connection` / `list_connections` / `disconnect_connection` - Connection pool
//! - `check_ssh_keys` - Key discovery
//!
//! ## Connection Architecture
//!
//! All new connections should be established through the SessionTree architecture:
//! 1. Frontend uses `expand_manual_preset` to create tree nodes
//! 2. `connectNodeWithAncestors` performs linear connection through the tree
//! 3. Each node uses `connect_tree_node` for the actual SSH connection

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;
use tracing::{info, warn};

use super::{ForwardingRegistry, HealthRegistry, ProfilerRegistry};
use crate::bridge::BridgeManager;
use crate::session::{
    AuthMethod, KeyAuth, SessionConfig, SessionInfo, SessionRegistry, SessionStats,
};
use crate::sftp::session::SftpRegistry;
use crate::ssh::SshConnectionRegistry;

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
// Session Management Commands
// ═══════════════════════════════════════════════════════════════════════════════════

/// Disconnect a session (v2 with registry)
#[tauri::command]
pub async fn disconnect_v2(
    session_id: String,
    registry: State<'_, Arc<SessionRegistry>>,
    bridge_manager: State<'_, BridgeManager>,
    sftp_registry: State<'_, Arc<SftpRegistry>>,
    forwarding_registry: State<'_, Arc<ForwardingRegistry>>,
    connection_registry: State<'_, Arc<SshConnectionRegistry>>,
    health_registry: State<'_, HealthRegistry>,
    profiler_registry: State<'_, ProfilerRegistry>,
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

    // Clean up health tracker and resource profiler
    health_registry.remove(&session_id);
    profiler_registry.remove(&session_id);

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

/// Check if SSH Agent is available on the current platform
///
/// - Unix: returns `true` if `SSH_AUTH_SOCK` is set
/// - Windows: always `true` (OpenSSH pipe exists when service is installed)
/// - Other: `false`
#[tauri::command]
pub fn is_ssh_agent_available() -> bool {
    crate::ssh::is_agent_available()
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
        AuthRequest::Agent => AuthMethod::Agent,
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
