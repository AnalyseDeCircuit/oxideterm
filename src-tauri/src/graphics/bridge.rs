//! WebSocket ↔ VNC TCP transparent proxy (bridge).
//!
//! Accepts a single WebSocket connection (from noVNC in the frontend plugin),
//! validates the one-time token, and bidirectionally proxies raw bytes to/from
//! the VNC TCP server running inside WSL.
//!
//! Key: Must respond with `Sec-WebSocket-Protocol: binary` header —
//! noVNC silently disconnects without it.

use crate::graphics::GraphicsError;
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use rand::RngCore;
use subtle::ConstantTimeEq;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::http::{Response, StatusCode};
use tokio_tungstenite::tungstenite::Message;

/// Start the WebSocket ↔ VNC TCP proxy.
///
/// Binds to `127.0.0.1:0` (random port), returns `(ws_port, token, task_handle)`.
/// The proxy accepts exactly one WebSocket connection, validates the token,
/// then transparently relays data between VNC and WebSocket.
pub async fn start_proxy(vnc_addr: String) -> Result<(u16, String, JoinHandle<()>), GraphicsError> {
    let token = generate_token();
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let ws_port = listener.local_addr()?.port();

    let expected_token = token.clone();
    let handle = tokio::spawn(async move {
        // Accept a single WebSocket connection
        match listener.accept().await {
            Ok((stream, addr)) => {
                tracing::info!("Graphics proxy: client connected from {}", addr);
                if let Err(e) = proxy_connection(stream, &vnc_addr, &expected_token).await {
                    tracing::warn!("Graphics proxy error: {}", e);
                }
                tracing::info!("Graphics proxy: session ended");
            }
            Err(e) => {
                tracing::error!("Graphics proxy: failed to accept connection: {}", e);
            }
        }
    });

    Ok((ws_port, token, handle))
}

/// The core proxy loop: WebSocket handshake → VNC connect → bidirectional relay.
async fn proxy_connection(
    tcp_stream: TcpStream,
    vnc_addr: &str,
    expected_token: &str,
) -> Result<(), GraphicsError> {
    // 1. WebSocket handshake with token validation + subprotocol negotiation
    let expected_token_owned = expected_token.to_string();
    let ws_stream = tokio_tungstenite::accept_hdr_async(
        tcp_stream,
        |req: &tokio_tungstenite::tungstenite::http::Request<()>, mut resp: Response<()>| {
            // Validate token from query string
            let uri = req.uri().to_string();
            let token_valid = extract_token(&uri)
                .map(|t| {
                    let a = t.as_bytes();
                    let b = expected_token_owned.as_bytes();
                    a.len() == b.len() && bool::from(a.ct_eq(b))
                })
                .unwrap_or(false);

            if !token_valid {
                tracing::warn!("Graphics proxy: invalid token from {}", uri);
                *resp.status_mut() = StatusCode::FORBIDDEN;
                return Err(resp);
            }

            // noVNC sends Sec-WebSocket-Protocol: binary
            // We MUST echo it back, otherwise noVNC silently disconnects
            if let Some(protocols) = req.headers().get("Sec-WebSocket-Protocol") {
                if let Ok(proto_str) = protocols.to_str() {
                    if proto_str.contains("binary") {
                        resp.headers_mut()
                            .insert("Sec-WebSocket-Protocol", "binary".parse().unwrap());
                    }
                }
            }

            Ok(resp)
        },
    )
    .await?;

    // 2. Connect to VNC TCP server
    let vnc_stream = TcpStream::connect(vnc_addr).await.map_err(|e| {
        tracing::error!(
            "Graphics proxy: failed to connect to VNC at {}: {}",
            vnc_addr,
            e
        );
        e
    })?;
    tracing::info!("Graphics proxy: connected to VNC at {}", vnc_addr);

    let (vnc_read, mut vnc_write) = tokio::io::split(vnc_stream);
    let (mut ws_tx, mut ws_rx) = ws_stream.split();

    // 3. Bidirectional relay — either direction ending terminates both
    tokio::select! {
        // VNC → WebSocket
        result = async {
            let mut reader = tokio::io::BufReader::new(vnc_read);
            let mut buf = vec![0u8; 65536];
            loop {
                let n = reader.read(&mut buf).await?;
                if n == 0 { break; }
                ws_tx.send(Message::Binary(buf[..n].to_vec().into())).await?;
            }
            Ok::<_, GraphicsError>(())
        } => {
            if let Err(e) = result {
                tracing::debug!("Graphics proxy: VNC→WS relay ended: {}", e);
            }
        }
        // WebSocket → VNC
        result = async {
            while let Some(msg) = ws_rx.next().await {
                match msg? {
                    Message::Binary(data) => {
                        vnc_write.write_all(&data).await
                            .map_err(|e| GraphicsError::Io(e))?;
                    }
                    Message::Close(_) => break,
                    _ => {} // Ignore text/ping/pong
                }
            }
            Ok::<_, GraphicsError>(())
        } => {
            if let Err(e) = result {
                tracing::debug!("Graphics proxy: WS→VNC relay ended: {}", e);
            }
        }
    }

    Ok(())
}

/// Generate a cryptographically secure one-time token (32 bytes, URL-safe Base64).
fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Extract token value from a URI query string.
/// e.g. "/?token=abc123" → Some("abc123")
fn extract_token(uri: &str) -> Option<String> {
    uri.split('?').nth(1)?.split('&').find_map(|pair| {
        let mut kv = pair.splitn(2, '=');
        let key = kv.next()?;
        let value = kv.next()?;
        if key == "token" {
            Some(value.to_string())
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_token() {
        assert_eq!(extract_token("/?token=abc123"), Some("abc123".to_string()));
        assert_eq!(
            extract_token("/path?foo=bar&token=xyz&baz=1"),
            Some("xyz".to_string())
        );
        assert_eq!(extract_token("/no-query"), None);
        assert_eq!(extract_token("/?other=val"), None);
    }

    #[test]
    fn test_generate_token_length() {
        let token = generate_token();
        // 32 bytes → 43 chars in URL-safe Base64 no-pad
        assert_eq!(token.len(), 43);
    }

    #[test]
    fn test_generate_token_uniqueness() {
        let t1 = generate_token();
        let t2 = generate_token();
        assert_ne!(t1, t2);
    }
}
