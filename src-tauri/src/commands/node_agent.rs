//! Node-first Agent commands
//!
//! Provides Tauri IPC commands for deploying and interacting with the
//! remote OxideTerm agent. Agent is optional — all operations fall back
//! to SFTP when the agent is unavailable.
//!
//! # Commands
//!
//! - `node_agent_deploy` — deploy and start the agent
//! - `node_agent_status` — check agent status
//! - `node_agent_read_file` — read file via agent (with hash)
//! - `node_agent_write_file` — atomic write via agent (with optimistic lock)
//! - `node_agent_list_tree` — recursive directory listing
//! - `node_agent_grep` — full-text search
//! - `node_agent_git_status` — git status
//! - `node_agent_watch_start` — start file watching
//! - `node_agent_watch_stop` — stop file watching

use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};
use tracing::{debug, info, warn};

use crate::agent::{
    AgentDeployer, AgentRegistry, AgentSession, AgentStatus, GitStatusResult,
    GrepMatch, ListTreeResult, ReadFileResult, SymbolIndexResult, SymbolInfo,
    WriteFileResult,
};
use crate::router::NodeRouter;

// ═══════════════════════════════════════════════════════════════════════════
// Deploy & Status
// ═══════════════════════════════════════════════════════════════════════════

/// Deploy the agent to a remote host (via node ID).
#[tauri::command]
pub async fn node_agent_deploy(
    node_id: String,
    router: State<'_, Arc<NodeRouter>>,
    agent_registry: State<'_, Arc<AgentRegistry>>,
    app_handle: AppHandle,
) -> Result<AgentStatus, String> {
    info!("[node_agent_deploy] Deploying agent for node {}", node_id);

    // Resolve connection
    let resolved = router
        .resolve_connection(&node_id)
        .await
        .map_err(|e| e.to_string())?;

    // Check if already deployed
    if agent_registry.has_agent(&resolved.connection_id) {
        let session = agent_registry.get(&resolved.connection_id).unwrap();
        return Ok(session.status());
    }

    // Need SFTP for binary upload
    let sftp_arc = router
        .acquire_sftp(&node_id)
        .await
        .map_err(|e| e.to_string())?;
    let sftp = sftp_arc.lock().await;

    // Deploy
    match AgentDeployer::deploy_and_start(
        &resolved.handle_controller,
        &sftp,
        &app_handle,
    )
    .await
    {
        Ok((transport, info)) => {
            let status = AgentStatus::Ready {
                version: info.version.clone(),
                arch: info.arch.clone(),
                pid: info.pid,
            };
            let session = AgentSession::new(transport, info);
            agent_registry.register(resolved.connection_id.clone(), session);
            Ok(status)
        }
        Err(e) => {
            warn!(
                "[node_agent_deploy] Failed to deploy agent for node {}: {}",
                node_id, e
            );
            Ok(AgentStatus::Failed {
                reason: e.to_string(),
            })
        }
    }
}

/// Get agent status for a node.
#[tauri::command]
pub async fn node_agent_status(
    node_id: String,
    router: State<'_, Arc<NodeRouter>>,
    agent_registry: State<'_, Arc<AgentRegistry>>,
) -> Result<AgentStatus, String> {
    let resolved = router
        .resolve_connection(&node_id)
        .await
        .map_err(|e| e.to_string())?;

    match agent_registry.get(&resolved.connection_id) {
        Some(session) => Ok(session.status()),
        None => Ok(AgentStatus::NotDeployed),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// File Operations (Agent-first with SFTP fallback)
// ═══════════════════════════════════════════════════════════════════════════

/// Read a file via agent (returns content + hash for optimistic locking).
#[tauri::command]
pub async fn node_agent_read_file(
    node_id: String,
    path: String,
    router: State<'_, Arc<NodeRouter>>,
    agent_registry: State<'_, Arc<AgentRegistry>>,
) -> Result<ReadFileResult, String> {
    let resolved = router
        .resolve_connection(&node_id)
        .await
        .map_err(|e| e.to_string())?;

    let session = agent_registry
        .get(&resolved.connection_id)
        .ok_or_else(|| "Agent not deployed".to_string())?;

    session
        .read_file(&path)
        .await
        .map_err(|e| e.to_string())
}

/// Write a file via agent (atomic write with optional optimistic lock).
#[tauri::command]
pub async fn node_agent_write_file(
    node_id: String,
    path: String,
    content: String,
    expect_hash: Option<String>,
    router: State<'_, Arc<NodeRouter>>,
    agent_registry: State<'_, Arc<AgentRegistry>>,
) -> Result<WriteFileResult, String> {
    let resolved = router
        .resolve_connection(&node_id)
        .await
        .map_err(|e| e.to_string())?;

    let session = agent_registry
        .get(&resolved.connection_id)
        .ok_or_else(|| "Agent not deployed".to_string())?;

    session
        .write_file(&path, &content, expect_hash.as_deref())
        .await
        .map_err(|e| e.to_string())
}

/// List directory tree (recursive) via agent — returns entries + truncation metadata.
#[tauri::command]
pub async fn node_agent_list_tree(
    node_id: String,
    path: String,
    max_depth: Option<u32>,
    max_entries: Option<u32>,
    router: State<'_, Arc<NodeRouter>>,
    agent_registry: State<'_, Arc<AgentRegistry>>,
) -> Result<ListTreeResult, String> {
    let resolved = router
        .resolve_connection(&node_id)
        .await
        .map_err(|e| e.to_string())?;

    let session = agent_registry
        .get(&resolved.connection_id)
        .ok_or_else(|| "Agent not deployed".to_string())?;

    session
        .list_tree(&path, max_depth, max_entries)
        .await
        .map_err(|e| e.to_string())
}

/// Search files for a pattern via agent.
#[tauri::command]
pub async fn node_agent_grep(
    node_id: String,
    pattern: String,
    path: String,
    case_sensitive: Option<bool>,
    max_results: Option<u32>,
    router: State<'_, Arc<NodeRouter>>,
    agent_registry: State<'_, Arc<AgentRegistry>>,
) -> Result<Vec<GrepMatch>, String> {
    let resolved = router
        .resolve_connection(&node_id)
        .await
        .map_err(|e| e.to_string())?;

    let session = agent_registry
        .get(&resolved.connection_id)
        .ok_or_else(|| "Agent not deployed".to_string())?;

    session
        .grep(&pattern, &path, case_sensitive.unwrap_or(false), max_results)
        .await
        .map_err(|e| e.to_string())
}

/// Get git status via agent.
#[tauri::command]
pub async fn node_agent_git_status(
    node_id: String,
    path: String,
    router: State<'_, Arc<NodeRouter>>,
    agent_registry: State<'_, Arc<AgentRegistry>>,
) -> Result<GitStatusResult, String> {
    let resolved = router
        .resolve_connection(&node_id)
        .await
        .map_err(|e| e.to_string())?;

    let session = agent_registry
        .get(&resolved.connection_id)
        .ok_or_else(|| "Agent not deployed".to_string())?;

    session
        .git_status(&path)
        .await
        .map_err(|e| e.to_string())
}

// ═══════════════════════════════════════════════════════════════════════════
// File Watching
// ═══════════════════════════════════════════════════════════════════════════

/// Start watching a directory for changes via agent.
#[tauri::command]
pub async fn node_agent_watch_start(
    node_id: String,
    path: String,
    ignore: Option<Vec<String>>,
    router: State<'_, Arc<NodeRouter>>,
    agent_registry: State<'_, Arc<AgentRegistry>>,
) -> Result<(), String> {
    let resolved = router
        .resolve_connection(&node_id)
        .await
        .map_err(|e| e.to_string())?;

    let session = agent_registry
        .get(&resolved.connection_id)
        .ok_or_else(|| "Agent not deployed".to_string())?;

    session
        .watch_start(&path, ignore.unwrap_or_default())
        .await
        .map_err(|e| e.to_string())
}

/// Stop watching a directory.
#[tauri::command]
pub async fn node_agent_watch_stop(
    node_id: String,
    path: String,
    router: State<'_, Arc<NodeRouter>>,
    agent_registry: State<'_, Arc<AgentRegistry>>,
) -> Result<(), String> {
    let resolved = router
        .resolve_connection(&node_id)
        .await
        .map_err(|e| e.to_string())?;

    let session = agent_registry
        .get(&resolved.connection_id)
        .ok_or_else(|| "Agent not deployed".to_string())?;

    session
        .watch_stop(&path)
        .await
        .map_err(|e| e.to_string())
}

/// Start relaying watch events from the agent to Tauri frontend events.
///
/// Spawns a background task that reads from the agent's watch channel
/// and emits `agent:watch-event:{nodeId}` events to the frontend.
/// The task automatically stops when the agent channel closes.
#[tauri::command]
pub async fn node_agent_start_watch_relay(
    node_id: String,
    router: State<'_, Arc<NodeRouter>>,
    agent_registry: State<'_, Arc<AgentRegistry>>,
    app_handle: AppHandle,
) -> Result<(), String> {
    let resolved = router
        .resolve_connection(&node_id)
        .await
        .map_err(|e| e.to_string())?;

    let session = agent_registry
        .get(&resolved.connection_id)
        .ok_or_else(|| "Agent not deployed".to_string())?;

    let mut watch_rx = session
        .take_watch_rx()
        .await
        .ok_or_else(|| "Watch relay already started".to_string())?;

    let node_id_clone = node_id.clone();
    let event_name = format!("agent:watch-event:{}", node_id);

    tokio::spawn(async move {
        info!("[agent-watch-relay] Started for node {}", node_id_clone);
        while let Some(event) = watch_rx.recv().await {
            debug!(
                "[agent-watch-relay] {} — {:?} {}",
                node_id_clone, event.kind, event.path
            );
            if let Err(e) = app_handle.emit(&event_name, &event) {
                warn!("[agent-watch-relay] Failed to emit: {}", e);
                break;
            }
        }
        info!("[agent-watch-relay] Ended for node {}", node_id_clone);
    });

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// Symbol Operations (code intelligence)
// ═══════════════════════════════════════════════════════════════════════════

/// Index symbols in a project directory via agent.
#[tauri::command]
pub async fn node_agent_symbol_index(
    node_id: String,
    path: String,
    max_files: Option<u32>,
    router: State<'_, Arc<NodeRouter>>,
    agent_registry: State<'_, Arc<AgentRegistry>>,
) -> Result<SymbolIndexResult, String> {
    let resolved = router
        .resolve_connection(&node_id)
        .await
        .map_err(|e| e.to_string())?;

    let session = agent_registry
        .get(&resolved.connection_id)
        .ok_or_else(|| "Agent not deployed".to_string())?;

    let result: SymbolIndexResult = session
        .symbol_index(&path, max_files)
        .await
        .map_err(|e| e.to_string())?;
    Ok(result)
}

/// Autocomplete a symbol prefix via agent.
#[tauri::command]
pub async fn node_agent_symbol_complete(
    node_id: String,
    path: String,
    prefix: String,
    limit: Option<u32>,
    router: State<'_, Arc<NodeRouter>>,
    agent_registry: State<'_, Arc<AgentRegistry>>,
) -> Result<Vec<SymbolInfo>, String> {
    let resolved = router
        .resolve_connection(&node_id)
        .await
        .map_err(|e| e.to_string())?;

    let session = agent_registry
        .get(&resolved.connection_id)
        .ok_or_else(|| "Agent not deployed".to_string())?;

    let result: Vec<SymbolInfo> = session
        .symbol_complete(&path, &prefix, limit)
        .await
        .map_err(|e| e.to_string())?;
    Ok(result)
}

/// Find symbol definitions by name via agent.
#[tauri::command]
pub async fn node_agent_symbol_definitions(
    node_id: String,
    path: String,
    name: String,
    router: State<'_, Arc<NodeRouter>>,
    agent_registry: State<'_, Arc<AgentRegistry>>,
) -> Result<Vec<SymbolInfo>, String> {
    let resolved = router
        .resolve_connection(&node_id)
        .await
        .map_err(|e| e.to_string())?;

    let session = agent_registry
        .get(&resolved.connection_id)
        .ok_or_else(|| "Agent not deployed".to_string())?;

    let result: Vec<SymbolInfo> = session
        .symbol_definitions(&path, &name)
        .await
        .map_err(|e| e.to_string())?;
    Ok(result)
}
