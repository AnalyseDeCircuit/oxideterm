struct AgentTransport {
    write_tx: mpsc::Sender<String>,
    pending: PendingMap,
    shutdown_tx: mpsc::Sender<()>,
    alive: Arc<AtomicBool>,
}

impl AgentTransport {
    async fn new(
        mut channel: russh::Channel<russh::client::Msg>,
        agent_command: &str,
    ) -> Result<Self, AgentError> {
        channel
            .exec(true, agent_command)
            .await
            .map_err(|error| AgentError::Ssh(format!("Failed to exec agent: {error}")))?;

        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let alive = Arc::new(AtomicBool::new(true));
        let (write_tx, mut write_rx) = mpsc::channel::<String>(256);
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);

        let pending_for_task = pending.clone();
        let alive_for_task = alive.clone();
        tokio::spawn(async move {
            let mut buffer = String::new();
            loop {
                tokio::select! {
                    Some(line) = write_rx.recv() => {
                        let data = format!("{line}\n");
                        if channel.data(data.as_bytes()).await.is_err() {
                            warn!("[ide-agent] write failed; channel closed");
                            break;
                        }
                    }
                    message = channel.wait() => {
                        match message {
                            Some(ChannelMsg::Data { data }) => {
                                buffer.push_str(&String::from_utf8_lossy(&data));
                                while let Some(newline) = buffer.find('\n') {
                                    let line = buffer[..newline].trim().to_string();
                                    buffer = buffer[newline + 1..].to_string();
                                    if line.is_empty() {
                                        continue;
                                    }
                                    handle_agent_line(&pending_for_task, &line).await;
                                }
                            }
                            Some(ChannelMsg::ExtendedData { data, ext: 1 }) => {
                                for line in String::from_utf8_lossy(&data).lines() {
                                    debug!("[ide-agent-stderr] {line}");
                                }
                            }
                            Some(ChannelMsg::ExitStatus { exit_status }) => {
                                info!("[ide-agent] exited with status {exit_status}");
                                break;
                            }
                            Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => break,
                            _ => {}
                        }
                    }
                    _ = shutdown_rx.recv() => break,
                }
            }

            alive_for_task.store(false, Ordering::Relaxed);
            let mut pending = pending_for_task.lock().await;
            for (_, tx) in pending.drain() {
                let _ = tx.send(Err(AgentRpcError {
                    code: -32603,
                    message: "Agent channel closed".to_string(),
                }));
            }
        });

        Ok(Self {
            write_tx,
            pending,
            shutdown_tx,
            alive,
        })
    }

    fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }

    async fn call(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, AgentError> {
        self.call_with_timeout(method, params, AGENT_RPC_TIMEOUT_SECS)
            .await
    }

    async fn call_with_timeout(
        &self,
        method: &str,
        params: serde_json::Value,
        timeout_secs: u64,
    ) -> Result<serde_json::Value, AgentError> {
        if !self.is_alive() {
            return Err(AgentError::ChannelClosed);
        }

        let id = NEXT_AGENT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        let request = AgentRequest {
            id,
            method: method.to_string(),
            params,
        };
        let json = serde_json::to_string(&request)
            .map_err(|error| AgentError::Serialize(error.to_string()))?;

        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        self.write_tx
            .send(json)
            .await
            .map_err(|_| AgentError::ChannelClosed)?;

        match tokio::time::timeout(Duration::from_secs(timeout_secs), rx).await {
            Ok(Ok(Ok(value))) => Ok(value),
            Ok(Ok(Err(error))) => Err(AgentError::from(error)),
            Ok(Err(_)) => Err(AgentError::ChannelClosed),
            Err(_) => {
                self.pending.lock().await.remove(&id);
                Err(AgentError::Timeout(timeout_secs))
            }
        }
    }

    async fn shutdown(&self) {
        let _ = self
            .call_with_timeout("sys/shutdown", serde_json::json!({}), 5)
            .await;
        let _ = self.shutdown_tx.send(()).await;
    }
}

async fn handle_agent_line(pending: &PendingMap, line: &str) {
    match serde_json::from_str::<AgentMessage>(line) {
        Ok(AgentMessage::Response(response)) => {
            let mut pending = pending.lock().await;
            if let Some(tx) = pending.remove(&response.id) {
                let result = if let Some(error) = response.error {
                    Err(error)
                } else {
                    Ok(response.result.unwrap_or_default())
                };
                let _ = tx.send(result);
            }
        }
        Ok(AgentMessage::Notification(notification)) => {
            debug!(
                "[ide-agent] notification {} {}",
                notification.method, notification.params
            );
        }
        Err(error) => debug!("[ide-agent] ignored non-JSON line: {line} ({error})"),
    }
}
