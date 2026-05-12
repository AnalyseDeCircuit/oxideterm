// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Session registry for WSL graphics.
//!
//! Tauri owns WSL graphics sessions in backend state, not in the React view.
//! Native keeps the same ownership boundary here: the registry owns VNC,
//! desktop/app child processes, and the one-shot noVNC bridge task.

use std::{collections::HashMap, sync::Arc};

use tokio::process::Child;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

use crate::{GraphicsSessionMode, WslGraphicsError, WslGraphicsSession, bridge, wsl};

pub const MAX_APP_SESSIONS_PER_DISTRO: usize = 4;
pub const MAX_APP_SESSIONS_GLOBAL: usize = 8;
pub const MAX_DESKTOP_SESSIONS_PER_DISTRO: usize = 1;

pub struct WslGraphicsState {
    sessions: Arc<RwLock<HashMap<String, WslGraphicsHandle>>>,
}

struct WslGraphicsHandle {
    info: WslGraphicsSession,
    vnc_child: Child,
    desktop_child: Option<Child>,
    app_child: Option<Child>,
    bridge_handle: JoinHandle<()>,
    distro: String,
    vnc_port: u16,
    stop_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl WslGraphicsState {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn start_desktop(
        &self,
        distro: String,
    ) -> Result<WslGraphicsSession, WslGraphicsError> {
        {
            let sessions = self.sessions.read().await;
            let desktop_count = sessions
                .values()
                .filter(|handle| {
                    handle.distro == distro
                        && matches!(handle.info.mode, GraphicsSessionMode::Desktop)
                })
                .count();
            if desktop_count >= MAX_DESKTOP_SESSIONS_PER_DISTRO {
                return Err(WslGraphicsError::SessionLimit(format!(
                    "A graphics session is already active for '{}'. Stop it first.",
                    distro
                )));
            }
        }

        let prerequisites = wsl::check_prerequisites_async(&distro).await?;
        let (vnc_port, vnc_child, desktop_child) = match wsl::start_session(
            &distro,
            prerequisites.desktop.launch_cmd,
            &prerequisites.dbus_cmd,
            prerequisites.desktop.extra_env,
        )
        .await
        {
            Ok(result) => result,
            Err(error) => {
                wsl::cleanup_wsl_session(&distro).await;
                return Err(error);
            }
        };

        let session_id = uuid::Uuid::new_v4().to_string();
        let vnc_addr = format!("127.0.0.1:{vnc_port}");
        let (ws_port, ws_token, bridge_handle) =
            match bridge::start_proxy(vnc_addr, session_id.clone()).await {
                Ok(result) => result,
                Err(error) => {
                    wsl::cleanup_wsl_session(&distro).await;
                    return Err(error);
                }
            };

        let session = WslGraphicsSession {
            id: session_id.clone(),
            ws_port,
            ws_token,
            distro: distro.clone(),
            desktop_name: prerequisites.desktop.display_name.to_string(),
            mode: GraphicsSessionMode::Desktop,
        };
        let handle = WslGraphicsHandle {
            info: session.clone(),
            vnc_child,
            desktop_child,
            app_child: None,
            bridge_handle,
            distro,
            vnc_port,
            stop_tx: None,
        };
        self.sessions.write().await.insert(session_id, handle);
        Ok(session)
    }

    pub async fn start_app(
        &self,
        distro: String,
        argv: Vec<String>,
        title: Option<String>,
        geometry: Option<String>,
    ) -> Result<WslGraphicsSession, WslGraphicsError> {
        validate_argv(&argv)?;
        {
            let sessions = self.sessions.read().await;
            let app_count = sessions
                .values()
                .filter(|handle| matches!(handle.info.mode, GraphicsSessionMode::App { .. }))
                .count();
            if app_count >= MAX_APP_SESSIONS_GLOBAL {
                return Err(WslGraphicsError::SessionLimit(format!(
                    "Global app session limit reached (max {}). Stop an existing session first.",
                    MAX_APP_SESSIONS_GLOBAL
                )));
            }

            let distro_count = sessions
                .values()
                .filter(|handle| {
                    handle.distro == distro
                        && matches!(handle.info.mode, GraphicsSessionMode::App { .. })
                })
                .count();
            if distro_count >= MAX_APP_SESSIONS_PER_DISTRO {
                return Err(WslGraphicsError::SessionLimit(format!(
                    "App session limit reached for '{}' (max {}). Stop an existing session first.",
                    distro, MAX_APP_SESSIONS_PER_DISTRO
                )));
            }
        }

        wsl::check_vnc_available(&distro).await?;
        let (vnc_port, _x_display, vnc_child, app_child) =
            wsl::start_app_session(&distro, &argv, geometry.as_deref()).await?;

        // Tauri gives app sessions a short grace point before relying on the
        // exit watcher. Keep the same observable ordering before bridge setup.
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let session_id = uuid::Uuid::new_v4().to_string();
        let vnc_addr = format!("127.0.0.1:{vnc_port}");
        let (ws_port, ws_token, bridge_handle) =
            match bridge::start_proxy(vnc_addr, session_id.clone()).await {
                Ok(result) => result,
                Err(error) => {
                    wsl::cleanup_wsl_session(&distro).await;
                    return Err(error);
                }
            };

        let app_title = title.clone().unwrap_or_else(|| argv[0].clone());
        let session = WslGraphicsSession {
            id: session_id.clone(),
            ws_port,
            ws_token,
            distro: distro.clone(),
            desktop_name: app_title,
            mode: GraphicsSessionMode::App { argv, title },
        };
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
        let handle = WslGraphicsHandle {
            info: session.clone(),
            vnc_child,
            desktop_child: None,
            app_child: Some(app_child),
            bridge_handle,
            distro: distro.clone(),
            vnc_port,
            stop_tx: Some(stop_tx),
        };
        self.sessions
            .write()
            .await
            .insert(session_id.clone(), handle);

        self.spawn_app_exit_watcher(session_id, distro, stop_rx)
            .await;
        Ok(session)
    }

    pub async fn stop(&self, session_id: &str) -> Result<(), WslGraphicsError> {
        let mut sessions = self.sessions.write().await;
        match sessions.remove(session_id) {
            Some(mut handle) => {
                tracing::info!("WSL Graphics: stopping session {}", session_id);
                if let Some(tx) = handle.stop_tx.take() {
                    let _ = tx.send(());
                }
                handle.bridge_handle.abort();
                let _ = handle.vnc_child.kill().await;
                if let Some(desktop) = handle.desktop_child.as_mut() {
                    let _ = desktop.kill().await;
                }
                if let Some(app) = handle.app_child.as_mut() {
                    let _ = app.kill().await;
                }
                drop(sessions);
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

    pub async fn reconnect(
        &self,
        session_id: &str,
    ) -> Result<WslGraphicsSession, WslGraphicsError> {
        let vnc_port = {
            let sessions = self.sessions.read().await;
            sessions
                .get(session_id)
                .ok_or_else(|| WslGraphicsError::SessionNotFound(session_id.to_string()))?
                .vnc_port
        };

        {
            let sessions = self.sessions.read().await;
            if let Some(handle) = sessions.get(session_id) {
                handle.bridge_handle.abort();
            }
        }

        tracing::info!(
            "WSL Graphics: reconnecting session {} (VNC port {})",
            session_id,
            vnc_port
        );

        let vnc_addr = format!("127.0.0.1:{vnc_port}");
        let (ws_port, ws_token, bridge_handle) =
            bridge::start_proxy(vnc_addr, session_id.to_string()).await?;

        let mut sessions = self.sessions.write().await;
        let handle = sessions
            .get_mut(session_id)
            .ok_or_else(|| WslGraphicsError::SessionNotFound(session_id.to_string()))?;
        handle.bridge_handle = bridge_handle;
        handle.info.ws_port = ws_port;
        handle.info.ws_token = ws_token;
        Ok(handle.info.clone())
    }

    pub async fn list_sessions(&self) -> Vec<WslGraphicsSession> {
        self.sessions
            .read()
            .await
            .values()
            .map(|handle| handle.info.clone())
            .collect()
    }

    pub async fn detect_wslg(&self, distro: &str) -> Result<crate::WslgStatus, WslGraphicsError> {
        crate::wslg::detect_wslg_async(distro).await
    }

    pub async fn shutdown(&self) {
        let sessions = self.list_session_ids().await;
        for session_id in sessions {
            let _ = self.stop(&session_id).await;
        }
    }

    async fn list_session_ids(&self) -> Vec<String> {
        self.sessions.read().await.keys().cloned().collect()
    }

    async fn spawn_app_exit_watcher(
        &self,
        session_id: String,
        distro: String,
        mut stop_rx: tokio::sync::oneshot::Receiver<()>,
    ) {
        // The watcher takes ownership of app_child like Tauri, so natural app
        // exit can tear down VNC/bridge without holding the registry lock.
        let app_child = {
            let mut sessions = self.sessions.write().await;
            let Some(handle) = sessions.get_mut(&session_id) else {
                return;
            };
            handle.app_child.take()
        };
        let Some(mut app_child) = app_child else {
            return;
        };

        let state = self.clone_handle();
        tokio::spawn(async move {
            tokio::select! {
                status = app_child.wait() => {
                    match status {
                        Ok(exit) => tracing::info!(
                            "WSL Graphics App: process exited for session {} (status: {:?})",
                            session_id,
                            exit
                        ),
                        Err(error) => tracing::warn!(
                            "WSL Graphics App: error waiting for process in session {}: {}",
                            session_id,
                            error
                        ),
                    }
                    let mut sessions = state.sessions.write().await;
                    if let Some(mut handle) = sessions.remove(&session_id) {
                        handle.bridge_handle.abort();
                        let _ = handle.vnc_child.kill().await;
                        if let Some(desktop) = handle.desktop_child.as_mut() {
                            let _ = desktop.kill().await;
                        }
                        drop(sessions);
                        wsl::cleanup_wsl_session(&distro).await;
                    }
                }
                _ = &mut stop_rx => {
                    tracing::info!(
                        "WSL Graphics App: stop signal received for session {}, killing app process",
                        session_id
                    );
                    let _ = app_child.kill().await;
                }
            }
        });
    }

    fn clone_handle(&self) -> WslGraphicsStateHandle {
        WslGraphicsStateHandle {
            sessions: self.sessions.clone(),
        }
    }
}

impl Default for WslGraphicsState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
struct WslGraphicsStateHandle {
    sessions: Arc<RwLock<HashMap<String, WslGraphicsHandle>>>,
}

fn validate_argv(argv: &[String]) -> Result<(), WslGraphicsError> {
    if argv.is_empty() {
        return Err(WslGraphicsError::InvalidAppArgv(
            "argv must contain at least one element (the program name)".to_string(),
        ));
    }
    let program = &argv[0];
    if program.is_empty() {
        return Err(WslGraphicsError::InvalidAppArgv(
            "Program name cannot be empty".to_string(),
        ));
    }

    const FORBIDDEN: &[char] = &[
        ';', '|', '&', '`', '$', '(', ')', '{', '}', '<', '>', '\n', '\r', '\\', '\'', '"', '!',
        '#',
    ];
    for (index, arg) in argv.iter().enumerate() {
        for ch in FORBIDDEN {
            if arg.contains(*ch) {
                return Err(WslGraphicsError::InvalidAppArgv(format!(
                    "argv[{index}] contains forbidden shell metacharacter '{ch}'"
                )));
            }
        }
    }
    if program.contains("..") {
        return Err(WslGraphicsError::InvalidAppArgv(
            "Program name must not contain '..' (path traversal)".to_string(),
        ));
    }
    if program.starts_with("./") || program.starts_with("../") {
        return Err(WslGraphicsError::InvalidAppArgv(
            "Program name must be a bare command or absolute path, not relative".to_string(),
        ));
    }

    let total_len = argv.iter().map(String::len).sum::<usize>();
    if total_len > 4096 {
        return Err(WslGraphicsError::InvalidAppArgv(format!(
            "Total argv length ({total_len}) exceeds limit (4096 bytes)"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_argv_matches_tauri_rejections() {
        assert!(validate_argv(&[]).is_err());
        assert!(validate_argv(&["".to_string()]).is_err());
        assert!(validate_argv(&["gedit".to_string(), "a;b".to_string()]).is_err());
        assert!(validate_argv(&["../gedit".to_string()]).is_err());
        assert!(validate_argv(&["./gedit".to_string()]).is_err());
        assert!(validate_argv(&["gedit".to_string(), "/tmp/file.txt".to_string()]).is_ok());
    }

    #[cfg(not(target_os = "windows"))]
    #[tokio::test]
    async fn detect_wslg_uses_same_non_windows_fallback() {
        let state = WslGraphicsState::new();
        assert_eq!(
            state.detect_wslg("Ubuntu").await.unwrap_err().to_string(),
            crate::WSL_GRAPHICS_UNAVAILABLE
        );
    }
}
