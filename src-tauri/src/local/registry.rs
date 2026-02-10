//! Local Terminal Registry
//!
//! Manages multiple local terminal sessions with thread-safe access.
//! Provides session lifecycle management and event routing.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

use crate::local::session::{LocalTerminalInfo, LocalTerminalSession, SessionError, SessionEvent};
use crate::local::shell::ShellInfo;

/// Registry for managing multiple local terminal sessions
pub struct LocalTerminalRegistry {
    sessions: Arc<RwLock<HashMap<String, LocalTerminalSession>>>,
    /// Channel senders for each session's events (session_id -> sender)
    event_channels: Arc<RwLock<HashMap<String, mpsc::Sender<SessionEvent>>>>,
}

impl LocalTerminalRegistry {
    /// Create a new registry
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            event_channels: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new local terminal session
    pub async fn create_session(
        &self,
        shell: ShellInfo,
        cols: u16,
        rows: u16,
        cwd: Option<std::path::PathBuf>,
    ) -> Result<(String, mpsc::Receiver<SessionEvent>), SessionError> {
        // Use defaults: load profile, no OMP
        self.create_session_with_options(shell, cols, rows, cwd, true, false, None)
            .await
    }

    /// Create a new local terminal session with extended options
    pub async fn create_session_with_options(
        &self,
        shell: ShellInfo,
        cols: u16,
        rows: u16,
        cwd: Option<std::path::PathBuf>,
        load_profile: bool,
        oh_my_posh_enabled: bool,
        oh_my_posh_theme: Option<String>,
    ) -> Result<(String, mpsc::Receiver<SessionEvent>), SessionError> {
        let mut session = LocalTerminalSession::new(shell, cols, rows);
        let session_id = session.id.clone();

        // Create event channel for this session
        let (event_tx, event_rx) = mpsc::channel::<SessionEvent>(256);

        // Store event sender
        {
            let mut channels = self.event_channels.write().await;
            channels.insert(session_id.clone(), event_tx.clone());
        }

        // Start the session with options
        session
            .start_with_options(
                cwd,
                event_tx,
                load_profile,
                oh_my_posh_enabled,
                oh_my_posh_theme,
            )
            .await?;

        // Store session
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id.clone(), session);
        }

        tracing::info!(
            "Created local terminal session: {}, total sessions: {}",
            session_id,
            self.sessions.read().await.len()
        );

        Ok((session_id, event_rx))
    }

    /// Get session info
    pub async fn get_session_info(&self, session_id: &str) -> Option<LocalTerminalInfo> {
        let sessions = self.sessions.read().await;
        sessions.get(session_id).map(|s| s.info())
    }

    /// List all sessions
    pub async fn list_sessions(&self) -> Vec<LocalTerminalInfo> {
        let sessions = self.sessions.read().await;
        sessions.values().map(|s| s.info()).collect()
    }

    /// Write data to a session
    pub async fn write_to_session(
        &self,
        session_id: &str,
        data: &[u8],
    ) -> Result<(), SessionError> {
        let sessions = self.sessions.read().await;
        match sessions.get(session_id) {
            Some(session) => session.write(data).await,
            None => Err(SessionError::NotFound(session_id.to_string())),
        }
    }

    /// Resize a session
    pub async fn resize_session(
        &self,
        session_id: &str,
        cols: u16,
        rows: u16,
    ) -> Result<(), SessionError> {
        let mut sessions = self.sessions.write().await;
        match sessions.get_mut(session_id) {
            Some(session) => session.resize(cols, rows),
            None => Err(SessionError::NotFound(session_id.to_string())),
        }
    }

    /// Close a session
    pub async fn close_session(&self, session_id: &str) -> Result<(), SessionError> {
        // Remove and close session
        let mut sessions = self.sessions.write().await;
        match sessions.remove(session_id) {
            Some(mut session) => {
                session.close();
                tracing::info!(
                    "Closed local terminal session: {}, remaining: {}",
                    session_id,
                    sessions.len()
                );
            }
            None => {
                return Err(SessionError::NotFound(session_id.to_string()));
            }
        }

        // Remove event channel
        {
            let mut channels = self.event_channels.write().await;
            channels.remove(session_id);
        }

        Ok(())
    }

    /// Close all sessions
    pub async fn close_all(&self) {
        let mut sessions = self.sessions.write().await;
        for (id, mut session) in sessions.drain() {
            tracing::info!("Closing local terminal session: {}", id);
            session.close();
        }

        let mut channels = self.event_channels.write().await;
        channels.clear();
    }

    /// Get the number of active sessions
    pub async fn session_count(&self) -> usize {
        self.sessions.read().await.len()
    }

    /// Check if a session exists and is running
    pub async fn is_session_running(&self, session_id: &str) -> bool {
        let sessions = self.sessions.read().await;
        sessions
            .get(session_id)
            .map(|s| s.is_running())
            .unwrap_or(false)
    }

    /// Clean up dead sessions (sessions that have stopped running)
    pub async fn cleanup_dead_sessions(&self) -> Vec<String> {
        let mut sessions = self.sessions.write().await;
        let mut channels = self.event_channels.write().await;

        let dead_ids: Vec<String> = sessions
            .iter()
            .filter(|(_, session)| !session.is_running())
            .map(|(id, _)| id.clone())
            .collect();

        for id in &dead_ids {
            if let Some(mut session) = sessions.remove(id) {
                session.close();
            }
            channels.remove(id);
            tracing::info!("Cleaned up dead session: {}", id);
        }

        dead_ids
    }
}

impl Default for LocalTerminalRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for LocalTerminalRegistry {
    fn drop(&mut self) {
        // Note: async cleanup cannot happen in Drop
        // Sessions should be closed explicitly before dropping
        tracing::debug!("LocalTerminalRegistry dropped");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_registry_new() {
        let registry = LocalTerminalRegistry::new();
        assert_eq!(registry.session_count().await, 0);
        assert!(registry.list_sessions().await.is_empty());
    }

    // Note: Full integration tests require a real terminal environment
}
