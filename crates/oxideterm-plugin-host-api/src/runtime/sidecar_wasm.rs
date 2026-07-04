//! Stdio bridge for the optional OxideTerm Wasm runtime sidecar.
//!
//! The sidecar binary is installed outside plugin directories, so this bridge
//! intentionally does not reuse process-plugin entry validation.

use super::*;

#[cfg(windows)]
const WASM_SIDECAR_BINARY: &str = "oxideterm-wasm-runtime.exe";
#[cfg(not(windows))]
const WASM_SIDECAR_BINARY: &str = "oxideterm-wasm-runtime";

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub fn wasm_sidecar_install_dir(settings_path: &Path) -> PathBuf {
    settings_path
        .parent()
        .unwrap_or(settings_path)
        .join("native-runtimes")
        .join("wasm")
}

pub fn installed_wasm_sidecar_binary_path(settings_path: &Path) -> PathBuf {
    wasm_sidecar_install_dir(settings_path).join(WASM_SIDECAR_BINARY)
}

pub struct NativeSidecarWasmPluginRuntime {
    sidecar_path: PathBuf,
    plugin_dir: PathBuf,
    entry: String,
    supervisor: PluginRuntimeSupervisorState,
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    stdout: Option<BufReader<ChildStdout>>,
    outbound_messages: VecDeque<PluginOutboundMessage>,
    outbound_effects: VecDeque<PluginOutboundEffect>,
}

impl NativeSidecarWasmPluginRuntime {
    pub fn new(
        plugin_id: impl Into<String>,
        sidecar_path: impl Into<PathBuf>,
        plugin_dir: impl Into<PathBuf>,
        entry: impl Into<String>,
        lifecycle_timeout: Duration,
    ) -> Self {
        Self {
            sidecar_path: sidecar_path.into(),
            plugin_dir: plugin_dir.into(),
            entry: entry.into(),
            supervisor: PluginRuntimeSupervisorState::new(plugin_id, lifecycle_timeout),
            child: None,
            stdin: None,
            stdout: None,
            outbound_messages: VecDeque::new(),
            outbound_effects: VecDeque::new(),
        }
    }

    pub fn drain_outbound_messages(&mut self) -> Vec<PluginOutboundMessage> {
        self.outbound_messages.drain(..).collect()
    }

    pub fn drain_outbound_effects(&mut self) -> Vec<PluginOutboundEffect> {
        self.outbound_effects.drain(..).collect()
    }

    fn start_sidecar<'a>(&'a mut self) -> PluginRuntimeFuture<'a, ()> {
        Box::pin(async move {
            let sidecar_path = fs::canonicalize(&self.sidecar_path).map_err(|error| {
                PluginError::runtime(
                    WASM_RUNTIME_NOT_INSTALLED_CODE,
                    format!("Cannot resolve Wasm runtime sidecar: {error}"),
                )
            })?;
            let plugin_dir = fs::canonicalize(&self.plugin_dir).map_err(|error| {
                PluginError::runtime(
                    "plugin_dir_unavailable",
                    format!("Cannot resolve plugin directory: {error}"),
                )
            })?;

            self.supervisor.start_activation();
            let mut command = tokio::process::Command::new(sidecar_path);
            command
                .arg("serve")
                .arg("--plugin-dir")
                .arg(&plugin_dir)
                .arg("--entry")
                .arg(&self.entry)
                .arg("--timeout-ms")
                .arg(self.supervisor.lifecycle_timeout().as_millis().to_string())
                .current_dir(&plugin_dir)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true);
            #[cfg(windows)]
            {
                command.creation_flags(CREATE_NO_WINDOW);
            }

            let mut child = command.spawn().map_err(|error| {
                PluginError::runtime(
                    "wasm_sidecar_spawn_failed",
                    format!("Cannot start Wasm runtime sidecar: {error}"),
                )
            })?;
            let stdin = child.stdin.take().ok_or_else(|| {
                PluginError::runtime(
                    "wasm_sidecar_stdin_unavailable",
                    "Wasm runtime sidecar did not expose stdin",
                )
            })?;
            let stdout = child.stdout.take().ok_or_else(|| {
                PluginError::runtime(
                    "wasm_sidecar_stdout_unavailable",
                    "Wasm runtime sidecar did not expose stdout",
                )
            })?;
            self.child = Some(child);
            self.stdin = Some(stdin);
            self.stdout = Some(BufReader::new(stdout));
            Ok(())
        })
    }

    fn stop_sidecar<'a>(&'a mut self) -> PluginRuntimeFuture<'a, PluginResponse> {
        Box::pin(async move {
            self.supervisor.start_deactivation();
            self.stdin.take();
            self.stdout.take();
            if let Some(mut child) = self.child.take() {
                let _ = child.kill().await;
            }
            self.supervisor.kill();
            Ok(PluginResponse::ok(
                "wasm-sidecar.kill",
                serde_json::json!({ "state": "killed" }),
            ))
        })
    }

    fn call_sidecar_request<'a>(
        &'a mut self,
        request: PluginRequest,
    ) -> PluginRuntimeFuture<'a, PluginResponse> {
        Box::pin(async move {
            let request_id = request.request_id.clone();
            let timeout = request
                .timeout_ms
                .map(Duration::from_millis)
                .unwrap_or_else(|| self.supervisor.lifecycle_timeout());
            self.write_sidecar_request(request).await?;
            self.read_sidecar_response(&request_id, timeout).await
        })
    }

    async fn write_sidecar_request(&mut self, request: PluginRequest) -> Result<(), PluginError> {
        let stdin = self.stdin.as_mut().ok_or_else(|| {
            PluginError::runtime(
                "wasm_sidecar_stdin_closed",
                "Wasm runtime sidecar stdin is not available",
            )
        })?;
        let mut line = serde_json::to_vec(&request).map_err(|error| {
            PluginError::protocol(
                "wasm_sidecar_request_encode_failed",
                format!("Cannot encode Wasm sidecar request: {error}"),
            )
        })?;
        line.push(b'\n');
        stdin.write_all(&line).await.map_err(|error| {
            PluginError::runtime(
                "wasm_sidecar_request_write_failed",
                format!("Cannot write Wasm sidecar request: {error}"),
            )
        })?;
        stdin.flush().await.map_err(|error| {
            PluginError::runtime(
                "wasm_sidecar_request_flush_failed",
                format!("Cannot flush Wasm sidecar request: {error}"),
            )
        })
    }

    async fn read_sidecar_response(
        &mut self,
        request_id: &str,
        timeout: Duration,
    ) -> Result<PluginResponse, PluginError> {
        let started_at = Instant::now();
        let mut line = String::new();
        loop {
            let remaining = timeout.checked_sub(started_at.elapsed()).ok_or_else(|| {
                PluginError::runtime(
                    "wasm_sidecar_response_timeout",
                    format!(
                        "Wasm runtime sidecar did not respond within {}ms",
                        timeout.as_millis()
                    ),
                )
            })?;
            line.clear();
            let read = {
                let stdout = self.stdout.as_mut().ok_or_else(|| {
                    PluginError::runtime(
                        "wasm_sidecar_stdout_closed",
                        "Wasm runtime sidecar stdout is not available",
                    )
                })?;
                time::timeout(remaining, stdout.read_line(&mut line))
                    .await
                    .map_err(|_| {
                        PluginError::runtime(
                            "wasm_sidecar_response_timeout",
                            format!(
                                "Wasm runtime sidecar did not respond within {}ms",
                                timeout.as_millis()
                            ),
                        )
                    })?
                    .map_err(|error| {
                        PluginError::runtime(
                            "wasm_sidecar_response_read_failed",
                            format!("Cannot read Wasm sidecar response: {error}"),
                        )
                    })?
            };
            if read == 0 {
                return Err(PluginError::runtime(
                    "wasm_sidecar_exited",
                    "Wasm runtime sidecar closed stdout before responding",
                ));
            }

            let envelope: SidecarWasmResponseEnvelope =
                serde_json::from_str(line.trim()).map_err(|error| {
                    PluginError::protocol(
                        "wasm_sidecar_response_decode_failed",
                        format!("Cannot decode Wasm sidecar response: {error}"),
                    )
                })?;
            if envelope.response.request_id != request_id {
                return Err(PluginError::protocol(
                    "wasm_sidecar_response_request_mismatch",
                    format!("Wasm sidecar response request id mismatch; expected \"{request_id}\""),
                ));
            }
            for message in envelope.messages {
                let effect = self
                    .supervisor
                    .handle_outbound_message(message.clone())
                    .map_err(|error| {
                        PluginError::protocol(
                            "wasm_sidecar_outbound_rejected",
                            format!("Wasm sidecar outbound frame rejected: {}", error.message),
                        )
                    })?;
                self.outbound_messages.push_back(message);
                self.outbound_effects.push_back(effect);
            }
            return Ok(envelope.response);
        }
    }

    fn record_runtime_error(&mut self, error: PluginError) -> PluginError {
        self.supervisor.record_error(error.clone());
        error
    }
}

impl PluginRuntimeBridge for NativeSidecarWasmPluginRuntime {
    fn activate<'a>(
        &'a mut self,
        request: PluginActivateRequest,
    ) -> PluginRuntimeFuture<'a, PluginResponse> {
        Box::pin(async move {
            if let Err(error) = self.start_sidecar().await {
                return Err(self.record_runtime_error(error));
            }
            let request_id = request.request_id.clone();
            let timeout_ms = request.timeout_ms;
            let response = self
                .call_sidecar_request(PluginRequest {
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
                        self.stop_sidecar().await.ok();
                        self.supervisor.record_error(error);
                    }
                    Ok(response)
                }
                Err(error) => {
                    self.stop_sidecar().await.ok();
                    Err(self.record_runtime_error(PluginError::runtime(
                        error.code,
                        format!(
                            "Wasm sidecar activate request \"{request_id}\" failed: {}",
                            error.message
                        ),
                    )))
                }
            }
        })
    }

    fn deactivate<'a>(&'a mut self) -> PluginRuntimeFuture<'a, PluginResponse> {
        Box::pin(async move {
            let response = self
                .call_sidecar_request(PluginRequest {
                    request_id: "wasm-sidecar.deactivate".to_string(),
                    kind: PluginRequestKind::Deactivate,
                    timeout_ms: Some(self.supervisor.lifecycle_timeout().as_millis() as u64),
                })
                .await;
            self.stop_sidecar().await.ok();
            response
        })
    }

    fn call<'a>(&'a mut self, request: PluginRequest) -> PluginRuntimeFuture<'a, PluginResponse> {
        self.call_sidecar_request(request)
    }

    fn send_event<'a>(&'a mut self, event: PluginEvent) -> PluginRuntimeFuture<'a, PluginResponse> {
        Box::pin(async move {
            self.call_sidecar_request(PluginRequest {
                request_id: format!("event:{}", event.name),
                kind: PluginRequestKind::SendEvent { event },
                timeout_ms: Some(self.supervisor.lifecycle_timeout().as_millis() as u64),
            })
            .await
        })
    }

    fn kill<'a>(&'a mut self) -> PluginRuntimeFuture<'a, PluginResponse> {
        self.stop_sidecar()
    }

    fn health<'a>(&'a mut self) -> PluginRuntimeFuture<'a, PluginRuntimeHealth> {
        Box::pin(async move {
            let response = self
                .call_sidecar_request(PluginRequest {
                    request_id: "wasm-sidecar.health".to_string(),
                    kind: PluginRequestKind::Health,
                    timeout_ms: Some(self.supervisor.lifecycle_timeout().as_millis() as u64),
                })
                .await?;
            match response.result {
                PluginResponseResult::Ok { value } => {
                    serde_json::from_value(value).map_err(|error| {
                        PluginError::protocol(
                            "wasm_sidecar_health_decode_failed",
                            format!("Cannot decode Wasm sidecar health response: {error}"),
                        )
                    })
                }
                PluginResponseResult::Error { error } => Err(error),
            }
        })
    }
}
