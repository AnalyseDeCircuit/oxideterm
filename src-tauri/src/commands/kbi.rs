//! Keyboard-Interactive Authentication Commands
//!
//! This module provides completely isolated 2FA authentication,
//! **NOT** touching connect_v2 or existing password/key flows.
//!
//! # Design Principles
//! - Strict isolation: separate command entry point
//! - 60s timeout on all waits to prevent deadlocks
//! - Immediate cleanup on failure/cancel
//! - Direct connection only (no proxy chain support in MVP)

use crate::bridge::WsBridge;
use crate::session::{SessionConfig, SessionRegistry};
use crate::ssh::{
    keyboard_interactive::{
        cancel_pending, cleanup_pending, complete_pending, register_pending, KbiCancelRequest,
        KbiPrompt, KbiPromptEvent, KbiRespondRequest, KbiResultEvent, EVENT_KBI_PROMPT,
        EVENT_KBI_RESULT,
    },
    AuthMethod, ClientHandler, SshSession,
};
use russh::client::KeyboardInteractiveAuthResponse;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};
use tracing::{debug, error, info, warn};

/// Timeout for waiting on user input (strict 60s as per requirements)
const KBI_USER_INPUT_TIMEOUT: Duration = Duration::from_secs(60);

/// Timeout for SSH handshake (before authentication)
const KBI_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);

// ============================================================================
// Tauri Commands
// ============================================================================

/// Start a keyboard-interactive (2FA) SSH connection
///
/// This is a completely separate entry point from connect_v2.
/// It establishes connection, runs KBI auth flow with frontend prompts,
/// and on success starts the session.
#[tauri::command]
pub async fn ssh_connect_kbi(
    app: AppHandle,
    registry: State<'_, Arc<SessionRegistry>>,
    host: String,
    port: u16,
    username: String,
    cols: u32,
    rows: u32,
    display_name: Option<String>,
) -> Result<(), String> {
    let auth_flow_id = uuid::Uuid::new_v4().to_string();
    info!(
        "Starting KBI auth flow {} for {}@{}:{}",
        auth_flow_id, username, host, port
    );

    // Run the entire flow, ensuring cleanup on any error
    let result = run_kbi_flow(
        app.clone(),
        registry.inner().clone(),
        auth_flow_id.clone(),
        host.clone(),
        port,
        username.clone(),
        cols,
        rows,
        display_name,
    )
    .await;

    // Always emit result event
    match &result {
        Ok((session_id, ws_port, ws_token)) => {
            info!("KBI auth flow {} succeeded, session: {}", auth_flow_id, session_id);
            let _ = app.emit(
                EVENT_KBI_RESULT,
                KbiResultEvent {
                    auth_flow_id: auth_flow_id.clone(),
                    success: true,
                    error: None,
                    session_id: Some(session_id.clone()),
                    ws_port: Some(*ws_port),
                    ws_token: Some(ws_token.clone()),
                },
            );
        }
        Err(e) => {
            error!("KBI auth flow {} failed: {}", auth_flow_id, e);
            let _ = app.emit(
                EVENT_KBI_RESULT,
                KbiResultEvent {
                    auth_flow_id: auth_flow_id.clone(),
                    success: false,
                    error: Some(e.clone()),
                    session_id: None,
                    ws_port: None,
                    ws_token: None,
                },
            );
        }
    }

    result.map(|_| ())
}

/// Respond to a keyboard-interactive prompt
#[tauri::command]
pub async fn ssh_kbi_respond(request: KbiRespondRequest) -> Result<(), String> {
    debug!(
        "KBI respond for flow {}: {} responses",
        request.auth_flow_id,
        request.responses.len()
    );
    complete_pending(&request.auth_flow_id, request.responses)
        .map_err(|e| e.to_string())
}

/// Cancel a keyboard-interactive authentication
#[tauri::command]
pub async fn ssh_kbi_cancel(request: KbiCancelRequest) -> Result<(), String> {
    warn!("KBI cancel requested for flow {}", request.auth_flow_id);
    cancel_pending(&request.auth_flow_id).map_err(|e| e.to_string())
}

// ============================================================================
// Internal Implementation
// ============================================================================

/// Run the complete KBI authentication flow
///
/// Returns (session_id, ws_port, ws_token) on success
async fn run_kbi_flow(
    app: AppHandle,
    registry: Arc<SessionRegistry>,
    auth_flow_id: String,
    host: String,
    port: u16,
    username: String,
    cols: u32,
    rows: u32,
    display_name: Option<String>,
) -> Result<(String, u16, String), String> {
    // 1. Establish TCP connection and SSH handshake (with timeout)
    let addr = format!("{}:{}", host, port);
    debug!("KBI flow {}: connecting to {}", auth_flow_id, addr);

    let socket_addr = tokio::net::lookup_host(&addr)
        .await
        .map_err(|e| format!("DNS resolution failed: {}", e))?
        .next()
        .ok_or_else(|| format!("No address found for {}", addr))?;

    let stream = tokio::time::timeout(
        KBI_HANDSHAKE_TIMEOUT,
        tokio::net::TcpStream::connect(socket_addr),
    )
    .await
    .map_err(|_| "Connection timeout")?
    .map_err(|e| format!("Connection failed: {}", e))?;

    debug!("KBI flow {}: TCP connected", auth_flow_id);

    // 2. SSH handshake
    let ssh_config = Arc::new(russh::client::Config {
        inactivity_timeout: Some(Duration::from_secs(300)),
        keepalive_interval: Some(Duration::from_secs(30)),
        keepalive_max: 3,
        ..Default::default()
    });

    let handler = ClientHandler::new(host.clone(), port, false);

    let mut handle = tokio::time::timeout(
        KBI_HANDSHAKE_TIMEOUT,
        russh::client::connect_stream(ssh_config, stream, handler),
    )
    .await
    .map_err(|_| "SSH handshake timeout")?
    .map_err(|e| format!("SSH handshake failed: {}", e))?;

    debug!("KBI flow {}: SSH handshake complete", auth_flow_id);

    // 3. Start keyboard-interactive authentication
    // Second parameter is submethods (Option<String>) - pass None for default
    let mut auth_result = handle
        .authenticate_keyboard_interactive_start(&username, None::<String>)
        .await
        .map_err(|e| format!("KBI start failed: {}", e))?;

    debug!("KBI flow {}: KBI started", auth_flow_id);

    // 4. Authentication loop (handle multiple rounds of prompts)
    loop {
        match auth_result {
            KeyboardInteractiveAuthResponse::Success => {
                info!("KBI flow {}: authentication successful", auth_flow_id);
                break;
            }
            KeyboardInteractiveAuthResponse::Failure { .. } => {
                return Err("Authentication rejected by server".to_string());
            }
            KeyboardInteractiveAuthResponse::InfoRequest {
                name,
                instructions,
                prompts,
            } => {
                debug!(
                    "KBI flow {}: InfoRequest with {} prompts",
                    auth_flow_id,
                    prompts.len()
                );

                // Convert prompts
                let prompts_for_frontend: Vec<KbiPrompt> = prompts
                    .iter()
                    .map(|p| KbiPrompt {
                        prompt: p.prompt.clone(),
                        echo: p.echo,
                    })
                    .collect();

                // Register pending request BEFORE emitting event
                let rx = register_pending(auth_flow_id.clone());

                // Emit prompt event to frontend
                app.emit(
                    EVENT_KBI_PROMPT,
                    KbiPromptEvent {
                        auth_flow_id: auth_flow_id.clone(),
                        name,
                        instructions,
                        prompts: prompts_for_frontend,
                    },
                )
                .map_err(|e| format!("Failed to emit prompt event: {}", e))?;

                // Wait for frontend response with STRICT 60s timeout
                let responses = tokio::time::timeout(KBI_USER_INPUT_TIMEOUT, rx)
                    .await
                    .map_err(|_| {
                        cleanup_pending(&auth_flow_id);
                        "Authentication timeout: no response within 60 seconds".to_string()
                    })?
                    .map_err(|_| {
                        cleanup_pending(&auth_flow_id);
                        "Response channel closed".to_string()
                    })?
                    .map_err(|e| {
                        cleanup_pending(&auth_flow_id);
                        e.to_string()
                    })?;

                debug!(
                    "KBI flow {}: got {} responses from frontend",
                    auth_flow_id,
                    responses.len()
                );

                // Continue authentication with responses
                auth_result = handle
                    .authenticate_keyboard_interactive_respond(responses)
                    .await
                    .map_err(|e| format!("KBI respond failed: {}", e))?;
            }
        }
    }

    // 5. Authentication succeeded - create session
    debug!("KBI flow {}: creating session", auth_flow_id);

    let session_config = SessionConfig {
        host: host.clone(),
        port,
        username: username.clone(),
        // Store as KeyboardInteractive for reconnection reference
        // (though reconnect won't auto-retry - user must re-initiate)
        auth: AuthMethod::KeyboardInteractive,
        name: display_name.clone(),
        color: None,
        cols,
        rows,
    };

    // Create session in registry
    let sid = registry
        .create_session(session_config.clone())
        .map_err(|e| format!("Failed to create session: {}", e))?;

    // Start connecting state
    if let Err(e) = registry.start_connecting(&sid) {
        registry.remove(&sid);
        return Err(format!("Failed to start connection: {}", e));
    }

    // Create SSH session from authenticated handle
    let ssh_session = SshSession::new(handle, cols, rows);

    // Request shell with PTY
    let (session_handle, handle_controller) = ssh_session
        .request_shell_extended()
        .await
        .map_err(|e| {
            registry.remove(&sid);
            format!("Failed to open shell: {}", e)
        })?;

    // Get command sender and scroll buffer
    let cmd_tx = session_handle.cmd_tx.clone();
    let scroll_buffer = registry
        .with_session(&sid, |entry| entry.scroll_buffer.clone())
        .ok_or_else(|| {
            registry.remove(&sid);
            "Session not found in registry".to_string()
        })?;

    // Start WebSocket bridge with disconnect tracking
    let (_, ws_port, ws_token, _disconnect_rx) =
        WsBridge::start_extended_with_disconnect(session_handle, scroll_buffer, false)
            .await
            .map_err(|e| {
                registry.remove(&sid);
                format!("Failed to start WebSocket bridge: {}", e)
            })?;

    // Update registry with success
    registry
        .connect_success(&sid, ws_port, cmd_tx, handle_controller)
        .map_err(|e| {
            registry.remove(&sid);
            format!("Failed to update session state: {}", e)
        })?;

    info!(
        "KBI flow {}: session {} created, ws://127.0.0.1:{}",
        auth_flow_id, sid, ws_port
    );

    Ok((sid, ws_port, ws_token))
}
