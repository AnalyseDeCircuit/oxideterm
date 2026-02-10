//! Global Session Registry
//!
//! Thread-safe session management using DashMap for concurrent access.
//! Includes connection limiting and lifecycle management.

use dashmap::DashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::{debug, info, warn};

use super::state::SessionState;
use super::types::{SessionConfig, SessionEntry, SessionInfo, SessionStats};
use crate::ssh::{HandleController, SessionCommand};
use crate::state::{session::SessionPersistence, PersistedSession, StateStore};

/// Default maximum concurrent sessions
const DEFAULT_MAX_SESSIONS: usize = 20;

/// Global session registry for managing all SSH sessions
pub struct SessionRegistry {
    /// Map of session ID to session entry
    sessions: DashMap<String, SessionEntry>,
    /// Counter for generating unique tab orders
    order_counter: AtomicUsize,
    /// Maximum allowed concurrent sessions
    max_sessions: AtomicUsize,
    /// Active session count (connecting + connected) - O(1) instead of O(n)
    active_count: AtomicUsize,
    /// State persistence
    persistence: Option<SessionPersistence>,
    /// Lock for create_session to prevent TOCTOU race
    create_lock: parking_lot::Mutex<()>,
}

impl Default for SessionRegistry {
    fn default() -> Self {
        Self::new_without_persistence()
    }
}

impl SessionRegistry {
    /// Create a new session registry with state persistence
    pub fn new(state_store: Arc<StateStore>) -> Self {
        let persistence = SessionPersistence::new(state_store);
        Self {
            sessions: DashMap::new(),
            order_counter: AtomicUsize::new(0),
            max_sessions: AtomicUsize::new(DEFAULT_MAX_SESSIONS),
            active_count: AtomicUsize::new(0),
            persistence: Some(persistence),
            create_lock: parking_lot::Mutex::new(()),
        }
    }

    /// Create a new session registry without persistence (for testing)
    pub fn new_without_persistence() -> Self {
        Self {
            sessions: DashMap::new(),
            order_counter: AtomicUsize::new(0),
            max_sessions: AtomicUsize::new(DEFAULT_MAX_SESSIONS),
            active_count: AtomicUsize::new(0),
            persistence: None,
            create_lock: parking_lot::Mutex::new(()),
        }
    }

    /// Create a new session registry with custom max sessions
    pub fn with_max_sessions(max: usize) -> Self {
        Self {
            sessions: DashMap::new(),
            order_counter: AtomicUsize::new(0),
            max_sessions: AtomicUsize::new(max),
            active_count: AtomicUsize::new(0),
            persistence: None,
            create_lock: parking_lot::Mutex::new(()),
        }
    }

    /// Set maximum concurrent sessions
    pub fn set_max_sessions(&self, max: usize) {
        self.max_sessions.store(max, Ordering::SeqCst);
    }

    /// Get maximum concurrent sessions
    pub fn max_sessions(&self) -> usize {
        self.max_sessions.load(Ordering::SeqCst)
    }

    /// Create a new session (in Disconnected state)
    /// Returns session ID or error if limit reached
    pub fn create_session(&self, config: SessionConfig) -> Result<String, RegistryError> {
        // Hold lock to prevent TOCTOU race between count check and insert
        let _guard = self.create_lock.lock();

        // Check connection limit (now atomic with insert)
        let active_count = self.active_count();
        let max = self.max_sessions();

        if active_count >= max {
            return Err(RegistryError::ConnectionLimitReached {
                current: active_count,
                max,
            });
        }

        // Generate session ID
        let session_id = uuid::Uuid::new_v4().to_string();
        let order = self.order_counter.fetch_add(1, Ordering::SeqCst);

        info!(
            "Creating session {}: {}@{}:{} (order: {})",
            session_id, config.username, config.host, config.port, order
        );

        let entry = SessionEntry::new(session_id.clone(), config, order);
        self.sessions.insert(session_id.clone(), entry);

        Ok(session_id)
    }

    /// Create session with custom buffer configuration
    pub fn create_session_with_buffer(
        &self,
        config: SessionConfig,
        max_lines: usize,
    ) -> Result<String, RegistryError> {
        // Hold lock to prevent TOCTOU race between count check and insert
        let _guard = self.create_lock.lock();

        // Check connection limit (now atomic with insert)
        let active_count = self.active_count();
        let max = self.max_sessions();

        if active_count >= max {
            return Err(RegistryError::ConnectionLimitReached {
                current: active_count,
                max,
            });
        }

        // Generate session ID
        let session_id = uuid::Uuid::new_v4().to_string();
        let order = self.order_counter.fetch_add(1, Ordering::SeqCst);

        info!(
            "Creating session {}: {}@{}:{} (order: {}, buffer: {} lines)",
            session_id, config.username, config.host, config.port, order, max_lines
        );

        let entry = SessionEntry::with_buffer_config(session_id.clone(), config, order, max_lines);
        self.sessions.insert(session_id.clone(), entry);

        Ok(session_id)
    }

    /// Start connecting a session
    pub fn start_connecting(&self, session_id: &str) -> Result<(), RegistryError> {
        let mut entry = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| RegistryError::SessionNotFound(session_id.to_string()))?;

        entry
            .state_machine
            .start_connecting()
            .map_err(|e| RegistryError::StateTransition(e.to_string()))?;

        // Increment active count when entering active state
        self.increment_active();

        debug!("Session {} state -> Connecting", session_id);
        Ok(())
    }

    /// Mark session as connected
    pub fn connect_success(
        &self,
        session_id: &str,
        ws_port: u16,
        cmd_tx: mpsc::Sender<SessionCommand>,
        handle_controller: HandleController,
    ) -> Result<(), RegistryError> {
        let mut entry = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| RegistryError::SessionNotFound(session_id.to_string()))?;

        entry
            .state_machine
            .connect_success()
            .map_err(|e| RegistryError::StateTransition(e.to_string()))?;

        entry.ws_port = Some(ws_port);
        entry.cmd_tx = Some(cmd_tx);
        entry.handle_controller = Some(handle_controller);
        entry.ws_detached = false;
        if let Some(cancel) = entry.ws_detach_cancel.take() {
            let _ = cancel.send(());
        }

        info!("Session {} connected on port {}", session_id, ws_port);
        Ok(())
    }

    /// Mark session as connected with associated connection ID
    /// Used by the new connection pool architecture
    pub fn connect_success_with_connection(
        &self,
        session_id: &str,
        ws_port: u16,
        ws_token: String,
        cmd_tx: mpsc::Sender<SessionCommand>,
        handle_controller: HandleController,
        connection_id: String,
    ) -> Result<(), RegistryError> {
        let mut entry = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| RegistryError::SessionNotFound(session_id.to_string()))?;

        entry
            .state_machine
            .connect_success()
            .map_err(|e| RegistryError::StateTransition(e.to_string()))?;

        entry.ws_port = Some(ws_port);
        entry.ws_token = Some(ws_token);
        entry.cmd_tx = Some(cmd_tx);
        entry.handle_controller = Some(handle_controller);
        entry.ws_detached = false;

        if let Some(cancel) = entry.ws_detach_cancel.take() {
            let _ = cancel.send(());
        }
        entry.connection_id = Some(connection_id.clone());

        info!(
            "Session {} connected on port {} (connection: {})",
            session_id, ws_port, connection_id
        );
        Ok(())
    }

    /// Update WebSocket token for a session (used after reconnection)
    pub fn update_ws_token(&self, session_id: &str, ws_token: String) -> Result<(), RegistryError> {
        let mut entry = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| RegistryError::SessionNotFound(session_id.to_string()))?;

        entry.ws_token = Some(ws_token);
        debug!("Session {} ws_token updated", session_id);
        Ok(())
    }

    /// Update WebSocket info for a session after PTY recreation
    /// Used when connection reconnects and Shell PTY needs to be recreated
    pub fn update_ws_info(
        &self,
        session_id: &str,
        ws_port: u16,
        ws_token: String,
        cmd_tx: mpsc::Sender<SessionCommand>,
        handle_controller: HandleController,
    ) -> Result<(), RegistryError> {
        let mut entry = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| RegistryError::SessionNotFound(session_id.to_string()))?;

        entry.ws_port = Some(ws_port);
        entry.ws_token = Some(ws_token);
        entry.cmd_tx = Some(cmd_tx);
        entry.handle_controller = Some(handle_controller);

        info!(
            "Session {} ws_info updated after PTY recreation (port: {})",
            session_id, ws_port
        );
        Ok(())
    }

    /// Mark WS detached and schedule PTY cleanup after TTL (for seamless reconnect)
    ///
    /// If `on_timeout` callback is provided, it will be called when the TTL expires
    /// before the session is fully disconnected. This is useful for releasing
    /// connection pool references.
    pub fn mark_ws_detached_with_cleanup<F>(
        self: &Arc<Self>,
        session_id: &str,
        ttl: Duration,
        on_timeout: Option<F>,
    ) -> Result<(), RegistryError>
    where
        F: FnOnce(String) + Send + 'static,
    {
        let mut entry = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| RegistryError::SessionNotFound(session_id.to_string()))?;

        // Get connection_id before we release the entry lock
        let connection_id = entry.connection_id.clone();

        entry.ws_detached = true;

        if let Some(cancel) = entry.ws_detach_cancel.take() {
            let _ = cancel.send(());
        }

        let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
        entry.ws_detach_cancel = Some(cancel_tx);

        let registry = Arc::clone(self);
        let session_id = session_id.to_string();
        tokio::spawn(async move {
            tokio::select! {
                _ = tokio::time::sleep(ttl) => {
                    info!("WS detach TTL expired for session {}, cleaning up", session_id);

                    if let Some(entry) = registry.sessions.get(&session_id) {
                        let _ = entry.close().await;
                    }

                    // Call the cleanup callback (e.g., release connection ref_count)
                    if let Some(callback) = on_timeout {
                        if let Some(conn_id) = connection_id {
                            callback(conn_id);
                        }
                    }

                    let _ = registry.disconnect_complete(&session_id, true);
                }
                _ = cancel_rx => {
                    debug!("WS detach cleanup cancelled for session {}", session_id);
                }
            }
        });

        Ok(())
    }

    /// Mark WS detached and schedule PTY cleanup after TTL (for seamless reconnect)
    ///
    /// Simple version without cleanup callback. Use `mark_ws_detached_with_cleanup`
    /// if you need to release connection pool references on timeout.
    pub fn mark_ws_detached(
        self: &Arc<Self>,
        session_id: &str,
        ttl: Duration,
    ) -> Result<(), RegistryError> {
        self.mark_ws_detached_with_cleanup::<fn(String)>(session_id, ttl, None)
    }

    /// Mark session as failed
    pub fn connect_failed(&self, session_id: &str, error: String) -> Result<(), RegistryError> {
        let mut entry = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| RegistryError::SessionNotFound(session_id.to_string()))?;

        // Check if was active before transition
        let was_active = entry.state_machine.is_active();

        entry
            .state_machine
            .connect_failed(error.clone())
            .map_err(|e| RegistryError::StateTransition(e.to_string()))?;

        // Decrement active count when leaving active state
        if was_active {
            self.decrement_active();
        }

        warn!("Session {} connection failed: {}", session_id, error);
        Ok(())
    }

    /// Start disconnecting a session
    pub fn start_disconnecting(&self, session_id: &str) -> Result<(), RegistryError> {
        let mut entry = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| RegistryError::SessionNotFound(session_id.to_string()))?;

        entry
            .state_machine
            .start_disconnecting()
            .map_err(|e| RegistryError::StateTransition(e.to_string()))?;

        debug!("Session {} state -> Disconnecting", session_id);
        Ok(())
    }

    /// Complete disconnection and optionally remove session
    pub fn disconnect_complete(&self, session_id: &str, remove: bool) -> Result<(), RegistryError> {
        if remove {
            if let Some((_, mut entry)) = self.sessions.remove(session_id) {
                // Decrement active count if was active
                if entry.state_machine.is_active() {
                    self.decrement_active();
                }
                if let Some(cancel) = entry.ws_detach_cancel.take() {
                    let _ = cancel.send(());
                }
                let _ = entry.state_machine.disconnect_complete();
                info!("Session {} disconnected and removed", session_id);
            }
        } else {
            let mut entry = self
                .sessions
                .get_mut(session_id)
                .ok_or_else(|| RegistryError::SessionNotFound(session_id.to_string()))?;

            // Check if was active before transition
            let was_active = entry.state_machine.is_active();

            entry
                .state_machine
                .disconnect_complete()
                .map_err(|e| RegistryError::StateTransition(e.to_string()))?;

            // Decrement active count when leaving active state
            if was_active {
                self.decrement_active();
            }

            entry.ws_port = None;
            entry.cmd_tx = None;
            entry.handle_controller = None;
            entry.ws_detached = false;
            if let Some(cancel) = entry.ws_detach_cancel.take() {
                let _ = cancel.send(());
            }

            info!("Session {} disconnected", session_id);
        }
        Ok(())
    }

    /// Set session error state
    pub fn set_error(&self, session_id: &str, error: String) {
        if let Some(mut entry) = self.sessions.get_mut(session_id) {
            // Check if was active before transition
            let was_active = entry.state_machine.is_active();
            entry.state_machine.set_error(error);
            // Decrement active count when entering error state
            if was_active {
                self.decrement_active();
            }
        }
    }

    /// Remove a session
    pub fn remove(&self, session_id: &str) -> Option<SessionEntry> {
        self.sessions.remove(session_id).map(|(_, entry)| {
            // Decrement active count if the removed session was active
            if entry.state_machine.is_active() {
                self.decrement_active();
            }
            info!("Session {} removed from registry", session_id);
            entry
        })
    }

    /// Get session info by ID
    pub fn get(&self, session_id: &str) -> Option<SessionInfo> {
        self.sessions
            .get(session_id)
            .map(|entry| SessionInfo::from(entry.value()))
    }

    /// Execute a function with access to a session entry
    /// This provides safe read-only access to the SessionEntry
    pub fn with_session<F, R>(&self, session_id: &str, f: F) -> Option<R>
    where
        F: FnOnce(&SessionEntry) -> R,
    {
        self.sessions.get(session_id).map(|entry| f(entry.value()))
    }

    /// Get output broadcast sender for a session
    pub fn get_output_tx(&self, session_id: &str) -> Option<broadcast::Sender<Vec<u8>>> {
        self.sessions
            .get(session_id)
            .map(|entry| entry.output_tx.clone())
    }

    /// Check if session is currently WS-detached
    pub fn is_ws_detached(&self, session_id: &str) -> bool {
        self.sessions
            .get(session_id)
            .map(|entry| entry.ws_detached)
            .unwrap_or(false)
    }

    /// Get session config by ID (for reconnection)
    pub fn get_config(&self, session_id: &str) -> Option<SessionConfig> {
        self.sessions
            .get(session_id)
            .map(|entry| entry.config.clone())
    }

    /// Get command sender for a session
    pub fn get_cmd_tx(&self, session_id: &str) -> Option<mpsc::Sender<SessionCommand>> {
        self.sessions
            .get(session_id)
            .and_then(|entry| entry.cmd_tx.clone())
    }

    /// Get HandleController for a session (used to open additional channels)
    /// The returned controller can be cloned and used for SFTP, forwarding, etc.
    pub fn get_handle_controller(&self, session_id: &str) -> Option<HandleController> {
        self.sessions
            .get(session_id)
            .and_then(|entry| entry.handle_controller.clone())
    }

    /// List all sessions
    pub fn list(&self) -> Vec<SessionInfo> {
        let mut sessions: Vec<_> = self
            .sessions
            .iter()
            .map(|entry| SessionInfo::from(entry.value()))
            .collect();

        // Sort by order (tab order)
        sessions.sort_by_key(|s| s.order);
        sessions
    }

    /// List sessions by state
    pub fn list_by_state(&self, state: SessionState) -> Vec<SessionInfo> {
        self.sessions
            .iter()
            .filter(|entry| entry.state() == state)
            .map(|entry| SessionInfo::from(entry.value()))
            .collect()
    }

    /// Get session count
    pub fn count(&self) -> usize {
        self.sessions.len()
    }

    /// Get count of active sessions (connecting or connected) - O(1)
    pub fn active_count(&self) -> usize {
        self.active_count.load(Ordering::SeqCst)
    }

    /// Increment active session count
    fn increment_active(&self) {
        self.active_count.fetch_add(1, Ordering::SeqCst);
    }

    /// Decrement active session count
    fn decrement_active(&self) {
        // Use saturating_sub behavior to prevent underflow
        let prev = self
            .active_count
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |x| {
                Some(x.saturating_sub(1))
            });
        if let Ok(0) = prev {
            warn!("Active count was already 0, possible accounting error");
        }
    }

    /// Get session statistics
    pub fn stats(&self) -> SessionStats {
        let mut connected = 0;
        let mut connecting = 0;
        let mut error = 0;

        for entry in self.sessions.iter() {
            match entry.state() {
                SessionState::Connected => connected += 1,
                SessionState::Connecting => connecting += 1,
                SessionState::Error => error += 1,
                _ => {}
            }
        }

        SessionStats {
            total: self.sessions.len(),
            connected,
            connecting,
            error,
            max_sessions: self.max_sessions(),
        }
    }

    /// Resize a session's PTY
    pub async fn resize(&self, session_id: &str, cols: u16, rows: u16) -> Result<(), String> {
        let entry = self
            .sessions
            .get(session_id)
            .ok_or_else(|| format!("Session {} not found", session_id))?;

        entry.resize(cols, rows).await
    }

    /// Close a session
    pub async fn close_session(&self, session_id: &str) -> Result<(), String> {
        // First send close command
        if let Some(entry) = self.sessions.get(session_id) {
            entry.close().await?;
        }

        // Then update state
        let _ = self.start_disconnecting(session_id);
        Ok(())
    }

    /// Update tab order for a session
    pub fn update_order(&self, session_id: &str, new_order: usize) -> Result<(), RegistryError> {
        let mut entry = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| RegistryError::SessionNotFound(session_id.to_string()))?;

        entry.order = new_order;
        Ok(())
    }

    /// Reorder sessions (for drag and drop)
    pub fn reorder(&self, ordered_ids: &[String]) -> Result<(), RegistryError> {
        for (index, id) in ordered_ids.iter().enumerate() {
            self.update_order(id, index)?;
        }
        Ok(())
    }

    /// Clean up disconnected/error sessions older than threshold
    pub fn cleanup_stale(&self, max_age_secs: u64) {
        let stale_ids: Vec<String> = self
            .sessions
            .iter()
            .filter(|entry| {
                entry.state_machine.is_terminal()
                    && entry.state_machine.time_in_state().as_secs() > max_age_secs
            })
            .map(|entry| entry.id.clone())
            .collect();

        if !stale_ids.is_empty() {
            info!("Cleaning up {} stale sessions", stale_ids.len());
        }

        for id in stale_ids {
            info!("Removing stale session: {}", id);
            self.remove(&id);
        }
    }

    /// Disconnect all sessions (for app shutdown)
    /// This drops all SSH handles and clears the registry
    pub async fn disconnect_all(&self) {
        let session_ids: Vec<String> = self.sessions.iter().map(|entry| entry.id.clone()).collect();

        info!("Disconnecting {} sessions on shutdown", session_ids.len());

        for session_id in session_ids {
            // Drop the handle controller to close SSH connection
            if let Some(mut entry) = self.sessions.get_mut(&session_id) {
                entry.handle_controller = None;
                entry.cmd_tx = None;
            }
            // Remove the session
            self.remove(&session_id);
        }
    }

    /// Persist a session's metadata
    pub fn persist_session(&self, session_id: &str) -> Result<(), RegistryError> {
        if let Some(persistence) = &self.persistence {
            let entry = self
                .sessions
                .get(session_id)
                .ok_or_else(|| RegistryError::SessionNotFound(session_id.to_string()))?;

            let persisted =
                PersistedSession::new(entry.id.clone(), entry.config.clone(), entry.order);

            persistence
                .save(&persisted)
                .map_err(|e| RegistryError::PersistenceError(e.to_string()))?;

            debug!("Persisted session metadata: {}", session_id);
        }
        Ok(())
    }

    /// Persist a session with terminal buffer (async)
    pub async fn persist_session_with_buffer(&self, session_id: &str) -> Result<(), RegistryError> {
        if let Some(persistence) = &self.persistence {
            let (id, config, order, buffer_data, buffer_config) = {
                let entry = self
                    .sessions
                    .get(session_id)
                    .ok_or_else(|| RegistryError::SessionNotFound(session_id.to_string()))?;

                // Get buffer config from entry (or use default)
                let buffer_config = crate::state::BufferConfig::default();

                // Only save buffer if enabled in config
                let buffer_data = if buffer_config.save_on_disconnect {
                    // Serialize buffer
                    match entry.scroll_buffer.save_to_bytes().await {
                        Ok(data) => Some(data),
                        Err(e) => {
                            tracing::warn!("Failed to serialize buffer for {}: {}", session_id, e);
                            None
                        }
                    }
                } else {
                    None
                };

                (
                    entry.id.clone(),
                    entry.config.clone(),
                    entry.order,
                    buffer_data,
                    buffer_config,
                )
            };

            let persisted = if let Some(buffer_data) = buffer_data {
                crate::state::PersistedSession::with_buffer(
                    id,
                    config,
                    order,
                    buffer_data,
                    buffer_config,
                )
            } else {
                crate::state::PersistedSession::new(id, config, order)
            };

            // Save asynchronously
            persistence
                .save_async(persisted)
                .await
                .map_err(|e| RegistryError::PersistenceError(e.to_string()))?;

            debug!("Persisted session with buffer: {}", session_id);
        }
        Ok(())
    }

    /// Delete persisted session metadata (synchronous)
    pub fn delete_persisted_session(&self, session_id: &str) -> Result<(), RegistryError> {
        if let Some(persistence) = &self.persistence {
            persistence
                .delete(session_id)
                .map_err(|e| RegistryError::PersistenceError(e.to_string()))?;
            debug!("Deleted persisted session: {}", session_id);
        }
        Ok(())
    }

    /// Delete persisted session metadata (async, non-blocking)
    pub async fn delete_persisted_session_async(
        &self,
        session_id: String,
    ) -> Result<(), RegistryError> {
        if let Some(persistence) = &self.persistence {
            persistence
                .delete_async(session_id)
                .await
                .map_err(|e| RegistryError::PersistenceError(e.to_string()))?;
            debug!("Deleted persisted session");
        }
        Ok(())
    }

    /// Restore sessions from persistence (returns session IDs for frontend to restore) - synchronous
    pub fn restore_sessions(&self) -> Result<Vec<PersistedSession>, RegistryError> {
        if let Some(persistence) = &self.persistence {
            let sessions = persistence
                .load_all()
                .map_err(|e| RegistryError::PersistenceError(e.to_string()))?;
            info!("Restored {} persisted sessions", sessions.len());
            Ok(sessions)
        } else {
            Ok(Vec::new())
        }
    }

    /// Restore sessions from persistence (async, non-blocking)
    pub async fn restore_sessions_async(&self) -> Result<Vec<PersistedSession>, RegistryError> {
        if let Some(persistence) = &self.persistence {
            let sessions = persistence
                .load_all_async()
                .await
                .map_err(|e| RegistryError::PersistenceError(e.to_string()))?;
            info!("Restored {} persisted sessions", sessions.len());
            Ok(sessions)
        } else {
            Ok(Vec::new())
        }
    }

    /// List persisted session IDs
    pub fn list_persisted_sessions(&self) -> Result<Vec<String>, RegistryError> {
        if let Some(persistence) = &self.persistence {
            persistence
                .list_ids()
                .map_err(|e| RegistryError::PersistenceError(e.to_string()))
        } else {
            Ok(Vec::new())
        }
    }
}

/// Registry error types
#[derive(Debug, Clone, thiserror::Error)]
pub enum RegistryError {
    #[error("Connection limit reached: {current}/{max} sessions active")]
    ConnectionLimitReached { current: usize, max: usize },

    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("State transition error: {0}")]
    StateTransition(String),

    #[error("Persistence error: {0}")]
    PersistenceError(String),
}

/// Create a shared session registry without persistence (for testing)
#[allow(dead_code)]
pub fn create_shared_registry() -> Arc<SessionRegistry> {
    Arc::new(SessionRegistry::new_without_persistence())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_session() {
        let registry = SessionRegistry::new_without_persistence();
        let config = SessionConfig::with_password("example.com", 22, "user", "pass");

        let id = registry.create_session(config).unwrap();
        assert!(!id.is_empty());
        assert_eq!(registry.count(), 1);
    }

    #[test]
    fn test_connection_limit() {
        let registry = SessionRegistry::with_max_sessions(2);

        let config1 = SessionConfig::with_password("server1.com", 22, "user", "pass");
        let config2 = SessionConfig::with_password("server2.com", 22, "user", "pass");
        let config3 = SessionConfig::with_password("server3.com", 22, "user", "pass");

        let id1 = registry.create_session(config1).unwrap();
        registry.start_connecting(&id1).unwrap();

        let id2 = registry.create_session(config2).unwrap();
        registry.start_connecting(&id2).unwrap();

        // Third should fail
        let result = registry.create_session(config3);
        assert!(matches!(
            result,
            Err(RegistryError::ConnectionLimitReached { .. })
        ));
    }

    #[test]
    fn test_state_transitions() {
        let registry = SessionRegistry::new_without_persistence();
        let config = SessionConfig::with_password("example.com", 22, "user", "pass");

        let id = registry.create_session(config).unwrap();

        // Initial state should be Disconnected
        let info = registry.get(&id).unwrap();
        assert_eq!(info.state, SessionState::Disconnected);

        // Transition to Connecting
        registry.start_connecting(&id).unwrap();
        let info = registry.get(&id).unwrap();
        assert_eq!(info.state, SessionState::Connecting);
    }

    #[test]
    fn test_active_count_tracking() {
        let registry = SessionRegistry::with_max_sessions(10);

        // Initial count should be 0
        assert_eq!(registry.active_count(), 0);

        let config1 = SessionConfig::with_password("server1.com", 22, "user", "pass");
        let config2 = SessionConfig::with_password("server2.com", 22, "user", "pass");

        // Create sessions (disconnected state - not active)
        let id1 = registry.create_session(config1).unwrap();
        let id2 = registry.create_session(config2).unwrap();
        assert_eq!(registry.active_count(), 0);

        // Start connecting (active state)
        registry.start_connecting(&id1).unwrap();
        assert_eq!(registry.active_count(), 1);

        registry.start_connecting(&id2).unwrap();
        assert_eq!(registry.active_count(), 2);

        // Remove one session
        registry.remove(&id1);
        assert_eq!(registry.active_count(), 1);

        // Connection fails
        registry
            .connect_failed(&id2, "test error".to_string())
            .unwrap();
        assert_eq!(registry.active_count(), 0);
    }
}
