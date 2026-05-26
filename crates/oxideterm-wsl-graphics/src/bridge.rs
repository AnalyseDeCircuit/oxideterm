// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! WebSocket to VNC bridge ported from Tauri's WSL graphics backend.

use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use rand::RngCore;
use subtle::ConstantTimeEq;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::http::{Response, StatusCode};
use zeroize::Zeroizing;

use crate::WslGraphicsError;

pub async fn start_proxy(
    vnc_addr: String,
    session_id: String,
) -> Result<(u16, String, JoinHandle<()>), WslGraphicsError> {
    start_proxy_impl(vnc_addr, session_id).await
}

pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub fn extract_token(uri: &str) -> Option<Zeroizing<String>> {
    uri.split('?').nth(1)?.split('&').find_map(|pair| {
        let mut kv = pair.splitn(2, '=');
        let key = kv.next()?;
        let value = kv.next()?;
        (key == "token").then(|| Zeroizing::new(value.to_string()))
    })
}

async fn start_proxy_impl(
    vnc_addr: String,
    session_id: String,
) -> Result<(u16, String, JoinHandle<()>), WslGraphicsError> {
    let token = generate_token();
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let ws_port = listener.local_addr()?.port();

    // The returned token is needed by the caller to build the one-shot noVNC
    // URL; keep the bridge-side comparison copy zeroized after the task exits.
    let expected_token = Zeroizing::new(token.clone());
    let handle = tokio::spawn(async move {
        match listener.accept().await {
            Ok((stream, addr)) => {
                tracing::info!("Graphics proxy: client connected from {}", addr);
                if let Err(error) = proxy_connection(stream, &vnc_addr, expected_token).await {
                    tracing::warn!("Graphics proxy error: {}", error);
                }
            }
            Err(error) => {
                tracing::error!("Graphics proxy: failed to accept connection: {}", error);
            }
        }
        // The bridge is intentionally one-shot, same as Tauri. A disconnected
        // noVNC client does not own the VNC/desktop lifecycle; reconnect creates
        // a fresh bridge against the same VNC port.
        tracing::info!(
            "Graphics proxy: bridge ended for session {} (VNC stays alive)",
            session_id
        );
    });

    Ok((ws_port, token, handle))
}

async fn proxy_connection(
    tcp_stream: TcpStream,
    vnc_addr: &str,
    expected_token: Zeroizing<String>,
) -> Result<(), WslGraphicsError> {
    let ws_stream = tokio_tungstenite::accept_hdr_async(
        tcp_stream,
        |req: &tokio_tungstenite::tungstenite::http::Request<()>,
         mut resp: Response<()>|
         -> Result<Response<()>, Response<Option<String>>> {
            let uri = req.uri().to_string();
            let token_valid = extract_token(&uri)
                .map(|token| {
                    let left = token.as_bytes();
                    let right = expected_token.as_bytes();
                    left.len() == right.len() && bool::from(left.ct_eq(right))
                })
                .unwrap_or(false);

            if !token_valid {
                tracing::warn!(
                    "Graphics proxy: invalid token for path {}",
                    req.uri().path()
                );
                return Err(Response::builder()
                    .status(StatusCode::FORBIDDEN)
                    .body(Some("Invalid token".to_string()))
                    .expect("valid forbidden response"));
            }

            if let Some(protocols) = req.headers().get("Sec-WebSocket-Protocol") {
                if protocols
                    .to_str()
                    .is_ok_and(|value| value.contains("binary"))
                {
                    resp.headers_mut()
                        .insert("Sec-WebSocket-Protocol", "binary".parse().unwrap());
                }
            }
            Ok(resp)
        },
    )
    .await?;

    let vnc_stream = TcpStream::connect(vnc_addr).await.map_err(|error| {
        tracing::error!(
            "Graphics proxy: failed to connect to VNC at {}: {}",
            vnc_addr,
            error
        );
        error
    })?;
    tracing::info!("Graphics proxy: connected to VNC at {}", vnc_addr);

    let (vnc_read, mut vnc_write) = tokio::io::split(vnc_stream);
    let (mut ws_tx, mut ws_rx) = ws_stream.split();

    tokio::select! {
        result = async {
            let mut reader = tokio::io::BufReader::new(vnc_read);
            let mut buf = vec![0u8; 65_536];
            loop {
                let n = reader.read(&mut buf).await?;
                if n == 0 {
                    break;
                }
                ws_tx.send(Message::Binary(buf[..n].to_vec().into())).await?;
            }
            Ok::<_, WslGraphicsError>(())
        } => {
            if let Err(error) = result {
                tracing::debug!("Graphics proxy: VNC->WS relay ended: {}", error);
            }
        }
        result = async {
            while let Some(msg) = ws_rx.next().await {
                match msg? {
                    Message::Binary(data) => {
                        vnc_write.write_all(&data).await?;
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            Ok::<_, WslGraphicsError>(())
        } => {
            if let Err(error) = result {
                tracing::debug!("Graphics proxy: WS->VNC relay ended: {}", error);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_token_matches_tauri_query_semantics() {
        assert_eq!(
            extract_token("/?token=abc123")
                .as_ref()
                .map(|token| token.as_str()),
            Some("abc123")
        );
        assert_eq!(
            extract_token("/path?foo=bar&token=xyz&baz=1")
                .as_ref()
                .map(|token| token.as_str()),
            Some("xyz")
        );
        assert_eq!(extract_token("/no-query"), None);
        assert_eq!(extract_token("/?other=val"), None);
    }

    #[test]
    fn generate_token_matches_tauri_length_and_randomness() {
        let first = generate_token();
        let second = generate_token();
        assert_eq!(first.len(), 43);
        assert_ne!(first, second);
    }
}
