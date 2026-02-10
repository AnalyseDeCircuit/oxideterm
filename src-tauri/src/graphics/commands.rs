//! Tauri IPC commands for WSL Graphics.
//!
//! Five commands exposed to the frontend via `generate_handler!`:
//! - `wsl_graphics_list_distros`: List WSL distributions
//! - `wsl_graphics_start`: Start a graphics session (Xtigervnc + desktop + bridge)
//! - `wsl_graphics_stop`: Stop a graphics session (full cleanup)
//! - `wsl_graphics_reconnect`: Rebuild the WS bridge without restarting VNC/desktop
//! - `wsl_graphics_list_sessions`: List active graphics sessions

use std::sync::Arc;

use tauri::State;

use super::{bridge, wsl, WslDistro, WslGraphicsHandle, WslGraphicsSession, WslGraphicsState};

/// List available WSL distributions.
#[tauri::command]
pub async fn wsl_graphics_list_distros() -> Result<Vec<WslDistro>, String> {
    wsl::list_distros().await.map_err(|e| e.to_string())
}

/// Start a graphics session for a WSL distribution.
///
/// 1. Checks prerequisites (Xtigervnc, desktop, D-Bus)
/// 2. Starts Xtigervnc + desktop session via bootstrap script
/// 3. Starts WebSocket ↔ TCP proxy bridge
/// 4. Returns session info (ws_port, token, etc.)
#[tauri::command]
pub async fn wsl_graphics_start(
    state: State<'_, Arc<WslGraphicsState>>,
    distro: String,
) -> Result<WslGraphicsSession, String> {
    // Check if there's already an active session for this distro
    {
        let sessions = state.sessions.read().await;
        for handle in sessions.values() {
            if handle.info.distro == distro {
                return Err(format!(
                    "A graphics session is already active for '{}'. Stop it first.",
                    distro
                ));
            }
        }
    }

    // 1. Check prerequisites: Xtigervnc, desktop environment, D-Bus
    let (desktop_cmd, dbus_cmd) = wsl::check_prerequisites(&distro)
        .await
        .map_err(|e| e.to_string())?;
    tracing::info!(
        "WSL Graphics: prerequisites OK for '{}' (desktop={}, dbus={})",
        distro,
        desktop_cmd,
        dbus_cmd
    );

    // 2. Start Xtigervnc + desktop session
    let (vnc_port, vnc_child, desktop_child) =
        wsl::start_session(&distro, &desktop_cmd, &dbus_cmd)
            .await
            .map_err(|e| e.to_string())?;
    tracing::info!(
        "WSL Graphics: Xtigervnc started on port {}, desktop launched",
        vnc_port
    );

    // 3. Start WebSocket ↔ TCP proxy bridge
    let vnc_addr = format!("127.0.0.1:{}", vnc_port);
    let session_id = uuid::Uuid::new_v4().to_string();
    let (ws_port, ws_token, bridge_handle) = match bridge::start_proxy(
        vnc_addr,
        session_id.clone(),
    )
    .await
    {
        Ok(result) => result,
        Err(e) => {
            // Kill VNC + desktop to avoid orphan processes
            let mut child = vnc_child;
            let _ = child.kill().await;
            if let Some(mut dc) = desktop_child {
                let _ = dc.kill().await;
            }
            wsl::cleanup_wsl_session(&distro).await;
            return Err(e.to_string());
        }
    };
    tracing::info!("WSL Graphics: WebSocket proxy on port {}", ws_port);

    // 4. Register session
    let session = WslGraphicsSession {
        id: session_id.clone(),
        ws_port,
        ws_token: ws_token.clone(),
        distro: distro.clone(),
    };

    let handle = WslGraphicsHandle {
        info: session.clone(),
        vnc_child,
        desktop_child,
        bridge_handle,
        distro: distro.clone(),
        vnc_port,
    };

    state.sessions.write().await.insert(session_id, handle);

    Ok(session)
}

/// Stop a graphics session (full cleanup).
///
/// Kills bridge, VNC, desktop, and runs WSL session cleanup (PID file, temp dirs).
/// Idempotent: returns Ok(()) even if the session was already removed.
#[tauri::command]
pub async fn wsl_graphics_stop(
    state: State<'_, Arc<WslGraphicsState>>,
    session_id: String,
) -> Result<(), String> {
    let mut sessions = state.sessions.write().await;
    match sessions.remove(&session_id) {
        Some(mut handle) => {
            tracing::info!("WSL Graphics: stopping session {}", session_id);
            // 1. Abort the bridge proxy task
            handle.bridge_handle.abort();
            // 2. Kill the VNC child process
            let _ = handle.vnc_child.kill().await;
            // 3. Kill the desktop child process
            if let Some(ref mut desktop) = handle.desktop_child {
                let _ = desktop.kill().await;
            }
            // 4. Session-level cleanup (PID file, pkill, temp dirs)
            wsl::cleanup_wsl_session(&handle.distro).await;
            Ok(())
        }
        None => {
            tracing::debug!(
                "WSL Graphics: session {} already removed, ignoring",
                session_id
            );
            Ok(())
        }
    }
}

/// Reconnect a graphics session by rebuilding only the WebSocket bridge.
///
/// VNC and desktop processes stay alive. A new bridge is spawned with
/// a new WS port and token. The old bridge is aborted first.
#[tauri::command]
pub async fn wsl_graphics_reconnect(
    state: State<'_, Arc<WslGraphicsState>>,
    session_id: String,
) -> Result<WslGraphicsSession, String> {
    // Look up existing session and extract VNC port
    let vnc_port = {
        let sessions = state.sessions.read().await;
        let handle = sessions
            .get(&session_id)
            .ok_or_else(|| format!("Session '{}' not found", session_id))?;
        handle.vnc_port
    };

    // Abort old bridge (if still running)
    {
        let sessions = state.sessions.read().await;
        if let Some(handle) = sessions.get(&session_id) {
            handle.bridge_handle.abort();
        }
    }

    tracing::info!(
        "WSL Graphics: reconnecting session {} (VNC port {})",
        session_id,
        vnc_port
    );

    // Start new bridge
    let vnc_addr = format!("127.0.0.1:{}", vnc_port);
    let (ws_port, ws_token, bridge_handle) =
        bridge::start_proxy(vnc_addr, session_id.clone())
            .await
            .map_err(|e| e.to_string())?;

    tracing::info!(
        "WSL Graphics: reconnected — new bridge on port {}",
        ws_port
    );

    // Update handle with new bridge info
    let session = {
        let mut sessions = state.sessions.write().await;
        let handle = sessions
            .get_mut(&session_id)
            .ok_or_else(|| format!("Session '{}' disappeared during reconnect", session_id))?;

        handle.bridge_handle = bridge_handle;
        handle.info.ws_port = ws_port;
        handle.info.ws_token = ws_token.clone();

        handle.info.clone()
    };

    Ok(session)
}

/// List all active graphics sessions.
#[tauri::command]
pub async fn wsl_graphics_list_sessions(
    state: State<'_, Arc<WslGraphicsState>>,
) -> Result<Vec<WslGraphicsSession>, String> {
    let sessions = state.sessions.read().await;
    let list: Vec<WslGraphicsSession> = sessions.values().map(|h| h.info.clone()).collect();
    Ok(list)
}
