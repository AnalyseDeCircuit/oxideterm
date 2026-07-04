// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! WASI/Wasmtime runtime bridge for native Wasm plugins.

use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    time::Duration,
};

use oxideterm_plugin_protocol::{
    PluginActivateRequest, PluginError, PluginEvent, PluginOutboundEffect, PluginOutboundMessage,
    PluginRequest, PluginRequestKind, PluginResponse, PluginRuntimeHealth,
    PluginRuntimeSupervisorState,
};
use serde::Deserialize;
use wasmtime::{
    Config as WasmConfig, Engine as WasmEngine, Instance as WasmInstance, Linker as WasmLinker,
    Memory as WasmMemory, Store as WasmStore,
};
use wasmtime_wasi::{WasiCtxBuilder, p1::WasiP1Ctx};

use crate::resolve_wasm_runtime_entry;

pub struct NativeWasmPluginRuntime {
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

    pub async fn activate(
        &mut self,
        request: PluginActivateRequest,
    ) -> Result<PluginResponse, PluginError> {
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
    }

    pub async fn deactivate(&mut self) -> Result<PluginResponse, PluginError> {
        self.supervisor.start_deactivation();
        self.instance = None;
        self.active = false;
        self.supervisor.kill();
        Ok(PluginResponse::ok(
            "wasm.deactivate",
            serde_json::json!({ "state": "inactive" }),
        ))
    }

    pub async fn call(&mut self, request: PluginRequest) -> Result<PluginResponse, PluginError> {
        self.call_wasm_guest(request, "oxideterm_plugin_command")
    }

    pub async fn send_event(&mut self, event: PluginEvent) -> Result<PluginResponse, PluginError> {
        let request_id = format!("event:{}", event.name);
        self.call_wasm_guest(
            PluginRequest {
                request_id,
                kind: PluginRequestKind::SendEvent { event },
                timeout_ms: None,
            },
            "oxideterm_plugin_event",
        )
    }

    pub async fn kill(&mut self) -> Result<PluginResponse, PluginError> {
        self.deactivate().await
    }

    pub async fn health(&mut self) -> Result<PluginRuntimeHealth, PluginError> {
        Ok(self.supervisor.health())
    }

    fn call_wasm_guest(
        &mut self,
        request: PluginRequest,
        export_name: &str,
    ) -> Result<PluginResponse, PluginError> {
        let request_id = request.request_id.clone();
        let timeout = request
            .timeout_ms
            .map(Duration::from_millis)
            .unwrap_or_else(|| self.supervisor.lifecycle_timeout());
        let instance = self.instance.as_mut().ok_or_else(|| {
            PluginError::runtime(
                "wasm_runtime_not_active",
                "Native WASM plugin runtime is not active",
            )
        })?;
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
    // Wasm plugins run inside Wasmtime, not a process that can be killed by the
    // OS. Epoch interruption ties lifecycle timeout to the engine so a tight
    // guest loop traps instead of occupying a runtime worker forever.
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
    // The selected ABI packs `(ptr, len)` as `ptr << 32 | len`. Keeping the
    // layout explicit lets non-Rust Wasm plugins implement the same boundary
    // without depending on native Rust structs.
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
