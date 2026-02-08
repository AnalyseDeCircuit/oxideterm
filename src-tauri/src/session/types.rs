//! Session Types and Data Structures

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, mpsc, oneshot};

use super::scroll_buffer::ScrollBuffer;
use super::state::{SessionState, SessionStateMachine};
use crate::ssh::{HandleController, SessionCommand};

// Re-export AuthMethod from ssh module (single source of truth)
pub use crate::ssh::AuthMethod;

/// Configuration for establishing an SSH connection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Target hostname or IP
    pub host: String,
    /// SSH port (default: 22)
    pub port: u16,
    /// Username for authentication
    pub username: String,
    /// Authentication method
    pub auth: AuthMethod,
    /// Display name (auto-generated if not provided)
    #[serde(default)]
    pub name: Option<String>,
    /// Color identifier for the session (hex color)
    #[serde(default)]
    pub color: Option<String>,
    /// Initial terminal columns
    #[serde(default = "default_cols")]
    pub cols: u32,
    /// Initial terminal rows
    #[serde(default = "default_rows")]
    pub rows: u32,
}

fn default_cols() -> u32 {
    80
}

fn default_rows() -> u32 {
    24
}

impl SessionConfig {
    /// Create a new config with password authentication
    pub fn with_password(
        host: impl Into<String>,
        port: u16,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        Self {
            host: host.into(),
            port,
            username: username.into(),
            auth: AuthMethod::password(password),
            name: None,
            color: None,
            cols: 80,
            rows: 24,
        }
    }

    /// Create a new config with key authentication
    pub fn with_key(
        host: impl Into<String>,
        port: u16,
        username: impl Into<String>,
        key_path: impl Into<String>,
        passphrase: Option<String>,
    ) -> Self {
        Self {
            host: host.into(),
            port,
            username: username.into(),
            auth: AuthMethod::key(key_path, passphrase),
            name: None,
            color: None,
            cols: 80,
            rows: 24,
        }
    }

    /// Get display name (or generate from config)
    pub fn display_name(&self) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| format!("{}@{}", self.username, self.host))
    }

    /// Generate a consistent color based on host
    pub fn auto_color(&self) -> String {
        self.color.clone().unwrap_or_else(|| {
            // Simple hash-based color generation
            let hash = self
                .host
                .bytes()
                .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
            let hue = hash % 360;
            // HSL to hex (simplified, fixed saturation and lightness)
            hsl_to_hex(hue as f32, 0.7, 0.5)
        })
    }
}

/// Convert HSL to hex color string
fn hsl_to_hex(h: f32, s: f32, l: f32) -> String {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;

    let (r, g, b) = match (h / 60.0) as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    let r = ((r + m) * 255.0) as u8;
    let g = ((g + m) * 255.0) as u8;
    let b = ((b + m) * 255.0) as u8;

    format!("#{:02x}{:02x}{:02x}", r, g, b)
}

/// An active session entry in the registry
pub struct SessionEntry {
    /// Unique session ID
    pub id: String,
    /// Session configuration
    pub config: SessionConfig,
    /// State machine for lifecycle management
    pub state_machine: SessionStateMachine,
    /// WebSocket port for this session
    pub ws_port: Option<u16>,
    /// WebSocket authentication token
    pub ws_token: Option<String>,
    /// Command channel to SSH session
    pub cmd_tx: Option<mpsc::Sender<SessionCommand>>,
    /// Handle controller for opening additional channels (e.g., SFTP, forwarding)
    /// This is a Clone-able wrapper that communicates with the Handle Owner Task
    pub handle_controller: Option<HandleController>,
    /// Terminal scroll buffer for backend storage and search
    pub scroll_buffer: Arc<ScrollBuffer>,
    /// Output broadcast channel for terminal data (supports WS reattach)
    pub output_tx: broadcast::Sender<Vec<u8>>,
    /// WS detached flag (true while client disconnected)
    pub ws_detached: bool,
    /// Cancel handle for WS detach cleanup task
    pub ws_detach_cancel: Option<oneshot::Sender<()>>,
    /// Creation timestamp
    pub created_at: Instant,
    /// Tab order (for UI sorting)
    pub order: usize,
    /// Associated SSH connection ID (for connection pool architecture)
    /// When set, this session uses a shared connection from SshConnectionRegistry
    pub connection_id: Option<String>,
}

impl SessionEntry {
    /// Create a new session entry
    pub fn new(id: String, config: SessionConfig, order: usize) -> Self {
        let (output_tx, _) = broadcast::channel::<Vec<u8>>(256);
        Self {
            id,
            config,
            state_machine: SessionStateMachine::new(),
            ws_port: None,
            ws_token: None,
            cmd_tx: None,
            handle_controller: None,
            scroll_buffer: Arc::new(ScrollBuffer::new()), // Default 10k lines
            output_tx,
            ws_detached: false,
            ws_detach_cancel: None,
            created_at: Instant::now(),
            order,
            connection_id: None,
        }
    }

    /// Create a new session entry with custom buffer size
    pub fn with_buffer_config(
        id: String,
        config: SessionConfig,
        order: usize,
        max_lines: usize,
    ) -> Self {
        let (output_tx, _) = broadcast::channel::<Vec<u8>>(256);
        Self {
            id,
            config,
            state_machine: SessionStateMachine::new(),
            ws_port: None,
            ws_token: None,
            cmd_tx: None,
            handle_controller: None,
            scroll_buffer: Arc::new(ScrollBuffer::with_capacity(max_lines)),
            output_tx,
            ws_detached: false,
            ws_detach_cancel: None,
            created_at: Instant::now(),
            order,
            connection_id: None,
        }
    }

    /// Get current state
    pub fn state(&self) -> SessionState {
        self.state_machine.state()
    }

    /// Get error message if any
    pub fn error(&self) -> Option<&str> {
        self.state_machine.error()
    }

    /// Get uptime in seconds
    pub fn uptime_secs(&self) -> u64 {
        self.created_at.elapsed().as_secs()
    }

    /// Get WebSocket URL
    pub fn ws_url(&self) -> Option<String> {
        self.ws_port.map(|port| format!("ws://localhost:{}", port))
    }

    /// Check if session is connected
    pub fn is_connected(&self) -> bool {
        self.state() == SessionState::Connected
    }

    /// Send resize command to SSH session
    pub async fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        if let Some(tx) = &self.cmd_tx {
            tx.send(SessionCommand::Resize(cols, rows))
                .await
                .map_err(|e| format!("Failed to send resize command: {}", e))
        } else {
            Err("Session not connected".to_string())
        }
    }

    /// Send close command to SSH session
    pub async fn close(&self) -> Result<(), String> {
        if let Some(tx) = &self.cmd_tx {
            let _ = tx.send(SessionCommand::Close).await;
        }
        Ok(())
    }
}

/// Session statistics
#[derive(Debug, Clone, Serialize)]
pub struct SessionStats {
    /// Total number of sessions
    pub total: usize,
    /// Number of connected sessions
    pub connected: usize,
    /// Number of connecting sessions
    pub connecting: usize,
    /// Number of sessions in error state
    pub error: usize,
    /// Maximum allowed concurrent sessions
    pub max_sessions: usize,
}

/// Session info for serialization to frontend
#[derive(Debug, Clone, Serialize)]
pub struct SessionInfo {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub state: SessionState,
    pub error: Option<String>,
    pub ws_url: Option<String>,
    pub ws_token: Option<String>,
    pub color: String,
    pub uptime_secs: u64,
    pub order: usize,
    // Authentication info for reconnection (password is never exposed)
    pub auth_type: String,
    pub key_path: Option<String>,
    // Connection ID for connection pool tracking
    #[serde(rename = "connectionId")]
    pub connection_id: Option<String>,
}

impl From<&SessionEntry> for SessionInfo {
    fn from(entry: &SessionEntry) -> Self {
        // Extract auth_type and key_path from config.auth
        let (auth_type, key_path) = match &entry.config.auth {
            AuthMethod::Password { .. } => ("password".to_string(), None),
            AuthMethod::Key { key_path, .. } => ("key".to_string(), Some(key_path.clone())),
            AuthMethod::Certificate { key_path, .. } => ("certificate".to_string(), Some(key_path.clone())),
            AuthMethod::Agent => ("agent".to_string(), None),
            AuthMethod::KeyboardInteractive => ("keyboard_interactive".to_string(), None),
        };

        Self {
            id: entry.id.clone(),
            name: entry.config.display_name(),
            host: entry.config.host.clone(),
            port: entry.config.port,
            username: entry.config.username.clone(),
            state: entry.state(),
            error: entry.error().map(String::from),
            ws_url: entry.ws_url(),
            ws_token: entry.ws_token.clone(),
            color: entry.config.auto_color(),
            uptime_secs: entry.uptime_secs(),
            order: entry.order,
            auth_type,
            key_path,
            connection_id: entry.connection_id.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_with_password() {
        let config = SessionConfig::with_password("example.com", 22, "user", "pass");
        assert_eq!(config.host, "example.com");
        assert_eq!(config.port, 22);
        assert_eq!(config.display_name(), "user@example.com");
    }

    #[test]
    fn test_auto_color() {
        let config1 = SessionConfig::with_password("server1.com", 22, "user", "pass");
        let config2 = SessionConfig::with_password("server2.com", 22, "user", "pass");

        // Different hosts should produce different colors
        assert_ne!(config1.auto_color(), config2.auto_color());

        // Same host should produce same color
        let config3 = SessionConfig::with_password("server1.com", 22, "other", "pass");
        assert_eq!(config1.auto_color(), config3.auto_color());
    }
}
