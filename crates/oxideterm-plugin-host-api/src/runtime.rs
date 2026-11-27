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

use serde_json::Value;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin},
    time,
};
use zeroize::Zeroizing;

use oxideterm_plugin_manifest::NativePluginManifest;
pub use oxideterm_plugin_protocol::{
    PluginActivateRequest, PluginError, PluginEvent, PluginHostCall, PluginHostCallSensitivity,
    PluginOutboundEffect, PluginOutboundMessage, PluginPermissionSet, PluginProtocolEnvelope,
    PluginRegistration, PluginRegistrationKind, PluginRequest, PluginRequestKind, PluginResponse,
    PluginResponseResult, PluginRuntimeHealth, PluginRuntimeSupervisorState,
};
use oxideterm_plugin_registry::validate_plugin_relative_path;

pub const WASM_RUNTIME_UNAVAILABLE_CODE: &str = "wasm_runtime_unavailable";

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

#[derive(Debug, PartialEq)]
pub struct NativePluginRuntimeActivation {
    pub plugin_id: String,
    pub generation: u64,
    pub response: PluginResponse,
    pub messages: Vec<PluginOutboundMessage>,
    pub effects: Vec<PluginOutboundEffect>,
}

#[derive(Debug, PartialEq)]
pub struct NativePluginRuntimeCommandDispatch {
    pub plugin_id: String,
    pub command: String,
    pub response: PluginResponse,
    pub messages: Vec<PluginOutboundMessage>,
    pub effects: Vec<PluginOutboundEffect>,
}

#[derive(Debug, PartialEq)]
pub struct NativePluginRuntimeEventDispatch {
    pub plugin_id: String,
    pub event: PluginEvent,
    pub response: PluginResponse,
    pub messages: Vec<PluginOutboundMessage>,
    pub effects: Vec<PluginOutboundEffect>,
}

pub struct NativePluginRuntimePush {
    pub messages: Vec<PluginOutboundMessage>,
    pub effects: Vec<PluginOutboundEffect>,
}

/// Only the outer dispatch error describes a failed transport. Provider codes are opaque.
#[derive(Debug)]
pub enum DiscoveryCommandError {
    Rejected(PluginError),
    Provider(PluginError),
}

pub type SharedRuntimeHost = Arc<tokio::sync::Mutex<NativePluginRuntimeHost>>;

/// Discovery refreshes and interaction share a queue only with their own process.
/// Existing plugins without discovery contributions retain their original host.
#[derive(Clone, Default)]
pub struct NativePluginRuntimeHosts {
    shared: SharedRuntimeHost,
    discovery: Arc<HashMap<String, SharedRuntimeHost>>,
}

impl NativePluginRuntimeHosts {
    pub fn shared(&self) -> SharedRuntimeHost {
        self.shared.clone()
    }

    pub fn for_plugin(&self, id: &str) -> SharedRuntimeHost {
        self.discovery.get(id).unwrap_or(&self.shared).clone()
    }

    pub fn prepare_process(&mut self, manifest: &NativePluginManifest) {
        if manifest
            .contributes
            .as_ref()
            .is_some_and(|c| !c.discovery_providers.is_empty())
        {
            Arc::make_mut(&mut self.discovery)
                .entry(manifest.id.clone())
                .or_default();
        } else if self.discovery.contains_key(&manifest.id) {
            Arc::make_mut(&mut self.discovery).remove(&manifest.id);
        }
    }

    pub fn retain_installed<'a>(&mut self, ids: impl Iterator<Item = &'a str>) {
        let installed = ids.collect::<std::collections::HashSet<_>>();
        Arc::make_mut(&mut self.discovery).retain(|id, _| installed.contains(id.as_str()));
    }
}

#[derive(Default)]
pub struct NativePluginRuntimeHost {
    output_wake: Arc<tokio::sync::Notify>,
    process_runtimes: HashMap<String, NativeProcessPluginRuntime>,
    process_permissions: HashMap<String, PluginPermissionSet>,
    #[cfg(feature = "wasm-runtime")]
    wasm_runtimes: HashMap<String, NativeWasmPluginRuntime>,
    #[cfg(feature = "wasm-runtime")]
    wasm_permissions: HashMap<String, PluginPermissionSet>,
    host_api_resolver: Option<NativeHostApiResolver>,
}

impl NativePluginRuntimeHost {
    pub fn set_discovery_invalidation_sink(
        &mut self,
        plugin_id: &str,
        generation: u64,
        sink: Arc<dyn Fn(PluginEvent) + Send + Sync>,
    ) {
        if let Some(runtime) = self.process_runtimes.get_mut(plugin_id)
            && runtime.generation == generation
            && self
                .process_permissions
                .get(plugin_id)
                .is_some_and(|permissions| {
                    permissions::capability_allowed(
                        permissions,
                        crate::capabilities::NATIVE_PLUGIN_CAPABILITY_UI_WRITE,
                    )
                })
        {
            runtime.discovery_invalidation_sink = Some(sink);
        }
    }
    /// Discovery calls bind to the activated manifest and exact process generation.
    pub async fn dispatch_discovery(
        &mut self,
        plugin_id: &str,
        generation: u64,
        source_id: &str,
        provider_id: &str,
        request: oxideterm_plugin_protocol::discovery::DiscoveryRequest,
        timeout: Duration,
    ) -> Result<
        (
            Result<oxideterm_plugin_protocol::discovery::DiscoveryReply, DiscoveryCommandError>,
            NativePluginRuntimePush,
        ),
        PluginError,
    > {
        // Local authorization failures have no effect on the shared process or its other sources.
        let reject = |error| {
            Ok((
                Err(DiscoveryCommandError::Rejected(error)),
                NativePluginRuntimePush {
                    messages: Vec::new(),
                    effects: Vec::new(),
                },
            ))
        };
        let runtime = self
            .process_runtimes
            .get_mut(plugin_id)
            .filter(|runtime| runtime.generation == generation)
            .ok_or_else(|| {
                PluginError::runtime("unavailable", "Discovery provider is no longer active")
            })?;
        let request = runtime
            .discovery_resources
            .cleanup_request(provider_id, source_id, request);
        let Some(provider) = runtime
            .discovery_providers
            .iter()
            .find(|p| p.id == provider_id)
        else {
            return reject(PluginError::protocol(
                "unsupported",
                "Unregistered discovery provider",
            ));
        };
        let (command, args) = match request.command_args(source_id, provider) {
            Ok(command) => command,
            Err(_) => {
                return reject(PluginError::protocol(
                    "unsupported",
                    "Operation is not supported by this discovery provider",
                ));
            }
        };
        let permissions = self
            .process_permissions
            .get(plugin_id)
            .cloned()
            .unwrap_or_default();
        if !permissions.capabilities.iter().any(|c| c == "ui.write") {
            return reject(PluginError::protocol(
                "permissionDenied",
                "Discovery requires approved ui.write",
            ));
        }
        runtime.next_discovery_request =
            runtime
                .next_discovery_request
                .checked_add(1)
                .ok_or_else(|| {
                    PluginError::runtime("unavailable", "Discovery request IDs exhausted")
                })?;
        let request_id = format!("discovery:{generation}:{}", runtime.next_discovery_request);
        let response = runtime
            .call(PluginRequest {
                request_id,
                kind: PluginRequestKind::DispatchCommand {
                    command: command.to_owned(),
                    args,
                },
                timeout_ms: Some(timeout.as_millis().try_into().unwrap_or(u64::MAX)),
            })
            .await?;
        runtime.last_completed_discovery_request = runtime.next_discovery_request;
        let messages = runtime.drain_outbound_messages();
        let effects = runtime.drain_outbound_effects();
        validate_outbound_message_permissions(
            &messages,
            &permissions,
            &runtime.discovery_providers,
        )?;
        validate_outbound_effect_permissions(&effects, &permissions)?;
        let result = match response.result {
            PluginResponseResult::Ok { value } => {
                runtime.discovery_resources.observe_ready(
                    provider_id,
                    source_id,
                    &request,
                    &value,
                )?;
                let definition = runtime
                    .discovery_providers
                    .iter()
                    .find(|p| p.id == provider_id)
                    .expect("the active runtime is exclusively borrowed for this dispatch");
                request
                    .parse_reply(definition, value)
                    .map_err(DiscoveryCommandError::Rejected)
            }
            PluginResponseResult::Error { mut error } => {
                if error.message.trim().is_empty() || error.message.chars().any(char::is_control) {
                    error.message = "Discovery operation failed".into();
                }
                Err(DiscoveryCommandError::Provider(error))
            }
        };
        if let Ok(reply) = &result {
            runtime
                .discovery_resources
                .record(provider_id, source_id, &request, reply);
        }
        Ok((result, NativePluginRuntimePush { messages, effects }))
    }
    pub fn dispose_connection_views(&mut self, plugin_id: &str, instance_id: &str) {
        if let Some(runtime) = self.process_runtimes.get_mut(plugin_id) {
            runtime.supervisor.dispose_connection_views(instance_id);
        }
    }

    pub fn process_generation(&self, plugin_id: &str) -> Option<u64> {
        self.process_runtimes
            .get(plugin_id)
            .map(|runtime| runtime.generation)
    }
    pub fn output_wake(&self) -> Arc<tokio::sync::Notify> {
        self.output_wake.clone()
    }

    pub async fn receive_pending_output(
        &mut self,
    ) -> Vec<(String, u64, Result<NativePluginRuntimePush, PluginError>)> {
        let mut pushes = Vec::new();
        for (plugin_id, runtime) in &mut self.process_runtimes {
            let result = time::timeout(
                runtime.supervisor.lifecycle_timeout(),
                runtime.receive_pending_output(),
            )
            .await
            .unwrap_or_else(|_| {
                Err(PluginError::runtime(
                    "process_response_timeout",
                    "Native plugin output handling timed out",
                ))
            });
            let messages = runtime.drain_outbound_messages();
            let effects = runtime.drain_outbound_effects();
            let permissions = self
                .process_permissions
                .get(plugin_id)
                .cloned()
                .unwrap_or_default();
            let result = result
                .and_then(|_| {
                    validate_outbound_message_permissions(
                        &messages,
                        &permissions,
                        &runtime.discovery_providers,
                    )
                })
                .and_then(|_| validate_outbound_effect_permissions(&effects, &permissions));
            match result {
                Err(error) => {
                    runtime.kill().await.ok();
                    pushes.push((plugin_id.clone(), runtime.generation, Err(error)));
                }
                Ok(()) if !messages.is_empty() || !effects.is_empty() => {
                    pushes.push((
                        plugin_id.clone(),
                        runtime.generation,
                        Ok(NativePluginRuntimePush { messages, effects }),
                    ));
                }
                Ok(()) => {}
            }
        }
        pushes
    }

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
        runtime.set_output_wake(self.output_wake.clone());
        // Multiple workspace hosts can publish into the same device-local discovery catalog.
        static NEXT_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let generation = NEXT_GENERATION
            .fetch_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |value| value.checked_add(1),
            )
            .map_err(|_| {
                PluginError::runtime("unavailable", "Plugin runtime generation exhausted")
            })?;
        runtime.generation = generation;
        runtime.discovery_providers = manifest
            .contributes
            .as_ref()
            .map(|c| c.discovery_providers.clone())
            .unwrap_or_default();
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
        validate_outbound_message_permissions(
            &messages,
            &permissions,
            &runtime.discovery_providers,
        )?;
        validate_outbound_effect_permissions(&effects, &permissions)?;

        if matches!(response.result, PluginResponseResult::Ok { .. }) {
            self.process_runtimes.insert(plugin_id.clone(), runtime);
            self.process_permissions
                .insert(plugin_id.clone(), permissions);
        }

        Ok(NativePluginRuntimeActivation {
            plugin_id,
            generation,
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
        #[cfg(not(feature = "wasm-runtime"))]
        {
            // Builds without the in-process runtime keep plugin discovery but
            // cannot execute Wasm plugins.
            let _ = (plugin_dir, entry, permissions, lifecycle_timeout);
            return Err(PluginError::runtime(
                WASM_RUNTIME_UNAVAILABLE_CODE,
                format!(
                    "WASM plugin support is not available in this OxideTerm build: {}",
                    manifest.id
                ),
            ));
        }

        #[cfg(feature = "wasm-runtime")]
        {
            let plugin_id = manifest.id.clone();
            if self.wasm_runtimes.contains_key(&plugin_id) {
                self.deactivate_plugin(&plugin_id).await?;
            }

            let mut runtime = NativeWasmPluginRuntime::new(
                plugin_id.clone(),
                plugin_dir,
                entry,
                lifecycle_timeout,
            );
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
            validate_outbound_message_permissions(&messages, &permissions, &[])?;
            validate_outbound_effect_permissions(&effects, &permissions)?;

            if matches!(response.result, PluginResponseResult::Ok { .. }) {
                self.wasm_runtimes.insert(plugin_id.clone(), runtime);
                self.wasm_permissions.insert(plugin_id.clone(), permissions);
            }

            Ok(NativePluginRuntimeActivation {
                plugin_id,
                generation: 0,
                response,
                messages,
                effects,
            })
        }
    }

    pub async fn dispatch_command(
        &mut self,
        plugin_id: &str,
        command: String,
        mut args: Value,
        timeout: Duration,
    ) -> Result<NativePluginRuntimeCommandDispatch, PluginError> {
        #[cfg(feature = "wasm-runtime")]
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
            validate_outbound_message_permissions(&messages, &permissions, &[])?;
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
        let Some(runtime) = self.process_runtimes.get_mut(plugin_id) else {
            oxideterm_plugin_protocol::zeroize_json_value(&mut args);
            return Err(PluginError::runtime(
                "plugin_runtime_not_active",
                format!("Native plugin runtime \"{plugin_id}\" is not active"),
            ));
        };
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
        validate_outbound_message_permissions(
            &messages,
            &permissions,
            &runtime.discovery_providers,
        )?;
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
        #[cfg(feature = "wasm-runtime")]
        if let Some(runtime) = self.wasm_runtimes.get_mut(plugin_id) {
            let permissions = self
                .wasm_permissions
                .get(plugin_id)
                .cloned()
                .unwrap_or_default();
            let response = runtime.send_event(event.clone()).await?;
            let messages = runtime.drain_outbound_messages();
            let effects = runtime.drain_outbound_effects();
            validate_outbound_message_permissions(&messages, &permissions, &[])?;
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
        validate_outbound_message_permissions(
            &messages,
            &permissions,
            &runtime.discovery_providers,
        )?;
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
        if let Some(runtime) = self.process_runtimes.get_mut(plugin_id) {
            let generation = runtime.generation;
            let deadline = Instant::now() + runtime.supervisor.lifecycle_timeout();
            let resources = runtime.discovery_resources.drain();
            // Bound shutdown even when the provider retained several resources.
            for (provider, source, request) in resources {
                let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                    break;
                };
                if !matches!(
                    self.dispatch_discovery(
                        plugin_id, generation, &source, &provider, request, remaining
                    )
                    .await,
                    Ok((Ok(_), _))
                ) {
                    break;
                }
            }
        }
        let response = if let Some(mut runtime) = self.process_runtimes.remove(plugin_id) {
            runtime.deactivate().await
        } else {
            #[cfg(feature = "wasm-runtime")]
            {
                if let Some(mut runtime) = self.wasm_runtimes.remove(plugin_id) {
                    runtime.deactivate().await
                } else {
                    Ok(PluginResponse::ok(
                        format!("deactivate:{plugin_id}"),
                        serde_json::json!({ "state": "not-running" }),
                    ))
                }
            }
            #[cfg(not(feature = "wasm-runtime"))]
            {
                Ok(PluginResponse::ok(
                    format!("deactivate:{plugin_id}"),
                    serde_json::json!({ "state": "not-running" }),
                ))
            }
        };
        // Permission snapshots must not outlive the runtime even when its
        // deactivation callback reports an error.
        self.process_permissions.remove(plugin_id);
        #[cfg(feature = "wasm-runtime")]
        self.wasm_permissions.remove(plugin_id);
        response
    }
}

mod discovery_resources;
mod paths;
mod permissions;
mod process;

#[cfg(feature = "wasm-runtime")]
pub use oxideterm_plugin_wasm_runtime::{NativeWasmPluginRuntime, resolve_wasm_runtime_entry};
pub use paths::resolve_process_runtime_entry;
use permissions::{
    install_process_host_call_handler, validate_outbound_effect_permissions,
    validate_outbound_message_permissions,
};
pub use process::NativeProcessPluginRuntime;
#[cfg(test)]
use process::{PluginProcessFrame, decode_process_output_frame};

#[cfg(test)]
mod tests;
