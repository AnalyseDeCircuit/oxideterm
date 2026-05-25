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

use serde::Deserialize;
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

use oxideterm_plugin_manifest::NativePluginManifest;
pub use oxideterm_plugin_protocol::{
    PluginActivateRequest, PluginError, PluginEvent, PluginHostCall, PluginOutboundEffect,
    PluginOutboundMessage, PluginPermissionSet, PluginProtocolEnvelope, PluginRegistration,
    PluginRegistrationKind, PluginRequest, PluginRequestKind, PluginResponse, PluginResponseResult,
    PluginRuntimeHealth, PluginRuntimeSupervisorState,
};
use oxideterm_plugin_registry::validate_plugin_relative_path;

#[cfg(test)]
pub use oxideterm_plugin_protocol::{
    NATIVE_PLUGIN_PROTOCOL_VERSION, PluginRuntimeLifecycleState, PluginRuntimeLogLevel,
};

pub type PluginRuntimeFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, PluginError>> + Send + 'a>>;
type PluginHostCallHandler = Box<dyn Fn(PluginHostCall) -> Option<PluginResponse> + Send + Sync>;
pub type NativeHostApiResolver = Arc<
    dyn Fn(String, PluginPermissionSet, PluginHostCall) -> Option<PluginResponse> + Send + Sync,
>;

#[allow(dead_code)]
pub trait PluginRuntimeBridge: Send {
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
pub struct NativePluginRuntimeActivation {
    pub plugin_id: String,
    pub response: PluginResponse,
    pub messages: Vec<PluginOutboundMessage>,
    pub effects: Vec<PluginOutboundEffect>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativePluginRuntimeCommandDispatch {
    pub plugin_id: String,
    pub command: String,
    pub response: PluginResponse,
    pub messages: Vec<PluginOutboundMessage>,
    pub effects: Vec<PluginOutboundEffect>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativePluginRuntimeEventDispatch {
    pub plugin_id: String,
    pub event: PluginEvent,
    pub response: PluginResponse,
    pub messages: Vec<PluginOutboundMessage>,
    pub effects: Vec<PluginOutboundEffect>,
}

#[derive(Default)]
pub struct NativePluginRuntimeHost {
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

mod paths;
mod permissions;
mod process;
mod wasm;

pub use paths::{resolve_process_runtime_entry, resolve_wasm_runtime_entry};
use permissions::{install_process_host_call_handler, validate_outbound_effect_permissions};
pub use process::NativeProcessPluginRuntime;
pub use wasm::NativeWasmPluginRuntime;

#[cfg(test)]
mod tests;
