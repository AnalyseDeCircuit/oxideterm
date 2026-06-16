// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    sync::mpsc,
    time::Duration,
};

use gpui::{AnyElement, Context, IntoElement, KeyDownEvent, ParentElement, Timer, Window, div};
use oxideterm_connections::{SavedConnectionsConflictStrategy, SavedConnectionsSyncSnapshot};
use oxideterm_gpui_terminal::{TerminalNotice, TerminalNoticeVariant};
use oxideterm_gpui_ui::{ConfirmDialogVariant, ConfirmDialogView, confirm_dialog_with_focus};
use oxideterm_sftp::BackgroundTransferState;
use serde_json::{Value, json};
use zeroize::Zeroizing;

use super::{
    TabKind, TelnetSessionConfig, TerminalInputInterceptor, TerminalOutputProcessor,
    TerminalSessionId, WorkspaceApp, WorkspaceToast, plugin_runtime,
    plugin_runtime::PluginResponseResult,
};

mod constants;
mod forwarding;
mod host_api_snapshot;
mod ide;
mod profiler;
mod secrets;
mod settings_payload;
mod sftp;
mod snapshots;
mod sync;
mod terminal_hooks;
mod terminal_queries;
mod types;
mod ui_helpers;
mod ui_host_calls;

use constants::*;
use forwarding::*;
use host_api_snapshot::*;
use ide::*;
use profiler::*;
use secrets::*;
use settings_payload::*;
use sftp::*;
use snapshots::*;
use sync::*;
use terminal_hooks::*;
use terminal_queries::*;
pub(super) use types::*;
pub(super) use ui_helpers::native_plugin_theme_snapshot;
use ui_helpers::*;
use ui_host_calls::*;

#[cfg(test)]
use oxideterm_plugin_host_api::terminal::NativePluginTerminalNodeSnapshot;
use oxideterm_plugin_host_api::{ai::*, transfers::*};

impl WorkspaceApp {
    pub(super) fn start_native_plugin_runtime_services_if_needed(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.native_plugin_runtime_services_started {
            return;
        }
        self.native_plugin_runtime_services_started = true;
        // Runtime request queues only need polling once a native process/WASM
        // plugin can issue host calls; keeping them cold avoids idle startup work.
        self.start_native_plugin_confirm_polling(cx);
        self.start_native_plugin_terminal_polling(cx);
        self.start_native_plugin_sync_polling(cx);
    }

    pub(super) fn start_native_plugin_confirm_polling(&mut self, cx: &mut Context<Self>) {
        if self.native_plugin_confirm_polling {
            return;
        }
        self.native_plugin_confirm_polling = true;
        cx.spawn(async move |weak, cx| {
            loop {
                Timer::after(NATIVE_PLUGIN_DELIVERY_POLL_INTERVAL).await;
                if weak
                    .update(cx, |this, cx| {
                        this.poll_native_plugin_confirm_requests(cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn poll_native_plugin_confirm_requests(&mut self, cx: &mut Context<Self>) {
        if self.native_plugin_confirm.is_some() {
            return;
        }

        match self.native_plugin_confirm_rx.try_recv() {
            Ok(request) => {
                // Tauri resolves ui.showConfirm from the window UI event bridge.
                // Native stores only the pending response channel here; plugin
                // code never runs in the render path.
                self.native_plugin_confirm = Some(request.into());
                self.reset_standard_confirm_focus();
                cx.notify();
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.native_plugin_confirm_polling = false;
            }
        }
    }

    fn respond_native_plugin_confirm(&mut self, confirmed: bool, cx: &mut Context<Self>) {
        if let Some(dialog) = self.native_plugin_confirm.take() {
            dialog.respond(confirmed);
        }
        self.clear_standard_confirm_focus();
        self.poll_native_plugin_confirm_requests(cx);
        cx.notify();
    }

    pub(super) fn start_native_plugin_terminal_polling(&mut self, cx: &mut Context<Self>) {
        if self.native_plugin_terminal_polling {
            return;
        }
        self.native_plugin_terminal_polling = true;
        cx.spawn(async move |weak, cx| {
            loop {
                Timer::after(NATIVE_PLUGIN_DELIVERY_POLL_INTERVAL).await;
                if weak
                    .update(cx, |this, cx| {
                        this.poll_native_plugin_terminal_requests(cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn poll_native_plugin_terminal_requests(&mut self, cx: &mut Context<Self>) {
        loop {
            match self.native_plugin_terminal_rx.try_recv() {
                Ok(request) => self.handle_native_plugin_terminal_request(request, cx),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.native_plugin_terminal_polling = false;
                    break;
                }
            }
        }
    }

    fn handle_native_plugin_terminal_request(
        &mut self,
        request: NativePluginTerminalRequest,
        cx: &mut Context<Self>,
    ) {
        if matches!(
            request.action,
            NativePluginTerminalAction::OpenTelnet { .. }
        ) {
            // Opening a terminal tab needs the GPUI Window; queue it for the
            // render pass instead of constructing a pane from the runtime task.
            self.native_plugin_terminal_ui_requests.push_back(request);
            cx.notify();
            return;
        }

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
            NativePluginTerminalAction::OpenTelnet { .. } => unreachable!(),
        };
        let _ = request.response_tx.send(response);
    }

    pub(super) fn poll_native_plugin_terminal_ui_requests(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        while let Some(request) = self.native_plugin_terminal_ui_requests.pop_front() {
            let response = match request.action {
                NativePluginTerminalAction::OpenTelnet { host, port } => self
                    .open_native_plugin_telnet_terminal(
                        &request.request_id,
                        host,
                        port,
                        window,
                        cx,
                    ),
                _ => plugin_runtime::PluginResponse::error(
                    request.request_id,
                    plugin_runtime::PluginError::protocol(
                        "invalid_terminal_ui_request",
                        "Native plugin terminal UI queue received a non-UI request",
                    ),
                ),
            };
            let _ = request.response_tx.send(response);
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
        match self.create_telnet_terminal_tab(config, window, cx) {
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

    pub(super) fn start_native_plugin_sync_polling(&mut self, cx: &mut Context<Self>) {
        if self.native_plugin_sync_polling {
            return;
        }
        self.native_plugin_sync_polling = true;
        cx.spawn(async move |weak, cx| {
            loop {
                Timer::after(NATIVE_PLUGIN_DELIVERY_POLL_INTERVAL).await;
                if weak
                    .update(cx, |this, cx| {
                        this.poll_native_plugin_sync_requests(cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn poll_native_plugin_sync_requests(&mut self, cx: &mut Context<Self>) {
        loop {
            match self.native_plugin_sync_rx.try_recv() {
                Ok(request) => self.handle_native_plugin_sync_request(request, cx),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.native_plugin_sync_polling = false;
                    break;
                }
            }
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
                self.update_native_plugin_progress(&plugin_id, registration_id, value);
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
            } => self.start_native_plugin_oxide_import(
                plugin_id,
                request.request_id,
                bytes,
                password,
                options,
                progress_registration_id,
                request.response_tx,
                cx,
            ),
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

    fn start_native_plugin_oxide_import(
        &mut self,
        plugin_id: String,
        request_id: String,
        bytes: Vec<u8>,
        password: Zeroizing<String>,
        options: NativePluginOxideImportOptions,
        progress_registration_id: Option<String>,
        response_tx: mpsc::Sender<plugin_runtime::PluginResponse>,
        cx: &mut Context<Self>,
    ) {
        let mut store = self.connection_store.clone();
        let oxide_options = options.oxide_options.clone();
        let (worker_tx, worker_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = native_plugin_apply_oxide_import_core_with_progress(
                &mut store,
                &bytes,
                &password,
                oxide_options,
                |stage, current, total| {
                    let _ = worker_tx.send(NativePluginOxideImportWorkerMessage::Progress {
                        stage: stage.to_string(),
                        current,
                        total,
                    });
                },
            )
            .map(|envelope| NativePluginOxideImportCoreResult { store, envelope });
            let _ = worker_tx.send(NativePluginOxideImportWorkerMessage::Done(result));
        });

        cx.spawn(async move |weak, cx| {
            loop {
                match worker_rx.try_recv() {
                    Ok(NativePluginOxideImportWorkerMessage::Progress {
                        stage,
                        current,
                        total,
                    }) => {
                        if let Some(registration_id) = progress_registration_id.as_ref() {
                            let value = native_plugin_sync_progress_value(
                                "Importing .oxide",
                                &stage,
                                current,
                                total,
                                false,
                            );
                            let _ = weak.update(cx, |this, _cx| {
                                this.update_native_plugin_progress(
                                    &plugin_id,
                                    registration_id.clone(),
                                    value,
                                );
                            });
                        }
                    }
                    Ok(NativePluginOxideImportWorkerMessage::Done(result)) => {
                        let _ = weak.update(cx, |this, cx| {
                            let response = this
                                .finish_native_plugin_oxide_import(request_id, result, options, cx);
                            if let Some(registration_id) = progress_registration_id {
                                this.update_native_plugin_progress(
                                    &plugin_id,
                                    registration_id,
                                    native_plugin_sync_progress_value(
                                        "Importing .oxide",
                                        "complete",
                                        1,
                                        1,
                                        true,
                                    ),
                                );
                            }
                            let _ = response_tx.send(response);
                            cx.notify();
                        });
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => {
                        Timer::after(Duration::from_millis(33)).await;
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        let _ = response_tx.send(plugin_runtime::PluginResponse::error(
                            request_id,
                            plugin_runtime::PluginError::runtime(
                                "plugin_sync_import_interrupted",
                                "Native plugin sync.importOxide worker stopped before completion",
                            ),
                        ));
                        break;
                    }
                }
            }
        })
        .detach();
    }

    fn finish_native_plugin_oxide_import(
        &mut self,
        request_id: String,
        result: Result<NativePluginOxideImportCoreResult, String>,
        options: NativePluginOxideImportOptions,
        cx: &mut Context<Self>,
    ) -> plugin_runtime::PluginResponse {
        let Ok(core) = result else {
            return plugin_runtime::PluginResponse::error(
                request_id,
                plugin_runtime::PluginError::runtime(
                    "plugin_sync_oxide_error",
                    result
                        .err()
                        .unwrap_or_else(|| "Unknown .oxide import error".to_string()),
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
        self.apply_oxide_import_portable_secrets(&mut envelope);
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
        let target = native_plugin_active_terminal_target(self, &connection_states);
        if target
            .get("connectionState")
            .and_then(Value::as_str)
            .is_some_and(|state| state != "active")
        {
            return false;
        }
        let Some(pane) = self.active_pane() else {
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
        let Some(pane) = native_plugin_pane_for_session(self, session_id) else {
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
        let Some(runtime) = self.node_runtime_store.snapshot(&node_id) else {
            return false;
        };
        if native_plugin_session_connection_state(&runtime.state, terminal_count) != "active" {
            return false;
        }
        let Some(session_id) = node.terminal_ids.first().copied() else {
            return false;
        };
        let Some(pane) = native_plugin_pane_for_session(self, session_id) else {
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
        let hooks = self
            .plugin_registry
            .contributions()
            .runtime_terminal_input_interceptors
            .clone();
        let interceptor = if hooks.is_empty() {
            None
        } else {
            let runtime_host = self.plugin_runtime_host.clone();
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

        for pane in self.panes.values() {
            pane.update(cx, |pane, _cx| {
                pane.set_plugin_input_interceptor(interceptor.clone());
            });
        }
    }

    fn refresh_native_plugin_terminal_output_processors(&mut self, cx: &mut Context<Self>) {
        let hooks = self
            .plugin_registry
            .contributions()
            .runtime_terminal_output_processors
            .clone();
        let processor = if hooks.is_empty() {
            None
        } else {
            let runtime_host = self.plugin_runtime_host.clone();
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

        for pane in self.panes.values() {
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
        if self.native_plugin_confirm.is_none() {
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
        let dialog = self.native_plugin_confirm.as_ref()?;
        Some(confirm_dialog_with_focus(
            &self.tokens,
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
        ))
    }

    pub(super) fn start_native_plugin_layout_polling(&mut self, cx: &mut Context<Self>) {
        if self.native_plugin_layout_polling {
            return;
        }
        self.native_plugin_layout_polling = true;
        cx.spawn(async move |weak, cx| {
            loop {
                Timer::after(NATIVE_PLUGIN_DELIVERY_POLL_INTERVAL).await;
                if weak
                    .update(cx, |this, cx| {
                        this.emit_native_plugin_layout_if_changed(cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    pub(super) fn native_plugin_layout_snapshot(&self) -> Value {
        native_plugin_layout_snapshot(
            self.sidebar_collapsed,
            self.active_tab_id.map(|tab_id| tab_id.0.to_string()),
            self.tabs.len(),
        )
    }

    pub(super) fn start_native_plugin_session_polling(&mut self, cx: &mut Context<Self>) {
        if self.native_plugin_session_polling {
            return;
        }
        self.native_plugin_session_polling = true;
        cx.spawn(async move |weak, cx| {
            loop {
                Timer::after(NATIVE_PLUGIN_DELIVERY_POLL_INTERVAL).await;
                if weak
                    .update(cx, |this, cx| {
                        this.emit_native_plugin_sessions_if_changed(cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    pub(super) fn native_plugin_session_tree_snapshot(&self) -> Value {
        json!(self.native_plugin_session_tree_snapshot_values())
    }

    pub(super) fn start_native_plugin_saved_forwards_polling(&mut self, cx: &mut Context<Self>) {
        if self.native_plugin_saved_forwards_polling {
            return;
        }
        self.native_plugin_saved_forwards_polling = true;
        cx.spawn(async move |weak, cx| {
            loop {
                Timer::after(NATIVE_PLUGIN_DELIVERY_POLL_INTERVAL).await;
                if weak
                    .update(cx, |this, cx| {
                        this.emit_native_plugin_saved_forwards_if_changed(cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    pub(super) fn native_plugin_saved_forwards_snapshot(&self) -> Value {
        native_plugin_forward_saved_forwards(&self.forwarding_registry)
            .unwrap_or_else(|_| json!([]))
    }

    pub(super) fn start_native_plugin_transfer_polling(&mut self, cx: &mut Context<Self>) {
        if self.native_plugin_transfer_polling {
            return;
        }
        self.native_plugin_transfer_polling = true;
        cx.spawn(async move |weak, cx| {
            loop {
                Timer::after(NATIVE_PLUGIN_DELIVERY_POLL_INTERVAL).await;
                if weak
                    .update(cx, |this, cx| {
                        this.emit_native_plugin_transfers_if_changed(cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    pub(super) fn native_plugin_transfer_snapshot(&self) -> Value {
        native_plugin_transfer_snapshot_array(&self.sftp_transfer_manager, None)
    }

    pub(super) fn start_native_plugin_profiler_polling(&mut self, cx: &mut Context<Self>) {
        if self.native_plugin_profiler_polling {
            return;
        }
        self.native_plugin_profiler_polling = true;
        cx.spawn(async move |weak, cx| {
            loop {
                Timer::after(NATIVE_PLUGIN_DELIVERY_POLL_INTERVAL).await;
                if weak
                    .update(cx, |this, cx| {
                        this.emit_native_plugin_profiler_if_changed(cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    pub(super) fn native_plugin_profiler_snapshot(&self) -> Value {
        native_plugin_profiler_snapshot_array(
            &self.connection_monitor.profiler_registry,
            &native_plugin_profiler_node_connection_ids(self),
        )
    }

    pub(super) fn start_native_plugin_ide_polling(&mut self, cx: &mut Context<Self>) {
        if self.native_plugin_ide_polling {
            return;
        }
        self.native_plugin_ide_polling = true;
        cx.spawn(async move |weak, cx| {
            loop {
                Timer::after(NATIVE_PLUGIN_DELIVERY_POLL_INTERVAL).await;
                if weak
                    .update(cx, |this, cx| {
                        this.emit_native_plugin_ide_if_changed(cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
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

    pub(super) fn start_native_plugin_ai_polling(&mut self, cx: &mut Context<Self>) {
        if self.native_plugin_ai_polling {
            return;
        }
        self.native_plugin_ai_polling = true;
        cx.spawn(async move |weak, cx| {
            loop {
                Timer::after(NATIVE_PLUGIN_DELIVERY_POLL_INTERVAL).await;
                if weak
                    .update(cx, |this, cx| {
                        this.emit_native_plugin_ai_if_changed(cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    pub(super) fn native_plugin_ai_snapshot(&self) -> Value {
        let settings = self.settings_store.settings();
        native_plugin_ai_snapshot_value(
            &self.ai_chat,
            &settings.ai.providers,
            settings.ai.active_provider_id.as_deref(),
            &settings.ai.model_context_windows,
        )
    }

    pub(super) fn start_native_plugin_event_log_polling(&mut self, cx: &mut Context<Self>) {
        if self.native_plugin_event_log_polling {
            return;
        }
        self.native_plugin_event_log_polling = true;
        cx.spawn(async move |weak, cx| {
            loop {
                Timer::after(NATIVE_PLUGIN_DELIVERY_POLL_INTERVAL).await;
                if weak
                    .update(cx, |this, cx| {
                        this.emit_native_plugin_event_log_entries(cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
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
        if self.has_native_plugin_subscription(
            super::plugin_host::NATIVE_PLUGIN_UI_LAYOUT_CHANGED_EVENT,
        ) {
            self.native_plugin_layout_snapshot = self.native_plugin_layout_snapshot();
            self.start_native_plugin_layout_polling(cx);
        }
        if self.has_native_plugin_subscription(
            super::plugin_host::NATIVE_PLUGIN_SESSION_TREE_CHANGED_EVENT,
        ) || self.has_native_plugin_subscription(
            super::plugin_host::NATIVE_PLUGIN_SESSION_NODE_STATE_CHANGED_EVENT,
        ) {
            self.native_plugin_session_tree_snapshot = self.native_plugin_session_tree_snapshot();
            self.start_native_plugin_session_polling(cx);
        }
        if self.has_native_plugin_subscription(
            super::plugin_host::NATIVE_PLUGIN_FORWARD_SAVED_FORWARDS_CHANGED_EVENT,
        ) {
            self.native_plugin_saved_forwards_snapshot =
                self.native_plugin_saved_forwards_snapshot();
            self.start_native_plugin_saved_forwards_polling(cx);
        }
        if self.has_native_plugin_subscription(
            super::plugin_host::NATIVE_PLUGIN_TRANSFER_PROGRESS_EVENT,
        ) || self.has_native_plugin_subscription(
            super::plugin_host::NATIVE_PLUGIN_TRANSFER_COMPLETE_EVENT,
        ) || self
            .has_native_plugin_subscription(super::plugin_host::NATIVE_PLUGIN_TRANSFER_ERROR_EVENT)
        {
            self.native_plugin_transfer_snapshot = self.native_plugin_transfer_snapshot();
            self.start_native_plugin_transfer_polling(cx);
        }
        if self.has_native_plugin_subscription(
            super::plugin_host::NATIVE_PLUGIN_PROFILER_METRICS_EVENT,
        ) {
            self.native_plugin_profiler_snapshot = self.native_plugin_profiler_snapshot();
            self.start_native_plugin_profiler_polling(cx);
        }
        if self
            .has_native_plugin_subscription(super::plugin_host::NATIVE_PLUGIN_IDE_FILE_OPEN_EVENT)
            || self.has_native_plugin_subscription(
                super::plugin_host::NATIVE_PLUGIN_IDE_FILE_CLOSE_EVENT,
            )
            || self.has_native_plugin_subscription(
                super::plugin_host::NATIVE_PLUGIN_IDE_ACTIVE_FILE_CHANGED_EVENT,
            )
        {
            self.native_plugin_ide_snapshot = self.native_plugin_ide_snapshot(cx);
            self.start_native_plugin_ide_polling(cx);
        }
        if self.has_native_plugin_subscription(super::plugin_host::NATIVE_PLUGIN_AI_MESSAGE_EVENT) {
            self.native_plugin_ai_snapshot = self.native_plugin_ai_snapshot();
            self.start_native_plugin_ai_polling(cx);
        }
        if self
            .has_native_plugin_subscription(super::plugin_host::NATIVE_PLUGIN_EVENT_LOG_ENTRY_EVENT)
        {
            self.native_plugin_event_log_last_id = self.native_plugin_last_event_log_id();
            self.start_native_plugin_event_log_polling(cx);
        }
    }

    fn has_native_plugin_subscription(&self, event_name: &str) -> bool {
        !self
            .plugin_registry
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
            self.node_runtime_store.export_snapshot().nodes,
            &titles,
            &terminal_ids,
        )
    }

    fn emit_native_plugin_layout_if_changed(&mut self, cx: &mut Context<Self>) {
        let layout = self.native_plugin_layout_snapshot();
        if layout == self.native_plugin_layout_snapshot {
            return;
        }

        self.native_plugin_layout_snapshot = layout.clone();
        let has_subscribers = !self
            .plugin_registry
            .contributions()
            .runtime_event_subscriptions_for(
                super::plugin_host::NATIVE_PLUGIN_UI_LAYOUT_CHANGED_EVENT,
            )
            .is_empty();
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
        if tree == self.native_plugin_session_tree_snapshot {
            return;
        }

        let previous_states =
            native_plugin_session_state_map(&self.native_plugin_session_tree_snapshot);
        let next_states = native_plugin_session_state_map(&tree);
        self.native_plugin_session_tree_snapshot = tree.clone();

        let has_tree_subscribers = !self
            .plugin_registry
            .contributions()
            .runtime_event_subscriptions_for(
                super::plugin_host::NATIVE_PLUGIN_SESSION_TREE_CHANGED_EVENT,
            )
            .is_empty();
        if has_tree_subscribers {
            // Tauri's onTreeChange callback receives the full frozen tree after
            // each Zustand nodes update. Native emits the same tree payload
            // over PluginEvent frames when the serialized projection changes.
            self.emit_native_plugin_event_to_subscribers(
                super::plugin_host::NATIVE_PLUGIN_SESSION_TREE_CHANGED_EVENT,
                tree.clone(),
                cx,
            );
        }

        let has_node_state_subscribers = !self
            .plugin_registry
            .contributions()
            .runtime_event_subscriptions_for(
                super::plugin_host::NATIVE_PLUGIN_SESSION_NODE_STATE_CHANGED_EVENT,
            )
            .is_empty();
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
        if saved_forwards == self.native_plugin_saved_forwards_snapshot {
            return;
        }
        self.native_plugin_saved_forwards_snapshot = saved_forwards.clone();

        let has_subscribers = !self
            .plugin_registry
            .contributions()
            .runtime_event_subscriptions_for(
                super::plugin_host::NATIVE_PLUGIN_FORWARD_SAVED_FORWARDS_CHANGED_EVENT,
            )
            .is_empty();
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
        let previous_states =
            native_plugin_transfer_state_map(&self.native_plugin_transfer_snapshot);
        let next_states = native_plugin_transfer_state_map(&transfers);
        let changed = transfers != self.native_plugin_transfer_snapshot;
        if changed {
            self.native_plugin_transfer_snapshot = transfers.clone();
        }

        let has_progress_subscribers = !self
            .plugin_registry
            .contributions()
            .runtime_event_subscriptions_for(
                super::plugin_host::NATIVE_PLUGIN_TRANSFER_PROGRESS_EVENT,
            )
            .is_empty();
        if has_progress_subscribers
            && native_plugin_transfer_progress_due(
                self.native_plugin_transfer_progress_last_emitted,
                NATIVE_PLUGIN_TRANSFER_PROGRESS_INTERVAL,
            )
        {
            // Tauri's transfer progress bridge is throttled to 500ms. Native keeps
            // the same throttle while polling the backend-owned SFTP transfer map.
            self.native_plugin_transfer_progress_last_emitted = Some(std::time::Instant::now());
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

        let has_complete_subscribers = !self
            .plugin_registry
            .contributions()
            .runtime_event_subscriptions_for(
                super::plugin_host::NATIVE_PLUGIN_TRANSFER_COMPLETE_EVENT,
            )
            .is_empty();
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

        let has_error_subscribers = !self
            .plugin_registry
            .contributions()
            .runtime_event_subscriptions_for(super::plugin_host::NATIVE_PLUGIN_TRANSFER_ERROR_EVENT)
            .is_empty();
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
        let metrics = self.native_plugin_profiler_snapshot();
        if metrics == self.native_plugin_profiler_snapshot {
            return;
        }
        let previous_timestamps =
            native_plugin_profiler_timestamp_map(&self.native_plugin_profiler_snapshot);
        let next_timestamps = native_plugin_profiler_timestamp_map(&metrics);
        self.native_plugin_profiler_snapshot = metrics.clone();

        let subscriptions = self
            .plugin_registry
            .contributions()
            .runtime_event_subscriptions_for(
                super::plugin_host::NATIVE_PLUGIN_PROFILER_METRICS_EVENT,
            );
        if subscriptions.is_empty() || !native_plugin_profiler_metrics_due(self) {
            return;
        }
        self.native_plugin_profiler_last_emitted = Some(std::time::Instant::now());

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
        if next == self.native_plugin_ide_snapshot {
            return;
        }
        let previous_files = native_plugin_ide_file_map(&self.native_plugin_ide_snapshot);
        let next_files = native_plugin_ide_file_map(&next);
        let previous_active = native_plugin_ide_active_file_path(&self.native_plugin_ide_snapshot);
        let next_active = native_plugin_ide_active_file_path(&next);
        self.native_plugin_ide_snapshot = next.clone();

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
        let next = self.native_plugin_ai_snapshot();
        if next == self.native_plugin_ai_snapshot {
            return;
        }
        let previous_counts = native_plugin_ai_message_count_map(&self.native_plugin_ai_snapshot);
        self.native_plugin_ai_snapshot = next.clone();

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
        let last_seen = self.native_plugin_event_log_last_id;
        let new_entries = self
            .notification_center
            .event_log
            .entries
            .iter()
            .filter(|entry| entry.id > last_seen)
            .cloned()
            .collect::<Vec<_>>();
        self.native_plugin_event_log_last_id = self.native_plugin_last_event_log_id();
        if new_entries.is_empty() {
            return;
        }

        let has_subscribers = !self
            .plugin_registry
            .contributions()
            .runtime_event_subscriptions_for(
                super::plugin_host::NATIVE_PLUGIN_EVENT_LOG_ENTRY_EVENT,
            )
            .is_empty();
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
        let process_plans = self.plugin_registry.process_activation_plans();
        let wasm_plans = self.plugin_registry.wasm_activation_plans();
        if process_plans.is_empty() && wasm_plans.is_empty() {
            return;
        }
        self.start_native_plugin_runtime_services_if_needed(cx);

        for plan in &process_plans {
            let _ = self.plugin_registry.mark_runtime_loading(&plan.plugin_id);
        }
        for plan in &wasm_plans {
            let _ = self.plugin_registry.mark_runtime_loading(&plan.plugin_id);
        }

        let (tx, rx) = mpsc::channel();
        let host = self.plugin_runtime_host.clone();
        let host_api_resolver = self.native_plugin_host_api_resolver(cx);
        self.forwarding_runtime.spawn(async move {
            let mut host = host.lock().await;
            host.set_host_api_resolver(host_api_resolver);
            // Tauri initializePluginSystem() loads enabled plugins sequentially.
            // Native keeps that ordering for process/WASM runtimes so
            // registration side effects are deterministic without executing JS
            // modules or WebViews.
            for plan in process_plans {
                let plugin_id = plan.plugin_id.clone();
                let result = host
                    .activate_process_plugin(
                        plan.manifest,
                        plan.install_dir,
                        plan.entry,
                        native_process_plugin_permissions(),
                        NATIVE_PLUGIN_LIFECYCLE_TIMEOUT,
                    )
                    .await;
                if tx
                    .send(NativePluginRuntimeDelivery::Activation { plugin_id, result })
                    .is_err()
                {
                    return;
                }
            }
            for plan in wasm_plans {
                let plugin_id = plan.plugin_id.clone();
                let result = host
                    .activate_wasm_plugin(
                        plan.manifest,
                        plan.install_dir,
                        plan.entry,
                        native_process_plugin_permissions(),
                        NATIVE_PLUGIN_LIFECYCLE_TIMEOUT,
                    )
                    .await;
                if tx
                    .send(NativePluginRuntimeDelivery::Activation { plugin_id, result })
                    .is_err()
                {
                    return;
                }
            }
            let _ = tx.send(NativePluginRuntimeDelivery::Finished);
        });

        cx.spawn(async move |weak, cx| {
            loop {
                Timer::after(NATIVE_PLUGIN_DELIVERY_POLL_INTERVAL).await;
                let mut finished = false;
                while let Ok(delivery) = rx.try_recv() {
                    if matches!(delivery, NativePluginRuntimeDelivery::Finished) {
                        finished = true;
                    }
                    if weak
                        .update(cx, |workspace, cx| {
                            workspace.handle_native_plugin_runtime_delivery(delivery, cx);
                        })
                        .is_err()
                    {
                        return;
                    }
                }
                if finished {
                    break;
                }
            }
        })
        .detach();
    }

    fn handle_native_plugin_runtime_delivery(
        &mut self,
        delivery: NativePluginRuntimeDelivery,
        cx: &mut Context<Self>,
    ) {
        match delivery {
            NativePluginRuntimeDelivery::Activation { plugin_id, result } => {
                self.handle_native_plugin_activation_result(plugin_id, result, cx);
            }
            NativePluginRuntimeDelivery::CommandDispatch { plugin_id, result } => {
                self.handle_native_plugin_command_dispatch_result(plugin_id, result, cx);
            }
            NativePluginRuntimeDelivery::EventDispatch { plugin_id, result } => {
                self.handle_native_plugin_event_dispatch_result(plugin_id, result, cx);
            }
            NativePluginRuntimeDelivery::Finished => {
                cx.notify();
            }
        }
    }

    fn handle_native_plugin_activation_result(
        &mut self,
        plugin_id: String,
        result: Result<plugin_runtime::NativePluginRuntimeActivation, plugin_runtime::PluginError>,
        cx: &mut Context<Self>,
    ) {
        let activation = match result {
            Ok(activation) => activation,
            Err(error) => {
                let _ = self
                    .plugin_registry
                    .mark_runtime_error(&plugin_id, error.message);
                cx.notify();
                return;
            }
        };

        if activation.plugin_id != plugin_id {
            let _ = self.plugin_registry.mark_runtime_error(
                &plugin_id,
                format!(
                    "Runtime activated plugin \"{}\" while loading \"{}\"",
                    activation.plugin_id, plugin_id
                ),
            );
            cx.notify();
            return;
        }

        for message in &activation.messages {
            if let Err(error) = self
                .plugin_registry
                .apply_runtime_outbound_message(&plugin_id, message)
            {
                self.plugin_registry
                    .cleanup_runtime_plugin_contributions(&plugin_id);
                let _ = self.plugin_registry.mark_runtime_error(&plugin_id, error);
                cx.notify();
                return;
            }
        }

        match &activation.response.result {
            PluginResponseResult::Ok { .. } => {
                let _ = self.plugin_registry.mark_runtime_active(&plugin_id);
            }
            PluginResponseResult::Error { error } => {
                self.plugin_registry
                    .cleanup_runtime_plugin_contributions(&plugin_id);
                let _ = self
                    .plugin_registry
                    .mark_runtime_error(&plugin_id, error.message.clone());
            }
        }

        for effect in activation.effects {
            self.handle_native_plugin_outbound_effect(&plugin_id, effect, cx);
        }
        self.refresh_native_plugin_event_polling(cx);
        self.refresh_native_plugin_terminal_hooks(cx);
        cx.notify();
    }

    pub(super) fn dispatch_native_plugin_command(
        &mut self,
        plugin_id: String,
        command: String,
        cx: &mut Context<Self>,
    ) {
        let host = self.plugin_runtime_host.clone();
        let host_api_resolver = self.native_plugin_host_api_resolver(cx);
        let (tx, rx) = mpsc::channel();
        self.forwarding_runtime.spawn({
            let plugin_id = plugin_id.clone();
            let command = command.clone();
            async move {
                let mut host = host.lock().await;
                host.set_host_api_resolver(host_api_resolver);
                let result = host
                    .dispatch_command(
                        &plugin_id,
                        command,
                        serde_json::Value::Null,
                        NATIVE_PLUGIN_LIFECYCLE_TIMEOUT,
                    )
                    .await;
                let _ = tx.send(NativePluginRuntimeDelivery::CommandDispatch { plugin_id, result });
                let _ = tx.send(NativePluginRuntimeDelivery::Finished);
            }
        });
        cx.spawn(async move |weak, cx| {
            loop {
                Timer::after(NATIVE_PLUGIN_DELIVERY_POLL_INTERVAL).await;
                let mut finished = false;
                while let Ok(delivery) = rx.try_recv() {
                    if matches!(delivery, NativePluginRuntimeDelivery::Finished) {
                        finished = true;
                    }
                    if weak
                        .update(cx, |workspace, cx| {
                            workspace.handle_native_plugin_runtime_delivery(delivery, cx);
                        })
                        .is_err()
                    {
                        return;
                    }
                }
                if finished {
                    break;
                }
            }
        })
        .detach();
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
            .plugin_registry
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

    fn handle_native_plugin_command_dispatch_result(
        &mut self,
        plugin_id: String,
        result: Result<
            plugin_runtime::NativePluginRuntimeCommandDispatch,
            plugin_runtime::PluginError,
        >,
        cx: &mut Context<Self>,
    ) {
        let dispatch = match result {
            Ok(dispatch) => dispatch,
            Err(error) => {
                self.plugin_registry.record_manager_error(
                    plugin_id,
                    format!("Native plugin command dispatch failed: {}", error.message),
                );
                cx.notify();
                return;
            }
        };

        for message in &dispatch.messages {
            if let Err(error) = self
                .plugin_registry
                .apply_runtime_outbound_message(&dispatch.plugin_id, message)
            {
                self.plugin_registry.record_manager_error(
                    dispatch.plugin_id.clone(),
                    format!("Native plugin command contribution update failed: {error}"),
                );
            }
        }
        if let PluginResponseResult::Error { error } = &dispatch.response.result {
            self.plugin_registry.record_manager_error(
                dispatch.plugin_id.clone(),
                format!(
                    "Native plugin command \"{}\" failed: {}",
                    dispatch.command, error.message
                ),
            );
        }
        for effect in dispatch.effects {
            self.handle_native_plugin_outbound_effect(&dispatch.plugin_id, effect, cx);
        }
        self.refresh_native_plugin_event_polling(cx);
        self.refresh_native_plugin_terminal_hooks(cx);
        cx.notify();
    }

    fn handle_native_plugin_event_dispatch_result(
        &mut self,
        plugin_id: String,
        result: Result<
            plugin_runtime::NativePluginRuntimeEventDispatch,
            plugin_runtime::PluginError,
        >,
        cx: &mut Context<Self>,
    ) {
        let dispatch = match result {
            Ok(dispatch) => dispatch,
            Err(error) => {
                self.plugin_registry.record_manager_error(
                    plugin_id,
                    format!("Native plugin event dispatch failed: {}", error.message),
                );
                cx.notify();
                return;
            }
        };

        for message in &dispatch.messages {
            if let Err(error) = self
                .plugin_registry
                .apply_runtime_outbound_message(&dispatch.plugin_id, message)
            {
                self.plugin_registry.record_manager_error(
                    dispatch.plugin_id.clone(),
                    format!("Native plugin event contribution update failed: {error}"),
                );
            }
        }
        if let PluginResponseResult::Error { error } = &dispatch.response.result {
            self.plugin_registry.record_manager_error(
                dispatch.plugin_id.clone(),
                format!(
                    "Native plugin event \"{}\" failed: {}",
                    dispatch.event.name, error.message
                ),
            );
        }
        for effect in dispatch.effects {
            self.handle_native_plugin_outbound_effect(&dispatch.plugin_id, effect, cx);
        }
        self.refresh_native_plugin_event_polling(cx);
        self.refresh_native_plugin_terminal_input_interceptors(cx);
        cx.notify();
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
            .plugin_registry
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
        let host = self.plugin_runtime_host.clone();
        let host_api_resolver = self.native_plugin_host_api_resolver(cx);
        let (tx, rx) = mpsc::channel();
        let event = plugin_runtime::PluginEvent {
            name: event_name.to_string(),
            payload,
        };
        self.forwarding_runtime.spawn({
            let plugin_id = plugin_id.clone();
            async move {
                let mut host = host.lock().await;
                host.set_host_api_resolver(host_api_resolver);
                let result = host
                    .dispatch_event(&plugin_id, event, NATIVE_PLUGIN_LIFECYCLE_TIMEOUT)
                    .await;
                let _ = tx.send(NativePluginRuntimeDelivery::EventDispatch { plugin_id, result });
                let _ = tx.send(NativePluginRuntimeDelivery::Finished);
            }
        });
        cx.spawn(async move |weak, cx| {
            loop {
                Timer::after(NATIVE_PLUGIN_DELIVERY_POLL_INTERVAL).await;
                let mut finished = false;
                while let Ok(delivery) = rx.try_recv() {
                    if matches!(delivery, NativePluginRuntimeDelivery::Finished) {
                        finished = true;
                    }
                    if weak
                        .update(cx, |workspace, cx| {
                            workspace.handle_native_plugin_runtime_delivery(delivery, cx);
                        })
                        .is_err()
                    {
                        return;
                    }
                }
                if finished {
                    break;
                }
            }
        })
        .detach();
    }

    fn native_plugin_host_api_resolver(
        &self,
        cx: &mut Context<Self>,
    ) -> plugin_runtime::NativeHostApiResolver {
        let snapshot = native_plugin_host_api_snapshot_from_workspace(self, cx);
        let confirm_tx = self.native_plugin_confirm_tx.clone();
        let terminal_tx = self.native_plugin_terminal_tx.clone();
        let sync_tx = self.native_plugin_sync_tx.clone();
        let sftp_router = self.node_router.clone();
        let sftp_runtime = self.forwarding_runtime.clone();
        let forwarding_registry = self.forwarding_registry.clone();
        let forwarding_runtime = self.forwarding_runtime.clone();
        let transfer_manager = self.sftp_transfer_manager.clone();
        let profiler_registry = self.connection_monitor.profiler_registry.clone();
        let profiler_node_connection_ids = native_plugin_profiler_node_connection_ids(self);
        let ide_snapshot = self.native_plugin_ide_snapshot(cx);
        let ai_snapshot = self.native_plugin_ai_snapshot();
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
            .forwarding_registry
            .export_saved_forwards_snapshot()
            .ok()
            .map(|snapshot| snapshot.revision);
        let sync_plugin_settings =
            super::plugin_settings_store::load_plugin_settings(self.settings_store.path())
                .unwrap_or_default();
        let sync_plugin_settings_revisions =
            native_plugin_settings_revision_map(&sync_plugin_settings);
        let plugin_secret_store = self.ai_key_store.clone();
        let telnet_transport_plugins = self
            .plugin_registry
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
                    Some(&sync_tx),
                ));
            }
            if call.namespace == "ui" && call.method == "showConfirm" {
                return Some(native_plugin_show_confirm_response(
                    &plugin_id,
                    call,
                    &confirm_tx,
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
                    Some(&sync_tx),
                ));
            }
            if call.namespace == "transfers" {
                return Some(native_plugin_transfers_response(call, &transfer_manager));
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
                return Some(native_plugin_terminal_response(call, &terminal_tx));
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
                return Some(native_plugin_terminal_response(call, &terminal_tx));
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
            } => self.update_native_plugin_progress(plugin_id, registration_id, value),
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
        match (namespace, method) {
            ("ui", "showToast") => self.push_native_plugin_toast(plugin_id, args),
            ("ui", "showNotification") => self.push_native_plugin_notification(plugin_id, args),
            ("ui", "registerTabView") => self.register_native_plugin_declarative_view(
                plugin_id,
                plugin_runtime::PluginRegistrationKind::Tab,
                args,
                cx,
            ),
            ("ui", "registerSidebarPanel") => self.register_native_plugin_declarative_view(
                plugin_id,
                plugin_runtime::PluginRegistrationKind::SidebarPanel,
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
            ("storage", "set") => self.set_native_plugin_storage(plugin_id, args),
            ("storage", "remove") => self.remove_native_plugin_storage(plugin_id, args),
            ("settings", "set") => self.set_native_plugin_setting(plugin_id, args, cx),
            ("settings", "applySyncableSettings") => {
                self.apply_native_plugin_syncable_settings(plugin_id, args, cx)
            }
            _ => self.plugin_registry.record_manager_error(
                plugin_id.to_string(),
                format!("Unsupported native plugin host call \"{namespace}.{method}\""),
            ),
        }
    }

    fn register_native_plugin_declarative_view(
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
                if let Err(error) = self
                    .plugin_registry
                    .apply_runtime_registration(registration)
                {
                    self.plugin_registry.record_manager_error(
                        plugin_id.to_string(),
                        format!("Native plugin declarative UI registration failed: {error}"),
                    );
                }
            }
            Err(error) => self.plugin_registry.record_manager_error(
                plugin_id.to_string(),
                format!("Native plugin declarative UI registration failed: {error}"),
            ),
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
            self.plugin_registry.record_manager_error(
                plugin_id.to_string(),
                "Native plugin ui.openTab requires args.tabId".to_string(),
            );
            return;
        };
        if let Err(error) = self.open_native_plugin_tab(plugin_id, &tab_id, cx) {
            self.plugin_registry.record_manager_error(
                plugin_id.to_string(),
                format!("Native plugin ui.openTab failed: {error}"),
            );
        }
    }

    fn push_native_plugin_toast(&mut self, plugin_id: &str, args: serde_json::Value) {
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

        let id = self.next_workspace_toast_id();
        self.workspace_toasts.push(WorkspaceToast {
            id,
            notice: TerminalNotice {
                title: native_plugin_notice_title(plugin_id, title),
                description,
                status_text: None,
                progress: None,
                variant,
            },
            expires_at: std::time::Instant::now() + NATIVE_PLUGIN_TOAST_TTL,
        });
    }

    fn push_native_plugin_notification(&mut self, plugin_id: &str, args: serde_json::Value) {
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

        let id = self.next_workspace_toast_id();
        self.workspace_toasts.push(WorkspaceToast {
            id,
            notice: TerminalNotice {
                title: native_plugin_notice_title(plugin_id, title),
                description,
                status_text: None,
                progress: None,
                variant,
            },
            expires_at: std::time::Instant::now() + NATIVE_PLUGIN_TOAST_TTL,
        });
    }

    fn refresh_native_after_external_sync(&mut self, plugin_id: &str, cx: &mut Context<Self>) {
        if let Err(error) = self.reload_after_external_sync(cx) {
            self.plugin_registry.record_manager_error(
                plugin_id.to_string(),
                format!("Native plugin app.refreshAfterExternalSync failed: {error}"),
            );
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
            Err(error) => self.plugin_registry.record_manager_error(
                plugin_id.to_string(),
                format!("Native plugin events.emit failed: {error}"),
            ),
        }
    }

    fn set_native_plugin_storage(&mut self, plugin_id: &str, args: serde_json::Value) {
        let Some(key) = args.get("key").and_then(serde_json::Value::as_str) else {
            self.plugin_registry.record_manager_error(
                plugin_id.to_string(),
                "Native plugin storage.set requires args.key".to_string(),
            );
            return;
        };
        let value = args
            .get("value")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        if let Err(error) = self
            .plugin_registry
            .set_plugin_storage_value(plugin_id, key, value)
        {
            self.plugin_registry.record_manager_error(
                plugin_id.to_string(),
                format!("Native plugin storage.set failed: {error}"),
            );
        }
    }

    fn remove_native_plugin_storage(&mut self, plugin_id: &str, args: serde_json::Value) {
        let Some(key) = args.get("key").and_then(serde_json::Value::as_str) else {
            self.plugin_registry.record_manager_error(
                plugin_id.to_string(),
                "Native plugin storage.remove requires args.key".to_string(),
            );
            return;
        };
        if let Err(error) = self
            .plugin_registry
            .remove_plugin_storage_value(plugin_id, key)
        {
            self.plugin_registry.record_manager_error(
                plugin_id.to_string(),
                format!("Native plugin storage.remove failed: {error}"),
            );
        }
    }

    fn set_native_plugin_setting(
        &mut self,
        plugin_id: &str,
        args: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = args.get("key").and_then(serde_json::Value::as_str) else {
            self.plugin_registry.record_manager_error(
                plugin_id.to_string(),
                "Native plugin settings.set requires args.key".to_string(),
            );
            return;
        };
        let value = args
            .get("value")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        if let Err(error) = self.set_native_plugin_setting_value_and_emit(plugin_id, key, value, cx)
        {
            self.plugin_registry.record_manager_error(
                plugin_id.to_string(),
                format!("Native plugin settings.set failed: {error}"),
            );
        }
    }

    pub(super) fn set_native_plugin_setting_value_and_emit(
        &mut self,
        plugin_id: &str,
        key: &str,
        value: serde_json::Value,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.plugin_registry
            .set_plugin_setting_value(plugin_id, key, value)?;
        self.emit_native_plugin_event_to_matching_subscribers(
            super::plugin_host::NATIVE_PLUGIN_SETTING_CHANGED_EVENT,
            Some(plugin_id),
            serde_json::json!({
                "pluginId": plugin_id,
                "key": key,
                "value": self
                    .plugin_registry
                    .plugin_setting_value(plugin_id, key)
                    .unwrap_or(serde_json::Value::Null),
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
            self.plugin_registry.record_manager_error(
                plugin_id.to_string(),
                format!("Native plugin settings.applySyncableSettings failed: {error}"),
            );
        }
    }

    fn update_native_plugin_progress(
        &mut self,
        plugin_id: &str,
        registration_id: String,
        value: serde_json::Value,
    ) {
        let progress_key = native_plugin_progress_key(plugin_id, &registration_id);
        if native_plugin_progress_is_done(&value) {
            self.plugin_progress_toasts.remove(&progress_key);
            return;
        }

        let notice = native_plugin_progress_notice(plugin_id, &registration_id, value);
        // Tauri plugin progress is host-owned and keyed by reporter id. Native
        // updates the same toast entry instead of appending one toast per event
        // burst, which keeps noisy process runtimes from flooding the overlay.
        let id = self.next_workspace_toast_id();
        self.plugin_progress_toasts.insert(
            progress_key,
            WorkspaceToast {
                id,
                notice,
                expires_at: std::time::Instant::now() + NATIVE_PLUGIN_TOAST_TTL,
            },
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

fn native_process_plugin_permissions() -> plugin_runtime::PluginPermissionSet {
    // Process plugins receive only host APIs that have native capability gates
    // or read-only snapshot boundaries. SFTP keeps an explicit read/write
    // split so future per-plugin permissions can deny mutating calls without
    // changing the transport schema.
    plugin_runtime::PluginPermissionSet {
        capabilities: vec![
            NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_READ.to_string(),
            NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_WRITE.to_string(),
            NATIVE_PLUGIN_CAPABILITY_NETWORK_FORWARD.to_string(),
        ],
        allowed_host_apis: vec![
            "app.getTheme".to_string(),
            "app.getSettings".to_string(),
            "app.getVersion".to_string(),
            "app.getPlatform".to_string(),
            "app.getLocale".to_string(),
            "app.getPoolStats".to_string(),
            "app.refreshAfterExternalSync".to_string(),
            "connections.getAll".to_string(),
            "connections.get".to_string(),
            "connections.getState".to_string(),
            "connections.getByNode".to_string(),
            "sessions.getTree".to_string(),
            "sessions.getActiveNodes".to_string(),
            "sessions.getNodeState".to_string(),
            "eventLog.getEntries".to_string(),
            "terminal.getActiveTarget".to_string(),
            "terminal.getNodeBuffer".to_string(),
            "terminal.getNodeSelection".to_string(),
            "terminal.search".to_string(),
            "terminal.getScrollBuffer".to_string(),
            "terminal.getBufferSize".to_string(),
            "terminal.writeToActive".to_string(),
            "terminal.writeToNode".to_string(),
            "terminal.clearBuffer".to_string(),
            "terminal.openTelnet".to_string(),
            "sftp.listDir".to_string(),
            "sftp.stat".to_string(),
            "sftp.readFile".to_string(),
            "sftp.writeFile".to_string(),
            "sftp.mkdir".to_string(),
            "sftp.delete".to_string(),
            "sftp.rename".to_string(),
            "forward.list".to_string(),
            "forward.listSavedForwards".to_string(),
            "forward.onSavedForwardsChange".to_string(),
            "forward.exportSavedForwardsSnapshot".to_string(),
            "forward.applySavedForwardsSnapshot".to_string(),
            "forward.create".to_string(),
            "forward.stop".to_string(),
            "forward.stopAll".to_string(),
            "forward.getStats".to_string(),
            "secrets.get".to_string(),
            "secrets.getMany".to_string(),
            "secrets.set".to_string(),
            "secrets.has".to_string(),
            "secrets.delete".to_string(),
            "sync.listSavedConnections".to_string(),
            "sync.refreshSavedConnections".to_string(),
            "sync.exportSavedConnectionsSnapshot".to_string(),
            "sync.applySavedConnectionsSnapshot".to_string(),
            "sync.getLocalSyncMetadata".to_string(),
            "sync.preflightExport".to_string(),
            "sync.exportOxide".to_string(),
            "sync.validateOxide".to_string(),
            "sync.previewImport".to_string(),
            "sync.importOxide".to_string(),
            "transfers.getAll".to_string(),
            "transfers.getByNode".to_string(),
            "transfers.onProgress".to_string(),
            "transfers.onComplete".to_string(),
            "transfers.onError".to_string(),
            "profiler.getMetrics".to_string(),
            "profiler.getHistory".to_string(),
            "profiler.isRunning".to_string(),
            "profiler.onMetrics".to_string(),
            "ide.isOpen".to_string(),
            "ide.getProject".to_string(),
            "ide.getOpenFiles".to_string(),
            "ide.getActiveFile".to_string(),
            "ide.onFileOpen".to_string(),
            "ide.onFileClose".to_string(),
            "ide.onActiveFileChange".to_string(),
            "ai.getConversations".to_string(),
            "ai.getMessages".to_string(),
            "ai.getActiveProvider".to_string(),
            "ai.getAvailableModels".to_string(),
            "ai.onMessage".to_string(),
            "api.invoke".to_string(),
            "events.emit".to_string(),
            "i18n.t".to_string(),
            "i18n.getLanguage".to_string(),
            "settings.get".to_string(),
            "settings.set".to_string(),
            "settings.exportSyncableSettings".to_string(),
            "settings.applySyncableSettings".to_string(),
            "ui.getLayout".to_string(),
            "ui.registerTabView".to_string(),
            "ui.registerSidebarPanel".to_string(),
            "ui.openTab".to_string(),
            "ui.showToast".to_string(),
            "ui.showConfirm".to_string(),
            "ui.showProgress".to_string(),
            "ui.showNotification".to_string(),
            "storage.set".to_string(),
            "storage.remove".to_string(),
            "storage.get".to_string(),
        ],
    }
}

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
