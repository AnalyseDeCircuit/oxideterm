//! WebSocket Server for SSH bridge with Wire Protocol support

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use subtle::ConstantTimeEq;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tracing::{debug, error, info, warn};

use super::protocol::{data_frame, error_frame, heartbeat_frame, Frame, FrameCodec};
use crate::session::{parse_terminal_output, ScrollBuffer};
use crate::ssh::{
    ExtendedSessionHandle as SshExtendedSessionHandle, SessionCommand, SessionHandle,
};

/// Heartbeat interval (seconds)
const HEARTBEAT_INTERVAL_SECS: u64 = 30;
/// Heartbeat timeout - consider connection dead if no response (seconds)
const HEARTBEAT_TIMEOUT_SECS: u64 = 90;
/// WebSocket accept timeout (seconds)
/// Extended to 60s to handle font loading and multiple concurrent terminals
const WS_ACCEPT_TIMEOUT_SECS: u64 = 60;
/// Number of lines to replay after reconnect
const REPLAY_LINE_COUNT: usize = 50;
/// Token validity window (seconds) - tokens older than this are rejected
/// Extended to 300s (5 min) to handle high-latency networks and system load
const TOKEN_VALIDITY_SECS: u64 = 300;

/// Frame channel capacity - larger on Windows due to slower I/O throughput
/// Windows: 16384 to compensate for higher syscall overhead
/// Unix: 4096 as baseline
#[cfg(target_os = "windows")]
const FRAME_CHANNEL_CAPACITY: usize = 16384;
#[cfg(not(target_os = "windows"))]
const FRAME_CHANNEL_CAPACITY: usize = 4096;

async fn build_replay_frame(scroll_buffer: Arc<ScrollBuffer>) -> Result<Vec<u8>, String> {
    let lines = scroll_buffer.tail_lines(REPLAY_LINE_COUNT).await;
    if lines.is_empty() {
        return Ok(Vec::new());
    }

    let mut text = String::new();
    for (idx, line) in lines.iter().enumerate() {
        if idx > 0 {
            text.push_str("\r\n");
        }
        text.push_str(&line.text);
    }
    text.push_str("\r\n");

    let frame = data_frame(Bytes::from(text.into_bytes())).encode();
    Ok(frame.to_vec())
}

/// Get current unix timestamp in seconds
fn unix_timestamp_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Token structure: 32 bytes random + 8 bytes timestamp
const TOKEN_RANDOM_LEN: usize = 32;
const TOKEN_TIMESTAMP_LEN: usize = 8;
const TOKEN_TOTAL_LEN: usize = TOKEN_RANDOM_LEN + TOKEN_TIMESTAMP_LEN; // 40 bytes

/// Generate a cryptographically secure authentication token
/// 
/// Format: Base64(random[32] || timestamp[8])
/// - 32 bytes of OS-level randomness (256 bits entropy)
/// - 8 bytes big-endian timestamp (hidden in encoding)
/// - Output: 54 character URL-safe Base64 string
/// 
/// The token appears opaque to external observers - no visible structure.
fn generate_token() -> String {
    let mut data = [0u8; TOKEN_TOTAL_LEN];
    // Fill first 32 bytes with cryptographically secure random data
    rand::rngs::OsRng.fill_bytes(&mut data[..TOKEN_RANDOM_LEN]);
    // Append timestamp as big-endian bytes
    data[TOKEN_RANDOM_LEN..].copy_from_slice(&unix_timestamp_secs().to_be_bytes());
    URL_SAFE_NO_PAD.encode(data)
}

/// Validate token with expiration check
/// 
/// Decodes both tokens, performs constant-time comparison on the random
/// portion, then checks timestamp expiration from the expected token.
/// 
/// Returns true if token matches and has not expired.
fn validate_token(received: &str, expected: &str) -> bool {
    let received_trimmed = received.trim();

    // Quick length check (doesn't leak token content)
    if received_trimmed.len() != expected.len() {
        return false;
    }

    // Decode both tokens
    let received_bytes = match URL_SAFE_NO_PAD.decode(received_trimmed) {
        Ok(bytes) if bytes.len() == TOKEN_TOTAL_LEN => bytes,
        _ => {
            warn!("Token validation failed: invalid Base64 or wrong length");
            return false;
        }
    };

    let expected_bytes = match URL_SAFE_NO_PAD.decode(expected) {
        Ok(bytes) if bytes.len() == TOKEN_TOTAL_LEN => bytes,
        _ => {
            warn!("Token validation failed: expected token malformed");
            return false;
        }
    };

    // Constant-time comparison of random portion (first 32 bytes)
    let random_matches = bool::from(
        received_bytes[..TOKEN_RANDOM_LEN].ct_eq(&expected_bytes[..TOKEN_RANDOM_LEN])
    );

    if !random_matches {
        return false;
    }

    // Extract timestamp from expected token and check expiration
    let timestamp_bytes: [u8; 8] = expected_bytes[TOKEN_RANDOM_LEN..]
        .try_into()
        .expect("timestamp slice length verified above");
    let created_at = u64::from_be_bytes(timestamp_bytes);

    let now = unix_timestamp_secs();
    let age = now.saturating_sub(created_at);
    if age > TOKEN_VALIDITY_SECS {
        warn!(
            "Token expired: age {} seconds exceeds limit {} seconds",
            age, TOKEN_VALIDITY_SECS
        );
        return false;
    }

    true
}

/// Reason for WebSocket disconnection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DisconnectReason {
    /// Client closed the connection normally
    ClientClosed,
    /// Heartbeat timeout - no response from client
    HeartbeatTimeout,
    /// SSH channel closed (remote side)
    SshChannelClosed,
    /// Network error
    NetworkError(String),
    /// Client never connected (accept timeout)
    AcceptTimeout,
    /// Authentication failed
    AuthFailed,
}

impl DisconnectReason {
    /// Check if the disconnection is recoverable (should trigger reconnect)
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            DisconnectReason::HeartbeatTimeout
                | DisconnectReason::NetworkError(_)
                | DisconnectReason::SshChannelClosed
        )
    }

    /// Get a human-readable description
    pub fn description(&self) -> String {
        match self {
            DisconnectReason::ClientClosed => "Client closed connection".to_string(),
            DisconnectReason::HeartbeatTimeout => "Heartbeat timeout".to_string(),
            DisconnectReason::SshChannelClosed => "SSH channel closed".to_string(),
            DisconnectReason::NetworkError(e) => format!("Network error: {}", e),
            DisconnectReason::AcceptTimeout => "Connection accept timeout".to_string(),
            DisconnectReason::AuthFailed => "Authentication failed".to_string(),
        }
    }
}

/// Shared state for a connection
struct ConnectionState {
    /// Last activity timestamp (unix millis)
    last_seen: AtomicU64,
    /// Heartbeat sequence counter
    heartbeat_seq: AtomicU32,
}

impl ConnectionState {
    fn new() -> Self {
        Self {
            last_seen: AtomicU64::new(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
            ),
            heartbeat_seq: AtomicU32::new(0),
        }
    }

    fn touch(&self) {
        self.last_seen.store(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            Ordering::SeqCst,
        );
    }

    fn next_seq(&self) -> u32 {
        self.heartbeat_seq.fetch_add(1, Ordering::SeqCst)
    }

    fn last_seen_millis(&self) -> u64 {
        self.last_seen.load(Ordering::SeqCst)
    }
}

/// Channel for sending resize events back to SSH
pub type ResizeTx = mpsc::Sender<(u16, u16)>;
pub type ResizeRx = mpsc::Receiver<(u16, u16)>;

/// Extended session handle with resize channel
pub struct ExtendedSessionHandle {
    pub handle: SessionHandle,
    pub resize_tx: ResizeTx,
}

/// WebSocket Bridge server
pub struct WsBridge;

impl WsBridge {
    /// Start a new WebSocket bridge for an SSH session
    /// Returns the port number the WS server is listening on
    pub async fn start(
        session_handle: SessionHandle,
        scroll_buffer: Arc<ScrollBuffer>,
    ) -> Result<(String, u16, String), String> {
        // Generate time-bound authentication token to prevent local process hijacking
        let token = generate_token();

        // Bind to localhost (not 127.0.0.1) to avoid macOS sandbox issues with WebView
        // Using port 0 lets the OS assign an available port
        let listener = TcpListener::bind("localhost:0")
            .await
            .map_err(|e| format!("Failed to bind WebSocket server: {}", e))?;

        let addr = listener
            .local_addr()
            .map_err(|e| format!("Failed to get local address: {}", e))?;

        let port = addr.port();
        let session_id = session_handle.id.clone();

        info!(
            "WebSocket bridge started on port {} for session {} with token auth",
            port, session_id
        );

        // Create a oneshot channel to signal when server is ready to accept
        let (ready_tx, ready_rx) = oneshot::channel::<()>();

        // Spawn the server task with token validation
        let token_clone = token.clone();
        tokio::spawn(Self::run_server(
            listener,
            session_handle,
            ready_tx,
            token_clone,
            scroll_buffer,
        ));

        // Wait for server to be ready (with timeout)
        let _ = tokio::time::timeout(Duration::from_millis(500), ready_rx).await;

        Ok((session_id, port, token))
    }

    /// Start with resize channel support
    pub async fn start_with_resize(
        session_handle: SessionHandle,
        scroll_buffer: Arc<ScrollBuffer>,
    ) -> Result<(String, u16, String, ResizeRx), String> {
        // Generate time-bound authentication token to prevent local process hijacking
        let token = generate_token();

        let (resize_tx, resize_rx) = mpsc::channel::<(u16, u16)>(32);

        let listener = TcpListener::bind("localhost:0")
            .await
            .map_err(|e| format!("Failed to bind WebSocket server: {}", e))?;

        let addr = listener
            .local_addr()
            .map_err(|e| format!("Failed to get local address: {}", e))?;

        let port = addr.port();
        let session_id = session_handle.id.clone();

        info!(
            "WebSocket bridge (with resize) started on port {} for session {} with token auth",
            port, session_id
        );

        let (ready_tx, ready_rx) = oneshot::channel::<()>();

        let extended = ExtendedSessionHandle {
            handle: session_handle,
            resize_tx,
        };

        let token_clone = token.clone();
        tokio::spawn(Self::run_server_extended(
            listener,
            extended,
            ready_tx,
            token_clone,
            scroll_buffer,
        ));

        let _ = tokio::time::timeout(Duration::from_millis(500), ready_rx).await;

        Ok((session_id, port, token, resize_rx))
    }

    /// Start bridge for ExtendedSessionHandle (with command channel)
    /// This is the v2 API that works with SessionRegistry
    pub async fn start_extended(
        session_handle: SshExtendedSessionHandle,
        scroll_buffer: Arc<ScrollBuffer>,
        replay_on_connect: bool,
    ) -> Result<(String, u16, String), String> {
        // Generate time-bound authentication token to prevent local process hijacking
        let token = generate_token();

        let listener = TcpListener::bind("localhost:0")
            .await
            .map_err(|e| format!("Failed to bind WebSocket server: {}", e))?;

        let addr = listener
            .local_addr()
            .map_err(|e| format!("Failed to get local address: {}", e))?;

        let port = addr.port();
        let session_id = session_handle.id.clone();

        info!(
            "WebSocket bridge (v2) started on port {} for session {} with token auth",
            port, session_id
        );

        let (ready_tx, ready_rx) = oneshot::channel::<()>();

        let token_clone = token.clone();
        tokio::spawn(Self::run_server_v2(
            listener,
            session_handle,
            ready_tx,
            token_clone,
            scroll_buffer,
            replay_on_connect,
        ));

        let _ = tokio::time::timeout(Duration::from_millis(500), ready_rx).await;

        Ok((session_id, port, token))
    }

    /// Start bridge for ExtendedSessionHandle (with command channel) and return disconnect reason
    /// This is the v2 API that works with SessionRegistry
    /// Returns: (session_id, port, token, disconnect_rx)
    /// The disconnect_rx will receive the reason when the WebSocket connection ends
    pub async fn start_extended_with_disconnect(
        session_handle: SshExtendedSessionHandle,
        scroll_buffer: Arc<ScrollBuffer>,
        replay_on_connect: bool,
    ) -> Result<(String, u16, String, oneshot::Receiver<DisconnectReason>), String> {
        // Generate time-bound authentication token to prevent local process hijacking
        let token = generate_token();

        let listener = TcpListener::bind("localhost:0")
            .await
            .map_err(|e| format!("Failed to bind WebSocket server: {}", e))?;

        let addr = listener
            .local_addr()
            .map_err(|e| format!("Failed to get local address: {}", e))?;

        let port = addr.port();
        let session_id = session_handle.id.clone();

        info!(
            "WebSocket bridge (v2+disconnect) started on port {} for session {} with token auth",
            port, session_id
        );

        let (ready_tx, ready_rx) = oneshot::channel::<()>();
        let (disconnect_tx, disconnect_rx) = oneshot::channel::<DisconnectReason>();

        let token_clone = token.clone();
        tokio::spawn(Self::run_server_v2_with_disconnect(
            listener,
            session_handle,
            ready_tx,
            token_clone,
            disconnect_tx,
            scroll_buffer,
            replay_on_connect,
        ));

        let _ = tokio::time::timeout(Duration::from_millis(500), ready_rx).await;

        Ok((session_id, port, token, disconnect_rx))
    }

    /// Run the WebSocket server (legacy mode - backward compatible)
    async fn run_server(
        listener: TcpListener,
        session_handle: SessionHandle,
        ready_tx: oneshot::Sender<()>,
        expected_token: String,
        scroll_buffer: Arc<ScrollBuffer>,
    ) {
        let session_id = session_handle.id.clone();

        // Signal that we're ready to accept connections
        let _ = ready_tx.send(());

        // Accept only one connection per session (with timeout)
        let accept_result = tokio::time::timeout(Duration::from_secs(WS_ACCEPT_TIMEOUT_SECS), listener.accept()).await;

        match accept_result {
            Ok(Ok((stream, addr))) => {
                // Disable Nagle's algorithm for low-latency interactive terminal
                if let Err(e) = stream.set_nodelay(true) {
                    warn!("Failed to set TCP_NODELAY: {}", e);
                }
                info!(
                    "WebSocket connection from {} for session {}",
                    addr, session_id
                );
                if let Err(e) = Self::handle_connection_v1(
                    stream,
                    session_handle,
                    None,
                    expected_token,
                    scroll_buffer,
                )
                .await
                {
                    error!("WebSocket connection error: {}", e);
                }
            }
            Ok(Err(e)) => {
                error!("Failed to accept WebSocket connection: {}", e);
            }
            Err(_) => {
                warn!("WebSocket accept timeout for session {}", session_id);
            }
        }

        info!("WebSocket server stopped for session {}", session_id);
    }

    /// Run the WebSocket server with extended features
    async fn run_server_extended(
        listener: TcpListener,
        extended: ExtendedSessionHandle,
        ready_tx: oneshot::Sender<()>,
        expected_token: String,
        scroll_buffer: Arc<ScrollBuffer>,
    ) {
        let session_id = extended.handle.id.clone();

        let _ = ready_tx.send(());

        let accept_result = tokio::time::timeout(Duration::from_secs(WS_ACCEPT_TIMEOUT_SECS), listener.accept()).await;

        match accept_result {
            Ok(Ok((stream, addr))) => {
                // Disable Nagle's algorithm for low-latency interactive terminal
                if let Err(e) = stream.set_nodelay(true) {
                    warn!("Failed to set TCP_NODELAY: {}", e);
                }
                info!(
                    "WebSocket connection (extended) from {} for session {}",
                    addr, session_id
                );
                if let Err(e) = Self::handle_connection_v1(
                    stream,
                    extended.handle,
                    Some(extended.resize_tx),
                    expected_token,
                    scroll_buffer,
                )
                .await
                {
                    error!("WebSocket connection error: {}", e);
                }
            }
            Ok(Err(e)) => {
                error!("Failed to accept WebSocket connection: {}", e);
            }
            Err(_) => {
                warn!("WebSocket accept timeout for session {}", session_id);
            }
        }

        info!(
            "WebSocket server (extended) stopped for session {}",
            session_id
        );
    }

    /// Handle a single WebSocket connection with v1 protocol
    async fn handle_connection_v1(
        stream: TcpStream,
        session_handle: SessionHandle,
        resize_tx: Option<ResizeTx>,
        expected_token: String,
        scroll_buffer: Arc<ScrollBuffer>,
    ) -> Result<(), String> {
        // Perform WebSocket handshake (no auth yet)
        let ws_stream = accept_async(stream)
            .await
            .map_err(|e| format!("WebSocket handshake failed: {}", e))?;

        let (ws_sender, mut ws_receiver) = ws_stream.split();

        // Authenticate: expect first message to contain token
        let auth_result = tokio::time::timeout(Duration::from_secs(5), ws_receiver.next()).await;

        match auth_result {
            Ok(Some(Ok(Message::Text(token)))) => {
                if validate_token(&token, &expected_token) {
                    debug!("WebSocket token authentication successful");
                } else {
                    error!("WebSocket token authentication failed: invalid or expired token");
                    return Err("Authentication failed: invalid or expired token".to_string());
                }
            }
            Ok(Some(Ok(Message::Binary(data)))) => {
                let token = String::from_utf8_lossy(&data);
                if validate_token(&token, &expected_token) {
                    debug!("WebSocket token authentication successful (binary)");
                } else {
                    error!("WebSocket token authentication failed: invalid or expired token");
                    return Err("Authentication failed: invalid or expired token".to_string());
                }
            }
            Ok(Some(Err(e))) => {
                error!("WebSocket error during authentication: {}", e);
                return Err(format!("Authentication failed: {}", e));
            }
            Ok(None) => {
                error!("WebSocket closed before authentication");
                return Err("Authentication failed: connection closed".to_string());
            }
            Err(_) => {
                error!("WebSocket authentication timeout");
                return Err("Authentication failed: timeout".to_string());
            }
            _ => {
                error!("WebSocket authentication failed: unexpected message type");
                return Err("Authentication failed: unexpected message".to_string());
            }
        }

        // Reunite the split stream for further processing
        let ws_stream = ws_sender
            .reunite(ws_receiver)
            .map_err(|e| format!("Failed to reunite WebSocket stream: {}", e))?;

        debug!(
            "WebSocket handshake and authentication completed for session {}",
            session_handle.id
        );

        let (mut ws_sender, mut ws_receiver) = ws_stream.split();
        
        // 发送 scroll_buffer 中最近的历史行给新连接的客户端
        // 只发送最近 REPLAY_LINE_COUNT 行，避免克隆整个缓冲区浪费内存
        let history_lines = scroll_buffer.tail_lines(REPLAY_LINE_COUNT).await;
        if !history_lines.is_empty() {
            debug!(
                "Sending {} history lines to reconnected client for session {}",
                history_lines.len(),
                session_handle.id
            );
            // 将历史行重新组合为原始数据发送
            let history_data: String = history_lines
                .iter()
                .map(|line| format!("{}\r\n", line.text))
                .collect();
            let frame = data_frame(Bytes::from(history_data));
            if let Err(e) = ws_sender.send(Message::Binary(frame.encode().to_vec())).await {
                warn!("Failed to send history data: {}", e);
            }
        }
        
        // Extract parts from handle, consuming it properly
        let (id, stdin_tx, mut stdout_rx) = session_handle.into_parts();

        let state = Arc::new(ConnectionState::new());
        let state_out = state.clone();
        let state_hb = state.clone();

        // Channel for sending frames to WebSocket (increased capacity to prevent deadlock)
        let (frame_tx, mut frame_rx) = mpsc::channel::<Bytes>(FRAME_CHANNEL_CAPACITY);
        let frame_tx_ssh = frame_tx.clone();
        let frame_tx_hb = frame_tx.clone();

        // Task: Frame sender - consolidates all outgoing frames
        let sender_task = tokio::spawn(async move {
            while let Some(data) = frame_rx.recv().await {
                // Use timeout to detect dead clients (prevents deadlock)
                match tokio::time::timeout(
                    Duration::from_secs(5),
                    ws_sender.send(Message::Binary(data.to_vec())),
                )
                .await
                {
                    Ok(Ok(_)) => {
                        // Send successful
                    }
                    Ok(Err(e)) => {
                        debug!("WebSocket send failed: {:?}", e);
                        break;
                    }
                    Err(_) => {
                        warn!(
                            "WebSocket send timeout after 5s - client unresponsive, disconnecting"
                        );
                        break;
                    }
                }
            }
            debug!("Frame sender stopped");
        });

        // Task: Forward SSH output to WebSocket as Data frames
        let buffer_clone = scroll_buffer.clone();
        let ssh_out_task = tokio::spawn(async move {
            while let Some(data) = stdout_rx.recv().await {
                // Parse terminal output and append to scroll buffer
                let lines = parse_terminal_output(&data);
                if !lines.is_empty() {
                    buffer_clone.append_batch(lines).await;
                }

                // Forward to WebSocket
                let frame = data_frame(Bytes::from(data));
                if frame_tx_ssh.send(frame.encode()).await.is_err() {
                    debug!("Frame channel closed");
                    break;
                }
                state_out.touch();
            }
            debug!("SSH -> WS forwarder stopped");
        });

        // Task: Heartbeat sender
        let sid_hb = id.clone();
        let heartbeat_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(HEARTBEAT_INTERVAL_SECS));
            loop {
                interval.tick().await;

                // Check for timeout
                let now_millis = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64;
                let last = state_hb.last_seen_millis();
                let elapsed_secs = (now_millis - last) / 1000;

                if elapsed_secs > HEARTBEAT_TIMEOUT_SECS {
                    warn!(
                        "Heartbeat timeout for session {} ({}s since last activity)",
                        sid_hb, elapsed_secs
                    );
                    // Send error frame before closing
                    let err = error_frame("Connection timeout - no heartbeat response");
                    let _ = frame_tx_hb.send(err.encode()).await;
                    break;
                }

                // Send heartbeat (non-blocking to avoid backpressure)
                let seq = state_hb.next_seq();
                let frame = heartbeat_frame(seq);
                if frame_tx_hb.try_send(frame.encode()).is_err() {
                    // Channel full means frontend is overloaded - abort heartbeat
                    debug!(
                        "Heartbeat channel full, terminating heartbeat task for session {}",
                        sid_hb
                    );
                    break;
                }
                debug!("Sent heartbeat seq={} for session {}", seq, sid_hb);
            }
            debug!("Heartbeat task stopped for session {}", sid_hb);
        });

        // Task: Process incoming WebSocket messages
        let sid_in = id.clone();
        let input_task = tokio::spawn(async move {
            let mut codec = FrameCodec::new();
            let start = Instant::now();

            while let Some(msg) = ws_receiver.next().await {
                match msg {
                    Ok(Message::Binary(data)) => {
                        state.touch();

                        // Feed data to codec
                        codec.feed(&data);

                        // Process all complete frames
                        loop {
                            match codec.decode_next() {
                                Ok(Some(frame)) => {
                                    match frame {
                                        Frame::Data(payload) => {
                                            // Forward to SSH stdin
                                            if stdin_tx.send(payload.to_vec()).await.is_err() {
                                                debug!("SSH stdin channel closed");
                                                return;
                                            }
                                        }
                                        Frame::Resize { cols, rows } => {
                                            info!(
                                                "Resize request: {}x{} for session {}",
                                                cols, rows, sid_in
                                            );
                                            if let Some(ref tx) = resize_tx {
                                                let _ = tx.send((cols, rows)).await;
                                            }
                                        }
                                        Frame::Heartbeat(seq) => {
                                            debug!(
                                                "Received heartbeat ack seq={} for session {}",
                                                seq, sid_in
                                            );
                                            // Heartbeat response received - connection is alive
                                        }
                                        Frame::Error(msg) => {
                                            warn!(
                                                "Received error frame from client: {} for session {}",
                                                msg, sid_in
                                            );
                                        }
                                    }
                                }
                                Ok(None) => {
                                    // Need more data
                                    break;
                                }
                                Err(e) => {
                                    warn!("Protocol decode error: {} for session {}", e, sid_in);
                                    // For backward compatibility, treat as raw data
                                    // This handles legacy clients that don't use the protocol
                                    if start.elapsed() < Duration::from_secs(5) {
                                        // Early in connection, might be legacy client
                                        debug!("Falling back to raw mode for legacy client");
                                        if stdin_tx.send(data.clone()).await.is_err() {
                                            return;
                                        }
                                    }
                                    codec.clear();
                                    break;
                                }
                            }
                        }
                    }
                    Ok(Message::Text(text)) => {
                        state.touch();
                        // Legacy text mode - treat as raw data
                        if stdin_tx.send(text.into_bytes()).await.is_err() {
                            debug!("SSH stdin channel closed");
                            break;
                        }
                    }
                    Ok(Message::Close(_)) => {
                        info!("WebSocket close message received for session {}", sid_in);
                        break;
                    }
                    Ok(Message::Ping(_)) => {
                        debug!("Received ping");
                        state.touch();
                    }
                    Ok(Message::Pong(_)) => {
                        debug!("Received pong");
                        state.touch();
                    }
                    Ok(Message::Frame(_)) => {
                        // Raw frame, ignore
                    }
                    Err(e) => {
                        warn!("WebSocket receive error: {} for session {}", e, sid_in);
                        break;
                    }
                }
            }
            debug!("WS -> SSH forwarder stopped for session {}", sid_in);
        });

        // Wait for any task to complete
        tokio::select! {
            _ = sender_task => {
                debug!("Sender task completed for session {}", id);
            }
            _ = ssh_out_task => {
                debug!("SSH output task completed for session {}", id);
            }
            _ = heartbeat_task => {
                debug!("Heartbeat task completed for session {}", id);
            }
            _ = input_task => {
                debug!("Input task completed for session {}", id);
            }
        }

        info!("WebSocket bridge terminated for session {}", id);
        Ok(())
    }

    /// Run the WebSocket server v2 (with SessionCommand support)
    async fn run_server_v2(
        listener: TcpListener,
        session_handle: SshExtendedSessionHandle,
        ready_tx: oneshot::Sender<()>,
        expected_token: String,
        scroll_buffer: Arc<ScrollBuffer>,
        replay_on_connect: bool,
    ) {
        let session_id = session_handle.id.clone();

        let _ = ready_tx.send(());

        let accept_result = tokio::time::timeout(Duration::from_secs(WS_ACCEPT_TIMEOUT_SECS), listener.accept()).await;

        match accept_result {
            Ok(Ok((stream, addr))) => {
                // Disable Nagle's algorithm for low-latency interactive terminal
                if let Err(e) = stream.set_nodelay(true) {
                    warn!("Failed to set TCP_NODELAY: {}", e);
                }
                info!(
                    "WebSocket connection (v2) from {} for session {}",
                    addr, session_id
                );
                if let Err(e) = Self::handle_connection_v2(
                    stream,
                    session_handle,
                    expected_token,
                    scroll_buffer,
                    replay_on_connect,
                )
                .await
                {
                    error!("WebSocket connection error: {}", e);
                }
            }
            Ok(Err(e)) => {
                error!("Failed to accept WebSocket connection: {}", e);
            }
            Err(_) => {
                warn!("WebSocket accept timeout for session {}", session_id);
            }
        }

        info!("WebSocket server (v2) stopped for session {}", session_id);
    }

    /// Run the WebSocket server v2 with disconnect reason reporting
    async fn run_server_v2_with_disconnect(
        listener: TcpListener,
        session_handle: SshExtendedSessionHandle,
        ready_tx: oneshot::Sender<()>,
        expected_token: String,
        disconnect_tx: oneshot::Sender<DisconnectReason>,
        scroll_buffer: Arc<ScrollBuffer>,
        replay_on_connect: bool,
    ) {
        let session_id = session_handle.id.clone();

        let _ = ready_tx.send(());

        let accept_result = tokio::time::timeout(Duration::from_secs(WS_ACCEPT_TIMEOUT_SECS), listener.accept()).await;

        let disconnect_reason = match accept_result {
            Ok(Ok((stream, addr))) => {
                // Disable Nagle's algorithm for low-latency interactive terminal
                if let Err(e) = stream.set_nodelay(true) {
                    warn!("Failed to set TCP_NODELAY: {}", e);
                }
                info!(
                    "WebSocket connection (v2+disconnect) from {} for session {}",
                    addr, session_id
                );
                match Self::handle_connection_v2_with_disconnect(
                    stream,
                    session_handle,
                    expected_token,
                    scroll_buffer,
                    replay_on_connect,
                )
                .await
                {
                    Ok(reason) => reason,
                    Err(e) => {
                        error!("WebSocket connection error: {}", e);
                        if e.contains("Authentication") {
                            DisconnectReason::AuthFailed
                        } else {
                            DisconnectReason::NetworkError(e)
                        }
                    }
                }
            }
            Ok(Err(e)) => {
                error!("Failed to accept WebSocket connection: {}", e);
                DisconnectReason::NetworkError(e.to_string())
            }
            Err(_) => {
                warn!("WebSocket accept timeout for session {}", session_id);
                DisconnectReason::AcceptTimeout
            }
        };

        info!(
            "WebSocket server (v2+disconnect) stopped for session {}: {:?}",
            session_id, disconnect_reason
        );

        // Send disconnect reason (ignore error if receiver dropped)
        let _ = disconnect_tx.send(disconnect_reason);
    }

    /// Handle connection with v2 protocol (uses SessionCommand)
    async fn handle_connection_v2(
        stream: TcpStream,
        session_handle: SshExtendedSessionHandle,
        expected_token: String,
        scroll_buffer: Arc<ScrollBuffer>,
        replay_on_connect: bool,
    ) -> Result<(), String> {
        // Perform WebSocket handshake (no auth yet)
        let ws_stream = accept_async(stream)
            .await
            .map_err(|e| format!("WebSocket handshake failed: {}", e))?;

        let (ws_sender, mut ws_receiver) = ws_stream.split();

        // Authenticate: expect first message to contain token
        let auth_result = tokio::time::timeout(Duration::from_secs(5), ws_receiver.next()).await;

        match auth_result {
            Ok(Some(Ok(Message::Text(token)))) => {
                if validate_token(&token, &expected_token) {
                    debug!("WebSocket token authentication successful (v2))");
                } else {
                    error!("WebSocket token authentication failed (v2): invalid or expired token");
                    return Err("Authentication failed: invalid or expired token".to_string());
                }
            }
            Ok(Some(Ok(Message::Binary(data)))) => {
                let token = String::from_utf8_lossy(&data);
                if validate_token(&token, &expected_token) {
                    debug!("WebSocket token authentication successful (v2, binary)");
                } else {
                    error!("WebSocket token authentication failed (v2): invalid or expired token");
                    return Err("Authentication failed: invalid or expired token".to_string());
                }
            }
            Ok(Some(Err(e))) => {
                error!("WebSocket error during authentication (v2): {}", e);
                return Err(format!("Authentication failed: {}", e));
            }
            Ok(None) => {
                error!("WebSocket closed before authentication (v2)");
                return Err("Authentication failed: connection closed".to_string());
            }
            Err(_) => {
                error!("WebSocket authentication timeout (v2)");
                return Err("Authentication failed: timeout".to_string());
            }
            _ => {
                error!("WebSocket authentication failed (v2): unexpected message type");
                return Err("Authentication failed: unexpected message".to_string());
            }
        }

        // Reunite the split stream for further processing
        let ws_stream = ws_sender
            .reunite(ws_receiver)
            .map_err(|e| format!("Failed to reunite WebSocket stream: {}", e))?;

        debug!(
            "WebSocket handshake (v2) completed for session {}",
            session_handle.id
        );

        let (mut ws_sender, mut ws_receiver) = ws_stream.split();
        // Extract parts from handle, consuming it properly
        let (id, cmd_tx, mut stdout_rx) = session_handle.into_parts();

        if replay_on_connect {
            if let Ok(replay) = build_replay_frame(scroll_buffer.clone()).await {
                if !replay.is_empty() {
                    let _ = ws_sender.send(Message::Binary(replay)).await;
                }
            }
        }

        let state = Arc::new(ConnectionState::new());
        let state_out = state.clone();
        let state_hb = state.clone();

        // Channel for sending frames to WebSocket (increased capacity to prevent deadlock)
        let (frame_tx, mut frame_rx) = mpsc::channel::<Bytes>(FRAME_CHANNEL_CAPACITY);
        let frame_tx_ssh = frame_tx.clone();
        let frame_tx_hb = frame_tx.clone();

        let sid_in = id.clone();
        let sid_out = id.clone();

        // Task: WebSocket sender (multiplexes frame_tx)
        let sender_task = tokio::spawn(async move {
            while let Some(frame) = frame_rx.recv().await {
                // Use timeout to detect dead clients (prevents deadlock)
                match tokio::time::timeout(
                    Duration::from_secs(5),
                    ws_sender.send(Message::Binary(frame.to_vec())),
                )
                .await
                {
                    Ok(Ok(_)) => {
                        // Send successful
                    }
                    Ok(Err(e)) => {
                        debug!("WebSocket send failed: {:?}", e);
                        break;
                    }
                    Err(_) => {
                        warn!(
                            "WebSocket send timeout after 5s - client unresponsive, disconnecting"
                        );
                        break;
                    }
                }
            }
            debug!("WebSocket sender task stopped");
        });

        // Task: SSH stdout -> WebSocket
        let ssh_out_task = tokio::spawn(async move {
            while let Ok(data) = stdout_rx.recv().await {
                state_out.touch();

                // Forward to WebSocket
                let frame = data_frame(Bytes::from(data)).encode();
                if frame_tx_ssh.send(frame).await.is_err() {
                    break;
                }
            }
            debug!("SSH -> WS forwarder stopped for session {}", sid_out);
        });

        // Task: Heartbeat sender
        let heartbeat_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(HEARTBEAT_INTERVAL_SECS));
            loop {
                interval.tick().await;

                // Check if connection is dead
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64;
                let last = state_hb.last_seen_millis();
                if now.saturating_sub(last) > HEARTBEAT_TIMEOUT_SECS * 1000 {
                    warn!("Heartbeat timeout detected");
                    break;
                }

                let seq = state_hb.next_seq();
                let frame = heartbeat_frame(seq).encode();
                if frame_tx_hb.try_send(frame).is_err() {
                    // Channel full means frontend is overloaded - abort heartbeat
                    debug!("Heartbeat channel full, terminating heartbeat task");
                    break;
                }
            }
            debug!("Heartbeat task stopped");
        });

        // Task: WebSocket -> SSH (uses cmd_tx with SessionCommand)
        let cmd_tx_clone = cmd_tx.clone();
        let input_task = tokio::spawn(async move {
            let mut codec = FrameCodec::new();
            let start = Instant::now();

            while let Some(msg) = ws_receiver.next().await {
                match msg {
                    Ok(Message::Binary(data)) => {
                        state.touch();
                        codec.feed(&data);

                        while let Ok(Some(frame)) = codec.decode_next() {
                            match frame {
                                Frame::Data(payload) => {
                                    if cmd_tx_clone
                                        .send(SessionCommand::Data(payload.to_vec()))
                                        .await
                                        .is_err()
                                    {
                                        debug!("SSH cmd channel closed");
                                        return;
                                    }
                                }
                                Frame::Resize { cols, rows } => {
                                    info!(
                                        "Resize request: {}x{} for session {}",
                                        cols, rows, sid_in
                                    );
                                    if cmd_tx_clone
                                        .send(SessionCommand::Resize(cols, rows))
                                        .await
                                        .is_err()
                                    {
                                        debug!("SSH cmd channel closed");
                                        return;
                                    }
                                }
                                Frame::Heartbeat(seq) => {
                                    debug!("Received heartbeat echo: seq={}", seq);
                                }
                                Frame::Error(msg) => {
                                    error!("Error frame from client: {}", msg);
                                }
                            }
                        }

                        if codec.is_overflow() {
                            if start.elapsed() < Duration::from_secs(5) {
                                debug!("Falling back to raw mode for legacy client");
                                if cmd_tx_clone
                                    .send(SessionCommand::Data(data.to_vec()))
                                    .await
                                    .is_err()
                                {
                                    return;
                                }
                            }
                            codec.clear();
                            break;
                        }
                    }
                    Ok(Message::Text(text)) => {
                        state.touch();
                        if cmd_tx_clone
                            .send(SessionCommand::Data(text.into_bytes()))
                            .await
                            .is_err()
                        {
                            debug!("SSH cmd channel closed");
                            break;
                        }
                    }
                    Ok(Message::Close(_)) => {
                        info!("WebSocket close message received for session {}", sid_in);
                        let _ = cmd_tx_clone.send(SessionCommand::Close).await;
                        break;
                    }
                    Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => {
                        state.touch();
                    }
                    Ok(Message::Frame(_)) => {}
                    Err(e) => {
                        warn!("WebSocket receive error: {} for session {}", e, sid_in);
                        break;
                    }
                }
            }
            debug!("WS -> SSH forwarder (v2) stopped for session {}", sid_in);
        });

        // Wait for any task to complete
        tokio::select! {
            _ = sender_task => {
                debug!("Sender task completed for session {}", id);
            }
            _ = ssh_out_task => {
                debug!("SSH output task completed for session {}", id);
            }
            _ = heartbeat_task => {
                debug!("Heartbeat task completed for session {}", id);
            }
            _ = input_task => {
                debug!("Input task completed for session {}", id);
            }
        }

        info!("WebSocket bridge (v2) terminated for session {}", id);
        Ok(())
    }

    /// Handle connection with v2 protocol and return disconnect reason
    async fn handle_connection_v2_with_disconnect(
        stream: TcpStream,
        session_handle: SshExtendedSessionHandle,
        expected_token: String,
        scroll_buffer: Arc<ScrollBuffer>,
        replay_on_connect: bool,
    ) -> Result<DisconnectReason, String> {
        // Perform WebSocket handshake (no auth yet)
        let ws_stream = accept_async(stream)
            .await
            .map_err(|e| format!("WebSocket handshake failed: {}", e))?;

        let (ws_sender, mut ws_receiver) = ws_stream.split();

        // Authenticate: expect first message to contain token
        let auth_result = tokio::time::timeout(Duration::from_secs(5), ws_receiver.next()).await;

        match auth_result {
            Ok(Some(Ok(Message::Text(token)))) => {
                if validate_token(&token, &expected_token) {
                    debug!("WebSocket token authentication successful (v2+disconnect)");
                } else {
                    error!("WebSocket token authentication failed: invalid or expired token");
                    return Err("Authentication failed: invalid or expired token".to_string());
                }
            }
            Ok(Some(Ok(Message::Binary(data)))) => {
                let token = String::from_utf8_lossy(&data);
                if validate_token(&token, &expected_token) {
                    debug!("WebSocket token authentication successful (v2+disconnect, binary)");
                } else {
                    error!("WebSocket token authentication failed: invalid or expired token");
                    return Err("Authentication failed: invalid or expired token".to_string());
                }
            }
            Ok(Some(Err(e))) => {
                error!("WebSocket error during authentication: {}", e);
                return Err(format!("Authentication failed: {}", e));
            }
            Ok(None) => {
                error!("WebSocket closed before authentication");
                return Err("Authentication failed: connection closed".to_string());
            }
            Err(_) => {
                error!("WebSocket authentication timeout");
                return Err("Authentication failed: timeout".to_string());
            }
            _ => {
                error!("WebSocket authentication failed: unexpected message type");
                return Err("Authentication failed: unexpected message".to_string());
            }
        }

        // Reunite the split stream for further processing
        let ws_stream = ws_sender
            .reunite(ws_receiver)
            .map_err(|e| format!("Failed to reunite WebSocket stream: {}", e))?;

        debug!(
            "WebSocket handshake (v2+disconnect) completed for session {}",
            session_handle.id
        );

        let (mut ws_sender, mut ws_receiver) = ws_stream.split();
        let (id, cmd_tx, mut stdout_rx) = session_handle.into_parts();

        if replay_on_connect {
            if let Ok(replay) = build_replay_frame(scroll_buffer.clone()).await {
                if !replay.is_empty() {
                    let _ = ws_sender.send(Message::Binary(replay)).await;
                }
            }
        }

        let state = Arc::new(ConnectionState::new());
        let state_out = state.clone();
        let state_hb = state.clone();

        // Channel for sending frames to WebSocket
        let (frame_tx, mut frame_rx) = mpsc::channel::<Bytes>(FRAME_CHANNEL_CAPACITY);
        let frame_tx_ssh = frame_tx.clone();
        let frame_tx_hb = frame_tx.clone();

        let sid_in = id.clone();
        let sid_out = id.clone();

        // Task: WebSocket sender
        let sender_task = tokio::spawn(async move {
            while let Some(frame) = frame_rx.recv().await {
                match tokio::time::timeout(
                    Duration::from_secs(5),
                    ws_sender.send(Message::Binary(frame.to_vec())),
                )
                .await
                {
                    Ok(Ok(_)) => {}
                    Ok(Err(e)) => {
                        debug!("WebSocket send failed: {:?}", e);
                        return "network_error";
                    }
                    Err(_) => {
                        warn!("WebSocket send timeout - client unresponsive");
                        return "send_timeout";
                    }
                }
            }
            "channel_closed"
        });

        // Task: SSH stdout -> WebSocket
        let ssh_out_task = tokio::spawn(async move {
            while let Ok(data) = stdout_rx.recv().await {
                state_out.touch();

                // Forward to WebSocket
                let frame = data_frame(Bytes::from(data)).encode();
                if frame_tx_ssh.send(frame).await.is_err() {
                    return "channel_closed";
                }
            }
            debug!("SSH -> WS forwarder stopped for session {}", sid_out);
            "ssh_closed"
        });

        // Task: Heartbeat sender - returns reason if timeout
        let heartbeat_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(HEARTBEAT_INTERVAL_SECS));
            loop {
                interval.tick().await;

                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64;
                let last = state_hb.last_seen_millis();
                if now.saturating_sub(last) > HEARTBEAT_TIMEOUT_SECS * 1000 {
                    warn!("Heartbeat timeout detected for session");
                    return "heartbeat_timeout";
                }

                let seq = state_hb.next_seq();
                let frame = heartbeat_frame(seq).encode();
                if frame_tx_hb.try_send(frame).is_err() {
                    debug!("Heartbeat channel full");
                    return "channel_full";
                }
            }
        });

        // Task: WebSocket -> SSH
        let cmd_tx_clone = cmd_tx.clone();
        let input_task = tokio::spawn(async move {
            let mut codec = FrameCodec::new();
            let start = Instant::now();

            while let Some(msg) = ws_receiver.next().await {
                match msg {
                    Ok(Message::Binary(data)) => {
                        state.touch();
                        codec.feed(&data);

                        while let Ok(Some(frame)) = codec.decode_next() {
                            match frame {
                                Frame::Data(payload) => {
                                    if cmd_tx_clone
                                        .send(SessionCommand::Data(payload.to_vec()))
                                        .await
                                        .is_err()
                                    {
                                        return "ssh_closed";
                                    }
                                }
                                Frame::Resize { cols, rows } => {
                                    info!("Resize: {}x{} for session {}", cols, rows, sid_in);
                                    if cmd_tx_clone
                                        .send(SessionCommand::Resize(cols, rows))
                                        .await
                                        .is_err()
                                    {
                                        return "ssh_closed";
                                    }
                                }
                                Frame::Heartbeat(seq) => {
                                    debug!("Received heartbeat echo: seq={}", seq);
                                }
                                Frame::Error(msg) => {
                                    error!("Error frame from client: {}", msg);
                                }
                            }
                        }

                        if codec.is_overflow() {
                            if start.elapsed() < Duration::from_secs(5)
                                && cmd_tx_clone
                                    .send(SessionCommand::Data(data.to_vec()))
                                    .await
                                    .is_err()
                            {
                                return "ssh_closed";
                            }
                            codec.clear();
                            break;
                        }
                    }
                    Ok(Message::Text(text)) => {
                        state.touch();
                        if cmd_tx_clone
                            .send(SessionCommand::Data(text.into_bytes()))
                            .await
                            .is_err()
                        {
                            return "ssh_closed";
                        }
                    }
                    Ok(Message::Close(_)) => {
                        info!("WebSocket close message received for session {}", sid_in);
                        let _ = cmd_tx_clone.send(SessionCommand::Close).await;
                        return "client_closed";
                    }
                    Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => {
                        state.touch();
                    }
                    Ok(Message::Frame(_)) => {}
                    Err(e) => {
                        warn!("WebSocket receive error: {} for session {}", e, sid_in);
                        return "network_error";
                    }
                }
            }
            "client_closed"
        });

        // Wait for any task to complete and determine disconnect reason
        let reason_str = tokio::select! {
            result = sender_task => result.unwrap_or("unknown"),
            result = ssh_out_task => result.unwrap_or("ssh_closed"),
            result = heartbeat_task => result.unwrap_or("heartbeat_timeout"),
            result = input_task => result.unwrap_or("client_closed"),
        };

        let disconnect_reason = match reason_str {
            "heartbeat_timeout" => DisconnectReason::HeartbeatTimeout,
            "ssh_closed" => DisconnectReason::SshChannelClosed,
            "client_closed" => DisconnectReason::ClientClosed,
            "network_error" | "send_timeout" => {
                DisconnectReason::NetworkError(reason_str.to_string())
            }
            "channel_full" => {
                // Channel full means server is overloaded, not client disconnect
                DisconnectReason::NetworkError("server_overloaded".to_string())
            }
            _ => DisconnectReason::ClientClosed,
        };

        if matches!(disconnect_reason, DisconnectReason::ClientClosed) {
            let _ = cmd_tx.send(SessionCommand::Close).await;
        }

        info!(
            "WebSocket bridge (v2+disconnect) terminated for session {}: {:?}",
            id, disconnect_reason
        );

        Ok(disconnect_reason)
    }
}
