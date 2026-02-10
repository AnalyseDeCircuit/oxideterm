//! Tauri IPC commands for WSL Graphics.
//!
//! Four commands exposed to the frontend plugin via `ctx.api.invoke()`:
//! - `wsl_graphics_list_distros`: List WSL distributions
//! - `wsl_graphics_start`: Start a graphics session
//! - `wsl_graphics_stop`: Stop a graphics session
//! - `wsl_graphics_list_sessions`: List active graphics sessions

use std::sync::Arc;

use tauri::State;

use super::{bridge, wsl, WslDistro, WslGraphicsHandle, WslGraphicsSession, WslGraphicsState};

/// List available WSL distributions.
#[tauri::command]
pub async fn wsl_graphics_list_distros() -> Result<Vec<super::WslDistro>, String> {
    wsl::list_distros().await.map_err(|e| e.to_string())
}

/// Start a graphics session for a WSL distribution.
///
/// 1. Detects available VNC server
/// 2. Starts VNC inside WSL
/// 3. Starts WebSocket ↔ TCP proxy
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

    // 1. Detect VNC server
    let vnc_binary = wsl::detect_vnc(&distro).await.map_err(|e| e.to_string())?;
    tracing::info!("WSL Graphics: detected {} in '{}'", vnc_binary, distro);

    // 2. Start VNC server
    let (vnc_port, vnc_child) = wsl::start_vnc(&distro, &vnc_binary)
        .await
        .map_err(|e| e.to_string())?;
    tracing::info!(
        "WSL Graphics: VNC started on port {} ({})",
        vnc_port,
        vnc_binary
    );

    // 3. Start WebSocket ↔ TCP proxy bridge
    let vnc_addr = format!("127.0.0.1:{}", vnc_port);
    let (ws_port, ws_token, bridge_handle) = bridge::start_proxy(vnc_addr)
        .await
        .map_err(|e| e.to_string())?;
    tracing::info!("WSL Graphics: WebSocket proxy on port {}", ws_port);

    // 4. Register session
    let session_id = uuid::Uuid::new_v4().to_string();
    let session = WslGraphicsSession {
        id: session_id.clone(),
        ws_port,
        ws_token: ws_token.clone(),
        distro: distro.clone(),
        vnc_server: vnc_binary,
    };

    let handle = WslGraphicsHandle {
        info: session.clone(),
        vnc_child,
        bridge_handle,
    };

    state.sessions.write().await.insert(session_id, handle);

    // Return session info (token included — frontend needs it)
    Ok(session)
}

/// Stop a graphics session.
#[tauri::command]
pub async fn wsl_graphics_stop(
    state: State<'_, Arc<WslGraphicsState>>,
    session_id: String,
) -> Result<(), String> {
    let mut sessions = state.sessions.write().await;
    match sessions.remove(&session_id) {
        Some(mut handle) => {
            tracing::info!("WSL Graphics: stopping session {}", session_id);
            // Abort the bridge proxy task
            handle.bridge_handle.abort();
            // Kill the VNC child process
            let _ = handle.vnc_child.kill().await;
            Ok(())
        }
        None => Err(format!("Session not found: {}", session_id)),
    }
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
