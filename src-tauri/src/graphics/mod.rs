//! WSL Graphics Forwarding Module
//!
//! Provides VNC-based graphics forwarding for Windows WSL (WSLg) environments.
//!
//! Architecture:
//! - `wsl.rs`: WSL distro detection + Xtigervnc server + desktop/app session management
//! - `bridge.rs`: WebSocket ↔ VNC TCP transparent proxy (supports reconnect)
//! - `wslg.rs`: WSLg availability detection (socket-level probing)
//! - `commands.rs`: Tauri IPC commands exposed to the frontend
//!
//! Two session modes:
//! - **Desktop mode**: Xtigervnc + full desktop environment (Xfce/GNOME/KDE/...)
//! - **App mode**: Xtigervnc + optional WM + single GUI application
//!
//! On non-Windows platforms or without the `wsl-graphics` feature,
//! stub commands are provided that return informative errors.

// Real implementation: Windows + wsl-graphics feature
#[cfg(all(feature = "wsl-graphics", target_os = "windows"))]
pub mod bridge;
#[cfg(all(feature = "wsl-graphics", target_os = "windows"))]
pub mod wsl;
#[cfg(all(feature = "wsl-graphics", target_os = "windows"))]
pub mod wslg;

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
        pub desktop_name: String,
    }

    /// Graphics session mode (stub).
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(tag = "type", rename_all = "camelCase")]
    pub enum GraphicsSessionMode {
        Desktop,
        App {
            argv: Vec<String>,
            title: Option<String>,
        },
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

    /// WSLg availability status (stub).
    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct WslgStatus {
        pub available: bool,
        pub wayland: bool,
        pub x11: bool,
        pub wslg_version: Option<String>,
        pub has_openbox: bool,
    }

    #[tauri::command]
    pub async fn wsl_graphics_detect_wslg(_distro: String) -> Result<WslgStatus, String> {
        Err(
            "WSL Graphics is only available on Windows with the wsl-graphics feature enabled"
                .into(),
        )
    }

    #[tauri::command]
    pub async fn wsl_graphics_start_app(
        _distro: String,
        _argv: Vec<String>,
        _title: Option<String>,
        _geometry: Option<String>,
    ) -> Result<WslGraphicsSession, String> {
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

    /// Resource limits for app sessions
    pub mod limits {
        /// Max app sessions per WSL distro
        pub const MAX_APP_SESSIONS_PER_DISTRO: usize = 4;
        /// Max app sessions globally (across all distros)
        pub const MAX_APP_SESSIONS_GLOBAL: usize = 8;
        /// Max desktop sessions per distro (existing: 1)
        pub const MAX_DESKTOP_SESSIONS_PER_DISTRO: usize = 1;
    }

    /// Graphics session mode
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(tag = "type", rename_all = "camelCase")]
    pub enum GraphicsSessionMode {
        /// Full desktop environment (Xfce, GNOME, KDE, etc.)
        Desktop,
        /// Single application mode (no desktop environment)
        App {
            /// Command argv array (argv[0] = program name)
            argv: Vec<String>,
            /// Optional display title override
            title: Option<String>,
        },
    }

    /// Errors specific to WSL Graphics operations
    #[derive(Debug, Error)]
    pub enum GraphicsError {
        #[error("No VNC server found in WSL distro '{0}'. Install prerequisites:\nsudo apt update && sudo apt install tigervnc-standalone-server dbus-x11 -y\nThen install a desktop: sudo apt install xfce4 -y (recommended), ubuntu-desktop (GNOME), or kde-plasma-desktop (KDE Plasma)")]
        NoVncServer(String),

        #[error("No desktop environment found in WSL distro '{0}'. Install one:\nsudo apt install xfce4 -y  (lightweight, recommended)\nsudo apt install ubuntu-desktop -y  (GNOME, experimental)\nsudo apt install kde-plasma-desktop -y  (KDE Plasma, experimental)")]
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
        /// Human-readable name (e.g. "Xfce", "gedit")
        pub desktop_name: String,
        /// Session mode: Desktop or App
        pub mode: GraphicsSessionMode,
    }

    /// Internal handle for an active graphics session.
    ///
    /// Tracks VNC server, desktop/app session, and WebSocket bridge processes.
    /// On stop/shutdown, all are cleaned up.
    pub(crate) struct WslGraphicsHandle {
        pub info: WslGraphicsSession,
        /// The Xtigervnc process
        pub vnc_child: Child,
        /// The desktop bootstrap script process (dbus + desktop session) — Desktop mode only
        pub desktop_child: Option<Child>,
        /// The application process — App mode only
        pub app_child: Option<Child>,
        /// The WSL distro name (needed for session-level cleanup)
        pub distro: String,
        /// WebSocket ↔ VNC bridge task
        pub bridge_handle: JoinHandle<()>,
        /// The VNC port on localhost (needed for reconnect bridge rebuilds)
        pub vnc_port: u16,
        /// Desktop environment / app display name (for UI)
        pub desktop_name: String,
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
                if let Some(ref mut app) = handle.app_child {
                    let _ = app.kill().await;
                }
                // Session-level cleanup inside WSL (kill orphaned processes)
                crate::graphics::wsl::cleanup_wsl_session(&handle.distro).await;
            }
        }
    }
}

#[cfg(all(feature = "wsl-graphics", target_os = "windows"))]
pub use types::*;

// Re-export WslgStatus from the wslg module (real impl)
#[cfg(all(feature = "wsl-graphics", target_os = "windows"))]
pub use wslg::WslgStatus;

// Re-export WslgStatus from stub commands module (non-Windows)
#[cfg(not(all(feature = "wsl-graphics", target_os = "windows")))]
pub use commands::WslgStatus;
