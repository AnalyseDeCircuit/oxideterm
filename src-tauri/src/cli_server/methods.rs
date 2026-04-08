// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! RPC method implementations for CLI server.
//!
//! Each method receives JSON params and an AppHandle,
//! extracts the needed state via `app.state()`, and
//! returns a JSON value or error tuple.

use serde_json::{Value, json};
use std::sync::Arc;
use tauri::Manager;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

use super::protocol;
use crate::bridge::{BridgeManager, WsBridge};
use crate::commands::config::ConfigState;
use crate::commands::forwarding::ForwardingRegistry;
use crate::commands::{HealthRegistry, ProfilerRegistry};
use crate::config::ssh_config::parse_ssh_config;
use crate::config::{CONFIG_VERSION, SavedAuth, SavedConnection};
use crate::router::NodeRouter;
use crate::session::SessionRegistry;
use crate::sftp::progress::DummyProgressStore;
use crate::sftp::session::SftpRegistry;
use crate::ssh::{ExtendedSessionHandle, SessionCommand, SshConnectionRegistry};
use crate::state::{AiChatStore, ConversationMeta, PersistedMessage};

const CLI_API_CURRENT_VERSION: u64 = 1;
const CLI_API_MIN_SUPPORTED_VERSION: u64 = 1;

/// Dispatch a JSON-RPC method call to the appropriate handler.
pub async fn dispatch(
    method: &str,
    params: Value,
    app: &tauri::AppHandle,
) -> Result<Value, (i32, String)> {
    match method {
        "status" => status(app).await,
        "list_saved_connections" => list_saved_connections(app).await,
        "list_sessions" => list_sessions(app).await,
        "list_active_connections" => list_active_connections(app).await,
        "list_forwards" => list_forwards(app, params).await,
        "health" => health(app, params).await,
        "disconnect" => disconnect(app, params).await,
        "ping" => Ok(json!({ "pong": true })),
        "config_list" => config_list(app).await,
        "config_get" => config_get(app, params).await,
        "create_forward" => create_forward(app, params).await,
        "delete_forward" => delete_forward(app, params).await,
        "connect" => connect(app, params).await,
        "open_tab" => open_tab(app, params).await,
        "focus_tab" => focus_tab(app, params).await,
        "list_local_terminals" => list_local_terminals(app).await,
        "attach" => attach(app, params).await,
        "sftp_ls" => sftp_ls(app, params).await,
        "sftp_get" => sftp_get(app, params).await,
        "sftp_put" => sftp_put(app, params).await,
        "import_list" => import_list(app).await,
        "import_hosts" => import_hosts(app, params).await,
        _ => Err((
            protocol::ERR_METHOD_NOT_FOUND,
            format!("Method not found: {method}"),
        )),
    }
}

/// Return application status summary.
async fn status(app: &tauri::AppHandle) -> Result<Value, (i32, String)> {
    let version = env!("CARGO_PKG_VERSION");

    let session_count = app
        .try_state::<Arc<SessionRegistry>>()
        .map(|r| r.list().len())
        .unwrap_or(0);

    let ssh_count = if let Some(registry) = app.try_state::<Arc<SshConnectionRegistry>>() {
        registry.inner().list_connections().await.len()
    } else {
        0
    };

    #[cfg(feature = "local-terminal")]
    let local_count =
        if let Some(s) = app.try_state::<Arc<crate::commands::local::LocalTerminalState>>() {
            s.registry.list_sessions().await.len()
        } else {
            0
        };
    #[cfg(not(feature = "local-terminal"))]
    let local_count = 0usize;

    Ok(json!({
        "version": version,
        "cli_api": {
            "version": CLI_API_CURRENT_VERSION,
            "min_supported": CLI_API_MIN_SUPPORTED_VERSION,
        },
        "sessions": session_count,
        "connections": {
            "ssh": ssh_count,
            "local": local_count,
        },
        "pid": std::process::id(),
    }))
}

/// List saved connection configurations (from connections.json).
async fn list_saved_connections(app: &tauri::AppHandle) -> Result<Value, (i32, String)> {
    let config_state = app
        .try_state::<Arc<ConfigState>>()
        .ok_or((protocol::ERR_INTERNAL, "Config not initialized".to_string()))?;

    let config = config_state.inner().get_config_snapshot();
    let connections: Vec<Value> = config
        .connections
        .iter()
        .map(|conn| {
            let (auth_type, key_path) = match &conn.auth {
                crate::config::SavedAuth::Password { .. } => ("password", None),
                crate::config::SavedAuth::Key { key_path, .. } => ("key", Some(key_path.as_str())),
                crate::config::SavedAuth::Certificate { key_path, .. } => {
                    ("certificate", Some(key_path.as_str()))
                }
                crate::config::SavedAuth::Agent => ("agent", None),
            };
            json!({
                "id": conn.id,
                "name": conn.name,
                "host": conn.host,
                "port": conn.port,
                "username": conn.username,
                "auth_type": auth_type,
                "key_path": key_path,
                "group": conn.group,
            })
        })
        .collect();

    Ok(json!(connections))
}

/// List active SSH sessions.
async fn list_sessions(app: &tauri::AppHandle) -> Result<Value, (i32, String)> {
    let registry = app.try_state::<Arc<SessionRegistry>>().ok_or((
        protocol::ERR_INTERNAL,
        "Session registry not initialized".to_string(),
    ))?;

    let sessions: Vec<Value> = registry
        .list()
        .iter()
        .map(|s| {
            json!({
                "id": s.id,
                "name": s.name,
                "host": s.host,
                "port": s.port,
                "username": s.username,
                "state": format!("{:?}", s.state),
                "uptime_secs": s.uptime_secs,
                "auth_type": s.auth_type,
                "connection_id": s.connection_id,
            })
        })
        .collect();

    Ok(json!(sessions))
}

/// List active local terminal sessions.
async fn list_local_terminals(app: &tauri::AppHandle) -> Result<Value, (i32, String)> {
    #[cfg(feature = "local-terminal")]
    {
        let state = app
            .try_state::<Arc<crate::commands::local::LocalTerminalState>>()
            .ok_or((
                protocol::ERR_INTERNAL,
                "Local terminal state not initialized".to_string(),
            ))?;

        let terminals: Vec<Value> = state
            .registry
            .list_sessions()
            .await
            .iter()
            .map(|t| {
                json!({
                    "id": t.id,
                    "shell_name": t.shell.label,
                    "shell_id": t.shell.id,
                    "running": t.running,
                    "detached": t.detached,
                })
            })
            .collect();

        Ok(json!(terminals))
    }

    #[cfg(not(feature = "local-terminal"))]
    Ok(json!([]))
}

/// List active SSH connections in the pool.
async fn list_active_connections(app: &tauri::AppHandle) -> Result<Value, (i32, String)> {
    let registry = app.try_state::<Arc<SshConnectionRegistry>>().ok_or((
        protocol::ERR_INTERNAL,
        "SSH connection registry not initialized".to_string(),
    ))?;

    let connections = registry.inner().list_connections().await;
    let result: Vec<Value> = connections
        .iter()
        .map(|c| serde_json::to_value(c).unwrap_or(json!(null)))
        .collect();

    Ok(json!(result))
}

/// List port forwards, optionally filtered by session_id.
async fn list_forwards(app: &tauri::AppHandle, params: Value) -> Result<Value, (i32, String)> {
    let forwarding_registry = app.try_state::<Arc<ForwardingRegistry>>().ok_or((
        protocol::ERR_INTERNAL,
        "Forwarding registry not initialized".to_string(),
    ))?;

    let session_registry = app.try_state::<Arc<SessionRegistry>>().ok_or((
        protocol::ERR_INTERNAL,
        "Session registry not initialized".to_string(),
    ))?;

    let session_filter = params
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let session_ids: Vec<String> = if let Some(sid) = session_filter {
        vec![sid]
    } else {
        session_registry
            .list()
            .iter()
            .map(|s| s.id.clone())
            .collect()
    };

    let mut all_forwards = Vec::new();
    for sid in &session_ids {
        if let Some(manager) = forwarding_registry.get(sid).await {
            let forwards = manager.list_forwards().await;
            for rule in forwards {
                all_forwards.push(json!({
                    "session_id": sid,
                    "id": rule.id,
                    "forward_type": format!("{:?}", rule.forward_type).to_lowercase(),
                    "bind_address": rule.bind_address,
                    "bind_port": rule.bind_port,
                    "target_host": rule.target_host,
                    "target_port": rule.target_port,
                    "status": format!("{:?}", rule.status).to_lowercase(),
                    "description": rule.description,
                }));
            }
        }
    }

    Ok(json!(all_forwards))
}

/// Get health status for one or all sessions.
async fn health(app: &tauri::AppHandle, params: Value) -> Result<Value, (i32, String)> {
    let health_registry = app.try_state::<HealthRegistry>().ok_or((
        protocol::ERR_INTERNAL,
        "Health registry not initialized".to_string(),
    ))?;

    if let Some(session_id) = params.get("session_id").and_then(|v| v.as_str()) {
        // Single session health
        let tracker = health_registry.get(session_id).ok_or((
            protocol::ERR_INVALID_PARAMS,
            format!("No health tracker for session: {session_id}"),
        ))?;

        let metrics = tracker.metrics().await;
        let check =
            crate::session::QuickHealthCheck::from_metrics(session_id.to_string(), &metrics);
        serde_json::to_value(&check).map_err(|e| (protocol::ERR_INTERNAL, e.to_string()))
    } else {
        // All sessions health
        let session_ids = health_registry.session_ids();
        let mut results = serde_json::Map::new();

        for session_id in session_ids {
            if let Some(tracker) = health_registry.get(&session_id) {
                if tracker.is_active() {
                    let metrics = tracker.metrics().await;
                    let check = crate::session::QuickHealthCheck::from_metrics(
                        session_id.clone(),
                        &metrics,
                    );
                    if let Ok(val) = serde_json::to_value(&check) {
                        results.insert(session_id, val);
                    }
                }
            }
        }

        Ok(Value::Object(results))
    }
}

/// Disconnect a session by ID or name.
async fn disconnect(app: &tauri::AppHandle, params: Value) -> Result<Value, (i32, String)> {
    let target = params.get("target").and_then(|v| v.as_str()).ok_or((
        protocol::ERR_INVALID_PARAMS,
        "Missing required parameter: target".to_string(),
    ))?;

    let registry = app.try_state::<Arc<SessionRegistry>>().ok_or((
        protocol::ERR_INTERNAL,
        "Session registry not initialized".to_string(),
    ))?;

    // Resolve target: try as session ID first, then match by name
    let session_id = {
        let sessions = registry.list();
        if sessions.iter().any(|s| s.id == target) {
            target.to_string()
        } else if let Some(s) = sessions.iter().find(|s| s.name == target) {
            s.id.clone()
        } else {
            return Err((
                protocol::ERR_INVALID_PARAMS,
                format!("Session not found: {target}"),
            ));
        }
    };

    // Persist buffer before disconnect
    if let Err(e) = registry.persist_session_with_buffer(&session_id).await {
        tracing::warn!("Failed to persist session buffer before CLI disconnect: {e}");
    }

    // Stop and remove all port forwards for this session
    if let Some(fwd_registry) = app.try_state::<Arc<ForwardingRegistry>>() {
        fwd_registry.remove(&session_id).await;
    }

    // Close session via registry
    registry
        .close_session(&session_id)
        .await
        .map_err(|e| (protocol::ERR_INTERNAL, e))?;

    // Complete disconnection
    let _ = registry.disconnect_complete(&session_id, true);

    // Clean up bridge manager
    if let Some(bridge_manager) = app.try_state::<BridgeManager>() {
        bridge_manager.unregister(&session_id);
    }

    // Clean up SFTP cache
    if let Some(sftp_registry) = app.try_state::<Arc<SftpRegistry>>() {
        sftp_registry.remove(&session_id);
    }

    // Clean up health tracker and profiler
    if let Some(health_reg) = app.try_state::<HealthRegistry>() {
        health_reg.remove(&session_id);
    }
    if let Some(profiler_reg) = app.try_state::<ProfilerRegistry>() {
        profiler_reg.remove(&session_id);
    }

    // Release connection from pool
    if let Some(conn_registry) = app.try_state::<Arc<SshConnectionRegistry>>() {
        if let Err(e) = conn_registry.release(&session_id).await {
            tracing::warn!("Failed to release connection from pool: {e}");
        }
    }

    Ok(json!({
        "success": true,
        "session_id": session_id,
    }))
}

// ═══════════════════════════════════════════════════════════════════════════
// Phase D: Config Query
// ═══════════════════════════════════════════════════════════════════════════

/// List connection groups with counts.
async fn config_list(app: &tauri::AppHandle) -> Result<Value, (i32, String)> {
    let config_state = app
        .try_state::<Arc<ConfigState>>()
        .ok_or((protocol::ERR_INTERNAL, "Config not initialized".to_string()))?;

    let config = config_state.inner().get_config_snapshot();

    let mut groups: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for conn in &config.connections {
        let group_name = conn.group.as_deref().unwrap_or("(ungrouped)").to_string();
        *groups.entry(group_name).or_insert(0) += 1;
    }

    let groups_json: Vec<Value> = groups
        .iter()
        .map(|(name, count)| json!({ "name": name, "count": count }))
        .collect();

    Ok(json!({
        "total_connections": config.connections.len(),
        "groups": groups_json,
    }))
}

/// Get details of a connection by name (no password/key content exposed).
async fn config_get(app: &tauri::AppHandle, params: Value) -> Result<Value, (i32, String)> {
    let name = params.get("name").and_then(|v| v.as_str()).ok_or((
        protocol::ERR_INVALID_PARAMS,
        "Missing required parameter: name".to_string(),
    ))?;

    let config_state = app
        .try_state::<Arc<ConfigState>>()
        .ok_or((protocol::ERR_INTERNAL, "Config not initialized".to_string()))?;

    let config = config_state.inner().get_config_snapshot();
    let conn = config
        .connections
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case(name) || c.id == name)
        .ok_or((
            protocol::ERR_INVALID_PARAMS,
            format!("Connection not found: {name}"),
        ))?;

    let (auth_type, key_path) = match &conn.auth {
        crate::config::SavedAuth::Password { .. } => ("password", None),
        crate::config::SavedAuth::Key { key_path, .. } => ("key", Some(key_path.as_str())),
        crate::config::SavedAuth::Certificate { key_path, .. } => {
            ("certificate", Some(key_path.as_str()))
        }
        crate::config::SavedAuth::Agent => ("agent", None),
    };

    Ok(json!({
        "id": conn.id,
        "name": conn.name,
        "host": conn.host,
        "port": conn.port,
        "username": conn.username,
        "auth_type": auth_type,
        "key_path": key_path,
        "group": conn.group,
        "tags": conn.tags,
        "color": conn.color,
        "options": {
            "keep_alive_interval": conn.options.keep_alive_interval,
            "compression": conn.options.compression,
            "jump_host": conn.options.jump_host,
            "term_type": conn.options.term_type,
        },
        "proxy_chain": conn.proxy_chain.iter().map(|p| json!({
            "host": p.host,
            "port": p.port,
            "username": p.username,
        })).collect::<Vec<_>>(),
        "created_at": conn.created_at.to_rfc3339(),
        "last_used_at": conn.last_used_at.map(|t| t.to_rfc3339()),
    }))
}

// ═══════════════════════════════════════════════════════════════════════════
// Phase B: Port Forwarding Management
// ═══════════════════════════════════════════════════════════════════════════

/// Create a port forward on an active session.
async fn create_forward(app: &tauri::AppHandle, params: Value) -> Result<Value, (i32, String)> {
    let session_id = params.get("session_id").and_then(|v| v.as_str()).ok_or((
        protocol::ERR_INVALID_PARAMS,
        "Missing required parameter: session_id".to_string(),
    ))?;

    let forward_type_str = params
        .get("forward_type")
        .and_then(|v| v.as_str())
        .unwrap_or("local");

    let bind_address = params
        .get("bind_address")
        .and_then(|v| v.as_str())
        .unwrap_or("127.0.0.1")
        .to_string();

    let bind_port = params.get("bind_port").and_then(|v| v.as_u64()).ok_or((
        protocol::ERR_INVALID_PARAMS,
        "Missing required parameter: bind_port".to_string(),
    ))? as u16;

    let target_host = params
        .get("target_host")
        .and_then(|v| v.as_str())
        .unwrap_or("localhost")
        .to_string();

    let target_port = params
        .get("target_port")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u16;

    let description = params
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    use crate::forwarding::{ForwardRule, ForwardStatus, ForwardType};

    let fwd_type = match forward_type_str {
        "local" => ForwardType::Local,
        "remote" => ForwardType::Remote,
        "dynamic" => ForwardType::Dynamic,
        other => {
            return Err((
                protocol::ERR_INVALID_PARAMS,
                format!("Unknown forward type: {other}"),
            ));
        }
    };

    // For non-dynamic forwards, target_port is required
    if fwd_type != ForwardType::Dynamic && target_port == 0 {
        return Err((
            protocol::ERR_INVALID_PARAMS,
            "Missing required parameter: target_port".to_string(),
        ));
    }

    let forwarding_registry = app.try_state::<Arc<ForwardingRegistry>>().ok_or((
        protocol::ERR_INTERNAL,
        "Forwarding registry not initialized".to_string(),
    ))?;

    let mgr = forwarding_registry.get(session_id).await.ok_or((
        protocol::ERR_NOT_CONNECTED,
        format!("No forwarding manager for session: {session_id}"),
    ))?;

    let rule = ForwardRule {
        id: uuid::Uuid::new_v4().to_string(),
        forward_type: fwd_type,
        bind_address,
        bind_port,
        target_host,
        target_port,
        status: ForwardStatus::Starting,
        description,
    };

    match mgr.create_forward(rule).await {
        Ok(created) => {
            let forward_id = created.id.clone();
            tracing::info!("CLI: Port forward created: {forward_id}");

            // Update ConnectionRegistry
            if let Some(conn_registry) = app.try_state::<Arc<SshConnectionRegistry>>() {
                if let Err(e) = conn_registry.add_forward(session_id, forward_id).await {
                    tracing::warn!("Failed to update forward state in ConnectionRegistry: {e}");
                }
            }

            Ok(json!({
                "success": true,
                "forward": {
                    "id": created.id,
                    "forward_type": format!("{:?}", created.forward_type).to_lowercase(),
                    "bind_address": created.bind_address,
                    "bind_port": created.bind_port,
                    "target_host": created.target_host,
                    "target_port": created.target_port,
                    "status": format!("{:?}", created.status).to_lowercase(),
                    "description": created.description,
                },
            }))
        }
        Err(e) => Ok(json!({
            "success": false,
            "error": e.to_string(),
        })),
    }
}

/// Delete a port forward from an active session.
async fn delete_forward(app: &tauri::AppHandle, params: Value) -> Result<Value, (i32, String)> {
    let session_id = params.get("session_id").and_then(|v| v.as_str()).ok_or((
        protocol::ERR_INVALID_PARAMS,
        "Missing required parameter: session_id".to_string(),
    ))?;

    let forward_id = params.get("forward_id").and_then(|v| v.as_str()).ok_or((
        protocol::ERR_INVALID_PARAMS,
        "Missing required parameter: forward_id".to_string(),
    ))?;

    let forwarding_registry = app.try_state::<Arc<ForwardingRegistry>>().ok_or((
        protocol::ERR_INTERNAL,
        "Forwarding registry not initialized".to_string(),
    ))?;

    let mgr = forwarding_registry.get(session_id).await.ok_or((
        protocol::ERR_NOT_CONNECTED,
        format!("No forwarding manager for session: {session_id}"),
    ))?;

    match mgr.delete_forward(forward_id).await {
        Ok(()) => {
            tracing::info!("CLI: Port forward deleted: {forward_id}");
            Ok(json!({
                "success": true,
                "forward_id": forward_id,
            }))
        }
        Err(e) => Ok(json!({
            "success": false,
            "error": e.to_string(),
        })),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Phase A: Connection Control
// ═══════════════════════════════════════════════════════════════════════════

/// Trigger the GUI to connect to a saved connection.
async fn connect(app: &tauri::AppHandle, params: Value) -> Result<Value, (i32, String)> {
    let target = params.get("target").and_then(|v| v.as_str()).ok_or((
        protocol::ERR_INVALID_PARAMS,
        "Missing required parameter: target".to_string(),
    ))?;

    // Resolve target to a saved connection
    let config_state = app
        .try_state::<Arc<ConfigState>>()
        .ok_or((protocol::ERR_INTERNAL, "Config not initialized".to_string()))?;

    let config = config_state.inner().get_config_snapshot();
    let conn = config
        .connections
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case(target) || c.id == target || c.host == target)
        .ok_or((
            protocol::ERR_INVALID_PARAMS,
            format!("Connection not found: {target}"),
        ))?;

    // Emit event for frontend to handle
    use tauri::Emitter;
    app.emit(
        "cli:connect",
        json!({
            "connection_id": conn.id,
            "name": conn.name,
            "host": conn.host,
        }),
    )
    .map_err(|e| (protocol::ERR_INTERNAL, format!("Failed to emit event: {e}")))?;

    Ok(json!({
        "success": true,
        "connection_id": conn.id,
        "name": conn.name,
    }))
}

/// Open a new local terminal tab in the GUI.
async fn open_tab(app: &tauri::AppHandle, params: Value) -> Result<Value, (i32, String)> {
    let path = params
        .get("path")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    use tauri::Emitter;
    app.emit("cli:open-tab", json!({ "path": path }))
        .map_err(|e| (protocol::ERR_INTERNAL, format!("Failed to emit event: {e}")))?;

    Ok(json!({ "success": true }))
}

/// Focus an existing tab in the GUI.
///
/// Resolution order:
/// 1. SSH session (by ID or name)
/// 2. Local terminal (by ID or shell name)
/// 3. Raw target passed to frontend (matches by tab title/id)
async fn focus_tab(app: &tauri::AppHandle, params: Value) -> Result<Value, (i32, String)> {
    let target = params.get("target").and_then(|v| v.as_str()).ok_or((
        protocol::ERR_INVALID_PARAMS,
        "Missing required parameter: target".to_string(),
    ))?;

    use tauri::Emitter;

    // 1. Try SSH session registry (by ID, name, or prefix)
    if let Some(registry) = app.try_state::<Arc<SessionRegistry>>() {
        let sessions = registry.list();
        if let Some(session) = sessions
            .iter()
            .find(|s| s.id == target || s.name == target)
            .or_else(|| sessions.iter().find(|s| s.id.starts_with(target)))
        {
            app.emit("cli:focus-tab", json!({ "session_id": session.id }))
                .map_err(|e| (protocol::ERR_INTERNAL, format!("Failed to emit event: {e}")))?;

            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }

            return Ok(json!({
                "success": true,
                "matched": "session",
                "session_id": session.id,
            }));
        }
    }

    // 2. Try local terminal registry (by ID or shell name)
    #[cfg(feature = "local-terminal")]
    if let Some(state) = app.try_state::<Arc<crate::commands::local::LocalTerminalState>>() {
        let locals = state.registry.list_sessions().await;
        if let Some(local) = locals.iter().find(|l| l.id == target).or_else(|| {
            let lower = target.to_lowercase();
            locals.iter().find(|l| {
                l.shell.id.to_lowercase() == lower || l.shell.label.to_lowercase() == lower
            })
        }) {
            app.emit("cli:focus-tab", json!({ "session_id": local.id }))
                .map_err(|e| (protocol::ERR_INTERNAL, format!("Failed to emit event: {e}")))?;

            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }

            return Ok(json!({
                "success": true,
                "matched": "local_terminal",
                "session_id": local.id,
            }));
        }
    }

    // 3. Fallback: pass raw target to frontend for title/id matching
    app.emit("cli:focus-tab", json!({ "target": target }))
        .map_err(|e| (protocol::ERR_INTERNAL, format!("Failed to emit event: {e}")))?;

    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_focus();
    }

    Ok(json!({
        "success": true,
        "matched": "frontend",
        "target": target,
    }))
}

// ═══════════════════════════════════════════════════════════════════════════
// Phase C: AI Pipeline (streaming)
// ═══════════════════════════════════════════════════════════════════════════

/// Built-in provider defaults (baseUrl, defaultModel, keychain ID).
struct BuiltinProvider {
    keychain_id: &'static str,
    provider_type: &'static str,
    base_url: &'static str,
    default_model: &'static str,
}

const BUILTIN_PROVIDERS: &[BuiltinProvider] = &[
    BuiltinProvider {
        keychain_id: "builtin-openai",
        provider_type: "openai",
        base_url: "https://api.openai.com/v1",
        default_model: "gpt-4o-mini",
    },
    BuiltinProvider {
        keychain_id: "builtin-anthropic",
        provider_type: "anthropic",
        base_url: "https://api.anthropic.com",
        default_model: "claude-sonnet-4-20250514",
    },
    BuiltinProvider {
        keychain_id: "builtin-gemini",
        provider_type: "gemini",
        base_url: "https://generativelanguage.googleapis.com/v1beta",
        default_model: "gemini-2.0-flash",
    },
    BuiltinProvider {
        keychain_id: "builtin-ollama",
        provider_type: "ollama",
        base_url: "http://localhost:11434",
        default_model: "llama3.2",
    },
];

/// Maximum prompt size (50 KB).
const MAX_PROMPT_SIZE: usize = 50_000;
/// Maximum piped context size (500 KB).
const MAX_CONTEXT_SIZE: usize = 500_000;
/// Maximum terminal buffer lines for AI context.
const MAX_TERMINAL_BUFFER_LINES: usize = 2_000;
/// HTTP request timeout for AI API calls.
const AI_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);
/// Maximum response tokens for Anthropic API.
const ANTHROPIC_MAX_TOKENS: u32 = 4096;
/// Fraction of context window allocated to conversation history.
const HISTORY_BUDGET_RATIO: f64 = 0.7;
/// Safety margin multiplier for heuristic token estimates.
const TOKEN_SAFETY_MARGIN: f64 = 1.15;
/// Default context window for unknown models.
const DEFAULT_CONTEXT_WINDOW: usize = 8192;

/// Heuristic token estimation (matches frontend tokenUtils.ts).
///
/// CJK characters ≈ 1.5 tokens/char, non-CJK ≈ 0.25 tokens/char.
/// A ×1.15 safety margin compensates for heuristic imprecision.
fn estimate_tokens(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    let mut cjk_count = 0usize;
    for c in text.chars() {
        if matches!(c,
            '\u{4e00}'..='\u{9fff}'   // CJK Unified Ideographs
            | '\u{3040}'..='\u{309f}' // Hiragana
            | '\u{30a0}'..='\u{30ff}' // Katakana
            | '\u{ac00}'..='\u{d7af}' // Hangul Syllables
        ) {
            cjk_count += 1;
        }
    }
    let non_cjk_len = text.len().saturating_sub(cjk_count);
    let raw = cjk_count as f64 * 1.5 + non_cjk_len as f64 * 0.25;
    (raw * TOKEN_SAFETY_MARGIN).ceil() as usize
}

/// Get model context window size by name pattern (matches frontend MODEL_CONTEXT_WINDOWS).
fn get_model_context_window(model: &str) -> usize {
    let m = model.to_lowercase();
    // OpenAI
    if m.contains("gpt-4.1") {
        return 1_048_576;
    }
    if m.starts_with("o1") || m.starts_with("o2") || m.starts_with("o3") {
        return 200_000;
    }
    if m.contains("gpt-4o-mini") {
        return 128_000;
    }
    if m.contains("gpt-4-turbo") || m.contains("gpt-4o") {
        return 128_000;
    }
    if m.contains("gpt-4") {
        return 8_192;
    }
    if m.contains("gpt-3.5") {
        return 4_096;
    }
    // Anthropic
    if m.contains("claude") {
        return 200_000;
    }
    // Google
    if m.contains("gemini-2") || m.contains("gemini-1.5") {
        return 1_048_576;
    }
    if m.contains("gemini") {
        return 128_000;
    }
    // Meta
    if m.contains("llama-4") {
        return 1_048_576;
    }
    if m.contains("llama-3.1") || m.contains("llama-3.2") || m.contains("llama-3.3") {
        return 128_000;
    }
    if m.contains("llama") {
        return 8_192;
    }
    // Others
    if m.contains("qwen") {
        return 128_000;
    }
    if m.contains("deepseek") {
        return 128_000;
    }
    if m.contains("mistral-large") || m.contains("mistral-medium") {
        return 128_000;
    }
    if m.contains("mistral") {
        return 32_000;
    }
    // Moonshot (common for this user)
    if m.contains("moonshot") {
        return 128_000;
    }
    DEFAULT_CONTEXT_WINDOW
}

/// Trim conversation history to fit within the token budget.
///
/// Strategy: sliding window — keeps the most recent messages, drops oldest first.
/// Always preserves the last user message (which is the current request).
fn trim_history_to_budget(history: &[Value], model: &str) -> Vec<Value> {
    if history.is_empty() {
        return vec![];
    }

    let context_window = get_model_context_window(model);
    // Budget for history = context_window × HISTORY_BUDGET_RATIO,
    // minus a reserve for system prompt (~200 tokens) and response (~4096 tokens)
    let budget = ((context_window as f64 * HISTORY_BUDGET_RATIO) as usize)
        .saturating_sub(200) // system prompt reserve
        .saturating_sub(4096); // response reserve

    let mut total_tokens = 0usize;
    let mut kept = Vec::new();

    // Walk from newest to oldest, accumulate until budget exceeded
    for msg in history.iter().rev() {
        let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
        let msg_tokens = estimate_tokens(content) + 4; // 4 tokens overhead per message
        if total_tokens + msg_tokens > budget && !kept.is_empty() {
            break;
        }
        total_tokens += msg_tokens;
        kept.push(msg.clone());
    }

    // Reverse back to chronological order
    kept.reverse();
    kept
}

/// Escape XML-like control characters before embedding text inside pseudo tags.
fn escape_tagged_text(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Dispatch the `ask` RPC method with streaming support.
///
/// Writes `stream_chunk` notifications to the writer as AI tokens arrive,
/// then returns the final result.
pub async fn dispatch_streaming<W: AsyncWriteExt + Unpin>(
    params: Value,
    app: &tauri::AppHandle,
    writer: &mut W,
) -> Result<Value, (i32, String)> {
    ask(app, params, writer).await
}

/// AI ask implementation with streaming and conversation persistence.
async fn ask<W: AsyncWriteExt + Unpin>(
    app: &tauri::AppHandle,
    params: Value,
    writer: &mut W,
) -> Result<Value, (i32, String)> {
    let prompt = params
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let context = params
        .get("context")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Validate input sizes
    if prompt.len() > MAX_PROMPT_SIZE {
        return Err((
            protocol::ERR_INVALID_PARAMS,
            format!("Prompt exceeds maximum size ({MAX_PROMPT_SIZE} bytes)"),
        ));
    }
    if let Some(ref ctx) = context {
        if ctx.len() > MAX_CONTEXT_SIZE {
            return Err((
                protocol::ERR_INVALID_PARAMS,
                format!("Context exceeds maximum size ({MAX_CONTEXT_SIZE} bytes)"),
            ));
        }
    }

    let session_id = params
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let model_override = params
        .get("model")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let provider_override = params
        .get("provider")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let stream = params
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let exec_mode = params
        .get("exec_mode")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let conversation_id = params
        .get("conversation_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Resolve provider
    let config_state = app
        .try_state::<Arc<ConfigState>>()
        .ok_or((protocol::ERR_INTERNAL, "Config not initialized".to_string()))?;

    if prompt.is_empty() && context.is_none() {
        return Err((
            protocol::ERR_INVALID_PARAMS,
            "Prompt cannot be empty".to_string(),
        ));
    }

    let (provider_type, base_url, api_key, model) = resolve_ai_provider(
        app,
        &config_state,
        provider_override.as_deref(),
        model_override.as_deref(),
    )
    .await?;

    // Optionally get terminal buffer as context
    let terminal_context = if let Some(sid) = &session_id {
        get_terminal_buffer(app, sid).await.ok()
    } else {
        None
    };

    // Build system prompt
    let system_prompt = if exec_mode {
        "You are a code generator. Output only executable code or commands. No explanations, no markdown fences, no comments unless part of the code."
            .to_string()
    } else {
        "You are OxideSens, an expert terminal & DevOps assistant in OxideTerm. Be concise and helpful. When given terminal output or logs, analyze them and provide actionable insights. You can use markdown for formatting."
            .to_string()
    };

    // Build user message
    let mut user_message = String::new();
    if let Some(ctx) = &context {
        let escaped_ctx = escape_tagged_text(ctx);
        user_message.push_str("<context>\n");
        user_message.push_str(&escaped_ctx);
        user_message.push_str("\n</context>\n\n");
    }
    if let Some(tc) = &terminal_context {
        let escaped_tc = escape_tagged_text(tc);
        user_message.push_str("<terminal_buffer>\n");
        user_message.push_str(&escaped_tc);
        user_message.push_str("\n</terminal_buffer>\n\n");
    }
    user_message.push_str(&prompt);

    // Load conversation history or create new conversation
    let ai_store = app.try_state::<Arc<AiChatStore>>();
    let (conv_id, history_messages) = if let Some(ref cid) = conversation_id {
        // Continue existing conversation
        if let Some(ref store) = ai_store {
            let full = store.get_conversation(cid).map_err(|e| {
                (
                    protocol::ERR_INVALID_PARAMS,
                    format!("Conversation not found: {e}"),
                )
            })?;
            let history: Vec<Value> = full
                .messages
                .iter()
                .filter(|m| m.role == "user" || m.role == "assistant")
                .map(|m| json!({ "role": m.role, "content": m.content }))
                .collect();
            (cid.clone(), history)
        } else {
            (cid.clone(), vec![])
        }
    } else {
        // New conversation
        let new_id = uuid::Uuid::new_v4().to_string();
        if let Some(ref store) = ai_store {
            let now = chrono::Utc::now().timestamp_millis();
            // Generate a title from the first ~50 chars of prompt
            let title = if prompt.len() > 50 {
                let short = prompt.chars().take(50).collect::<String>();
                format!("{short}…")
            } else if prompt.is_empty() {
                "CLI Query".to_string()
            } else {
                prompt.clone()
            };
            let meta = ConversationMeta {
                id: new_id.clone(),
                title,
                created_at: now,
                updated_at: now,
                message_count: 0,
                session_id: session_id.clone(),
                origin: "cli".to_string(),
            };
            let _ = store.create_conversation(&meta);
        }
        (new_id, vec![])
    };

    // Apply sliding window to history if needed
    let trimmed_history = trim_history_to_budget(&history_messages, &model);

    // Build full messages array: history + new user message
    let mut messages = trimmed_history;
    messages.push(json!({ "role": "user", "content": user_message }));

    // Save user message to store
    if let Some(ref store) = ai_store {
        let now = chrono::Utc::now().timestamp_millis();
        let user_msg = PersistedMessage {
            id: uuid::Uuid::new_v4().to_string(),
            conversation_id: conv_id.clone(),
            role: "user".to_string(),
            content: user_message.clone(),
            timestamp: now,
            tool_calls: vec![],
            context_snapshot: None,
        };
        let _ = store.save_message(user_msg);
    }

    // Make AI API call
    let client = reqwest::Client::builder()
        .timeout(AI_REQUEST_TIMEOUT)
        .build()
        .map_err(|e| {
            (
                protocol::ERR_INTERNAL,
                format!("Failed to create HTTP client: {e}"),
            )
        })?;

    let result = match provider_type.as_str() {
        "anthropic" => {
            call_anthropic(
                &client,
                &base_url,
                &api_key,
                &model,
                &system_prompt,
                &messages,
                stream,
                writer,
            )
            .await
        }
        _ => {
            call_openai_compatible(
                &client,
                &base_url,
                &api_key,
                &model,
                &system_prompt,
                &messages,
                stream,
                writer,
            )
            .await
        }
    }?;

    // Save assistant response to store
    if let Some(ref store) = ai_store {
        let assistant_text = result
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if !assistant_text.is_empty() {
            let now = chrono::Utc::now().timestamp_millis();
            let assistant_msg = PersistedMessage {
                id: uuid::Uuid::new_v4().to_string(),
                conversation_id: conv_id.clone(),
                role: "assistant".to_string(),
                content: assistant_text,
                timestamp: now,
                tool_calls: vec![],
                context_snapshot: None,
            };
            let _ = store.save_message(assistant_msg);
        }
    }

    // Merge conversation_id into the response
    let mut response = result;
    response["conversation_id"] = json!(conv_id);
    Ok(response)
}

/// Resolve AI provider from CLI params or auto-detect from keychain.
///
/// Resolution order:
/// 1. If `--provider` given: match against synced frontend providers, then builtins
/// 2. Auto-detect: try active provider from frontend settings first, then all synced
///    providers, then builtin defaults
async fn resolve_ai_provider(
    app: &tauri::AppHandle,
    config_state: &Arc<ConfigState>,
    provider_override: Option<&str>,
    model_override: Option<&str>,
) -> Result<(String, String, String, String), (i32, String)> {
    // Read synced provider config from frontend
    let (synced_providers, active_provider_id) = {
        let lock = config_state.ai_providers.read();
        lock.clone()
    };

    if let Some(provider_type) = provider_override {
        // Try synced providers first (matches by type)
        if let Some(p) = synced_providers
            .iter()
            .find(|p| p.provider_type == provider_type && p.enabled)
        {
            if let Ok(key) = get_provider_api_key(app, config_state, &p.id).await {
                let model = model_override.unwrap_or(&p.default_model).to_string();
                return Ok((p.provider_type.clone(), p.base_url.clone(), key, model));
            }
        }
        // Fall back to builtin defaults
        let builtin = BUILTIN_PROVIDERS
            .iter()
            .find(|b| b.provider_type == provider_type)
            .ok_or((
                protocol::ERR_INVALID_PARAMS,
                format!("Unknown provider: {provider_type}. Available: openai, anthropic, gemini, ollama, openai_compatible"),
            ))?;

        let api_key = get_provider_api_key(app, config_state, builtin.keychain_id).await?;
        let model = model_override.unwrap_or(builtin.default_model).to_string();

        return Ok((
            provider_type.to_string(),
            builtin.base_url.to_string(),
            api_key,
            model,
        ));
    }

    // Auto-detect: try active provider from frontend settings first
    if let Some(ref active_id) = active_provider_id {
        if let Some(p) = synced_providers
            .iter()
            .find(|p| &p.id == active_id && p.enabled)
        {
            if let Ok(key) = get_provider_api_key(app, config_state, &p.id).await {
                let model = model_override.unwrap_or(&p.default_model).to_string();
                return Ok((p.provider_type.clone(), p.base_url.clone(), key, model));
            }
        }
    }

    // Try all synced providers
    for p in &synced_providers {
        if !p.enabled {
            continue;
        }
        if let Ok(key) = get_provider_api_key(app, config_state, &p.id).await {
            let model = model_override.unwrap_or(&p.default_model).to_string();
            return Ok((p.provider_type.clone(), p.base_url.clone(), key, model));
        }
    }

    // Fall back to builtin defaults
    for builtin in BUILTIN_PROVIDERS {
        if let Ok(key) = get_provider_api_key(app, config_state, builtin.keychain_id).await {
            let model = model_override.unwrap_or(builtin.default_model).to_string();
            return Ok((
                builtin.provider_type.to_string(),
                builtin.base_url.to_string(),
                key,
                model,
            ));
        }
    }

    Err((
        protocol::ERR_INTERNAL,
        "No AI provider API key found. Configure one in OxideTerm Settings → AI, or use --provider"
            .to_string(),
    ))
}

/// Get API key from keychain for a provider.
async fn get_provider_api_key(
    _app: &tauri::AppHandle,
    config_state: &Arc<ConfigState>,
    provider_id: &str,
) -> Result<String, (i32, String)> {
    // Check in-memory cache first
    {
        let cache = config_state.api_key_cache.read();
        if let Some(key) = cache.get(provider_id) {
            return Ok(key.clone());
        }
    }

    // Try keychain (skip Touch ID — CLI context cannot display biometric prompt)
    match config_state.ai_keychain.get_without_biometrics(provider_id) {
        Ok(key) => {
            config_state
                .api_key_cache
                .write()
                .insert(provider_id.to_string(), key.clone());
            Ok(key)
        }
        Err(_) => Err((
            protocol::ERR_INTERNAL,
            format!("No API key for provider: {provider_id}"),
        )),
    }
}

/// Get terminal buffer lines as a string.
async fn get_terminal_buffer(
    app: &tauri::AppHandle,
    session_id: &str,
) -> Result<String, (i32, String)> {
    let registry = app.try_state::<Arc<SessionRegistry>>().ok_or((
        protocol::ERR_INTERNAL,
        "Session registry not initialized".to_string(),
    ))?;

    let scroll_buffer = registry
        .with_session(session_id, |entry| entry.scroll_buffer.clone())
        .ok_or((
            protocol::ERR_INVALID_PARAMS,
            format!("No buffer for session: {session_id}"),
        ))?;

    // Get last N lines for context (reasonable size for AI prompt)
    let (lines, _total) = scroll_buffer.get_capped(MAX_TERMINAL_BUFFER_LINES).await;
    let text: Vec<String> = lines.iter().map(|l| l.text.clone()).collect();
    Ok(text.join("\n"))
}

/// Call OpenAI-compatible API (works for OpenAI, Ollama, openai_compatible).
async fn call_openai_compatible<W: AsyncWriteExt + Unpin>(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    model: &str,
    system_prompt: &str,
    messages: &[Value],
    stream: bool,
    writer: &mut W,
) -> Result<Value, (i32, String)> {
    let clean_url = base_url.trim_end_matches('/');
    let url = format!("{clean_url}/chat/completions");

    if api_key.is_empty() && !base_url.contains("localhost") {
        return Err((protocol::ERR_INTERNAL, "API key is empty".to_string()));
    }

    // Build messages: system + history + current
    let mut all_messages = vec![json!({ "role": "system", "content": system_prompt })];
    all_messages.extend_from_slice(messages);

    let body = json!({
        "model": model,
        "messages": all_messages,
        "stream": stream,
    });

    let mut request = client.post(&url).header("Content-Type", "application/json");

    if !api_key.is_empty() {
        request = request.header("Authorization", format!("Bearer {api_key}"));
    }

    let response = request
        .body(body.to_string())
        .send()
        .await
        .map_err(|e| (protocol::ERR_INTERNAL, format!("AI request failed: {e}")))?;

    let status = response.status().as_u16();
    if status >= 400 {
        let body_text = response.text().await.unwrap_or_default();
        return Err((
            protocol::ERR_INTERNAL,
            format!("AI API error ({status}): {body_text}"),
        ));
    }

    if !stream {
        let body_text = response
            .text()
            .await
            .map_err(|e| (protocol::ERR_INTERNAL, e.to_string()))?;
        let parsed: Value = serde_json::from_str(&body_text)
            .map_err(|e| (protocol::ERR_INTERNAL, format!("Invalid AI response: {e}")))?;
        let text = parsed
            .pointer("/choices/0/message/content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        return Ok(json!({ "text": text, "model": model }));
    }

    // Streaming: read SSE lines
    let mut full_text = String::new();
    let mut byte_stream = response.bytes_stream();
    let mut line_buf = String::new();

    use futures_util::StreamExt;
    while let Some(chunk) = byte_stream.next().await {
        let chunk = chunk.map_err(|e| (protocol::ERR_INTERNAL, format!("Stream error: {e}")))?;
        let text = String::from_utf8_lossy(&chunk);
        line_buf.push_str(&text);

        while let Some(newline_pos) = line_buf.find('\n') {
            let line = line_buf[..newline_pos].trim().to_string();
            line_buf = line_buf[newline_pos + 1..].to_string();

            if line.is_empty() || line == "data: [DONE]" {
                continue;
            }

            if let Some(data) = line.strip_prefix("data: ") {
                if let Ok(parsed) = serde_json::from_str::<Value>(data) {
                    if let Some(content) = parsed
                        .pointer("/choices/0/delta/content")
                        .and_then(|v| v.as_str())
                    {
                        if !content.is_empty() {
                            full_text.push_str(content);
                            let _ = super::handler::write_notification(
                                writer,
                                "stream_chunk",
                                json!({ "text": content }),
                            )
                            .await;
                        }
                    }
                }
            }
        }
    }

    Ok(json!({ "text": full_text, "model": model, "done": true }))
}

/// Call Anthropic Messages API.
async fn call_anthropic<W: AsyncWriteExt + Unpin>(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    model: &str,
    system_prompt: &str,
    messages: &[Value],
    stream: bool,
    writer: &mut W,
) -> Result<Value, (i32, String)> {
    let clean_url = base_url.trim_end_matches('/');
    let url = format!("{clean_url}/v1/messages");

    if api_key.is_empty() {
        return Err((
            protocol::ERR_INTERNAL,
            "Anthropic API key is empty".to_string(),
        ));
    }

    let body = json!({
        "model": model,
        "max_tokens": ANTHROPIC_MAX_TOKENS,
        "system": system_prompt,
        "messages": messages,
        "stream": stream,
    });

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .body(body.to_string())
        .send()
        .await
        .map_err(|e| (protocol::ERR_INTERNAL, format!("AI request failed: {e}")))?;

    let status = response.status().as_u16();
    if status >= 400 {
        let body_text = response.text().await.unwrap_or_default();
        return Err((
            protocol::ERR_INTERNAL,
            format!("AI API error ({status}): {body_text}"),
        ));
    }

    if !stream {
        let body_text = response
            .text()
            .await
            .map_err(|e| (protocol::ERR_INTERNAL, e.to_string()))?;
        let parsed: Value = serde_json::from_str(&body_text)
            .map_err(|e| (protocol::ERR_INTERNAL, format!("Invalid AI response: {e}")))?;
        let text = parsed
            .get("content")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|block| block.get("text"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        return Ok(json!({ "text": text, "model": model }));
    }

    // Streaming: read SSE lines
    let mut full_text = String::new();
    let mut byte_stream = response.bytes_stream();
    let mut line_buf = String::new();

    use futures_util::StreamExt;
    while let Some(chunk) = byte_stream.next().await {
        let chunk = chunk.map_err(|e| (protocol::ERR_INTERNAL, format!("Stream error: {e}")))?;
        let text = String::from_utf8_lossy(&chunk);
        line_buf.push_str(&text);

        while let Some(newline_pos) = line_buf.find('\n') {
            let line = line_buf[..newline_pos].trim().to_string();
            line_buf = line_buf[newline_pos + 1..].to_string();

            if line.is_empty() {
                continue;
            }

            if let Some(data) = line.strip_prefix("data: ") {
                if let Ok(parsed) = serde_json::from_str::<Value>(data) {
                    // Anthropic content_block_delta
                    if parsed.get("type").and_then(|v| v.as_str()) == Some("content_block_delta") {
                        if let Some(text_delta) =
                            parsed.pointer("/delta/text").and_then(|v| v.as_str())
                        {
                            if !text_delta.is_empty() {
                                full_text.push_str(text_delta);
                                let _ = super::handler::write_notification(
                                    writer,
                                    "stream_chunk",
                                    json!({ "text": text_delta }),
                                )
                                .await;
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(json!({ "text": full_text, "model": model, "done": true }))
}

/// Attach a CLI client to an existing session (SSH or local terminal).
///
/// Creates a new WsBridge for the CLI, allowing it to mirror the session
/// while the GUI continues running independently. Both share the same
/// underlying data channels (mpsc for input, broadcast for output).
async fn attach(app: &tauri::AppHandle, params: Value) -> Result<Value, (i32, String)> {
    let session_id = params.get("session_id").and_then(|v| v.as_str()).ok_or((
        protocol::ERR_INVALID_PARAMS,
        "session_id required".to_string(),
    ))?;

    // Try SSH session first
    if let Some(session_registry) = app.try_state::<Arc<SessionRegistry>>() {
        if let (Some(cmd_tx), Some(output_tx)) = (
            session_registry.get_cmd_tx(session_id),
            session_registry.get_output_tx(session_id),
        ) {
            let scroll_buffer = session_registry
                .with_session(session_id, |entry| entry.scroll_buffer.clone())
                .ok_or((
                    protocol::ERR_NOT_CONNECTED,
                    format!("Session {session_id} not found"),
                ))?;

            let (cols, rows) = session_registry
                .get_session_dims(session_id)
                .unwrap_or((80, 24));

            // Create an adapter channel that intercepts Close commands.
            // This prevents ExtendedSessionHandle::drop() from killing
            // the real SSH session when the CLI client disconnects.
            let (adapter_tx, mut adapter_rx) = mpsc::channel::<SessionCommand>(1024);
            let real_cmd_tx = cmd_tx.clone();
            let sid_clone = session_id.to_string();
            tokio::spawn(async move {
                while let Some(cmd) = adapter_rx.recv().await {
                    match cmd {
                        SessionCommand::Close => {
                            tracing::debug!(
                                "CLI attach adapter: close for SSH {} (ignored)",
                                sid_clone
                            );
                            break;
                        }
                        SessionCommand::Resize(_c, _r) => {
                            tracing::debug!(
                                "CLI attach adapter: resize ignored for mirror SSH {}",
                                sid_clone
                            );
                        }
                        other => {
                            if real_cmd_tx.send(other).await.is_err() {
                                break;
                            }
                        }
                    }
                }
                tracing::debug!("CLI attach adapter stopped for SSH session");
            });

            let handle = ExtendedSessionHandle {
                id: session_id.to_string(),
                cmd_tx: adapter_tx,
                stdout_rx: output_tx.subscribe(),
            };

            let (_sid, port, token, _disconnect_rx) =
                WsBridge::start_extended_with_disconnect(handle, scroll_buffer, true)
                    .await
                    .map_err(|e| (protocol::ERR_INTERNAL, format!("Bridge start failed: {e}")))?;

            tracing::info!("CLI attach: SSH session {} → ws port {}", session_id, port);

            return Ok(json!({
                "ws_url": format!("ws://localhost:{}", port),
                "ws_token": token,
                "session_id": session_id,
                "cols": cols,
                "rows": rows,
                "terminal_type": "ssh",
            }));
        }
    }

    // Try local terminal
    #[cfg(feature = "local-terminal")]
    {
        if let Some(state) = app.try_state::<Arc<crate::commands::local::LocalTerminalState>>() {
            let result = state.registry.attach_for_cli(session_id).await;
            if let Ok((handle, scroll_buffer, cols, rows)) = result {
                let (_sid, port, token, _disconnect_rx) =
                    WsBridge::start_extended_with_disconnect(handle, scroll_buffer, true)
                        .await
                        .map_err(|e| {
                            (protocol::ERR_INTERNAL, format!("Bridge start failed: {e}"))
                        })?;

                tracing::info!(
                    "CLI attach: local terminal {} → ws port {}",
                    session_id,
                    port
                );

                return Ok(json!({
                    "ws_url": format!("ws://localhost:{}", port),
                    "ws_token": token,
                    "session_id": session_id,
                    "cols": cols,
                    "rows": rows,
                    "terminal_type": "local",
                }));
            }
        }
    }

    Err((
        protocol::ERR_NOT_CONNECTED,
        format!("Session not found: {session_id}"),
    ))
}

// =============================================================================
// SFTP commands
// =============================================================================

/// List directory contents via SFTP.
async fn sftp_ls(app: &tauri::AppHandle, params: Value) -> Result<Value, (i32, String)> {
    let session_id = params.get("session_id").and_then(|v| v.as_str()).ok_or((
        protocol::ERR_INVALID_PARAMS,
        "Missing required parameter: session_id".to_string(),
    ))?;

    let path = params.get("path").and_then(|v| v.as_str()).unwrap_or(".");

    let router = app
        .try_state::<Arc<NodeRouter>>()
        .ok_or((protocol::ERR_INTERNAL, "Router not initialized".to_string()))?;

    // Use session_id as node_id (they are the same in the session tree)
    let sftp = router
        .acquire_sftp(session_id)
        .await
        .map_err(|e| (protocol::ERR_NOT_CONNECTED, format!("SFTP error: {e}")))?;

    let sftp = sftp.lock().await;

    // Resolve "." to cwd
    let resolved_path = if path == "." {
        sftp.cwd().to_string()
    } else {
        path.to_string()
    };

    let entries = sftp
        .list_dir(&resolved_path, None)
        .await
        .map_err(|e| (protocol::ERR_INTERNAL, format!("SFTP list_dir failed: {e}")))?;

    let items: Vec<Value> = entries
        .iter()
        .map(|f| {
            json!({
                "name": f.name,
                "path": f.path,
                "type": format!("{:?}", f.file_type),
                "size": f.size,
                "modified": f.modified,
                "permissions": f.permissions,
                "owner": f.owner,
                "is_symlink": f.is_symlink,
            })
        })
        .collect();

    Ok(json!({
        "path": resolved_path,
        "entries": items,
    }))
}

/// Download a file via SFTP.
async fn sftp_get(app: &tauri::AppHandle, params: Value) -> Result<Value, (i32, String)> {
    let session_id = params.get("session_id").and_then(|v| v.as_str()).ok_or((
        protocol::ERR_INVALID_PARAMS,
        "Missing required parameter: session_id".to_string(),
    ))?;

    let remote_path = params.get("remote_path").and_then(|v| v.as_str()).ok_or((
        protocol::ERR_INVALID_PARAMS,
        "Missing required parameter: remote_path".to_string(),
    ))?;

    let local_path = params.get("local_path").and_then(|v| v.as_str()).ok_or((
        protocol::ERR_INVALID_PARAMS,
        "Missing required parameter: local_path".to_string(),
    ))?;

    // Validate local path: parent directory must exist
    let local = std::path::Path::new(local_path);
    if let Some(parent) = local.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Err((
                protocol::ERR_INVALID_PARAMS,
                format!("Local directory does not exist: {}", parent.display()),
            ));
        }
    }

    let router = app
        .try_state::<Arc<NodeRouter>>()
        .ok_or((protocol::ERR_INTERNAL, "Router not initialized".to_string()))?;

    // Use transfer SFTP (independent channel, doesn't block browsing)
    let sftp = router
        .acquire_transfer_sftp(session_id)
        .await
        .map_err(|e| (protocol::ERR_NOT_CONNECTED, format!("SFTP error: {e}")))?;

    let progress_store = Arc::new(DummyProgressStore);
    let bytes = sftp
        .download_with_resume(remote_path, local_path, progress_store, None, None, None)
        .await
        .map_err(|e| (protocol::ERR_INTERNAL, format!("Download failed: {e}")))?;

    Ok(json!({
        "status": "ok",
        "remote_path": remote_path,
        "local_path": local_path,
        "bytes": bytes,
    }))
}

/// Upload a file via SFTP.
async fn sftp_put(app: &tauri::AppHandle, params: Value) -> Result<Value, (i32, String)> {
    let session_id = params.get("session_id").and_then(|v| v.as_str()).ok_or((
        protocol::ERR_INVALID_PARAMS,
        "Missing required parameter: session_id".to_string(),
    ))?;

    let local_path = params.get("local_path").and_then(|v| v.as_str()).ok_or((
        protocol::ERR_INVALID_PARAMS,
        "Missing required parameter: local_path".to_string(),
    ))?;

    let remote_path = params.get("remote_path").and_then(|v| v.as_str()).ok_or((
        protocol::ERR_INVALID_PARAMS,
        "Missing required parameter: remote_path".to_string(),
    ))?;

    // Validate local file exists
    if !std::path::Path::new(local_path).exists() {
        return Err((
            protocol::ERR_INVALID_PARAMS,
            format!("Local file does not exist: {local_path}"),
        ));
    }

    let router = app
        .try_state::<Arc<NodeRouter>>()
        .ok_or((protocol::ERR_INTERNAL, "Router not initialized".to_string()))?;

    let sftp = router
        .acquire_transfer_sftp(session_id)
        .await
        .map_err(|e| (protocol::ERR_NOT_CONNECTED, format!("SFTP error: {e}")))?;

    let progress_store = Arc::new(DummyProgressStore);
    let bytes = sftp
        .upload_with_resume(local_path, remote_path, progress_store, None, None, None)
        .await
        .map_err(|e| (protocol::ERR_INTERNAL, format!("Upload failed: {e}")))?;

    Ok(json!({
        "status": "ok",
        "local_path": local_path,
        "remote_path": remote_path,
        "bytes": bytes,
    }))
}

// =============================================================================
// SSH config import
// =============================================================================

/// List importable hosts from ~/.ssh/config.
async fn import_list(app: &tauri::AppHandle) -> Result<Value, (i32, String)> {
    let config_state = app
        .try_state::<Arc<ConfigState>>()
        .ok_or((protocol::ERR_INTERNAL, "Config not initialized".to_string()))?;

    let hosts = parse_ssh_config(None).await.map_err(|e| {
        (
            protocol::ERR_INTERNAL,
            format!("Failed to parse SSH config: {e}"),
        )
    })?;

    let existing_names: std::collections::HashSet<String> = {
        let config = config_state.get_config_snapshot();
        config.connections.iter().map(|c| c.name.clone()).collect()
    };

    let items: Vec<Value> = hosts
        .iter()
        .map(|h| {
            json!({
                "alias": h.alias,
                "hostname": h.effective_hostname(),
                "user": h.user,
                "port": h.effective_port(),
                "identity_file": h.identity_file,
                "already_imported": existing_names.contains(&h.alias),
            })
        })
        .collect();

    Ok(json!(items))
}

/// Import SSH config hosts as saved connections.
async fn import_hosts(app: &tauri::AppHandle, params: Value) -> Result<Value, (i32, String)> {
    let config_state = app
        .try_state::<Arc<ConfigState>>()
        .ok_or((protocol::ERR_INTERNAL, "Config not initialized".to_string()))?;

    let aliases: Vec<String> = params
        .get("aliases")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    if aliases.is_empty() {
        return Err((
            protocol::ERR_INVALID_PARAMS,
            "Missing required parameter: aliases (array of host names)".to_string(),
        ));
    }

    // Limit batch size to prevent config bloat
    const MAX_IMPORT_BATCH: usize = 200;
    if aliases.len() > MAX_IMPORT_BATCH {
        return Err((
            protocol::ERR_INVALID_PARAMS,
            format!("Too many aliases (max {MAX_IMPORT_BATCH})"),
        ));
    }

    let hosts = parse_ssh_config(None).await.map_err(|e| {
        (
            protocol::ERR_INTERNAL,
            format!("Failed to parse SSH config: {e}"),
        )
    })?;

    let existing_names: std::collections::HashSet<String> = {
        let config = config_state.get_config_snapshot();
        config.connections.iter().map(|c| c.name.clone()).collect()
    };

    let mut imported = 0usize;
    let mut skipped = 0usize;
    let mut errors = Vec::new();

    for alias in &aliases {
        let host = match hosts.iter().find(|h| &h.alias == alias) {
            Some(h) => h,
            None => {
                errors.push(format!("Host '{}' not found in SSH config", alias));
                continue;
            }
        };

        if existing_names.contains(alias) {
            skipped += 1;
            continue;
        }

        let auth = if let Some(ref key_path) = host.identity_file {
            if !std::path::Path::new(key_path).exists() {
                errors.push(format!("Key file not found for '{}': {}", alias, key_path));
                continue;
            }
            SavedAuth::Key {
                key_path: key_path.clone(),
                has_passphrase: false,
                passphrase_keychain_id: None,
            }
        } else {
            SavedAuth::Agent
        };

        let username = host.user.clone().unwrap_or_else(whoami::username);

        let conn = SavedConnection {
            id: uuid::Uuid::new_v4().to_string(),
            version: CONFIG_VERSION,
            name: alias.clone(),
            group: Some("Imported".to_string()),
            host: host.effective_hostname().to_string(),
            port: host.effective_port(),
            username,
            auth,
            options: Default::default(),
            created_at: chrono::Utc::now(),
            last_used_at: None,
            color: None,
            tags: vec!["ssh-config".to_string()],
            proxy_chain: Vec::new(),
        };

        config_state
            .update_config(|config| {
                config.add_connection(conn);
                if !config.groups.contains(&"Imported".to_string()) {
                    config.groups.push("Imported".to_string());
                }
            })
            .map_err(|e| {
                (
                    protocol::ERR_INTERNAL,
                    format!("Failed to update config: {e}"),
                )
            })?;

        imported += 1;
    }

    if imported > 0 {
        config_state.save_config().await.map_err(|e| {
            (
                protocol::ERR_INTERNAL,
                format!("Failed to save config: {e}"),
            )
        })?;
    }

    Ok(json!({
        "imported": imported,
        "skipped": skipped,
        "errors": errors,
    }))
}
