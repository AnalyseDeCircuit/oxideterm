// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use gpui::Task;
#[cfg(test)]
use std::sync::atomic::AtomicUsize;
use std::{
    future::Future,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

use super::plugin_lifecycle::{
    NativePluginConfirmDialog, NativePluginConfirmRequest, NativePluginOxideImportCoreResult,
    NativePluginOxideImportWorkerMessage, NativePluginOxidePostImportOptions,
    NativePluginProductUiEffect, NativePluginRuntimeDelivery, NativePluginSyncRequest,
    NativePluginTerminalRequest,
};
#[cfg(test)]
use super::plugin_lifecycle::{NativePluginSyncAction, NativePluginTerminalAction};

// Plugin-owned workers use bounded waits without involving the workspace heartbeat.
const NATIVE_PLUGIN_LIFECYCLE_TIMEOUT: Duration = Duration::from_secs(5);
const NATIVE_PLUGIN_DELIVERY_POLL_INTERVAL: Duration = Duration::from_millis(80);
const NATIVE_PLUGIN_RELEASE_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
const NATIVE_PLUGIN_WORKSPACE_RELEASED_CODE: &str = "plugin_workspace_released";
const NATIVE_PLUGIN_MANAGED_INSTALL_CANCELLED_CODE: &str = "managed_plugin_install_cancelled";

pub(in crate::workspace) enum PluginWorkspaceEvent {
    ManagerDeliveryReady,
    RuntimeRequestsReady,
    RuntimeSubscriptionSampleDue,
    RuntimeIntentsReady,
    OxideImportIntentsReady,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::workspace) enum PluginSubscriptionSample {
    Layout,
    Sessions,
    SavedForwards,
    Transfers,
    Profiler,
    Ide,
    Ai,
    EventLog,
}

/// Producer endpoints shared by one native plugin host resolver.
pub(in crate::workspace) struct PluginRuntimeRequestSenders {
    pub(in crate::workspace) confirm: delivery::ActiveDeliverySender<NativePluginConfirmRequest>,
    pub(in crate::workspace) terminal: delivery::ActiveDeliverySender<NativePluginTerminalRequest>,
    pub(in crate::workspace) sync: delivery::ActiveDeliverySender<NativePluginSyncRequest>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum PluginRuntimeAdapterRefresh {
    TerminalHooks,
    TerminalInputInterceptors,
    All,
}

pub(in crate::workspace) enum PluginRuntimeIntent {
    ApplyEffects {
        plugin_id: String,
        effects: Vec<plugin_runtime::PluginOutboundEffect>,
        refresh: PluginRuntimeAdapterRefresh,
    },
    StateChanged,
}

pub(in crate::workspace) enum PluginOxideImportIntent {
    Progress {
        plugin_id: Arc<str>,
        registration_id: Arc<str>,
        value: serde_json::Value,
    },
    Complete {
        plugin_id: Arc<str>,
        progress_registration_id: Option<Arc<str>>,
        request_id: String,
        result: Result<NativePluginOxideImportCoreResult, ()>,
        options: NativePluginOxidePostImportOptions,
        response_tx: std::sync::mpsc::Sender<plugin_runtime::PluginResponse>,
    },
}

struct PluginOxideImportContext {
    plugin_id: Arc<str>,
    progress_registration_id: Option<Arc<str>>,
    request_id: String,
    options: NativePluginOxidePostImportOptions,
    response_tx: std::sync::mpsc::Sender<plugin_runtime::PluginResponse>,
}

pub(in crate::workspace) const NATIVE_PLUGIN_RUNTIME_FAILURE_DIAGNOSTIC: &str =
    "native_plugin_runtime_failure";

fn native_plugin_runtime_failure_message(error: plugin_runtime::PluginError) -> String {
    let plugin_runtime::PluginError {
        code,
        message,
        recoverable: _,
    } = error;
    // Runtime text is plugin-controlled and may echo request data. Keep only
    // known host codes and erase the raw text at the delivery boundary.
    let _sensitive_message = Zeroizing::new(message);
    if code == plugin_runtime::WASM_RUNTIME_NOT_INSTALLED_CODE {
        code
    } else {
        NATIVE_PLUGIN_RUNTIME_FAILURE_DIAGNOSTIC.to_string()
    }
}

fn plugin_workspace_released_response(request_id: String) -> plugin_runtime::PluginResponse {
    plugin_runtime::PluginResponse::error(
        request_id,
        plugin_runtime::PluginError::runtime(
            NATIVE_PLUGIN_WORKSPACE_RELEASED_CODE,
            "Plugin workspace is shutting down",
        ),
    )
}

/// Owns plugin workers and reliable delivery independently from plugin page visibility.
pub(in crate::workspace) struct PluginWorkspaceEntity {
    task_runtime: Arc<tokio::runtime::Runtime>,
    registry: Arc<plugin_host::NativePluginRegistry>,
    runtime_host: Arc<tokio::sync::Mutex<plugin_runtime::NativePluginRuntimeHost>>,
    runtime_delivery_tx: delivery::ActiveDeliverySender<NativePluginRuntimeDelivery>,
    runtime_delivery_rx: std::sync::mpsc::Receiver<NativePluginRuntimeDelivery>,
    runtime_intents: VecDeque<PluginRuntimeIntent>,
    active_runtime_plugin_ids: HashSet<String>,
    owned_tasks: Arc<Mutex<HashMap<u64, Option<tokio::task::AbortHandle>>>>,
    next_owned_task_id: AtomicU64,
    release_shutdown_started: bool,
    #[cfg(test)]
    release_shutdown_invocations: Option<Arc<AtomicUsize>>,
    #[cfg(test)]
    release_shutdown_targets: Option<Arc<AtomicUsize>>,
    manager_state: plugin_manager::NativePluginManagerState,
    ui_state: plugin_ui::NativePluginUiState,
    manager_operation_in_flight: bool,
    manager_delivery_tx:
        delivery::ActiveDeliverySender<plugin_manager::NativePluginManagerDelivery>,
    manager_delivery_rx: std::sync::mpsc::Receiver<plugin_manager::NativePluginManagerDelivery>,
    manager_deliveries: VecDeque<plugin_manager::NativePluginManagerDelivery>,
    runtime_request_wake: delivery::ActiveDeliveryWake,
    confirm_tx: delivery::ActiveDeliverySender<NativePluginConfirmRequest>,
    confirm_rx: std::sync::mpsc::Receiver<NativePluginConfirmRequest>,
    confirm: Option<NativePluginConfirmDialog>,
    confirm_presence: oxideterm_gpui_ui::motion::ExitPresence,
    terminal_tx: delivery::ActiveDeliverySender<NativePluginTerminalRequest>,
    terminal_rx: std::sync::mpsc::Receiver<NativePluginTerminalRequest>,
    sync_tx: delivery::ActiveDeliverySender<NativePluginSyncRequest>,
    sync_rx: std::sync::mpsc::Receiver<NativePluginSyncRequest>,
    product_ui_effects: VecDeque<NativePluginProductUiEffect>,
    oxide_import_delivery_tx: delivery::ActiveDeliverySender<NativePluginOxideImportWorkerMessage>,
    oxide_import_delivery_rx: std::sync::mpsc::Receiver<NativePluginOxideImportWorkerMessage>,
    oxide_import_contexts: HashMap<u64, PluginOxideImportContext>,
    oxide_import_intents: VecDeque<PluginOxideImportIntent>,
    next_oxide_import_id: u64,
    runtime_services_started: bool,
    subscription_samples: Vec<PluginSubscriptionSample>,
    subscription_snapshots: HashMap<PluginSubscriptionSample, serde_json::Value>,
    subscription_sampler_generation: u64,
    subscription_sampler_running: bool,
    subscription_sampler_task: Option<Task<()>>,
    transfer_progress_last_emitted: Option<Instant>,
    runtime_profiler_last_emitted: Option<Instant>,
    event_log_last_id: u64,
}

impl PluginWorkspaceEntity {
    pub(in crate::workspace) fn new(
        task_runtime: Arc<tokio::runtime::Runtime>,
        registry: plugin_host::NativePluginRegistry,
        cx: &mut Context<Self>,
    ) -> Self {
        let (runtime_delivery_tx, runtime_delivery_rx) = delivery::ActiveDeliverySender::channel();
        let (manager_delivery_tx, manager_delivery_rx) = delivery::ActiveDeliverySender::channel();
        let runtime_request_wake = delivery::ActiveDeliveryWake::default();
        let (confirm_tx, confirm_rx) =
            delivery::ActiveDeliverySender::channel_with_wake(runtime_request_wake.clone());
        let (terminal_tx, terminal_rx) =
            delivery::ActiveDeliverySender::channel_with_wake(runtime_request_wake.clone());
        let (sync_tx, sync_rx) =
            delivery::ActiveDeliverySender::channel_with_wake(runtime_request_wake.clone());
        let (oxide_import_delivery_tx, oxide_import_delivery_rx) =
            delivery::ActiveDeliverySender::channel();
        let entity = Self {
            task_runtime,
            registry: Arc::new(registry),
            runtime_host: Arc::new(tokio::sync::Mutex::new(
                plugin_runtime::NativePluginRuntimeHost::default(),
            )),
            runtime_delivery_tx,
            runtime_delivery_rx,
            runtime_intents: VecDeque::new(),
            active_runtime_plugin_ids: HashSet::new(),
            owned_tasks: Arc::new(Mutex::new(HashMap::new())),
            next_owned_task_id: AtomicU64::new(1),
            release_shutdown_started: false,
            #[cfg(test)]
            release_shutdown_invocations: None,
            #[cfg(test)]
            release_shutdown_targets: None,
            manager_state: plugin_manager::NativePluginManagerState::new(),
            ui_state: plugin_ui::NativePluginUiState::default(),
            manager_operation_in_flight: false,
            manager_delivery_tx,
            manager_delivery_rx,
            manager_deliveries: VecDeque::new(),
            runtime_request_wake,
            confirm_tx,
            confirm_rx,
            confirm: None,
            confirm_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            terminal_tx,
            terminal_rx,
            sync_tx,
            sync_rx,
            product_ui_effects: VecDeque::new(),
            oxide_import_delivery_tx,
            oxide_import_delivery_rx,
            oxide_import_contexts: HashMap::new(),
            oxide_import_intents: VecDeque::new(),
            next_oxide_import_id: 1,
            runtime_services_started: false,
            subscription_samples: Vec::new(),
            subscription_snapshots: HashMap::new(),
            subscription_sampler_generation: 0,
            subscription_sampler_running: false,
            subscription_sampler_task: None,
            transfer_progress_last_emitted: None,
            runtime_profiler_last_emitted: None,
            event_log_last_id: 0,
        };
        entity.schedule_runtime_delivery(cx);
        entity.schedule_manager_delivery(cx);
        entity.schedule_runtime_request_delivery(cx);
        entity.schedule_oxide_import_delivery(cx);
        entity.schedule_release_shutdown(cx);
        entity
    }

    pub(in crate::workspace) fn manager_operation_in_flight(&self) -> bool {
        self.manager_operation_in_flight
    }

    pub(in crate::workspace) fn manager_state(&self) -> &plugin_manager::NativePluginManagerState {
        &self.manager_state
    }

    pub(in crate::workspace) fn manager_state_mut(
        &mut self,
    ) -> &mut plugin_manager::NativePluginManagerState {
        &mut self.manager_state
    }

    pub(in crate::workspace) fn ui_state(&self) -> &plugin_ui::NativePluginUiState {
        &self.ui_state
    }

    pub(in crate::workspace) fn ui_state_mut(&mut self) -> &mut plugin_ui::NativePluginUiState {
        &mut self.ui_state
    }

    pub(in crate::workspace) fn select_sidebar_panel(
        &mut self,
        selection: plugin_ui::NativePluginSidebarPanelSelection,
    ) {
        let previous = self.manager_state.active_sidebar_panel.replace(selection);
        if let Some(previous) = previous
            && self.manager_state.active_sidebar_panel.as_ref() != Some(&previous)
        {
            self.ui_state
                .remove_surface(&previous.plugin_id, "sidebarPanel", &previous.panel_id);
        }
    }

    pub(in crate::workspace) fn registry(&self) -> &plugin_host::NativePluginRegistry {
        &self.registry
    }

    pub(in crate::workspace) fn registry_mut(&mut self) -> &mut plugin_host::NativePluginRegistry {
        Arc::make_mut(&mut self.registry)
    }

    pub(in crate::workspace) fn registry_snapshot(&self) -> Arc<plugin_host::NativePluginRegistry> {
        Arc::clone(&self.registry)
    }

    pub(in crate::workspace) fn replace_registry(
        &mut self,
        registry: plugin_host::NativePluginRegistry,
    ) {
        let enabled_runtime_plugin_ids = registry
            .plugins()
            .iter()
            .filter(|plugin| {
                matches!(
                    plugin.state,
                    plugin_host::NativePluginState::ReadyProcess
                        | plugin_host::NativePluginState::ReadyWasm
                        | plugin_host::NativePluginState::Loading
                        | plugin_host::NativePluginState::Active
                )
            })
            .map(|plugin| plugin.manifest.id.as_str())
            .collect::<HashSet<_>>();
        let stale_runtime_plugin_ids = self
            .active_runtime_plugin_ids
            .iter()
            .filter(|plugin_id| !enabled_runtime_plugin_ids.contains(plugin_id.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        self.registry = Arc::new(registry);
        for plugin_id in stale_runtime_plugin_ids {
            self.start_runtime_deactivation(plugin_id);
        }
    }

    pub(in crate::workspace) fn set_plugin_enabled(
        &mut self,
        plugin_id: &str,
        enabled: bool,
    ) -> Result<(), String> {
        self.registry_mut().set_plugin_enabled(plugin_id, enabled)?;
        if !enabled {
            self.start_runtime_deactivation(plugin_id.to_string());
        }
        Ok(())
    }

    pub(in crate::workspace) fn uninstall_plugin(
        &mut self,
        plugin_id: &str,
        remove_storage: bool,
    ) -> Result<(), String> {
        self.registry_mut()
            .uninstall_plugin(plugin_id, remove_storage)?;
        self.start_runtime_deactivation(plugin_id.to_string());
        Ok(())
    }

    pub(in crate::workspace) fn set_plugin_setting_value(
        &mut self,
        plugin_id: &str,
        setting_id: &str,
        value: serde_json::Value,
    ) -> Result<(), String> {
        // AI-provided settings must remain on the declared non-secret settings
        // path; plugin-scoped secrets have a separate user-approved host API.
        if oxideterm_ai::sanitize_json_for_ai(&value) != value {
            return Err("Secret-like plugin settings must be entered by the user.".to_string());
        }
        self.registry_mut()
            .set_plugin_setting_value(plugin_id, setting_id, value)
    }

    pub(in crate::workspace) fn runtime_host(
        &self,
    ) -> Arc<tokio::sync::Mutex<plugin_runtime::NativePluginRuntimeHost>> {
        self.runtime_host.clone()
    }

    fn spawn_owned_task<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let task_id = self.next_owned_task_id.fetch_add(1, Ordering::Relaxed);
        {
            let mut tasks = self
                .owned_tasks
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            tasks.insert(task_id, None);
        }
        let owned_tasks = Arc::clone(&self.owned_tasks);
        let task = self.task_runtime.spawn(async move {
            future.await;
            owned_tasks
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(&task_id);
        });
        let abort_handle = task.abort_handle();
        let mut tasks = self
            .owned_tasks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(handle) = tasks.get_mut(&task_id) {
            *handle = Some(abort_handle);
        }
    }

    pub(in crate::workspace) fn start_runtime_bootstrap(
        &mut self,
        host_api_resolver: plugin_runtime::NativeHostApiResolver,
        wasm_sidecar_path: Option<PathBuf>,
    ) -> bool {
        if self.release_shutdown_started {
            return false;
        }
        let process_plans = self.registry.process_activation_plans();
        let wasm_plans = self.registry.wasm_activation_plans();
        if process_plans.is_empty() && wasm_plans.is_empty() {
            return false;
        }
        self.start_runtime_services();
        for plan in &process_plans {
            let _ = self.registry_mut().mark_runtime_loading(&plan.plugin_id);
        }
        for plan in &wasm_plans {
            let _ = self.registry_mut().mark_runtime_loading(&plan.plugin_id);
        }

        let host = self.runtime_host.clone();
        let delivery_tx = self.runtime_delivery_tx.clone();
        self.spawn_owned_task(async move {
            let mut host = host.lock().await;
            host.set_wasm_sidecar_path(wasm_sidecar_path);
            host.set_host_api_resolver(host_api_resolver);
            // Preserve deterministic activation order across process and WASM runtimes.
            for plan in process_plans {
                let plugin_id = plan.plugin_id.clone();
                let result = match super::plugin_lifecycle::native_plugin_permissions(
                    &plan.manifest,
                    true,
                ) {
                    Ok(permissions) => {
                        host.activate_process_plugin(
                            plan.manifest,
                            plan.install_dir,
                            plan.entry,
                            permissions,
                            NATIVE_PLUGIN_LIFECYCLE_TIMEOUT,
                        )
                        .await
                    }
                    Err(error) => Err(error),
                };
                if delivery_tx
                    .send(NativePluginRuntimeDelivery::Activation { plugin_id, result })
                    .is_err()
                {
                    return;
                }
            }
            for plan in wasm_plans {
                let plugin_id = plan.plugin_id.clone();
                let result =
                    match super::plugin_lifecycle::native_plugin_permissions(&plan.manifest, false)
                    {
                        Ok(permissions) => {
                            host.activate_wasm_plugin(
                                plan.manifest,
                                plan.install_dir,
                                plan.entry,
                                permissions,
                                NATIVE_PLUGIN_LIFECYCLE_TIMEOUT,
                            )
                            .await
                        }
                        Err(error) => Err(error),
                    };
                if delivery_tx
                    .send(NativePluginRuntimeDelivery::Activation { plugin_id, result })
                    .is_err()
                {
                    return;
                }
            }
        });
        true
    }

    pub(in crate::workspace) fn start_runtime_command(
        &self,
        plugin_id: String,
        command: String,
        host_api_resolver: plugin_runtime::NativeHostApiResolver,
    ) {
        self.start_runtime_command_with_arguments(
            plugin_id,
            command,
            serde_json::Value::Null,
            host_api_resolver,
        );
    }

    pub(in crate::workspace) fn start_runtime_command_with_arguments(
        &self,
        plugin_id: String,
        command: String,
        arguments: serde_json::Value,
        host_api_resolver: plugin_runtime::NativeHostApiResolver,
    ) {
        if self.release_shutdown_started {
            return;
        }
        let host = self.runtime_host.clone();
        let delivery_tx = self.runtime_delivery_tx.clone();
        self.spawn_owned_task(async move {
            let mut host = host.lock().await;
            host.set_host_api_resolver(host_api_resolver);
            let result = host
                .dispatch_command(
                    &plugin_id,
                    command,
                    arguments,
                    NATIVE_PLUGIN_LIFECYCLE_TIMEOUT,
                )
                .await;
            let _ = delivery_tx
                .send(NativePluginRuntimeDelivery::CommandDispatch { plugin_id, result });
        });
    }

    pub(in crate::workspace) fn start_runtime_event(
        &self,
        plugin_id: String,
        event: plugin_runtime::PluginEvent,
        host_api_resolver: plugin_runtime::NativeHostApiResolver,
    ) {
        if self.release_shutdown_started {
            return;
        }
        let host = self.runtime_host.clone();
        let delivery_tx = self.runtime_delivery_tx.clone();
        self.spawn_owned_task(async move {
            let mut host = host.lock().await;
            host.set_host_api_resolver(host_api_resolver);
            let result = host
                .dispatch_event(&plugin_id, event, NATIVE_PLUGIN_LIFECYCLE_TIMEOUT)
                .await;
            let _ =
                delivery_tx.send(NativePluginRuntimeDelivery::EventDispatch { plugin_id, result });
        });
    }

    fn start_runtime_deactivation(&mut self, plugin_id: String) {
        if self.release_shutdown_started {
            return;
        }
        // Declarative views are no longer valid once their runtime starts deactivating.
        self.ui_state.remove_plugin(&plugin_id);
        let host = self.runtime_host.clone();
        let delivery_tx = self.runtime_delivery_tx.clone();
        self.spawn_owned_task(async move {
            let result = host.lock().await.deactivate_plugin(&plugin_id).await;
            let _ =
                delivery_tx.send(NativePluginRuntimeDelivery::Deactivation { plugin_id, result });
        });
    }

    pub(in crate::workspace) fn take_runtime_intents(&mut self) -> VecDeque<PluginRuntimeIntent> {
        std::mem::take(&mut self.runtime_intents)
    }

    pub(in crate::workspace) fn start_oxide_import(
        &mut self,
        store: oxideterm_connections::ConnectionStore,
        plugin_id: String,
        request_id: String,
        bytes: Vec<u8>,
        password: Zeroizing<String>,
        options: oxideterm_plugin_host_api::sync::NativePluginOxideImportOptions,
        progress_registration_id: Option<String>,
        response_tx: std::sync::mpsc::Sender<plugin_runtime::PluginResponse>,
    ) {
        if self.release_shutdown_started {
            // Dropping these owned inputs also erases the zeroizing password.
            let _ = response_tx.send(plugin_workspace_released_response(request_id));
            return;
        }
        let operation_id = self.next_oxide_import_id;
        self.next_oxide_import_id = self.next_oxide_import_id.wrapping_add(1).max(1);
        let oxideterm_plugin_host_api::sync::NativePluginOxideImportOptions {
            oxide_options,
            import_app_settings,
            selected_app_settings_sections,
            import_plugin_settings,
            selected_plugin_ids,
            import_quick_commands,
            quick_command_strategy,
        } = options;
        self.oxide_import_contexts.insert(
            operation_id,
            PluginOxideImportContext {
                plugin_id: Arc::from(plugin_id),
                progress_registration_id: progress_registration_id.map(Arc::from),
                request_id,
                options: NativePluginOxidePostImportOptions {
                    import_app_settings,
                    selected_app_settings_sections,
                    import_plugin_settings,
                    selected_plugin_ids,
                    import_quick_commands,
                    quick_command_strategy,
                },
                response_tx,
            },
        );

        let delivery_tx = self.oxide_import_delivery_tx.clone();
        std::thread::spawn(move || {
            let mut store = store;
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                oxideterm_plugin_host_api::sync::native_plugin_apply_oxide_import_core_with_progress(
                    &mut store,
                    &bytes,
                    &password,
                    oxide_options,
                    |stage, current, total| {
                        let _ = delivery_tx.send(
                            NativePluginOxideImportWorkerMessage::Progress {
                                operation_id,
                                stage: stage.to_string(),
                                current,
                                total,
                            },
                        );
                    },
                )
            }))
            .map_err(|_| ())
            .and_then(|result| {
                result
                    .map(|envelope| NativePluginOxideImportCoreResult { store, envelope })
                    .map_err(|error| {
                        // Import errors may include paths or decrypted payload details.
                        let _sensitive_error = Zeroizing::new(error);
                    })
            });
            let _ = delivery_tx.send(NativePluginOxideImportWorkerMessage::Done {
                operation_id,
                result,
            });
        });
    }

    pub(in crate::workspace) fn take_oxide_import_intents(
        &mut self,
    ) -> VecDeque<PluginOxideImportIntent> {
        std::mem::take(&mut self.oxide_import_intents)
    }

    pub(in crate::workspace) fn start_package_install(
        &mut self,
        settings_path: PathBuf,
        download_url: Zeroizing<String>,
        checksum: Option<String>,
        overwrite: bool,
    ) -> bool {
        if self.manager_operation_in_flight || self.release_shutdown_started {
            return false;
        }
        self.manager_operation_in_flight = true;
        let delivery_tx = self.manager_delivery_tx.clone();
        self.spawn_owned_task(async move {
            let result = plugin_host::NativePluginRegistry::install_plugin_package_from_url(
                &settings_path,
                download_url.trim(),
                checksum.as_deref(),
                overwrite,
            )
            .await;
            let outcome = match result {
                Ok(result) => plugin_manager::NativePluginInstallOutcome::Installed(result),
                Err(error) => {
                    let error = Zeroizing::new(error);
                    match plugin_host::native_plugin_conflict_id(&error) {
                        Some(plugin_id) => {
                            plugin_manager::NativePluginInstallOutcome::Conflict { plugin_id }
                        }
                        None => plugin_manager::NativePluginInstallOutcome::Failed,
                    }
                }
            };
            // Move the original request values into the result; no duplicate
            // URL or checksum is retained while the package worker runs.
            let _ = delivery_tx.send(plugin_manager::NativePluginManagerDelivery::Install {
                download_url,
                checksum,
                outcome,
            });
        });
        true
    }

    pub(in crate::workspace) fn start_managed_package_install(
        &mut self,
        settings_path: PathBuf,
        expected_id: String,
        checksum: String,
        package_bytes: Zeroizing<Vec<u8>>,
        overwrite: bool,
        cancellation: tokio_util::sync::CancellationToken,
    ) -> Option<
        tokio::sync::oneshot::Receiver<Result<plugin_host::NativePluginUrlInstallResult, String>>,
    > {
        if self.manager_operation_in_flight || self.release_shutdown_started {
            return None;
        }
        self.manager_operation_in_flight = true;
        let (result_tx, result_rx) = tokio::sync::oneshot::channel();
        self.spawn_owned_task(async move {
            if cancellation.is_cancelled() {
                let _ =
                    result_tx.send(Err(NATIVE_PLUGIN_MANAGED_INSTALL_CANCELLED_CODE.to_owned()));
                return;
            }
            // The package bytes stay zeroizing while the registry validates and extracts them.
            let result = plugin_host::NativePluginRegistry::install_managed_plugin_package(
                &settings_path,
                &expected_id,
                Some(&checksum),
                &package_bytes,
                overwrite,
            );
            let _ = result_tx.send(result);
        });
        Some(result_rx)
    }

    pub(in crate::workspace) fn finish_managed_package_install(
        &mut self,
        settings_path: &Path,
        installed: bool,
    ) {
        self.manager_operation_in_flight = false;
        if installed {
            self.replace_registry(plugin_host::NativePluginRegistry::discover(settings_path));
        }
    }

    pub(in crate::workspace) fn start_update_check(
        &mut self,
        registry_url: Zeroizing<String>,
        installed: Vec<plugin_host::NativePluginInstalledInfo>,
    ) -> bool {
        if self.manager_operation_in_flight || self.release_shutdown_started {
            return false;
        }
        self.manager_operation_in_flight = true;
        let delivery_tx = self.manager_delivery_tx.clone();
        self.spawn_owned_task(async move {
            let result =
                match plugin_host::NativePluginRegistry::fetch_plugin_registry(registry_url.trim())
                    .await
                {
                    Ok(index) => Some(plugin_host::NativePluginRegistry::check_plugin_updates(
                        index, &installed,
                    )),
                    Err(error) => {
                        // Registry errors may echo credential-bearing URLs.
                        drop(Zeroizing::new(error));
                        None
                    }
                };
            let _ = delivery_tx.send(plugin_manager::NativePluginManagerDelivery::CheckUpdates(
                result,
            ));
        });
        true
    }

    pub(in crate::workspace) fn start_wasm_runtime_install(
        &mut self,
        settings_path: PathBuf,
    ) -> bool {
        if self.manager_operation_in_flight || self.release_shutdown_started {
            return false;
        }
        self.manager_operation_in_flight = true;
        let delivery_tx = self.manager_delivery_tx.clone();
        self.spawn_owned_task(async move {
            let result =
                oxideterm_plugin_runtime_install::install_wasm_runtime_sidecar(&settings_path)
                    .await;
            let result = result.map_err(Zeroizing::new).ok();
            let _ = delivery_tx
                .send(plugin_manager::NativePluginManagerDelivery::InstallWasmRuntime(result));
        });
        true
    }

    pub(in crate::workspace) fn apply_manager_deliveries(
        &mut self,
        settings_path: &std::path::Path,
        i18n: &I18n,
    ) -> bool {
        let mut bootstrap_runtime = false;
        while let Some(delivery) = self.manager_deliveries.pop_front() {
            bootstrap_runtime |= self.apply_manager_delivery(delivery, settings_path, i18n);
        }
        bootstrap_runtime
    }

    fn apply_manager_delivery(
        &mut self,
        delivery: plugin_manager::NativePluginManagerDelivery,
        settings_path: &std::path::Path,
        i18n: &I18n,
    ) -> bool {
        match delivery {
            plugin_manager::NativePluginManagerDelivery::Install {
                download_url,
                checksum,
                outcome,
            } => match outcome {
                plugin_manager::NativePluginInstallOutcome::Installed(result) => {
                    let installed_id = result.manifest.id.clone();
                    let message = i18n
                        .t("plugin.url_install_success")
                        .replace("{{name}}", &result.manifest.name);
                    self.replace_registry(plugin_host::NativePluginRegistry::discover(
                        settings_path,
                    ));
                    self.manager_state
                        .available_updates
                        .retain(|entry| entry.id != installed_id);
                    self.manager_state.pending_overwrite = None;
                    self.manager_state.operation_status =
                        plugin_manager::NativePluginManagerOperationStatus::Success(message);
                    true
                }
                plugin_manager::NativePluginInstallOutcome::Conflict { plugin_id } => {
                    // The retry keeps the only secret-bearing URL owner.
                    self.manager_state.pending_overwrite =
                        Some(plugin_manager::NativePluginPendingOverwrite {
                            plugin_id,
                            download_url,
                            checksum,
                        });
                    self.manager_state.operation_status =
                        plugin_manager::NativePluginManagerOperationStatus::Error(
                            i18n.t("plugin.url_conflict_title"),
                        );
                    false
                }
                plugin_manager::NativePluginInstallOutcome::Failed => {
                    self.manager_state.operation_status =
                        plugin_manager::NativePluginManagerOperationStatus::Error(
                            i18n.t("plugin.install_error"),
                        );
                    false
                }
            },
            plugin_manager::NativePluginManagerDelivery::CheckUpdates(result) => {
                match result {
                    Some(updates) => {
                        let update_count = updates.len();
                        self.manager_state.available_updates = updates;
                        self.manager_state.operation_status =
                            plugin_manager::NativePluginManagerOperationStatus::Success(format!(
                                "{update_count} {}",
                                i18n.t("plugin.updates")
                            ));
                    }
                    None => {
                        self.manager_state.operation_status =
                            plugin_manager::NativePluginManagerOperationStatus::Error(
                                i18n.t("plugin.registry_error"),
                            );
                    }
                }
                false
            }
            plugin_manager::NativePluginManagerDelivery::InstallWasmRuntime(result) => match result
            {
                Some(result) => {
                    self.manager_state.operation_status =
                        plugin_manager::NativePluginManagerOperationStatus::Success(
                            i18n.t("plugin.wasm_runtime_install_success")
                                .replace("{{version}}", &result.version),
                        );
                    true
                }
                None => {
                    self.manager_state.operation_status =
                        plugin_manager::NativePluginManagerOperationStatus::Error(
                            i18n.t("plugin.install_error"),
                        );
                    false
                }
            },
        }
    }

    pub(in crate::workspace) fn runtime_request_senders(&self) -> PluginRuntimeRequestSenders {
        // These are lightweight channel endpoints; request payloads stay unique
        // and are moved through the channels without being cloned.
        PluginRuntimeRequestSenders {
            confirm: self.confirm_tx.clone(),
            terminal: self.terminal_tx.clone(),
            sync: self.sync_tx.clone(),
        }
    }

    pub(in crate::workspace) fn promote_confirm_request(&mut self) -> bool {
        if self.confirm.is_some() {
            return false;
        }
        let Ok(request) = self.confirm_rx.try_recv() else {
            return false;
        };
        self.confirm = Some(request.into());
        self.confirm_presence.reopen();
        true
    }

    pub(in crate::workspace) fn confirm_dialog(&self) -> Option<&NativePluginConfirmDialog> {
        self.confirm.as_ref()
    }

    pub(in crate::workspace) fn confirm_phase(&self) -> oxideterm_gpui_ui::motion::ExitPhase {
        self.confirm_presence.phase()
    }

    pub(in crate::workspace) fn begin_confirm_exit(&mut self, confirmed: bool) -> Option<u64> {
        let dialog = self.confirm.as_mut()?;
        let generation = self.confirm_presence.begin_exit()?;
        // Resolve exactly once while retaining the dialog for its exit frame.
        dialog.respond(confirmed);
        Some(generation)
    }

    pub(in crate::workspace) fn finish_confirm_exit(&mut self, generation: u64) -> bool {
        if !self.confirm_presence.finish_exit(generation) {
            return false;
        }
        self.confirm = None;
        self.promote_confirm_request()
    }

    pub(in crate::workspace) fn take_terminal_requests(
        &mut self,
    ) -> delivery::ChannelDrain<NativePluginTerminalRequest> {
        delivery::drain_channel(&self.terminal_rx, delivery::USER_ACTION_DELIVERY_BUDGET)
    }

    pub(in crate::workspace) fn take_sync_requests(
        &mut self,
    ) -> delivery::ChannelDrain<NativePluginSyncRequest> {
        delivery::drain_channel(&self.sync_rx, delivery::USER_ACTION_DELIVERY_BUDGET)
    }

    pub(in crate::workspace) fn enqueue_product_ui_effect(
        &mut self,
        effect: NativePluginProductUiEffect,
    ) {
        self.product_ui_effects.push_back(effect);
        self.runtime_request_wake.mark();
    }

    pub(in crate::workspace) fn take_product_ui_effects(
        &mut self,
    ) -> (VecDeque<NativePluginProductUiEffect>, bool) {
        let started_at = Instant::now();
        let mut effects = VecDeque::new();
        while delivery::USER_ACTION_DELIVERY_BUDGET.allows_next(effects.len(), started_at.elapsed())
        {
            let Some(effect) = self.product_ui_effects.pop_front() else {
                break;
            };
            effects.push_back(effect);
        }
        (effects, !self.product_ui_effects.is_empty())
    }

    pub(in crate::workspace) fn mark_runtime_requests_ready(&self) {
        self.runtime_request_wake.mark();
    }

    pub(in crate::workspace) fn start_runtime_services(&mut self) -> bool {
        if self.runtime_services_started || self.release_shutdown_started {
            return false;
        }
        self.runtime_services_started = true;
        true
    }

    fn schedule_release_shutdown(&self, cx: &mut Context<Self>) {
        cx.on_release(|entity, _| {
            entity.begin_release_shutdown();
        })
        .detach();
    }

    fn begin_release_shutdown(&mut self) -> bool {
        if self.release_shutdown_started {
            return false;
        }
        self.release_shutdown_started = true;
        self.runtime_services_started = false;
        self.manager_operation_in_flight = false;
        self.subscription_sampler_running = false;
        self.subscription_sampler_generation = self.subscription_sampler_generation.wrapping_add(1);
        self.subscription_sampler_task.take();
        self.subscription_samples.clear();
        self.subscription_snapshots.clear();

        // Stopping every wake rejects new foreground delivery before receivers drop.
        self.manager_delivery_tx.wake().stop();
        self.runtime_delivery_tx.wake().stop();
        self.runtime_request_wake.stop();
        self.oxide_import_delivery_tx.wake().stop();
        self.reject_pending_runtime_requests();

        let abort_handles = self
            .owned_tasks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .drain()
            .filter_map(|(_, handle)| handle)
            .collect::<Vec<_>>();
        for abort_handle in abort_handles {
            abort_handle.abort();
        }
        // Dropping queued producer results releases owned URLs and import data promptly.
        while let Ok(delivery) = self.runtime_delivery_rx.try_recv() {
            drop(delivery);
        }
        while let Ok(delivery) = self.manager_delivery_rx.try_recv() {
            drop(delivery);
        }
        while let Ok(delivery) = self.oxide_import_delivery_rx.try_recv() {
            drop(delivery);
        }

        let mut plugin_ids = std::mem::take(&mut self.active_runtime_plugin_ids);
        plugin_ids.extend(
            self.registry
                .plugins()
                .iter()
                .filter(|plugin| {
                    matches!(
                        plugin.state,
                        plugin_host::NativePluginState::ReadyProcess
                            | plugin_host::NativePluginState::ReadyWasm
                            | plugin_host::NativePluginState::Loading
                            | plugin_host::NativePluginState::Active
                    )
                })
                .map(|plugin| plugin.manifest.id.clone()),
        );
        let mut plugin_ids = plugin_ids.into_iter().collect::<Vec<_>>();
        plugin_ids.sort_unstable();

        #[cfg(test)]
        {
            if let Some(invocations) = &self.release_shutdown_invocations {
                invocations.fetch_add(1, Ordering::SeqCst);
            }
            if let Some(targets) = &self.release_shutdown_targets {
                targets.store(plugin_ids.len(), Ordering::SeqCst);
            }
        }

        let runtime_host = Arc::clone(&self.runtime_host);
        self.task_runtime.spawn(async move {
            // One total deadline bounds lock acquisition and every process/WASM teardown.
            let _ = tokio::time::timeout(NATIVE_PLUGIN_RELEASE_SHUTDOWN_TIMEOUT, async move {
                let mut runtime_host = runtime_host.lock().await;
                for plugin_id in plugin_ids {
                    let _ = runtime_host.deactivate_plugin(&plugin_id).await;
                }
            })
            .await;
        });
        true
    }

    fn reject_pending_runtime_requests(&mut self) {
        if let Some(mut dialog) = self.confirm.take() {
            dialog.respond(false);
        }
        while let Ok(request) = self.confirm_rx.try_recv() {
            let _ = request.response_tx.send(false);
        }
        while let Ok(request) = self.terminal_rx.try_recv() {
            let response = plugin_workspace_released_response(request.request_id);
            let _ = request.response_tx.send(response);
            // The action is dropped in place; terminal text is never cloned for shutdown.
            drop(request.action);
        }
        while let Ok(request) = self.sync_rx.try_recv() {
            let response = plugin_workspace_released_response(request.request_id);
            let _ = request.response_tx.send(response);
            // ImportOxide owns a Zeroizing password that is erased when this action drops.
            drop(request.action);
        }
        for (_, context) in self.oxide_import_contexts.drain() {
            let response = plugin_workspace_released_response(context.request_id);
            let _ = context.response_tx.send(response);
        }
        while let Some(intent) = self.oxide_import_intents.pop_front() {
            if let PluginOxideImportIntent::Complete {
                request_id,
                response_tx,
                ..
            } = intent
            {
                let _ = response_tx.send(plugin_workspace_released_response(request_id));
            }
        }
        self.product_ui_effects.clear();
        self.runtime_intents.clear();
        self.manager_deliveries.clear();
    }

    pub(in crate::workspace) fn configure_subscription_samples(
        &mut self,
        samples: Vec<(PluginSubscriptionSample, serde_json::Value)>,
        event_log_last_id: Option<u64>,
        cx: &mut Context<Self>,
    ) {
        if self.release_shutdown_started {
            return;
        }
        // These samples serve registered runtime event subscriptions. They are
        // intentionally independent from Plugin Manager page visibility.
        let sample_kinds = samples.iter().map(|(kind, _)| *kind).collect::<Vec<_>>();
        self.subscription_samples = sample_kinds;
        self.subscription_snapshots = samples.into_iter().collect();
        if let Some(last_id) = event_log_last_id {
            self.event_log_last_id = last_id;
        }

        if self.subscription_samples.is_empty() {
            self.subscription_sampler_running = false;
            self.subscription_sampler_generation =
                self.subscription_sampler_generation.wrapping_add(1);
            self.subscription_sampler_task.take();
            return;
        }
        if self.subscription_sampler_running {
            return;
        }

        self.subscription_sampler_running = true;
        self.subscription_sampler_generation = self.subscription_sampler_generation.wrapping_add(1);
        let generation = self.subscription_sampler_generation;
        self.subscription_sampler_task = Some(cx.spawn(async move |entity, cx| {
            loop {
                Timer::after(NATIVE_PLUGIN_DELIVERY_POLL_INTERVAL).await;
                let keep_running = entity
                    .update(cx, |entity, cx| {
                        if !entity.subscription_sampler_running
                            || entity.subscription_sampler_generation != generation
                        {
                            return false;
                        }
                        cx.emit(PluginWorkspaceEvent::RuntimeSubscriptionSampleDue);
                        true
                    })
                    .unwrap_or(false);
                if !keep_running {
                    break;
                }
            }
        }));
    }

    pub(in crate::workspace) fn subscription_samples(&self) -> Vec<PluginSubscriptionSample> {
        self.subscription_samples.clone()
    }

    pub(in crate::workspace) fn update_subscription_snapshot(
        &mut self,
        kind: PluginSubscriptionSample,
        next: serde_json::Value,
    ) -> (Option<serde_json::Value>, serde_json::Value) {
        let Some(previous) = self.subscription_snapshots.get_mut(&kind) else {
            return (None, next);
        };
        if *previous == next {
            return (None, next);
        }
        let retained = next.clone();
        let previous = std::mem::replace(previous, retained);
        (Some(previous), next)
    }

    pub(in crate::workspace) fn transfer_progress_due(&mut self, interval: Duration) -> bool {
        let due = self
            .transfer_progress_last_emitted
            .map(|last_emitted| last_emitted.elapsed() >= interval)
            .unwrap_or(true);
        if due {
            self.transfer_progress_last_emitted = Some(Instant::now());
        }
        due
    }

    pub(in crate::workspace) fn runtime_profiler_metrics_due(
        &mut self,
        interval: Duration,
    ) -> bool {
        let due = self
            .runtime_profiler_last_emitted
            .map(|last_emitted| last_emitted.elapsed() >= interval)
            .unwrap_or(true);
        if due {
            self.runtime_profiler_last_emitted = Some(Instant::now());
        }
        due
    }

    pub(in crate::workspace) fn advance_event_log_last_id(&mut self, next_last_id: u64) -> u64 {
        std::mem::replace(&mut self.event_log_last_id, next_last_id)
    }

    fn schedule_runtime_delivery(&self, cx: &mut Context<Self>) {
        let delivery_wake = self.runtime_delivery_tx.wake();
        let release_wake = delivery_wake.clone();
        cx.on_release(move |_, _| {
            // Runtime workers may finish independently, but this UI waiter is workspace-scoped.
            release_wake.stop();
        })
        .detach();
        cx.spawn(async move |entity, cx| {
            loop {
                delivery_wake.wait().await;
                let should_drain = delivery_wake.take();
                let stopped = delivery_wake.is_stopped();
                if should_drain {
                    let backlog_remaining = entity
                        .update(cx, |entity, cx| entity.drain_runtime_deliveries(cx))
                        .unwrap_or(false);
                    if backlog_remaining {
                        delivery_wake.mark();
                    }
                }
                if stopped {
                    break;
                }
            }
        })
        .detach();
    }

    fn drain_runtime_deliveries(&mut self, cx: &mut Context<Self>) -> bool {
        let drain = delivery::drain_channel(
            &self.runtime_delivery_rx,
            delivery::LIFECYCLE_DELIVERY_BUDGET,
        );
        if !drain.items.is_empty() {
            for delivery in drain.items {
                let intent = self.apply_runtime_delivery(delivery);
                self.runtime_intents.push_back(intent);
            }
            cx.emit(PluginWorkspaceEvent::RuntimeIntentsReady);
            cx.notify();
        }
        drain.outcome.backlog_remaining
    }

    fn apply_runtime_delivery(
        &mut self,
        delivery: NativePluginRuntimeDelivery,
    ) -> PluginRuntimeIntent {
        match delivery {
            NativePluginRuntimeDelivery::Activation { plugin_id, result } => {
                self.apply_activation_result(plugin_id, result)
            }
            NativePluginRuntimeDelivery::Deactivation { plugin_id, result } => {
                self.apply_deactivation_result(plugin_id, result)
            }
            NativePluginRuntimeDelivery::CommandDispatch { plugin_id, result } => {
                self.apply_command_dispatch_result(plugin_id, result)
            }
            NativePluginRuntimeDelivery::EventDispatch { plugin_id, result } => {
                self.apply_event_dispatch_result(plugin_id, result)
            }
        }
    }

    fn apply_activation_result(
        &mut self,
        plugin_id: String,
        result: Result<plugin_runtime::NativePluginRuntimeActivation, plugin_runtime::PluginError>,
    ) -> PluginRuntimeIntent {
        let activation = match result {
            Ok(activation) => activation,
            Err(error) => {
                let message = native_plugin_runtime_failure_message(error);
                let _ = self.registry_mut().mark_runtime_error(&plugin_id, message);
                return PluginRuntimeIntent::StateChanged;
            }
        };

        let plugin_runtime::NativePluginRuntimeActivation {
            plugin_id: activated_plugin_id,
            response,
            messages,
            effects,
        } = activation;
        if activated_plugin_id != plugin_id {
            self.start_runtime_deactivation(activated_plugin_id.clone());
            let _ = self.registry_mut().mark_runtime_error(
                &plugin_id,
                format!(
                    "Runtime activated plugin \"{}\" while loading \"{}\"",
                    activated_plugin_id, plugin_id
                ),
            );
            return PluginRuntimeIntent::StateChanged;
        }

        for message in &messages {
            if let Err(error) = self
                .registry_mut()
                .apply_runtime_outbound_message(&plugin_id, message)
            {
                self.registry_mut()
                    .cleanup_runtime_plugin_contributions(&plugin_id);
                let _ = self.registry_mut().mark_runtime_error(&plugin_id, error);
                self.start_runtime_deactivation(plugin_id);
                return PluginRuntimeIntent::StateChanged;
            }
        }

        match response.result {
            plugin_runtime::PluginResponseResult::Ok { .. } => {
                let _ = self.registry_mut().mark_runtime_active(&plugin_id);
                self.active_runtime_plugin_ids.insert(plugin_id.clone());
            }
            plugin_runtime::PluginResponseResult::Error { error } => {
                self.registry_mut()
                    .cleanup_runtime_plugin_contributions(&plugin_id);
                let message = native_plugin_runtime_failure_message(error);
                let _ = self.registry_mut().mark_runtime_error(&plugin_id, message);
                self.start_runtime_deactivation(plugin_id.clone());
            }
        }

        PluginRuntimeIntent::ApplyEffects {
            plugin_id,
            effects,
            refresh: PluginRuntimeAdapterRefresh::TerminalHooks,
        }
    }

    fn apply_deactivation_result(
        &mut self,
        plugin_id: String,
        result: Result<plugin_runtime::PluginResponse, plugin_runtime::PluginError>,
    ) -> PluginRuntimeIntent {
        self.active_runtime_plugin_ids.remove(&plugin_id);
        self.registry_mut()
            .cleanup_runtime_plugin_contributions(&plugin_id);
        match result {
            Ok(response) => {
                if let plugin_runtime::PluginResponseResult::Error { error } = response.result {
                    let message = native_plugin_runtime_failure_message(error);
                    self.registry_mut()
                        .record_manager_error(plugin_id.clone(), message);
                }
            }
            Err(error) => {
                let message = native_plugin_runtime_failure_message(error);
                self.registry_mut()
                    .record_manager_error(plugin_id.clone(), message);
            }
        }
        PluginRuntimeIntent::ApplyEffects {
            plugin_id,
            effects: Vec::new(),
            refresh: PluginRuntimeAdapterRefresh::All,
        }
    }

    fn apply_command_dispatch_result(
        &mut self,
        plugin_id: String,
        result: Result<
            plugin_runtime::NativePluginRuntimeCommandDispatch,
            plugin_runtime::PluginError,
        >,
    ) -> PluginRuntimeIntent {
        let dispatch = match result {
            Ok(dispatch) => dispatch,
            Err(error) => {
                let message = native_plugin_runtime_failure_message(error);
                self.registry_mut().record_manager_error(plugin_id, message);
                return PluginRuntimeIntent::StateChanged;
            }
        };

        let plugin_runtime::NativePluginRuntimeCommandDispatch {
            plugin_id: dispatched_plugin_id,
            command: _command,
            response,
            messages,
            effects,
        } = dispatch;
        if dispatched_plugin_id != plugin_id {
            self.registry_mut().record_manager_error(
                plugin_id,
                NATIVE_PLUGIN_RUNTIME_FAILURE_DIAGNOSTIC.to_string(),
            );
            return PluginRuntimeIntent::StateChanged;
        }

        for message in &messages {
            if let Err(error) = self
                .registry_mut()
                .apply_runtime_outbound_message(&dispatched_plugin_id, message)
            {
                self.registry_mut().record_manager_error(
                    dispatched_plugin_id.clone(),
                    format!("Native plugin command contribution update failed: {error}"),
                );
            }
        }
        if let plugin_runtime::PluginResponseResult::Error { error } = response.result {
            let message = native_plugin_runtime_failure_message(error);
            self.registry_mut()
                .record_manager_error(dispatched_plugin_id.clone(), message);
        }

        PluginRuntimeIntent::ApplyEffects {
            plugin_id: dispatched_plugin_id,
            effects,
            refresh: PluginRuntimeAdapterRefresh::TerminalHooks,
        }
    }

    fn apply_event_dispatch_result(
        &mut self,
        plugin_id: String,
        result: Result<
            plugin_runtime::NativePluginRuntimeEventDispatch,
            plugin_runtime::PluginError,
        >,
    ) -> PluginRuntimeIntent {
        let dispatch = match result {
            Ok(dispatch) => dispatch,
            Err(error) => {
                let message = native_plugin_runtime_failure_message(error);
                self.registry_mut().record_manager_error(plugin_id, message);
                return PluginRuntimeIntent::StateChanged;
            }
        };

        let plugin_runtime::NativePluginRuntimeEventDispatch {
            plugin_id: dispatched_plugin_id,
            event: _event,
            response,
            messages,
            effects,
        } = dispatch;
        if dispatched_plugin_id != plugin_id {
            self.registry_mut().record_manager_error(
                plugin_id,
                NATIVE_PLUGIN_RUNTIME_FAILURE_DIAGNOSTIC.to_string(),
            );
            return PluginRuntimeIntent::StateChanged;
        }

        for message in &messages {
            if let Err(error) = self
                .registry_mut()
                .apply_runtime_outbound_message(&dispatched_plugin_id, message)
            {
                self.registry_mut().record_manager_error(
                    dispatched_plugin_id.clone(),
                    format!("Native plugin event contribution update failed: {error}"),
                );
            }
        }
        if let plugin_runtime::PluginResponseResult::Error { error } = response.result {
            let message = native_plugin_runtime_failure_message(error);
            self.registry_mut()
                .record_manager_error(dispatched_plugin_id.clone(), message);
        }

        PluginRuntimeIntent::ApplyEffects {
            plugin_id: dispatched_plugin_id,
            effects,
            refresh: PluginRuntimeAdapterRefresh::TerminalInputInterceptors,
        }
    }

    fn schedule_manager_delivery(&self, cx: &mut Context<Self>) {
        let delivery_wake = self.manager_delivery_tx.wake();
        let release_wake = delivery_wake.clone();
        cx.on_release(move |_, _| {
            // The entity release path stops this waiter and aborts its package producer.
            release_wake.stop();
        })
        .detach();
        cx.spawn(async move |entity, cx| {
            loop {
                delivery_wake.wait().await;
                let should_drain = delivery_wake.take();
                let stopped = delivery_wake.is_stopped();
                if should_drain {
                    let backlog_remaining = entity
                        .update(cx, |entity, cx| entity.drain_manager_deliveries(cx))
                        .unwrap_or(false);
                    if backlog_remaining {
                        delivery_wake.mark();
                    }
                }
                if stopped {
                    break;
                }
            }
        })
        .detach();
    }

    fn schedule_runtime_request_delivery(&self, cx: &mut Context<Self>) {
        let request_wake = self.runtime_request_wake.clone();
        let release_wake = request_wake.clone();
        cx.on_release(move |_, _| {
            // The entity owns this waiter for the full workspace lifetime.
            release_wake.stop();
        })
        .detach();
        cx.spawn(async move |_entity, cx| {
            loop {
                request_wake.wait().await;
                let should_deliver = request_wake.take();
                let stopped = request_wake.is_stopped();
                if should_deliver {
                    let _ = _entity.update(cx, |_entity, cx| {
                        cx.emit(PluginWorkspaceEvent::RuntimeRequestsReady);
                    });
                }
                if stopped {
                    break;
                }
            }
        })
        .detach();
    }

    fn schedule_oxide_import_delivery(&self, cx: &mut Context<Self>) {
        let delivery_wake = self.oxide_import_delivery_tx.wake();
        let release_wake = delivery_wake.clone();
        cx.on_release(move |_, _| {
            // Workspace release stops only foreground delivery; an in-flight
            // import still owns and erases its password inside the worker.
            release_wake.stop();
        })
        .detach();
        cx.spawn(async move |entity, cx| {
            loop {
                delivery_wake.wait().await;
                let should_drain = delivery_wake.take();
                let stopped = delivery_wake.is_stopped();
                if should_drain {
                    let backlog_remaining = entity
                        .update(cx, |entity, cx| entity.drain_oxide_import_deliveries(cx))
                        .unwrap_or(false);
                    if backlog_remaining {
                        delivery_wake.mark();
                    }
                }
                if stopped {
                    break;
                }
            }
        })
        .detach();
    }

    fn drain_oxide_import_deliveries(&mut self, cx: &mut Context<Self>) -> bool {
        let drain = delivery::drain_channel(
            &self.oxide_import_delivery_rx,
            delivery::USER_ACTION_DELIVERY_BUDGET,
        );
        let mut added_intent = false;
        for delivery in drain.items {
            match delivery {
                NativePluginOxideImportWorkerMessage::Progress {
                    operation_id,
                    stage,
                    current,
                    total,
                } => {
                    let Some(context) = self.oxide_import_contexts.get(&operation_id) else {
                        continue;
                    };
                    let Some(registration_id) = context.progress_registration_id.as_ref() else {
                        continue;
                    };
                    self.oxide_import_intents
                        .push_back(PluginOxideImportIntent::Progress {
                            plugin_id: Arc::clone(&context.plugin_id),
                            registration_id: Arc::clone(registration_id),
                            value:
                                oxideterm_plugin_host_api::sync::native_plugin_sync_progress_value(
                                    "Importing .oxide",
                                    &stage,
                                    current,
                                    total,
                                    false,
                                ),
                        });
                    added_intent = true;
                }
                NativePluginOxideImportWorkerMessage::Done {
                    operation_id,
                    result,
                } => {
                    let Some(context) = self.oxide_import_contexts.remove(&operation_id) else {
                        continue;
                    };
                    self.oxide_import_intents
                        .push_back(PluginOxideImportIntent::Complete {
                            plugin_id: context.plugin_id,
                            progress_registration_id: context.progress_registration_id,
                            request_id: context.request_id,
                            result,
                            options: context.options,
                            response_tx: context.response_tx,
                        });
                    added_intent = true;
                }
            }
        }
        if added_intent {
            cx.emit(PluginWorkspaceEvent::OxideImportIntentsReady);
            cx.notify();
        }
        drain.outcome.backlog_remaining
    }

    fn drain_manager_deliveries(&mut self, cx: &mut Context<Self>) -> bool {
        let drain = delivery::drain_channel(
            &self.manager_delivery_rx,
            delivery::USER_ACTION_DELIVERY_BUDGET,
        );
        if !drain.items.is_empty() {
            self.manager_operation_in_flight = false;
            self.manager_deliveries.extend(drain.items);
            cx.emit(PluginWorkspaceEvent::ManagerDeliveryReady);
            cx.notify();
        }
        drain.outcome.backlog_remaining
    }
}

impl gpui::EventEmitter<PluginWorkspaceEvent> for PluginWorkspaceEntity {}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    #[gpui::test]
    fn manager_operation_and_delivery_are_entity_owned(cx: &mut TestAppContext) {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("plugin entity test runtime"),
        );
        let entity = cx.new(|cx| {
            PluginWorkspaceEntity::new(runtime, plugin_host::NativePluginRegistry::default(), cx)
        });
        let delivery_tx = entity.update(cx, |entity, _cx| {
            entity.manager_operation_in_flight = true;
            entity.manager_delivery_tx.clone()
        });
        delivery_tx
            .send(plugin_manager::NativePluginManagerDelivery::CheckUpdates(
                Some(Vec::new()),
            ))
            .expect("manager delivery");

        cx.run_until_parked();

        entity.update(cx, |entity, _cx| {
            assert!(!entity.manager_operation_in_flight());
            let i18n = I18n::new(Locale::En);
            assert!(!entity.apply_manager_deliveries(std::path::Path::new(""), &i18n));
            assert!(entity.manager_deliveries.is_empty());
            assert!(matches!(
                entity.manager_state().operation_status,
                plugin_manager::NativePluginManagerOperationStatus::Success(_)
            ));
        });
    }

    #[gpui::test]
    fn declarative_ui_control_and_surface_cleanup_are_entity_owned(cx: &mut TestAppContext) {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("plugin entity test runtime"),
        );
        let entity = cx.new(|cx| {
            PluginWorkspaceEntity::new(runtime, plugin_host::NativePluginRegistry::default(), cx)
        });
        let control: plugin_host::NativePluginDeclarativeUiControl =
            serde_json::from_value(serde_json::json!({
                "kind": "password",
                "id": "credential",
                "value": "entity-owned-secret"
            }))
            .expect("password control");

        entity.update(cx, |entity, _cx| {
            let ui = entity.ui_state_mut();
            let render_generation = ui.begin_surface_render();
            let key = ui.sync_control(
                plugin_ui::NativePluginUiControlContext {
                    plugin_id: "plugin.test".to_string(),
                    surface_kind: "tab".to_string(),
                    surface_id: "settings".to_string(),
                    section_id: "root".to_string(),
                    control_id: "credential".to_string(),
                    control_kind: "password".to_string(),
                },
                &control,
                render_generation,
            );
            ui.focused_input = Some(key);
            ui.open_select = Some(key);
            assert_eq!(ui.text(key), Some("entity-owned-secret"));

            let empty_generation = ui.begin_surface_render();
            ui.finish_surface_render("plugin.test", "tab", "settings", empty_generation);
            assert!(ui.context(key).is_none());
            assert!(ui.focused_input.is_none());
            assert!(ui.open_select.is_none());
        });

        entity.update(cx, |entity, _cx| {
            let render_generation = entity.ui_state_mut().begin_surface_render();
            let key = entity.ui_state_mut().sync_control(
                plugin_ui::NativePluginUiControlContext {
                    plugin_id: "plugin.test".to_string(),
                    surface_kind: "sidebarPanel".to_string(),
                    surface_id: "settings".to_string(),
                    section_id: "root".to_string(),
                    control_id: "credential".to_string(),
                    control_kind: "password".to_string(),
                },
                &control,
                render_generation,
            );
            entity.manager_state_mut().active_sidebar_panel =
                Some(plugin_ui::NativePluginSidebarPanelSelection {
                    plugin_id: "plugin.test".to_string(),
                    panel_id: "settings".to_string(),
                });
            entity.select_sidebar_panel(plugin_ui::NativePluginSidebarPanelSelection {
                plugin_id: "plugin.other".to_string(),
                panel_id: "overview".to_string(),
            });
            assert!(entity.ui_state().context(key).is_none());
        });
    }

    #[gpui::test]
    fn runtime_requests_and_confirm_lifecycle_are_entity_owned(cx: &mut TestAppContext) {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("plugin entity test runtime"),
        );
        let entity = cx.new(|cx| {
            PluginWorkspaceEntity::new(runtime, plugin_host::NativePluginRegistry::default(), cx)
        });
        let senders = entity.read_with(cx, |entity, _cx| entity.runtime_request_senders());
        let (first_confirm_tx, first_confirm_rx) = std::sync::mpsc::channel();
        let (second_confirm_tx, _second_confirm_rx) = std::sync::mpsc::channel();
        let (terminal_response_tx, _terminal_response_rx) = std::sync::mpsc::channel();
        let (sync_response_tx, _sync_response_rx) = std::sync::mpsc::channel();

        senders
            .confirm
            .send(NativePluginConfirmRequest {
                plugin_id: "plugin.test".to_string(),
                request_id: "confirm-first".to_string(),
                title: "First".to_string(),
                description: "First request".to_string(),
                response_tx: first_confirm_tx,
            })
            .expect("first confirm request");
        senders
            .confirm
            .send(NativePluginConfirmRequest {
                plugin_id: "plugin.test".to_string(),
                request_id: "confirm-second".to_string(),
                title: "Second".to_string(),
                description: "Second request".to_string(),
                response_tx: second_confirm_tx,
            })
            .expect("second confirm request");
        senders
            .terminal
            .send(NativePluginTerminalRequest {
                request_id: "terminal-clear".to_string(),
                action: NativePluginTerminalAction::ClearBuffer {
                    node_id: "node-test".to_string(),
                },
                response_tx: terminal_response_tx,
            })
            .expect("terminal request");
        senders
            .sync
            .send(NativePluginSyncRequest {
                request_id: "sync-progress".to_string(),
                action: NativePluginSyncAction::ReportProgress {
                    plugin_id: "plugin.test".to_string(),
                    registration_id: "progress-test".to_string(),
                    value: serde_json::json!({"current": 1}),
                },
                response_tx: sync_response_tx,
            })
            .expect("sync request");
        entity.update(cx, |entity, _cx| {
            entity.enqueue_product_ui_effect(NativePluginProductUiEffect {
                plugin_id: "plugin.test".to_string(),
                namespace: "connections".to_string(),
                method: "connect".to_string(),
                args: serde_json::json!({"connectionId": "connection-test"}),
            });
        });

        cx.run_until_parked();

        entity.update(cx, |entity, _cx| {
            assert!(entity.promote_confirm_request());
            assert!(entity.confirm_dialog().is_some());
            let generation = entity
                .begin_confirm_exit(true)
                .expect("visible confirm generation");
            assert!(entity.finish_confirm_exit(generation));
            assert!(entity.confirm_dialog().is_some());

            let terminal_requests = entity.take_terminal_requests();
            assert_eq!(terminal_requests.items.len(), 1);
            assert!(!terminal_requests.outcome.backlog_remaining);

            let sync_requests = entity.take_sync_requests();
            assert_eq!(sync_requests.items.len(), 1);
            assert!(!sync_requests.outcome.backlog_remaining);

            let (product_effects, product_backlog) = entity.take_product_ui_effects();
            assert_eq!(product_effects.len(), 1);
            assert!(!product_backlog);
        });
        assert_eq!(first_confirm_rx.try_recv(), Ok(true));
    }

    #[gpui::test]
    fn runtime_delivery_is_entity_owned_and_redacts_plugin_error_text(cx: &mut TestAppContext) {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("plugin entity test runtime"),
        );
        let entity = cx.new(|cx| {
            PluginWorkspaceEntity::new(runtime, plugin_host::NativePluginRegistry::default(), cx)
        });
        let delivery_tx = entity.read_with(cx, |entity, _cx| entity.runtime_delivery_tx.clone());
        let sensitive_marker = "token=must-not-reach-diagnostics";
        delivery_tx
            .send(NativePluginRuntimeDelivery::CommandDispatch {
                plugin_id: "plugin.test".to_string(),
                result: Err(plugin_runtime::PluginError::runtime(
                    "command_failed",
                    sensitive_marker,
                )),
            })
            .expect("runtime delivery");

        cx.run_until_parked();

        entity.update(cx, |entity, _cx| {
            assert!(matches!(
                entity.take_runtime_intents().pop_front(),
                Some(PluginRuntimeIntent::StateChanged)
            ));
            let diagnostic = entity
                .registry()
                .diagnostics()
                .last()
                .expect("sanitized runtime diagnostic");
            assert_eq!(diagnostic.message, NATIVE_PLUGIN_RUNTIME_FAILURE_DIAGNOSTIC);
            assert!(!diagnostic.message.contains(sensitive_marker));
        });
    }

    #[gpui::test]
    fn deactivation_releases_entity_runtime_ownership_and_refreshes_adapters(
        cx: &mut TestAppContext,
    ) {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("plugin entity test runtime"),
        );
        let entity = cx.new(|cx| {
            PluginWorkspaceEntity::new(runtime, plugin_host::NativePluginRegistry::default(), cx)
        });

        entity.update(cx, |entity, _cx| {
            let plugin_id = "plugin.test".to_string();
            entity.active_runtime_plugin_ids.insert(plugin_id.clone());
            let intent = entity.apply_deactivation_result(
                plugin_id.clone(),
                Err(plugin_runtime::PluginError::runtime(
                    "deactivate_failed",
                    "credential-like runtime text",
                )),
            );

            assert!(!entity.active_runtime_plugin_ids.contains(&plugin_id));
            assert!(matches!(
                intent,
                PluginRuntimeIntent::ApplyEffects {
                    refresh: PluginRuntimeAdapterRefresh::All,
                    ..
                }
            ));
            assert_eq!(
                entity
                    .registry()
                    .diagnostics()
                    .last()
                    .expect("deactivation diagnostic")
                    .message,
                NATIVE_PLUGIN_RUNTIME_FAILURE_DIAGNOSTIC
            );
        });
    }

    #[gpui::test]
    fn oxide_import_progress_and_completion_are_entity_owned(cx: &mut TestAppContext) {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("plugin entity test runtime"),
        );
        let entity = cx.new(|cx| {
            PluginWorkspaceEntity::new(runtime, plugin_host::NativePluginRegistry::default(), cx)
        });
        let (response_tx, _response_rx) = std::sync::mpsc::channel();
        let delivery_tx = entity.update(cx, |entity, _cx| {
            entity.oxide_import_contexts.insert(
                7,
                PluginOxideImportContext {
                    plugin_id: Arc::from("plugin.test"),
                    progress_registration_id: Some(Arc::from("import-progress")),
                    request_id: "import-request".to_string(),
                    options: NativePluginOxidePostImportOptions {
                        import_app_settings: false,
                        selected_app_settings_sections: None,
                        import_plugin_settings: false,
                        selected_plugin_ids: None,
                        import_quick_commands: false,
                        quick_command_strategy: oxideterm_plugin_host_api::sync::
                            NativePluginQuickCommandImportStrategy::Rename,
                    },
                    response_tx,
                },
            );
            entity.oxide_import_delivery_tx.clone()
        });
        delivery_tx
            .send(NativePluginOxideImportWorkerMessage::Progress {
                operation_id: 7,
                stage: "connections".to_string(),
                current: 1,
                total: 2,
            })
            .expect("oxide progress delivery");
        delivery_tx
            .send(NativePluginOxideImportWorkerMessage::Done {
                operation_id: 7,
                result: Err(()),
            })
            .expect("oxide completion delivery");

        cx.run_until_parked();

        entity.update(cx, |entity, _cx| {
            let mut intents = entity.take_oxide_import_intents();
            assert!(matches!(
                intents.pop_front(),
                Some(PluginOxideImportIntent::Progress {
                    plugin_id,
                    registration_id,
                    ..
                }) if plugin_id.as_ref() == "plugin.test"
                    && registration_id.as_ref() == "import-progress"
            ));
            assert!(matches!(
                intents.pop_front(),
                Some(PluginOxideImportIntent::Complete {
                    request_id,
                    result: Err(()),
                    ..
                }) if request_id == "import-request"
            ));
            assert!(entity.oxide_import_contexts.is_empty());
        });
    }

    #[gpui::test]
    fn hidden_plugin_manager_keeps_runtime_and_reliable_deliveries_alive(cx: &mut TestAppContext) {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("plugin entity test runtime"),
        );
        let workspace_owner = cx.new(|cx| {
            PluginWorkspaceEntity::new(runtime, plugin_host::NativePluginRegistry::default(), cx)
        });
        // The page mount may disappear while the workspace-owned Entity remains
        // alive. This models the manager surface transitioning visible->hidden.
        let visible_manager_mount = workspace_owner.clone();
        let (confirm_response_tx, confirm_response_rx) = std::sync::mpsc::channel();
        let (import_response_tx, _import_response_rx) = std::sync::mpsc::channel();
        let (runtime_delivery_tx, request_senders, import_delivery_tx) =
            workspace_owner.update(cx, |entity, cx| {
                assert!(entity.start_runtime_services());
                entity.configure_subscription_samples(
                    vec![(
                        PluginSubscriptionSample::Profiler,
                        serde_json::json!({ "nodes": [] }),
                    )],
                    None,
                    cx,
                );
                entity.oxide_import_contexts.insert(
                    11,
                    PluginOxideImportContext {
                        plugin_id: Arc::from("plugin.test"),
                        progress_registration_id: None,
                        request_id: "import-hidden".to_string(),
                        options: NativePluginOxidePostImportOptions {
                            import_app_settings: false,
                            selected_app_settings_sections: None,
                            import_plugin_settings: false,
                            selected_plugin_ids: None,
                            import_quick_commands: false,
                            quick_command_strategy: oxideterm_plugin_host_api::sync::
                                NativePluginQuickCommandImportStrategy::Rename,
                        },
                        response_tx: import_response_tx,
                    },
                );
                (
                    entity.runtime_delivery_tx.clone(),
                    entity.runtime_request_senders(),
                    entity.oxide_import_delivery_tx.clone(),
                )
            });

        drop(visible_manager_mount);

        runtime_delivery_tx
            .send(NativePluginRuntimeDelivery::CommandDispatch {
                plugin_id: "plugin.test".to_string(),
                result: Ok(plugin_runtime::NativePluginRuntimeCommandDispatch {
                    plugin_id: "plugin.test".to_string(),
                    command: "demo.command".to_string(),
                    response: plugin_runtime::PluginResponse::ok(
                        "command-hidden",
                        serde_json::Value::Null,
                    ),
                    messages: Vec::new(),
                    effects: Vec::new(),
                }),
            })
            .expect("hidden command delivery");
        runtime_delivery_tx
            .send(NativePluginRuntimeDelivery::EventDispatch {
                plugin_id: "plugin.test".to_string(),
                result: Ok(plugin_runtime::NativePluginRuntimeEventDispatch {
                    plugin_id: "plugin.test".to_string(),
                    event: plugin_runtime::PluginEvent {
                        name: "demo.event".to_string(),
                        payload: serde_json::Value::Null,
                    },
                    response: plugin_runtime::PluginResponse::ok(
                        "event-hidden",
                        serde_json::Value::Null,
                    ),
                    messages: Vec::new(),
                    effects: Vec::new(),
                }),
            })
            .expect("hidden event delivery");
        request_senders
            .confirm
            .send(NativePluginConfirmRequest {
                plugin_id: "plugin.test".to_string(),
                request_id: "confirm-hidden".to_string(),
                title: "Confirm".to_string(),
                description: "Continue hidden request".to_string(),
                response_tx: confirm_response_tx,
            })
            .expect("hidden confirm delivery");
        import_delivery_tx
            .send(NativePluginOxideImportWorkerMessage::Done {
                operation_id: 11,
                result: Err(()),
            })
            .expect("hidden import completion");

        cx.run_until_parked();

        workspace_owner.update(cx, |entity, cx| {
            let mut runtime_intents = entity.take_runtime_intents();
            assert!(matches!(
                runtime_intents.pop_front(),
                Some(PluginRuntimeIntent::ApplyEffects {
                    refresh: PluginRuntimeAdapterRefresh::TerminalHooks,
                    ..
                })
            ));
            assert!(matches!(
                runtime_intents.pop_front(),
                Some(PluginRuntimeIntent::ApplyEffects {
                    refresh: PluginRuntimeAdapterRefresh::TerminalInputInterceptors,
                    ..
                })
            ));
            assert!(runtime_intents.is_empty());

            assert!(entity.promote_confirm_request());
            let confirm_generation = entity
                .begin_confirm_exit(true)
                .expect("hidden confirm generation");
            assert!(!entity.finish_confirm_exit(confirm_generation));

            assert!(matches!(
                entity.take_oxide_import_intents().pop_front(),
                Some(PluginOxideImportIntent::Complete { request_id, .. })
                    if request_id == "import-hidden"
            ));

            // Profiler sampling here belongs to a plugin runtime subscription,
            // not to the hidden manager page, so it must remain active.
            assert!(entity.subscription_sampler_running);
            assert_eq!(
                entity.subscription_samples(),
                vec![PluginSubscriptionSample::Profiler]
            );
            assert!(entity.runtime_profiler_metrics_due(Duration::from_secs(1)));
            entity.configure_subscription_samples(Vec::new(), None, cx);
        });
        assert_eq!(confirm_response_rx.try_recv(), Ok(true));
    }

    fn assert_workspace_released_response(response: plugin_runtime::PluginResponse) {
        assert!(matches!(
            response.result,
            plugin_runtime::PluginResponseResult::Error { error }
                if error.code == NATIVE_PLUGIN_WORKSPACE_RELEASED_CODE
        ));
    }

    #[gpui::test]
    fn release_shutdown_is_idempotent_and_aborts_runtime_producers(cx: &mut TestAppContext) {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("plugin entity test runtime"),
        );
        let entity = cx.new(|cx| {
            PluginWorkspaceEntity::new(runtime, plugin_host::NativePluginRegistry::default(), cx)
        });
        let shutdown_invocations = Arc::new(AtomicUsize::new(0));
        let shutdown_targets = Arc::new(AtomicUsize::new(0));
        entity.update(cx, |entity, cx| {
            entity.release_shutdown_invocations = Some(Arc::clone(&shutdown_invocations));
            entity.release_shutdown_targets = Some(Arc::clone(&shutdown_targets));
            entity
                .active_runtime_plugin_ids
                .insert("plugin.release".to_string());
            entity.manager_operation_in_flight = true;
            entity.configure_subscription_samples(
                vec![(
                    PluginSubscriptionSample::Layout,
                    serde_json::json!({"activeTabId": "tab-release"}),
                )],
                None,
                cx,
            );
            assert!(entity.subscription_sampler_running);
            assert!(entity.subscription_sampler_task.is_some());
            entity.spawn_owned_task(std::future::pending::<()>());
            assert_eq!(
                entity
                    .owned_tasks
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .len(),
                1
            );
            assert!(entity.begin_release_shutdown());
            assert!(!entity.begin_release_shutdown());
            assert!(
                entity
                    .owned_tasks
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .is_empty()
            );
            assert!(!entity.subscription_sampler_running);
            assert!(entity.subscription_sampler_task.is_none());
            assert!(entity.subscription_samples.is_empty());
            assert!(!entity.manager_operation_in_flight);
            assert!(!entity.start_runtime_services());
            entity.configure_subscription_samples(
                vec![(PluginSubscriptionSample::Layout, serde_json::json!({}))],
                None,
                cx,
            );
            assert!(!entity.subscription_sampler_running);
        });

        assert_eq!(shutdown_invocations.load(Ordering::SeqCst), 1);
        assert_eq!(shutdown_targets.load(Ordering::SeqCst), 1);
    }

    #[gpui::test]
    fn last_entity_owner_release_rejects_pending_requests_and_stops_waiters(
        cx: &mut TestAppContext,
    ) {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("plugin entity test runtime"),
        );
        let entity = cx.new(|cx| {
            PluginWorkspaceEntity::new(runtime, plugin_host::NativePluginRegistry::default(), cx)
        });
        let shutdown_invocations = Arc::new(AtomicUsize::new(0));
        let shutdown_targets = Arc::new(AtomicUsize::new(0));
        let senders = entity.read_with(cx, |entity, _cx| entity.runtime_request_senders());
        let (active_confirm_tx, active_confirm_rx) = std::sync::mpsc::channel();
        let (queued_confirm_tx, queued_confirm_rx) = std::sync::mpsc::channel();
        let (terminal_response_tx, terminal_response_rx) = std::sync::mpsc::channel();
        let (sync_response_tx, sync_response_rx) = std::sync::mpsc::channel();
        senders
            .confirm
            .send(NativePluginConfirmRequest {
                plugin_id: "plugin.release".to_string(),
                request_id: "confirm-active".to_string(),
                title: "Active".to_string(),
                description: "Already answered before release".to_string(),
                response_tx: active_confirm_tx,
            })
            .expect("active confirm request");
        senders
            .confirm
            .send(NativePluginConfirmRequest {
                plugin_id: "plugin.release".to_string(),
                request_id: "confirm-queued".to_string(),
                title: "Queued".to_string(),
                description: "Rejected on release".to_string(),
                response_tx: queued_confirm_tx,
            })
            .expect("queued confirm request");
        senders
            .terminal
            .send(NativePluginTerminalRequest {
                request_id: "terminal-release".to_string(),
                action: NativePluginTerminalAction::ClearBuffer {
                    node_id: "node-release".to_string(),
                },
                response_tx: terminal_response_tx,
            })
            .expect("terminal request");
        senders
            .sync
            .send(NativePluginSyncRequest {
                request_id: "sync-release".to_string(),
                action: NativePluginSyncAction::ReportProgress {
                    plugin_id: "plugin.release".to_string(),
                    registration_id: "progress-release".to_string(),
                    value: serde_json::json!({"current": 1}),
                },
                response_tx: sync_response_tx,
            })
            .expect("sync request");

        entity.update(cx, |entity, _cx| {
            entity.release_shutdown_invocations = Some(Arc::clone(&shutdown_invocations));
            entity.release_shutdown_targets = Some(Arc::clone(&shutdown_targets));
            entity
                .active_runtime_plugin_ids
                .insert("plugin.release".to_string());
            assert!(entity.promote_confirm_request());
            assert!(entity.begin_confirm_exit(true).is_some());
        });
        let (manager_wake, runtime_delivery_wake, runtime_request_wake, oxide_import_wake) = cx
            .read(|cx| {
                let entity = entity.read(cx);
                (
                    entity.manager_delivery_tx.wake(),
                    entity.runtime_delivery_tx.wake(),
                    entity.runtime_request_wake.clone(),
                    entity.oxide_import_delivery_tx.wake(),
                )
            });

        let retained_for_detached_window = entity.clone();
        drop(entity);
        cx.update(|_cx| {});
        cx.run_until_parked();

        // Closing one window cannot stop plugins while a detached owner retains the Entity.
        assert_eq!(shutdown_invocations.load(Ordering::SeqCst), 0);
        assert!(!manager_wake.is_stopped());
        assert_eq!(active_confirm_rx.try_recv(), Ok(true));
        assert!(queued_confirm_rx.try_recv().is_err());
        assert!(terminal_response_rx.try_recv().is_err());
        assert!(sync_response_rx.try_recv().is_err());

        drop(retained_for_detached_window);
        cx.update(|_cx| {});
        cx.run_until_parked();

        // Releasing the final shared owner ends delivery and rejects every pending action.
        assert!(manager_wake.is_stopped());
        assert!(runtime_delivery_wake.is_stopped());
        assert!(runtime_request_wake.is_stopped());
        assert!(oxide_import_wake.is_stopped());
        assert_eq!(shutdown_invocations.load(Ordering::SeqCst), 1);
        assert_eq!(shutdown_targets.load(Ordering::SeqCst), 1);
        assert_eq!(queued_confirm_rx.try_recv(), Ok(false));
        assert_workspace_released_response(
            terminal_response_rx
                .try_recv()
                .expect("terminal release response"),
        );
        assert_workspace_released_response(
            sync_response_rx.try_recv().expect("sync release response"),
        );
        assert!(matches!(
            active_confirm_rx.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Disconnected)
        ));
    }

    #[gpui::test]
    fn subscription_sampling_state_and_lifecycle_are_entity_owned(cx: &mut TestAppContext) {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("plugin entity test runtime"),
        );
        let entity = cx.new(|cx| {
            PluginWorkspaceEntity::new(runtime, plugin_host::NativePluginRegistry::default(), cx)
        });
        let initial_layout = serde_json::json!({"tabCount": 1});
        entity.update(cx, |entity, cx| {
            assert!(entity.start_runtime_services());
            assert!(!entity.start_runtime_services());
            entity.configure_subscription_samples(
                vec![(PluginSubscriptionSample::Layout, initial_layout.clone())],
                Some(7),
                cx,
            );
            assert!(entity.subscription_sampler_running);
            assert_eq!(
                entity.subscription_samples(),
                vec![PluginSubscriptionSample::Layout]
            );

            let (previous, unchanged) = entity.update_subscription_snapshot(
                PluginSubscriptionSample::Layout,
                initial_layout.clone(),
            );
            assert!(previous.is_none());
            assert_eq!(unchanged, initial_layout);

            let next_layout = serde_json::json!({"tabCount": 2});
            let (previous, current) = entity.update_subscription_snapshot(
                PluginSubscriptionSample::Layout,
                next_layout.clone(),
            );
            assert_eq!(previous, Some(serde_json::json!({"tabCount": 1})));
            assert_eq!(current, next_layout);

            assert!(entity.transfer_progress_due(Duration::from_secs(1)));
            assert!(!entity.transfer_progress_due(Duration::from_secs(1)));
            assert!(entity.runtime_profiler_metrics_due(Duration::from_secs(1)));
            assert!(!entity.runtime_profiler_metrics_due(Duration::from_secs(1)));
            assert_eq!(entity.advance_event_log_last_id(9), 7);

            entity.configure_subscription_samples(Vec::new(), None, cx);
            assert!(!entity.subscription_sampler_running);
            assert!(entity.subscription_samples().is_empty());
        });
    }
}
