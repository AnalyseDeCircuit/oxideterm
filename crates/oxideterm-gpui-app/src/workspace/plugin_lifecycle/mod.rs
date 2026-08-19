// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use gpui::{
    AnyElement, AnyWindowHandle, App, AppContext, Context, IntoElement, KeyDownEvent,
    ParentElement, Timer, Window, div,
};
use oxideterm_connections::{SavedConnectionsConflictStrategy, SavedConnectionsSyncSnapshot};
use oxideterm_gpui_terminal::{TerminalNotice, TerminalNoticeVariant};
use oxideterm_gpui_ui::{ConfirmDialogVariant, ConfirmDialogView};
use oxideterm_sftp::BackgroundTransferState;
use serde_json::{Value, json};

use super::{
    TabKind, TelnetSessionConfig, TerminalInputInterceptor, TerminalOutputProcessor,
    TerminalSessionId, WorkspaceApp, WorkspaceOverlayIntent, plugin_entity, plugin_host,
    plugin_runtime,
};

mod host_api_snapshot;
mod ide;
mod product_host_calls;
mod profiler;
mod secrets;
mod settings_payload;
mod snapshots;
mod sync;
mod terminal_hooks;
mod terminal_queries;
mod types;
mod ui_helpers;
mod ui_host_calls;

use host_api_snapshot::*;
use ide::*;
use profiler::*;
use secrets::*;
use settings_payload::*;
use snapshots::*;
use sync::*;
use terminal_hooks::*;
use terminal_queries::*;
pub(super) use types::{
    NativePluginConfirmDialog, NativePluginConfirmRequest, NativePluginOxideImportCoreResult,
    NativePluginOxideImportWorkerMessage, NativePluginOxidePostImportOptions,
    NativePluginProductUiEffect, NativePluginRuntimeDelivery, NativePluginSyncAction,
    NativePluginSyncRequest, NativePluginTerminalAction, NativePluginTerminalRequest,
};
pub(super) use ui_helpers::native_plugin_theme_snapshot;
use ui_helpers::*;
use ui_host_calls::*;

#[cfg(test)]
use super::delivery;
#[cfg(test)]
use oxideterm_plugin_host_api::terminal::NativePluginTerminalNodeSnapshot;
use oxideterm_plugin_host_api::{
    ai::*,
    backend::*,
    catalog::{allowed_host_apis_for_capabilities, is_supported_host_api_capability},
    forwarding::{native_plugin_forward_response, native_plugin_forward_saved_forwards},
    host_tools::*,
    scp::native_plugin_scp_response,
    sftp::native_plugin_sftp_response,
    transfers::*,
};
#[cfg(test)]
use oxideterm_plugin_host_api::{
    forwarding::{native_plugin_forward_check_capability, native_plugin_forward_create_request},
    sftp::{
        native_plugin_sftp_check_capability, native_plugin_sftp_node_id_arg,
        native_plugin_sftp_path_arg,
    },
};

// Runtime subscription sampling is independent from Plugin Manager visibility.
const NATIVE_PLUGIN_TRANSFER_PROGRESS_INTERVAL: Duration = Duration::from_millis(500);
const NATIVE_PLUGIN_PROFILER_METRICS_INTERVAL: Duration = Duration::from_secs(1);
const NATIVE_PLUGIN_TOAST_TTL: Duration = Duration::from_secs(4);

impl WorkspaceApp {
    fn promote_native_plugin_confirm(&mut self, cx: &mut Context<Self>) {
        let promoted = self
            .plugin_entity
            .update(cx, |plugins, _cx| plugins.promote_confirm_request());
        if promoted {
            // The root retains only the shared overlay focus adapter.
            self.reset_standard_confirm_focus();
            cx.notify();
        }
    }

    fn respond_native_plugin_confirm(&mut self, confirmed: bool, cx: &mut Context<Self>) {
        let Some(generation) = self
            .plugin_entity
            .update(cx, |plugins, _cx| plugins.begin_confirm_exit(confirmed))
        else {
            return;
        };
        self.clear_standard_confirm_focus();
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        if delay.is_zero() {
            let promoted = self
                .plugin_entity
                .update(cx, |plugins, _cx| plugins.finish_confirm_exit(generation));
            if promoted {
                self.reset_standard_confirm_focus();
            }
            cx.notify();
            return;
        }
        cx.spawn(async move |weak, cx| {
            Timer::after(delay).await;
            let _ = weak.update(cx, |this, cx| {
                let promoted = this
                    .plugin_entity
                    .update(cx, |plugins, _cx| plugins.finish_confirm_exit(generation));
                if promoted {
                    this.reset_standard_confirm_focus();
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn handle_native_plugin_terminal_request(
        &mut self,
        request: NativePluginTerminalRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let response = match request.action {
            NativePluginTerminalAction::WriteActive { text } => {
                let ok = self.write_native_plugin_active_terminal_text(&text, cx);
                plugin_runtime::PluginResponse::ok(request.request_id, json!(ok))
            }
            NativePluginTerminalAction::WriteNode { node_id, text } => {
                let ok = self.write_native_plugin_node_terminal_text(&node_id, &text, cx);
                plugin_runtime::PluginResponse::ok(request.request_id, json!(ok))
            }
            NativePluginTerminalAction::ClearBuffer { node_id } => {
                self.clear_native_plugin_node_terminal_buffer(&node_id, cx);
                plugin_runtime::PluginResponse::ok(request.request_id, Value::Null)
            }
            NativePluginTerminalAction::OpenTelnet { host, port } => {
                self.open_native_plugin_telnet_terminal(&request.request_id, host, port, window, cx)
            }
        };
        let _ = request.response_tx.send(response);
    }

    pub(super) fn schedule_native_plugin_runtime_request_apply(
        &mut self,
        window_handle: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |weak, cx| {
            let _ = cx.update_window(window_handle, |_, window, cx| {
                weak.update(cx, |workspace, cx| {
                    workspace.apply_native_plugin_runtime_requests(window, cx);
                })
            });
        })
        .detach();
    }

    fn apply_native_plugin_runtime_requests(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.promote_native_plugin_confirm(cx);
        let terminal_batch = self
            .plugin_entity
            .update(cx, |plugins, _cx| plugins.take_terminal_requests());
        let terminal_backlog = terminal_batch.outcome.backlog_remaining;
        for request in terminal_batch.items {
            self.handle_native_plugin_terminal_request(request, window, cx);
        }

        let sync_batch = self
            .plugin_entity
            .update(cx, |plugins, _cx| plugins.take_sync_requests());
        let sync_backlog = sync_batch.outcome.backlog_remaining;
        for request in sync_batch.items {
            self.handle_native_plugin_sync_request(request, cx);
        }

        let product_backlog = self.apply_native_plugin_product_ui_effects(window, cx);
        if terminal_backlog || sync_backlog || product_backlog {
            // Continue through the Entity wake so one budgeted UI turn cannot
            // monopolize the application thread.
            self.plugin_entity.read(cx).mark_runtime_requests_ready();
        }
    }

    fn open_native_plugin_telnet_terminal(
        &mut self,
        request_id: &str,
        host: String,
        port: u16,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> plugin_runtime::PluginResponse {
        let config = TelnetSessionConfig {
            host: host.clone(),
            port,
        };
        match self.create_telnet_terminal_tab(config, Default::default(), window, cx) {
            Ok(session_id) => {
                let label = format!("Telnet {host}:{port}");
                plugin_runtime::PluginResponse::ok(
                    request_id.to_string(),
                    json!({
                        "sessionId": session_id.0.to_string(),
                        "info": {
                            "id": session_id.0.to_string(),
                            "running": true,
                            "detached": false,
                            "shell": {
                                "id": "telnet",
                                "label": label,
                                "path": "telnet",
                                "args": []
                            },
                            "transport": {
                                "type": "telnet",
                                "host": host,
                                "port": port
                            }
                        }
                    }),
                )
            }
            Err(error) => plugin_runtime::PluginResponse::error(
                request_id.to_string(),
                plugin_runtime::PluginError::runtime(
                    "telnet_terminal_open_failed",
                    format!("Failed to create Telnet terminal: {error}"),
                ),
            ),
        }
    }

    fn handle_native_plugin_sync_request(
        &mut self,
        request: NativePluginSyncRequest,
        cx: &mut Context<Self>,
    ) {
        match request.action {
            NativePluginSyncAction::ApplySavedConnectionsSnapshot {
                snapshot,
                conflict_strategy,
            } => {
                let response = self.finish_native_plugin_apply_saved_connections_snapshot(
                    request.request_id,
                    snapshot,
                    conflict_strategy,
                    cx,
                );
                let _ = request.response_tx.send(response);
            }
            NativePluginSyncAction::ReportProgress {
                plugin_id,
                registration_id,
                value,
            } => {
                self.update_native_plugin_progress(&plugin_id, &registration_id, value, cx);
                let _ = request.response_tx.send(plugin_runtime::PluginResponse::ok(
                    request.request_id,
                    Value::Null,
                ));
            }
            NativePluginSyncAction::ImportOxide {
                bytes,
                password,
                options,
                progress_registration_id,
                plugin_id,
            } => {
                let store = self.connection_store.clone();
                self.plugin_entity.update(cx, |plugins, _cx| {
                    plugins.start_oxide_import(
                        store,
                        plugin_id,
                        request.request_id,
                        bytes,
                        password,
                        options,
                        progress_registration_id,
                        request.response_tx,
                    );
                });
            }
        }
    }

    fn finish_native_plugin_apply_saved_connections_snapshot(
        &mut self,
        request_id: String,
        snapshot: SavedConnectionsSyncSnapshot,
        conflict_strategy: SavedConnectionsConflictStrategy,
        cx: &mut Context<Self>,
    ) -> plugin_runtime::PluginResponse {
        let mut store = self.connection_store.clone();
        match store.apply_saved_connections_snapshot(snapshot, conflict_strategy) {
            Ok(outcome) => {
                // Apply through the Workspace owner so saved connections,
                // tombstones, and cloud-sync dirty state advance together.
                self.connection_store = store;
                self.queue_cloud_sync_dirty_refresh(cx);
                plugin_runtime::PluginResponse::ok(request_id, json!(outcome.result))
            }
            Err(error) => plugin_runtime::PluginResponse::error(
                request_id,
                plugin_runtime::PluginError::runtime(
                    "plugin_sync_apply_saved_connections_failed",
                    error.to_string(),
                ),
            ),
        }
    }

    fn finish_native_plugin_oxide_import(
        &mut self,
        request_id: String,
        result: Result<NativePluginOxideImportCoreResult, ()>,
        options: NativePluginOxidePostImportOptions,
        cx: &mut Context<Self>,
    ) -> plugin_runtime::PluginResponse {
        let Ok(core) = result else {
            return plugin_runtime::PluginResponse::error(
                request_id,
                plugin_runtime::PluginError::runtime(
                    "plugin_sync_oxide_error",
                    "Native plugin .oxide import failed",
                ),
            );
        };

        self.connection_store = core.store;
        let mut envelope = core.envelope;
        // Tauri applies side-car forwards, quick commands, plugin settings,
        // app settings, and portable secrets only after the connection import
        // has committed. Native preserves that order on the Workspace owner.
        envelope.imported_forwards = self.apply_oxide_import_forward_records(&mut envelope);
        let (imported_quick_commands, skipped_quick_commands, quick_commands_errors) = self
            .apply_oxide_import_quick_commands(
                envelope.quick_commands_json.as_deref(),
                options.import_quick_commands,
                native_plugin_quick_command_import_strategy(options.quick_command_strategy),
                cx,
            );
        let imported_plugin_settings = self.apply_oxide_import_plugin_settings(
            &envelope.plugin_settings,
            options.import_plugin_settings,
            options.selected_plugin_ids.as_ref(),
        );
        let skipped_plugin_settings =
            !options.import_plugin_settings && !envelope.plugin_settings.is_empty();
        let (imported_app_settings, skipped_app_settings) = self.apply_oxide_import_app_settings(
            envelope.app_settings_json.as_deref(),
            options.import_app_settings,
            options.selected_app_settings_sections.as_ref(),
            cx,
        );
        self.apply_oxide_import_portable_secrets(&mut envelope, cx);
        self.queue_cloud_sync_dirty_refresh(cx);

        plugin_runtime::PluginResponse::ok(
            request_id,
            native_plugin_sync_import_result_value(
                &envelope,
                imported_app_settings,
                skipped_app_settings,
                imported_quick_commands,
                skipped_quick_commands,
                quick_commands_errors,
                imported_plugin_settings,
                skipped_plugin_settings,
            ),
        )
    }

    fn write_native_plugin_active_terminal_text(
        &mut self,
        text: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let connection_states = self
            .ssh_registry
            .list()
            .into_iter()
            .map(|info| {
                (
                    info.connection_id.clone(),
                    native_plugin_connection_state(&info.state),
                )
            })
            .collect::<HashMap<_, _>>();
        let target = native_plugin_active_terminal_target(self, &connection_states, cx);
        if target
            .get("connectionState")
            .and_then(Value::as_str)
            .is_some_and(|state| state != "active")
        {
            return false;
        }
        let Some(pane) = self.active_pane(cx) else {
            return false;
        };
        // Plugin writes are routed through the same terminal input method used
        // by AI tooling so shell input tracking and terminal input guards stay
        // on the native terminal pane rather than in the plugin runtime.
        pane.update(cx, |pane, cx| pane.send_ai_input_bytes(text.as_bytes(), cx));
        true
    }

    fn clear_native_plugin_node_terminal_buffer(&mut self, node_id: &str, cx: &mut Context<Self>) {
        let node_id = oxideterm_ssh::NodeId::new(node_id);
        let Some(node) = self.ssh_nodes.get(&node_id) else {
            return;
        };
        let Some(session_id) = node.terminal_ids.first().copied() else {
            return;
        };
        let Some(pane) = native_plugin_pane_for_session(self, session_id, cx) else {
            return;
        };
        // Tauri clearBuffer is host-side and void-returning: missing nodes are
        // no-ops, while an existing pane clears native emulator state without
        // writing bytes into the remote or local shell.
        pane.update(cx, |pane, cx| pane.clear_buffer(cx));
    }

    fn write_native_plugin_node_terminal_text(
        &mut self,
        node_id: &str,
        text: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let node_id = oxideterm_ssh::NodeId::new(node_id);
        let Some(node) = self.ssh_nodes.get(&node_id) else {
            return false;
        };
        let terminal_count = node.terminal_ids.len();
        let Some(runtime) = self.node_router.node_metadata(&node_id) else {
            return false;
        };
        if native_plugin_session_connection_state(
            &runtime.readiness,
            runtime.error.as_deref(),
            terminal_count,
        ) != "active"
        {
            return false;
        }
        let Some(session_id) = node.terminal_ids.first().copied() else {
            return false;
        };
        let Some(pane) = native_plugin_pane_for_session(self, session_id, cx) else {
            return false;
        };
        pane.update(cx, |pane, cx| pane.send_ai_input_bytes(text.as_bytes(), cx));
        true
    }

    pub(super) fn refresh_native_plugin_terminal_hooks(&mut self, cx: &mut Context<Self>) {
        self.refresh_native_plugin_terminal_input_interceptors(cx);
        self.refresh_native_plugin_terminal_output_processors(cx);
    }

    fn refresh_native_plugin_terminal_input_interceptors(&mut self, cx: &mut Context<Self>) {
        let (hooks, runtime_host) = {
            let plugins = self.plugin_entity.read(cx);
            (
                plugins
                    .registry()
                    .contributions()
                    .runtime_terminal_input_interceptors
                    .clone(),
                plugins.runtime_host(),
            )
        };
        let interceptor = if hooks.is_empty() {
            None
        } else {
            let runtime = self.forwarding_runtime.clone();
            let host_api_resolver = native_plugin_terminal_hook_host_api_resolver();
            Some(Arc::new(move |bytes: &[u8]| {
                native_plugin_apply_input_interceptors(
                    bytes,
                    &hooks,
                    runtime_host.clone(),
                    runtime.clone(),
                    host_api_resolver.clone(),
                )
            }) as TerminalInputInterceptor)
        };

        // Drop the registry borrow before updating pane entities through GPUI.
        let panes = self
            .tab_host
            .read(cx)
            .panes()
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for pane in panes {
            pane.update(cx, |pane, _cx| {
                pane.set_plugin_input_interceptor(interceptor.clone());
            });
        }
    }

    fn refresh_native_plugin_terminal_output_processors(&mut self, cx: &mut Context<Self>) {
        let (hooks, runtime_host) = {
            let plugins = self.plugin_entity.read(cx);
            (
                plugins
                    .registry()
                    .contributions()
                    .runtime_terminal_output_processors
                    .clone(),
                plugins.runtime_host(),
            )
        };
        let processor = if hooks.is_empty() {
            None
        } else {
            let runtime = self.forwarding_runtime.clone();
            let host_api_resolver = native_plugin_terminal_hook_host_api_resolver();
            Some(Arc::new(move |bytes: &[u8]| {
                native_plugin_apply_output_processors(
                    bytes,
                    &hooks,
                    runtime_host.clone(),
                    runtime.clone(),
                    host_api_resolver.clone(),
                )
            }) as TerminalOutputProcessor)
        };

        // Drop the registry borrow before updating pane entities through GPUI.
        let panes = self
            .tab_host
            .read(cx)
            .panes()
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for pane in panes {
            pane.update(cx, |pane, _cx| {
                pane.set_plugin_output_processor(processor.clone());
            });
        }
    }

    pub(super) fn handle_native_plugin_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let plugins = self.plugin_entity.read(cx);
        let confirm_is_visible = plugins.confirm_dialog().is_some()
            && plugins.confirm_phase() == oxideterm_gpui_ui::motion::ExitPhase::Visible;
        if !confirm_is_visible {
            return false;
        }

        match self.handle_standard_confirm_key(event, cx) {
            Some(super::ConfirmKeyboardAction::Cancel) => {
                self.respond_native_plugin_confirm(false, cx);
                true
            }
            Some(super::ConfirmKeyboardAction::Confirm) => {
                self.respond_native_plugin_confirm(true, cx);
                true
            }
            Some(super::ConfirmKeyboardAction::Handled) => true,
            None => false,
        }
    }

    pub(super) fn render_native_plugin_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let plugins = self.plugin_entity.read(cx);
        let dialog = plugins.confirm_dialog()?;
        Some(
            oxideterm_gpui_ui::confirm::confirm_dialog_with_focus_motion(
                &self.tokens,
                "native-plugin-confirm-motion",
                plugins.confirm_phase(),
                ConfirmDialogView {
                    variant: ConfirmDialogVariant::Default,
                    title: div()
                        .child(native_plugin_dialog_title(&dialog.plugin_id, &dialog.title))
                        .into_any_element(),
                    description: Some(div().child(dialog.description.clone()).into_any_element()),
                    cancel_label: div()
                        .child(self.i18n.t("common.actions.cancel"))
                        .into_any_element(),
                    confirm_label: div()
                        .child(self.i18n.t("common.actions.confirm"))
                        .into_any_element(),
                },
                self.standard_confirm_focus(),
                cx.listener(|this, _event, _window, cx| {
                    this.respond_native_plugin_confirm(false, cx);
                    cx.stop_propagation();
                }),
                cx.listener(|this, _event, _window, cx| {
                    this.respond_native_plugin_confirm(true, cx);
                    cx.stop_propagation();
                }),
            ),
        )
    }

    pub(super) fn native_plugin_layout_snapshot(&self, cx: &App) -> Value {
        native_plugin_layout_snapshot(
            self.sidebar_collapsed,
            self.active_tab_id(cx).map(|tab_id| tab_id.0.to_string()),
            self.tabs(cx).len(),
        )
    }

    pub(super) fn native_plugin_session_tree_snapshot(&self) -> Value {
        json!(self.native_plugin_session_tree_snapshot_values())
    }

    pub(super) fn native_plugin_saved_forwards_snapshot(&self) -> Value {
        native_plugin_forward_saved_forwards(self.forwarding_service.registry())
            .unwrap_or_else(|_| json!([]))
    }

    pub(super) fn native_plugin_transfer_snapshot(&self) -> Value {
        native_plugin_transfer_snapshot_array(&self.sftp_transfer_manager, None)
    }

    pub(super) fn native_plugin_profiler_snapshot(&self, cx: &mut Context<Self>) -> Value {
        native_plugin_profiler_snapshot_array(
            self.host_tools.read(cx).profiler_registry(),
            &native_plugin_profiler_node_connection_ids(self),
        )
    }

    pub(super) fn native_plugin_ide_snapshot(&self, cx: &mut Context<Self>) -> Value {
        native_plugin_ide_workspace_snapshot(self, cx)
            .map(|snapshot| native_plugin_ide_snapshot_value(&snapshot))
            .unwrap_or_else(|| {
                json!({
                    "isOpen": false,
                    "project": null,
                    "openFiles": [],
                    "activeFile": null,
                })
            })
    }

    pub(super) fn native_plugin_ai_snapshot(&self, cx: &App) -> Value {
        let settings = self.settings_store.settings();
        native_plugin_ai_snapshot_value(
            &self.ai_entity.read(cx).conversation_state(),
            &settings.ai.providers,
            settings.ai.active_provider_id.as_deref(),
            &settings.ai.model_context_windows,
        )
    }

    pub(super) fn native_plugin_last_event_log_id(&self) -> u64 {
        self.notification_center
            .event_log
            .entries
            .back()
            .map(|entry| entry.id)
            .unwrap_or_default()
    }

    pub(super) fn refresh_native_plugin_event_polling(&mut self, cx: &mut Context<Self>) {
        let mut samples = Vec::new();
        if self.has_native_plugin_subscription(
            super::plugin_host::NATIVE_PLUGIN_UI_LAYOUT_CHANGED_EVENT,
            cx,
        ) {
            samples.push((
                plugin_entity::PluginSubscriptionSample::Layout,
                self.native_plugin_layout_snapshot(cx),
            ));
        }
        if self.has_native_plugin_subscription(
            super::plugin_host::NATIVE_PLUGIN_SESSION_TREE_CHANGED_EVENT,
            cx,
        ) || self.has_native_plugin_subscription(
            super::plugin_host::NATIVE_PLUGIN_SESSION_NODE_STATE_CHANGED_EVENT,
            cx,
        ) {
            samples.push((
                plugin_entity::PluginSubscriptionSample::Sessions,
                self.native_plugin_session_tree_snapshot(),
            ));
        }
        if self.has_native_plugin_subscription(
            super::plugin_host::NATIVE_PLUGIN_FORWARD_SAVED_FORWARDS_CHANGED_EVENT,
            cx,
        ) {
            samples.push((
                plugin_entity::PluginSubscriptionSample::SavedForwards,
                self.native_plugin_saved_forwards_snapshot(),
            ));
        }
        if self.has_native_plugin_subscription(
            super::plugin_host::NATIVE_PLUGIN_TRANSFER_PROGRESS_EVENT,
            cx,
        ) || self.has_native_plugin_subscription(
            super::plugin_host::NATIVE_PLUGIN_TRANSFER_COMPLETE_EVENT,
            cx,
        ) || self.has_native_plugin_subscription(
            super::plugin_host::NATIVE_PLUGIN_TRANSFER_ERROR_EVENT,
            cx,
        ) {
            samples.push((
                plugin_entity::PluginSubscriptionSample::Transfers,
                self.native_plugin_transfer_snapshot(),
            ));
        }
        if self.has_native_plugin_subscription(
            super::plugin_host::NATIVE_PLUGIN_PROFILER_METRICS_EVENT,
            cx,
        ) {
            samples.push((
                plugin_entity::PluginSubscriptionSample::Profiler,
                self.native_plugin_profiler_snapshot(cx),
            ));
        }
        if self.has_native_plugin_subscription(
            super::plugin_host::NATIVE_PLUGIN_IDE_FILE_OPEN_EVENT,
            cx,
        ) || self.has_native_plugin_subscription(
            super::plugin_host::NATIVE_PLUGIN_IDE_FILE_CLOSE_EVENT,
            cx,
        ) || self.has_native_plugin_subscription(
            super::plugin_host::NATIVE_PLUGIN_IDE_ACTIVE_FILE_CHANGED_EVENT,
            cx,
        ) {
            samples.push((
                plugin_entity::PluginSubscriptionSample::Ide,
                self.native_plugin_ide_snapshot(cx),
            ));
        }
        if self
            .has_native_plugin_subscription(super::plugin_host::NATIVE_PLUGIN_AI_MESSAGE_EVENT, cx)
        {
            samples.push((
                plugin_entity::PluginSubscriptionSample::Ai,
                self.native_plugin_ai_snapshot(cx),
            ));
        }
        let event_log_last_id = self
            .has_native_plugin_subscription(
                super::plugin_host::NATIVE_PLUGIN_EVENT_LOG_ENTRY_EVENT,
                cx,
            )
            .then(|| self.native_plugin_last_event_log_id());
        if event_log_last_id.is_some() {
            samples.push((
                plugin_entity::PluginSubscriptionSample::EventLog,
                Value::Null,
            ));
        }
        self.plugin_entity.update(cx, move |plugins, cx| {
            plugins.configure_subscription_samples(samples, event_log_last_id, cx);
        });
    }

    pub(in crate::workspace) fn sample_native_plugin_subscriptions(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let sample_kinds = self.plugin_entity.read(cx).subscription_samples();
        for kind in sample_kinds {
            match kind {
                plugin_entity::PluginSubscriptionSample::Layout => {
                    self.emit_native_plugin_layout_if_changed(cx)
                }
                plugin_entity::PluginSubscriptionSample::Sessions => {
                    self.emit_native_plugin_sessions_if_changed(cx)
                }
                plugin_entity::PluginSubscriptionSample::SavedForwards => {
                    self.emit_native_plugin_saved_forwards_if_changed(cx)
                }
                plugin_entity::PluginSubscriptionSample::Transfers => {
                    self.emit_native_plugin_transfers_if_changed(cx)
                }
                plugin_entity::PluginSubscriptionSample::Profiler => {
                    self.emit_native_plugin_profiler_if_changed(cx)
                }
                plugin_entity::PluginSubscriptionSample::Ide => {
                    self.emit_native_plugin_ide_if_changed(cx)
                }
                plugin_entity::PluginSubscriptionSample::Ai => {
                    self.emit_native_plugin_ai_if_changed(cx)
                }
                plugin_entity::PluginSubscriptionSample::EventLog => {
                    self.emit_native_plugin_event_log_entries(cx)
                }
            }
        }
    }

    fn has_native_plugin_subscription(&self, event_name: &str, cx: &App) -> bool {
        !self
            .plugin_entity
            .read(cx)
            .registry()
            .contributions()
            .runtime_event_subscriptions_for(event_name)
            .is_empty()
    }

    fn native_plugin_session_tree_snapshot_values(&self) -> Vec<Value> {
        let titles = self
            .ssh_nodes
            .iter()
            .map(|(node_id, node)| (node_id.0.clone(), node.title.clone()))
            .collect::<HashMap<_, _>>();
        let terminal_ids = self
            .ssh_nodes
            .iter()
            .map(|(node_id, node)| {
                (
                    node_id.0.clone(),
                    node.terminal_ids
                        .iter()
                        .map(|session_id| session_id.0.to_string())
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<HashMap<_, _>>();
        native_plugin_session_tree_from_nodes(
            self.node_router.node_metadata_snapshots(),
            &titles,
            &terminal_ids,
        )
    }

    fn emit_native_plugin_layout_if_changed(&mut self, cx: &mut Context<Self>) {
        let layout = self.native_plugin_layout_snapshot(cx);
        let (previous, layout) = self.plugin_entity.update(cx, |plugins, _cx| {
            plugins.update_subscription_snapshot(
                plugin_entity::PluginSubscriptionSample::Layout,
                layout,
            )
        });
        if previous.is_none() {
            return;
        }
        let has_subscribers = self.has_native_plugin_subscription(
            super::plugin_host::NATIVE_PLUGIN_UI_LAYOUT_CHANGED_EVENT,
            cx,
        );
        if has_subscribers {
            // Tauri onLayoutChange compares the serialized layout snapshot
            // before invoking callbacks. Native keeps that same edge-triggered
            // behavior and emits only when the observed shape changes.
            self.emit_native_plugin_event_to_subscribers(
                super::plugin_host::NATIVE_PLUGIN_UI_LAYOUT_CHANGED_EVENT,
                layout,
                cx,
            );
        }
    }

    fn emit_native_plugin_sessions_if_changed(&mut self, cx: &mut Context<Self>) {
        let tree = self.native_plugin_session_tree_snapshot();
        let (previous_tree, tree) = self.plugin_entity.update(cx, |plugins, _cx| {
            plugins.update_subscription_snapshot(
                plugin_entity::PluginSubscriptionSample::Sessions,
                tree,
            )
        });
        let Some(previous_tree) = previous_tree else {
            return;
        };

        let previous_states = native_plugin_session_state_map(&previous_tree);
        let next_states = native_plugin_session_state_map(&tree);

        let has_tree_subscribers = self.has_native_plugin_subscription(
            super::plugin_host::NATIVE_PLUGIN_SESSION_TREE_CHANGED_EVENT,
            cx,
        );
        if has_tree_subscribers {
            // Tauri's onTreeChange callback receives the full frozen tree after
            // each Zustand nodes update. Native emits the same tree payload
            // over PluginEvent frames when the serialized projection changes.
            self.emit_native_plugin_event_to_subscribers(
                super::plugin_host::NATIVE_PLUGIN_SESSION_TREE_CHANGED_EVENT,
                tree,
                cx,
            );
        }

        let has_node_state_subscribers = self.has_native_plugin_subscription(
            super::plugin_host::NATIVE_PLUGIN_SESSION_NODE_STATE_CHANGED_EVENT,
            cx,
        );
        if has_node_state_subscribers {
            let mut node_ids = previous_states
                .keys()
                .chain(next_states.keys())
                .cloned()
                .collect::<Vec<_>>();
            node_ids.sort();
            node_ids.dedup();
            for node_id in node_ids {
                let previous = previous_states.get(&node_id).map(String::as_str);
                let next = next_states
                    .get(&node_id)
                    .map(String::as_str)
                    .unwrap_or("idle");
                if previous != Some(next) {
                    self.emit_native_plugin_event_to_subscribers(
                        super::plugin_host::NATIVE_PLUGIN_SESSION_NODE_STATE_CHANGED_EVENT,
                        json!({
                            "nodeId": node_id,
                            "state": next,
                        }),
                        cx,
                    );
                }
            }
        }
    }

    fn emit_native_plugin_saved_forwards_if_changed(&mut self, cx: &mut Context<Self>) {
        let saved_forwards = self.native_plugin_saved_forwards_snapshot();
        let (previous, saved_forwards) = self.plugin_entity.update(cx, |plugins, _cx| {
            plugins.update_subscription_snapshot(
                plugin_entity::PluginSubscriptionSample::SavedForwards,
                saved_forwards,
            )
        });
        if previous.is_none() {
            return;
        }

        let has_subscribers = self.has_native_plugin_subscription(
            super::plugin_host::NATIVE_PLUGIN_FORWARD_SAVED_FORWARDS_CHANGED_EVENT,
            cx,
        );
        if has_subscribers {
            // Tauri's onSavedForwardsChange listener receives the current
            // frozen saved-forward list after the backend update event. Native
            // emits the same list whenever the host-owned snapshot changes.
            self.emit_native_plugin_event_to_subscribers(
                super::plugin_host::NATIVE_PLUGIN_FORWARD_SAVED_FORWARDS_CHANGED_EVENT,
                saved_forwards,
                cx,
            );
        }
    }

    fn emit_native_plugin_transfers_if_changed(&mut self, cx: &mut Context<Self>) {
        let transfers = self.native_plugin_transfer_snapshot();
        let (previous_transfers, transfers) = self.plugin_entity.update(cx, |plugins, _cx| {
            plugins.update_subscription_snapshot(
                plugin_entity::PluginSubscriptionSample::Transfers,
                transfers,
            )
        });
        let previous_states = previous_transfers
            .as_ref()
            .map(native_plugin_transfer_state_map)
            .unwrap_or_default();
        let next_states = native_plugin_transfer_state_map(&transfers);
        let changed = previous_transfers.is_some();

        let has_progress_subscribers = self.has_native_plugin_subscription(
            super::plugin_host::NATIVE_PLUGIN_TRANSFER_PROGRESS_EVENT,
            cx,
        );
        if has_progress_subscribers
            && self.plugin_entity.update(cx, |plugins, _cx| {
                plugins.transfer_progress_due(NATIVE_PLUGIN_TRANSFER_PROGRESS_INTERVAL)
            })
        {
            // Tauri's transfer progress bridge is throttled to 500ms. Native keeps
            // the same throttle while polling the backend-owned SFTP transfer map.
            for transfer in
                native_plugin_transfer_values_by_state(&transfers, BackgroundTransferState::Active)
            {
                self.emit_native_plugin_event_to_subscribers(
                    super::plugin_host::NATIVE_PLUGIN_TRANSFER_PROGRESS_EVENT,
                    transfer,
                    cx,
                );
            }
        }

        if !changed {
            return;
        }

        let has_complete_subscribers = self.has_native_plugin_subscription(
            super::plugin_host::NATIVE_PLUGIN_TRANSFER_COMPLETE_EVENT,
            cx,
        );
        if has_complete_subscribers {
            for transfer in native_plugin_transfer_transition_values(
                &transfers,
                &previous_states,
                &next_states,
                BackgroundTransferState::Completed,
            ) {
                self.emit_native_plugin_event_to_subscribers(
                    super::plugin_host::NATIVE_PLUGIN_TRANSFER_COMPLETE_EVENT,
                    transfer,
                    cx,
                );
            }
        }

        let has_error_subscribers = self.has_native_plugin_subscription(
            super::plugin_host::NATIVE_PLUGIN_TRANSFER_ERROR_EVENT,
            cx,
        );
        if has_error_subscribers {
            for transfer in native_plugin_transfer_transition_values(
                &transfers,
                &previous_states,
                &next_states,
                BackgroundTransferState::Error,
            ) {
                self.emit_native_plugin_event_to_subscribers(
                    super::plugin_host::NATIVE_PLUGIN_TRANSFER_ERROR_EVENT,
                    transfer,
                    cx,
                );
            }
        }
    }

    fn emit_native_plugin_profiler_if_changed(&mut self, cx: &mut Context<Self>) {
        let metrics = self.native_plugin_profiler_snapshot(cx);
        let (previous_metrics, metrics) = self.plugin_entity.update(cx, |plugins, _cx| {
            plugins.update_subscription_snapshot(
                plugin_entity::PluginSubscriptionSample::Profiler,
                metrics,
            )
        });
        let Some(previous_metrics) = previous_metrics else {
            return;
        };
        let previous_timestamps = native_plugin_profiler_timestamp_map(&previous_metrics);
        let next_timestamps = native_plugin_profiler_timestamp_map(&metrics);

        let subscriptions = self
            .plugin_entity
            .read(cx)
            .registry()
            .contributions()
            .runtime_event_subscriptions_for(
                super::plugin_host::NATIVE_PLUGIN_PROFILER_METRICS_EVENT,
            );
        let metrics_due = self.plugin_entity.update(cx, |plugins, _cx| {
            plugins.runtime_profiler_metrics_due(NATIVE_PLUGIN_PROFILER_METRICS_INTERVAL)
        });
        if subscriptions.is_empty() || !metrics_due {
            return;
        }

        for entry in native_plugin_profiler_changed_metric_entries(
            &metrics,
            &previous_timestamps,
            &next_timestamps,
        ) {
            let node_id = entry
                .get("nodeId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let Some(metric_payload) = entry.get("metrics").cloned() else {
                continue;
            };
            for subscription in subscriptions.iter().filter(|subscription| {
                native_plugin_subscription_allows_node(subscription.filter.as_ref(), &node_id)
            }) {
                let mut payload = metric_payload.clone();
                if let Value::Object(fields) = &mut payload {
                    fields.insert(
                        "registrationId".to_string(),
                        Value::String(subscription.registration_id.clone()),
                    );
                }
                // Tauri's profiler store emits one throttled metric snapshot per
                // subscribed node. Native keeps node filtering at the host bridge
                // so process runtimes do not need to sample unrelated nodes.
                self.dispatch_native_plugin_event(
                    subscription.plugin_id.clone(),
                    super::plugin_host::NATIVE_PLUGIN_PROFILER_METRICS_EVENT,
                    payload,
                    cx,
                );
            }
        }
    }

    fn emit_native_plugin_ide_if_changed(&mut self, cx: &mut Context<Self>) {
        let next = self.native_plugin_ide_snapshot(cx);
        let (previous, next) = self.plugin_entity.update(cx, |plugins, _cx| {
            plugins.update_subscription_snapshot(plugin_entity::PluginSubscriptionSample::Ide, next)
        });
        let Some(previous) = previous else {
            return;
        };
        let previous_files = native_plugin_ide_file_map(&previous);
        let next_files = native_plugin_ide_file_map(&next);
        let previous_active = native_plugin_ide_active_file_path(&previous);
        let next_active = native_plugin_ide_active_file_path(&next);

        for (path, file) in &next_files {
            if !previous_files.contains_key(path) {
                self.emit_native_plugin_event_to_subscribers(
                    super::plugin_host::NATIVE_PLUGIN_IDE_FILE_OPEN_EVENT,
                    file.clone(),
                    cx,
                );
            }
        }
        for path in previous_files.keys() {
            if !next_files.contains_key(path) {
                self.emit_native_plugin_event_to_subscribers(
                    super::plugin_host::NATIVE_PLUGIN_IDE_FILE_CLOSE_EVENT,
                    json!(path),
                    cx,
                );
            }
        }
        if previous_active != next_active {
            // Tauri's active-file subscription receives the active file snapshot
            // or null after activeTabId changes. Native compares the same path
            // projection from the host-owned IDE surface.
            self.emit_native_plugin_event_to_subscribers(
                super::plugin_host::NATIVE_PLUGIN_IDE_ACTIVE_FILE_CHANGED_EVENT,
                next.get("activeFile").cloned().unwrap_or(Value::Null),
                cx,
            );
        }
    }

    fn emit_native_plugin_ai_if_changed(&mut self, cx: &mut Context<Self>) {
        let next = self.native_plugin_ai_snapshot(cx);
        let (previous, next) = self.plugin_entity.update(cx, |plugins, _cx| {
            plugins.update_subscription_snapshot(plugin_entity::PluginSubscriptionSample::Ai, next)
        });
        let Some(previous) = previous else {
            return;
        };
        let previous_counts = native_plugin_ai_message_count_map(&previous);

        for event in native_plugin_ai_new_message_events(&next, &previous_counts) {
            // AI message events intentionally omit message content; plugins can
            // explicitly request sanitized history through ctx.ai.getMessages.
            self.emit_native_plugin_event_to_subscribers(
                super::plugin_host::NATIVE_PLUGIN_AI_MESSAGE_EVENT,
                event,
                cx,
            );
        }
    }

    fn emit_native_plugin_event_log_entries(&mut self, cx: &mut Context<Self>) {
        let next_last_id = self.native_plugin_last_event_log_id();
        let last_seen = self.plugin_entity.update(cx, |plugins, _cx| {
            plugins.advance_event_log_last_id(next_last_id)
        });
        let new_entries = self
            .notification_center
            .event_log
            .entries
            .iter()
            .filter(|entry| entry.id > last_seen)
            .cloned()
            .collect::<Vec<_>>();
        if new_entries.is_empty() {
            return;
        }

        let has_subscribers = self.has_native_plugin_subscription(
            super::plugin_host::NATIVE_PLUGIN_EVENT_LOG_ENTRY_EVENT,
            cx,
        );
        if has_subscribers {
            for entry in new_entries {
                // Tauri's onEntry subscription only invokes callbacks for
                // entries appended after subscription setup. Native tracks the
                // monotonic id and emits one PluginEvent per new log row.
                self.emit_native_plugin_event_to_subscribers(
                    super::plugin_host::NATIVE_PLUGIN_EVENT_LOG_ENTRY_EVENT,
                    native_plugin_event_log_entry_snapshot(&entry),
                    cx,
                );
            }
        }
    }

    pub(super) fn bootstrap_native_plugin_runtime(&mut self, cx: &mut Context<Self>) {
        let host_api_resolver = self.native_plugin_host_api_resolver(cx);
        let _started = self.plugin_entity.update(cx, |plugins, _cx| {
            plugins.start_runtime_bootstrap(host_api_resolver)
        });
        cx.notify();
    }

    pub(in crate::workspace) fn apply_native_plugin_runtime_intents(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let intents = self
            .plugin_entity
            .update(cx, |plugins, _cx| plugins.take_runtime_intents());
        for intent in intents {
            match intent {
                plugin_entity::PluginRuntimeIntent::ApplyEffects {
                    plugin_id,
                    effects,
                    refresh,
                } => {
                    for effect in effects {
                        self.handle_native_plugin_outbound_effect(&plugin_id, effect, cx);
                    }
                    self.refresh_native_plugin_event_polling(cx);
                    match refresh {
                        plugin_entity::PluginRuntimeAdapterRefresh::TerminalHooks => {
                            self.refresh_native_plugin_terminal_hooks(cx)
                        }
                        plugin_entity::PluginRuntimeAdapterRefresh::TerminalInputInterceptors => {
                            self.refresh_native_plugin_terminal_input_interceptors(cx)
                        }
                        plugin_entity::PluginRuntimeAdapterRefresh::All => {
                            self.refresh_native_plugin_terminal_hooks(cx);
                            self.refresh_native_plugin_terminal_input_interceptors(cx);
                        }
                    }
                }
                plugin_entity::PluginRuntimeIntent::StateChanged => {}
            }
        }
        cx.notify();
    }

    pub(in crate::workspace) fn apply_native_plugin_oxide_import_intents(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let intents = self
            .plugin_entity
            .update(cx, |plugins, _cx| plugins.take_oxide_import_intents());
        for intent in intents {
            match intent {
                plugin_entity::PluginOxideImportIntent::Progress {
                    plugin_id,
                    registration_id,
                    value,
                } => {
                    self.update_native_plugin_progress(&plugin_id, &registration_id, value, cx);
                }
                plugin_entity::PluginOxideImportIntent::Complete {
                    plugin_id,
                    progress_registration_id,
                    request_id,
                    result,
                    options,
                    response_tx,
                } => {
                    let response =
                        self.finish_native_plugin_oxide_import(request_id, result, options, cx);
                    if let Some(registration_id) = progress_registration_id {
                        self.update_native_plugin_progress(
                            &plugin_id,
                            &registration_id,
                            native_plugin_sync_progress_value(
                                "Importing .oxide",
                                "complete",
                                1,
                                1,
                                true,
                            ),
                            cx,
                        );
                    }
                    let _ = response_tx.send(response);
                }
            }
        }
        cx.notify();
    }

    pub(super) fn dispatch_native_plugin_command(
        &mut self,
        plugin_id: String,
        command: String,
        cx: &mut Context<Self>,
    ) {
        let host_api_resolver = self.native_plugin_host_api_resolver(cx);
        self.plugin_entity.update(cx, |plugins, _cx| {
            plugins.start_runtime_command(plugin_id, command, host_api_resolver);
        });
    }

    pub(super) fn dispatch_runtime_plugin_keybinding(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(normalized_keybinding) =
            crate::keybindings::normalize_plugin_keystroke(&event.keystroke)
        else {
            return false;
        };
        let Some(keybinding) = self
            .plugin_entity
            .read(cx)
            .registry()
            .contributions()
            .runtime_keybinding_for_normalized_key(&normalized_keybinding)
            .cloned()
        else {
            return false;
        };

        // Tauri registerKeybinding stores a handler closure; native keeps the
        // same user-visible result by routing the matched key to the command RPC
        // associated with the host-owned registration.
        self.dispatch_native_plugin_command(keybinding.plugin_id, keybinding.command, cx);
        true
    }

    pub(super) fn emit_native_plugin_event_to_subscribers(
        &mut self,
        event_name: &str,
        payload: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        self.emit_native_plugin_event_to_matching_subscribers(event_name, None, payload, cx);
    }

    fn emit_native_plugin_event_to_matching_subscribers(
        &mut self,
        event_name: &str,
        plugin_filter: Option<&str>,
        payload: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        let subscriptions = self
            .plugin_entity
            .read(cx)
            .registry()
            .contributions()
            .runtime_event_subscriptions_for(event_name);
        for subscription in subscriptions {
            if plugin_filter.is_some_and(|plugin_id| subscription.plugin_id != plugin_id) {
                continue;
            }
            let mut event_payload = payload.clone();
            if let serde_json::Value::Object(fields) = &mut event_payload {
                fields.insert(
                    "registrationId".to_string(),
                    serde_json::Value::String(subscription.registration_id.clone()),
                );
            }
            // Native event subscriptions replace Tauri callback closures with a
            // PluginEvent frame so process runtimes never execute code on the
            // GPUI render stack.
            self.dispatch_native_plugin_event(
                subscription.plugin_id,
                event_name,
                event_payload,
                cx,
            );
        }
    }

    pub(super) fn dispatch_native_plugin_event(
        &mut self,
        plugin_id: String,
        event_name: &str,
        payload: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        let host_api_resolver = self.native_plugin_host_api_resolver(cx);
        let event = plugin_runtime::PluginEvent {
            name: event_name.to_string(),
            payload,
        };
        self.plugin_entity.update(cx, |plugins, _cx| {
            plugins.start_runtime_event(plugin_id, event, host_api_resolver);
        });
    }

    pub(in crate::workspace) fn native_plugin_host_api_resolver(
        &self,
        cx: &mut Context<Self>,
    ) -> plugin_runtime::NativeHostApiResolver {
        let snapshot = native_plugin_host_api_snapshot_from_workspace(self, cx);
        let request_senders = self.plugin_entity.read(cx).runtime_request_senders();
        let sftp_router = self.node_router.clone();
        let sftp_runtime = self.forwarding_runtime.clone();
        let forwarding_registry = self.forwarding_service.registry().clone();
        let forwarding_runtime = self.forwarding_runtime.clone();
        let transfer_manager = self.sftp_transfer_manager.clone();
        let profiler_registry = self.host_tools.read(cx).profiler_registry().clone();
        let profiler_node_connection_ids = native_plugin_profiler_node_connection_ids(self);
        let ide_snapshot = self.native_plugin_ide_snapshot(cx);
        let ai_snapshot = self.native_plugin_ai_snapshot(cx);
        let forward_valid_owner_connection_ids = self
            .connection_store
            .connections()
            .iter()
            .map(|connection| connection.id.clone())
            .collect::<HashSet<_>>();
        let sync_saved_connections = json!(self.connection_store.connection_infos());
        let sync_connection_store = self.connection_store.clone();
        let sync_saved_connections_snapshot =
            self.connection_store.export_saved_connections_snapshot();
        let sync_local_metadata = self.connection_store.local_sync_metadata();
        let sync_saved_forwards_revision = self
            .forwarding_service
            .registry()
            .export_saved_forwards_snapshot()
            .ok()
            .map(|snapshot| snapshot.revision);
        let sync_plugin_settings =
            oxideterm_cloud_sync::plugin_settings::load_plugin_settings(self.settings_store.path())
                .unwrap_or_default();
        let sync_plugin_settings_revisions =
            native_plugin_settings_revision_map(&sync_plugin_settings);
        let plugin_secret_store = self.ai_entity.read(cx).key_store().clone();
        let telnet_transport_plugins = self
            .plugin_entity
            .read(cx)
            .registry()
            .contributions()
            .terminal_transports
            .iter()
            .filter(|transport| transport.transport == "telnet")
            .map(|transport| transport.plugin_id.clone())
            .collect::<std::collections::HashSet<_>>();
        Arc::new(move |plugin_id, permissions, call| {
            if call.namespace == "api" && call.method == "invoke" {
                return Some(native_plugin_api_invoke_response(
                    &snapshot,
                    &plugin_id,
                    call,
                    NativePluginBackendAdapters {
                        permissions: &permissions,
                        sftp_router: &sftp_router,
                        sftp_runtime: &sftp_runtime,
                        forwarding_registry: &forwarding_registry,
                        forwarding_runtime: &forwarding_runtime,
                        transfer_manager: &transfer_manager,
                    },
                ));
            }
            if call.namespace == "ui" && call.method == "showProgress" {
                return Some(native_plugin_show_progress_response(
                    &plugin_id,
                    call,
                    Some(&request_senders.sync),
                ));
            }
            if call.namespace == "ui" && call.method == "showConfirm" {
                return Some(native_plugin_show_confirm_response(
                    &plugin_id,
                    call,
                    &request_senders.confirm,
                ));
            }
            if call.namespace == "secrets" {
                return Some(native_plugin_secret_response(
                    &plugin_id,
                    call,
                    &plugin_secret_store,
                ));
            }
            if call.namespace == "sftp" {
                return Some(native_plugin_sftp_response(
                    call,
                    &permissions,
                    &sftp_router,
                    &sftp_runtime,
                    Some(&transfer_manager),
                ));
            }
            if call.namespace == "scp" {
                return Some(native_plugin_scp_response(
                    call,
                    &permissions,
                    &sftp_router,
                    &sftp_runtime,
                    &transfer_manager,
                ));
            }
            if call.namespace == "forward" {
                return Some(native_plugin_forward_response(
                    call,
                    &permissions,
                    &forwarding_registry,
                    &forwarding_runtime,
                    &forward_valid_owner_connection_ids,
                ));
            }
            if call.namespace == "sync" {
                return Some(native_plugin_sync_response(
                    &plugin_id,
                    call,
                    &sync_connection_store,
                    &sync_saved_connections,
                    sync_saved_connections_snapshot.as_ref(),
                    sync_local_metadata.as_ref(),
                    sync_saved_forwards_revision.as_deref(),
                    &sync_plugin_settings,
                    &sync_plugin_settings_revisions,
                    Some(&request_senders.sync),
                ));
            }
            if call.namespace == "transfers" {
                return Some(native_plugin_transfers_response(call, &transfer_manager));
            }
            if call.namespace == "hostTools"
                && matches!(
                    call.method.as_str(),
                    "getExtensions" | "capture" | "execute" | "terminate" | "runExtension"
                )
            {
                return Some(native_plugin_host_tools_response(
                    &plugin_id,
                    call,
                    &permissions,
                    snapshot.registry.contributions(),
                    &sftp_router,
                    &sftp_runtime,
                ));
            }
            if call.namespace == "profiler" {
                return Some(native_plugin_profiler_response(
                    call,
                    &profiler_registry,
                    &profiler_node_connection_ids,
                ));
            }
            if call.namespace == "ide" {
                return Some(native_plugin_ide_response(call, &ide_snapshot));
            }
            if call.namespace == "ai" {
                return Some(native_plugin_ai_response(call, &ai_snapshot));
            }
            if call.namespace == "terminal"
                && matches!(
                    call.method.as_str(),
                    "writeToActive" | "writeToNode" | "clearBuffer"
                )
            {
                return Some(native_plugin_terminal_response(
                    call,
                    &request_senders.terminal,
                ));
            }
            if call.namespace == "terminal" && call.method == "openTelnet" {
                if !telnet_transport_plugins.contains(&plugin_id) {
                    return Some(plugin_runtime::PluginResponse::error(
                        call.request_id,
                        plugin_runtime::PluginError::protocol(
                            "terminal_transport_not_declared",
                            "terminal.openTelnet requires contributes.terminalTransports to include \"telnet\"",
                        ),
                    ));
                }
                return Some(native_plugin_terminal_response(
                    call,
                    &request_senders.terminal,
                ));
            }
            native_plugin_returnable_host_api_response(&snapshot, &plugin_id, call)
        })
    }

    fn handle_native_plugin_outbound_effect(
        &mut self,
        plugin_id: &str,
        effect: plugin_runtime::PluginOutboundEffect,
        cx: &mut Context<Self>,
    ) {
        match effect {
            plugin_runtime::PluginOutboundEffect::HostCall {
                namespace,
                method,
                args,
                ..
            } => self.handle_native_plugin_host_call(plugin_id, &namespace, &method, args, cx),
            plugin_runtime::PluginOutboundEffect::Progress {
                registration_id,
                value,
            } => self.update_native_plugin_progress(plugin_id, &registration_id, value, cx),
            _ => {}
        }
    }

    fn handle_native_plugin_host_call(
        &mut self,
        plugin_id: &str,
        namespace: &str,
        method: &str,
        args: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        let is_product_effect = matches!(
            (namespace, method),
            ("connections", "connect" | "reconnect" | "disconnect")
                | (
                    "notifications",
                    "markRead" | "markAllRead" | "setDnd" | "remove" | "clear"
                )
                | ("quickCommands", "execute" | "upsert" | "remove")
                | ("theme", "setActive")
                | (
                    "ide",
                    "openFile"
                        | "replaceActiveText"
                        | "insertActiveText"
                        | "saveActive"
                        | "closeFile"
                        | "refreshProject"
                )
                | (
                    "ai",
                    "createConversation"
                        | "selectConversation"
                        | "sendMessage"
                        | "cancelGeneration"
                        | "deleteConversation"
                        | "clearConversations"
                )
                | (
                    "cloudSync",
                    "check" | "upload" | "pullPreview" | "applyPreview" | "setAutoUpload"
                )
        );
        if is_product_effect {
            let _ =
                self.handle_native_plugin_product_host_call(plugin_id, namespace, method, args, cx);
            return;
        }
        if matches!(
            namespace,
            "connections"
                | "sessions"
                | "eventLog"
                | "notifications"
                | "cloudSync"
                | "quickCommands"
                | "theme"
                | "terminal"
                | "sftp"
                | "scp"
                | "forward"
                | "transfers"
                | "profiler"
                | "hostTools"
                | "ide"
                | "ai"
                | "secrets"
                | "sync"
        ) || (namespace == "app" && method != "refreshAfterExternalSync")
            || (namespace == "storage" && method == "get")
            || (namespace == "settings" && matches!(method, "get" | "exportSyncableSettings"))
        {
            // The synchronous resolver already completed these calls. Their
            // retained outbound effects are audit records, not a second action.
            return;
        }
        match (namespace, method) {
            ("ui", "showToast") => self.push_native_plugin_toast(plugin_id, args, cx),
            ("ui", "showNotification") => self.push_native_plugin_notification(plugin_id, args, cx),
            ("ui", "registerTabView") => self.register_native_plugin_ui_contribution(
                plugin_id,
                plugin_runtime::PluginRegistrationKind::Tab,
                args,
                cx,
            ),
            ("ui", "registerSidebarPanel") => self.register_native_plugin_ui_contribution(
                plugin_id,
                plugin_runtime::PluginRegistrationKind::SidebarPanel,
                args,
                cx,
            ),
            ("ui", "registerActivityBarItem") => self.register_native_plugin_ui_contribution(
                plugin_id,
                plugin_runtime::PluginRegistrationKind::ActivityBarItem,
                args,
                cx,
            ),
            ("ui", "openTab") => self.open_native_plugin_tab_from_args(plugin_id, args, cx),
            ("ui", "showConfirm") => {
                // The stdio transport still records returnable host calls as
                // outbound effects for auditing. The resolver already opened
                // the protected dialog and returned the boolean to the plugin.
            }
            ("app", "refreshAfterExternalSync") => {
                self.refresh_native_after_external_sync(plugin_id, cx)
            }
            ("events", "emit") => self.emit_native_plugin_custom_event(plugin_id, args, cx),
            ("storage", "set") => self.set_native_plugin_storage(plugin_id, args, cx),
            ("storage", "remove") => self.remove_native_plugin_storage(plugin_id, args, cx),
            ("settings", "set") => self.set_native_plugin_setting(plugin_id, args, cx),
            ("settings", "applySyncableSettings") => {
                self.apply_native_plugin_syncable_settings(plugin_id, args, cx)
            }
            _ => {
                self.plugin_entity.update(cx, |plugins, _cx| {
                    plugins.registry_mut().record_manager_error(
                        plugin_id.to_string(),
                        format!("Unsupported native plugin host call \"{namespace}.{method}\""),
                    );
                });
            }
        }
    }

    fn register_native_plugin_ui_contribution(
        &mut self,
        plugin_id: &str,
        kind: plugin_runtime::PluginRegistrationKind,
        args: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        match native_plugin_ui_registration_from_args(plugin_id, kind, &args) {
            Ok(registration) => {
                // Runtime protocol frames and ctx.ui calls share one mutation
                // path so manifest gates and schema validation cannot diverge.
                let registration_result = self.plugin_entity.update(cx, |plugins, _cx| {
                    plugins
                        .registry_mut()
                        .apply_runtime_registration(registration)
                });
                if let Err(error) = registration_result {
                    self.plugin_entity.update(cx, |plugins, _cx| {
                        plugins.registry_mut().record_manager_error(
                            plugin_id.to_string(),
                            format!("Native plugin UI registration failed: {error}"),
                        );
                    });
                }
            }
            Err(error) => {
                self.plugin_entity.update(cx, |plugins, _cx| {
                    plugins.registry_mut().record_manager_error(
                        plugin_id.to_string(),
                        format!("Native plugin UI registration failed: {error}"),
                    );
                });
            }
        }
        cx.notify();
    }

    fn open_native_plugin_tab_from_args(
        &mut self,
        plugin_id: &str,
        args: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        let Some(tab_id) = native_plugin_ui_tab_id_arg(&args) else {
            self.plugin_entity.update(cx, |plugins, _cx| {
                plugins.registry_mut().record_manager_error(
                    plugin_id.to_string(),
                    "Native plugin ui.openTab requires args.tabId".to_string(),
                );
            });
            return;
        };
        if let Err(error) = self.open_native_plugin_tab(plugin_id, &tab_id, cx) {
            self.plugin_entity.update(cx, |plugins, _cx| {
                plugins.registry_mut().record_manager_error(
                    plugin_id.to_string(),
                    format!("Native plugin ui.openTab failed: {error}"),
                );
            });
        }
    }

    fn push_native_plugin_toast(
        &mut self,
        plugin_id: &str,
        args: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        let title = args
            .get("title")
            .and_then(|value| value.as_str())
            .unwrap_or("Plugin")
            .to_string();
        let description = args
            .get("description")
            .and_then(|value| value.as_str())
            .map(str::to_string);
        let variant = args
            .get("variant")
            .and_then(|value| value.as_str())
            .map(native_plugin_toast_variant)
            .unwrap_or(TerminalNoticeVariant::Default);

        self.apply_workspace_overlay_intent(
            WorkspaceOverlayIntent::Notice {
                notice: TerminalNotice {
                    title: native_plugin_notice_title(plugin_id, title),
                    description,
                    status_text: None,
                    progress: None,
                    variant,
                },
                ttl: NATIVE_PLUGIN_TOAST_TTL,
            },
            cx,
        );
    }

    fn push_native_plugin_notification(
        &mut self,
        plugin_id: &str,
        args: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        let title = args
            .get("title")
            .and_then(|value| value.as_str())
            .unwrap_or("Plugin")
            .to_string();
        let description = args
            .get("body")
            .and_then(|value| value.as_str())
            .map(str::to_string);
        let variant = args
            .get("severity")
            .and_then(|value| value.as_str())
            .map(native_plugin_notification_variant)
            .unwrap_or(TerminalNoticeVariant::Default);

        self.apply_workspace_overlay_intent(
            WorkspaceOverlayIntent::Notice {
                notice: TerminalNotice {
                    title: native_plugin_notice_title(plugin_id, title),
                    description,
                    status_text: None,
                    progress: None,
                    variant,
                },
                ttl: NATIVE_PLUGIN_TOAST_TTL,
            },
            cx,
        );
    }

    fn refresh_native_after_external_sync(&mut self, plugin_id: &str, cx: &mut Context<Self>) {
        if let Err(error) = self.reload_after_external_sync(cx) {
            self.plugin_entity.update(cx, |plugins, _cx| {
                plugins.registry_mut().record_manager_error(
                    plugin_id.to_string(),
                    format!("Native plugin app.refreshAfterExternalSync failed: {error}"),
                );
            });
        }
    }

    fn emit_native_plugin_custom_event(
        &mut self,
        plugin_id: &str,
        args: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        match native_plugin_custom_event_from_args(plugin_id, args) {
            Ok((event_key, payload)) => {
                self.emit_native_plugin_event_to_subscribers(&event_key, payload, cx);
            }
            Err(error) => {
                self.plugin_entity.update(cx, |plugins, _cx| {
                    plugins.registry_mut().record_manager_error(
                        plugin_id.to_string(),
                        format!("Native plugin events.emit failed: {error}"),
                    );
                });
            }
        }
    }

    fn set_native_plugin_storage(
        &mut self,
        plugin_id: &str,
        args: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = args.get("key").and_then(serde_json::Value::as_str) else {
            self.plugin_entity.update(cx, |plugins, _cx| {
                plugins.registry_mut().record_manager_error(
                    plugin_id.to_string(),
                    "Native plugin storage.set requires args.key".to_string(),
                );
            });
            return;
        };
        let value = args
            .get("value")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let result = self.plugin_entity.update(cx, |plugins, _cx| {
            plugins
                .registry_mut()
                .set_plugin_storage_value(plugin_id, key, value)
        });
        if let Err(error) = result {
            self.plugin_entity.update(cx, |plugins, _cx| {
                plugins.registry_mut().record_manager_error(
                    plugin_id.to_string(),
                    format!("Native plugin storage.set failed: {error}"),
                );
            });
        }
    }

    fn remove_native_plugin_storage(
        &mut self,
        plugin_id: &str,
        args: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = args.get("key").and_then(serde_json::Value::as_str) else {
            self.plugin_entity.update(cx, |plugins, _cx| {
                plugins.registry_mut().record_manager_error(
                    plugin_id.to_string(),
                    "Native plugin storage.remove requires args.key".to_string(),
                );
            });
            return;
        };
        let result = self.plugin_entity.update(cx, |plugins, _cx| {
            plugins
                .registry_mut()
                .remove_plugin_storage_value(plugin_id, key)
        });
        if let Err(error) = result {
            self.plugin_entity.update(cx, |plugins, _cx| {
                plugins.registry_mut().record_manager_error(
                    plugin_id.to_string(),
                    format!("Native plugin storage.remove failed: {error}"),
                );
            });
        }
    }

    fn set_native_plugin_setting(
        &mut self,
        plugin_id: &str,
        args: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = args.get("key").and_then(serde_json::Value::as_str) else {
            self.plugin_entity.update(cx, |plugins, _cx| {
                plugins.registry_mut().record_manager_error(
                    plugin_id.to_string(),
                    "Native plugin settings.set requires args.key".to_string(),
                );
            });
            return;
        };
        let value = args
            .get("value")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        if let Err(error) = self.set_native_plugin_setting_value_and_emit(plugin_id, key, value, cx)
        {
            self.plugin_entity.update(cx, |plugins, _cx| {
                plugins.registry_mut().record_manager_error(
                    plugin_id.to_string(),
                    format!("Native plugin settings.set failed: {error}"),
                );
            });
        }
    }

    pub(super) fn set_native_plugin_setting_value_and_emit(
        &mut self,
        plugin_id: &str,
        key: &str,
        value: serde_json::Value,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let current_value = self.plugin_entity.update(cx, |plugins, _cx| {
            plugins
                .registry_mut()
                .set_plugin_setting_value(plugin_id, key, value)?;
            Ok::<serde_json::Value, String>(
                plugins
                    .registry()
                    .plugin_setting_value(plugin_id, key)
                    .unwrap_or(serde_json::Value::Null),
            )
        })?;
        self.emit_native_plugin_event_to_matching_subscribers(
            super::plugin_host::NATIVE_PLUGIN_SETTING_CHANGED_EVENT,
            Some(plugin_id),
            serde_json::json!({
                "pluginId": plugin_id,
                "key": key,
                "value": current_value,
            }),
            cx,
        );
        Ok(())
    }

    fn apply_native_plugin_syncable_settings(
        &mut self,
        plugin_id: &str,
        args: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        let payload = native_syncable_settings_payload_arg(args);
        let normalized = native_normalize_syncable_settings_payload(&payload);
        if let Err(error) = native_apply_syncable_settings_payload(self, &normalized.payload, cx) {
            self.plugin_entity.update(cx, |plugins, _cx| {
                plugins.registry_mut().record_manager_error(
                    plugin_id.to_string(),
                    format!("Native plugin settings.applySyncableSettings failed: {error}"),
                );
            });
        }
    }

    fn update_native_plugin_progress(
        &mut self,
        plugin_id: &str,
        registration_id: &str,
        value: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        let progress_key = native_plugin_progress_key(plugin_id, registration_id);
        if native_plugin_progress_is_done(&value) {
            self.apply_workspace_overlay_intent(
                WorkspaceOverlayIntent::DismissPluginProgress { key: progress_key },
                cx,
            );
            return;
        }

        let notice = native_plugin_progress_notice(plugin_id, registration_id, value);
        // Tauri plugin progress is host-owned and keyed by reporter id. Native
        // updates the same toast entry instead of appending one toast per event
        // burst, which keeps noisy process runtimes from flooding the overlay.
        self.apply_workspace_overlay_intent(
            WorkspaceOverlayIntent::PluginProgress {
                key: progress_key,
                notice,
                ttl: NATIVE_PLUGIN_TOAST_TTL,
            },
            cx,
        );
    }
}

fn native_plugin_toast_variant(variant: &str) -> TerminalNoticeVariant {
    match variant {
        "success" => TerminalNoticeVariant::Success,
        "error" => TerminalNoticeVariant::Error,
        "warning" => TerminalNoticeVariant::Warning,
        _ => TerminalNoticeVariant::Default,
    }
}

pub(in crate::workspace) fn native_plugin_permissions(
    manifest: &plugin_host::NativePluginManifest,
    trusted_process: bool,
) -> Result<plugin_runtime::PluginPermissionSet, plugin_runtime::PluginError> {
    let mut capabilities =
        plugin_host::normalize_native_plugin_capabilities(&manifest.permissions.capabilities)
            .map_err(|error| {
                plugin_runtime::PluginError::protocol("invalid_plugin_capability", error)
            })?;
    for capability in &capabilities {
        if !is_supported_host_api_capability(capability) {
            return Err(plugin_runtime::PluginError::protocol(
                "unsupported_plugin_capability",
                format!("Native plugin capability \"{capability}\" is not supported"),
            ));
        }
    }
    if trusted_process {
        // This capability records the user's trust decision; it does not grant
        // another host API because an unsandboxed process can access the OS directly.
        capabilities.push(plugin_host::NATIVE_PLUGIN_TRUSTED_PROCESS_CAPABILITY.to_string());
        capabilities.sort_unstable();
    }
    let allowed_host_apis = allowed_host_apis_for_capabilities(&capabilities);
    Ok(plugin_runtime::PluginPermissionSet {
        capabilities,
        allowed_host_apis,
    })
}

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
