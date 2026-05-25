// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

// Phase 3 lands the shared native protocol before the process/WASM runners are
// connected. Keeping these types compiled and tested now prevents each runtime
// from inventing a separate message schema in later phases.
#![allow(dead_code)]

use std::{
    collections::{HashMap, VecDeque},
    fs,
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    process::Stdio,
    sync::Arc,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout},
    time,
};
use wasmtime::{
    Config as WasmConfig, Engine as WasmEngine, Instance as WasmInstance, Linker as WasmLinker,
    Memory as WasmMemory, Store as WasmStore,
};
use wasmtime_wasi::{WasiCtxBuilder, p1::WasiP1Ctx};

use super::plugin_host::NativePluginManifest;

pub(super) const NATIVE_PLUGIN_PROTOCOL_VERSION: u32 = 1;
const DEFAULT_RUNTIME_MAX_ERROR_COUNT: u32 = 3;

pub(super) type PluginRuntimeFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, PluginError>> + Send + 'a>>;
type PluginHostCallHandler = Box<dyn Fn(PluginHostCall) -> Option<PluginResponse> + Send + Sync>;
pub(super) type NativeHostApiResolver = Arc<
    dyn Fn(String, PluginPermissionSet, PluginHostCall) -> Option<PluginResponse> + Send + Sync,
>;

#[allow(dead_code)]
pub(super) trait PluginRuntimeBridge: Send {
    fn activate<'a>(
        &'a mut self,
        request: PluginActivateRequest,
    ) -> PluginRuntimeFuture<'a, PluginResponse>;
    fn deactivate<'a>(&'a mut self) -> PluginRuntimeFuture<'a, PluginResponse>;
    fn call<'a>(&'a mut self, request: PluginRequest) -> PluginRuntimeFuture<'a, PluginResponse>;
    fn send_event<'a>(&'a mut self, event: PluginEvent) -> PluginRuntimeFuture<'a, PluginResponse>;
    fn kill<'a>(&'a mut self) -> PluginRuntimeFuture<'a, PluginResponse>;
    fn health<'a>(&'a mut self) -> PluginRuntimeFuture<'a, PluginRuntimeHealth>;
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct NativePluginRuntimeActivation {
    pub plugin_id: String,
    pub response: PluginResponse,
    pub messages: Vec<PluginOutboundMessage>,
    pub effects: Vec<PluginOutboundEffect>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct NativePluginRuntimeCommandDispatch {
    pub plugin_id: String,
    pub command: String,
    pub response: PluginResponse,
    pub messages: Vec<PluginOutboundMessage>,
    pub effects: Vec<PluginOutboundEffect>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct NativePluginRuntimeEventDispatch {
    pub plugin_id: String,
    pub event: PluginEvent,
    pub response: PluginResponse,
    pub messages: Vec<PluginOutboundMessage>,
    pub effects: Vec<PluginOutboundEffect>,
}

#[derive(Default)]
pub(super) struct NativePluginRuntimeHost {
    process_runtimes: HashMap<String, NativeProcessPluginRuntime>,
    process_permissions: HashMap<String, PluginPermissionSet>,
    wasm_runtimes: HashMap<String, NativeWasmPluginRuntime>,
    wasm_permissions: HashMap<String, PluginPermissionSet>,
    host_api_resolver: Option<NativeHostApiResolver>,
}

impl NativePluginRuntimeHost {
    pub fn set_host_api_resolver(&mut self, resolver: NativeHostApiResolver) {
        self.host_api_resolver = Some(resolver);
        let Some(resolver) = self.host_api_resolver.clone() else {
            return;
        };
        for (plugin_id, runtime) in &mut self.process_runtimes {
            let permissions = self
                .process_permissions
                .get(plugin_id)
                .cloned()
                .unwrap_or_default();
            install_process_host_call_handler(
                runtime,
                plugin_id.clone(),
                permissions,
                resolver.clone(),
            );
        }
    }

    pub async fn activate_process_plugin(
        &mut self,
        manifest: NativePluginManifest,
        plugin_dir: PathBuf,
        entry: String,
        permissions: PluginPermissionSet,
        lifecycle_timeout: Duration,
    ) -> Result<NativePluginRuntimeActivation, PluginError> {
        let plugin_id = manifest.id.clone();
        if self.process_runtimes.contains_key(&plugin_id) {
            self.deactivate_plugin(&plugin_id).await?;
        }

        let mut runtime = NativeProcessPluginRuntime::new(
            plugin_id.clone(),
            plugin_dir,
            entry,
            lifecycle_timeout,
        );
        if let Some(resolver) = self.host_api_resolver.clone() {
            install_process_host_call_handler(
                &mut runtime,
                plugin_id.clone(),
                permissions.clone(),
                resolver,
            );
        }
        let response = runtime
            .activate(PluginActivateRequest {
                request_id: format!("activate:{plugin_id}"),
                manifest,
                permissions: permissions.clone(),
                timeout_ms: lifecycle_timeout.as_millis() as u64,
            })
            .await?;

        // Tauri applies ctx registrations during activate(). Native preserves
        // that ordering by returning the validated frames to WorkspaceApp so it
        // can mutate the host registry on the UI thread.
        let messages = runtime.drain_outbound_messages();
        let effects = runtime.drain_outbound_effects();
        validate_outbound_effect_permissions(&effects, &permissions)?;

        if matches!(response.result, PluginResponseResult::Ok { .. }) {
            self.process_runtimes.insert(plugin_id.clone(), runtime);
            self.process_permissions
                .insert(plugin_id.clone(), permissions);
        }

        Ok(NativePluginRuntimeActivation {
            plugin_id,
            response,
            messages,
            effects,
        })
    }

    pub async fn activate_wasm_plugin(
        &mut self,
        manifest: NativePluginManifest,
        plugin_dir: PathBuf,
        entry: String,
        permissions: PluginPermissionSet,
        lifecycle_timeout: Duration,
    ) -> Result<NativePluginRuntimeActivation, PluginError> {
        let plugin_id = manifest.id.clone();
        if self.wasm_runtimes.contains_key(&plugin_id) {
            self.deactivate_plugin(&plugin_id).await?;
        }

        let mut runtime =
            NativeWasmPluginRuntime::new(plugin_id.clone(), plugin_dir, entry, lifecycle_timeout);
        let response = runtime
            .activate(PluginActivateRequest {
                request_id: format!("activate:{plugin_id}"),
                manifest,
                permissions: permissions.clone(),
                timeout_ms: lifecycle_timeout.as_millis() as u64,
            })
            .await?;
        let messages = runtime.drain_outbound_messages();
        let effects = runtime.drain_outbound_effects();
        validate_outbound_effect_permissions(&effects, &permissions)?;

        if matches!(response.result, PluginResponseResult::Ok { .. }) {
            self.wasm_runtimes.insert(plugin_id.clone(), runtime);
            self.wasm_permissions.insert(plugin_id.clone(), permissions);
        }

        Ok(NativePluginRuntimeActivation {
            plugin_id,
            response,
            messages,
            effects,
        })
    }

    pub async fn dispatch_command(
        &mut self,
        plugin_id: &str,
        command: String,
        args: Value,
        timeout: Duration,
    ) -> Result<NativePluginRuntimeCommandDispatch, PluginError> {
        if let Some(runtime) = self.wasm_runtimes.get_mut(plugin_id) {
            let permissions = self
                .wasm_permissions
                .get(plugin_id)
                .cloned()
                .unwrap_or_default();
            let response = runtime
                .call(PluginRequest {
                    request_id: format!("command:{plugin_id}:{command}"),
                    kind: PluginRequestKind::DispatchCommand {
                        command: command.clone(),
                        args,
                    },
                    timeout_ms: Some(timeout.as_millis() as u64),
                })
                .await?;
            let messages = runtime.drain_outbound_messages();
            let effects = runtime.drain_outbound_effects();
            validate_outbound_effect_permissions(&effects, &permissions)?;
            return Ok(NativePluginRuntimeCommandDispatch {
                plugin_id: plugin_id.to_string(),
                command,
                response,
                messages,
                effects,
            });
        }

        let permissions = self
            .process_permissions
            .get(plugin_id)
            .cloned()
            .unwrap_or_default();
        let runtime = self.process_runtimes.get_mut(plugin_id).ok_or_else(|| {
            PluginError::runtime(
                "plugin_runtime_not_active",
                format!("Native plugin runtime \"{plugin_id}\" is not active"),
            )
        })?;
        if let Some(resolver) = self.host_api_resolver.clone() {
            install_process_host_call_handler(
                runtime,
                plugin_id.to_string(),
                permissions.clone(),
                resolver,
            );
        }
        let response = runtime
            .call(PluginRequest {
                request_id: format!("command:{plugin_id}:{command}"),
                kind: PluginRequestKind::DispatchCommand {
                    command: command.clone(),
                    args,
                },
                timeout_ms: Some(timeout.as_millis() as u64),
            })
            .await?;
        let messages = runtime.drain_outbound_messages();
        let effects = runtime.drain_outbound_effects();
        validate_outbound_effect_permissions(&effects, &permissions)?;
        Ok(NativePluginRuntimeCommandDispatch {
            plugin_id: plugin_id.to_string(),
            command,
            response,
            messages,
            effects,
        })
    }

    pub async fn dispatch_event(
        &mut self,
        plugin_id: &str,
        event: PluginEvent,
        timeout: Duration,
    ) -> Result<NativePluginRuntimeEventDispatch, PluginError> {
        if let Some(runtime) = self.wasm_runtimes.get_mut(plugin_id) {
            let permissions = self
                .wasm_permissions
                .get(plugin_id)
                .cloned()
                .unwrap_or_default();
            let response = runtime.send_event(event.clone()).await?;
            let messages = runtime.drain_outbound_messages();
            let effects = runtime.drain_outbound_effects();
            validate_outbound_effect_permissions(&effects, &permissions)?;
            let _ = timeout;
            return Ok(NativePluginRuntimeEventDispatch {
                plugin_id: plugin_id.to_string(),
                event,
                response,
                messages,
                effects,
            });
        }

        let permissions = self
            .process_permissions
            .get(plugin_id)
            .cloned()
            .unwrap_or_default();
        let runtime = self.process_runtimes.get_mut(plugin_id).ok_or_else(|| {
            PluginError::runtime(
                "plugin_runtime_not_active",
                format!("Native plugin runtime \"{plugin_id}\" is not active"),
            )
        })?;
        if let Some(resolver) = self.host_api_resolver.clone() {
            install_process_host_call_handler(
                runtime,
                plugin_id.to_string(),
                permissions.clone(),
                resolver,
            );
        }
        let response = runtime
            .send_event(PluginEvent {
                name: event.name.clone(),
                payload: event.payload.clone(),
            })
            .await?;
        let messages = runtime.drain_outbound_messages();
        let effects = runtime.drain_outbound_effects();
        validate_outbound_effect_permissions(&effects, &permissions)?;
        // Keep the caller-supplied timeout in the API even though the current
        // PluginRuntimeBridge::send_event path uses the lifecycle timeout; the
        // explicit parameter documents the host boundary for future runtimes.
        let _ = timeout;
        Ok(NativePluginRuntimeEventDispatch {
            plugin_id: plugin_id.to_string(),
            event,
            response,
            messages,
            effects,
        })
    }

    pub async fn deactivate_plugin(
        &mut self,
        plugin_id: &str,
    ) -> Result<PluginResponse, PluginError> {
        // Runtime shutdown owns dynamic contribution cleanup. Manifest-only
        // contributions are cleaned by WorkspaceApp after this call so registry
        // mutation stays on the UI thread.
        let response = if let Some(mut runtime) = self.process_runtimes.remove(plugin_id) {
            runtime.deactivate().await?
        } else if let Some(mut runtime) = self.wasm_runtimes.remove(plugin_id) {
            runtime.deactivate().await?
        } else {
            PluginResponse::ok(
                format!("deactivate:{plugin_id}"),
                serde_json::json!({ "state": "not-running" }),
            )
        };
        self.process_permissions.remove(plugin_id);
        self.wasm_permissions.remove(plugin_id);
        Ok(response)
    }
}

fn install_process_host_call_handler(
    runtime: &mut NativeProcessPluginRuntime,
    plugin_id: String,
    permissions: PluginPermissionSet,
    resolver: NativeHostApiResolver,
) {
    runtime.set_host_call_handler(Box::new(move |call| {
        if !host_api_allowed(&permissions, &call.namespace, &call.method) {
            return Some(PluginResponse::error(
                call.request_id,
                PluginError::protocol(
                    "host_api_not_allowed",
                    format!(
                        "Native plugin host call \"{}.{}\" is not allowed",
                        call.namespace, call.method
                    ),
                ),
            ));
        }
        resolver(plugin_id.clone(), permissions.clone(), call)
    }));
}

fn validate_outbound_effect_permissions(
    effects: &[PluginOutboundEffect],
    permissions: &PluginPermissionSet,
) -> Result<(), PluginError> {
    for effect in effects {
        let PluginOutboundEffect::HostCall {
            namespace, method, ..
        } = effect
        else {
            continue;
        };
        if !host_api_allowed(permissions, namespace, method) {
            return Err(PluginError::protocol(
                "host_api_not_allowed",
                format!("Native plugin host call \"{namespace}.{method}\" is not allowed"),
            ));
        }
    }
    Ok(())
}

fn host_api_allowed(permissions: &PluginPermissionSet, namespace: &str, method: &str) -> bool {
    let exact = format!("{namespace}.{method}");
    let namespace_wildcard = format!("{namespace}.*");
    permissions
        .allowed_host_apis
        .iter()
        .any(|allowed| allowed == &exact || allowed == &namespace_wildcard)
}

pub(super) struct NativeWasmPluginRuntime {
    plugin_dir: PathBuf,
    entry: String,
    supervisor: PluginRuntimeSupervisorState,
    instance: Option<NativeWasmPluginInstance>,
    outbound_messages: VecDeque<PluginOutboundMessage>,
    outbound_effects: VecDeque<PluginOutboundEffect>,
    active: bool,
}

struct NativeWasmPluginInstance {
    engine: WasmEngine,
    store: WasmStore<WasiP1Ctx>,
    instance: WasmInstance,
    memory: WasmMemory,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum NativeWasmGuestResponse {
    Response(PluginResponse),
    Envelope {
        response: PluginResponse,
        #[serde(default)]
        messages: Vec<PluginOutboundMessage>,
    },
}

struct WasmGuestCallResult {
    response: PluginResponse,
    messages: Vec<PluginOutboundMessage>,
}

impl NativeWasmPluginRuntime {
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
            instance: None,
            outbound_messages: VecDeque::new(),
            outbound_effects: VecDeque::new(),
            active: false,
        }
    }

    pub fn drain_outbound_messages(&mut self) -> Vec<PluginOutboundMessage> {
        self.outbound_messages.drain(..).collect()
    }

    pub fn drain_outbound_effects(&mut self) -> Vec<PluginOutboundEffect> {
        self.outbound_effects.drain(..).collect()
    }

    fn call_wasm_guest(
        &mut self,
        request: PluginRequest,
        export_name: &str,
    ) -> Result<PluginResponse, PluginError> {
        let request_id = request.request_id.clone();
        let instance = self.instance.as_mut().ok_or_else(|| {
            PluginError::runtime(
                "wasm_runtime_not_active",
                "Native WASM plugin runtime is not active",
            )
        })?;
        let timeout = request
            .timeout_ms
            .map(Duration::from_millis)
            .unwrap_or_else(|| self.supervisor.lifecycle_timeout());
        instance.reset_epoch_deadline(timeout);
        let guest_response = instance.call_json_request(export_name, &request)?;
        self.capture_wasm_outbound_messages(guest_response.messages)?;
        let response = guest_response.response;
        if response.request_id != request_id {
            return Err(PluginError::protocol(
                "wasm_response_request_mismatch",
                format!(
                    "Native WASM plugin response request id mismatch; expected \"{request_id}\""
                ),
            ));
        }
        Ok(response)
    }

    fn drain_wasm_guest_outbound(&mut self) -> Result<Vec<PluginOutboundMessage>, PluginError> {
        let Some(instance) = self.instance.as_mut() else {
            return Ok(Vec::new());
        };
        instance.drain_outbound_messages()
    }

    fn capture_wasm_outbound_messages(
        &mut self,
        messages: Vec<PluginOutboundMessage>,
    ) -> Result<(), PluginError> {
        for message in messages {
            let effect = self
                .supervisor
                .handle_outbound_message(message.clone())
                .map_err(|error| {
                    PluginError::protocol(
                        "wasm_outbound_rejected",
                        format!(
                            "Native WASM plugin outbound frame rejected: {}",
                            error.message
                        ),
                    )
                })?;
            self.outbound_messages.push_back(message);
            self.outbound_effects.push_back(effect);
        }
        Ok(())
    }
}

impl NativeWasmPluginInstance {
    fn reset_epoch_deadline(&mut self, timeout: Duration) {
        self.store.set_epoch_deadline(1);
        schedule_wasm_epoch_timeout(self.engine.clone(), timeout);
    }

    fn call_json_request(
        &mut self,
        export_name: &str,
        request: &PluginRequest,
    ) -> Result<WasmGuestCallResult, PluginError> {
        let request_bytes = serde_json::to_vec(request).map_err(|error| {
            PluginError::protocol(
                "wasm_request_encode_failed",
                format!("Cannot encode native WASM plugin request: {error}"),
            )
        })?;
        let request_ptr = self.guest_alloc(request_bytes.len())?;
        self.memory
            .write(&mut self.store, request_ptr, &request_bytes)
            .map_err(|error| {
                PluginError::runtime(
                    "wasm_request_write_failed",
                    format!("Cannot write native WASM plugin request into guest memory: {error}"),
                )
            })?;
        let handler = self
            .instance
            .get_typed_func::<(i32, i32), i64>(&mut self.store, export_name)
            .map_err(|error| {
                PluginError::protocol(
                    "wasm_handler_missing",
                    format!("Native WASM plugin must export {export_name}: {error}"),
                )
            })?;
        let packed = handler
            .call(
                &mut self.store,
                (request_ptr as i32, request_bytes.len() as i32),
            )
            .map_err(|error| {
                wasm_execution_error(
                    "wasm_handler_failed",
                    export_name,
                    "execute native WASM plugin handler",
                    error,
                )
            })?;
        self.read_guest_call_result(packed)
    }

    fn drain_outbound_messages(&mut self) -> Result<Vec<PluginOutboundMessage>, PluginError> {
        let Some(drain) = self
            .instance
            .get_typed_func::<(), i64>(&mut self.store, "oxideterm_plugin_drain_outbound")
            .ok()
        else {
            return Ok(Vec::new());
        };
        let packed = drain.call(&mut self.store, ()).map_err(|error| {
            wasm_execution_error(
                "wasm_outbound_drain_failed",
                "oxideterm_plugin_drain_outbound",
                "drain native WASM outbound messages",
                error,
            )
        })?;
        if packed == 0 {
            return Ok(Vec::new());
        }
        let bytes = self.read_guest_bytes(packed, "wasm_outbound_read_failed")?;
        serde_json::from_slice::<Vec<PluginOutboundMessage>>(&bytes).map_err(|error| {
            PluginError::protocol(
                "wasm_outbound_decode_failed",
                format!("Cannot decode native WASM plugin outbound messages: {error}"),
            )
        })
    }

    fn read_guest_call_result(&self, packed: i64) -> Result<WasmGuestCallResult, PluginError> {
        let response_bytes = self.read_guest_bytes(packed, "wasm_response_read_failed")?;
        let guest_response = serde_json::from_slice::<NativeWasmGuestResponse>(&response_bytes)
            .map_err(|error| {
                PluginError::protocol(
                    "wasm_response_decode_failed",
                    format!("Cannot decode native WASM plugin response: {error}"),
                )
            })?;
        Ok(match guest_response {
            NativeWasmGuestResponse::Response(response) => WasmGuestCallResult {
                response,
                messages: Vec::new(),
            },
            NativeWasmGuestResponse::Envelope { response, messages } => {
                WasmGuestCallResult { response, messages }
            }
        })
    }

    fn read_guest_bytes(
        &self,
        packed: i64,
        error_code: &'static str,
    ) -> Result<Vec<u8>, PluginError> {
        let (ptr, len) = wasm_unpack_ptr_len(packed)?;
        let mut bytes = vec![0; len];
        self.memory
            .read(&self.store, ptr, &mut bytes)
            .map_err(|error| {
                PluginError::protocol(
                    error_code,
                    format!("Cannot read native WASM plugin memory buffer: {error}"),
                )
            })?;
        Ok(bytes)
    }

    fn guest_alloc(&mut self, len: usize) -> Result<usize, PluginError> {
        if len > i32::MAX as usize {
            return Err(PluginError::protocol(
                "wasm_request_too_large",
                "Native WASM plugin request is too large for the ABI",
            ));
        }
        let alloc = self
            .instance
            .get_typed_func::<i32, i32>(&mut self.store, "oxideterm_plugin_alloc")
            .map_err(|error| {
                PluginError::protocol(
                    "wasm_alloc_missing",
                    format!("Native WASM plugin must export oxideterm_plugin_alloc: {error}"),
                )
            })?;
        let ptr = alloc.call(&mut self.store, len as i32).map_err(|error| {
            wasm_execution_error(
                "wasm_alloc_failed",
                "oxideterm_plugin_alloc",
                "allocate native WASM guest memory",
                error,
            )
        })?;
        if ptr < 0 {
            return Err(PluginError::protocol(
                "wasm_alloc_invalid_pointer",
                format!("Native WASM plugin returned negative allocation pointer {ptr}"),
            ));
        }
        Ok(ptr as usize)
    }
}

impl PluginRuntimeBridge for NativeWasmPluginRuntime {
    fn activate<'a>(
        &'a mut self,
        request: PluginActivateRequest,
    ) -> PluginRuntimeFuture<'a, PluginResponse> {
        Box::pin(async move {
            self.supervisor.start_activation();
            match instantiate_wasi_preview1_plugin(
                &request.manifest.id,
                &self.plugin_dir,
                &self.entry,
                self.supervisor.lifecycle_timeout(),
            ) {
                Ok(instance) => {
                    self.instance = Some(instance);
                    let messages = self.drain_wasm_guest_outbound()?;
                    self.capture_wasm_outbound_messages(messages)?;
                    self.active = true;
                    self.supervisor.mark_active();
                    Ok(PluginResponse::ok(
                        request.request_id,
                        serde_json::json!({
                            "state": "active",
                            "runtime": "wasm",
                            "wasi": "preview1",
                        }),
                    ))
                }
                Err(error) => {
                    self.instance = None;
                    self.active = false;
                    self.supervisor.record_error(error.clone());
                    Err(error)
                }
            }
        })
    }

    fn deactivate<'a>(&'a mut self) -> PluginRuntimeFuture<'a, PluginResponse> {
        Box::pin(async move {
            self.supervisor.start_deactivation();
            self.instance = None;
            self.active = false;
            self.supervisor.kill();
            Ok(PluginResponse::ok(
                "wasm.deactivate",
                serde_json::json!({ "state": "inactive" }),
            ))
        })
    }

    fn call<'a>(&'a mut self, request: PluginRequest) -> PluginRuntimeFuture<'a, PluginResponse> {
        Box::pin(async move {
            let response = self.call_wasm_guest(request, "oxideterm_plugin_command")?;
            Ok(response)
        })
    }

    fn send_event<'a>(&'a mut self, event: PluginEvent) -> PluginRuntimeFuture<'a, PluginResponse> {
        Box::pin(async move {
            let request_id = format!("event:{}", event.name);
            let response = self.call_wasm_guest(
                PluginRequest {
                    request_id,
                    kind: PluginRequestKind::SendEvent { event },
                    timeout_ms: None,
                },
                "oxideterm_plugin_event",
            )?;
            Ok(response)
        })
    }

    fn kill<'a>(&'a mut self) -> PluginRuntimeFuture<'a, PluginResponse> {
        self.deactivate()
    }

    fn health<'a>(&'a mut self) -> PluginRuntimeFuture<'a, PluginRuntimeHealth> {
        Box::pin(async move { Ok(self.supervisor.health()) })
    }
}

fn instantiate_wasi_preview1_plugin(
    plugin_id: &str,
    plugin_dir: &Path,
    entry: &str,
    lifecycle_timeout: Duration,
) -> Result<NativeWasmPluginInstance, PluginError> {
    let module_path = resolve_wasm_runtime_entry(plugin_dir, entry)?;
    let engine = wasm_runtime_engine()?;
    let module = wasmtime::Module::from_file(&engine, &module_path).map_err(|error| {
        PluginError::runtime(
            "wasm_module_load_failed",
            format!("Cannot load native WASM plugin module \"{entry}\": {error}"),
        )
    })?;
    let mut linker: WasmLinker<WasiP1Ctx> = WasmLinker::new(&engine);
    wasmtime_wasi::p1::add_to_linker_sync(&mut linker, |ctx| ctx).map_err(|error| {
        PluginError::runtime(
            "wasi_link_failed",
            format!("Cannot link WASIp1 imports for native WASM plugin \"{plugin_id}\": {error}"),
        )
    })?;
    let pre = linker.instantiate_pre(&module).map_err(|error| {
        PluginError::runtime(
            "wasm_preinstantiate_failed",
            format!("Cannot pre-instantiate native WASM plugin \"{plugin_id}\": {error}"),
        )
    })?;

    let wasi_args = vec![entry.to_string()];
    let mut wasi_builder = WasiCtxBuilder::new();
    wasi_builder.args(&wasi_args);
    let wasi_ctx = wasi_builder.build_p1();
    let mut store = wasmtime::Store::new(&engine, wasi_ctx);
    store.set_epoch_deadline(1);
    store.epoch_deadline_trap();
    schedule_wasm_epoch_timeout(engine.clone(), lifecycle_timeout);

    let instance = pre.instantiate(&mut store).map_err(|error| {
        wasm_execution_error(
            "wasm_instantiate_failed",
            plugin_id,
            "instantiate native WASM plugin",
            error,
        )
    })?;
    let start = instance
        .get_typed_func::<(), ()>(&mut store, "_start")
        .map_err(|error| {
            PluginError::protocol(
                "wasm_start_missing",
                format!("Native WASM plugin \"{plugin_id}\" must export WASIp1 _start: {error}"),
            )
        })?;
    start.call(&mut store, ()).map_err(|error| {
        wasm_execution_error(
            "wasm_start_failed",
            plugin_id,
            "execute native WASM plugin _start",
            error,
        )
    })?;
    let memory = instance.get_memory(&mut store, "memory").ok_or_else(|| {
        PluginError::protocol(
            "wasm_memory_missing",
            format!("Native WASM plugin \"{plugin_id}\" must export memory"),
        )
    })?;

    Ok(NativeWasmPluginInstance {
        engine,
        store,
        instance,
        memory,
    })
}

fn wasm_runtime_engine() -> Result<WasmEngine, PluginError> {
    let mut config = WasmConfig::new();
    // WASM plugins run inside Wasmtime, not a process that can be killed by the
    // OS. Epoch interruption ties Phase 3 lifecycle timeout to the engine so a
    // tight guest loop traps instead of occupying a runtime worker forever.
    config.epoch_interruption(true);
    WasmEngine::new(&config).map_err(|error| {
        PluginError::runtime(
            "wasm_engine_unavailable",
            format!("Cannot create native WASM runtime engine: {error}"),
        )
    })
}

fn schedule_wasm_epoch_timeout(engine: WasmEngine, timeout: Duration) {
    std::thread::spawn(move || {
        std::thread::sleep(timeout);
        engine.increment_epoch();
    });
}

fn wasm_unpack_ptr_len(packed: i64) -> Result<(usize, usize), PluginError> {
    if packed < 0 {
        return Err(PluginError::protocol(
            "wasm_response_invalid_pointer",
            format!("Native WASM plugin returned negative response pointer {packed}"),
        ));
    }
    // The selected Phase 3 ABI packs `(ptr, len)` as `ptr << 32 | len`.
    // Keeping the layout explicit lets non-Rust WASM plugins implement the
    // same deterministic boundary without depending on native Rust structs.
    let packed = packed as u64;
    let ptr = (packed >> 32) as usize;
    let len = (packed & 0xffff_ffff) as usize;
    if len == 0 {
        return Err(PluginError::protocol(
            "wasm_response_empty",
            "Native WASM plugin returned an empty response buffer",
        ));
    }
    Ok((ptr, len))
}

fn wasm_execution_error(
    code: &'static str,
    plugin_id: &str,
    action: &str,
    error: anyhow::Error,
) -> PluginError {
    if let Some(exit) = error.downcast_ref::<wasmtime_wasi::I32Exit>() {
        if exit.0 == 0 {
            return PluginError::runtime(
                code,
                format!("Native WASM plugin \"{plugin_id}\" exited while trying to {action}"),
            );
        }
        return PluginError::runtime(
            "wasm_exit_status",
            format!(
                "Native WASM plugin \"{plugin_id}\" exited with status {} while trying to {action}",
                exit.0
            ),
        );
    }
    PluginError::runtime(
        code,
        format!("Cannot {action} for \"{plugin_id}\": {error}"),
    )
}

pub(super) struct NativeProcessPluginRuntime {
    plugin_dir: PathBuf,
    entry: String,
    supervisor: PluginRuntimeSupervisorState,
    child: Option<Child>,
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
            let mut child = tokio::process::Command::new(executable)
                .current_dir(&self.plugin_dir)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .spawn()
                .map_err(|error| {
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

pub(super) fn resolve_process_runtime_entry(
    plugin_dir: &Path,
    entry: &str,
) -> Result<PathBuf, PluginError> {
    super::plugin_host::validate_plugin_relative_path(entry).map_err(|error| {
        PluginError::protocol(
            "invalid_process_entry",
            format!("Invalid runtime entry: {error}"),
        )
    })?;
    let plugin_dir = fs::canonicalize(plugin_dir).map_err(|error| {
        PluginError::runtime(
            "plugin_dir_unavailable",
            format!("Cannot resolve plugin directory: {error}"),
        )
    })?;
    let executable = fs::canonicalize(plugin_dir.join(entry)).map_err(|error| {
        PluginError::runtime(
            "process_entry_unavailable",
            format!("Cannot resolve native plugin process entry \"{entry}\": {error}"),
        )
    })?;
    if !executable.starts_with(&plugin_dir) {
        return Err(PluginError::protocol(
            "process_entry_escapes_plugin_dir",
            format!(
                "Native plugin process entry \"{}\" resolves outside plugin directory",
                entry
            ),
        ));
    }
    Ok(executable)
}

pub(super) fn resolve_wasm_runtime_entry(
    plugin_dir: &Path,
    entry: &str,
) -> Result<PathBuf, PluginError> {
    let module = resolve_plugin_runtime_entry(plugin_dir, entry, "wasm")?;
    let bytes = fs::read(&module).map_err(|error| {
        PluginError::runtime(
            "wasm_entry_unreadable",
            format!("Cannot read native plugin WASM entry \"{entry}\": {error}"),
        )
    })?;
    if bytes.get(0..4) != Some(b"\0asm") {
        return Err(PluginError::protocol(
            "wasm_entry_invalid_magic",
            format!("Native plugin WASM entry \"{entry}\" is not a WebAssembly module"),
        ));
    }
    Ok(module)
}

fn resolve_plugin_runtime_entry(
    plugin_dir: &Path,
    entry: &str,
    runtime_kind: &str,
) -> Result<PathBuf, PluginError> {
    super::plugin_host::validate_plugin_relative_path(entry).map_err(|error| {
        PluginError::protocol(
            format!("invalid_{runtime_kind}_entry"),
            format!("Invalid runtime entry: {error}"),
        )
    })?;
    let plugin_dir = fs::canonicalize(plugin_dir).map_err(|error| {
        PluginError::runtime(
            "plugin_dir_unavailable",
            format!("Cannot resolve plugin directory: {error}"),
        )
    })?;
    let executable = fs::canonicalize(plugin_dir.join(entry)).map_err(|error| {
        PluginError::runtime(
            format!("{runtime_kind}_entry_unavailable"),
            format!("Cannot resolve native plugin {runtime_kind} entry \"{entry}\": {error}"),
        )
    })?;
    if !executable.starts_with(&plugin_dir) {
        return Err(PluginError::protocol(
            format!("{runtime_kind}_entry_escapes_plugin_dir"),
            format!(
                "Native plugin {runtime_kind} entry \"{}\" resolves outside plugin directory",
                entry
            ),
        ));
    }
    Ok(executable)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PluginProtocolEnvelope<T> {
    pub protocol_version: u32,
    pub request_id: Option<String>,
    pub payload: T,
}

impl<T> PluginProtocolEnvelope<T> {
    pub fn new(request_id: Option<String>, payload: T) -> Self {
        Self {
            protocol_version: NATIVE_PLUGIN_PROTOCOL_VERSION,
            request_id,
            payload,
        }
    }

    pub fn validate_version(&self) -> Result<(), PluginError> {
        validate_protocol_version(self.protocol_version)
    }
}

pub(super) fn validate_protocol_version(protocol_version: u32) -> Result<(), PluginError> {
    if protocol_version == NATIVE_PLUGIN_PROTOCOL_VERSION {
        return Ok(());
    }
    Err(PluginError::protocol(
        "unsupported_protocol_version",
        format!(
            "Unsupported native plugin protocol version {protocol_version}; expected {NATIVE_PLUGIN_PROTOCOL_VERSION}"
        ),
    ))
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PluginActivateRequest {
    pub request_id: String,
    pub manifest: NativePluginManifest,
    pub permissions: PluginPermissionSet,
    pub timeout_ms: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PluginPermissionSet {
    pub capabilities: Vec<String>,
    pub allowed_host_apis: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PluginRequest {
    pub request_id: String,
    pub kind: PluginRequestKind,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(super) enum PluginRequestKind {
    Activate {
        manifest: NativePluginManifest,
        permissions: PluginPermissionSet,
    },
    Deactivate,
    CallHostApi {
        namespace: String,
        method: String,
        args: Value,
    },
    DispatchCommand {
        command: String,
        args: Value,
    },
    SendEvent {
        event: PluginEvent,
    },
    CancelRequest {
        request_id: String,
    },
    Health,
    Kill,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PluginResponse {
    pub request_id: String,
    pub result: PluginResponseResult,
}

impl PluginResponse {
    pub fn ok(request_id: impl Into<String>, value: Value) -> Self {
        Self {
            request_id: request_id.into(),
            result: PluginResponseResult::Ok { value },
        }
    }

    pub fn error(request_id: impl Into<String>, error: PluginError) -> Self {
        Self {
            request_id: request_id.into(),
            result: PluginResponseResult::Error { error },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub(super) enum PluginResponseResult {
    Ok { value: Value },
    Error { error: PluginError },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PluginEvent {
    pub name: String,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(super) enum PluginOutboundMessage {
    RegisterContribution {
        registration: PluginRegistration,
    },
    DisposeContribution {
        registration_id: String,
    },
    Log {
        level: PluginRuntimeLogLevel,
        message: String,
    },
    ReportProgress {
        registration_id: String,
        value: Value,
    },
    RuntimeReady,
    RuntimeError {
        error: PluginError,
    },
    EmitEvent {
        event: PluginEvent,
    },
    CallHostApi {
        request_id: String,
        namespace: String,
        method: String,
        args: Value,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PluginRegistration {
    pub registration_id: String,
    pub plugin_id: String,
    pub kind: PluginRegistrationKind,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum PluginRegistrationKind {
    Command,
    Keybinding,
    ContextMenu,
    StatusBar,
    Tab,
    SidebarPanel,
    TerminalInputInterceptor,
    TerminalOutputProcessor,
    TerminalShortcut,
    EventSubscription,
    Progress,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PluginError {
    pub code: String,
    pub message: String,
    pub recoverable: bool,
}

impl PluginError {
    pub fn protocol(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            recoverable: false,
        }
    }

    pub fn runtime(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            recoverable: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum PluginRuntimeLifecycleState {
    Inactive,
    Activating,
    Active,
    Deactivating,
    Error,
    AutoDisabled,
    Killed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PluginRuntimeHealth {
    pub state: PluginRuntimeLifecycleState,
    pub healthy: bool,
    pub error_count: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PluginHostCall {
    pub request_id: String,
    pub namespace: String,
    pub method: String,
    #[serde(default)]
    pub args: Value,
}

#[derive(Clone, Debug)]
pub(super) struct PluginRuntimeSupervisorState {
    plugin_id: String,
    state: PluginRuntimeLifecycleState,
    lifecycle_timeout: Duration,
    max_error_count: u32,
    error_count: u32,
    last_error: Option<PluginError>,
    registrations: HashMap<String, PluginRegistration>,
    logs: Vec<PluginRuntimeLogEntry>,
}

impl PluginRuntimeSupervisorState {
    pub fn new(plugin_id: impl Into<String>, lifecycle_timeout: Duration) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            state: PluginRuntimeLifecycleState::Inactive,
            lifecycle_timeout,
            max_error_count: DEFAULT_RUNTIME_MAX_ERROR_COUNT,
            error_count: 0,
            last_error: None,
            registrations: HashMap::new(),
            logs: Vec::new(),
        }
    }

    pub fn state(&self) -> PluginRuntimeLifecycleState {
        self.state
    }

    pub fn lifecycle_timeout(&self) -> Duration {
        self.lifecycle_timeout
    }

    pub fn health(&self) -> PluginRuntimeHealth {
        PluginRuntimeHealth {
            state: self.state,
            healthy: matches!(self.state, PluginRuntimeLifecycleState::Active),
            error_count: self.error_count,
        }
    }

    pub fn start_activation(&mut self) {
        self.state = PluginRuntimeLifecycleState::Activating;
    }

    pub fn mark_active(&mut self) {
        self.state = PluginRuntimeLifecycleState::Active;
        self.error_count = 0;
        self.last_error = None;
    }

    pub fn start_deactivation(&mut self) {
        self.state = PluginRuntimeLifecycleState::Deactivating;
    }

    pub fn kill(&mut self) {
        self.state = PluginRuntimeLifecycleState::Killed;
        self.dispose_all_registrations();
    }

    pub fn record_registration(&mut self, registration: PluginRegistration) -> Result<(), String> {
        if registration.plugin_id != self.plugin_id {
            return Err(format!(
                "Registration \"{}\" belongs to plugin \"{}\", expected \"{}\"",
                registration.registration_id, registration.plugin_id, self.plugin_id
            ));
        }
        self.registrations
            .insert(registration.registration_id.clone(), registration);
        Ok(())
    }

    pub fn dispose_all_registrations(&mut self) -> usize {
        let count = self.registrations.len();
        self.registrations.clear();
        count
    }

    pub fn dispose_registration(&mut self, registration_id: &str) -> bool {
        self.registrations.remove(registration_id).is_some()
    }

    pub fn registration_count(&self) -> usize {
        self.registrations.len()
    }

    pub fn record_log(&mut self, level: PluginRuntimeLogLevel, message: impl Into<String>) {
        self.logs.push(PluginRuntimeLogEntry {
            level,
            message: message.into(),
        });
    }

    pub fn log_count(&self) -> usize {
        self.logs.len()
    }

    pub fn record_error(&mut self, error: PluginError) {
        self.error_count = self.error_count.saturating_add(1);
        self.last_error = Some(error);
        // Tauri auto-disables repeatedly failing plugins. Native keeps the same
        // user-visible safety boundary in the supervisor instead of leaving it
        // to individual process/WASM runners.
        if self.error_count >= self.max_error_count {
            self.state = PluginRuntimeLifecycleState::AutoDisabled;
            self.dispose_all_registrations();
        } else {
            self.state = PluginRuntimeLifecycleState::Error;
        }
    }

    pub fn handle_outbound_message(
        &mut self,
        message: PluginOutboundMessage,
    ) -> Result<PluginOutboundEffect, PluginError> {
        match message {
            PluginOutboundMessage::RegisterContribution { registration } => {
                self.record_registration(registration)
                    .map_err(|error| PluginError::protocol("invalid_registration", error))?;
                Ok(PluginOutboundEffect::RegistrationChanged)
            }
            PluginOutboundMessage::DisposeContribution { registration_id } => {
                self.dispose_registration(&registration_id);
                Ok(PluginOutboundEffect::RegistrationChanged)
            }
            PluginOutboundMessage::Log { level, message } => {
                self.record_log(level, message);
                Ok(PluginOutboundEffect::None)
            }
            PluginOutboundMessage::RuntimeReady => {
                self.mark_active();
                Ok(PluginOutboundEffect::LifecycleChanged)
            }
            PluginOutboundMessage::RuntimeError { error } => {
                self.record_error(error);
                Ok(PluginOutboundEffect::LifecycleChanged)
            }
            PluginOutboundMessage::ReportProgress {
                registration_id,
                value,
            } => Ok(PluginOutboundEffect::Progress {
                registration_id,
                value,
            }),
            PluginOutboundMessage::EmitEvent { event } => Ok(PluginOutboundEffect::Event(event)),
            PluginOutboundMessage::CallHostApi {
                request_id,
                namespace,
                method,
                args,
            } => Ok(PluginOutboundEffect::HostCall {
                request_id,
                namespace,
                method,
                args,
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum PluginOutboundEffect {
    None,
    RegistrationChanged,
    LifecycleChanged,
    Progress {
        registration_id: String,
        value: Value,
    },
    Event(PluginEvent),
    HostCall {
        request_id: String,
        namespace: String,
        method: String,
        args: Value,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PluginRuntimeLogEntry {
    pub level: PluginRuntimeLogLevel,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum PluginRuntimeLogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[cfg(test)]
mod tests {
    use super::super::plugin_host::{
        NativePluginRegistry, NativePluginRuntime, NativePluginRuntimeKind, native_plugins_dir,
    };
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(name: &str) -> PathBuf {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        std::env::temp_dir().join(format!("oxideterm-{name}-{millis}"))
    }

    fn sample_manifest() -> NativePluginManifest {
        NativePluginManifest {
            id: "com.example.runtime".to_string(),
            name: "Runtime".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            author: None,
            main: None,
            engines: None,
            manifest_version: None,
            format: None,
            assets: None,
            styles: None,
            shared_dependencies: None,
            repository: None,
            checksum: None,
            contributes: None,
            locales: None,
            runtime: None,
        }
    }

    #[test]
    fn protocol_envelope_rejects_unknown_version() {
        let envelope = PluginProtocolEnvelope {
            protocol_version: NATIVE_PLUGIN_PROTOCOL_VERSION + 1,
            request_id: Some("req-1".to_string()),
            payload: PluginEvent {
                name: "demo".to_string(),
                payload: Value::Null,
            },
        };

        let error = envelope.validate_version().unwrap_err();
        assert_eq!(error.code, "unsupported_protocol_version");
        assert!(!error.recoverable);
    }

    #[test]
    fn runtime_request_round_trips_as_versioned_json() {
        let request = PluginRequest {
            request_id: "activate-1".to_string(),
            kind: PluginRequestKind::Activate {
                manifest: sample_manifest(),
                permissions: PluginPermissionSet {
                    capabilities: vec!["plugin.invoke".to_string()],
                    allowed_host_apis: vec!["ui.registerCommand".to_string()],
                },
            },
            timeout_ms: Some(5_000),
        };
        let envelope = PluginProtocolEnvelope::new(Some(request.request_id.clone()), request);
        let encoded = serde_json::to_string(&envelope).unwrap();
        let decoded: PluginProtocolEnvelope<PluginRequest> =
            serde_json::from_str(&encoded).unwrap();

        decoded.validate_version().unwrap();
        assert_eq!(decoded.request_id.as_deref(), Some("activate-1"));
        assert!(matches!(
            decoded.payload.kind,
            PluginRequestKind::Activate { .. }
        ));
    }

    #[test]
    fn response_helpers_and_supervisor_lifecycle_state_are_covered() {
        let ok = PluginResponse::ok("req-ok", serde_json::json!({ "done": true }));
        assert!(matches!(ok.result, PluginResponseResult::Ok { .. }));
        let error = PluginResponse::error("req-error", PluginError::runtime("boom", "failed"));
        assert!(matches!(error.result, PluginResponseResult::Error { .. }));

        let mut supervisor =
            PluginRuntimeSupervisorState::new("com.example.runtime", Duration::from_millis(250));
        assert_eq!(supervisor.lifecycle_timeout(), Duration::from_millis(250));
        supervisor.start_activation();
        assert_eq!(supervisor.state(), PluginRuntimeLifecycleState::Activating);
        supervisor.mark_active();
        assert!(supervisor.health().healthy);
        supervisor.record_log(PluginRuntimeLogLevel::Info, "activated");
        assert_eq!(supervisor.log_count(), 1);
        supervisor.start_deactivation();
        assert_eq!(
            supervisor.state(),
            PluginRuntimeLifecycleState::Deactivating
        );
        supervisor.kill();
        assert_eq!(supervisor.state(), PluginRuntimeLifecycleState::Killed);
    }

    #[test]
    fn process_runtime_entry_resolves_inside_plugin_dir() {
        let temp_dir = unique_temp_dir("plugin-process-entry");
        let plugin_dir = temp_dir.join("plugin");
        let bin_dir = plugin_dir.join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        fs::write(bin_dir.join("plugin"), b"#!/bin/sh\n").unwrap();

        let resolved = resolve_process_runtime_entry(&plugin_dir, "bin/plugin").unwrap();
        assert!(resolved.starts_with(fs::canonicalize(&plugin_dir).unwrap()));
    }

    #[test]
    fn process_runtime_entry_rejects_path_traversal() {
        let temp_dir = unique_temp_dir("plugin-process-traversal");
        let plugin_dir = temp_dir.join("plugin");
        fs::create_dir_all(&plugin_dir).unwrap();

        let error = resolve_process_runtime_entry(&plugin_dir, "../outside").unwrap_err();
        assert_eq!(error.code, "invalid_process_entry");
    }

    #[cfg(unix)]
    #[test]
    fn process_runtime_entry_rejects_symlink_escape() {
        let temp_dir = unique_temp_dir("plugin-process-symlink");
        let plugin_dir = temp_dir.join("plugin");
        let outside_dir = temp_dir.join("outside");
        fs::create_dir_all(&plugin_dir).unwrap();
        fs::create_dir_all(&outside_dir).unwrap();
        fs::write(outside_dir.join("runner"), b"#!/bin/sh\n").unwrap();
        std::os::unix::fs::symlink(outside_dir.join("runner"), plugin_dir.join("runner")).unwrap();

        let error = resolve_process_runtime_entry(&plugin_dir, "runner").unwrap_err();
        assert_eq!(error.code, "process_entry_escapes_plugin_dir");
    }

    #[test]
    fn wasm_runtime_entry_validates_plugin_path_and_magic() {
        let temp_dir = unique_temp_dir("plugin-wasm-entry");
        let plugin_dir = temp_dir.join("plugin");
        fs::create_dir_all(&plugin_dir).unwrap();
        fs::write(plugin_dir.join("plugin.wasm"), b"\0asm\x01\0\0\0").unwrap();
        fs::write(plugin_dir.join("not-wasm.bin"), b"nope").unwrap();

        let resolved = resolve_wasm_runtime_entry(&plugin_dir, "plugin.wasm").unwrap();
        assert!(resolved.starts_with(fs::canonicalize(&plugin_dir).unwrap()));
        let error = resolve_wasm_runtime_entry(&plugin_dir, "not-wasm.bin").unwrap_err();
        assert_eq!(error.code, "wasm_entry_invalid_magic");
        let traversal = resolve_wasm_runtime_entry(&plugin_dir, "../plugin.wasm").unwrap_err();
        assert_eq!(traversal.code, "invalid_wasm_entry");
    }

    #[tokio::test]
    async fn wasm_runtime_activation_executes_wasi_preview1_start() {
        let temp_dir = unique_temp_dir("plugin-wasm-activate");
        let plugin_dir = temp_dir.join("plugin");
        fs::create_dir_all(&plugin_dir).unwrap();
        fs::write(plugin_dir.join("plugin.wasm"), wasm_noop_start_module()).unwrap();

        let mut runtime = NativeWasmPluginRuntime::new(
            "com.example.runtime",
            &plugin_dir,
            "plugin.wasm",
            Duration::from_millis(50),
        );
        let response = runtime
            .activate(PluginActivateRequest {
                request_id: "activate-test".to_string(),
                manifest: sample_manifest(),
                permissions: PluginPermissionSet::default(),
                timeout_ms: 50,
            })
            .await
            .unwrap();

        assert_eq!(
            response.result,
            PluginResponseResult::Ok {
                value: serde_json::json!({
                    "state": "active",
                    "runtime": "wasm",
                    "wasi": "preview1",
                })
            }
        );
        assert_eq!(
            runtime.health().await.unwrap().state,
            PluginRuntimeLifecycleState::Active
        );
    }

    #[tokio::test]
    async fn wasm_runtime_dispatches_command_and_event_over_memory_abi() {
        let temp_dir = unique_temp_dir("plugin-wasm-dispatch");
        let plugin_dir = temp_dir.join("plugin");
        fs::create_dir_all(&plugin_dir).unwrap();
        fs::write(plugin_dir.join("plugin.wasm"), wasm_dispatch_module()).unwrap();

        let mut host = NativePluginRuntimeHost::default();
        let manifest = sample_manifest();
        let activation = host
            .activate_wasm_plugin(
                manifest,
                plugin_dir,
                "plugin.wasm".to_string(),
                PluginPermissionSet::default(),
                Duration::from_millis(250),
            )
            .await
            .unwrap();
        assert!(matches!(
            activation.messages.as_slice(),
            [PluginOutboundMessage::Log { level: PluginRuntimeLogLevel::Info, message }]
                if message == "wasm activated"
        ));

        let command = host
            .dispatch_command(
                "com.example.runtime",
                "demo.run".to_string(),
                serde_json::json!({}),
                Duration::from_millis(250),
            )
            .await
            .unwrap();
        assert_eq!(
            command.response.result,
            PluginResponseResult::Ok {
                value: serde_json::json!({ "handled": true })
            }
        );

        let event = host
            .dispatch_event(
                "com.example.runtime",
                PluginEvent {
                    name: "demo.event".to_string(),
                    payload: serde_json::json!({}),
                },
                Duration::from_millis(250),
            )
            .await
            .unwrap();
        assert_eq!(
            event.response.result,
            PluginResponseResult::Ok {
                value: serde_json::json!({ "eventHandled": true })
            }
        );
    }

    fn wasm_noop_start_module() -> Vec<u8> {
        wat::parse_str(
            r#"
            (module
              (memory (export "memory") 1)
              (global $heap (mut i32) (i32.const 2048))
              (func (export "_start"))
              (func (export "oxideterm_plugin_alloc") (param $len i32) (result i32)
                (local $ptr i32)
                global.get $heap
                local.set $ptr
                global.get $heap
                local.get $len
                i32.add
                global.set $heap
                local.get $ptr))
            "#,
        )
        .unwrap()
    }

    fn wasm_dispatch_module() -> Vec<u8> {
        let command_response = r#"{"requestId":"command:com.example.runtime:demo.run","result":{"status":"ok","value":{"handled":true}}}"#;
        let event_response = r#"{"requestId":"event:demo.event","result":{"status":"ok","value":{"eventHandled":true}}}"#;
        let drain_response = r#"[{"type":"log","level":"info","message":"wasm activated"}]"#;
        let command_data = wat_data_string(command_response);
        let event_data = wat_data_string(event_response);
        let drain_data = wat_data_string(drain_response);
        let wat = format!(
            r#"
            (module
              (memory (export "memory") 1)
              (global $heap (mut i32) (i32.const 4096))
              (func (export "_start"))
              (func (export "oxideterm_plugin_alloc") (param $len i32) (result i32)
                (local $ptr i32)
                global.get $heap
                local.set $ptr
                global.get $heap
                local.get $len
                i32.add
                global.set $heap
                local.get $ptr)
              (data (i32.const 1024) "{command_data}")
              (data (i32.const 2048) "{event_data}")
              (data (i32.const 3072) "{drain_data}")
              (func (export "oxideterm_plugin_command") (param i32 i32) (result i64)
                i64.const 1024
                i64.const 32
                i64.shl
                i64.const {command_len}
                i64.or)
              (func (export "oxideterm_plugin_event") (param i32 i32) (result i64)
                i64.const 2048
                i64.const 32
                i64.shl
                i64.const {event_len}
                i64.or)
              (func (export "oxideterm_plugin_drain_outbound") (result i64)
                i64.const 3072
                i64.const 32
                i64.shl
                i64.const {drain_len}
                i64.or))
            "#,
            command_len = command_response.len(),
            event_len = event_response.len(),
            drain_len = drain_response.len(),
        );
        wat::parse_str(wat).unwrap()
    }

    fn wat_data_string(value: &str) -> String {
        value.replace('\\', "\\\\").replace('"', "\\\"")
    }

    #[cfg(unix)]
    fn write_process_plugin(plugin_dir: &Path, body: &str) {
        use std::os::unix::fs::PermissionsExt;

        let bin_dir = plugin_dir.join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let entry = bin_dir.join("plugin");
        fs::write(&entry, body).unwrap();
        let mut permissions = fs::metadata(&entry).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(entry, permissions).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn process_runtime_activate_uses_json_lines_protocol() {
        let temp_dir = unique_temp_dir("plugin-process-activate");
        let plugin_dir = temp_dir.join("plugin");
        write_process_plugin(
            &plugin_dir,
            r#"#!/bin/sh
read request
printf '%s\n' '{"protocolVersion":1,"requestId":"activate-test","payload":{"requestId":"activate-test","result":{"status":"ok","value":{"activated":true}}}}'
"#,
        );

        let mut runtime = NativeProcessPluginRuntime::new(
            "com.example.runtime",
            &plugin_dir,
            "bin/plugin",
            Duration::from_secs(2),
        );
        let response = runtime
            .activate(PluginActivateRequest {
                request_id: "activate-test".to_string(),
                manifest: sample_manifest(),
                permissions: PluginPermissionSet::default(),
                timeout_ms: 2_000,
            })
            .await
            .unwrap();

        assert!(matches!(response.result, PluginResponseResult::Ok { .. }));
        assert_eq!(
            runtime.health().await.unwrap().state,
            PluginRuntimeLifecycleState::Active
        );
        runtime.kill().await.unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn process_runtime_collects_activate_time_outbound_frames() {
        let temp_dir = unique_temp_dir("plugin-process-outbound");
        let plugin_dir = temp_dir.join("plugin");
        write_process_plugin(
            &plugin_dir,
            r#"#!/bin/sh
read request
printf '%s\n' '{"protocolVersion":1,"payload":{"type":"registerContribution","registration":{"registrationId":"cmd-1","pluginId":"com.example.runtime","kind":"command","metadata":{"id":"demo.run","label":"Run Demo"}}}}'
printf '%s\n' '{"protocolVersion":1,"payload":{"type":"log","level":"info","message":"registered command"}}'
printf '%s\n' '{"protocolVersion":1,"requestId":"activate-test","payload":{"requestId":"activate-test","result":{"status":"ok","value":{"activated":true}}}}'
"#,
        );

        let mut runtime = NativeProcessPluginRuntime::new(
            "com.example.runtime",
            &plugin_dir,
            "bin/plugin",
            Duration::from_secs(2),
        );
        let response = runtime
            .activate(PluginActivateRequest {
                request_id: "activate-test".to_string(),
                manifest: sample_manifest(),
                permissions: PluginPermissionSet::default(),
                timeout_ms: 2_000,
            })
            .await
            .unwrap();

        assert!(matches!(response.result, PluginResponseResult::Ok { .. }));
        assert_eq!(runtime.supervisor.registration_count(), 1);
        assert_eq!(runtime.supervisor.log_count(), 1);
        let messages = runtime.drain_outbound_messages();
        assert_eq!(messages.len(), 2);
        assert!(matches!(
            messages[0],
            PluginOutboundMessage::RegisterContribution { .. }
        ));
        let effects = runtime.drain_outbound_effects();
        assert_eq!(effects.len(), 2);
        assert_eq!(effects[0], PluginOutboundEffect::RegistrationChanged);
        assert!(runtime.drain_outbound_messages().is_empty());
        assert!(runtime.drain_outbound_effects().is_empty());
        runtime.kill().await.unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn process_runtime_exposes_host_call_effects_for_workspace_dispatch() {
        let temp_dir = unique_temp_dir("plugin-process-host-call");
        let plugin_dir = temp_dir.join("plugin");
        write_process_plugin(
            &plugin_dir,
            r#"#!/bin/sh
read request
printf '%s\n' '{"protocolVersion":1,"payload":{"type":"callHostApi","requestId":"host-1","namespace":"ui","method":"showToast","args":{"title":"Plugin ready","variant":"success"}}}'
printf '%s\n' '{"protocolVersion":1,"requestId":"activate-test","payload":{"requestId":"activate-test","result":{"status":"ok","value":{"activated":true}}}}'
"#,
        );

        let mut runtime = NativeProcessPluginRuntime::new(
            "com.example.runtime",
            &plugin_dir,
            "bin/plugin",
            Duration::from_secs(2),
        );
        runtime
            .activate(PluginActivateRequest {
                request_id: "activate-test".to_string(),
                manifest: sample_manifest(),
                permissions: PluginPermissionSet {
                    capabilities: Vec::new(),
                    allowed_host_apis: vec!["ui.showToast".to_string()],
                },
                timeout_ms: 2_000,
            })
            .await
            .unwrap();

        let effects = runtime.drain_outbound_effects();
        assert_eq!(
            effects[0],
            PluginOutboundEffect::HostCall {
                request_id: "host-1".to_string(),
                namespace: "ui".to_string(),
                method: "showToast".to_string(),
                args: serde_json::json!({
                    "title": "Plugin ready",
                    "variant": "success",
                }),
            }
        );
        runtime.kill().await.unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runtime_host_activates_process_plugin_applies_registry_and_cleans_on_deactivate() {
        let temp_dir = unique_temp_dir("plugin-runtime-host-process");
        let settings_path = temp_dir.join("settings.json");
        let plugins_dir = native_plugins_dir(&settings_path);
        let plugin_dir = plugins_dir.join("runtime");
        fs::create_dir_all(&plugin_dir).unwrap();
        let mut manifest = sample_manifest();
        manifest.runtime = Some(NativePluginRuntime {
            kind: NativePluginRuntimeKind::Process,
            entry: "bin/plugin".to_string(),
        });
        fs::write(
            plugin_dir.join("plugin.json"),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        write_process_plugin(
            &plugin_dir,
            r#"#!/bin/sh
read request
printf '%s\n' '{"protocolVersion":1,"payload":{"type":"registerContribution","registration":{"registrationId":"cmd-1","pluginId":"com.example.runtime","kind":"command","metadata":{"id":"demo.run","label":"Run Demo"}}}}'
printf '%s\n' '{"protocolVersion":1,"payload":{"type":"callHostApi","requestId":"host-1","namespace":"ui","method":"showToast","args":{"title":"Plugin ready","variant":"success"}}}'
printf '%s\n' '{"protocolVersion":1,"requestId":"activate:com.example.runtime","payload":{"requestId":"activate:com.example.runtime","result":{"status":"ok","value":{"activated":true}}}}'
"#,
        );

        let mut registry = NativePluginRegistry::discover(&settings_path);
        let mut host = NativePluginRuntimeHost::default();
        let activation = host
            .activate_process_plugin(
                manifest,
                plugin_dir,
                "bin/plugin".to_string(),
                PluginPermissionSet {
                    capabilities: Vec::new(),
                    allowed_host_apis: vec!["ui.showToast".to_string()],
                },
                Duration::from_secs(2),
            )
            .await
            .unwrap();
        for message in &activation.messages {
            registry
                .apply_runtime_outbound_message(&activation.plugin_id, message)
                .unwrap();
        }

        assert!(matches!(
            activation.response.result,
            PluginResponseResult::Ok { .. }
        ));
        assert_eq!(registry.contributions().runtime_commands.len(), 1);
        assert_eq!(
            registry.contributions().runtime_commands[0].command,
            "demo.run"
        );
        assert!(activation.effects.iter().any(|effect| matches!(
            effect,
            PluginOutboundEffect::HostCall { method, .. } if method == "showToast"
        )));

        host.deactivate_plugin("com.example.runtime").await.unwrap();
        registry.cleanup_runtime_plugin_contributions("com.example.runtime");
        assert!(registry.contributions().runtime_commands.is_empty());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runtime_host_dispatches_registered_command_over_process_rpc() {
        let temp_dir = unique_temp_dir("plugin-runtime-host-dispatch-command");
        let plugin_dir = temp_dir.join("plugin");
        fs::create_dir_all(&plugin_dir).unwrap();
        write_process_plugin(
            &plugin_dir,
            r#"#!/bin/sh
read activate
printf '%s\n' '{"protocolVersion":1,"payload":{"type":"registerContribution","registration":{"registrationId":"cmd-1","pluginId":"com.example.runtime","kind":"command","metadata":{"id":"demo.run","label":"Run Demo"}}}}'
printf '%s\n' '{"protocolVersion":1,"requestId":"activate:com.example.runtime","payload":{"requestId":"activate:com.example.runtime","result":{"status":"ok","value":{"activated":true}}}}'
read dispatch
printf '%s\n' '{"protocolVersion":1,"payload":{"type":"callHostApi","requestId":"host-2","namespace":"ui","method":"showToast","args":{"title":"Command ran","variant":"success"}}}'
printf '%s\n' '{"protocolVersion":1,"requestId":"command:com.example.runtime:demo.run","payload":{"requestId":"command:com.example.runtime:demo.run","result":{"status":"ok","value":{"handled":true}}}}'
"#,
        );

        let mut host = NativePluginRuntimeHost::default();
        host.activate_process_plugin(
            sample_manifest(),
            plugin_dir,
            "bin/plugin".to_string(),
            PluginPermissionSet {
                capabilities: Vec::new(),
                allowed_host_apis: vec!["ui.showToast".to_string()],
            },
            Duration::from_secs(2),
        )
        .await
        .unwrap();

        let dispatch = host
            .dispatch_command(
                "com.example.runtime",
                "demo.run".to_string(),
                Value::Null,
                Duration::from_secs(2),
            )
            .await
            .unwrap();

        assert_eq!(dispatch.command, "demo.run");
        assert!(matches!(
            dispatch.response.result,
            PluginResponseResult::Ok { .. }
        ));
        assert!(dispatch.effects.iter().any(|effect| matches!(
            effect,
            PluginOutboundEffect::HostCall { method, .. } if method == "showToast"
        )));
        host.deactivate_plugin("com.example.runtime").await.unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runtime_host_dispatches_subscription_event_over_process_rpc() {
        let temp_dir = unique_temp_dir("plugin-runtime-host-dispatch-event");
        let plugin_dir = temp_dir.join("plugin");
        fs::create_dir_all(&plugin_dir).unwrap();
        write_process_plugin(
            &plugin_dir,
            r#"#!/bin/sh
read activate
printf '%s\n' '{"protocolVersion":1,"payload":{"type":"registerContribution","registration":{"registrationId":"theme-sub-1","pluginId":"com.example.runtime","kind":"event-subscription","metadata":{"event":"app.themeChanged"}}}}'
printf '%s\n' '{"protocolVersion":1,"requestId":"activate:com.example.runtime","payload":{"requestId":"activate:com.example.runtime","result":{"status":"ok","value":{"activated":true}}}}'
read event_request
case "$event_request" in
  *'"name":"app.themeChanged"'*) result='{"status":"ok","value":{"received":true}}' ;;
  *) result='{"status":"error","error":{"code":"bad_event","message":"missing event","recoverable":false}}' ;;
esac
printf '%s\n' "{\"protocolVersion\":1,\"requestId\":\"event:app.themeChanged\",\"payload\":{\"requestId\":\"event:app.themeChanged\",\"result\":$result}}"
"#,
        );

        let mut host = NativePluginRuntimeHost::default();
        let activation = host
            .activate_process_plugin(
                sample_manifest(),
                plugin_dir,
                "bin/plugin".to_string(),
                PluginPermissionSet::default(),
                Duration::from_secs(2),
            )
            .await
            .unwrap();
        assert!(activation.messages.iter().any(|message| {
            matches!(
                message,
                PluginOutboundMessage::RegisterContribution { registration }
                    if registration.kind == PluginRegistrationKind::EventSubscription
            )
        }));

        let dispatch = host
            .dispatch_event(
                "com.example.runtime",
                PluginEvent {
                    name: "app.themeChanged".to_string(),
                    payload: serde_json::json!({
                        "theme": {
                            "name": "azurite",
                            "isDark": true,
                        }
                    }),
                },
                Duration::from_secs(2),
            )
            .await
            .unwrap();

        assert_eq!(dispatch.event.name, "app.themeChanged");
        assert_eq!(
            dispatch.response.result,
            PluginResponseResult::Ok {
                value: serde_json::json!({ "received": true })
            }
        );
        host.deactivate_plugin("com.example.runtime").await.unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn process_runtime_replies_to_returnable_host_call_before_final_response() {
        let temp_dir = unique_temp_dir("plugin-process-returnable-host-call");
        let plugin_dir = temp_dir.join("plugin");
        fs::create_dir_all(&plugin_dir).unwrap();
        write_process_plugin(
            &plugin_dir,
            r#"#!/bin/sh
read activate
printf '%s\n' '{"protocolVersion":1,"requestId":"activate-test","payload":{"requestId":"activate-test","result":{"status":"ok","value":{"activated":true}}}}'
read dispatch
printf '%s\n' '{"protocolVersion":1,"payload":{"type":"callHostApi","requestId":"host-storage-get","namespace":"storage","method":"get","args":{"key":"recent"}}}'
read host_response
case "$host_response" in
  *'"value":"stored"'*) result='{"status":"ok","value":{"read":true}}' ;;
  *) result='{"status":"error","error":{"code":"bad_host_response","message":"missing host value","recoverable":false}}' ;;
esac
printf '%s\n' "{\"protocolVersion\":1,\"requestId\":\"command:demo.read\",\"payload\":{\"requestId\":\"command:demo.read\",\"result\":$result}}"
"#,
        );

        let mut runtime = NativeProcessPluginRuntime::new(
            "com.example.runtime",
            &plugin_dir,
            "bin/plugin",
            Duration::from_secs(2),
        );
        runtime.set_host_call_handler(Box::new(|call| {
            assert_eq!(call.namespace, "storage");
            assert_eq!(call.method, "get");
            Some(PluginResponse::ok(
                call.request_id,
                serde_json::json!({ "value": "stored" }),
            ))
        }));
        runtime
            .activate(PluginActivateRequest {
                request_id: "activate-test".to_string(),
                manifest: sample_manifest(),
                permissions: PluginPermissionSet::default(),
                timeout_ms: 2_000,
            })
            .await
            .unwrap();

        let response = runtime
            .call(PluginRequest {
                request_id: "command:demo.read".to_string(),
                kind: PluginRequestKind::DispatchCommand {
                    command: "demo.read".to_string(),
                    args: Value::Null,
                },
                timeout_ms: Some(2_000),
            })
            .await
            .unwrap();

        assert_eq!(
            response.result,
            PluginResponseResult::Ok {
                value: serde_json::json!({ "read": true })
            }
        );
        assert!(runtime.drain_outbound_effects().iter().any(|effect| {
            matches!(
                effect,
                PluginOutboundEffect::HostCall {
                    namespace,
                    method,
                    ..
                } if namespace == "storage" && method == "get"
            )
        }));
        runtime.kill().await.unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runtime_host_installs_returnable_host_call_resolver_for_commands() {
        let temp_dir = unique_temp_dir("plugin-runtime-host-returnable-host-call");
        let plugin_dir = temp_dir.join("plugin");
        fs::create_dir_all(&plugin_dir).unwrap();
        write_process_plugin(
            &plugin_dir,
            r#"#!/bin/sh
read activate
printf '%s\n' '{"protocolVersion":1,"requestId":"activate:com.example.runtime","payload":{"requestId":"activate:com.example.runtime","result":{"status":"ok","value":{"activated":true}}}}'
read dispatch
printf '%s\n' '{"protocolVersion":1,"payload":{"type":"callHostApi","requestId":"host-storage-get","namespace":"storage","method":"get","args":{"key":"recent"}}}'
read host_response
case "$host_response" in
  *'"stored"'*) result='{"status":"ok","value":{"read":true}}' ;;
  *) result='{"status":"error","error":{"code":"bad_host_response","message":"missing host value","recoverable":false}}' ;;
esac
printf '%s\n' "{\"protocolVersion\":1,\"requestId\":\"command:com.example.runtime:demo.read\",\"payload\":{\"requestId\":\"command:com.example.runtime:demo.read\",\"result\":$result}}"
"#,
        );

        let mut host = NativePluginRuntimeHost::default();
        host.set_host_api_resolver(Arc::new(|plugin_id, _permissions, call| {
            assert_eq!(plugin_id, "com.example.runtime");
            assert_eq!(call.namespace, "storage");
            assert_eq!(call.method, "get");
            Some(PluginResponse::ok(
                call.request_id,
                serde_json::json!("stored"),
            ))
        }));
        host.activate_process_plugin(
            sample_manifest(),
            plugin_dir,
            "bin/plugin".to_string(),
            PluginPermissionSet {
                capabilities: Vec::new(),
                allowed_host_apis: vec!["storage.get".to_string()],
            },
            Duration::from_secs(2),
        )
        .await
        .unwrap();

        let dispatch = host
            .dispatch_command(
                "com.example.runtime",
                "demo.read".to_string(),
                Value::Null,
                Duration::from_secs(2),
            )
            .await
            .unwrap();

        assert_eq!(
            dispatch.response.result,
            PluginResponseResult::Ok {
                value: serde_json::json!({ "read": true })
            }
        );
        host.deactivate_plugin("com.example.runtime").await.unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runtime_host_accepts_keybinding_registration_and_dispatches_its_command() {
        let temp_dir = unique_temp_dir("plugin-runtime-host-keybinding-command");
        let settings_path = temp_dir.join("settings.json");
        let plugins_dir = native_plugins_dir(&settings_path);
        let plugin_dir = plugins_dir.join("runtime");
        fs::create_dir_all(&plugin_dir).unwrap();
        let mut manifest = sample_manifest();
        manifest.runtime = Some(NativePluginRuntime {
            kind: NativePluginRuntimeKind::Process,
            entry: "bin/plugin".to_string(),
        });
        fs::write(
            plugin_dir.join("plugin.json"),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        write_process_plugin(
            &plugin_dir,
            r#"#!/bin/sh
read activate
printf '%s\n' '{"protocolVersion":1,"payload":{"type":"registerContribution","registration":{"registrationId":"key-1","pluginId":"com.example.runtime","kind":"keybinding","metadata":{"keybinding":"Cmd+Shift+R","command":"demo.run","label":"Run Demo"}}}}'
printf '%s\n' '{"protocolVersion":1,"requestId":"activate:com.example.runtime","payload":{"requestId":"activate:com.example.runtime","result":{"status":"ok","value":{"activated":true}}}}'
read dispatch
printf '%s\n' '{"protocolVersion":1,"requestId":"command:com.example.runtime:demo.run","payload":{"requestId":"command:com.example.runtime:demo.run","result":{"status":"ok","value":{"handled":true}}}}'
"#,
        );

        let mut registry = NativePluginRegistry::discover(&settings_path);
        let mut host = NativePluginRuntimeHost::default();
        let activation = host
            .activate_process_plugin(
                manifest,
                plugin_dir,
                "bin/plugin".to_string(),
                PluginPermissionSet::default(),
                Duration::from_secs(2),
            )
            .await
            .unwrap();
        for message in &activation.messages {
            registry
                .apply_runtime_outbound_message(&activation.plugin_id, message)
                .unwrap();
        }

        assert_eq!(registry.contributions().runtime_keybindings.len(), 1);
        assert_eq!(
            registry.contributions().runtime_keybindings[0].keybinding,
            "Cmd+Shift+R"
        );
        let dispatch = host
            .dispatch_command(
                "com.example.runtime",
                registry.contributions().runtime_keybindings[0]
                    .command
                    .clone(),
                Value::Null,
                Duration::from_secs(2),
            )
            .await
            .unwrap();
        assert!(matches!(
            dispatch.response.result,
            PluginResponseResult::Ok { .. }
        ));
        host.deactivate_plugin("com.example.runtime").await.unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runtime_host_rejects_unauthorized_host_call_effects() {
        let temp_dir = unique_temp_dir("plugin-runtime-host-denied-host-call");
        let plugin_dir = temp_dir.join("plugin");
        fs::create_dir_all(&plugin_dir).unwrap();
        write_process_plugin(
            &plugin_dir,
            r#"#!/bin/sh
read request
printf '%s\n' '{"protocolVersion":1,"payload":{"type":"callHostApi","requestId":"host-1","namespace":"secrets","method":"get","args":{"key":"token"}}}'
printf '%s\n' '{"protocolVersion":1,"requestId":"activate:com.example.runtime","payload":{"requestId":"activate:com.example.runtime","result":{"status":"ok","value":{"activated":true}}}}'
"#,
        );

        let mut host = NativePluginRuntimeHost::default();
        let error = host
            .activate_process_plugin(
                sample_manifest(),
                plugin_dir,
                "bin/plugin".to_string(),
                PluginPermissionSet {
                    capabilities: Vec::new(),
                    allowed_host_apis: vec!["ui.showToast".to_string()],
                },
                Duration::from_secs(2),
            )
            .await
            .unwrap_err();

        assert_eq!(error.code, "host_api_not_allowed");
        let health = host.deactivate_plugin("com.example.runtime").await.unwrap();
        assert!(matches!(health.result, PluginResponseResult::Ok { .. }));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn process_runtime_rejects_unknown_response_protocol_version() {
        let temp_dir = unique_temp_dir("plugin-process-bad-version");
        let plugin_dir = temp_dir.join("plugin");
        write_process_plugin(
            &plugin_dir,
            r#"#!/bin/sh
read request
printf '%s\n' '{"protocolVersion":2,"requestId":"activate-test","payload":{"requestId":"activate-test","result":{"status":"ok","value":{}}}}'
"#,
        );

        let mut runtime = NativeProcessPluginRuntime::new(
            "com.example.runtime",
            &plugin_dir,
            "bin/plugin",
            Duration::from_secs(2),
        );
        let error = runtime
            .activate(PluginActivateRequest {
                request_id: "activate-test".to_string(),
                manifest: sample_manifest(),
                permissions: PluginPermissionSet::default(),
                timeout_ms: 2_000,
            })
            .await
            .unwrap_err();

        assert_eq!(error.code, "unsupported_protocol_version");
        assert_eq!(runtime.child.is_none(), true);
        assert_eq!(
            runtime.supervisor.state(),
            PluginRuntimeLifecycleState::Error
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn process_runtime_cleans_up_when_activate_process_exits() {
        let temp_dir = unique_temp_dir("plugin-process-exits");
        let plugin_dir = temp_dir.join("plugin");
        write_process_plugin(
            &plugin_dir,
            r#"#!/bin/sh
exit 0
"#,
        );

        let mut runtime = NativeProcessPluginRuntime::new(
            "com.example.runtime",
            &plugin_dir,
            "bin/plugin",
            Duration::from_secs(2),
        );
        let error = runtime
            .activate(PluginActivateRequest {
                request_id: "activate-test".to_string(),
                manifest: sample_manifest(),
                permissions: PluginPermissionSet::default(),
                timeout_ms: 2_000,
            })
            .await
            .unwrap_err();

        assert_eq!(error.code, "process_exited");
        assert!(runtime.child.is_none());
        assert_eq!(
            runtime.supervisor.state(),
            PluginRuntimeLifecycleState::Error
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn process_runtime_activate_timeout_moves_runtime_to_error_state() {
        let temp_dir = unique_temp_dir("plugin-process-timeout");
        let plugin_dir = temp_dir.join("plugin");
        write_process_plugin(
            &plugin_dir,
            r#"#!/bin/sh
read request
sleep 2
"#,
        );

        let mut runtime = NativeProcessPluginRuntime::new(
            "com.example.runtime",
            &plugin_dir,
            "bin/plugin",
            Duration::from_millis(50),
        );
        let error = runtime
            .activate(PluginActivateRequest {
                request_id: "activate-test".to_string(),
                manifest: sample_manifest(),
                permissions: PluginPermissionSet::default(),
                timeout_ms: 50,
            })
            .await
            .unwrap_err();

        assert_eq!(error.code, "process_response_timeout");
        assert!(runtime.child.is_none());
        assert_eq!(
            runtime.supervisor.state(),
            PluginRuntimeLifecycleState::Error
        );
    }

    #[test]
    fn supervisor_auto_disables_and_cleans_registrations_after_repeated_errors() {
        let mut supervisor =
            PluginRuntimeSupervisorState::new("com.example.runtime", Duration::from_secs(5));
        supervisor.mark_active();
        supervisor
            .record_registration(PluginRegistration {
                registration_id: "command-1".to_string(),
                plugin_id: "com.example.runtime".to_string(),
                kind: PluginRegistrationKind::Command,
                metadata: serde_json::json!({ "command": "demo.run" }),
            })
            .unwrap();

        supervisor.record_error(PluginError::runtime("crash", "first"));
        supervisor.record_error(PluginError::runtime("crash", "second"));
        assert_eq!(supervisor.state(), PluginRuntimeLifecycleState::Error);
        assert_eq!(supervisor.registration_count(), 1);

        supervisor.record_error(PluginError::runtime("crash", "third"));
        assert_eq!(
            supervisor.state(),
            PluginRuntimeLifecycleState::AutoDisabled
        );
        assert_eq!(supervisor.registration_count(), 0);
    }

    #[test]
    fn supervisor_rejects_foreign_plugin_registration() {
        let mut supervisor =
            PluginRuntimeSupervisorState::new("com.example.runtime", Duration::from_secs(5));
        let result = supervisor.record_registration(PluginRegistration {
            registration_id: "status-1".to_string(),
            plugin_id: "com.example.other".to_string(),
            kind: PluginRegistrationKind::StatusBar,
            metadata: Value::Null,
        });

        assert!(result.is_err());
        assert_eq!(supervisor.registration_count(), 0);
    }

    #[test]
    fn supervisor_applies_register_dispose_log_and_error_outbound_messages() {
        let mut supervisor =
            PluginRuntimeSupervisorState::new("com.example.runtime", Duration::from_secs(5));
        let registration = PluginRegistration {
            registration_id: "status-1".to_string(),
            plugin_id: "com.example.runtime".to_string(),
            kind: PluginRegistrationKind::StatusBar,
            metadata: serde_json::json!({ "text": "ready" }),
        };

        let effect = supervisor
            .handle_outbound_message(PluginOutboundMessage::RegisterContribution {
                registration: registration.clone(),
            })
            .unwrap();
        assert_eq!(effect, PluginOutboundEffect::RegistrationChanged);
        assert_eq!(supervisor.registration_count(), 1);

        let effect = supervisor
            .handle_outbound_message(PluginOutboundMessage::Log {
                level: PluginRuntimeLogLevel::Info,
                message: "registered".to_string(),
            })
            .unwrap();
        assert_eq!(effect, PluginOutboundEffect::None);
        assert_eq!(supervisor.log_count(), 1);

        let effect = supervisor
            .handle_outbound_message(PluginOutboundMessage::DisposeContribution {
                registration_id: registration.registration_id,
            })
            .unwrap();
        assert_eq!(effect, PluginOutboundEffect::RegistrationChanged);
        assert_eq!(supervisor.registration_count(), 0);

        supervisor
            .handle_outbound_message(PluginOutboundMessage::RuntimeError {
                error: PluginError::runtime("crash", "failed"),
            })
            .unwrap();
        assert_eq!(supervisor.state(), PluginRuntimeLifecycleState::Error);
    }

    #[test]
    fn supervisor_rejects_foreign_registration_from_outbound_message() {
        let mut supervisor =
            PluginRuntimeSupervisorState::new("com.example.runtime", Duration::from_secs(5));
        let error = supervisor
            .handle_outbound_message(PluginOutboundMessage::RegisterContribution {
                registration: PluginRegistration {
                    registration_id: "command-1".to_string(),
                    plugin_id: "com.example.other".to_string(),
                    kind: PluginRegistrationKind::Command,
                    metadata: Value::Null,
                },
            })
            .unwrap_err();

        assert_eq!(error.code, "invalid_registration");
        assert_eq!(supervisor.registration_count(), 0);
    }
}
