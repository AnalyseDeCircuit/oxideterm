//! Auto Reconnect Service
//!
//! Provides automatic reconnection for SSH sessions when connections drop.
//! Note: The primary reconnection mechanism is now in SshConnectionRegistry via heartbeat.
//! This service provides supplementary network status management.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;
use tauri::AppHandle;
use tracing::{error, info, warn};

use super::reconnect::{ReconnectConfig, ReconnectError};
use super::registry::SessionRegistry;
use super::types::SessionConfig;
use crate::commands::forwarding::ForwardingRegistry;
use crate::forwarding::ForwardingManager;
use crate::ssh::{AuthMethod as SshAuthMethod, SshClient, SshConfig};

/// Auto reconnect service that manages reconnection for all sessions
pub struct AutoReconnectService {
    /// Session registry
    registry: Arc<SessionRegistry>,
    /// Forwarding registry for restoring port forwards after reconnection
    forwarding_registry: Arc<ForwardingRegistry>,
    /// Tauri app handle (kept for potential future use)
    #[allow(dead_code)]
    app_handle: AppHandle,
    /// Active reconnection tasks (session_id -> cancel flag)
    active_reconnects: RwLock<HashMap<String, Arc<AtomicBool>>>,
    /// Global network online status
    network_online: AtomicBool,
    /// Global pause flag (pause all reconnects when offline)
    paused: AtomicBool,
}

impl AutoReconnectService {
    /// Create a new auto reconnect service
    pub fn new(
        registry: Arc<SessionRegistry>,
        forwarding_registry: Arc<ForwardingRegistry>,
        app_handle: AppHandle,
    ) -> Self {
        Self {
            registry,
            forwarding_registry,
            app_handle,
            active_reconnects: RwLock::new(HashMap::new()),
            network_online: AtomicBool::new(true),
            paused: AtomicBool::new(false),
        }
    }

    /// Check if a session is currently reconnecting
    pub fn is_reconnecting(&self, session_id: &str) -> bool {
        self.active_reconnects.read().contains_key(session_id)
    }

    /// Trigger reconnection for a disconnected session
    /// Note: Frontend events are now handled by connection_status_changed in SshConnectionRegistry
    pub async fn trigger_reconnect(
        self: &Arc<Self>,
        session_id: String,
        reason: String,
        recoverable: bool,
    ) {
        // Check if already reconnecting
        if self.is_reconnecting(&session_id) {
            warn!(
                "Session {} is already reconnecting, ignoring duplicate trigger",
                session_id
            );
            return;
        }

        info!(
            "Session {} disconnect triggered: reason={}, recoverable={}",
            session_id, reason, recoverable
        );

        if !recoverable {
            info!(
                "Session {} disconnect is not recoverable, skipping reconnect",
                session_id
            );
            return;
        }

        // Get session config for reconnection
        let config = match self.registry.get_config(&session_id) {
            Some(c) => c,
            None => {
                warn!(
                    "Session {} not found in registry, cannot reconnect",
                    session_id
                );
                return;
            }
        };

        // Create cancel flag
        let cancel_flag = Arc::new(AtomicBool::new(false));
        {
            let mut active = self.active_reconnects.write();
            active.insert(session_id.clone(), cancel_flag.clone());
        }

        // Clone self for the spawned task
        let service = Arc::clone(self);
        let session_id_clone = session_id.clone();

        // Spawn reconnection task
        tokio::spawn(async move {
            let result = service
                .run_reconnect_loop(&session_id_clone, config, cancel_flag)
                .await;

            // Remove from active reconnects
            {
                let mut active = service.active_reconnects.write();
                active.remove(&session_id_clone);
            }

            if let Err(e) = result {
                error!(
                    "Reconnection failed for session {}: {:?}",
                    session_id_clone, e
                );
            }
        });
    }

    /// Run the reconnection loop with exponential backoff
    /// Note: Frontend events are now handled by connection_status_changed in SshConnectionRegistry
    async fn run_reconnect_loop(
        &self,
        session_id: &str,
        config: SessionConfig,
        cancel_flag: Arc<AtomicBool>,
    ) -> Result<(), ReconnectError> {
        let reconnect_config = ReconnectConfig::default();
        let max_attempts = reconnect_config.max_attempts;
        let mut current_delay = reconnect_config.initial_delay_ms;

        for attempt in 1..=max_attempts {
            // Check cancel flag
            if cancel_flag.load(Ordering::SeqCst) {
                info!("Session {}: reconnection cancelled by user", session_id);
                return Err(ReconnectError::Cancelled);
            }

            // Check if paused (network offline)
            while self.paused.load(Ordering::SeqCst) {
                if cancel_flag.load(Ordering::SeqCst) {
                    return Err(ReconnectError::Cancelled);
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }

            info!(
                "Session {}: reconnect attempt {}/{} in {}ms",
                session_id, attempt, max_attempts, current_delay
            );

            // Wait before attempt (except first)
            if attempt > 1 {
                // Wait in small increments to allow cancellation
                let delay = Duration::from_millis(current_delay);
                let check_interval = Duration::from_millis(100);
                let mut elapsed = Duration::ZERO;

                while elapsed < delay {
                    if cancel_flag.load(Ordering::SeqCst) {
                        return Err(ReconnectError::Cancelled);
                    }
                    if self.paused.load(Ordering::SeqCst) {
                        break; // Will be handled in next iteration
                    }
                    tokio::time::sleep(check_interval.min(delay - elapsed)).await;
                    elapsed += check_interval;
                }
            }

            // Attempt reconnection
            match self.try_reconnect(session_id, &config).await {
                Ok(()) => {
                    info!(
                        "Session {}: reconnected successfully on attempt {}",
                        session_id, attempt
                    );
                    return Ok(());
                }
                Err(e) => {
                    warn!(
                        "Session {}: reconnect attempt {} failed: {}",
                        session_id, attempt, e
                    );
                }
            }

            // Calculate next delay with exponential backoff
            current_delay = ((current_delay as f64 * reconnect_config.backoff_multiplier) as u64)
                .min(reconnect_config.max_delay_ms);
        }

        // All attempts exhausted
        error!(
            "Session {}: all {} reconnection attempts exhausted",
            session_id, max_attempts
        );

        Err(ReconnectError::MaxAttemptsReached(max_attempts))
    }

    /// Try to reconnect a single session
    async fn try_reconnect(&self, session_id: &str, config: &SessionConfig) -> Result<(), String> {
        // Build SSH config from session config
        let ssh_auth = match &config.auth {
            super::types::AuthMethod::Password { password } => {
                SshAuthMethod::Password { password: password.clone() }
            }
            super::types::AuthMethod::Key {
                key_path,
                passphrase,
            } => SshAuthMethod::Key {
                key_path: key_path.clone(),
                passphrase: passphrase.clone(),
            },
            super::types::AuthMethod::Certificate {
                key_path,
                cert_path,
                passphrase,
            } => SshAuthMethod::Certificate {
                key_path: key_path.clone(),
                cert_path: cert_path.clone(),
                passphrase: passphrase.clone(),
            },
            super::types::AuthMethod::Agent => {
                // Agent authentication is supported
                SshAuthMethod::Agent
            }
            super::types::AuthMethod::KeyboardInteractive => {
                // KeyboardInteractive cannot be auto-reconnected
                // User must manually re-initiate 2FA auth flow
                return Err("KeyboardInteractive sessions cannot be auto-reconnected. Please manually reconnect with 2FA.".to_string());
            }
        };

        let ssh_config = SshConfig {
            host: config.host.clone(),
            port: config.port,
            username: config.username.clone(),
            auth: ssh_auth,
            timeout_secs: 30,
            cols: config.cols,
            rows: config.rows,
            proxy_chain: None,
            strict_host_key_checking: false,
            trust_host_key: None, // Auto-reconnect uses known_hosts, no TOFU needed
        };

        // Connect
        let client = SshClient::new(ssh_config);
        let session = client
            .connect()
            .await
            .map_err(|e| format!("Connection failed: {}", e))?;

        // Request shell
        let (session_handle, handle_controller) = session
            .request_shell_extended()
            .await
            .map_err(|e| format!("Shell request failed: {}", e))?;

        // Clone cmd_tx BEFORE passing session_handle to WsBridge
        // (WsBridge consumes the handle, including its cmd_tx)
        let cmd_tx = session_handle.cmd_tx.clone();

        // Get scroll buffer for this session
        let scroll_buffer = self
            .registry
            .with_session(session_id, |entry| entry.scroll_buffer.clone())
            .ok_or_else(|| "Session not found in registry".to_string())?;

        // Start WebSocket bridge (consumes session_handle)
        let (_, ws_port, ws_token) =
            crate::bridge::WsBridge::start_extended(session_handle, scroll_buffer, false)
                .await
                .map_err(|e| format!("WebSocket bridge failed: {}", e))?;

        // Clone handle_controller for forwarding manager
        let forwarding_controller = handle_controller.clone();

        // Update registry with new connection details
        self.registry
            .connect_success(session_id, ws_port, cmd_tx, handle_controller)
            .map_err(|e| format!("Registry update failed: {}", e))?;

        // Update ws_token in registry
        if let Err(e) = self.registry.update_ws_token(session_id, ws_token) {
            warn!("Failed to update ws_token: {}", e);
        }

        // Restore port forwards
        self.restore_port_forwards(session_id, forwarding_controller)
            .await;

        Ok(())
    }

    /// Restore port forwards after successful reconnection
    async fn restore_port_forwards(
        &self,
        session_id: &str,
        handle_controller: crate::ssh::HandleController,
    ) {
        // Get the old forwarding manager to retrieve saved rules
        if let Some(old_manager) = self.forwarding_registry.get(session_id).await {
            let stopped_rules = old_manager.list_stopped_forwards().await;

            if stopped_rules.is_empty() {
                info!("No port forwards to restore for session {}", session_id);
                return;
            }

            info!(
                "Restoring {} port forwards for session {}",
                stopped_rules.len(),
                session_id
            );

            // Create new forwarding manager with the new handle_controller
            let new_manager = ForwardingManager::new(handle_controller, session_id.to_string());

            // Restore each forward rule
            let mut restored_count = 0;
            let mut failed_count = 0;

            for rule in stopped_rules {
                info!(
                    "Restoring forward: {} ({}:{} -> {}:{})",
                    rule.id, rule.bind_address, rule.bind_port, rule.target_host, rule.target_port
                );

                match new_manager.create_forward(rule.clone()).await {
                    Ok(_) => {
                        restored_count += 1;
                        info!("Successfully restored forward: {}", rule.id);
                    }
                    Err(e) => {
                        failed_count += 1;
                        warn!("Failed to restore forward {}: {}", rule.id, e);
                    }
                }
            }

            info!(
                "Port forward restoration complete for session {}: {} restored, {} failed",
                session_id, restored_count, failed_count
            );

            // Replace the old manager with the new one
            self.forwarding_registry
                .register(session_id.to_string(), new_manager)
                .await;
        } else {
            info!(
                "No forwarding manager found for session {}, skipping port forward restoration",
                session_id
            );
        }
    }

    /// Cancel reconnection for a session
    pub fn cancel_reconnect(&self, session_id: &str) {
        if let Some(cancel_flag) = self.active_reconnects.read().get(session_id) {
            cancel_flag.store(true, Ordering::SeqCst);
            info!("Cancelled reconnection for session {}", session_id);
        }
    }

    /// Set network status
    pub fn set_network_status(&self, online: bool) {
        let was_offline = !self.network_online.swap(online, Ordering::SeqCst);

        if online && was_offline {
            info!("Network recovered, resuming reconnection attempts");
            self.paused.store(false, Ordering::SeqCst);
        } else if !online {
            info!("Network offline, pausing reconnection attempts");
            self.paused.store(true, Ordering::SeqCst);
        }
    }

    /// Check if network is online
    pub fn is_network_online(&self) -> bool {
        self.network_online.load(Ordering::SeqCst)
    }

    /// Trigger reconnect for all disconnected sessions (e.g., on network recovery)
    pub async fn reconnect_all_disconnected(self: &Arc<Self>) {
        let disconnected = self
            .registry
            .list_by_state(super::state::SessionState::Error);

        for session in disconnected {
            if !self.is_reconnecting(&session.id) {
                info!(
                    "Triggering reconnect for disconnected session {}",
                    session.id
                );
                self.trigger_reconnect(session.id.clone(), "Network recovered".to_string(), true)
                    .await;
            }
        }
    }

    /// Pause all reconnection attempts
    pub fn pause_all(&self) {
        self.paused.store(true, Ordering::SeqCst);
    }

    /// Resume all reconnection attempts
    pub fn resume_all(&self) {
        self.paused.store(false, Ordering::SeqCst);
    }
}
