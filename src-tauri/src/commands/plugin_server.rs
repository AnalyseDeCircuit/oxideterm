//! Plugin File Server
//!
//! Lightweight HTTP/1.1 server for serving multi-file plugin packages.
//! Binds to 127.0.0.1:0 (OS-assigned port) and serves plugin files at:
//!   http://127.0.0.1:{port}/plugins/{plugin-id}/{path}
//!
//! Security:
//! - Only binds to loopback (127.0.0.1)
//! - Reuses validate_plugin_id() + validate_relative_path() for path safety
//! - Canonicalization check prevents directory escape
//! - No directory listing

use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

use super::plugin::{validate_plugin_id, validate_relative_path};
use crate::config::storage::config_dir;

/// Shared state for the plugin file server
pub struct PluginFileServer {
    port: Mutex<Option<u16>>,
    /// Shutdown signal sender: send `true` to request server shutdown.
    shutdown_tx: Mutex<Option<tokio::sync::watch::Sender<bool>>>,
}

impl PluginFileServer {
    pub fn new() -> Self {
        Self {
            port: Mutex::new(None),
            shutdown_tx: Mutex::new(None),
        }
    }
}

/// MIME type lookup by file extension
fn mime_for_ext(ext: &str) -> &'static str {
    match ext {
        "js" | "mjs" => "application/javascript",
        "json" => "application/json",
        "css" => "text/css",
        "html" | "htm" => "text/html",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "ico" => "image/x-icon",
        "wasm" => "application/wasm",
        "map" => "application/json",
        "txt" => "text/plain",
        _ => "application/octet-stream",
    }
}

/// Get the plugins directory path
fn plugins_dir() -> Result<std::path::PathBuf, String> {
    config_dir()
        .map(|dir| dir.join("plugins"))
        .map_err(|e| e.to_string())
}

/// Build an HTTP response with the given status, content-type, and body.
fn build_response(status: u16, reason: &str, content_type: &str, body: &[u8]) -> Vec<u8> {
    let header = format!(
        "HTTP/1.1 {} {}\r\n\
         Content-Type: {}\r\n\
         Content-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Cache-Control: no-cache\r\n\
         Connection: close\r\n\
         \r\n",
        status, reason, content_type, body.len()
    );
    let mut resp = header.into_bytes();
    resp.extend_from_slice(body);
    resp
}

/// Build a simple text error response.
fn error_response(status: u16, reason: &str, message: &str) -> Vec<u8> {
    build_response(status, reason, "text/plain", message.as_bytes())
}

/// Simple percent-decoding for URL paths.
fn percent_decode(input: &str) -> Option<String> {
    let mut out = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return None;
            }
            let hi = char::from(bytes[i + 1]).to_digit(16)?;
            let lo = char::from(bytes[i + 2]).to_digit(16)?;
            out.push((hi * 16 + lo) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Handle a single HTTP request.
/// Expected URL format: /plugins/{plugin-id}/{relative-path...}
async fn handle_request(request_line: &str) -> Vec<u8> {
    // Parse method and path from "GET /path HTTP/1.1"
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 {
        return error_response(400, "Bad Request", "Malformed request line");
    }

    let method = parts[0];

    // Handle CORS preflight
    if method == "OPTIONS" {
        return build_response(204, "No Content", "text/plain", b"");
    }

    if method != "GET" {
        return error_response(405, "Method Not Allowed", "Only GET is supported");
    }

    let raw_path = parts[1];

    // Strip query string if present
    let path = raw_path.split('?').next().unwrap_or(raw_path);

    // Decode percent-encoded characters
    let decoded = match percent_decode(path) {
        Some(d) => d,
        None => return error_response(400, "Bad Request", "Invalid URL encoding"),
    };

    // Parse: /plugins/{plugin_id}/{relative_path...}
    let stripped = decoded.strip_prefix("/plugins/").unwrap_or("");
    if stripped.is_empty() {
        return error_response(404, "Not Found", "Missing plugin ID");
    }

    let (plugin_id, relative_path) = match stripped.split_once('/') {
        Some((id, rest)) => (id, rest),
        None => return error_response(404, "Not Found", "Missing file path"),
    };

    if relative_path.is_empty() {
        return error_response(403, "Forbidden", "Directory listing not allowed");
    }

    // Validate plugin ID and relative path
    if let Err(e) = validate_plugin_id(plugin_id) {
        return error_response(400, "Bad Request", &e);
    }
    if let Err(e) = validate_relative_path(relative_path) {
        return error_response(400, "Bad Request", &e);
    }

    // Resolve and validate the file path
    let base_dir = match plugins_dir() {
        Ok(d) => d,
        Err(e) => return error_response(500, "Internal Server Error", &e),
    };

    let file_path = base_dir.join(plugin_id).join(relative_path);

    // Canonicalize and verify it's inside the plugin directory
    let canonical = match file_path.canonicalize() {
        Ok(c) => c,
        Err(_) => return error_response(404, "Not Found", "File not found"),
    };

    let plugin_root = base_dir.join(plugin_id);
    if let Ok(canonical_root) = plugin_root.canonicalize() {
        if !canonical.starts_with(&canonical_root) {
            return error_response(403, "Forbidden", "Path escapes plugin directory");
        }
    }

    // Reject directories
    if canonical.is_dir() {
        return error_response(403, "Forbidden", "Directory listing not allowed");
    }

    // Read the file
    let body = match tokio::fs::read(&canonical).await {
        Ok(b) => b,
        Err(_) => return error_response(404, "Not Found", "File not found"),
    };

    // Determine MIME type from extension
    let ext = canonical
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let mime = mime_for_ext(ext);

    build_response(200, "OK", mime, &body)
}

/// Handle a single TCP connection: read the HTTP request line, dispatch, respond.
async fn handle_connection(stream: tokio::net::TcpStream) {
    let (reader, mut writer) = stream.into_split();
    let mut buf_reader = BufReader::new(reader);
    let mut request_line = String::new();

    // Read the first line (request line)
    match buf_reader.read_line(&mut request_line).await {
        Ok(0) | Err(_) => return,
        Ok(_) => {}
    }

    // Consume remaining headers (read until empty line)
    loop {
        let mut line = String::new();
        match buf_reader.read_line(&mut line).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                if line.trim().is_empty() {
                    break;
                }
            }
        }
    }

    let response = handle_request(request_line.trim()).await;
    let _ = writer.write_all(&response).await;
    let _ = writer.shutdown().await;
}

// ═══════════════════════════════════════════════════════════════════════════
// Tauri Commands
// ═══════════════════════════════════════════════════════════════════════════

/// Start the plugin file server. Returns the port number.
/// If already running, returns the existing port.
#[tauri::command]
pub async fn start_plugin_server(
    server: tauri::State<'_, Arc<PluginFileServer>>,
) -> Result<u16, String> {
    let mut port_guard = server.port.lock().await;

    // Already running
    if let Some(port) = *port_guard {
        return Ok(port);
    }

    // Bind to loopback with OS-assigned port
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("Failed to bind plugin server: {}", e))?;

    let port = listener
        .local_addr()
        .map_err(|e| format!("Failed to get local address: {}", e))?
        .port();

    tracing::info!("[PluginServer] Started on 127.0.0.1:{}", port);

    // Create shutdown channel
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);

    // Spawn the accept loop with graceful shutdown support
    tokio::spawn(async move {
        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, _addr)) => {
                            tokio::spawn(handle_connection(stream));
                        }
                        Err(e) => {
                            tracing::warn!("[PluginServer] Accept error: {}", e);
                        }
                    }
                }
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        tracing::info!("[PluginServer] Shutting down");
                        break;
                    }
                }
            }
        }
    });

    *port_guard = Some(port);
    *server.shutdown_tx.lock().await = Some(shutdown_tx);
    Ok(port)
}

/// Get the plugin server port, if running.
#[tauri::command]
pub async fn get_plugin_server_port(
    server: tauri::State<'_, Arc<PluginFileServer>>,
) -> Result<Option<u16>, String> {
    let port_guard = server.port.lock().await;
    Ok(*port_guard)
}

/// Stop the plugin file server gracefully.
/// Returns Ok(true) if the server was running and has been stopped,
/// Ok(false) if the server was not running.
#[tauri::command]
pub async fn stop_plugin_server(
    server: tauri::State<'_, Arc<PluginFileServer>>,
) -> Result<bool, String> {
    let mut port_guard = server.port.lock().await;
    let mut shutdown_guard = server.shutdown_tx.lock().await;

    if port_guard.is_none() {
        return Ok(false);
    }

    // Send shutdown signal
    if let Some(tx) = shutdown_guard.take() {
        let _ = tx.send(true);
    }

    *port_guard = None;
    tracing::info!("[PluginServer] Server stopped");
    Ok(true)
}
