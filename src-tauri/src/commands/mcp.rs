//! MCP (Model Context Protocol) Stdio Transport
//!
//! Manages MCP server processes that communicate via stdin/stdout JSON-RPC.
//! Each server is spawned as a child process with configurable command, args, and env.
//!
//! # Concurrency Model
//!
//! Requests to the same MCP server can be sent concurrently. The stdin writer
//! is serialized (necessary for stream ordering), but the response reader runs
//! in a background task that dispatches responses to waiting callers via
//! per-request oneshot channels, keyed by JSON-RPC request ID.

use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tauri::State;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{oneshot, Mutex};
use tokio::task::JoinHandle;

// ═══════════════════════════════════════════════════════════════════════════
// State
// ═══════════════════════════════════════════════════════════════════════════

/// Pending response waiters — maps request_id → oneshot sender.
/// The reader task dispatches responses here.
type PendingMap = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;

struct McpProcess {
    child: Mutex<Child>,
    /// Stdin is serialized: writes must not interleave.
    stdin: Mutex<tokio::process::ChildStdin>,
    /// Monotonic request ID counter (lock-free).
    next_id: AtomicU64,
    /// Pending response waiters.
    pending: PendingMap,
    /// Background task reading stdout and dispatching responses.
    reader_task: JoinHandle<()>,
    /// Background task logging stderr.
    stderr_task: JoinHandle<()>,
}

pub struct McpProcessRegistry {
    processes: Mutex<HashMap<String, Arc<McpProcess>>>,
}

impl McpProcessRegistry {
    pub fn new() -> Self {
        Self {
            processes: Mutex::new(HashMap::new()),
        }
    }

    pub async fn stop_all(&self) {
        let mut procs = self.processes.lock().await;
        for (id, proc) in procs.drain() {
            tracing::info!("[MCP] Stopping server {}", id);
            proc.reader_task.abort();
            proc.stderr_task.abort();
            // Reject all pending waiters
            {
                let mut pending = proc.pending.lock().await;
                for (_, tx) in pending.drain() {
                    let _ = tx.send(Err("MCP server shutting down".to_string()));
                }
            }
            let _ = proc.child.lock().await.kill().await;
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Stdout Reader Task
// ═══════════════════════════════════════════════════════════════════════════

/// Background task that reads lines from MCP server stdout and dispatches
/// JSON-RPC responses to the corresponding pending waiter by request ID.
/// Notifications (no "id" field) are silently skipped.
async fn stdout_reader_loop(
    mut reader: BufReader<tokio::process::ChildStdout>,
    pending: PendingMap,
    server_id: String,
) {
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => {
                tracing::info!("[MCP:{}] stdout closed", server_id);
                break;
            }
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                match serde_json::from_str::<Value>(trimmed) {
                    Ok(val) => {
                        if let Some(id) = val.get("id").and_then(|v| v.as_u64()) {
                            // This is a response — find and notify the waiter
                            let tx = {
                                let mut map = pending.lock().await;
                                map.remove(&id)
                            };
                            if let Some(tx) = tx {
                                // Check for error
                                if let Some(error) = val.get("error") {
                                    let msg = error
                                        .get("message")
                                        .and_then(|m| m.as_str())
                                        .unwrap_or("Unknown MCP error");
                                    let _ = tx.send(Err(format!("MCP error: {}", msg)));
                                } else {
                                    let result =
                                        val.get("result").cloned().unwrap_or(Value::Null);
                                    let _ = tx.send(Ok(result));
                                }
                            } else {
                                tracing::warn!(
                                    "[MCP:{}] Received response for unknown request id {}",
                                    server_id,
                                    id
                                );
                            }
                        }
                        // Notifications (no id) are silently skipped
                    }
                    Err(e) => {
                        tracing::debug!(
                            "[MCP:{}] Non-JSON line from stdout: {} — {}",
                            server_id,
                            e,
                            &trimmed[..trimmed.len().min(100)]
                        );
                    }
                }
            }
            Err(e) => {
                tracing::warn!("[MCP:{}] stdout read error: {}", server_id, e);
                break;
            }
        }
    }

    // Reader exiting — reject all remaining pending waiters
    let mut map = pending.lock().await;
    for (_, tx) in map.drain() {
        let _ = tx.send(Err("MCP server closed stdout".to_string()));
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Commands
// ═══════════════════════════════════════════════════════════════════════════

/// Spawn an MCP stdio server process. Returns a runtime server ID.
#[tauri::command]
pub async fn mcp_spawn_server(
    state: State<'_, Arc<McpProcessRegistry>>,
    command: String,
    args: Vec<String>,
    env: HashMap<String, String>,
) -> Result<String, String> {
    let server_id = format!("mcp-{}", uuid::Uuid::new_v4());

    let mut cmd = Command::new(&command);
    cmd.args(&args)
        .envs(&env)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn MCP server '{}': {}", command, e))?;

    let stdin = child.stdin.take().ok_or("Failed to capture stdin")?;
    let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;

    // Log stderr in background — tracked so we can cancel on cleanup
    let stderr_task = if let Some(stderr) = child.stderr.take() {
        let sid = server_id.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break,
                    Ok(_) => tracing::debug!("[MCP:{}] stderr: {}", sid, line.trim_end()),
                    Err(_) => break,
                }
            }
        })
    } else {
        tokio::spawn(async {})
    };

    // Pending response map — shared between writer (registers) and reader (dispatches)
    let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));

    // Spawn the stdout reader task
    let reader_task = {
        let pending_clone = Arc::clone(&pending);
        let sid = server_id.clone();
        tokio::spawn(stdout_reader_loop(
            BufReader::new(stdout),
            pending_clone,
            sid,
        ))
    };

    let proc = Arc::new(McpProcess {
        child: Mutex::new(child),
        stdin: Mutex::new(stdin),
        next_id: AtomicU64::new(1),
        pending,
        reader_task,
        stderr_task,
    });

    state.processes.lock().await.insert(server_id.clone(), proc);
    tracing::info!("[MCP] Spawned server '{}' as {}", command, server_id);

    Ok(server_id)
}

/// Send a JSON-RPC request to an MCP server and return the result.
///
/// Concurrent requests to the same server are now supported:
/// - Stdin writes are serialized (short critical section)
/// - Response reading is done by a background task
/// - Each caller waits on its own oneshot channel, keyed by request ID
///
/// `params` is a JSON string to avoid Tauri serde issues with generic Value.
#[tauri::command]
pub async fn mcp_send_request(
    state: State<'_, Arc<McpProcessRegistry>>,
    server_id: String,
    method: String,
    params: String,
) -> Result<Value, String> {
    // Clone the Arc so we can release the registry lock immediately
    let proc = {
        let procs = state.processes.lock().await;
        procs
            .get(&server_id)
            .cloned()
            .ok_or_else(|| format!("MCP server {} not found", server_id))?
    };

    // Parse params — return error instead of silently falling back to null
    let params_value: Value =
        serde_json::from_str(&params).map_err(|e| format!("Invalid MCP params JSON: {}", e))?;

    let is_notification = method.starts_with("notifications/");

    // Allocate request ID (lock-free)
    let request_id = proc.next_id.fetch_add(1, Ordering::Relaxed);

    // Build JSON-RPC request
    let request = if is_notification {
        serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params_value,
        })
    } else {
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": method,
            "params": params_value,
        })
    };

    let request_str = serde_json::to_string(&request).map_err(|e| e.to_string())?;

    // For non-notifications, register the waiter BEFORE writing to stdin
    // to avoid a race where the reader task dispatches before we register.
    let rx = if !is_notification {
        let (tx, rx) = oneshot::channel();
        proc.pending.lock().await.insert(request_id, tx);
        Some(rx)
    } else {
        None
    };

    // Write to stdin — short critical section, only serializes the write itself
    {
        let mut stdin = proc.stdin.lock().await;
        stdin
            .write_all(request_str.as_bytes())
            .await
            .map_err(|e| format!("Failed to write to MCP server: {}", e))?;
        stdin
            .write_all(b"\n")
            .await
            .map_err(|e| format!("Failed to write newline: {}", e))?;
        stdin
            .flush()
            .await
            .map_err(|e| format!("Failed to flush: {}", e))?;
    }
    // stdin lock released here — other requests can write immediately

    // For notifications, return immediately
    if is_notification {
        return Ok(Value::Null);
    }

    // Wait for the response from the reader task (with timeout)
    let rx = rx.unwrap(); // Safe: we created it above for non-notifications
    match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => {
            // oneshot dropped — reader task died or server closed
            Err(format!("MCP server {} connection lost", server_id))
        }
        Err(_) => {
            // Timeout — clean up the pending entry
            proc.pending.lock().await.remove(&request_id);
            Err(format!("MCP server {} timed out (30s)", server_id))
        }
    }
}

/// Close an MCP server process.
#[tauri::command]
pub async fn mcp_close_server(
    state: State<'_, Arc<McpProcessRegistry>>,
    server_id: String,
) -> Result<(), String> {
    let proc = {
        let mut procs = state.processes.lock().await;
        procs.remove(&server_id)
    };
    if let Some(proc) = proc {
        tracing::info!("[MCP] Closing server {}", server_id);
        // Send shutdown notification, wait briefly, then force kill
        {
            let mut stdin = proc.stdin.lock().await;
            let shutdown = b"{\"jsonrpc\":\"2.0\",\"method\":\"shutdown\"}\n";
            let _ = stdin.write_all(shutdown).await;
            let _ = stdin.flush().await;
        }
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            proc.child.lock().await.wait(),
        )
        .await;
        let _ = proc.child.lock().await.kill().await;
        proc.reader_task.abort();
        proc.stderr_task.abort();
        // Reject all remaining pending waiters
        {
            let mut pending = proc.pending.lock().await;
            for (_, tx) in pending.drain() {
                let _ = tx.send(Err("MCP server closed".to_string()));
            }
        }
    }
    Ok(())
}
