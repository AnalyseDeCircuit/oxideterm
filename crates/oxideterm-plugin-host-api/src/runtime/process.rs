//! Stdio process runtime bridge for native plugins.
//!
//! The host-api crate owns process startup, protocol frame IO, returnable host
//! calls, and lifecycle timeout handling so the GPUI workspace only reacts to
//! validated runtime effects.

use super::*;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub struct NativeProcessPluginRuntime {
    plugin_dir: PathBuf,
    entry: String,
    pub(super) supervisor: PluginRuntimeSupervisorState,
    pub(super) child: Option<Child>,
    stdin: Option<ChildStdin>,
    stdout: Option<BufReader<ChildStdout>>,
    outbound_messages: VecDeque<PluginOutboundMessage>,
    outbound_effects: VecDeque<PluginOutboundEffect>,
    host_call_handler: Option<PluginHostCallHandler>,
}

impl NativeProcessPluginRuntime {
    pub fn new(
        plugin_id: impl Into<String>,
        plugin_dir: impl Into<PathBuf>,
        entry: impl Into<String>,
        lifecycle_timeout: Duration,
    ) -> Self {
        Self {
            plugin_dir: plugin_dir.into(),
            entry: entry.into(),
            supervisor: PluginRuntimeSupervisorState::new(plugin_id, lifecycle_timeout),
            child: None,
            stdin: None,
            stdout: None,
            outbound_messages: VecDeque::new(),
            outbound_effects: VecDeque::new(),
            host_call_handler: None,
        }
    }

    pub fn set_host_call_handler(&mut self, handler: PluginHostCallHandler) {
        // Returnable host calls are optional at the transport layer. Existing
        // Phase 3 plugins that only emit one-way effects keep the same behavior,
        // while Phase 4 APIs such as storage.get can opt into request/response.
        self.host_call_handler = Some(handler);
    }

    pub fn drain_outbound_messages(&mut self) -> Vec<PluginOutboundMessage> {
        // Tauri ctx registration calls mutate the plugin store during
        // activate(). Native process plugins emit the equivalent mutations as
        // host-owned outbound frames; the Workspace registry applies them after
        // the transport validates ownership and protocol shape.
        self.outbound_messages.drain(..).collect()
    }

    pub fn drain_outbound_effects(&mut self) -> Vec<PluginOutboundEffect> {
        // Effects are the transport-neutral handoff for WorkspaceApp. They let
        // Phase 4 host APIs such as ui.showToast attach without re-reading or
        // re-validating raw process stdout.
        self.outbound_effects.drain(..).collect()
    }

    fn start_process<'a>(&'a mut self) -> PluginRuntimeFuture<'a, ()> {
        Box::pin(async move {
            let executable = resolve_process_runtime_entry(&self.plugin_dir, &self.entry)?;
            self.supervisor.start_activation();
            // Process plugins communicate over host-owned stdio. Do not inherit
            // workspace stdio, because plugin logs/results must be captured and
            // classified by the runtime bridge instead of leaking to the app.
            let mut command = tokio::process::Command::new(executable);
            command
                .current_dir(&self.plugin_dir)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true);
            #[cfg(windows)]
            {
                // Windows GUI launches must not flash a console window for
                // process-backed plugins; stdio remains captured via pipes.
                command.creation_flags(CREATE_NO_WINDOW);
            }
            let mut child = command.spawn().map_err(|error| {
                PluginError::runtime(
                    "process_spawn_failed",
                    format!("Cannot start native plugin process: {error}"),
                )
            })?;
            let stdin = child.stdin.take().ok_or_else(|| {
                PluginError::runtime(
                    "process_stdin_unavailable",
                    "Native plugin process did not expose stdin",
                )
            })?;
            let stdout = child.stdout.take().ok_or_else(|| {
                PluginError::runtime(
                    "process_stdout_unavailable",
                    "Native plugin process did not expose stdout",
                )
            })?;
            self.child = Some(child);
            self.stdin = Some(stdin);
            self.stdout = Some(BufReader::new(stdout));
            Ok(())
        })
    }

    fn stop_process<'a>(&'a mut self) -> PluginRuntimeFuture<'a, PluginResponse> {
        Box::pin(async move {
            self.supervisor.start_deactivation();
            self.stdin.take();
            self.stdout.take();
            if let Some(mut child) = self.child.take() {
                let _ = child.kill().await;
            }
            self.supervisor.kill();
            Ok(PluginResponse::ok(
                "process.kill",
                serde_json::json!({ "state": "killed" }),
            ))
        })
    }

    fn call_process_request<'a>(
        &'a mut self,
        request: PluginRequest,
    ) -> PluginRuntimeFuture<'a, PluginResponse> {
        Box::pin(async move {
            let request_id = request.request_id.clone();
            let timeout = request
                .timeout_ms
                .map(Duration::from_millis)
                .unwrap_or_else(|| self.supervisor.lifecycle_timeout());
            self.write_process_request(request).await?;
            self.read_process_response(&request_id, timeout).await
        })
    }

    async fn write_process_request(&mut self, request: PluginRequest) -> Result<(), PluginError> {
        let stdin = self.stdin.as_mut().ok_or_else(|| {
            PluginError::runtime(
                "process_stdin_closed",
                "Native plugin process stdin is not available",
            )
        })?;
        let envelope = PluginProtocolEnvelope::new(Some(request.request_id.clone()), request);
        let mut line = serde_json::to_vec(&envelope).map_err(|error| {
            PluginError::protocol(
                "process_request_encode_failed",
                format!("Cannot encode native plugin request: {error}"),
            )
        })?;
        line.push(b'\n');
        stdin.write_all(&line).await.map_err(|error| {
            PluginError::runtime(
                "process_request_write_failed",
                format!("Cannot write native plugin request: {error}"),
            )
        })?;
        stdin.flush().await.map_err(|error| {
            PluginError::runtime(
                "process_request_flush_failed",
                format!("Cannot flush native plugin request: {error}"),
            )
        })
    }

    async fn write_process_host_call_response(
        &mut self,
        response: PluginResponse,
    ) -> Result<(), PluginError> {
        let stdin = self.stdin.as_mut().ok_or_else(|| {
            PluginError::runtime(
                "process_stdin_closed",
                "Native plugin process stdin is not available",
            )
        })?;
        let envelope = PluginProtocolEnvelope::new(Some(response.request_id.clone()), response);
        let mut line = serde_json::to_vec(&envelope).map_err(|error| {
            PluginError::protocol(
                "process_host_response_encode_failed",
                format!("Cannot encode native plugin host-call response: {error}"),
            )
        })?;
        line.push(b'\n');
        stdin.write_all(&line).await.map_err(|error| {
            PluginError::runtime(
                "process_host_response_write_failed",
                format!("Cannot write native plugin host-call response: {error}"),
            )
        })?;
        stdin.flush().await.map_err(|error| {
            PluginError::runtime(
                "process_host_response_flush_failed",
                format!("Cannot flush native plugin host-call response: {error}"),
            )
        })
    }

    async fn read_process_response(
        &mut self,
        request_id: &str,
        timeout: Duration,
    ) -> Result<PluginResponse, PluginError> {
        let started_at = Instant::now();
        let mut line = String::new();
        loop {
            let remaining = timeout.checked_sub(started_at.elapsed()).ok_or_else(|| {
                PluginError::runtime(
                    "process_response_timeout",
                    format!(
                        "Native plugin process did not respond within {}ms",
                        timeout.as_millis()
                    ),
                )
            })?;
            line.clear();
            let read = {
                let stdout = self.stdout.as_mut().ok_or_else(|| {
                    PluginError::runtime(
                        "process_stdout_closed",
                        "Native plugin process stdout is not available",
                    )
                })?;
                time::timeout(remaining, stdout.read_line(&mut line))
                    .await
                    .map_err(|_| {
                        PluginError::runtime(
                            "process_response_timeout",
                            format!(
                                "Native plugin process did not respond within {}ms",
                                timeout.as_millis()
                            ),
                        )
                    })?
                    .map_err(|error| {
                        PluginError::runtime(
                            "process_response_read_failed",
                            format!("Cannot read native plugin response: {error}"),
                        )
                    })?
            };
            if read == 0 {
                return Err(PluginError::runtime(
                    "process_exited",
                    "Native plugin process closed stdout before responding",
                ));
            }
            match decode_process_output_frame(&line)? {
                PluginProcessFrame::Response {
                    envelope_request_id,
                    response,
                } => {
                    if envelope_request_id.as_deref() != Some(request_id)
                        || response.request_id != request_id
                    {
                        return Err(PluginError::protocol(
                            "process_response_request_mismatch",
                            format!(
                                "Native plugin response request id mismatch; expected \"{request_id}\""
                            ),
                        ));
                    }
                    return Ok(response);
                }
                PluginProcessFrame::Outbound(message) => {
                    let effect = self
                        .supervisor
                        .handle_outbound_message(message.clone())
                        .map_err(|error| {
                            PluginError::protocol(
                                "process_outbound_rejected",
                                format!("Native plugin outbound frame rejected: {}", error.message),
                            )
                        })?;
                    let host_call_response = self.returnable_host_call_response(&message);
                    self.outbound_messages.push_back(message);
                    self.outbound_effects.push_back(effect);
                    if let Some(response) = host_call_response {
                        self.write_process_host_call_response(response).await?;
                    }
                }
            }
        }
    }

    fn returnable_host_call_response(
        &self,
        message: &PluginOutboundMessage,
    ) -> Option<PluginResponse> {
        let PluginOutboundMessage::CallHostApi {
            request_id,
            namespace,
            method,
            args,
        } = message
        else {
            return None;
        };
        let handler = self.host_call_handler.as_ref()?;
        handler(PluginHostCall {
            request_id: request_id.clone(),
            namespace: namespace.clone(),
            method: method.clone(),
            args: args.clone(),
        })
    }

    fn record_runtime_error(&mut self, error: PluginError) -> PluginError {
        self.supervisor.record_error(error.clone());
        error
    }
}

impl PluginRuntimeBridge for NativeProcessPluginRuntime {
    fn activate<'a>(
        &'a mut self,
        request: PluginActivateRequest,
    ) -> PluginRuntimeFuture<'a, PluginResponse> {
        Box::pin(async move {
            if let Err(error) = self.start_process().await {
                return Err(self.record_runtime_error(error));
            }
            let request_id = request.request_id.clone();
            let timeout_ms = request.timeout_ms;
            let response = self
                .call_process_request(PluginRequest {
                    request_id: request.request_id,
                    kind: PluginRequestKind::Activate {
                        manifest: request.manifest,
                        permissions: request.permissions,
                    },
                    timeout_ms: Some(timeout_ms),
                })
                .await;
            match response {
                Ok(response) => {
                    if matches!(response.result, PluginResponseResult::Ok { .. }) {
                        self.supervisor.mark_active();
                    } else if let PluginResponseResult::Error { error } = &response.result {
                        let error = error.clone();
                        self.stop_process().await.ok();
                        self.supervisor.record_error(error);
                    }
                    Ok(response)
                }
                Err(error) => {
                    self.stop_process().await.ok();
                    Err(self.record_runtime_error(PluginError::runtime(
                        error.code,
                        format!(
                            "Plugin activate request \"{request_id}\" failed: {}",
                            error.message
                        ),
                    )))
                }
            }
        })
    }

    fn deactivate<'a>(&'a mut self) -> PluginRuntimeFuture<'a, PluginResponse> {
        self.stop_process()
    }

    fn call<'a>(&'a mut self, request: PluginRequest) -> PluginRuntimeFuture<'a, PluginResponse> {
        Box::pin(async move {
            match self.call_process_request(request).await {
                Ok(response) => Ok(response),
                Err(error) => Err(self.record_runtime_error(error)),
            }
        })
    }

    fn send_event<'a>(&'a mut self, event: PluginEvent) -> PluginRuntimeFuture<'a, PluginResponse> {
        Box::pin(async move {
            let request_id = format!("event:{}", event.name);
            match self
                .call_process_request(PluginRequest {
                    request_id,
                    kind: PluginRequestKind::SendEvent { event },
                    timeout_ms: None,
                })
                .await
            {
                Ok(response) => Ok(response),
                Err(error) => Err(self.record_runtime_error(error)),
            }
        })
    }

    fn kill<'a>(&'a mut self) -> PluginRuntimeFuture<'a, PluginResponse> {
        self.stop_process()
    }

    fn health<'a>(&'a mut self) -> PluginRuntimeFuture<'a, PluginRuntimeHealth> {
        Box::pin(async move { Ok(self.supervisor.health()) })
    }
}

#[derive(Clone, Debug, PartialEq)]
enum PluginProcessFrame {
    Response {
        envelope_request_id: Option<String>,
        response: PluginResponse,
    },
    Outbound(PluginOutboundMessage),
}

fn decode_process_output_frame(line: &str) -> Result<PluginProcessFrame, PluginError> {
    let envelope: PluginProtocolEnvelope<Value> = serde_json::from_str(line).map_err(|error| {
        PluginError::protocol(
            "process_output_decode_failed",
            format!("Cannot decode native plugin process output: {error}"),
        )
    })?;
    envelope.validate_version()?;

    // The stdio transport is line-oriented and intentionally keeps response
    // frames and spontaneous plugin->host frames in the same versioned envelope.
    // That matches Tauri's activate-time ctx registration semantics without
    // allowing arbitrary JS callbacks to run inside native render paths.
    if envelope.payload.get("result").is_some() && envelope.payload.get("requestId").is_some() {
        let response =
            serde_json::from_value::<PluginResponse>(envelope.payload).map_err(|error| {
                PluginError::protocol(
                    "process_response_decode_failed",
                    format!("Cannot decode native plugin response: {error}"),
                )
            })?;
        return Ok(PluginProcessFrame::Response {
            envelope_request_id: envelope.request_id,
            response,
        });
    }

    if envelope.payload.get("type").is_some() {
        let message =
            serde_json::from_value::<PluginOutboundMessage>(envelope.payload).map_err(|error| {
                PluginError::protocol(
                    "process_outbound_decode_failed",
                    format!("Cannot decode native plugin outbound frame: {error}"),
                )
            })?;
        return Ok(PluginProcessFrame::Outbound(message));
    }

    Err(PluginError::protocol(
        "process_output_unknown_payload",
        "Native plugin process output is neither a response nor an outbound message",
    ))
}
