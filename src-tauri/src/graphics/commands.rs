//! Tauri IPC commands for WSL Graphics.
//!
//! Seven commands exposed to the frontend via `generate_handler!`:
//! - `wsl_graphics_list_distros`: List WSL distributions
//! - `wsl_graphics_start`: Start a desktop graphics session (Xtigervnc + DE + bridge)
//! - `wsl_graphics_start_app`: Start a single-app graphics session (Xtigervnc + WM + app)
//! - `wsl_graphics_stop`: Stop a graphics session (full cleanup)
//! - `wsl_graphics_reconnect`: Rebuild the WS bridge without restarting VNC/desktop
//! - `wsl_graphics_list_sessions`: List active graphics sessions
//! - `wsl_graphics_detect_wslg`: Detect WSLg availability in a distro

use std::sync::Arc;
use std::time::Duration;

use tauri::State;

use super::{
    bridge, limits, wsl, wslg, GraphicsSessionMode, WslDistro, WslGraphicsHandle,
    WslGraphicsSession, WslGraphicsState, WslgStatus,
};

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
    let (desktop_cmd, dbus_cmd, extra_env, desktop_name) = wsl::check_prerequisites(&distro)
        .await
        .map_err(|e| e.to_string())?;
    tracing::info!(
        "WSL Graphics: prerequisites OK for '{}' (desktop={} [{}], dbus={})",
        distro,
        desktop_cmd,
        desktop_name,
        dbus_cmd
    );

    // 2. Start Xtigervnc + desktop session
    let (vnc_port, vnc_child, desktop_child) =
        wsl::start_session(&distro, &desktop_cmd, &dbus_cmd, extra_env)
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
        desktop_name: desktop_name.to_string(),
        mode: GraphicsSessionMode::Desktop,
    };

    let handle = WslGraphicsHandle {
        info: session.clone(),
        vnc_child,
        desktop_child,
        app_child: None,
        bridge_handle,
        distro: distro.clone(),
        vnc_port,
        desktop_name: desktop_name.to_string(),
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
            // 4. Kill the app child process
            if let Some(ref mut app) = handle.app_child {
                let _ = app.kill().await;
            }
            // 5. Session-level cleanup (PID file, pkill, temp dirs)
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

/// Detect WSLg availability in a WSL distribution.
///
/// Probes system-level mounts and sockets (not environment variables)
/// to determine whether WSLg Wayland and XWayland are available.
#[tauri::command]
pub async fn wsl_graphics_detect_wslg(distro: String) -> Result<WslgStatus, String> {
    wslg::detect_wslg(&distro).await.map_err(|e| e.to_string())
}

// ─── App Mode ───────────────────────────────────────────────────────

/// Validate argv array, rejecting dangerous inputs.
///
/// Six-layer defense per §11 of the design document:
/// 1. Non-empty argv
/// 2. Non-empty program name
/// 3. No shell metacharacters in any argument
/// 4. No path traversal (..)
/// 5. No relative paths (./)
/// 6. Total length limit (4096 bytes)
fn validate_argv(argv: &[String]) -> Result<(), String> {
    if argv.is_empty() {
        return Err("argv must contain at least one element (the program name)".into());
    }

    let program = &argv[0];

    // Rule 1: program name must not be empty
    if program.is_empty() {
        return Err("Program name cannot be empty".into());
    }

    // Rule 2: forbid shell metacharacters (prevent injection)
    const FORBIDDEN: &[char] = &[
        ';', '|', '&', '`', '$', '(', ')', '{', '}', '<', '>', '\n', '\r', '\\', '\'', '"', '!',
        '#',
    ];
    for (i, arg) in argv.iter().enumerate() {
        for ch in FORBIDDEN {
            if arg.contains(*ch) {
                return Err(format!(
                    "argv[{}] contains forbidden shell metacharacter '{}'",
                    i, ch
                ));
            }
        }
    }

    // Rule 3: program name must be a bare command or absolute path
    //   ✓ "gedit"           → which lookup
    //   ✓ "/usr/bin/gedit"  → absolute path
    //   ✗ "../../../bin/sh" → path traversal
    //   ✗ "./exploit"       → relative path
    if program.contains("..") {
        return Err("Program name must not contain '..' (path traversal)".into());
    }
    if program.starts_with("./") || program.starts_with("../") {
        return Err(
            "Program name must be a bare command or absolute path, not relative".into(),
        );
    }

    // Rule 4: total length limit (prevent huge payloads)
    let total_len: usize = argv.iter().map(|a| a.len()).sum();
    if total_len > 4096 {
        return Err(format!(
            "Total argv length ({}) exceeds limit (4096 bytes)",
            total_len
        ));
    }

    Ok(())
}

/// Start a single-app graphics session (no desktop environment).
///
/// 1. Validates argv (security: no shell metacharacters, path traversal, etc.)
/// 2. Checks concurrency limits
/// 3. Checks VNC server availability
/// 4. Starts Xtigervnc + optional WM + target application
/// 5. Starts WebSocket ↔ TCP proxy bridge
/// 6. Spawns app exit watcher for automatic cleanup
/// 7. Returns session info
#[tauri::command]
pub async fn wsl_graphics_start_app(
    state: State<'_, Arc<WslGraphicsState>>,
    distro: String,
    argv: Vec<String>,
    title: Option<String>,
    geometry: Option<String>,
) -> Result<WslGraphicsSession, String> {
    // ── 1. Validate argv (§11 security) ──
    validate_argv(&argv)?;

    // ── 2. Concurrency limits (§12) ──
    {
        let sessions = state.sessions.read().await;

        // Global app session limit
        let app_count = sessions
            .values()
            .filter(|h| matches!(h.info.mode, GraphicsSessionMode::App { .. }))
            .count();
        if app_count >= limits::MAX_APP_SESSIONS_GLOBAL {
            return Err(format!(
                "Global app session limit reached (max {}). Stop an existing session first.",
                limits::MAX_APP_SESSIONS_GLOBAL
            ));
        }

        // Per-distro app session limit
        let distro_count = sessions
            .values()
            .filter(|h| {
                h.distro == distro && matches!(h.info.mode, GraphicsSessionMode::App { .. })
            })
            .count();
        if distro_count >= limits::MAX_APP_SESSIONS_PER_DISTRO {
            return Err(format!(
                "App session limit reached for '{}' (max {}). Stop an existing session first.",
                distro, limits::MAX_APP_SESSIONS_PER_DISTRO
            ));
        }
    }

    // ── 3. Check VNC availability (no need to check desktop env) ──
    wsl::check_vnc_available(&distro)
        .await
        .map_err(|e| e.to_string())?;

    // ── 4. Start Xtigervnc + app ──
    let geo = geometry.as_deref().unwrap_or("1280x720");
    let (vnc_port, _x_display, mut vnc_child, app_child) =
        match wsl::start_app_session(&distro, &argv, Some(geo)).await {
            Ok(result) => result,
            Err(e) => return Err(e.to_string()),
        };
    tracing::info!(
        "WSL Graphics App: Xtigervnc on port {}, app '{}' launched",
        vnc_port,
        argv[0]
    );

    // ── 4.5. Check if app exited immediately ──
    tokio::time::sleep(Duration::from_millis(500)).await;
    // We can't try_wait on the wsl.exe wrapper easily, so skip instant-crash detection
    // and rely on the exit watcher

    // ── 5. Start WebSocket ↔ TCP proxy bridge ──
    let session_id = uuid::Uuid::new_v4().to_string();
    let vnc_addr = format!("127.0.0.1:{}", vnc_port);
    let (ws_port, ws_token, bridge_handle) =
        match bridge::start_proxy(vnc_addr, session_id.clone()).await {
            Ok(result) => result,
            Err(e) => {
                // Rollback: kill app + vnc explicitly, then clean up WSL session
                let _ = app_child.kill().await;
                let _ = vnc_child.kill().await;
                wsl::cleanup_wsl_session(&distro).await;
                return Err(e.to_string());
            }
        };
    tracing::info!("WSL Graphics App: WebSocket proxy on port {}", ws_port);

    // ── 6. Register session ──
    let app_title = title
        .clone()
        .unwrap_or_else(|| argv[0].clone());

    let session = WslGraphicsSession {
        id: session_id.clone(),
        ws_port,
        ws_token: ws_token.clone(),
        distro: distro.clone(),
        desktop_name: app_title.clone(),
        mode: GraphicsSessionMode::App {
            argv: argv.clone(),
            title,
        },
    };

    let handle = WslGraphicsHandle {
        info: session.clone(),
        vnc_child,
        desktop_child: None,
        app_child: Some(app_child),
        bridge_handle,
        distro: distro.clone(),
        vnc_port,
        desktop_name: app_title,
    };

    state
        .sessions
        .write()
        .await
        .insert(session_id.clone(), handle);

    // ── 7. Spawn app exit watcher (automatic cleanup when app closes) ──
    let state_clone = state.inner().clone();
    let sid = session_id.clone();
    let distro_clone = distro.clone();
    tokio::spawn(async move {
        watch_app_exit(sid, distro_clone, state_clone).await;
    });

    Ok(session)
}

/// Watch for app process exit and automatically clean up the session.
///
/// Polls the session's app_child process. When it exits, the entire session
/// (VNC + bridge + WSL cleanup) is torn down automatically.
async fn watch_app_exit(
    session_id: String,
    distro: String,
    state: Arc<WslGraphicsState>,
) {
    // Poll every 2 seconds until the app process exits
    loop {
        tokio::time::sleep(Duration::from_secs(2)).await;

        let mut sessions = state.sessions.write().await;
        let Some(handle) = sessions.get_mut(&session_id) else {
            // Session was already removed (manual stop)
            return;
        };

        // Check if app_child has exited
        if let Some(ref mut app) = handle.app_child {
            match app.try_wait() {
                Ok(Some(status)) => {
                    // Read stderr for diagnostic info
                    let stderr_msg = if let Some(mut stderr) = handle
                        .app_child
                        .as_mut()
                        .and_then(|c| c.stderr.take())
                    {
                        use tokio::io::AsyncReadExt;
                        let mut buf = Vec::with_capacity(4096);
                        let _ = stderr.read_to_end(&mut buf).await;
                        String::from_utf8_lossy(&buf).trim().to_string()
                    } else {
                        String::new()
                    };

                    if stderr_msg.is_empty() {
                        tracing::info!(
                            "WSL Graphics App: process exited for session {} (status: {:?})",
                            session_id,
                            status
                        );
                    } else {
                        tracing::warn!(
                            "WSL Graphics App: process exited for session {} (status: {:?}), stderr: {}",
                            session_id,
                            status,
                            stderr_msg
                        );
                    }
                    // App exited — tear down the whole session
                    let mut handle = sessions.remove(&session_id).unwrap();
                    handle.bridge_handle.abort();
                    let _ = handle.vnc_child.kill().await;
                    if let Some(ref mut desktop) = handle.desktop_child {
                        let _ = desktop.kill().await;
                    }
                    wsl::cleanup_wsl_session(&distro).await;
                    // TODO: emit Tauri event to frontend for UI update
                    // e.g. app_handle.emit("wsl-graphics-app-exited", &session_id)
                    return;
                }
                Ok(None) => {
                    // Still running — continue polling
                }
                Err(e) => {
                    tracing::warn!(
                        "WSL Graphics App: error checking process for session {}: {}",
                        session_id,
                        e
                    );
                    // Treat as exited to be safe
                    let mut handle = sessions.remove(&session_id).unwrap();
                    handle.bridge_handle.abort();
                    let _ = handle.vnc_child.kill().await;
                    wsl::cleanup_wsl_session(&distro).await;
                    return;
                }
            }
        } else {
            // No app_child (shouldn't happen for app sessions, but be safe)
            return;
        }
    }
}
