impl McpProcessRegistry {
    async fn stop_all(&self) {
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
        let reader_task = {
            let pending = pending.clone();
            let sid = server_id.clone();
            tokio::spawn(stdout_reader_loop(BufReader::new(stdout), pending, sid))
        };
        self.processes.lock().await.insert(
            server_id.clone(),
            Arc::new(McpProcess {
                child: Mutex::new(child),
                stdin: Mutex::new(stdin),
                next_id: AtomicU64::new(1),
                pending,
                reader_task,
                stderr_task,
            }),
        );
        Ok(server_id)
    }

    async fn send_request(
        &self,
        server_id: &str,
        method: &str,
        params: Value,
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
            if let Err(error) = write_framed_message(&mut *stdin, &body).await {
                if !is_notification {
                    process.pending.lock().await.remove(&request_id);
                }
                return Err(error);
            }
        }
        let Some(rx) = rx else {
            return Ok(Value::Null);
        };
        match tokio::time::timeout(MCP_REQUEST_TIMEOUT, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(McpError::Message(format!(
                "MCP server {server_id} connection lost"
            ))),
            Err(_) => {
                process.pending.lock().await.remove(&request_id);
                Err(McpError::Timeout(server_id.to_string()))
            }
        }
    }

    async fn close(&self, server_id: &str) -> Result<(), McpError> {
        let process = self.processes.lock().await.remove(server_id);
        let Some(process) = process else {
            return Ok(());
        };
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
            let _ = write_framed_message(&mut *stdin, r#"{"jsonrpc":"2.0","method":"exit"}"#).await;
        }
        let _ = process.child.lock().await.kill().await;
        process.reader_task.abort();
        process.stderr_task.abort();
        for (_, tx) in process.pending.lock().await.drain() {
            let _ = tx.send(Err(McpError::Message("MCP server closed".to_string())));
        }
        Ok(())
    }
}

impl Drop for McpRegistry {
    fn drop(&mut self) {
        let processes = self.processes.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                processes.stop_all().await;
            });
        }
    }
}

async fn stdout_reader_loop<R>(mut reader: R, pending: PendingMap, server_id: String)
where
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
            continue;
        };
        let tx = pending.lock().await.remove(&id);
        if let Some(tx) = tx {
            if let Some(error) = value.get("error") {
                let message = error
                    .get("message")
                    .and_then(|value| value.as_str())
                    .unwrap_or("Unknown MCP error");
                let _ = tx.send(Err(McpError::Message(format!("MCP error: {message}"))));
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
