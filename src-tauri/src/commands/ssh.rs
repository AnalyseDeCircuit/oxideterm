// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! SSH Connection Commands
//!
//! 独立的 SSH 连接管理命令，与终端界面解耦。
//!
//! # 命令列表
//!
//! - `ssh_disconnect` - 断开 SSH 连接
//! - `ssh_list_connections` - 列出所有连接
//! - `ssh_set_keep_alive` - 设置连接保持
//! - `create_terminal` - 为已有连接创建终端
//! - `close_terminal` - 关闭终端（不断开连接）

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, State};
use tracing::{debug, info, warn};

use super::{ForwardingRegistry, HealthRegistry, ProfilerRegistry};
use crate::agent::AgentRegistry;
use crate::bridge::{BridgeManager, WsBridge};
use crate::forwarding::ForwardingManager;
use crate::session::{
    AuthMethod, SessionConfig, SessionInfo, SessionRegistry, parse_terminal_output,
};
use crate::state::BufferConfig;
use crate::sftp::session::SftpRegistry;
use crate::ssh::{
    ConnectionInfo, ConnectionPoolConfig, HostKeyStatus, SshConnectionRegistry, accept_host_key,
    check_host_key, get_host_key_cache,
};

/// 断开 SSH 连接
#[tauri::command]
pub async fn ssh_disconnect(
    connection_id: String,
    connection_registry: State<'_, Arc<SshConnectionRegistry>>,
    sftp_registry: State<'_, Arc<SftpRegistry>>,
    _forwarding_registry: State<'_, Arc<ForwardingRegistry>>,
    session_registry: State<'_, Arc<SessionRegistry>>,
    bridge_manager: State<'_, BridgeManager>,
    health_registry: State<'_, HealthRegistry>,
    profiler_registry: State<'_, ProfilerRegistry>,
    agent_registry: State<'_, Arc<AgentRegistry>>,
) -> Result<(), String> {
    info!("SSH disconnect request: {}", connection_id);

    // 获取关联的 session IDs
    let connection_info = connection_registry
        .get_info(&connection_id)
        .await
        .ok_or_else(|| format!("Connection not found: {}", connection_id))?;

    // 关闭所有关联的终端
    for session_id in &connection_info.terminal_ids {
        // 关闭 WebSocket bridge
        bridge_manager.unregister(session_id);
        // 从 session registry 移除
        session_registry.remove(session_id);
        // 清理 health tracker
        health_registry.remove(session_id);
    }

    // 停止并清理 resource profiler
    profiler_registry.remove(&connection_id);

    // 关闭关联的 SFTP
    if let Some(sftp_session_id) = &connection_info.sftp_session_id {
        sftp_registry.remove(sftp_session_id);
    }

    // 关闭所有关联的端口转发
    for forward_id in &connection_info.forward_ids {
        // ForwardingRegistry 按 session_id 管理，需要找到对应的 session
        // 这里暂时跳过，后续重构 ForwardingRegistry
        let _ = forward_id;
    }

    // 关闭关联的 agent
    agent_registry.remove(&connection_id).await;

    // 断开 SSH 连接
    connection_registry
        .disconnect(&connection_id)
        .await
        .map_err(|e| format!("Failed to disconnect: {}", e))?;

    info!("SSH connection {} disconnected", connection_id);

    Ok(())
}

/// 列出所有 SSH 连接
#[tauri::command]
pub async fn ssh_list_connections(
    connection_registry: State<'_, Arc<SshConnectionRegistry>>,
) -> Result<Vec<ConnectionInfo>, String> {
    Ok(connection_registry.list_connections().await)
}

/// 设置连接保持
#[tauri::command]
pub async fn ssh_set_keep_alive(
    connection_id: String,
    keep_alive: bool,
    connection_registry: State<'_, Arc<SshConnectionRegistry>>,
) -> Result<(), String> {
    connection_registry
        .set_keep_alive(&connection_id, keep_alive)
        .await
        .map_err(|e| format!("Failed to set keep_alive: {}", e))
}

/// 获取连接池配置
#[tauri::command]
pub async fn ssh_get_pool_config(
    connection_registry: State<'_, Arc<SshConnectionRegistry>>,
) -> Result<ConnectionPoolConfig, String> {
    Ok(connection_registry.config().await)
}

/// 设置连接池配置
#[tauri::command]
pub async fn ssh_set_pool_config(
    config: ConnectionPoolConfig,
    connection_registry: State<'_, Arc<SshConnectionRegistry>>,
) -> Result<(), String> {
    connection_registry.set_config(config).await;
    Ok(())
}

/// 获取连接池统计信息
///
/// 返回连接池实时状态，用于监控面板
#[tauri::command]
pub async fn ssh_get_pool_stats(
    connection_registry: State<'_, Arc<SshConnectionRegistry>>,
) -> Result<crate::ssh::ConnectionPoolStats, String> {
    Ok(connection_registry.get_stats().await)
}

// ============================================================================
// 终端创建命令
// ============================================================================

/// 创建终端请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTerminalRequest {
    /// SSH 连接 ID
    pub connection_id: String,
    /// 终端列数
    #[serde(default = "default_cols")]
    pub cols: u32,
    /// 终端行数
    #[serde(default = "default_rows")]
    pub rows: u32,
    /// 缓冲区最大行数
    pub max_buffer_lines: Option<usize>,
    /// 关闭时是否持久化 buffer
    pub save_on_disconnect: Option<bool>,
}

fn default_cols() -> u32 {
    120
}
fn default_rows() -> u32 {
    40
}

/// 创建终端响应
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTerminalResponse {
    /// Session ID
    pub session_id: String,
    /// WebSocket URL
    pub ws_url: String,
    /// WebSocket 端口
    pub port: u16,
    /// WebSocket Token
    pub ws_token: String,
    /// Session 信息
    pub session: SessionInfo,
}

/// 为已有 SSH 连接创建终端
#[tauri::command]
pub async fn create_terminal(
    _app_handle: AppHandle,
    request: CreateTerminalRequest,
    connection_registry: State<'_, Arc<SshConnectionRegistry>>,
    session_registry: State<'_, Arc<SessionRegistry>>,
    forwarding_registry: State<'_, Arc<ForwardingRegistry>>,
) -> Result<CreateTerminalResponse, String> {
    info!(
        "Create terminal request for connection: {}",
        request.connection_id
    );

    // 检查连接状态 - 如果正在重连则拒绝创建
    let connection_info = connection_registry
        .get_info(&request.connection_id)
        .await
        .ok_or_else(|| "Connection not found".to_string())?;

    use crate::ssh::ConnectionState;
    match &connection_info.state {
        ConnectionState::LinkDown => {
            return Err(
                "CONNECTION_RECONNECTING: Connection is down, waiting for reconnect".to_string(),
            );
        }
        ConnectionState::Reconnecting => {
            return Err(
                "CONNECTION_RECONNECTING: Connection is reconnecting, please wait".to_string(),
            );
        }
        ConnectionState::Disconnected => {
            return Err("Connection is disconnected".to_string());
        }
        ConnectionState::Error(e) => {
            return Err(format!("Connection error: {}", e));
        }
        _ => {} // Active, Idle, Connecting are OK
    }

    // 获取 HandleController（增加引用计数）
    let handle_controller = connection_registry
        .acquire(&request.connection_id)
        .await
        .map_err(|e| format!("Failed to acquire connection: {}", e))?;

    let agent_forwarding = connection_registry
        .get_connection(&request.connection_id)
        .map(|entry| entry.config.agent_forwarding)
        .unwrap_or(false);

    // 创建 session 配置
    let config = SessionConfig {
        host: connection_info.host.clone(),
        port: connection_info.port,
        username: connection_info.username.clone(),
        auth: AuthMethod::Agent, // 占位，实际使用已有连接
        name: None,
        color: None,
        cols: request.cols,
        rows: request.rows,
        agent_forwarding,
    };

    // 在 SessionRegistry 创建 session
    let buffer_config = BufferConfig {
        max_lines: request
            .max_buffer_lines
            .unwrap_or(crate::session::scroll_buffer::DEFAULT_MAX_LINES)
            .clamp(5_000, 200_000),
        save_on_disconnect: request.save_on_disconnect.unwrap_or(true),
    };
    let session_id = session_registry
        .create_session_with_buffer(config.clone(), buffer_config)
        .map_err(|e| format!("Failed to create session: {}", e))?;

    // 开始连接
    if let Err(e) = session_registry.start_connecting(&session_id) {
        session_registry.remove(&session_id);
        // 释放连接引用
        let _ = connection_registry.release(&request.connection_id).await;
        return Err(format!("Failed to start connection: {}", e));
    }

    // 通过已有的 HandleController 打开新的 shell channel
    let mut channel = match handle_controller.open_session_channel().await {
        Ok(ch) => ch,
        Err(e) => {
            session_registry.remove(&session_id);
            let conn_reg = connection_registry.inner().clone();
            let conn_id = request.connection_id.clone();

            // 检查是否是连接断开错误
            let err_str = e.to_string().to_lowercase();
            let is_connection_error = err_str.contains("disconnected")
                || err_str.contains("connectfailed")
                || err_str.contains("channel error");

            if is_connection_error {
                // 连接已断开，标记为 LinkDown
                // 🛑 后端禁止自动重连：只广播事件，等待前端指令
                warn!(
                    "Channel open failed, connection {} may be dead: {}",
                    conn_id, e
                );
                let node_emitter = conn_reg.node_emitter(); // Oxide-Next Phase 2
                tokio::spawn(async move {
                    // 先释放引用
                    let _ = conn_reg.release(&conn_id).await;
                    // 标记连接为 LinkDown
                    if let Some(entry) = conn_reg.get_connection(&conn_id) {
                        let current_state = entry.state().await;
                        // 只有当连接还不是 LinkDown/Reconnecting 时才标记
                        if !matches!(
                            current_state,
                            ConnectionState::LinkDown | ConnectionState::Reconnecting
                        ) {
                            entry.set_state(ConnectionState::LinkDown).await;
                            // 发送状态变更事件
                            conn_reg
                                .emit_connection_status_changed(&conn_id, "link_down")
                                .await;

                            // Oxide-Next Phase 2: node:state 事件
                            if let Some(ref emitter) = node_emitter {
                                emitter.emit_state_from_connection(
                                    &conn_id,
                                    &ConnectionState::LinkDown,
                                    "channel open failed",
                                );
                            }

                            // ❌ 已删除: conn_reg.start_reconnect(&conn_id).await;
                            // 后端只广播，前端决定是否重连
                        }
                    }
                });
                return Err(
                    "CONNECTION_LINK_DOWN: Connection lost, waiting for frontend command"
                        .to_string(),
                );
            } else {
                tokio::spawn(async move {
                    let _ = conn_reg.release(&conn_id).await;
                });
                return Err(format!("Failed to open channel: {}", e));
            }
        }
    };

    // Deferred PTY: when cols=0 or rows=0, delay PTY allocation until WebSocket
    // sends the first resize frame with real terminal dimensions. This prevents
    // the shell from starting with wrong dimensions (which breaks async prompt
    // themes like spaceship-zsh that cache cursor positions during init).
    let deferred_pty = request.cols == 0 || request.rows == 0;

    if !deferred_pty {
        // Standard flow: request PTY immediately with provided dimensions
        // Clamp to sane limits to prevent resource abuse
        let clamped_cols = request.cols.clamp(1, 500);
        let clamped_rows = request.rows.clamp(1, 200);
        channel
            .request_pty(
                false,
                "xterm-256color",
                clamped_cols,
                clamped_rows,
                0,
                0,
                crate::ssh::DEFAULT_PTY_MODES,
            )
            .await
            .map_err(|e| {
                session_registry.remove(&session_id);
                let conn_reg = connection_registry.inner().clone();
                let conn_id = request.connection_id.clone();
                tokio::spawn(async move {
                    let _ = conn_reg.release(&conn_id).await;
                });
                format!("Failed to request PTY: {}", e)
            })?;

        // Request agent forwarding if enabled (must be after PTY, before shell)
        if agent_forwarding {
            debug!("Requesting SSH agent forwarding on terminal channel");
            if let Err(e) = channel.agent_forward(true).await {
                warn!("Agent forwarding request failed (non-fatal): {}", e);
            }
        }

        channel.request_shell(false).await.map_err(|e| {
            session_registry.remove(&session_id);
            let conn_reg = connection_registry.inner().clone();
            let conn_id = request.connection_id.clone();
            tokio::spawn(async move {
                let _ = conn_reg.release(&conn_id).await;
            });
            format!("Failed to request shell: {}", e)
        })?;
    }

    // 创建 ExtendedSessionHandle（用于 WsBridge）
    use crate::ssh::{ExtendedSessionHandle, SessionCommand};
    use russh::ChannelMsg;
    use tokio::sync::mpsc;

    let (cmd_tx, mut cmd_rx) = mpsc::channel::<SessionCommand>(1024);

    let scroll_buffer = session_registry
        .with_session(&session_id, |entry| entry.scroll_buffer.clone())
        .ok_or_else(|| "Session not found in registry".to_string())?;

    let output_tx = session_registry
        .with_session(&session_id, |entry| entry.output_tx.clone())
        .ok_or_else(|| "Session output channel not found".to_string())?;

    let output_rx = output_tx.subscribe();
    let scroll_buffer_clone = scroll_buffer.clone();

    // 启动 channel 处理任务
    let sid = session_id.clone();
    tokio::spawn(async move {
        tracing::debug!("Channel handler started for session {}", sid);

        // Deferred PTY: wait for first resize command before allocating PTY + shell.
        // This ensures the shell starts with the correct terminal dimensions,
        // preventing async prompt themes (spaceship-zsh) from caching wrong cursor positions.
        if deferred_pty {
            let fallback_cols: u16 = 120;
            let fallback_rows: u16 = 40;
            let pty_timeout = tokio::time::Duration::from_secs(15);

            let (pty_cols, pty_rows) = tokio::select! {
                cmd = cmd_rx.recv() => {
                    match cmd {
                        Some(SessionCommand::Resize(cols, rows)) => {
                            // Clamp to sane limits to prevent resource abuse
                            (cols.clamp(1, 500), rows.clamp(1, 200))
                        }
                        Some(SessionCommand::Close) => {
                            info!("Close command before PTY allocated for session {}", sid);
                            let _ = channel.eof().await;
                            return;
                        }
                        // Data arrived before resize (unlikely) — use fallback
                        Some(SessionCommand::Data(_)) => {
                            warn!("Data before resize for deferred PTY {}, using fallback", sid);
                            (fallback_cols, fallback_rows)
                        }
                        None => {
                            info!("Command channel closed before PTY allocated for session {}", sid);
                            let _ = channel.eof().await;
                            return;
                        }
                    }
                }
                _ = tokio::time::sleep(pty_timeout) => {
                    warn!(
                        "Deferred PTY timeout for session {}, using fallback {}x{}",
                        sid, fallback_cols, fallback_rows
                    );
                    (fallback_cols, fallback_rows)
                }
            };

            if let Err(e) = channel
                .request_pty(
                    false,
                    "xterm-256color",
                    pty_cols as u32,
                    pty_rows as u32,
                    0,
                    0,
                    crate::ssh::DEFAULT_PTY_MODES,
                )
                .await
            {
                tracing::error!("Failed to request deferred PTY for session {}: {}", sid, e);
                let _ = channel.eof().await;
                return;
            }
            // Request agent forwarding if enabled (must be after PTY, before shell)
            if agent_forwarding {
                debug!("Requesting SSH agent forwarding on deferred PTY channel");
                if let Err(e) = channel.agent_forward(true).await {
                    warn!("Agent forwarding request failed (non-fatal): {}", e);
                }
            }
            if let Err(e) = channel.request_shell(false).await {
                tracing::error!("Failed to request shell for session {}: {}", sid, e);
                let _ = channel.eof().await;
                return;
            }
            info!(
                "Deferred PTY allocated at {}x{} for session {}",
                pty_cols, pty_rows, sid
            );
        }

        let mut pending_resize: Option<(u16, u16)> = None;
        let mut resize_deadline = std::pin::pin!(tokio::time::sleep(tokio::time::Duration::MAX));

        loop {
            tokio::select! {
                Some(cmd) = cmd_rx.recv() => {
                    match cmd {
                        SessionCommand::Data(data) => {
                            if let Err(e) = channel.data(&data[..]).await {
                                tracing::error!("Failed to send data to SSH channel: {}", e);
                                break;
                            }
                        }
                        SessionCommand::Resize(cols, rows) => {
                            let cols = cols.clamp(1, 500);
                            let rows = rows.clamp(1, 200);
                            pending_resize = Some((cols, rows));
                            resize_deadline.as_mut().reset(tokio::time::Instant::now() + tokio::time::Duration::from_millis(100));
                        }
                        SessionCommand::Close => {
                            info!("Close command received for session {}", sid);
                            let _ = channel.eof().await;
                            break;
                        }
                    }
                }

                () = &mut resize_deadline, if pending_resize.is_some() => {
                    if let Some((cols, rows)) = pending_resize.take() {
                        tracing::debug!("Sending debounced window_change: {}x{}", cols, rows);
                        if let Err(e) = channel.window_change(cols as u32, rows as u32, 0, 0).await {
                            tracing::error!("Failed to resize PTY: {}", e);
                        }
                    }
                    // Reset to far future so this branch doesn't fire again
                    resize_deadline.as_mut().reset(tokio::time::Instant::now() + tokio::time::Duration::from_secs(86400));
                }

                Some(msg) = channel.wait() => {
                    match msg {
                        ChannelMsg::Data { data } => {
                            let bytes = data.to_vec();
                            let lines = parse_terminal_output(&bytes);
                            if !lines.is_empty() {
                                scroll_buffer_clone.append_batch(lines).await;
                            }
                            let _ = output_tx.send(bytes);
                        }
                        ChannelMsg::ExtendedData { data, ext } => {
                            if ext == 1 {
                                let bytes = data.to_vec();
                                let lines = parse_terminal_output(&bytes);
                                if !lines.is_empty() {
                                    scroll_buffer_clone.append_batch(lines).await;
                                }
                                let _ = output_tx.send(bytes);
                            }
                        }
                        ChannelMsg::Eof | ChannelMsg::Close => {
                            info!("SSH channel closed for session {}", sid);
                            break;
                        }
                        _ => {}
                    }
                }

                else => break,
            }
        }

        tracing::debug!("Channel handler terminated for session {}", sid);
    });

    let extended_handle = ExtendedSessionHandle {
        id: session_id.clone(),
        cmd_tx: cmd_tx.clone(),
        stdout_rx: output_rx,
    };

    // 启动 WebSocket bridge
    let (_, port, token, disconnect_rx) =
        WsBridge::start_extended_with_disconnect(extended_handle, scroll_buffer.clone(), false)
            .await
            .map_err(|e| {
                session_registry.remove(&session_id);
                let conn_reg = connection_registry.inner().clone();
                let conn_id = request.connection_id.clone();
                tokio::spawn(async move {
                    let _ = conn_reg.release(&conn_id).await;
                });
                format!("Failed to start WebSocket bridge: {}", e)
            })?;

    // 处理 WebSocket 断开事件
    // Note: connection_status_changed events are emitted by heartbeat monitoring
    // Important: 不要在这里移除 terminal_id 或释放连接，因为重连时需要这些信息
    let session_id_clone = session_id.clone();
    let registry_clone = session_registry.inner().clone();
    let conn_registry_clone = connection_registry.inner().clone();
    let conn_id_clone = request.connection_id.clone();
    let my_ws_port = port; // Capture this bridge's port for affinity check
    tokio::spawn(async move {
        if let Ok(reason) = disconnect_rx.await {
            warn!(
                "Session {} WebSocket bridge disconnected: {:?}",
                session_id_clone, reason
            );
            if reason.is_recoverable() {
                // 🔧 修复 ref_count 泄漏：超时后释放连接引用
                let conn_reg_for_cleanup = conn_registry_clone.clone();
                let session_id_for_cleanup = session_id_clone.clone();
                let _ = registry_clone.mark_ws_detached_with_cleanup(
                    &session_id_clone,
                    Duration::from_secs(300),
                    Some(move |conn_id: String| {
                        // 在 TTL 过期时释放连接池引用
                        let conn_reg = conn_reg_for_cleanup;
                        let sid = session_id_for_cleanup;
                        tokio::spawn(async move {
                            info!(
                                "Releasing connection {} ref after WS detach timeout (session: {})",
                                conn_id, sid
                            );
                            let _ = conn_reg.remove_terminal(&conn_id, &sid).await;
                            let _ = conn_reg.release(&conn_id).await;
                        });
                    }),
                );
            } else {
                // AcceptTimeout 或其他不可恢复的断开：清理会话
                // 这是因为如果前端从未连接，保留这个会话没有意义
                if matches!(reason, crate::bridge::DisconnectReason::AcceptTimeout) {
                    // 代际校验：检查 session 当前的 ws_port 是否仍属于本 bridge。
                    // 如果 recreateTerminalPty 已经创建了新 bridge 并更新了 ws_port，
                    // 则本 bridge 的清理不应删除已被新 bridge 接管的 session。
                    let current_port = registry_clone
                        .with_session(&session_id_clone, |entry| entry.ws_port)
                        .flatten();
                    if current_port != Some(my_ws_port) {
                        info!(
                            "Session {} AcceptTimeout from stale bridge (port {}), \
                             session now on port {:?} — skipping cleanup",
                            session_id_clone, my_ws_port, current_port
                        );
                    } else {
                        warn!(
                            "Session {} WS accept timeout, removing from registries",
                            session_id_clone
                        );

                        // 从连接的终端列表中移除
                        let _ = conn_registry_clone
                            .remove_terminal(&conn_id_clone, &session_id_clone)
                            .await;
                        // 释放连接引用（release 会正确设置连接状态为 Idle 并发送 node:state）
                        let _ = conn_registry_clone.release(&conn_id_clone).await;
                        // 完全移除会话
                        let _ = registry_clone.disconnect_complete(&session_id_clone, true);
                    }
                } else {
                    // 其他不可恢复的断开：只更新状态，不移除
                    // 终端关联由 close_terminal 命令显式移除
                    let _ = registry_clone.disconnect_complete(&session_id_clone, false);
                }
            }
        }
    });

    // 克隆 HandleController 用于 ForwardingManager
    let forwarding_controller = handle_controller.clone();

    // 更新 session registry
    session_registry
        .connect_success_with_connection(
            &session_id,
            port,
            token.clone(),
            cmd_tx,
            handle_controller,
            request.connection_id.clone(),
        )
        .map_err(|e| {
            session_registry.remove(&session_id);
            let conn_reg = connection_registry.inner().clone();
            let conn_id = request.connection_id.clone();
            tokio::spawn(async move {
                let _ = conn_reg.release(&conn_id).await;
            });
            format!("Failed to update session state: {}", e)
        })?;

    // 记录终端关联
    let _ = connection_registry
        .add_terminal(&request.connection_id, session_id.clone())
        .await;

    // 注册 ForwardingManager
    let forwarding_manager = ForwardingManager::new(forwarding_controller, session_id.clone());
    forwarding_registry
        .register(session_id.clone(), forwarding_manager)
        .await;

    let ws_url = format!("ws://localhost:{}", port);
    let session_info = session_registry
        .get(&session_id)
        .ok_or_else(|| "Session disappeared".to_string())?;

    info!(
        "Terminal created: session={}, ws_port={}, connection={}",
        session_id, port, request.connection_id
    );

    Ok(CreateTerminalResponse {
        session_id,
        ws_url,
        port,
        ws_token: token,
        session: session_info,
    })
}

/// 关闭终端（不断开 SSH 连接）
#[tauri::command]
pub async fn close_terminal(
    session_id: String,
    session_registry: State<'_, Arc<SessionRegistry>>,
    connection_registry: State<'_, Arc<SshConnectionRegistry>>,
    bridge_manager: State<'_, BridgeManager>,
    sftp_registry: State<'_, Arc<SftpRegistry>>,
    forwarding_registry: State<'_, Arc<ForwardingRegistry>>,
    health_registry: State<'_, HealthRegistry>,
) -> Result<(), String> {
    info!("Close terminal request: {}", session_id);

    // 获取关联的 connection_id
    let connection_id = session_registry
        .with_session(&session_id, |entry| entry.connection_id.clone())
        .flatten();

    // 保存终端缓冲区
    if let Err(e) = session_registry
        .persist_session_with_buffer(&session_id)
        .await
    {
        tracing::warn!("Failed to persist session buffer: {}", e);
    }

    // 停止端口转发
    forwarding_registry.remove(&session_id).await;

    // 关闭 session
    session_registry.close_session(&session_id).await?;

    // 完成断开
    let _ = session_registry.disconnect_complete(&session_id, true);

    // 注销 bridge
    bridge_manager.unregister(&session_id);

    // 移除 SFTP
    sftp_registry.remove(&session_id);

    // 清理 health tracker
    // Note: profiler is per-connection (not per-session), cleaned up by disconnect_ssh
    health_registry.remove(&session_id);

    // 释放连接引用
    if let Some(connection_id) = connection_id {
        // 从连接中移除终端关联
        let _ = connection_registry
            .remove_terminal(&connection_id, &session_id)
            .await;
        // 释放引用计数
        let _ = connection_registry.release(&connection_id).await;
    }

    info!("Terminal {} closed", session_id);

    Ok(())
}

/// 重建终端 PTY（用于连接重连后恢复 Shell）
///
/// 当物理连接重连成功后，前端调用此命令为每个关联的 session 重建 PTY。
/// 这会创建新的 shell channel 和 WebSocket bridge，并返回新的 ws_url 和 ws_token。
#[tauri::command]
pub async fn recreate_terminal_pty(
    _app_handle: AppHandle,
    session_id: String,
    connection_registry: State<'_, Arc<SshConnectionRegistry>>,
    session_registry: State<'_, Arc<SessionRegistry>>,
    _forwarding_registry: State<'_, Arc<ForwardingRegistry>>,
) -> Result<RecreateTerminalResponse, String> {
    info!("Recreate terminal PTY request: {}", session_id);

    // === Fast path: reuse existing PTY if WS was detached ===
    if session_registry.is_ws_detached(&session_id) {
        if let (Some(cmd_tx), Some(output_tx)) = (
            session_registry.get_cmd_tx(&session_id),
            session_registry.get_output_tx(&session_id),
        ) {
            let handle_controller = session_registry
                .get_handle_controller(&session_id)
                .ok_or_else(|| "Session handle controller not found".to_string())?;

            let scroll_buffer = session_registry
                .with_session(&session_id, |entry| entry.scroll_buffer.clone())
                .ok_or_else(|| "Session not found in registry".to_string())?;

            let extended_handle = ExtendedSessionHandle {
                id: session_id.clone(),
                cmd_tx: cmd_tx.clone(),
                stdout_rx: output_tx.subscribe(),
            };

            // replay_on_connect = true — key-driven remount (ws_url change → new React key)
            // destroys the old xterm buffer, so the new component needs the backend
            // to replay the last N lines to fill the fresh terminal.
            let (_, port, token, disconnect_rx) =
                WsBridge::start_extended_with_disconnect(extended_handle, scroll_buffer, true)
                    .await
                    .map_err(|e| format!("Failed to start WebSocket bridge: {}", e))?;

            session_registry
                .update_ws_info(&session_id, port, token.clone(), cmd_tx, handle_controller)
                .map_err(|e| format!("Failed to update session: {}", e))?;

            let ws_url = format!("ws://localhost:{}", port);

            // Bug fix: Monitor disconnect_rx to handle WS bridge lifecycle.
            // Previously _disconnect_rx was dropped, leaking the cleanup chain.
            let session_id_for_monitor = session_id.clone();
            let registry_for_monitor = session_registry.inner().clone();
            let conn_registry_for_monitor = connection_registry.inner().clone();
            let conn_id_for_monitor = session_registry
                .with_session(&session_id, |entry| entry.connection_id.clone())
                .flatten()
                .unwrap_or_default();
            tokio::spawn(async move {
                if let Ok(reason) = disconnect_rx.await {
                    warn!(
                        "Reattached session {} WS bridge disconnected: {:?}",
                        session_id_for_monitor, reason
                    );
                    if reason.is_recoverable() {
                        let conn_reg = conn_registry_for_monitor.clone();
                        let sid = session_id_for_monitor.clone();
                        let cid = conn_id_for_monitor.clone();
                        let _ = registry_for_monitor.mark_ws_detached_with_cleanup(
                            &session_id_for_monitor,
                            std::time::Duration::from_secs(300),
                            Some(move |_conn_id: String| {
                                tokio::spawn(async move {
                                    info!(
                                        "Releasing connection {} ref after reattach WS detach timeout (session: {})",
                                        cid, sid
                                    );
                                    let _ = conn_reg.remove_terminal(&cid, &sid).await;
                                    let _ = conn_reg.release(&cid).await;
                                });
                            }),
                        );
                    } else if matches!(reason, crate::bridge::DisconnectReason::AcceptTimeout) {
                        // 代际校验：确保本 bridge 仍拥有该 session
                        let current_port = registry_for_monitor
                            .with_session(&session_id_for_monitor, |entry| entry.ws_port)
                            .flatten();
                        if current_port != Some(port) {
                            info!(
                                "Reattached session {} AcceptTimeout from stale bridge (port {}), \
                                 session now on port {:?} — skipping cleanup",
                                session_id_for_monitor, port, current_port
                            );
                        } else {
                            warn!(
                                "Reattached session {} WS accept timeout, cleaning up",
                                session_id_for_monitor
                            );
                            if !conn_id_for_monitor.is_empty() {
                                let _ = conn_registry_for_monitor
                                    .remove_terminal(&conn_id_for_monitor, &session_id_for_monitor)
                                    .await;
                                let _ = conn_registry_for_monitor
                                    .release(&conn_id_for_monitor)
                                    .await;
                            }
                            let _ = registry_for_monitor
                                .disconnect_complete(&session_id_for_monitor, true);
                        }
                    } else {
                        let _ = registry_for_monitor
                            .disconnect_complete(&session_id_for_monitor, false);
                    }
                }
            });

            info!(
                "Terminal WS reattached: session={}, ws_port={}",
                session_id, port
            );

            return Ok(RecreateTerminalResponse {
                session_id,
                ws_url,
                port,
                ws_token: token,
            });
        }
    }

    // 获取 session 信息
    let session_info = session_registry
        .get(&session_id)
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    let connection_id = session_info
        .connection_id
        .ok_or_else(|| "Session has no connection_id".to_string())?;

    // 获取新的 HandleController
    let handle_controller = connection_registry
        .get_handle_controller(&connection_id)
        .ok_or_else(|| "Connection not found".to_string())?;

    // 获取 session 配置
    let config = session_registry
        .get_config(&session_id)
        .ok_or_else(|| "Session config not found".to_string())?;

    // 打开新的 shell channel
    let mut channel = handle_controller
        .open_session_channel()
        .await
        .map_err(|e| format!("Failed to open channel: {}", e))?;

    // 请求 PTY
    channel
        .request_pty(
            false,
            "xterm-256color",
            config.cols,
            config.rows,
            0,
            0,
            crate::ssh::DEFAULT_PTY_MODES,
        )
        .await
        .map_err(|e| format!("Failed to request PTY: {}", e))?;

    // 请求 shell
    channel
        .request_shell(false)
        .await
        .map_err(|e| format!("Failed to request shell: {}", e))?;

    // 创建新的 channel handler
    use crate::ssh::{ExtendedSessionHandle, SessionCommand};
    use russh::ChannelMsg;
    use tokio::sync::mpsc;

    let (cmd_tx, mut cmd_rx) = mpsc::channel::<SessionCommand>(1024);

    let scroll_buffer = session_registry
        .with_session(&session_id, |entry| entry.scroll_buffer.clone())
        .ok_or_else(|| "Session not found in registry".to_string())?;

    let output_tx = session_registry
        .with_session(&session_id, |entry| entry.output_tx.clone())
        .ok_or_else(|| "Session output channel not found".to_string())?;

    let output_rx = output_tx.subscribe();
    let scroll_buffer_clone = scroll_buffer.clone();

    let sid = session_id.clone();
    tokio::spawn(async move {
        tracing::debug!("Recreated channel handler started for session {}", sid);

        loop {
            tokio::select! {
                Some(cmd) = cmd_rx.recv() => {
                    match cmd {
                        SessionCommand::Data(data) => {
                            if let Err(e) = channel.data(&data[..]).await {
                                tracing::error!("Failed to send data to SSH channel: {}", e);
                                break;
                            }
                        }
                        SessionCommand::Resize(cols, rows) => {
                            if let Err(e) = channel.window_change(cols as u32, rows as u32, 0, 0).await {
                                tracing::error!("Failed to resize PTY: {}", e);
                            }
                        }
                        SessionCommand::Close => {
                            let _ = channel.eof().await;
                            break;
                        }
                    }
                }

                Some(msg) = channel.wait() => {
                    match msg {
                        ChannelMsg::Data { data } => {
                            let bytes = data.to_vec();
                            let lines = parse_terminal_output(&bytes);
                            if !lines.is_empty() {
                                scroll_buffer_clone.append_batch(lines).await;
                            }
                            let _ = output_tx.send(bytes);
                        }
                        ChannelMsg::ExtendedData { data, ext } => {
                            if ext == 1 {
                                let bytes = data.to_vec();
                                let lines = parse_terminal_output(&bytes);
                                if !lines.is_empty() {
                                    scroll_buffer_clone.append_batch(lines).await;
                                }
                                let _ = output_tx.send(bytes);
                            }
                        }
                        ChannelMsg::Eof | ChannelMsg::Close => {
                            break;
                        }
                        _ => {}
                    }
                }

                else => break,
            }
        }

        tracing::debug!("Recreated channel handler terminated for session {}", sid);
    });

    let extended_handle = ExtendedSessionHandle {
        id: session_id.clone(),
        cmd_tx: cmd_tx.clone(),
        stdout_rx: output_rx,
    };

    // 启动新的 WebSocket bridge
    // replay_on_connect = true — key-driven remount destroys old xterm buffer,
    // new component needs backend to replay last N lines.
    let (_, port, token, disconnect_rx) =
        WsBridge::start_extended_with_disconnect(extended_handle, scroll_buffer, true)
            .await
            .map_err(|e| format!("Failed to start WebSocket bridge: {}", e))?;

    // 处理 WebSocket 断开事件
    // Note: connection_status_changed events are emitted by heartbeat monitoring
    // Important: 不要在这里移除 terminal_id 或释放连接，因为重连时需要这些信息
    let session_id_clone = session_id.clone();
    let registry_clone = session_registry.inner().clone();
    let conn_registry_clone = connection_registry.inner().clone();
    let conn_id_clone = connection_id.clone();
    let my_ws_port = port; // Capture this bridge's port for affinity check
    tokio::spawn(async move {
        if let Ok(reason) = disconnect_rx.await {
            warn!(
                "Recreated session {} WebSocket bridge disconnected: {:?}",
                session_id_clone, reason
            );
            if reason.is_recoverable() {
                // 🔧 修复 ref_count 泄漏：超时后释放连接引用
                let conn_reg_for_cleanup = conn_registry_clone.clone();
                let session_id_for_cleanup = session_id_clone.clone();
                let _ = registry_clone.mark_ws_detached_with_cleanup(
                    &session_id_clone,
                    Duration::from_secs(300),
                    Some(move |conn_id: String| {
                        let conn_reg = conn_reg_for_cleanup;
                        let sid = session_id_for_cleanup;
                        tokio::spawn(async move {
                            info!("Releasing connection {} ref after recreated WS detach timeout (session: {})", conn_id, sid);
                            let _ = conn_reg.remove_terminal(&conn_id, &sid).await;
                            let _ = conn_reg.release(&conn_id).await;
                        });
                    }),
                );
            } else {
                // AcceptTimeout: 前端没有连接，清理会话
                if matches!(reason, crate::bridge::DisconnectReason::AcceptTimeout) {
                    // 代际校验：确保本 bridge 仍拥有该 session
                    let current_port = registry_clone
                        .with_session(&session_id_clone, |entry| entry.ws_port)
                        .flatten();
                    if current_port != Some(my_ws_port) {
                        info!(
                            "Recreated session {} AcceptTimeout from stale bridge (port {}), \
                             session now on port {:?} — skipping cleanup",
                            session_id_clone, my_ws_port, current_port
                        );
                    } else {
                        warn!(
                            "Recreated session {} WS accept timeout, removing from registries",
                            session_id_clone
                        );
                        let _ = conn_registry_clone
                            .remove_terminal(&conn_id_clone, &session_id_clone)
                            .await;
                        let _ = conn_registry_clone.release(&conn_id_clone).await;
                        let _ = registry_clone.disconnect_complete(&session_id_clone, true);
                    }
                } else {
                    // 其他不可恢复的断开：只更新状态
                    let _ = registry_clone.disconnect_complete(&session_id_clone, false);
                }
            }
        }
    });

    // 更新 session registry 的 ws_port 和 ws_token
    session_registry
        .update_ws_info(
            &session_id,
            port,
            token.clone(),
            cmd_tx,
            handle_controller.clone(),
        )
        .map_err(|e| format!("Failed to update session: {}", e))?;

    let ws_url = format!("ws://localhost:{}", port);

    info!(
        "Terminal PTY recreated: session={}, ws_port={}, connection={}",
        session_id, port, connection_id
    );

    Ok(RecreateTerminalResponse {
        session_id,
        ws_url,
        port,
        ws_token: token,
    })
}

/// 重建终端 PTY 的响应
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecreateTerminalResponse {
    pub session_id: String,
    pub ws_url: String,
    pub port: u16,
    pub ws_token: String,
}

// ============================================================================
// SSH Host Key Preflight (TOFU - Trust On First Use)
// ============================================================================

/// Preflight timeout (shorter than full connection)
const PREFLIGHT_TIMEOUT_SECS: u64 = 10;

/// SSH 主机密钥预检查请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshPreflightRequest {
    pub host: String,
    pub port: u16,
}

/// SSH 主机密钥预检查响应
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshPreflightResponse {
    /// Host key verification status
    #[serde(flatten)]
    pub status: HostKeyStatus,
}

/// 预检查 SSH 主机密钥（TOFU 模式）
///
/// 在建立完整连接前，先检查主机密钥状态：
/// - `Verified`: 主机密钥已在 known_hosts 中验证通过
/// - `Unknown`: 首次连接，需要用户确认指纹
/// - `Changed`: 主机密钥已变更，可能是 MITM 攻击！
/// - `Error`: 连接错误
///
/// 前端根据返回状态决定是否显示确认对话框。
#[tauri::command]
pub async fn ssh_preflight(request: SshPreflightRequest) -> Result<SshPreflightResponse, String> {
    info!("SSH preflight check: {}:{}", request.host, request.port);

    let status = check_host_key(&request.host, request.port, PREFLIGHT_TIMEOUT_SECS).await;

    Ok(SshPreflightResponse { status })
}

/// 接受主机密钥请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptHostKeyRequest {
    pub host: String,
    pub port: u16,
    /// SHA256 fingerprint to accept
    pub fingerprint: String,
    /// Whether to persist to known_hosts (true) or trust for this session only (false)
    pub persist: bool,
}

/// 接受 SSH 主机密钥
///
/// 用户在确认对话框中选择信任后调用此命令。
/// - `persist=true`: 保存到 ~/.ssh/known_hosts（永久信任）
/// - `persist=false`: 仅本次会话信任（内存缓存）
///
/// 注意：实际保存到 known_hosts 发生在后续 ssh_connect 时，
/// 因为我们需要完整的公钥数据（不仅仅是指纹）。
#[tauri::command]
pub async fn ssh_accept_host_key(request: AcceptHostKeyRequest) -> Result<(), String> {
    info!(
        "Accepting host key for {}:{} (persist={})",
        request.host, request.port, request.persist
    );

    // Mark as trusted in memory cache
    accept_host_key(&request.host, request.port, &request.fingerprint)
        .map_err(|e| format!("Failed to accept host key: {}", e))?;

    // Note: If persist=true, the actual save to known_hosts happens during
    // the real ssh_connect call when we have the full public key.
    // We store a flag in the cache to indicate this should be persisted.
    if request.persist {
        // The cache entry already marks this as trusted.
        // The ssh_connect flow will check this and save to known_hosts.
        info!("Host key will be saved to known_hosts on next connection");
    }

    Ok(())
}

/// 清除主机密钥缓存（用于测试或强制重新验证）
#[tauri::command]
pub async fn ssh_clear_host_key_cache() -> Result<(), String> {
    info!("Clearing host key cache");
    get_host_key_cache().clear();
    Ok(())
}

/// 获取已检测的远程环境信息
///
/// 返回 SSH 连接的远程操作系统、架构、shell 等信息。
/// 如果检测尚未完成，返回 `None`。
///
/// # Arguments
/// * `connection_id` - SSH 连接 ID
///
/// # Returns
/// * `Ok(Some(RemoteEnvInfo))` - 检测完成
/// * `Ok(None)` - 检测尚未完成或检测失败
/// * `Err(String)` - 连接不存在
#[tauri::command]
pub async fn get_remote_env(
    connection_id: String,
    connection_registry: State<'_, Arc<SshConnectionRegistry>>,
) -> Result<Option<crate::session::RemoteEnvInfo>, String> {
    let entry = connection_registry
        .get_connection(&connection_id)
        .ok_or_else(|| format!("Connection not found: {}", connection_id))?;

    let env = entry.remote_env();
    Ok(env)
}
