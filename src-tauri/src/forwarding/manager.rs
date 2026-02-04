//! Port Forwarding Manager
//!
//! Centralized management for all port forwards in a session.
//! Provides lifecycle management, status tracking, and cleanup.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

use super::dynamic::{
    start_dynamic_forward_with_disconnect, DynamicForward, DynamicForwardHandle,
    ForwardStats as DynamicForwardStats,
};
use super::events::ForwardEventEmitter;
use super::local::{
    start_local_forward_with_disconnect, ForwardStats as LocalForwardStats, LocalForward, LocalForwardHandle,
};
use super::remote::{
    start_remote_forward_with_disconnect, ForwardStats as RemoteForwardStats, RemoteForward, RemoteForwardHandle,
};
use crate::ssh::{HandleController, SshError};

/// Forward statistics (unified for all types)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ForwardStats {
    /// Total connection count
    pub connection_count: u64,
    /// Currently active connections
    pub active_connections: u64,
    /// Total bytes sent
    pub bytes_sent: u64,
    /// Total bytes received
    pub bytes_received: u64,
}

impl From<LocalForwardStats> for ForwardStats {
    fn from(s: LocalForwardStats) -> Self {
        Self {
            connection_count: s.connection_count,
            active_connections: s.active_connections,
            bytes_sent: s.bytes_sent,
            bytes_received: s.bytes_received,
        }
    }
}

impl From<RemoteForwardStats> for ForwardStats {
    fn from(s: RemoteForwardStats) -> Self {
        Self {
            connection_count: s.connection_count,
            active_connections: s.active_connections,
            bytes_sent: s.bytes_sent,
            bytes_received: s.bytes_received,
        }
    }
}

impl From<DynamicForwardStats> for ForwardStats {
    fn from(s: DynamicForwardStats) -> Self {
        Self {
            connection_count: s.connection_count,
            active_connections: s.active_connections,
            bytes_sent: s.bytes_sent,
            bytes_received: s.bytes_received,
        }
    }
}

/// Type of port forward
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ForwardType {
    /// Local port forwarding (-L)
    Local,
    /// Remote port forwarding (-R)  
    Remote,
    /// Dynamic SOCKS proxy (-D)
    Dynamic,
}

/// Status of a port forward
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ForwardStatus {
    /// Forward is starting
    Starting,
    /// Forward is active and running
    Active,
    /// Forward has stopped (user requested)
    Stopped,
    /// Forward encountered an error
    Error,
    /// Forward is suspended due to SSH disconnect (awaiting reconnect)
    Suspended,
}

/// Port forward rule configuration (for serialization)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardRule {
    /// Unique identifier
    pub id: String,
    /// Type of forward
    pub forward_type: ForwardType,
    /// Local address (for local forward) or remote bind address (for remote)
    pub bind_address: String,
    /// Bind port
    pub bind_port: u16,
    /// Target host
    pub target_host: String,
    /// Target port
    pub target_port: u16,
    /// Current status
    pub status: ForwardStatus,
    /// Description for UI
    pub description: Option<String>,
}

impl ForwardRule {
    /// Create a local forward rule
    pub fn local(
        bind_addr: impl Into<String>,
        bind_port: u16,
        target_host: impl Into<String>,
        target_port: u16,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            forward_type: ForwardType::Local,
            bind_address: bind_addr.into(),
            bind_port,
            target_host: target_host.into(),
            target_port,
            status: ForwardStatus::Starting,
            description: None,
        }
    }

    /// Create a remote forward rule
    pub fn remote(
        bind_addr: impl Into<String>,
        bind_port: u16,
        target_host: impl Into<String>,
        target_port: u16,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            forward_type: ForwardType::Remote,
            bind_address: bind_addr.into(),
            bind_port,
            target_host: target_host.into(),
            target_port,
            status: ForwardStatus::Starting,
            description: None,
        }
    }

    /// Create a dynamic (SOCKS5) forward rule
    pub fn dynamic(bind_addr: impl Into<String>, bind_port: u16) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            forward_type: ForwardType::Dynamic,
            bind_address: bind_addr.into(),
            bind_port,
            target_host: String::new(), // Not used for dynamic
            target_port: 0,             // Not used for dynamic
            status: ForwardStatus::Starting,
            description: Some("SOCKS5 Proxy".into()),
        }
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set custom ID
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }
}

/// Updates for an existing forward rule (for edit operation)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ForwardRuleUpdate {
    /// New bind address
    pub bind_address: Option<String>,
    /// New bind port
    pub bind_port: Option<u16>,
    /// New target host
    pub target_host: Option<String>,
    /// New target port
    pub target_port: Option<u16>,
    /// New description
    pub description: Option<String>,
}

/// Internal tracking for local forwards
struct LocalForwardEntry {
    rule: ForwardRule,
    handle: LocalForwardHandle,
}

/// Internal tracking for remote forwards
struct RemoteForwardEntry {
    rule: ForwardRule,
    handle: RemoteForwardHandle,
}

/// Internal tracking for dynamic (SOCKS5) forwards
struct DynamicForwardEntry {
    rule: ForwardRule,
    handle: DynamicForwardHandle,
}

/// Port forwarding manager
///
/// Manages all port forwards for a session. Thread-safe and designed
/// for concurrent access from multiple Tauri commands.
///
/// Uses `HandleController` (message-passing) to communicate with the
/// Handle Owner Task, avoiding direct Handle access and mutex locks.
pub struct ForwardingManager {
    /// Handle controller for SSH operations
    handle_controller: HandleController,
    /// Event emitter for frontend notifications (optional)
    event_emitter: Option<ForwardEventEmitter>,
    /// Active local forwards
    local_forwards: RwLock<HashMap<String, LocalForwardEntry>>,
    /// Active remote forwards
    remote_forwards: RwLock<HashMap<String, RemoteForwardEntry>>,
    /// Active dynamic (SOCKS5) forwards
    dynamic_forwards: RwLock<HashMap<String, DynamicForwardEntry>>,
    /// Stopped forwards (preserved for restart/edit)
    stopped_forwards: RwLock<HashMap<String, ForwardRule>>,
    /// Session ID for correlation
    session_id: String,
}

impl ForwardingManager {
    /// Create a new forwarding manager
    pub fn new(handle_controller: HandleController, session_id: impl Into<String>) -> Self {
        let session_id = session_id.into();
        Self {
            handle_controller,
            event_emitter: None,
            local_forwards: RwLock::new(HashMap::new()),
            remote_forwards: RwLock::new(HashMap::new()),
            dynamic_forwards: RwLock::new(HashMap::new()),
            stopped_forwards: RwLock::new(HashMap::new()),
            session_id,
        }
    }
    
    /// Create a new forwarding manager with event emitter
    pub fn with_event_emitter(
        handle_controller: HandleController,
        session_id: impl Into<String>,
        event_emitter: ForwardEventEmitter,
    ) -> Self {
        let session_id = session_id.into();
        Self {
            handle_controller,
            event_emitter: Some(event_emitter),
            local_forwards: RwLock::new(HashMap::new()),
            remote_forwards: RwLock::new(HashMap::new()),
            dynamic_forwards: RwLock::new(HashMap::new()),
            stopped_forwards: RwLock::new(HashMap::new()),
            session_id,
        }
    }
    
    /// Set event emitter after construction
    pub fn set_event_emitter(&mut self, event_emitter: ForwardEventEmitter) {
        self.event_emitter = Some(event_emitter);
    }
    
    /// Emit status changed event if emitter is configured
    fn emit_status_changed(&self, forward_id: &str, status: ForwardStatus, error: Option<String>) {
        if let Some(ref emitter) = self.event_emitter {
            emitter.emit_status_changed(forward_id, status, error);
        }
    }

    /// Get session ID
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Get a clone of the handle controller
    pub fn handle_controller(&self) -> HandleController {
        self.handle_controller.clone()
    }

    /// Create a local port forward
    pub async fn create_local_forward(
        &self,
        mut rule: ForwardRule,
    ) -> Result<ForwardRule, SshError> {
        if rule.forward_type != ForwardType::Local {
            return Err(SshError::ConnectionFailed("Invalid forward type".into()));
        }

        let config = LocalForward {
            local_addr: format!("{}:{}", rule.bind_address, rule.bind_port),
            remote_host: rule.target_host.clone(),
            remote_port: rule.target_port,
            description: rule.description.clone(),
        };

        info!(
            "Creating local forward {} -> {}:{}",
            config.local_addr, config.remote_host, config.remote_port
        );

        // Subscribe to disconnect and pass event emitter for death reporting
        let disconnect_rx = self.handle_controller.subscribe_disconnect();
        let handle = start_local_forward_with_disconnect(
            self.handle_controller.clone(),
            config,
            disconnect_rx,
            Some(rule.id.clone()),
            self.event_emitter.clone(),
        ).await?;

        // Update rule with actual bound address
        rule.bind_address = handle.bound_addr.ip().to_string();
        rule.bind_port = handle.bound_addr.port();
        rule.status = ForwardStatus::Active;

        let entry = LocalForwardEntry {
            rule: rule.clone(),
            handle,
        };

        self.local_forwards
            .write()
            .await
            .insert(rule.id.clone(), entry);

        // Emit event after releasing lock
        self.emit_status_changed(&rule.id, ForwardStatus::Active, None);

        info!("Local forward created: {}", rule.id);
        Ok(rule)
    }

    /// Create a remote port forward
    pub async fn create_remote_forward(
        &self,
        mut rule: ForwardRule,
    ) -> Result<ForwardRule, SshError> {
        if rule.forward_type != ForwardType::Remote {
            return Err(SshError::ConnectionFailed("Invalid forward type".into()));
        }

        let config = RemoteForward {
            remote_addr: rule.bind_address.clone(),
            remote_port: rule.bind_port,
            local_host: rule.target_host.clone(),
            local_port: rule.target_port,
            description: rule.description.clone(),
        };

        info!(
            "Creating remote forward {}:{} -> {}:{}",
            config.remote_addr, config.remote_port, config.local_host, config.local_port
        );

        // Subscribe to disconnect and pass event emitter for death reporting
        let disconnect_rx = self.handle_controller.subscribe_disconnect();
        let handle = start_remote_forward_with_disconnect(
            self.handle_controller.clone(),
            config,
            disconnect_rx,
            Some(rule.id.clone()),
            self.event_emitter.clone(),
        ).await?;
        rule.status = ForwardStatus::Active;

        let entry = RemoteForwardEntry {
            rule: rule.clone(),
            handle,
        };

        self.remote_forwards
            .write()
            .await
            .insert(rule.id.clone(), entry);

        // Emit event after releasing lock
        self.emit_status_changed(&rule.id, ForwardStatus::Active, None);

        info!("Remote forward created: {}", rule.id);
        Ok(rule)
    }

    /// Create a dynamic (SOCKS5) port forward
    pub async fn create_dynamic_forward(
        &self,
        mut rule: ForwardRule,
    ) -> Result<ForwardRule, SshError> {
        if rule.forward_type != ForwardType::Dynamic {
            return Err(SshError::ConnectionFailed("Invalid forward type".into()));
        }

        let config = DynamicForward {
            local_addr: format!("{}:{}", rule.bind_address, rule.bind_port),
            description: rule.description.clone(),
        };

        info!("Creating dynamic (SOCKS5) forward on {}", config.local_addr);

        // Subscribe to disconnect and pass event emitter for death reporting
        let disconnect_rx = self.handle_controller.subscribe_disconnect();
        let handle = start_dynamic_forward_with_disconnect(
            self.handle_controller.clone(),
            config,
            disconnect_rx,
            Some(rule.id.clone()),
            self.event_emitter.clone(),
        ).await?;

        // Update rule with actual bound address
        rule.bind_address = handle.bound_addr.ip().to_string();
        rule.bind_port = handle.bound_addr.port();
        rule.status = ForwardStatus::Active;

        let entry = DynamicForwardEntry {
            rule: rule.clone(),
            handle,
        };

        self.dynamic_forwards
            .write()
            .await
            .insert(rule.id.clone(), entry);

        // Emit event after releasing lock
        self.emit_status_changed(&rule.id, ForwardStatus::Active, None);

        info!("Dynamic forward created: {}", rule.id);
        Ok(rule)
    }

    /// Create a forward (dispatches to appropriate type)
    pub async fn create_forward(&self, rule: ForwardRule) -> Result<ForwardRule, SshError> {
        match rule.forward_type {
            ForwardType::Local => self.create_local_forward(rule).await,
            ForwardType::Remote => self.create_remote_forward(rule).await,
            ForwardType::Dynamic => self.create_dynamic_forward(rule).await,
        }
    }

    /// Stop a forward by ID (preserves the rule for restart)
    pub async fn stop_forward(&self, forward_id: &str) -> Result<(), SshError> {
        // Try local forwards first
        if let Some(entry) = self.local_forwards.write().await.remove(forward_id) {
            entry.handle.stop().await;
            // Save the rule for potential restart
            let mut rule = entry.rule.clone();
            rule.status = ForwardStatus::Stopped;
            self.stopped_forwards
                .write()
                .await
                .insert(forward_id.to_string(), rule);
            // Emit event after releasing lock
            self.emit_status_changed(forward_id, ForwardStatus::Stopped, None);
            info!("Stopped local forward: {}", forward_id);
            return Ok(());
        }

        // Try remote forwards
        if let Some(entry) = self.remote_forwards.write().await.remove(forward_id) {
            entry.handle.stop().await;
            // Save the rule for potential restart
            let mut rule = entry.rule.clone();
            rule.status = ForwardStatus::Stopped;
            self.stopped_forwards
                .write()
                .await
                .insert(forward_id.to_string(), rule);
            // Emit event after releasing lock
            self.emit_status_changed(forward_id, ForwardStatus::Stopped, None);
            info!("Stopped remote forward: {}", forward_id);
            return Ok(());
        }

        // Try dynamic forwards
        if let Some(entry) = self.dynamic_forwards.write().await.remove(forward_id) {
            entry.handle.stop().await;
            // Save the rule for potential restart
            let mut rule = entry.rule.clone();
            rule.status = ForwardStatus::Stopped;
            self.stopped_forwards
                .write()
                .await
                .insert(forward_id.to_string(), rule);
            // Emit event after releasing lock
            self.emit_status_changed(forward_id, ForwardStatus::Stopped, None);
            info!("Stopped dynamic forward: {}", forward_id);
            return Ok(());
        }

        Err(SshError::ConnectionFailed(format!(
            "Forward not found: {}",
            forward_id
        )))
    }

    /// Delete a forward by ID (permanently removes from both active and stopped)
    pub async fn delete_forward(&self, forward_id: &str) -> Result<(), SshError> {
        // First try to stop if it's still active
        let _ = self.stop_forward(forward_id).await;

        // Now remove from stopped_forwards
        if self
            .stopped_forwards
            .write()
            .await
            .remove(forward_id)
            .is_some()
        {
            info!("Deleted forward: {}", forward_id);
            return Ok(());
        }

        Err(SshError::ConnectionFailed(format!(
            "Forward not found: {}",
            forward_id
        )))
    }

    /// Restart a stopped forward
    pub async fn restart_forward(&self, forward_id: &str) -> Result<ForwardRule, SshError> {
        // Get the rule from stopped_forwards
        let rule = self
            .stopped_forwards
            .write()
            .await
            .remove(forward_id)
            .ok_or_else(|| {
                SshError::ConnectionFailed(format!("Stopped forward not found: {}", forward_id))
            })?;

        // Create a new forward with the same rule (keep the same ID)
        self.create_forward(rule).await
    }

    /// Update a stopped forward's configuration
    pub async fn update_forward(
        &self,
        forward_id: &str,
        updates: ForwardRuleUpdate,
    ) -> Result<ForwardRule, SshError> {
        let mut stopped = self.stopped_forwards.write().await;

        let rule = stopped.get_mut(forward_id).ok_or_else(|| {
            SshError::ConnectionFailed(format!(
                "Stopped forward not found: {}. Only stopped forwards can be edited.",
                forward_id
            ))
        })?;

        // Apply updates
        if let Some(bind_address) = updates.bind_address {
            rule.bind_address = bind_address;
        }
        if let Some(bind_port) = updates.bind_port {
            rule.bind_port = bind_port;
        }
        if let Some(target_host) = updates.target_host {
            rule.target_host = target_host;
        }
        if let Some(target_port) = updates.target_port {
            rule.target_port = target_port;
        }
        if let Some(description) = updates.description {
            rule.description = Some(description);
        }

        info!("Updated forward: {}", forward_id);
        Ok(rule.clone())
    }

    /// Get forward statistics
    pub async fn get_forward_stats(&self, forward_id: &str) -> Option<ForwardStats> {
        // Check local forwards
        if let Some(entry) = self.local_forwards.read().await.get(forward_id) {
            return Some(entry.handle.stats().into());
        }

        // Check remote forwards
        if let Some(entry) = self.remote_forwards.read().await.get(forward_id) {
            return Some(entry.handle.stats().into());
        }

        // Check dynamic forwards
        if let Some(entry) = self.dynamic_forwards.read().await.get(forward_id) {
            return Some(entry.handle.stats().into());
        }

        None
    }

    /// List all active forwards
    pub async fn list_forwards(&self) -> Vec<ForwardRule> {
        let mut forwards = Vec::new();

        // Add local forwards
        for entry in self.local_forwards.read().await.values() {
            let mut rule = entry.rule.clone();
            rule.status = if entry.handle.is_running() {
                ForwardStatus::Active
            } else {
                ForwardStatus::Stopped
            };
            forwards.push(rule);
        }

        // Add remote forwards
        for entry in self.remote_forwards.read().await.values() {
            let mut rule = entry.rule.clone();
            rule.status = if entry.handle.is_running() {
                ForwardStatus::Active
            } else {
                ForwardStatus::Stopped
            };
            forwards.push(rule);
        }

        // Add dynamic forwards
        for entry in self.dynamic_forwards.read().await.values() {
            let mut rule = entry.rule.clone();
            rule.status = if entry.handle.is_running() {
                ForwardStatus::Active
            } else {
                ForwardStatus::Stopped
            };
            forwards.push(rule);
        }

        // Add stopped forwards
        for rule in self.stopped_forwards.read().await.values() {
            forwards.push(rule.clone());
        }

        forwards
    }

    /// Get a specific forward by ID
    pub async fn get_forward(&self, forward_id: &str) -> Option<ForwardRule> {
        if let Some(entry) = self.local_forwards.read().await.get(forward_id) {
            return Some(entry.rule.clone());
        }
        if let Some(entry) = self.remote_forwards.read().await.get(forward_id) {
            return Some(entry.rule.clone());
        }
        if let Some(entry) = self.dynamic_forwards.read().await.get(forward_id) {
            return Some(entry.rule.clone());
        }
        if let Some(rule) = self.stopped_forwards.read().await.get(forward_id) {
            return Some(rule.clone());
        }
        None
    }

    /// Stop all forwards
    pub async fn stop_all(&self) {
        info!("Stopping all forwards for session {}", self.session_id);

        // Stop local forwards
        let local_ids: Vec<String> = self.local_forwards.read().await.keys().cloned().collect();
        for id in local_ids {
            if let Some(entry) = self.local_forwards.write().await.remove(&id) {
                entry.handle.stop().await;
            }
        }

        // Stop remote forwards
        let remote_ids: Vec<String> = self.remote_forwards.read().await.keys().cloned().collect();
        for id in remote_ids {
            if let Some(entry) = self.remote_forwards.write().await.remove(&id) {
                entry.handle.stop().await;
            }
        }

        // Stop dynamic forwards
        let dynamic_ids: Vec<String> = self.dynamic_forwards.read().await.keys().cloned().collect();
        for id in dynamic_ids {
            if let Some(entry) = self.dynamic_forwards.write().await.remove(&id) {
                entry.handle.stop().await;
            }
        }

        info!("All forwards stopped for session {}", self.session_id);
    }

    /// Stop all forwards and save their rules for recovery after reconnection
    /// Returns the list of rules that were saved
    pub async fn stop_all_and_save_rules(&self) -> Vec<ForwardRule> {
        info!(
            "Stopping all forwards and saving rules for session {}",
            self.session_id
        );
        let mut saved_rules = Vec::new();

        // Stop local forwards and save rules
        let local_ids: Vec<String> = self.local_forwards.read().await.keys().cloned().collect();
        for id in local_ids {
            if let Some(entry) = self.local_forwards.write().await.remove(&id) {
                entry.handle.stop().await;
                let mut rule = entry.rule.clone();
                rule.status = ForwardStatus::Stopped;
                saved_rules.push(rule.clone());
                self.stopped_forwards.write().await.insert(id, rule);
            }
        }

        // Stop remote forwards and save rules
        let remote_ids: Vec<String> = self.remote_forwards.read().await.keys().cloned().collect();
        for id in remote_ids {
            if let Some(entry) = self.remote_forwards.write().await.remove(&id) {
                entry.handle.stop().await;
                let mut rule = entry.rule.clone();
                rule.status = ForwardStatus::Stopped;
                saved_rules.push(rule.clone());
                self.stopped_forwards.write().await.insert(id, rule);
            }
        }

        // Stop dynamic forwards and save rules
        let dynamic_ids: Vec<String> = self.dynamic_forwards.read().await.keys().cloned().collect();
        for id in dynamic_ids {
            if let Some(entry) = self.dynamic_forwards.write().await.remove(&id) {
                entry.handle.stop().await;
                let mut rule = entry.rule.clone();
                rule.status = ForwardStatus::Stopped;
                saved_rules.push(rule.clone());
                self.stopped_forwards.write().await.insert(id, rule);
            }
        }

        info!(
            "Saved {} forward rules for session {}",
            saved_rules.len(),
            self.session_id
        );
        saved_rules
    }

    /// Get all stopped forward rules (for recovery after reconnect)
    pub async fn list_stopped_forwards(&self) -> Vec<ForwardRule> {
        self.stopped_forwards
            .read()
            .await
            .values()
            .cloned()
            .collect()
    }

    /// Count active forwards
    pub async fn count(&self) -> usize {
        self.local_forwards.read().await.len()
            + self.remote_forwards.read().await.len()
            + self.dynamic_forwards.read().await.len()
    }

    /// Check if a port is available on the remote host
    ///
    /// This attempts to open a TCP connection to the target host:port
    /// through SSH to verify the port is reachable before creating a forward.
    ///
    /// Returns:
    /// - `Ok(true)` if port is reachable
    /// - `Ok(false)` if connection refused (port not listening)
    /// - `Err` for other errors (network issues, SSH errors, etc.)
    pub async fn check_port_available(
        &self,
        host: &str,
        port: u16,
        timeout_ms: u64,
    ) -> Result<bool, SshError> {
        use tokio::time::{timeout, Duration};

        let timeout_duration = Duration::from_millis(timeout_ms);

        // Attempt to open a direct-tcpip channel to the target
        let result = timeout(
            timeout_duration,
            self.handle_controller
                .open_direct_tcpip(host, port as u32, "127.0.0.1", 0),
        )
        .await;

        match result {
            Ok(Ok(channel)) => {
                // Connection successful - port is available
                // Close the channel immediately
                let _ = channel.close().await;
                Ok(true)
            }
            Ok(Err(e)) => {
                // Connection failed - check if it's a connection refused
                let err_str = e.to_string().to_lowercase();
                if err_str.contains("connection refused")
                    || err_str.contains("connect failed")
                    || err_str.contains("connectfailed")
                    || err_str.contains("refused")
                    || err_str.contains("failed to open channel")
                {
                    Ok(false)
                } else {
                    Err(e)
                }
            }
            Err(_) => {
                // Timeout
                Err(SshError::ConnectionFailed(format!(
                    "Timeout checking port {}:{} ({}ms)",
                    host, port, timeout_ms
                )))
            }
        }
    }

    // === Quick shortcuts for common HPC use cases ===

    /// Create a Jupyter notebook forward (local 8888 -> remote 8888)
    pub async fn forward_jupyter(
        &self,
        local_port: u16,
        remote_port: u16,
    ) -> Result<ForwardRule, SshError> {
        let rule = ForwardRule::local("127.0.0.1", local_port, "localhost", remote_port)
            .with_description(format!("Jupyter Notebook ({})", remote_port));
        self.create_forward(rule).await
    }

    /// Create a TensorBoard forward (local 6006 -> remote 6006)
    pub async fn forward_tensorboard(
        &self,
        local_port: u16,
        remote_port: u16,
    ) -> Result<ForwardRule, SshError> {
        let rule = ForwardRule::local("127.0.0.1", local_port, "localhost", remote_port)
            .with_description(format!("TensorBoard ({})", remote_port));
        self.create_forward(rule).await
    }

    /// Create a VS Code Remote forward (for code-server)
    pub async fn forward_vscode(
        &self,
        local_port: u16,
        remote_port: u16,
    ) -> Result<ForwardRule, SshError> {
        let rule = ForwardRule::local("127.0.0.1", local_port, "localhost", remote_port)
            .with_description(format!("VS Code Server ({})", remote_port));
        self.create_forward(rule).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_forward_rule_local() {
        let rule = ForwardRule::local("127.0.0.1", 8888, "localhost", 8888);
        assert_eq!(rule.forward_type, ForwardType::Local);
        assert_eq!(rule.bind_port, 8888);
        assert_eq!(rule.target_port, 8888);
        assert_eq!(rule.status, ForwardStatus::Starting);
    }

    #[test]
    fn test_forward_rule_remote() {
        let rule =
            ForwardRule::remote("0.0.0.0", 9000, "localhost", 3000).with_description("API Server");
        assert_eq!(rule.forward_type, ForwardType::Remote);
        assert!(rule.description.unwrap().contains("API"));
    }

    #[test]
    fn test_forward_rule_custom_id() {
        let rule = ForwardRule::local("127.0.0.1", 8888, "localhost", 8888).with_id("my-jupyter");
        assert_eq!(rule.id, "my-jupyter");
    }
}
