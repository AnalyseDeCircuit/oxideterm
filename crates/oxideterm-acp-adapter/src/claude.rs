// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Claude Code process launch and stream-json protocol handling.

use super::*;

pub(super) async fn stream_claude_code_provider(
    config: &AdapterConfig,
    active_runs: &ActiveRuns,
    session_id: SessionId,
    cwd: PathBuf,
    previous_session_id: Option<String>,
    selected_model: Option<String>,
    prompt: String,
    connection: ConnectionTo<Client>,
) -> Result<ProviderOutcome, agent_client_protocol::Error> {
    let mut command = Command::new(config.command.trim());
    command.current_dir(cwd);
    command.args([
        "-p",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
    ]);
    if let Some(previous_session_id) = previous_session_id {
        command.args(["--resume", previous_session_id.as_str()]);
    }
    if let Some(selected_model) = selected_model {
        // Claude accepts a session-selected model even though it cannot enumerate models.
        command.args(["--model", selected_model.as_str()]);
    }
    command.args(&config.extra_args);
    command.arg(prompt);
    command.kill_on_drop(true);
    command.stdin(ProcessStdio::null());
    command.stderr(ProcessStdio::piped());
    command.stdout(ProcessStdio::piped());

    let mut child = command
        .spawn()
        .map_err(agent_client_protocol::Error::into_internal_error)?;
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(async move {
            let mut stderr = stderr;
            let mut sink = tokio::io::sink();
            let _ = tokio::io::copy(&mut stderr, &mut sink).await;
        });
    }
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| agent_client_protocol::util::internal_error("provider stdout missing"))?;

    let run_id = Uuid::new_v4();
    let (cancel_tx, cancel_rx) = mpsc::unbounded_channel();
    {
        let previous = active_runs
            .lock()
            .expect("active ACP run lock")
            .insert(session_id.clone(), ActiveRun { run_id, cancel_tx });
        if let Some(previous) = previous {
            // A new prompt supersedes the previous process for the same ACP session.
            let _ = previous.cancel_tx.send(());
        }
    }

    let outcome = read_claude_stream_json_stdout(
        config,
        &session_id,
        &connection,
        &mut child,
        stdout,
        cancel_rx,
    )
    .await;
    cleanup_active_run(active_runs, &session_id, run_id);
    outcome
}

async fn read_claude_stream_json_stdout(
    config: &AdapterConfig,
    session_id: &SessionId,
    connection: &ConnectionTo<Client>,
    child: &mut Child,
    stdout: ChildStdout,
    mut cancel_rx: mpsc::UnboundedReceiver<()>,
) -> Result<ProviderOutcome, agent_client_protocol::Error> {
    let mut stdout = BufReader::new(stdout);
    let mut line = String::new();
    let mut claude_session_id = None;
    let mut resolved_model = None;

    loop {
        line.clear();
        tokio::select! {
            _ = cancel_rx.recv() => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Ok(ProviderOutcome {
                    stop_reason: StopReason::Cancelled,
                    claude_session_id,
                    codex_thread_id: None,
                    resolved_model,
                });
            }
            read_result = stdout.read_line(&mut line) => {
                let read_len = read_result.map_err(agent_client_protocol::Error::into_internal_error)?;
                if read_len == 0 {
                    break;
                }
                let value = serde_json::from_str::<Value>(line.trim_end())
                    .map_err(agent_client_protocol::Error::into_internal_error)?;
                if let Some(model) = claude_system_init_model(&value) {
                    resolved_model = Some(model.to_string());
                }
                if let Some(session_id) = handle_claude_stream_json_message(connection, session_id, &value)? {
                    claude_session_id = Some(session_id);
                }
            }
        }
    }

    let status = child
        .wait()
        .await
        .map_err(agent_client_protocol::Error::into_internal_error)?;
    if status.success() {
        Ok(ProviderOutcome {
            stop_reason: StopReason::EndTurn,
            claude_session_id,
            codex_thread_id: None,
            resolved_model,
        })
    } else {
        Err(agent_client_protocol::util::internal_error(format!(
            "{} command exited unsuccessfully",
            config.provider.agent_name()
        )))
    }
}

fn handle_claude_stream_json_message(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    value: &Value,
) -> Result<Option<String>, agent_client_protocol::Error> {
    let claude_session_id = value
        .get("session_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    match value.get("type").and_then(Value::as_str) {
        Some("stream_event") => handle_claude_stream_event(connection, session_id, value)?,
        Some("system") => handle_claude_system_event(connection, session_id, value)?,
        Some("error") => {
            if let Some(message) = value.get("message").and_then(Value::as_str) {
                emit_thought_chunk(connection, session_id, message)?;
            }
        }
        _ => {}
    }
    Ok(claude_session_id)
}

fn handle_claude_stream_event(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    value: &Value,
) -> Result<(), agent_client_protocol::Error> {
    let event = value.get("event").unwrap_or(&Value::Null);
    if let Some(delta) = event.get("delta") {
        match delta.get("type").and_then(Value::as_str) {
            Some("text_delta") => {
                if let Some(text) = delta.get("text").and_then(Value::as_str) {
                    emit_text_chunk(connection, session_id, text)?;
                }
            }
            Some("thinking_delta") => {
                if let Some(text) = delta.get("thinking").and_then(Value::as_str) {
                    emit_thought_chunk(connection, session_id, text)?;
                }
            }
            Some("input_json_delta") => {
                if let Some(delta) = delta.get("partial_json").and_then(Value::as_str) {
                    emit_claude_tool_delta(connection, session_id, event, delta)?;
                }
            }
            _ => {}
        }
    }
    match event.get("type").and_then(Value::as_str) {
        Some("content_block_start") => {
            emit_claude_content_block_start(connection, session_id, event)?
        }
        Some("content_block_stop") => {
            emit_claude_content_block_stop(connection, session_id, event)?
        }
        _ => {}
    }
    Ok(())
}

fn handle_claude_system_event(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    value: &Value,
) -> Result<(), agent_client_protocol::Error> {
    let subtype = value
        .get("subtype")
        .and_then(Value::as_str)
        .unwrap_or("system");
    match subtype {
        "init" => {
            if let Some(model) = claude_system_init_model(value) {
                // Claude Code reports the resolved runtime model only after the process starts.
                connection.send_notification(SessionNotification::new(
                    session_id.clone(),
                    SessionUpdate::ConfigOptionUpdate(ConfigOptionUpdate::new(
                        observed_model_config_options(model),
                    )),
                ))?;
            }
        }
        "api_retry" => {
            let attempt = value.get("attempt").and_then(Value::as_u64).unwrap_or(0);
            let max_retries = value
                .get("max_retries")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            emit_thought_chunk(
                connection,
                session_id,
                &format!("Claude Code API retry {attempt}/{max_retries}"),
            )?;
        }
        "plugin_install" => {
            if let Some(status) = value.get("status").and_then(Value::as_str) {
                emit_thought_chunk(connection, session_id, &format!("Claude plugin {status}"))?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn claude_system_init_model(value: &Value) -> Option<&str> {
    (value.get("type").and_then(Value::as_str) == Some("system")
        && value.get("subtype").and_then(Value::as_str) == Some("init"))
    .then(|| value.get("model").and_then(Value::as_str))
    .flatten()
    .map(str::trim)
    .filter(|model| !model.is_empty())
}

fn emit_claude_content_block_start(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    event: &Value,
) -> Result<(), agent_client_protocol::Error> {
    let Some(block) = event.get("content_block") else {
        return Ok(());
    };
    if block.get("type").and_then(Value::as_str) != Some("tool_use") {
        return Ok(());
    }
    let Some(tool_call_id) = block.get("id").and_then(Value::as_str) else {
        return Ok(());
    };
    let name = block
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Claude tool");
    connection.send_notification(SessionNotification::new(
        session_id.clone(),
        SessionUpdate::ToolCall(
            ToolCall::new(tool_call_id.to_string(), name.to_string())
                .kind(claude_tool_kind(name))
                .status(ToolCallStatus::InProgress)
                .raw_input(Some(block.clone())),
        ),
    ))?;
    Ok(())
}

fn emit_claude_content_block_stop(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    event: &Value,
) -> Result<(), agent_client_protocol::Error> {
    let Some(tool_call_id) = claude_tool_call_id_from_event(event) else {
        return Ok(());
    };
    connection.send_notification(SessionNotification::new(
        session_id.clone(),
        SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            tool_call_id,
            ToolCallUpdateFields::new().status(ToolCallStatus::Completed),
        )),
    ))?;
    Ok(())
}

fn emit_claude_tool_delta(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    event: &Value,
    delta: &str,
) -> Result<(), agent_client_protocol::Error> {
    let Some(tool_call_id) = claude_tool_call_id_from_event(event) else {
        return Ok(());
    };
    connection.send_notification(SessionNotification::new(
        session_id.clone(),
        SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            tool_call_id,
            ToolCallUpdateFields::new().content(Some(vec![delta.to_string().into()])),
        )),
    ))?;
    Ok(())
}

fn claude_tool_call_id_from_event(event: &Value) -> Option<String> {
    event
        .get("content_block")
        .and_then(|block| block.get("id"))
        .or_else(|| event.get("content_block_id"))
        .or_else(|| event.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn claude_tool_kind(name: &str) -> ToolKind {
    match name {
        "Bash" => ToolKind::Execute,
        "Edit" | "MultiEdit" | "Write" | "NotebookEdit" => ToolKind::Edit,
        "Grep" | "Glob" | "WebSearch" => ToolKind::Search,
        "Read" | "LS" => ToolKind::Read,
        _ => ToolKind::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_init_reports_the_resolved_claude_model() {
        assert_eq!(
            claude_system_init_model(&json!({
                "type": "system",
                "subtype": "init",
                "model": "claude-sonnet-4-6",
            })),
            Some("claude-sonnet-4-6")
        );
        assert_eq!(
            claude_system_init_model(&json!({
                "type": "system",
                "subtype": "api_retry",
                "model": "claude-sonnet-4-6",
            })),
            None
        );
    }
}
