//! Stdio process runtime bridge for native plugins.
//!
//! The host-api crate owns process startup, protocol frame IO, returnable host
//! calls, and lifecycle timeout handling so the GPUI workspace only reacts to
//! validated runtime effects.

use super::*;
use tokio::sync::{Notify, mpsc};

const PROCESS_OUTPUT_QUEUE_CAPACITY: usize = 32;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub struct NativeProcessPluginRuntime {
    plugin_dir: PathBuf,
    entry: String,
    pub(super) supervisor: PluginRuntimeSupervisorState,
    pub(super) generation: u64,
    pub(super) child: Option<Child>,
    stdin: Option<ChildStdin>,
    output: Option<mpsc::Receiver<Result<Zeroizing<String>, PluginError>>>,
    output_reader: Option<tokio::task::JoinHandle<()>>,
    stderr_reader: Option<tokio::task::JoinHandle<()>>,
    output_wake: Arc<Notify>,
    outbound_messages: VecDeque<PluginOutboundMessage>,
    outbound_effects: VecDeque<PluginOutboundEffect>,
    host_call_handler: Option<PluginHostCallHandler>,
}

struct ProcessPluginRequestOwner(PluginRequest);

impl std::ops::Deref for ProcessPluginRequestOwner {
    type Target = PluginRequest;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for ProcessPluginRequestOwner {
    fn drop(&mut self) {
        self.0.zeroize_sensitive_host_call_args();
    }
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
            generation: 0,
            child: None,
            stdin: None,
            output: None,
            output_reader: None,
            stderr_reader: None,
            output_wake: Arc::new(Notify::new()),
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

    pub fn set_output_wake(&mut self, wake: Arc<Notify>) {
        self.output_wake = wake;
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
            // Raw frames retain zeroizing ownership until consumed. The bounded
            // queue applies pipe backpressure without blocking the UI thread.
            let (output_tx, output_rx) = mpsc::channel(PROCESS_OUTPUT_QUEUE_CAPACITY);
            let wake = self.output_wake.clone();
            self.output_reader = Some(tokio::spawn(async move {
                let mut stdout = BufReader::new(stdout);
                loop {
                    let mut line = Zeroizing::new(String::new());
                    let frame = match stdout.read_line(&mut line).await {
                        Ok(0) => Err(PluginError::runtime(
                            "process_exited",
                            "Native plugin process closed stdout",
                        )),
                        Ok(_) => Ok(line),
                        Err(_) => Err(PluginError::runtime(
                            "process_response_read_failed",
                            "Cannot read native plugin output",
                        )),
                    };
                    let ended = frame.is_err();
                    if output_tx.send(frame).await.is_err() {
                        break;
                    }
                    wake.notify_one();
                    if ended {
                        break;
                    }
                }
            }));
            if let Some(mut stderr) = child.stderr.take() {
                // Drain diagnostics without retaining potentially sensitive plugin text.
                self.stderr_reader = Some(tokio::spawn(async move {
                    let _ = tokio::io::copy(&mut stderr, &mut tokio::io::sink()).await;
                }));
            }
            self.child = Some(child);
            self.stdin = Some(stdin);
            self.output = Some(output_rx);
            Ok(())
        })
    }

    fn stop_process<'a>(&'a mut self) -> PluginRuntimeFuture<'a, PluginResponse> {
        Box::pin(async move {
            self.supervisor.start_deactivation();
            self.stdin.take();
            self.output.take();
            if let Some(task) = self.output_reader.take() {
                task.abort();
            }
            if let Some(task) = self.stderr_reader.take() {
                task.abort();
            }
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

    /// Consume unsolicited frames without sending a request or waiting for more data.
    pub async fn receive_pending_output(&mut self) -> Result<(), PluginError> {
        for _ in 0..PROCESS_OUTPUT_QUEUE_CAPACITY {
            let frame = match self.output.as_mut().map(|output| output.try_recv()) {
                Some(Ok(frame)) => frame?,
                _ => return Ok(()),
            };
            match decode_process_output_frame(&frame)? {
                PluginProcessFrame::Outbound(message) => self.accept_outbound(message).await?,
                PluginProcessFrame::Response { response, .. } => {
                    let _owner = PluginHostResponseOwner(response);
                    return Err(PluginError::protocol(
                        "process_response_request_mismatch",
                        "Unexpected native plugin response while idle",
                    ));
                }
            }
        }
        self.output_wake.notify_one();
        Ok(())
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
        let request = ProcessPluginRequestOwner(request);
        let stdin = self.stdin.as_mut().ok_or_else(|| {
            PluginError::runtime(
                "process_stdin_closed",
                "Native plugin process stdin is not available",
            )
        })?;
        let envelope = PluginProtocolEnvelope::new(Some(request.request_id.clone()), &*request);
        let mut line = Zeroizing::new(serde_json::to_vec(&envelope).map_err(|error| {
            PluginError::protocol(
                "process_request_encode_failed",
                format!("Cannot encode native plugin request: {error}"),
            )
        })?);
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
        let response = PluginHostResponseOwner(response);
        let envelope =
            PluginProtocolEnvelope::new(Some(response.0.request_id.clone()), &response.0);
        let mut line = Zeroizing::new(serde_json::to_vec(&envelope).map_err(|error| {
            PluginError::protocol(
                "process_host_response_encode_failed",
                format!("Cannot encode native plugin host-call response: {error}"),
            )
        })?);
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
            let line = {
                let output = self.output.as_mut().ok_or_else(|| {
                    PluginError::runtime(
                        "process_stdout_closed",
                        "Native plugin process stdout is not available",
                    )
                })?;
                time::timeout(remaining, output.recv())
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
                    .ok_or_else(|| {
                        PluginError::runtime(
                            "process_exited",
                            "Native plugin process closed stdout before responding",
                        )
                    })??
            };
            let frame = decode_process_output_frame(&line);
            drop(line);
            match frame? {
                PluginProcessFrame::Response {
                    envelope_request_id,
                    response,
                } => {
                    if envelope_request_id.as_deref() != Some(request_id)
                        || response.request_id != request_id
                    {
                        let _owner = PluginHostResponseOwner(response);
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
                    self.accept_outbound(message).await?;
                }
            }
        }
    }

    async fn accept_outbound(&mut self, message: PluginOutboundMessage) -> Result<(), PluginError> {
        if message.host_call_sensitivity().is_sensitive() {
            // Credentials never enter the public message/effect queues, even while idle.
            if let Some(response) = self.returnable_sensitive_host_call_response(message) {
                self.write_process_host_call_response(response).await?;
            }
            return Ok(());
        }
        let supervisor_message = message.clone_public().ok_or_else(|| {
            PluginError::protocol(
                "sensitive_host_call_clone_rejected",
                "Sensitive host calls cannot enter the outbound audit path",
            )
        })?;
        let effect = self
            .supervisor
            .handle_outbound_message(supervisor_message)
            .map_err(|error| {
                PluginError::protocol(
                    "process_outbound_rejected",
                    format!("Native plugin outbound frame rejected: {}", error.message),
                )
            })?;
        let response = self.returnable_host_call_response(&message);
        self.outbound_messages.push_back(message);
        self.outbound_effects.push_back(effect);
        if let Some(response) = response {
            self.write_process_host_call_response(response).await?;
        }
        Ok(())
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

    fn returnable_sensitive_host_call_response(
        &self,
        message: PluginOutboundMessage,
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
        let Some(handler) = self.host_call_handler.as_ref() else {
            // No consumer may inherit sensitive arguments when the optional host-call
            // resolver is absent.
            let mut call = PluginHostCall {
                request_id,
                namespace,
                method,
                args,
            };
            call.zeroize_args();
            return None;
        };
        handler(PluginHostCall {
            request_id,
            namespace,
            method,
            args,
        })
    }

    fn record_runtime_error(&mut self, error: PluginError) -> PluginError {
        self.supervisor.record_error(error.clone());
        error
    }
}

impl Drop for NativeProcessPluginRuntime {
    fn drop(&mut self) {
        if let Some(task) = self.output_reader.take() {
            task.abort();
        }
        if let Some(task) = self.stderr_reader.take() {
            task.abort();
        }
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

struct PluginHostResponseOwner(PluginResponse);

impl Drop for PluginHostResponseOwner {
    fn drop(&mut self) {
        // Sensitive responses remain wire-compatible `PluginResponse` values,
        // but this transport owner clears their JSON payload on every exit.
        self.0.zeroize_sensitive_payload();
    }
}

struct PluginProcessEnvelopeOwner {
    envelope: PluginProtocolEnvelope<Value>,
}

impl PluginProcessEnvelopeOwner {
    fn new(envelope: PluginProtocolEnvelope<Value>) -> Self {
        Self { envelope }
    }

    fn take_payload(&mut self) -> Value {
        std::mem::replace(&mut self.envelope.payload, Value::Null)
    }
}

impl Drop for PluginProcessEnvelopeOwner {
    fn drop(&mut self) {
        // Parsed JSON owns allocations distinct from the already-zeroizing line.
        // Clear them on version, shape, and typed decoding failures.
        oxideterm_plugin_protocol::zeroize_json_value(&mut self.envelope.payload);
    }
}

#[derive(Debug, PartialEq)]
pub(super) enum PluginProcessFrame {
    Response {
        envelope_request_id: Option<String>,
        response: PluginResponse,
    },
    Outbound(PluginOutboundMessage),
}

pub(super) fn decode_process_output_frame(line: &str) -> Result<PluginProcessFrame, PluginError> {
    let envelope: PluginProtocolEnvelope<Value> = serde_json::from_str(line).map_err(|error| {
        PluginError::protocol(
            "process_output_decode_failed",
            format!("Cannot decode native plugin process output: {error}"),
        )
    })?;
    let mut envelope = PluginProcessEnvelopeOwner::new(envelope);
    envelope.envelope.validate_version()?;

    // The stdio transport is line-oriented and intentionally keeps response
    // frames and spontaneous plugin->host frames in the same versioned envelope.
    // That matches Tauri's activate-time ctx registration semantics without
    // allowing arbitrary JS callbacks to run inside native render paths.
    if envelope.envelope.payload.get("result").is_some()
        && envelope.envelope.payload.get("requestId").is_some()
    {
        let response =
            serde_json::from_value::<PluginResponse>(envelope.take_payload()).map_err(|error| {
                PluginError::protocol(
                    "process_response_decode_failed",
                    format!("Cannot decode native plugin response: {error}"),
                )
            })?;
        return Ok(PluginProcessFrame::Response {
            envelope_request_id: std::mem::take(&mut envelope.envelope.request_id),
            response,
        });
    }

    if envelope.envelope.payload.get("type").is_some() {
        if let Some(message) = take_sensitive_host_call_message(&mut envelope.envelope.payload)? {
            return Ok(PluginProcessFrame::Outbound(message));
        }
        let message = serde_json::from_value::<PluginOutboundMessage>(envelope.take_payload())
            .map_err(|error| {
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

fn take_sensitive_host_call_message(
    payload: &mut Value,
) -> Result<Option<PluginOutboundMessage>, PluginError> {
    let Some(fields) = payload.as_object_mut() else {
        return Ok(None);
    };
    if fields.get("type").and_then(Value::as_str) != Some("callHostApi") {
        return Ok(None);
    }
    let namespace = fields.get("namespace").and_then(Value::as_str);
    let method = fields.get("method").and_then(Value::as_str);
    let sensitivity = match (namespace, method) {
        (Some(namespace), Some(method)) => PluginHostCallSensitivity::classify(namespace, method),
        _ => {
            return Err(PluginError::protocol(
                "process_outbound_decode_failed",
                "Cannot decode native plugin host call without string namespace and method",
            ));
        }
    };
    if !sensitivity.is_sensitive() {
        return Ok(None);
    }

    let request_id = fields
        .get("requestId")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            PluginError::protocol(
                "process_outbound_decode_failed",
                "Cannot decode sensitive native plugin host call without string requestId",
            )
        })?;
    let request_id = request_id.to_string();
    let namespace = namespace.unwrap_or_default().to_string();
    let method = method.unwrap_or_default().to_string();
    let args = fields.remove("args").unwrap_or(Value::Null);
    Ok(Some(PluginOutboundMessage::CallHostApi {
        request_id,
        namespace,
        method,
        args,
    }))
}
