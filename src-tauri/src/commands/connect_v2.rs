// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

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
use crate::ssh::{
    ProxyChain, ProxyConnectEndpoint, ProxyConnectEndpointKind, ProxyConnectError,
    ProxyConnectOperation, ProxyHop, SshClient, SshConfig, SshConnectionRegistry, SshError,
    connect_via_proxy_for_test,
};
use zeroize::Zeroizing;

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
    pub trust_host_key: Option<bool>,
    #[serde(default)]
    pub expected_host_key_fingerprint: Option<String>,
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
        password: Zeroizing<String>,
    },
    Key {
        key_path: String,
        passphrase: Option<Zeroizing<String>>,
    },
    DefaultKey {
        passphrase: Option<Zeroizing<String>>,
    },
    Certificate {
        key_path: String,
        cert_path: String,
        passphrase: Option<Zeroizing<String>>,
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
    120
}
fn default_rows() -> u32 {
    40
}

fn build_session_auth(auth: AuthRequest) -> Result<AuthMethod, String> {
    build_session_auth_with(auth, |passphrase| {
        KeyAuth::from_default_locations(passphrase).map_err(|e| format!("No SSH key found: {}", e))
    })
}

fn build_session_auth_with<F>(
    auth: AuthRequest,
    default_key_loader: F,
) -> Result<AuthMethod, String>
where
    F: FnOnce(Option<&str>) -> Result<KeyAuth, String>,
{
    match auth {
        AuthRequest::Password { password } => Ok(AuthMethod::Password { password }),
        AuthRequest::Key {
            key_path,
            passphrase,
        } => Ok(AuthMethod::Key {
            key_path,
            passphrase,
        }),
        AuthRequest::DefaultKey { passphrase } => {
            let key_auth = default_key_loader(passphrase.as_deref().map(|s| s.as_str()))?;
            Ok(AuthMethod::Key {
                key_path: key_auth.key_path.to_string_lossy().to_string(),
                passphrase,
            })
        }
        AuthRequest::Certificate {
            key_path,
            cert_path,
            passphrase,
        } => Ok(AuthMethod::Certificate {
            key_path,
            cert_path,
            passphrase,
        }),
        AuthRequest::Agent => Ok(AuthMethod::Agent),
    }
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
    let auth = build_session_auth(request.auth)?;

    let config = SessionConfig {
        host: request.host.clone(),
        port: request.port,
        username: request.username.clone(),
        auth,
        name: request.name.clone(),
        color: None,
        cols: request.cols,
        rows: request.rows,
        agent_forwarding: false,
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

/// Test connection response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestConnectionResponse {
    /// Whether the connection succeeded
    pub success: bool,
    /// Time in milliseconds for the connection attempt
    pub elapsed_ms: u64,
    /// Structured diagnostic information for the UI.
    pub diagnostic: TestConnectionDiagnostic,
}

#[derive(Debug, Serialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum TestConnectionPhase {
    Preparation,
    HostKeyVerification,
    Transport,
    Authentication,
    Complete,
}

#[derive(Debug, Serialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum TestConnectionCategory {
    Success,
    Unsupported,
    DnsResolution,
    Timeout,
    Network,
    Tunnel,
    HostKeyUnknown,
    HostKeyChanged,
    Authentication,
    KeyMaterial,
    Agent,
    Protocol,
    Unknown,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum TestConnectionLocationKind {
    JumpHost,
    Target,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TestConnectionLocation {
    pub kind: TestConnectionLocationKind,
    pub host: String,
    pub port: u16,
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hop_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_hops: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via_hop_index: Option<usize>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestConnectionDiagnostic {
    pub phase: TestConnectionPhase,
    pub category: TestConnectionCategory,
    pub summary: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<TestConnectionLocation>,
}

fn target_location(request: &ConnectRequest) -> TestConnectionLocation {
    TestConnectionLocation {
        kind: TestConnectionLocationKind::Target,
        host: request.host.clone(),
        port: request.port,
        username: request.username.clone(),
        hop_index: None,
        total_hops: request
            .proxy_chain
            .as_ref()
            .map(|chain| chain.len())
            .filter(|count| *count > 0),
        via_hop_index: None,
    }
}

fn jump_host_location(
    hop: &ProxyChainRequest,
    hop_index: usize,
    total_hops: usize,
) -> TestConnectionLocation {
    TestConnectionLocation {
        kind: TestConnectionLocationKind::JumpHost,
        host: hop.host.clone(),
        port: hop.port,
        username: hop.username.clone(),
        hop_index: Some(hop_index),
        total_hops: Some(total_hops),
        via_hop_index: None,
    }
}

fn format_location(location: &TestConnectionLocation) -> String {
    match location.kind {
        TestConnectionLocationKind::JumpHost => format!(
            "jump host {} ({}@{}:{})",
            location.hop_index.unwrap_or_default(),
            location.username,
            location.host,
            location.port,
        ),
        TestConnectionLocationKind::Target => {
            format!("target {}@{}:{}", location.username, location.host, location.port)
        }
    }
}

fn route_overview(request: &ConnectRequest) -> String {
    if let Some(proxy_chain) = request.proxy_chain.as_ref().filter(|chain| !chain.is_empty()) {
        let hops = proxy_chain
            .iter()
            .enumerate()
            .map(|(index, hop)| {
                format!("jump host {} {}@{}:{}", index + 1, hop.username, hop.host, hop.port)
            })
            .collect::<Vec<_>>()
            .join(" -> ");
        format!(
            "{} -> target {}@{}:{}",
            hops, request.username, request.host, request.port
        )
    } else {
        format!("target {}@{}:{}", request.username, request.host, request.port)
    }
}

fn build_success_test_connection_response(
    elapsed_ms: u64,
    location: Option<TestConnectionLocation>,
) -> TestConnectionResponse {
    TestConnectionResponse {
        success: true,
        elapsed_ms,
        diagnostic: TestConnectionDiagnostic {
            phase: TestConnectionPhase::Complete,
            category: TestConnectionCategory::Success,
            summary: "Connection test succeeded".to_string(),
            detail: "SSH handshake and authentication completed successfully".to_string(),
            location,
        },
    }
}

fn build_failed_test_connection_response(
    elapsed_ms: u64,
    phase: TestConnectionPhase,
    category: TestConnectionCategory,
    summary: impl Into<String>,
    detail: impl Into<String>,
    location: Option<TestConnectionLocation>,
) -> TestConnectionResponse {
    TestConnectionResponse {
        success: false,
        elapsed_ms,
        diagnostic: TestConnectionDiagnostic {
            phase,
            category,
            summary: summary.into(),
            detail: detail.into(),
            location,
        },
    }
}

fn classify_test_connection_error(error: &SshError) -> (TestConnectionPhase, TestConnectionCategory) {
    match error {
        SshError::Timeout(message) => {
            let phase = if message.to_lowercase().contains("auth") {
                TestConnectionPhase::Authentication
            } else {
                TestConnectionPhase::Transport
            };
            (phase, TestConnectionCategory::Timeout)
        }
        SshError::AuthenticationFailed(_) => (
            TestConnectionPhase::Authentication,
            TestConnectionCategory::Authentication,
        ),
        SshError::KeyError(_) | SshError::CertificateLoadError(_) | SshError::CertificateParseError(_) => (
            TestConnectionPhase::Authentication,
            TestConnectionCategory::KeyMaterial,
        ),
        SshError::AgentNotAvailable(_) | SshError::AgentError(_) => (
            TestConnectionPhase::Authentication,
            TestConnectionCategory::Agent,
        ),
        SshError::ProtocolError(_) | SshError::ChannelError(_) => (
            TestConnectionPhase::Transport,
            TestConnectionCategory::Protocol,
        ),
        SshError::DnsResolution { .. } => {
            (TestConnectionPhase::Transport, TestConnectionCategory::DnsResolution)
        }
        SshError::HostKeyUnknown { .. } => (
            TestConnectionPhase::HostKeyVerification,
            TestConnectionCategory::HostKeyUnknown,
        ),
        SshError::HostKeyChanged { .. } => (
            TestConnectionPhase::HostKeyVerification,
            TestConnectionCategory::HostKeyChanged,
        ),
        SshError::ConnectionFailed(_) => (TestConnectionPhase::Transport, TestConnectionCategory::Unknown),
        _ => (TestConnectionPhase::Preparation, TestConnectionCategory::Unknown),
    }
}

fn direct_failure_summary(phase: &TestConnectionPhase, category: &TestConnectionCategory) -> String {
    match phase {
        TestConnectionPhase::Preparation => match category {
            TestConnectionCategory::KeyMaterial => {
                "Target authentication material is invalid".to_string()
            }
            _ => "Target connection request is incomplete".to_string(),
        },
        TestConnectionPhase::HostKeyVerification => "Target host key verification failed".to_string(),
        TestConnectionPhase::Transport => match category {
            TestConnectionCategory::DnsResolution => "Target DNS resolution failed".to_string(),
            TestConnectionCategory::Timeout => "Timed out reaching the target".to_string(),
            TestConnectionCategory::Network => "Target network connection failed".to_string(),
            TestConnectionCategory::Protocol => "Target SSH transport negotiation failed".to_string(),
            _ => "Target SSH transport failed".to_string(),
        },
        TestConnectionPhase::Authentication => match category {
            TestConnectionCategory::Agent => "Target SSH agent authentication failed".to_string(),
            TestConnectionCategory::KeyMaterial => {
                "Target key or certificate material could not be loaded".to_string()
            }
            TestConnectionCategory::Timeout => {
                "Timed out while authenticating to the target".to_string()
            }
            _ => "Target authentication failed".to_string(),
        },
        TestConnectionPhase::Complete => "Connection test succeeded".to_string(),
    }
}

fn location_from_proxy_endpoint(endpoint: &ProxyConnectEndpoint) -> TestConnectionLocation {
    TestConnectionLocation {
        kind: match endpoint.kind {
            ProxyConnectEndpointKind::JumpHost => TestConnectionLocationKind::JumpHost,
            ProxyConnectEndpointKind::Target => TestConnectionLocationKind::Target,
        },
        host: endpoint.host.clone(),
        port: endpoint.port,
        username: endpoint.username.clone(),
        hop_index: endpoint.hop_index,
        total_hops: Some(endpoint.total_hops),
        via_hop_index: endpoint.via_hop_index,
    }
}

fn proxy_failure_summary(
    operation: ProxyConnectOperation,
    phase: &TestConnectionPhase,
    category: &TestConnectionCategory,
    location: &TestConnectionLocation,
) -> String {
    if operation == ProxyConnectOperation::OpenTunnel {
        return match location.kind {
            TestConnectionLocationKind::JumpHost => format!(
                "Tunnel from jump host {} to jump host {} failed",
                location.via_hop_index.unwrap_or_default(),
                location.hop_index.unwrap_or_default(),
            ),
            TestConnectionLocationKind::Target => format!(
                "Tunnel from jump host {} to the target failed",
                location.via_hop_index.unwrap_or_default(),
            ),
        };
    }

    match location.kind {
        TestConnectionLocationKind::JumpHost => {
            let hop = location.hop_index.unwrap_or_default();
            match phase {
                TestConnectionPhase::Preparation => match category {
                    TestConnectionCategory::KeyMaterial => {
                        format!("Jump host {} authentication material is invalid", hop)
                    }
                    _ => format!("Jump host {} configuration is incomplete", hop),
                },
                TestConnectionPhase::HostKeyVerification => {
                    format!("Jump host {} host key verification failed", hop)
                }
                TestConnectionPhase::Transport => match category {
                    TestConnectionCategory::DnsResolution => {
                        format!("Jump host {} DNS resolution failed", hop)
                    }
                    TestConnectionCategory::Timeout => {
                        format!("Timed out reaching jump host {}", hop)
                    }
                    TestConnectionCategory::Protocol => {
                        format!("Jump host {} SSH transport negotiation failed", hop)
                    }
                    _ => format!("Jump host {} transport failed", hop),
                },
                TestConnectionPhase::Authentication => match category {
                    TestConnectionCategory::Agent => {
                        format!("Jump host {} SSH agent authentication failed", hop)
                    }
                    TestConnectionCategory::KeyMaterial => {
                        format!("Jump host {} key or certificate material could not be loaded", hop)
                    }
                    TestConnectionCategory::Timeout => {
                        format!("Timed out while authenticating to jump host {}", hop)
                    }
                    _ => format!("Jump host {} authentication failed", hop),
                },
                TestConnectionPhase::Complete => "Connection test succeeded".to_string(),
            }
        }
        TestConnectionLocationKind::Target => match phase {
            TestConnectionPhase::Preparation => direct_failure_summary(phase, category),
            TestConnectionPhase::HostKeyVerification => {
                "Target host key verification failed after traversing jump hosts".to_string()
            }
            TestConnectionPhase::Transport => match category {
                TestConnectionCategory::Timeout => {
                    "Timed out reaching the target through jump hosts".to_string()
                }
                TestConnectionCategory::Protocol => {
                    "Target SSH transport negotiation failed after jump hosts connected".to_string()
                }
                _ => "Target transport failed after jump hosts connected".to_string(),
            },
            TestConnectionPhase::Authentication => match category {
                TestConnectionCategory::Agent => {
                    "Target SSH agent authentication failed after jump hosts connected".to_string()
                }
                TestConnectionCategory::KeyMaterial => {
                    "Target key or certificate material could not be loaded after jump hosts connected"
                        .to_string()
                }
                TestConnectionCategory::Timeout => {
                    "Timed out while authenticating to the target through jump hosts".to_string()
                }
                _ => "Target authentication failed after jump hosts connected".to_string(),
            },
            TestConnectionPhase::Complete => "Connection test succeeded".to_string(),
        },
    }
}

fn build_direct_failure_diagnostic(
    request: &ConnectRequest,
    error: &SshError,
) -> TestConnectionDiagnostic {
    let (phase, category) = classify_test_connection_error(error);
    let location = target_location(request);

    TestConnectionDiagnostic {
        summary: direct_failure_summary(&phase, &category),
        detail: format!("While testing {}, {}", format_location(&location), error),
        phase,
        category,
        location: Some(location),
    }
}

fn build_proxy_failure_diagnostic(
    request: &ConnectRequest,
    error: &ProxyConnectError,
) -> TestConnectionDiagnostic {
    match error {
        ProxyConnectError::InvalidChain { detail, total_hops } => TestConnectionDiagnostic {
            phase: TestConnectionPhase::Preparation,
            category: TestConnectionCategory::Unsupported,
            summary: "Proxy chain configuration is invalid".to_string(),
            detail: format!(
                "While testing route with {} jump host(s), {}",
                total_hops, detail
            ),
            location: Some(target_location(request)),
        },
        ProxyConnectError::Step {
            operation,
            endpoint,
            source,
        } => {
            let location = location_from_proxy_endpoint(endpoint);
            let (_, classified_category) = classify_test_connection_error(source);
            let category = if *operation == ProxyConnectOperation::OpenTunnel {
                TestConnectionCategory::Tunnel
            } else {
                classified_category
            };
            let phase = match operation {
                ProxyConnectOperation::ResolveAddress => TestConnectionPhase::Transport,
                ProxyConnectOperation::Authenticate => TestConnectionPhase::Authentication,
                ProxyConnectOperation::OpenTunnel => TestConnectionPhase::Transport,
                ProxyConnectOperation::EstablishTransport => match category {
                    TestConnectionCategory::HostKeyUnknown
                    | TestConnectionCategory::HostKeyChanged => {
                        TestConnectionPhase::HostKeyVerification
                    }
                    _ => TestConnectionPhase::Transport,
                },
            };

            TestConnectionDiagnostic {
                phase,
                category,
                summary: proxy_failure_summary(*operation, &phase, &category, &location),
                detail: format!(
                    "While testing route {}, {} failed: {}",
                    route_overview(request),
                    format_location(&location),
                    source,
                ),
                location: Some(location),
            }
        }
    }
}

fn build_proxy_chain(requests: &[ProxyChainRequest]) -> Result<ProxyChain, (usize, String)> {
    let mut chain = ProxyChain::new();

    for (index, hop) in requests.iter().enumerate() {
        let auth = build_session_auth(hop.auth.clone()).map_err(|error| (index + 1, error))?;
        chain = chain.add_hop(ProxyHop {
            host: hop.host.clone(),
            port: hop.port,
            username: hop.username.clone(),
            auth,
        });
    }

    Ok(chain)
}

/// Test an SSH connection without creating a persistent session.
///
/// Performs the full SSH handshake + authentication, then immediately
/// disconnects. This verifies connectivity, authentication, and
/// reports the round-trip time.
#[tauri::command]
pub async fn test_connection(
    request: ConnectRequest,
) -> Result<TestConnectionResponse, String> {
    info!(
        "Testing connection: {}@{}:{}",
        request.username, request.host, request.port
    );

    let start = std::time::Instant::now();

    let auth = match build_session_auth(request.auth.clone()) {
        Ok(auth) => auth,
        Err(message) => {
            let category = if message.to_lowercase().contains("ssh key")
                || message.to_lowercase().contains("certificate")
            {
                TestConnectionCategory::KeyMaterial
            } else {
                TestConnectionCategory::Unknown
            };

            return Ok(build_failed_test_connection_response(
                start.elapsed().as_millis() as u64,
                TestConnectionPhase::Preparation,
                category,
                direct_failure_summary(&TestConnectionPhase::Preparation, &category),
                message,
                Some(target_location(&request)),
            ));
        }
    };

    let response = if let Some(proxy_chain_requests) =
        request.proxy_chain.as_ref().filter(|chain| !chain.is_empty())
    {
        let proxy_chain = match build_proxy_chain(proxy_chain_requests) {
            Ok(chain) => chain,
            Err((hop_index, message)) => {
                let location = proxy_chain_requests
                    .get(hop_index - 1)
                    .map(|hop| jump_host_location(hop, hop_index, proxy_chain_requests.len()));
                let category = if message.to_lowercase().contains("ssh key")
                    || message.to_lowercase().contains("certificate")
                {
                    TestConnectionCategory::KeyMaterial
                } else {
                    TestConnectionCategory::Unknown
                };

                return Ok(build_failed_test_connection_response(
                    start.elapsed().as_millis() as u64,
                    TestConnectionPhase::Preparation,
                    category,
                    match category {
                        TestConnectionCategory::KeyMaterial => {
                            format!("Jump host {} authentication material is invalid", hop_index)
                        }
                        _ => format!("Jump host {} configuration is incomplete", hop_index),
                    },
                    format!("Jump host {} could not be prepared: {}", hop_index, message),
                    location,
                ));
            }
        };

        match connect_via_proxy_for_test(
            &proxy_chain,
            &request.host,
            request.port,
            &request.username,
            &auth,
            30,
        )
        .await
        {
            Ok(connection) => {
                drop(connection);
                build_success_test_connection_response(
                    start.elapsed().as_millis() as u64,
                    Some(target_location(&request)),
                )
            }
            Err(error) => TestConnectionResponse {
                success: false,
                elapsed_ms: start.elapsed().as_millis() as u64,
                diagnostic: build_proxy_failure_diagnostic(&request, &error),
            },
        }
    } else {
        let config = SshConfig {
            host: request.host.clone(),
            port: request.port,
            username: request.username.clone(),
            auth,
            timeout_secs: 30,
            cols: request.cols,
            rows: request.rows,
            proxy_chain: None,
            strict_host_key_checking: true,
            trust_host_key: request.trust_host_key,
            expected_host_key_fingerprint: request.expected_host_key_fingerprint.clone(),
            agent_forwarding: false,
        };

        match SshClient::new(config).connect(None).await {
            Ok(session) => {
                drop(session);
                build_success_test_connection_response(
                    start.elapsed().as_millis() as u64,
                    Some(target_location(&request)),
                )
            }
            Err(error) => TestConnectionResponse {
                success: false,
                elapsed_ms: start.elapsed().as_millis() as u64,
                diagnostic: build_direct_failure_diagnostic(&request, &error),
            },
        }
    };

    info!(
        "Test connection finished (success={}) for {}@{}:{}",
        response.success,
        request.username,
        request.host,
        request.port
    );

    Ok(response)
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

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;
    use russh::keys::{Algorithm, PrivateKey};
    use std::path::PathBuf;

    fn fake_key_auth(path: &str) -> KeyAuth {
        let mut rng = OsRng;
        KeyAuth {
            key_path: PathBuf::from(path),
            key_pair: PrivateKey::random(&mut rng, Algorithm::Ed25519).unwrap(),
        }
    }

    #[test]
    fn test_build_session_auth_password() {
        let auth = build_session_auth(AuthRequest::Password {
            password: Zeroizing::new("secret".to_string()),
        })
        .unwrap();

        assert!(matches!(
            auth,
            AuthMethod::Password { password } if &*password == "secret"
        ));
    }

    #[test]
    fn test_build_session_auth_key() {
        let auth = build_session_auth(AuthRequest::Key {
            key_path: "/tmp/id_ed25519".to_string(),
            passphrase: Some(Zeroizing::new("pp".to_string())),
        })
        .unwrap();

        assert!(matches!(
            auth,
            AuthMethod::Key { key_path, passphrase }
                if key_path == "/tmp/id_ed25519" && passphrase.as_deref().map(|s| s.as_str()) == Some("pp")
        ));
    }

    #[test]
    fn test_build_session_auth_agent() {
        let auth = build_session_auth(AuthRequest::Agent).unwrap();

        assert!(matches!(auth, AuthMethod::Agent));
    }

    #[test]
    fn test_build_session_auth_default_key_uses_resolved_key_path() {
        let auth = build_session_auth_with(
            AuthRequest::DefaultKey {
                passphrase: Some(Zeroizing::new("pp".to_string())),
            },
            |passphrase| {
                assert_eq!(passphrase, Some("pp"));
                Ok(fake_key_auth("/tmp/id_default"))
            },
        )
        .unwrap();

        assert!(matches!(
            auth,
            AuthMethod::Key { key_path, passphrase }
                if key_path == "/tmp/id_default" && passphrase.as_deref().map(|s| s.as_str()) == Some("pp")
        ));
    }

    #[test]
    fn test_build_session_auth_default_key_propagates_lookup_errors() {
        let error = build_session_auth_with(AuthRequest::DefaultKey { passphrase: None }, |_| {
            Err("No SSH key found: missing key".to_string())
        })
        .unwrap_err();

        assert_eq!(error, "No SSH key found: missing key");
    }

    #[test]
    fn test_build_proxy_chain_preserves_hop_order() {
        let requests = vec![
            ProxyChainRequest {
                host: "jump-1.example.com".to_string(),
                port: 22,
                username: "tester-1".to_string(),
                auth: AuthRequest::Agent,
            },
            ProxyChainRequest {
                host: "jump-2.example.com".to_string(),
                port: 2222,
                username: "tester-2".to_string(),
                auth: AuthRequest::Agent,
            },
        ];

        let chain = build_proxy_chain(&requests).expect("proxy chain");

        assert_eq!(chain.hops.len(), 2);
        assert_eq!(chain.hops[0].host, "jump-1.example.com");
        assert_eq!(chain.hops[1].port, 2222);
    }

    #[test]
    fn test_build_proxy_failure_diagnostic_marks_tunnel_step() {
        let request = ConnectRequest {
            host: "target.example.com".to_string(),
            port: 22,
            username: "target-user".to_string(),
            auth: AuthRequest::Agent,
            cols: default_cols(),
            rows: default_rows(),
            name: Some("Target".to_string()),
            proxy_chain: Some(vec![
                ProxyChainRequest {
                    host: "jump-1.example.com".to_string(),
                    port: 22,
                    username: "jump1".to_string(),
                    auth: AuthRequest::Agent,
                },
                ProxyChainRequest {
                    host: "jump-2.example.com".to_string(),
                    port: 2222,
                    username: "jump2".to_string(),
                    auth: AuthRequest::Agent,
                },
            ]),
            trust_host_key: None,
            expected_host_key_fingerprint: None,
            buffer_config: None,
        };

        let diagnostic = build_proxy_failure_diagnostic(
            &request,
            &ProxyConnectError::Step {
                operation: ProxyConnectOperation::OpenTunnel,
                endpoint: ProxyConnectEndpoint {
                    kind: ProxyConnectEndpointKind::JumpHost,
                    host: "jump-2.example.com".to_string(),
                    port: 2222,
                    username: "jump2".to_string(),
                    hop_index: Some(2),
                    total_hops: 2,
                    via_hop_index: Some(1),
                },
                source: SshError::ConnectionFailed(
                    "administratively prohibited".to_string(),
                ),
            },
        );

        assert!(matches!(diagnostic.phase, TestConnectionPhase::Transport));
        assert!(matches!(diagnostic.category, TestConnectionCategory::Tunnel));
        assert_eq!(diagnostic.summary, "Tunnel from jump host 1 to jump host 2 failed");
        let location = diagnostic.location.expect("missing location");
        assert!(matches!(location.kind, TestConnectionLocationKind::JumpHost));
        assert_eq!(location.hop_index, Some(2));
        assert_eq!(location.via_hop_index, Some(1));
    }

    #[test]
    fn test_build_proxy_failure_diagnostic_marks_target_authentication() {
        let request = ConnectRequest {
            host: "target.example.com".to_string(),
            port: 22,
            username: "target-user".to_string(),
            auth: AuthRequest::Agent,
            cols: default_cols(),
            rows: default_rows(),
            name: Some("Target".to_string()),
            proxy_chain: Some(vec![ProxyChainRequest {
                host: "jump-1.example.com".to_string(),
                port: 22,
                username: "jump1".to_string(),
                auth: AuthRequest::Agent,
            }]),
            trust_host_key: None,
            expected_host_key_fingerprint: None,
            buffer_config: None,
        };

        let diagnostic = build_proxy_failure_diagnostic(
            &request,
            &ProxyConnectError::Step {
                operation: ProxyConnectOperation::Authenticate,
                endpoint: ProxyConnectEndpoint {
                    kind: ProxyConnectEndpointKind::Target,
                    host: "target.example.com".to_string(),
                    port: 22,
                    username: "target-user".to_string(),
                    hop_index: None,
                    total_hops: 1,
                    via_hop_index: None,
                },
                source: SshError::AuthenticationFailed(
                    "Authentication rejected by server".to_string(),
                ),
            },
        );

        assert!(matches!(diagnostic.phase, TestConnectionPhase::Authentication));
        assert!(matches!(diagnostic.category, TestConnectionCategory::Authentication));
        assert_eq!(
            diagnostic.summary,
            "Target authentication failed after jump hosts connected"
        );
        let location = diagnostic.location.expect("missing location");
        assert!(matches!(location.kind, TestConnectionLocationKind::Target));
    }
}
