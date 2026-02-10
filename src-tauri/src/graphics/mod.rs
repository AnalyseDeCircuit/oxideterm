//! WSL Graphics Forwarding Module
//!
//! Provides VNC-based graphics forwarding for Windows WSL (WSLg) environments.
//!
//! Architecture:
//! - `wsl.rs`: WSL distro detection + Xtigervnc server + desktop session management
//! - `bridge.rs`: WebSocket ↔ VNC TCP transparent proxy (supports reconnect)
//! - `commands.rs`: 5 Tauri IPC commands exposed to the frontend
//!
//! Only Xtigervnc is supported — it creates a standalone X server on a free
//! display (avoiding WSLg's Weston on `:0`), then launches a desktop session
//! via a bootstrap script that initializes D-Bus, XDG env vars, etc.
//!
//! On non-Windows platforms or without the `wsl-graphics` feature,
//! stub commands are provided that return informative errors.

// Real implementation: Windows + wsl-graphics feature
#[cfg(all(feature = "wsl-graphics", target_os = "windows"))]
pub mod bridge;
#[cfg(all(feature = "wsl-graphics", target_os = "windows"))]
pub mod wsl;

// Commands: real on Windows+feature, stub otherwise
#[cfg(all(feature = "wsl-graphics", target_os = "windows"))]
pub mod commands;

#[cfg(not(all(feature = "wsl-graphics", target_os = "windows")))]
pub mod commands {
    //! Stub commands for non-Windows platforms or when wsl-graphics feature is disabled.
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct WslDistro {
        pub name: String,
        pub is_default: bool,
        pub is_running: bool,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct WslGraphicsSession {
        pub id: String,
        pub ws_port: u16,
        pub ws_token: String,
        pub distro: String,
    }

    #[tauri::command]
    pub async fn wsl_graphics_list_distros() -> Result<Vec<WslDistro>, String> {
        Err(
            "WSL Graphics is only available on Windows with the wsl-graphics feature enabled"
                .into(),
        )
    }

    #[tauri::command]
    pub async fn wsl_graphics_start(distro: String) -> Result<WslGraphicsSession, String> {
        let _ = distro;
        Err(
            "WSL Graphics is only available on Windows with the wsl-graphics feature enabled"
                .into(),
        )
    }

    #[tauri::command]
    pub async fn wsl_graphics_stop(session_id: String) -> Result<(), String> {
        let _ = session_id;
        Err(
            "WSL Graphics is only available on Windows with the wsl-graphics feature enabled"
                .into(),
        )
    }

    #[tauri::command]
    pub async fn wsl_graphics_reconnect(
        session_id: String,
    ) -> Result<WslGraphicsSession, String> {
        let _ = session_id;
        Err(
            "WSL Graphics is only available on Windows with the wsl-graphics feature enabled"
                .into(),
        )
    }

    #[tauri::command]
    pub async fn wsl_graphics_list_sessions() -> Result<Vec<WslGraphicsSession>, String> {
        Err(
            "WSL Graphics is only available on Windows with the wsl-graphics feature enabled"
                .into(),
        )
    }
}

// Shared types and state — only on Windows+feature
#[cfg(all(feature = "wsl-graphics", target_os = "windows"))]
mod types {
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;
    use thiserror::Error;
    use tokio::process::Child;
    use tokio::sync::RwLock;
    use tokio::task::JoinHandle;

    /// Errors specific to WSL Graphics operations
    #[derive(Debug, Error)]
    pub enum GraphicsError {
        #[error("No VNC server found in WSL distro '{0}'. Install prerequisites:\nsudo apt update && sudo apt install tigervnc-standalone-server xfce4 dbus-x11 -y")]
        NoVncServer(String),

        #[error("No desktop environment found in WSL distro '{0}'. Install one:\nsudo apt update && sudo apt install xfce4 -y")]
        NoDesktop(String),

        #[error("D-Bus is not available in WSL distro '{0}'. Install it:\nsudo apt update && sudo apt install dbus-x11 -y")]
        NoDbus(String),

        #[error("VNC server failed to start within timeout")]
        VncStartTimeout,

        #[error("WSL not available or no distributions found")]
        WslNotAvailable,

        #[error("Session not found: {0}")]
        SessionNotFound(String),

        #[error("IO error: {0}")]
        Io(#[from] std::io::Error),

        #[error("WebSocket error: {0}")]
        WebSocket(#[from] tokio_tungstenite::tungstenite::Error),
    }

    impl From<GraphicsError> for String {
        fn from(e: GraphicsError) -> Self {
            e.to_string()
        }
    }

    /// Information about a WSL distribution
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct WslDistro {
        pub name: String,
        pub is_default: bool,
        pub is_running: bool,
    }

    /// An active graphics session (returned to frontend)
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct WslGraphicsSession {
        pub id: String,
        pub ws_port: u16,
        pub ws_token: String,
        pub distro: String,
    }

    /// Internal handle for an active graphics session.
    ///
    /// Tracks VNC server, desktop session, and WebSocket bridge processes.
    /// On stop/shutdown, all three are cleaned up.
    pub(crate) struct WslGraphicsHandle {
        pub info: WslGraphicsSession,
        /// The Xtigervnc process
        pub vnc_child: Child,
        /// The desktop bootstrap script process (dbus + desktop session)
        pub desktop_child: Option<Child>,
        /// The WSL distro name (needed for session-level cleanup)
        pub distro: String,
        /// WebSocket ↔ VNC bridge task
        pub bridge_handle: JoinHandle<()>,
        /// The VNC port on localhost (needed for reconnect bridge rebuilds)
        pub vnc_port: u16,
    }

    /// Global state for WSL Graphics, managed by Tauri
    pub struct WslGraphicsState {
        pub(crate) sessions: RwLock<HashMap<String, WslGraphicsHandle>>,
    }

    impl WslGraphicsState {
        pub fn new() -> Self {
            Self {
                sessions: RwLock::new(HashMap::new()),
            }
        }

        /// Shut down all active graphics sessions (called on app exit)
        pub async fn shutdown(&self) {
            let mut sessions = self.sessions.write().await;
            for (id, mut handle) in sessions.drain() {
                tracing::info!("Shutting down graphics session: {}", id);
                handle.bridge_handle.abort();
                let _ = handle.vnc_child.kill().await;
                if let Some(ref mut desktop) = handle.desktop_child {
                    let _ = desktop.kill().await;
                }
                // Session-level cleanup inside WSL (kill orphaned processes)
                crate::graphics::wsl::cleanup_wsl_session(&handle.distro).await;
            }
        }
    }
}

#[cfg(all(feature = "wsl-graphics", target_os = "windows"))]
pub use types::*;
