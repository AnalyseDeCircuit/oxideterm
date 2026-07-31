#[cfg(windows)]
const MCP_STDIO_CREATE_NO_WINDOW: u32 = 0x08000000;

struct McpStdioRequestGuard {
    process: Arc<McpProcess>,
    request_id: u64,
    framing: McpStdioFraming,
    notify_server: bool,
    armed: bool,
}

struct McpPendingStdioRequest {
    request_id: u64,
    server_id: String,
    receiver: oneshot::Receiver<Result<Value, McpError>>,
    cancellation: McpStdioRequestGuard,
}

impl McpPendingStdioRequest {
    async fn wait(mut self) -> Result<Value, McpError> {
        let result = self.receiver.await;
        self.cancellation.disarm();
        result.unwrap_or_else(|_| {
            Err(McpError::Message(format!(
                "MCP server {} connection lost",
                self.server_id
            )))
        })
    }
}

impl McpStdioRequestGuard {
    fn new(
        process: Arc<McpProcess>,
        request_id: u64,
        framing: McpStdioFraming,
        notify_server: bool,
    ) -> Self {
        Self {
            process,
            request_id,
            framing,
            notify_server,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for McpStdioRequestGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let process = self.process.clone();
        let request_id = self.request_id;
        let framing = self.framing;
        let notify_server = self.notify_server;
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                // Only notify the server if this request was still pending when
                // its caller stopped waiting for the result.
                if process.pending.lock().await.remove(&request_id).is_none() {
                    return;
                }
                if !notify_server {
                    return;
                }
                let notification = serde_json::json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/cancelled",
                    "params": {
                        "requestId": request_id,
                        "reason": "OxideTerm stopped waiting for the request"
                    }
                });
                let Ok(body) = serde_json::to_string(&notification) else {
                    return;
                };
                let mut stdin = process.stdin.lock().await;
                let _ = match framing {
                    McpStdioFraming::LineDelimited => write_line_message(&mut *stdin, &body).await,
                    McpStdioFraming::LegacyContentLength => {
                        write_framed_message(&mut *stdin, &body).await
                    }
                };
            });
        }
    }
}

impl McpProcessRegistry {
    async fn stop_all(&self) {
        #[cfg(test)]
        self.stop_all_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let ids = self
            .processes
            .lock()
            .await
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for id in ids {
            let _ = self.close(&id).await;
        }
    }

    async fn spawn(
        &self,
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
    ) -> Result<String, McpError> {
        validate_mcp_command(command)?;
        validate_mcp_env(env)?;
        let server_id = format!("mcp-{}", uuid::Uuid::new_v4());

        let mut cmd = Command::new(command);
        configure_mcp_stdio_command(&mut cmd);
        cmd.args(args)
            .env_clear()
            .envs(env)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        if let Ok(path) = std::env::var("PATH") {
            cmd.env("PATH", path);
        }
        if let Ok(home) = std::env::var("HOME") {
            cmd.env("HOME", home);
        }

        let mut child = cmd.spawn().map_err(|error| {
            McpError::Message(format!("Failed to spawn MCP server '{command}': {error}"))
        })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| McpError::Message("Failed to capture stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| McpError::Message("Failed to capture stdout".to_string()))?;
        let stderr_task = if let Some(stderr) = child.stderr.take() {
            let sid = server_id.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) | Err(_) => break,
                        Ok(_) => tracing::debug!("[MCP:{sid}] stderr: {}", line.trim_end()),
                    }
                }
            })
        } else {
            tokio::spawn(async {})
        };

        let pending = Arc::new(Mutex::new(HashMap::new()));
        let (notifications, _) = broadcast::channel(128);
        let reader_task = {
            let pending = pending.clone();
            let notifications = notifications.clone();
            let sid = server_id.clone();
            tokio::spawn(stdout_reader_loop(
                BufReader::new(stdout),
                pending,
                notifications,
                sid,
            ))
        };
        self.processes.lock().await.insert(
            server_id.clone(),
            Arc::new(McpProcess {
                child: Mutex::new(child),
                stdin: Mutex::new(stdin),
                next_id: AtomicU64::new(1),
                pending,
                notifications,
                reader_task,
                stderr_task,
                eof_shutdown: AtomicBool::new(false),
            }),
        );
        Ok(server_id)
    }

    async fn subscribe_notifications(
        &self,
        server_id: &str,
    ) -> Result<broadcast::Receiver<Value>, McpError> {
        self.processes
            .lock()
            .await
            .get(server_id)
            .map(|process| process.notifications.subscribe())
            .ok_or_else(|| McpError::Message(format!("MCP server {server_id} not found")))
    }

    async fn send_protocol_request(
        &self,
        server_id: &str,
        protocol: &McpProtocol,
        method: &str,
        params: Value,
    ) -> Result<Value, McpError> {
        let framing = protocol.stdio_framing;
        self.send_request_with_framing(server_id, method, params, MCP_REQUEST_TIMEOUT, framing)
            .await
    }

    async fn begin_protocol_request_until_completion(
        &self,
        server_id: &str,
        protocol: &McpProtocol,
        method: &str,
        params: Value,
    ) -> Result<McpPendingStdioRequest, McpError> {
        let framing = protocol.stdio_framing;
        let process = self
            .processes
            .lock()
            .await
            .get(server_id)
            .cloned()
            .ok_or_else(|| McpError::Message(format!("MCP server {server_id} not found")))?;
        let request_id = process.next_id.fetch_add(1, Ordering::Relaxed);
        let request = serde_json::json!({ "jsonrpc": "2.0", "id": request_id, "method": method, "params": params });
        let body = serde_json::to_string(&request)
            .map_err(|error| McpError::Message(error.to_string()))?;
        let (sender, receiver) = oneshot::channel();
        process.pending.lock().await.insert(request_id, sender);
        let write_result = {
            let mut stdin = process.stdin.lock().await;
            match framing {
                McpStdioFraming::LineDelimited => write_line_message(&mut *stdin, &body).await,
                McpStdioFraming::LegacyContentLength => {
                    write_framed_message(&mut *stdin, &body).await
                }
            }
        };
        if let Err(error) = write_result {
            process.pending.lock().await.remove(&request_id);
            return Err(error);
        }
        Ok(McpPendingStdioRequest {
            request_id,
            server_id: server_id.to_string(),
            receiver,
            cancellation: McpStdioRequestGuard::new(process, request_id, framing, true),
        })
    }

    async fn send_request_with_framing(
        &self,
        server_id: &str,
        method: &str,
        params: Value,
        timeout: Duration,
        framing: McpStdioFraming,
    ) -> Result<Value, McpError> {
        self.send_request_with_optional_timeout(
            server_id,
            method,
            params,
            Some(timeout),
            framing,
            true,
        )
        .await
    }

    async fn send_probe_request_with_framing(
        &self,
        server_id: &str,
        method: &str,
        params: Value,
        timeout: Duration,
        framing: McpStdioFraming,
    ) -> Result<Value, McpError> {
        // A modern probe may be sent to a legacy parser. Do not append a
        // modern cancellation message when the probe times out.
        self.send_request_with_optional_timeout(
            server_id,
            method,
            params,
            Some(timeout),
            framing,
            false,
        )
        .await
    }

    async fn send_request_with_optional_timeout(
        &self,
        server_id: &str,
        method: &str,
        params: Value,
        timeout: Option<Duration>,
        framing: McpStdioFraming,
        notify_cancellation: bool,
    ) -> Result<Value, McpError> {
        let process = self
            .processes
            .lock()
            .await
            .get(server_id)
            .cloned()
            .ok_or_else(|| McpError::Message(format!("MCP server {server_id} not found")))?;
        let is_notification = method.starts_with("notifications/");
        let request_id = process.next_id.fetch_add(1, Ordering::Relaxed);
        let request = if is_notification {
            serde_json::json!({ "jsonrpc": "2.0", "method": method, "params": params })
        } else {
            serde_json::json!({ "jsonrpc": "2.0", "id": request_id, "method": method, "params": params })
        };
        let body = serde_json::to_string(&request)
            .map_err(|error| McpError::Message(error.to_string()))?;
        let rx = if is_notification {
            None
        } else {
            let (tx, rx) = oneshot::channel();
            process.pending.lock().await.insert(request_id, tx);
            Some(rx)
        };
        {
            let mut stdin = process.stdin.lock().await;
            let write_result = match framing {
                McpStdioFraming::LineDelimited => write_line_message(&mut *stdin, &body).await,
                McpStdioFraming::LegacyContentLength => {
                    write_framed_message(&mut *stdin, &body).await
                }
            };
            if let Err(error) = write_result {
                if !is_notification {
                    process.pending.lock().await.remove(&request_id);
                }
                return Err(error);
            }
        }
        let Some(rx) = rx else {
            return Ok(Value::Null);
        };
        let mut cancellation =
            McpStdioRequestGuard::new(process, request_id, framing, notify_cancellation);
        if let Some(timeout) = timeout {
            match tokio::time::timeout(timeout, rx).await {
                Ok(Ok(result)) => {
                    cancellation.disarm();
                    result
                }
                Ok(Err(_)) => {
                    cancellation.disarm();
                    Err(McpError::Message(format!(
                        "MCP server {server_id} connection lost"
                    )))
                }
                Err(_) => Err(McpError::Timeout(server_id.to_string())),
            }
        } else {
            let result = rx.await;
            cancellation.disarm();
            result.unwrap_or_else(|_| {
                Err(McpError::Message(format!(
                    "MCP server {server_id} connection lost"
                )))
            })
        }
    }

    async fn set_eof_shutdown(&self, server_id: &str, eof_shutdown: bool) {
        if let Some(process) = self.processes.lock().await.get(server_id) {
            process.eof_shutdown.store(eof_shutdown, Ordering::Release);
        }
    }

    async fn terminate_unnegotiated(&self, server_id: &str) {
        let process = self.processes.lock().await.remove(server_id);
        let Some(process) = process else {
            return;
        };
        // No protocol was negotiated, so shutdown traffic could use the wrong
        // framing and further confuse the subprocess.
        let _ = process.child.lock().await.kill().await;
        process.reader_task.abort();
        process.stderr_task.abort();
        for (_, sender) in process.pending.lock().await.drain() {
            let _ = sender.send(Err(McpError::Message(
                "MCP server was stopped during protocol negotiation".to_string(),
            )));
        }
    }

    async fn close(&self, server_id: &str) -> Result<(), McpError> {
        let process = self.processes.lock().await.remove(server_id);
        let Some(process) = process else {
            return Ok(());
        };
        let eof_shutdown = process.eof_shutdown.load(Ordering::Acquire);
        if !eof_shutdown {
            let id = process.next_id.fetch_add(1, Ordering::Relaxed);
            let (tx, rx) = oneshot::channel();
            process.pending.lock().await.insert(id, tx);
            let shutdown = format!(r#"{{"jsonrpc":"2.0","id":{id},"method":"shutdown"}}"#);
            let write_ok = {
                let mut stdin = process.stdin.lock().await;
                write_framed_message(&mut *stdin, &shutdown).await.is_ok()
            };
            if write_ok {
                let _ = tokio::time::timeout(MCP_SHUTDOWN_TIMEOUT, rx).await;
            } else {
                process.pending.lock().await.remove(&id);
            }
            {
                let mut stdin = process.stdin.lock().await;
                let _ =
                    write_framed_message(&mut *stdin, r#"{"jsonrpc":"2.0","method":"exit"}"#).await;
            }
        } else {
            // Standards-compliant MCP stdio uses EOF as its portable graceful
            // shutdown signal in both supported protocol eras.
            let _ = process.stdin.lock().await.shutdown().await;
            let exited = {
                let mut child = process.child.lock().await;
                matches!(
                    tokio::time::timeout(MCP_SHUTDOWN_TIMEOUT, child.wait()).await,
                    Ok(Ok(_))
                )
            };
            if !exited {
                let _ = process.child.lock().await.kill().await;
            }
        }
        if !eof_shutdown {
            let _ = process.child.lock().await.kill().await;
        }
        process.reader_task.abort();
        process.stderr_task.abort();
        for (_, tx) in process.pending.lock().await.drain() {
            let _ = tx.send(Err(McpError::Message("MCP server closed".to_string())));
        }
        Ok(())
    }
}

fn configure_mcp_stdio_command(command: &mut Command) {
    #[cfg(windows)]
    {
        // Stdio MCP servers are protocol children owned by the app. Hide their
        // console windows while keeping stdin/stdout/stderr pipes captured.
        command.creation_flags(MCP_STDIO_CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = command;
    }
}

impl Drop for McpProcessOwner {
    fn drop(&mut self) {
        // This owner is dropped only after all registry handles and process tasks release it.
        let processes = self.registry.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                processes.stop_all().await;
            });
        }
    }
}

async fn stdout_reader_loop<R>(
    mut reader: R,
    pending: PendingMap,
    notifications: broadcast::Sender<Value>,
    server_id: String,
) where
    R: AsyncBufRead + Unpin,
{
    let mut header_line = String::new();
    loop {
        header_line.clear();
        let bytes_read = match reader.read_line(&mut header_line).await {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        let trimmed = header_line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let body = if trimmed.starts_with('{') || trimmed.starts_with('[') {
            let _ = bytes_read;
            trimmed.to_string()
        } else {
            let mut headers = vec![trimmed.to_string()];
            let mut next = String::new();
            loop {
                next.clear();
                match reader.read_line(&mut next).await {
                    Ok(0) => break,
                    Ok(_) if next.trim().is_empty() => break,
                    Ok(_) => headers.push(next.trim().to_string()),
                    Err(_) => break,
                }
            }
            let Some(length) = headers.iter().find_map(|header| {
                let (name, value) = header.split_once(':')?;
                name.trim()
                    .eq_ignore_ascii_case("content-length")
                    .then_some(value.trim())
            }) else {
                break;
            };
            let Ok(length) = length.parse::<usize>() else {
                break;
            };
            if length == 0 || length > MAX_MCP_MESSAGE_BYTES {
                break;
            }
            let mut buf = vec![0u8; length];
            if tokio::io::AsyncReadExt::read_exact(&mut reader, &mut buf)
                .await
                .is_err()
            {
                break;
            }
            String::from_utf8_lossy(&buf).into_owned()
        };
        let Ok(value) = serde_json::from_str::<Value>(body.trim()) else {
            continue;
        };
        let Some(id) = value.get("id").and_then(Value::as_u64) else {
            tracing::debug!(
                "[MCP:{server_id}] notification: {}",
                value
                    .get("method")
                    .and_then(|value| value.as_str())
                    .unwrap_or("unknown")
            );
            let _ = notifications.send(value);
            continue;
        };
        let tx = pending.lock().await.remove(&id);
        if let Some(tx) = tx {
            if let Some(error) = value.get("error") {
                let _ = tx.send(Err(rpc_error_from_value(error)));
            } else if let Some(result) = value.get("result") {
                let _ = tx.send(Ok(result.clone()));
            } else {
                let _ = tx.send(Err(McpError::Message(
                    "MCP response missing result".to_string(),
                )));
            }
        }
    }
    for (_, tx) in pending.lock().await.drain() {
        let _ = tx.send(Err(McpError::Message(
            "MCP server closed stdout".to_string(),
        )));
    }
}

async fn write_framed_message<W>(writer: &mut W, body: &str) -> Result<(), McpError>
where
    W: AsyncWrite + Unpin,
{
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    writer.write_all(header.as_bytes()).await.map_err(|error| {
        McpError::Message(format!("Failed to write header to MCP server: {error}"))
    })?;
    writer
        .write_all(body.as_bytes())
        .await
        .map_err(|error| McpError::Message(format!("Failed to write to MCP server: {error}")))?;
    writer
        .flush()
        .await
        .map_err(|error| McpError::Message(format!("Failed to flush: {error}")))
}

async fn write_line_message<W>(writer: &mut W, body: &str) -> Result<(), McpError>
where
    W: AsyncWrite + Unpin,
{
    // Modern MCP stdio uses one compact JSON-RPC message per line.
    writer
        .write_all(body.as_bytes())
        .await
        .map_err(|error| McpError::Message(format!("Failed to write to MCP server: {error}")))?;
    writer
        .write_all(b"\n")
        .await
        .map_err(|error| McpError::Message(format!("Failed to write to MCP server: {error}")))?;
    writer
        .flush()
        .await
        .map_err(|error| McpError::Message(format!("Failed to flush: {error}")))
}
