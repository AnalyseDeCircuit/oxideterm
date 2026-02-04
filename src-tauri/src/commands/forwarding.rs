//! Port Forwarding Tauri Commands
//!
//! Provides Tauri commands for managing port forwarding from the frontend.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::forwarding::{
    ForwardRule, ForwardRuleUpdate, ForwardStats, ForwardStatus, ForwardType, ForwardingManager,
};
use crate::state::{forwarding::ForwardPersistence, PersistedForward, StateStore};

/// Global registry of forwarding managers (one per session)
pub struct ForwardingRegistry {
    managers: RwLock<HashMap<String, Arc<ForwardingManager>>>,
    persistence: Option<ForwardPersistence>,
}

impl ForwardingRegistry {
    /// Create a new forwarding registry
    pub fn new() -> Self {
        Self {
            managers: RwLock::new(HashMap::new()),
            persistence: None,
        }
    }

    /// Create a new forwarding registry with state persistence
    pub fn new_with_state(state_store: Arc<StateStore>) -> Self {
        Self {
            managers: RwLock::new(HashMap::new()),
            persistence: Some(ForwardPersistence::new(state_store)),
        }
    }

    /// Register a forwarding manager for a session
    pub async fn register(&self, session_id: String, manager: ForwardingManager) {
        self.managers
            .write()
            .await
            .insert(session_id, Arc::new(manager));
    }

    /// Get a forwarding manager by session ID
    pub async fn get(&self, session_id: &str) -> Option<Arc<ForwardingManager>> {
        self.managers.read().await.get(session_id).cloned()
    }

    /// Remove and stop all forwards for a session
    pub async fn remove(&self, session_id: &str) {
        if let Some(manager) = self.managers.write().await.remove(session_id) {
            manager.stop_all().await;
        }

        // Delete persisted forwards for this session
        if let Some(persistence) = &self.persistence {
            if let Err(e) = persistence.delete_by_session(session_id) {
                error!(
                    "Failed to delete persisted forwards for session {}: {:?}",
                    session_id, e
                );
            }
        }
    }

    /// Pause all forwards for a session (save rules for recovery) without removing from registry
    /// Used when connection goes down but we want to keep the manager for recovery
    pub async fn pause_forwards(&self, session_id: &str) -> Vec<ForwardRule> {
        if let Some(manager) = self.managers.read().await.get(session_id) {
            manager.stop_all_and_save_rules().await
        } else {
            Vec::new()
        }
    }

    /// Update the HandleController for a session's forwarding manager and restore forwards
    /// Used after connection reconnects
    pub async fn restore_forwards(
        &self,
        session_id: &str,
        new_handle_controller: crate::ssh::HandleController,
    ) -> Result<Vec<ForwardRule>, String> {
        // Get stopped rules from the old manager
        let stopped_rules: Vec<ForwardRule> = if let Some(old_manager) = self.managers.read().await.get(session_id) {
            old_manager.list_stopped_forwards().await
        } else {
            Vec::new()
        };

        let rules_count = stopped_rules.len();
        if rules_count == 0 {
            info!("No forwards to restore for session {}", session_id);
            return Ok(Vec::new());
        }

        info!(
            "Restoring {} forwards for session {} with new HandleController",
            rules_count,
            session_id
        );

        // Create a new manager with the new HandleController
        let new_manager = ForwardingManager::new(new_handle_controller, session_id);

        // Restore each forward rule
        let mut restored_rules = Vec::new();
        for rule in stopped_rules {
            match new_manager.create_forward(rule.clone()).await {
                Ok(restored_rule) => {
                    info!("Restored forward: {}", restored_rule.id);
                    restored_rules.push(restored_rule);
                }
                Err(e) => {
                    warn!("Failed to restore forward {}: {}", rule.id, e);
                }
            }
        }

        // Replace the old manager with the new one
        self.managers
            .write()
            .await
            .insert(session_id.to_string(), Arc::new(new_manager));

        info!(
            "Restored {}/{} forwards for session {}",
            restored_rules.len(),
            rules_count,
            session_id
        );

        Ok(restored_rules)
    }

    /// Stop all forwards across all sessions (for app shutdown)
    pub async fn stop_all_forwards(&self) {
        let managers: Vec<Arc<ForwardingManager>> = {
            self.managers.read().await.values().cloned().collect()
        };

        info!(
            "Stopping all port forwards across {} sessions on shutdown",
            managers.len()
        );

        for manager in managers {
            manager.stop_all().await;
        }

        // Clear all managers
        self.managers.write().await.clear();
    }

    /// Persist a forward rule
    pub async fn persist_forward(&self, forward: PersistedForward) -> Result<(), String> {
        if let Some(persistence) = &self.persistence {
            persistence
                .save_async(forward)
                .await
                .map_err(|e| format!("Failed to persist forward: {:?}", e))?;
            info!("Persisted forward rule");
        }
        Ok(())
    }

    /// Delete a persisted forward
    pub async fn delete_persisted_forward(&self, forward_id: String) -> Result<(), String> {
        if let Some(persistence) = &self.persistence {
            persistence
                .delete_async(forward_id)
                .await
                .map_err(|e| format!("Failed to delete persisted forward: {:?}", e))?;
            info!("Deleted persisted forward");
        }
        Ok(())
    }

    /// Load persisted forwards for a session (uses async internally where possible)
    pub async fn load_persisted_forwards(
        &self,
        session_id: &str,
    ) -> Result<Vec<PersistedForward>, String> {
        if let Some(persistence) = &self.persistence {
            // Use async load_all and filter (more efficient than loading all synchronously)
            let all_forwards = persistence
                .load_all_async()
                .await
                .map_err(|e| format!("Failed to load persisted forwards: {:?}", e))?;

            Ok(all_forwards
                .into_iter()
                .filter(|f| f.session_id == session_id)
                .collect())
        } else {
            Ok(Vec::new())
        }
    }

    /// Update auto-start flag for a forward
    pub async fn update_auto_start(
        &self,
        forward_id: &str,
        auto_start: bool,
    ) -> Result<(), String> {
        if let Some(persistence) = &self.persistence {
            persistence
                .update_auto_start(forward_id, auto_start)
                .map_err(|e| format!("Failed to update auto_start: {:?}", e))?;
            info!(
                "Updated auto_start for forward {}: {}",
                forward_id, auto_start
            );
        }
        Ok(())
    }
}

impl Default for ForwardingRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Request to create a port forward
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateForwardRequest {
    /// Session ID to create forward for
    pub session_id: String,
    /// Type of forward: "local" or "remote"
    pub forward_type: String,
    /// Bind address (local address for local forward, remote bind for remote)
    pub bind_address: String,
    /// Bind port
    pub bind_port: u16,
    /// Target host
    pub target_host: String,
    /// Target port
    pub target_port: u16,
    /// Optional description
    pub description: Option<String>,
    /// Check port availability before creating forward (default: true)
    #[serde(default = "default_check_health")]
    pub check_health: bool,
}

fn default_check_health() -> bool {
    true
}

/// Response for forward operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardResponse {
    /// Whether the operation succeeded
    pub success: bool,
    /// Forward rule (if successful)
    pub forward: Option<ForwardRuleDto>,
    /// Error message (if failed)
    pub error: Option<String>,
}

/// Forward rule DTO for frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardRuleDto {
    pub id: String,
    pub forward_type: String,
    pub bind_address: String,
    pub bind_port: u16,
    pub target_host: String,
    pub target_port: u16,
    pub status: String,
    pub description: Option<String>,
}

impl From<ForwardRule> for ForwardRuleDto {
    fn from(rule: ForwardRule) -> Self {
        Self {
            id: rule.id,
            forward_type: match rule.forward_type {
                ForwardType::Local => "local".to_string(),
                ForwardType::Remote => "remote".to_string(),
                ForwardType::Dynamic => "dynamic".to_string(),
            },
            bind_address: rule.bind_address,
            bind_port: rule.bind_port,
            target_host: rule.target_host,
            target_port: rule.target_port,
            status: match rule.status {
                ForwardStatus::Starting => "starting".to_string(),
                ForwardStatus::Active => "active".to_string(),
                ForwardStatus::Stopped => "stopped".to_string(),
                ForwardStatus::Error => "error".to_string(),
                ForwardStatus::Suspended => "suspended".to_string(),
            },
            description: rule.description,
        }
    }
}

/// Create a new port forward
#[tauri::command]
pub async fn create_port_forward(
    registry: State<'_, Arc<ForwardingRegistry>>,
    connection_registry: State<'_, Arc<crate::ssh::SshConnectionRegistry>>,
    request: CreateForwardRequest,
) -> Result<ForwardResponse, String> {
    info!(
        "Creating port forward for session {}: {:?}",
        request.session_id, request
    );

    let manager = registry
        .get(&request.session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", request.session_id))?;

    let forward_type = match request.forward_type.as_str() {
        "local" => ForwardType::Local,
        "remote" => ForwardType::Remote,
        "dynamic" => ForwardType::Dynamic,
        _ => return Err(format!("Invalid forward type: {}", request.forward_type)),
    };

    // Perform health check if enabled (skip for dynamic forwards)
    if request.check_health && forward_type != ForwardType::Dynamic {
        info!(
            "Checking port availability: {}:{}",
            request.target_host, request.target_port
        );

        match manager
            .check_port_available(&request.target_host, request.target_port, 3000)
            .await
        {
            Ok(true) => {
                info!(
                    "Port {}:{} is available",
                    request.target_host, request.target_port
                );
            }
            Ok(false) => {
                let error_msg = format!(
                    "Target port {}:{} is not reachable. Please ensure the service is running on the remote server.\n\nTroubleshooting:\n• Check if service is running: ss -tlnp | grep {}\n• Verify the port number is correct\n• Try connecting manually: nc -zv {} {}",
                    request.target_host, request.target_port, request.target_port, request.target_host, request.target_port
                );
                error!("Port health check failed: {}", error_msg);
                return Ok(ForwardResponse {
                    success: false,
                    forward: None,
                    error: Some(error_msg),
                });
            }
            Err(e) => {
                // Timeout or other network error
                let error_msg = format!(
                    "Failed to check port availability: {}\n\nThis might indicate:\n• Network connectivity issues\n• SSH connection problems\n• Port may be unreachable\n\nYou can skip this check with the 'Skip port availability check' option.",
                    e
                );
                error!("Health check error: {}", error_msg);
                return Ok(ForwardResponse {
                    success: false,
                    forward: None,
                    error: Some(error_msg),
                });
            }
        }
    }

    let rule = ForwardRule {
        id: uuid::Uuid::new_v4().to_string(),
        forward_type,
        bind_address: request.bind_address,
        bind_port: request.bind_port,
        target_host: request.target_host,
        target_port: request.target_port,
        status: ForwardStatus::Starting,
        description: request.description,
    };

    match manager.create_forward(rule).await {
        Ok(created_rule) => {
            let forward_id = created_rule.id.clone();
            info!("Port forward created: {}", forward_id);

            // 🔴 关键修复: 更新 ConnectionRegistry 的 forward 列表
            if let Err(e) = connection_registry
                .add_forward(&request.session_id, forward_id)
                .await
            {
                warn!("Failed to update forward state in ConnectionRegistry: {}", e);
            }

            Ok(ForwardResponse {
                success: true,
                forward: Some(created_rule.into()),
                error: None,
            })
        }
        Err(e) => {
            error!("Failed to create port forward: {}", e);
            Ok(ForwardResponse {
                success: false,
                forward: None,
                error: Some(e.to_string()),
            })
        }
    }
}

/// Stop a port forward
#[tauri::command]
pub async fn stop_port_forward(
    registry: State<'_, Arc<ForwardingRegistry>>,
    connection_registry: State<'_, Arc<crate::ssh::SshConnectionRegistry>>,
    session_id: String,
    forward_id: String,
) -> Result<ForwardResponse, String> {
    info!(
        "Stopping port forward {} for session {}",
        forward_id, session_id
    );

    let manager = registry
        .get(&session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", session_id))?;

    match manager.stop_forward(&forward_id).await {
        Ok(()) => {
            info!("Port forward stopped: {}", forward_id);

            // 从 ConnectionRegistry 移除 forward
            if let Err(e) = connection_registry
                .remove_forward(&session_id, &forward_id)
                .await
            {
                warn!("Failed to remove forward from ConnectionRegistry: {}", e);
            }

            Ok(ForwardResponse {
                success: true,
                forward: None,
                error: None,
            })
        }
        Err(e) => {
            warn!("Failed to stop port forward: {}", e);
            Ok(ForwardResponse {
                success: false,
                forward: None,
                error: Some(e.to_string()),
            })
        }
    }
}

/// Pause all port forwards for a session (used when connection goes down)
/// This saves the rules for later recovery
#[tauri::command]
pub async fn pause_port_forwards(
    registry: State<'_, Arc<ForwardingRegistry>>,
    session_id: String,
) -> Result<Vec<ForwardRuleDto>, String> {
    info!("Pausing all forwards for session {}", session_id);
    
    let paused_rules = registry.pause_forwards(&session_id).await;
    
    info!("Paused {} forwards for session {}", paused_rules.len(), session_id);
    
    Ok(paused_rules.into_iter().map(|r| r.into()).collect())
}

/// Restore port forwards after connection reconnects
/// This creates new forwards using the new HandleController
#[tauri::command]
pub async fn restore_port_forwards(
    registry: State<'_, Arc<ForwardingRegistry>>,
    connection_registry: State<'_, Arc<crate::ssh::SshConnectionRegistry>>,
    session_id: String,
) -> Result<Vec<ForwardRuleDto>, String> {
    info!("Restoring forwards for session {}", session_id);
    
    // Get the session's connection ID
    // For now, we assume session_id == connection_id in the new architecture
    // This might need adjustment based on actual architecture
    let handle_controller = connection_registry
        .get_handle_controller(&session_id)
        .or_else(|| {
            // Try to find connection by iterating (fallback)
            // This is a simplified approach - ideally we'd have a direct mapping
            connection_registry.get_handle_controller(&session_id)
        })
        .ok_or_else(|| format!("Cannot find HandleController for session {}", session_id))?;
    
    let restored_rules = registry
        .restore_forwards(&session_id, handle_controller)
        .await?;
    
    info!("Restored {} forwards for session {}", restored_rules.len(), session_id);
    
    Ok(restored_rules.into_iter().map(|r| r.into()).collect())
}

/// List all port forwards for a session
#[tauri::command]
pub async fn list_port_forwards(
    registry: State<'_, Arc<ForwardingRegistry>>,
    session_id: String,
) -> Result<Vec<ForwardRuleDto>, String> {
    let manager = registry
        .get(&session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", session_id))?;

    let forwards = manager.list_forwards().await;
    Ok(forwards.into_iter().map(|r| r.into()).collect())
}

/// Quick forward for Jupyter (convenience command)
#[tauri::command]
pub async fn forward_jupyter(
    registry: State<'_, Arc<ForwardingRegistry>>,
    session_id: String,
    local_port: u16,
    remote_port: u16,
) -> Result<ForwardResponse, String> {
    info!(
        "Creating Jupyter forward for session {}: {} -> {}",
        session_id, local_port, remote_port
    );

    let manager = registry
        .get(&session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", session_id))?;

    match manager.forward_jupyter(local_port, remote_port).await {
        Ok(rule) => Ok(ForwardResponse {
            success: true,
            forward: Some(rule.into()),
            error: None,
        }),
        Err(e) => Ok(ForwardResponse {
            success: false,
            forward: None,
            error: Some(e.to_string()),
        }),
    }
}

/// Quick forward for TensorBoard (convenience command)
#[tauri::command]
pub async fn forward_tensorboard(
    registry: State<'_, Arc<ForwardingRegistry>>,
    session_id: String,
    local_port: u16,
    remote_port: u16,
) -> Result<ForwardResponse, String> {
    info!(
        "Creating TensorBoard forward for session {}: {} -> {}",
        session_id, local_port, remote_port
    );

    let manager = registry
        .get(&session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", session_id))?;

    match manager.forward_tensorboard(local_port, remote_port).await {
        Ok(rule) => Ok(ForwardResponse {
            success: true,
            forward: Some(rule.into()),
            error: None,
        }),
        Err(e) => Ok(ForwardResponse {
            success: false,
            forward: None,
            error: Some(e.to_string()),
        }),
    }
}

/// Quick forward for VS Code (convenience command)
#[tauri::command]
pub async fn forward_vscode(
    registry: State<'_, Arc<ForwardingRegistry>>,
    session_id: String,
    local_port: u16,
    remote_port: u16,
) -> Result<ForwardResponse, String> {
    info!(
        "Creating VS Code forward for session {}: {} -> {}",
        session_id, local_port, remote_port
    );

    let manager = registry
        .get(&session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", session_id))?;

    match manager.forward_vscode(local_port, remote_port).await {
        Ok(rule) => Ok(ForwardResponse {
            success: true,
            forward: Some(rule.into()),
            error: None,
        }),
        Err(e) => Ok(ForwardResponse {
            success: false,
            forward: None,
            error: Some(e.to_string()),
        }),
    }
}

/// Stop all forwards for a session
#[tauri::command]
pub async fn stop_all_forwards(
    registry: State<'_, Arc<ForwardingRegistry>>,
    session_id: String,
) -> Result<(), String> {
    info!("Stopping all port forwards for session {}", session_id);

    if let Some(manager) = registry.get(&session_id).await {
        manager.stop_all().await;
    }

    Ok(())
}

/// Request to update a forward's configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateForwardRequest {
    /// Session ID
    pub session_id: String,
    /// Forward ID to update
    pub forward_id: String,
    /// New bind address (optional)
    pub bind_address: Option<String>,
    /// New bind port (optional)
    pub bind_port: Option<u16>,
    /// New target host (optional)
    pub target_host: Option<String>,
    /// New target port (optional)
    pub target_port: Option<u16>,
    /// New description (optional)
    pub description: Option<String>,
}

/// Forward stats DTO for frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardStatsDto {
    pub connection_count: u64,
    pub active_connections: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
}

impl From<ForwardStats> for ForwardStatsDto {
    fn from(stats: ForwardStats) -> Self {
        Self {
            connection_count: stats.connection_count,
            active_connections: stats.active_connections,
            bytes_sent: stats.bytes_sent,
            bytes_received: stats.bytes_received,
        }
    }
}

/// Delete a port forward (permanently remove)
#[tauri::command]
pub async fn delete_port_forward(
    registry: State<'_, Arc<ForwardingRegistry>>,
    connection_registry: State<'_, Arc<crate::ssh::SshConnectionRegistry>>,
    session_id: String,
    forward_id: String,
) -> Result<ForwardResponse, String> {
    info!(
        "Deleting port forward {} for session {}",
        forward_id, session_id
    );

    let manager = registry
        .get(&session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", session_id))?;

    match manager.delete_forward(&forward_id).await {
        Ok(()) => {
            info!("Port forward deleted: {}", forward_id);

            // 从 ConnectionRegistry 移除 forward
            if let Err(e) = connection_registry
                .remove_forward(&session_id, &forward_id)
                .await
            {
                warn!("Failed to remove forward from ConnectionRegistry: {}", e);
            }

            Ok(ForwardResponse {
                success: true,
                forward: None,
                error: None,
            })
        }
        Err(e) => {
            warn!("Failed to delete port forward: {}", e);
            Ok(ForwardResponse {
                success: false,
                forward: None,
                error: Some(e.to_string()),
            })
        }
    }
}

/// Restart a stopped port forward
#[tauri::command]
pub async fn restart_port_forward(
    registry: State<'_, Arc<ForwardingRegistry>>,
    session_id: String,
    forward_id: String,
) -> Result<ForwardResponse, String> {
    info!(
        "Restarting port forward {} for session {}",
        forward_id, session_id
    );

    let manager = registry
        .get(&session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", session_id))?;

    match manager.restart_forward(&forward_id).await {
        Ok(rule) => {
            info!("Port forward restarted: {}", rule.id);
            Ok(ForwardResponse {
                success: true,
                forward: Some(rule.into()),
                error: None,
            })
        }
        Err(e) => {
            warn!("Failed to restart port forward: {}", e);
            Ok(ForwardResponse {
                success: false,
                forward: None,
                error: Some(e.to_string()),
            })
        }
    }
}

/// Update a stopped port forward's configuration
#[tauri::command]
pub async fn update_port_forward(
    registry: State<'_, Arc<ForwardingRegistry>>,
    request: UpdateForwardRequest,
) -> Result<ForwardResponse, String> {
    info!(
        "Updating port forward {} for session {}",
        request.forward_id, request.session_id
    );

    let manager = registry
        .get(&request.session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", request.session_id))?;

    let updates = ForwardRuleUpdate {
        bind_address: request.bind_address,
        bind_port: request.bind_port,
        target_host: request.target_host,
        target_port: request.target_port,
        description: request.description,
    };

    match manager.update_forward(&request.forward_id, updates).await {
        Ok(rule) => {
            info!("Port forward updated: {}", rule.id);
            Ok(ForwardResponse {
                success: true,
                forward: Some(rule.into()),
                error: None,
            })
        }
        Err(e) => {
            warn!("Failed to update port forward: {}", e);
            Ok(ForwardResponse {
                success: false,
                forward: None,
                error: Some(e.to_string()),
            })
        }
    }
}

/// Get statistics for a port forward
#[tauri::command]
pub async fn get_port_forward_stats(
    registry: State<'_, Arc<ForwardingRegistry>>,
    session_id: String,
    forward_id: String,
) -> Result<Option<ForwardStatsDto>, String> {
    let manager = registry
        .get(&session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", session_id))?;

    Ok(manager
        .get_forward_stats(&forward_id)
        .await
        .map(|s| s.into()))
}

/// List saved forwards for a session
#[tauri::command]
pub async fn list_saved_forwards(
    registry: State<'_, Arc<ForwardingRegistry>>,
    session_id: String,
) -> Result<Vec<PersistedForwardDto>, String> {
    info!("Listing saved forwards for session {}", session_id);

    let forwards = registry.load_persisted_forwards(&session_id).await?;

    Ok(forwards
        .into_iter()
        .map(|f| PersistedForwardDto {
            id: f.id,
            session_id: f.session_id,
            forward_type: format!("{:?}", f.forward_type).to_lowercase(),
            bind_address: f.rule.bind_address,
            bind_port: f.rule.bind_port,
            target_host: f.rule.target_host,
            target_port: f.rule.target_port,
            auto_start: f.auto_start,
            created_at: f.created_at.to_rfc3339(),
        })
        .collect())
}

/// Set auto-start flag for a forward
#[tauri::command]
pub async fn set_forward_auto_start(
    registry: State<'_, Arc<ForwardingRegistry>>,
    forward_id: String,
    auto_start: bool,
) -> Result<(), String> {
    info!(
        "Setting auto_start={} for forward {}",
        auto_start, forward_id
    );
    registry.update_auto_start(&forward_id, auto_start).await
}

/// Delete a persisted forward rule
#[tauri::command]
pub async fn delete_saved_forward(
    registry: State<'_, Arc<ForwardingRegistry>>,
    forward_id: String,
) -> Result<(), String> {
    info!("Deleting saved forward {}", forward_id);
    registry.delete_persisted_forward(forward_id).await
}

/// DTO for persisted forward info
#[derive(Debug, Serialize)]
pub struct PersistedForwardDto {
    pub id: String,
    pub session_id: String,
    pub forward_type: String,
    pub bind_address: String,
    pub bind_port: u16,
    pub target_host: String,
    pub target_port: u16,
    pub auto_start: bool,
    pub created_at: String,
}
