// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::{HashMap, HashSet},
    future::Future,
    pin::Pin,
    sync::Arc,
    sync::mpsc,
    time::Duration,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, SecondsFormat, Utc};
use gpui::{AnyElement, Context, IntoElement, KeyDownEvent, ParentElement, Timer, Window, div};
use oxideterm_connection_monitor::{ProfilerRegistry, ProfilerState, ResourceMetrics};
use oxideterm_connections::{
    LocalSyncMetadata as SavedConnectionsLocalSyncMetadata, SavedConnectionsConflictStrategy,
    SavedConnectionsSyncSnapshot,
    oxide_file::{
        ImportConflictStrategy, ImportResultEnvelope, OxideExportOptions, OxideFile,
        OxideImportOptions, apply_oxide_import_with_options_with_progress,
        export_connections_to_oxide, export_connections_to_oxide_with_progress, preflight_export,
        preview_oxide_import, preview_oxide_import_with_progress,
    },
};
use oxideterm_forwarding::{
    ForwardRule, ForwardStats, ForwardStatus, ForwardType, ForwardUpdate, ForwardingRegistry,
    SavedForwardsSyncSnapshot,
};
use oxideterm_gpui_ide::{IdePluginFileSnapshot, IdePluginSnapshot};
use oxideterm_gpui_terminal::{TerminalNotice, TerminalNoticeVariant};
use oxideterm_gpui_ui::{ConfirmDialogVariant, ConfirmDialogView, confirm_dialog_with_focus};
use oxideterm_i18n::I18n;
use oxideterm_notification_center::{EventCategory, EventLogEntry, EventSeverity};
use oxideterm_settings::{Language, UiDensity};
use oxideterm_sftp::{
    BackgroundTransferDirection, BackgroundTransferSnapshot, BackgroundTransferState, ListFilter,
    PreviewContent, SftpError, SftpSession, SftpTransferManager, encode_to_encoding,
    probe_tar_support, tar_download_directory, tar_upload_directory,
};
use oxideterm_ssh::{
    ConnectionConsumer, ConnectionInfo, ConnectionState, NodeId, NodeReadiness, NodeRouter,
    NodeTreeSnapshotNode,
};
use serde_json::{Map, Value, json};
use zeroize::Zeroizing;

use super::{
    TabKind, TelnetSessionConfig, TerminalInputInterceptor, TerminalInputInterceptorResult,
    TerminalOutputProcessor, TerminalSessionId, WorkspaceApp, WorkspaceToast, plugin_runtime,
    plugin_runtime::PluginResponseResult, quick_commands::QuickCommandImportStrategy,
};

const NATIVE_PLUGIN_LIFECYCLE_TIMEOUT: Duration = Duration::from_secs(5);
const NATIVE_PLUGIN_TERMINAL_HOOK_TIMEOUT: Duration = Duration::from_millis(5);
const NATIVE_PLUGIN_DELIVERY_POLL_INTERVAL: Duration = Duration::from_millis(80);
const NATIVE_PLUGIN_TRANSFER_PROGRESS_INTERVAL: Duration = Duration::from_millis(500);
const NATIVE_PLUGIN_PROFILER_METRICS_INTERVAL: Duration = Duration::from_secs(1);
const NATIVE_PLUGIN_TOAST_TTL: Duration = Duration::from_secs(4);
const NATIVE_PLUGIN_HTTP_BODY_LIMIT: usize = 10 * 1024 * 1024;
const NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_READ: &str = "filesystem.read";
const NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_WRITE: &str = "filesystem.write";
const NATIVE_PLUGIN_CAPABILITY_NETWORK_FORWARD: &str = "network.forward";
const NATIVE_PLUGIN_API_COMMAND_SSH_POOL_STATS: &str = "ssh_get_pool_stats";
const NATIVE_PLUGIN_API_COMMAND_LIST_CONNECTIONS: &str = "list_connections";
const NATIVE_PLUGIN_API_COMMAND_GET_APP_VERSION: &str = "get_app_version";
const NATIVE_PLUGIN_API_COMMAND_GET_SYSTEM_INFO: &str = "get_system_info";
const NATIVE_PLUGIN_API_COMMAND_SFTP_CANCEL_TRANSFER: &str = "sftp_cancel_transfer";
const NATIVE_PLUGIN_API_COMMAND_SFTP_PAUSE_TRANSFER: &str = "sftp_pause_transfer";
const NATIVE_PLUGIN_API_COMMAND_SFTP_RESUME_TRANSFER: &str = "sftp_resume_transfer";
const NATIVE_PLUGIN_API_COMMAND_SFTP_TRANSFER_STATS: &str = "sftp_transfer_stats";
const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_INIT: &str = "node_sftp_init";
const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_LIST_DIR: &str = "node_sftp_list_dir";
const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_STAT: &str = "node_sftp_stat";
const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_PREVIEW: &str = "node_sftp_preview";
const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_WRITE: &str = "node_sftp_write";
const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DOWNLOAD: &str = "node_sftp_download";
const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_UPLOAD: &str = "node_sftp_upload";
const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_MKDIR: &str = "node_sftp_mkdir";
const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DELETE: &str = "node_sftp_delete";
const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DELETE_RECURSIVE: &str = "node_sftp_delete_recursive";
const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_RENAME: &str = "node_sftp_rename";
const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DOWNLOAD_DIR: &str = "node_sftp_download_dir";
const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_UPLOAD_DIR: &str = "node_sftp_upload_dir";
const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_TAR_PROBE: &str = "node_sftp_tar_probe";
const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_TAR_UPLOAD: &str = "node_sftp_tar_upload";
const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_TAR_DOWNLOAD: &str = "node_sftp_tar_download";
const NATIVE_PLUGIN_API_COMMAND_LIST_PORT_FORWARDS: &str = "list_port_forwards";
const NATIVE_PLUGIN_API_COMMAND_CREATE_PORT_FORWARD: &str = "create_port_forward";
const NATIVE_PLUGIN_API_COMMAND_STOP_PORT_FORWARD: &str = "stop_port_forward";
const NATIVE_PLUGIN_API_COMMAND_DELETE_PORT_FORWARD: &str = "delete_port_forward";
const NATIVE_PLUGIN_API_COMMAND_RESTART_PORT_FORWARD: &str = "restart_port_forward";
const NATIVE_PLUGIN_API_COMMAND_UPDATE_PORT_FORWARD: &str = "update_port_forward";
const NATIVE_PLUGIN_API_COMMAND_GET_PORT_FORWARD_STATS: &str = "get_port_forward_stats";
const NATIVE_PLUGIN_API_COMMAND_STOP_ALL_FORWARDS: &str = "stop_all_forwards";
const NATIVE_PLUGIN_API_COMMAND_PLUGIN_HTTP_REQUEST: &str = "plugin_http_request";

// Keep the documented api.invoke adapter surface in one place so tests can
// detect a command that is listed but not dispatched through a native owner.
#[cfg(test)]
fn native_plugin_supported_backend_commands() -> &'static [&'static str] {
    &[
        NATIVE_PLUGIN_API_COMMAND_SSH_POOL_STATS,
        NATIVE_PLUGIN_API_COMMAND_LIST_CONNECTIONS,
        NATIVE_PLUGIN_API_COMMAND_GET_APP_VERSION,
        NATIVE_PLUGIN_API_COMMAND_GET_SYSTEM_INFO,
        NATIVE_PLUGIN_API_COMMAND_SFTP_CANCEL_TRANSFER,
        NATIVE_PLUGIN_API_COMMAND_SFTP_PAUSE_TRANSFER,
        NATIVE_PLUGIN_API_COMMAND_SFTP_RESUME_TRANSFER,
        NATIVE_PLUGIN_API_COMMAND_SFTP_TRANSFER_STATS,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_INIT,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_LIST_DIR,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_STAT,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_PREVIEW,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_WRITE,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DOWNLOAD,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_UPLOAD,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_MKDIR,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DELETE,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DELETE_RECURSIVE,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_RENAME,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DOWNLOAD_DIR,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_UPLOAD_DIR,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_TAR_PROBE,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_TAR_UPLOAD,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_TAR_DOWNLOAD,
        NATIVE_PLUGIN_API_COMMAND_LIST_PORT_FORWARDS,
        NATIVE_PLUGIN_API_COMMAND_CREATE_PORT_FORWARD,
        NATIVE_PLUGIN_API_COMMAND_STOP_PORT_FORWARD,
        NATIVE_PLUGIN_API_COMMAND_DELETE_PORT_FORWARD,
        NATIVE_PLUGIN_API_COMMAND_RESTART_PORT_FORWARD,
        NATIVE_PLUGIN_API_COMMAND_UPDATE_PORT_FORWARD,
        NATIVE_PLUGIN_API_COMMAND_GET_PORT_FORWARD_STATS,
        NATIVE_PLUGIN_API_COMMAND_STOP_ALL_FORWARDS,
        NATIVE_PLUGIN_API_COMMAND_PLUGIN_HTTP_REQUEST,
    ]
}

type NativePluginSharedSftp = Arc<tokio::sync::Mutex<SftpSession>>;
type NativePluginSftpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, SftpError>> + Send + 'a>>;

pub(super) enum NativePluginRuntimeDelivery {
    Activation {
        plugin_id: String,
        result: Result<plugin_runtime::NativePluginRuntimeActivation, plugin_runtime::PluginError>,
    },
    CommandDispatch {
        plugin_id: String,
        result:
            Result<plugin_runtime::NativePluginRuntimeCommandDispatch, plugin_runtime::PluginError>,
    },
    EventDispatch {
        plugin_id: String,
        result:
            Result<plugin_runtime::NativePluginRuntimeEventDispatch, plugin_runtime::PluginError>,
    },
    Finished,
}

pub(super) struct NativePluginConfirmRequest {
    plugin_id: String,
    request_id: String,
    title: String,
    description: String,
    response_tx: mpsc::Sender<bool>,
}

pub(super) struct NativePluginConfirmDialog {
    plugin_id: String,
    request_id: String,
    title: String,
    description: String,
    response_tx: mpsc::Sender<bool>,
}

pub(super) struct NativePluginTerminalRequest {
    request_id: String,
    action: NativePluginTerminalAction,
    response_tx: mpsc::Sender<plugin_runtime::PluginResponse>,
}

pub(super) enum NativePluginTerminalAction {
    WriteActive { text: String },
    WriteNode { node_id: String, text: String },
    ClearBuffer { node_id: String },
    OpenTelnet { host: String, port: u16 },
}

pub(super) struct NativePluginSyncRequest {
    request_id: String,
    action: NativePluginSyncAction,
    response_tx: mpsc::Sender<plugin_runtime::PluginResponse>,
}

enum NativePluginSyncAction {
    ApplySavedConnectionsSnapshot {
        snapshot: SavedConnectionsSyncSnapshot,
        conflict_strategy: SavedConnectionsConflictStrategy,
    },
    ReportProgress {
        plugin_id: String,
        registration_id: String,
        value: Value,
    },
    ImportOxide {
        bytes: Vec<u8>,
        password: Zeroizing<String>,
        options: NativePluginOxideImportOptions,
        progress_registration_id: Option<String>,
        plugin_id: String,
    },
}

#[derive(Clone, Debug)]
struct NativePluginOxideImportOptions {
    oxide_options: OxideImportOptions,
    import_app_settings: bool,
    selected_app_settings_sections: Option<HashSet<String>>,
    import_plugin_settings: bool,
    selected_plugin_ids: Option<HashSet<String>>,
    import_quick_commands: bool,
    quick_command_strategy: QuickCommandImportStrategy,
}

struct NativePluginOxideImportCoreResult {
    store: oxideterm_connections::ConnectionStore,
    envelope: ImportResultEnvelope,
}

enum NativePluginOxideImportWorkerMessage {
    Progress {
        stage: String,
        current: usize,
        total: usize,
    },
    Done(Result<NativePluginOxideImportCoreResult, String>),
}

impl From<NativePluginConfirmRequest> for NativePluginConfirmDialog {
    fn from(request: NativePluginConfirmRequest) -> Self {
        Self {
            plugin_id: request.plugin_id,
            request_id: request.request_id,
            title: request.title,
            description: request.description,
            response_tx: request.response_tx,
        }
    }
}

impl NativePluginConfirmDialog {
    fn respond(self, confirmed: bool) {
        let _request_id = self.request_id;
        let _ = self.response_tx.send(confirmed);
    }
}

impl WorkspaceApp {
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
                options.quick_command_strategy,
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
        if has_progress_subscribers && native_plugin_transfer_progress_due(self) {
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
        let snapshot = NativePluginHostApiSnapshot::from_workspace(self, cx);
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

        self.workspace_toasts.push(WorkspaceToast {
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

        self.workspace_toasts.push(WorkspaceToast {
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
        self.plugin_progress_toasts.insert(
            progress_key,
            WorkspaceToast {
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

#[derive(Clone)]
struct NativePluginHostApiSnapshot {
    registry: super::plugin_host::NativePluginRegistry,
    i18n: I18n,
    settings: Value,
    locale: String,
    theme_name: String,
    pool_stats: Value,
    layout: Value,
    connections: Vec<Value>,
    connection_states: HashMap<String, Value>,
    node_connection_ids: HashMap<String, String>,
    session_tree: Vec<Value>,
    session_node_states: HashMap<String, String>,
    event_log_entries: Vec<Value>,
    active_terminal_target: Value,
    terminal_nodes: HashMap<String, NativePluginTerminalNodeSnapshot>,
}

impl NativePluginHostApiSnapshot {
    fn from_workspace(workspace: &WorkspaceApp, cx: &mut Context<WorkspaceApp>) -> Self {
        let settings = workspace.settings_store.settings();
        let monitor_stats = workspace.ssh_registry.monitor_stats();
        let mut connection_infos = workspace.ssh_registry.list();
        connection_infos.sort_by(|left, right| left.connection_id.cmp(&right.connection_id));
        let connections = connection_infos
            .iter()
            .map(native_plugin_connection_snapshot)
            .collect::<Vec<_>>();
        let connection_states = connection_infos
            .iter()
            .map(|info| {
                (
                    info.connection_id.clone(),
                    native_plugin_connection_state(&info.state),
                )
            })
            .collect::<HashMap<_, _>>();
        let node_connection_ids = workspace
            .node_runtime_store
            .export_snapshot()
            .nodes
            .into_iter()
            .filter_map(|node| {
                node.connection_id
                    .map(|connection_id| (node.id.0, connection_id))
            })
            .collect::<HashMap<_, _>>();
        let session_tree = workspace.native_plugin_session_tree_snapshot_values();
        let session_node_states = native_plugin_session_state_map_from_nodes(&session_tree);
        let event_log_entries =
            native_plugin_event_log_entries(workspace.notification_center.event_log.entries.iter());
        let (active_terminal_target, terminal_nodes) =
            native_plugin_terminal_snapshots(workspace, &connection_states, cx);
        Self {
            registry: workspace.plugin_registry.clone(),
            i18n: workspace.i18n.clone(),
            settings: serde_json::to_value(settings).unwrap_or_else(|_| json!({})),
            locale: settings.general.language.as_str().to_string(),
            theme_name: settings.terminal.theme.clone(),
            // Tauri's PluginAppAPI exposes the compact ssh_get_pool_stats shape,
            // not the full native monitor payload. Keep this RPC-compatible.
            pool_stats: json!({
                "activeConnections": monitor_stats.active_connections,
                "totalSessions": monitor_stats.total_terminals,
            }),
            layout: workspace.native_plugin_layout_snapshot(),
            connections,
            connection_states,
            node_connection_ids,
            session_tree,
            session_node_states,
            event_log_entries,
            active_terminal_target,
            terminal_nodes,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct NativePluginTerminalNodeSnapshot {
    buffer: String,
    selection: Option<String>,
    current_lines: usize,
}

fn native_plugin_show_confirm_response(
    plugin_id: &str,
    call: plugin_runtime::PluginHostCall,
    confirm_tx: &mpsc::Sender<NativePluginConfirmRequest>,
) -> plugin_runtime::PluginResponse {
    let request_id = call.request_id.clone();
    let (title, description) = match native_plugin_confirm_args(&call.args) {
        Ok(args) => args,
        Err(error) => {
            return plugin_runtime::PluginResponse::error(
                request_id,
                plugin_runtime::PluginError::protocol("invalid_confirm_args", error),
            );
        }
    };
    let (response_tx, response_rx) = mpsc::channel();
    let request = NativePluginConfirmRequest {
        plugin_id: plugin_id.to_string(),
        request_id: request_id.clone(),
        title,
        description,
        response_tx,
    };
    if confirm_tx.send(request).is_err() {
        return plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime(
                "confirm_host_unavailable",
                "Native plugin ui.showConfirm cannot reach the workspace dialog host",
            ),
        );
    }

    // Match Tauri's Promise<boolean> semantics: the plugin request waits for
    // the user's protected native dialog choice instead of receiving a default.
    match response_rx.recv() {
        Ok(confirmed) => plugin_runtime::PluginResponse::ok(request_id, json!(confirmed)),
        Err(_) => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime(
                "confirm_response_unavailable",
                "Native plugin ui.showConfirm closed before the workspace answered",
            ),
        ),
    }
}

fn native_plugin_confirm_args(args: &Value) -> Result<(String, String), String> {
    let title = args
        .get("title")
        .and_then(Value::as_str)
        .filter(|title| !title.trim().is_empty())
        .ok_or_else(|| "ui.showConfirm requires args.title".to_string())?;
    let description = args
        .get("description")
        .and_then(Value::as_str)
        .ok_or_else(|| "ui.showConfirm requires args.description".to_string())?;
    Ok((title.to_string(), description.to_string()))
}

fn native_plugin_show_progress_response(
    plugin_id: &str,
    call: plugin_runtime::PluginHostCall,
    sync_tx: Option<&mpsc::Sender<NativePluginSyncRequest>>,
) -> plugin_runtime::PluginResponse {
    let request_id = call.request_id.clone();
    let title = call
        .args
        .get("title")
        .and_then(Value::as_str)
        .filter(|title| !title.trim().is_empty())
        .unwrap_or("Plugin progress");
    let registration_id = call
        .args
        .get("registrationId")
        .or_else(|| call.args.get("id"))
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    native_plugin_emit_sync_progress(
        sync_tx,
        plugin_id,
        &registration_id,
        json!({
            "title": title,
            "message": call.args.get("message").and_then(Value::as_str),
            "progress": 0.0,
            "done": false,
        }),
    );

    plugin_runtime::PluginResponse::ok(
        request_id,
        json!({
            "id": registration_id,
            "registrationId": registration_id,
        }),
    )
}

fn native_plugin_secret_response(
    plugin_id: &str,
    call: plugin_runtime::PluginHostCall,
    key_store: &oxideterm_ai::AiProviderKeyStore,
) -> plugin_runtime::PluginResponse {
    let request_id = call.request_id.clone();
    match native_plugin_secret_result(plugin_id, &call.method, &call.args, key_store) {
        Ok(value) => plugin_runtime::PluginResponse::ok(request_id, value),
        Err(error) => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime("plugin_secret_error", error),
        ),
    }
}

fn native_plugin_secret_result(
    plugin_id: &str,
    method: &str,
    args: &Value,
    key_store: &oxideterm_ai::AiProviderKeyStore,
) -> Result<Value, String> {
    match method {
        "get" => {
            let key = native_plugin_secret_key_arg(args)?;
            let account_id = native_plugin_secret_account_id(plugin_id, key)?;
            let secret = key_store
                .get_provider_key(&account_id)
                .map_err(|error| format!("Failed to read plugin secret: {error}"))?;
            Ok(secret
                .map(|secret| json!(secret.as_str()))
                .unwrap_or(Value::Null))
        }
        "getMany" => {
            let keys = native_plugin_secret_keys_arg(args)?;
            let mut account_ids = Vec::with_capacity(keys.len());
            for key in &keys {
                account_ids.push(native_plugin_secret_account_id(plugin_id, key)?);
            }
            let secrets = key_store
                .get_provider_keys(&account_ids)
                .map_err(|error| format!("Failed to read plugin secrets: {error}"))?;
            let secret_by_account = secrets
                .into_iter()
                .map(|(account_id, secret)| (account_id, secret))
                .collect::<HashMap<_, _>>();
            let mut values = Map::new();
            for (key, account_id) in keys.iter().zip(account_ids.iter()) {
                let value = secret_by_account
                    .get(account_id)
                    .map(|secret| json!(secret.as_str()))
                    .unwrap_or(Value::Null);
                values.insert(key.clone(), value);
            }
            Ok(Value::Object(values))
        }
        "set" => {
            let key = native_plugin_secret_key_arg(args)?;
            let value = args
                .get("value")
                .and_then(Value::as_str)
                .ok_or_else(|| "secrets.set requires args.value".to_string())?;
            let account_id = native_plugin_secret_account_id(plugin_id, key)?;
            // The JSON bridge gives us a borrowed string; wrap the owned copy at
            // the keychain boundary so the temporary is zeroized after storage,
            // matching Tauri's rule that plugin secrets live only in keychain
            // and the runtime response.
            key_store
                .store_provider_key(&account_id, Zeroizing::new(value.to_string()))
                .map_err(|error| {
                    if value.is_empty() {
                        format!("Failed to delete plugin secret: {error}")
                    } else {
                        format!("Failed to save plugin secret: {error}")
                    }
                })?;
            Ok(Value::Null)
        }
        "has" => {
            let key = native_plugin_secret_key_arg(args)?;
            let account_id = native_plugin_secret_account_id(plugin_id, key)?;
            Ok(json!(key_store.has_provider_key(&account_id)))
        }
        "delete" => {
            let key = native_plugin_secret_key_arg(args)?;
            let account_id = native_plugin_secret_account_id(plugin_id, key)?;
            key_store
                .delete_provider_key(&account_id)
                .map_err(|error| format!("Failed to delete plugin secret: {error}"))?;
            Ok(Value::Null)
        }
        method => Err(format!("Unsupported secrets host call: {method}")),
    }
}

fn native_plugin_secret_key_arg(args: &Value) -> Result<&str, String> {
    args.get("key")
        .and_then(Value::as_str)
        .ok_or_else(|| "secrets host call requires args.key".to_string())
}

fn native_plugin_secret_keys_arg(args: &Value) -> Result<Vec<String>, String> {
    let keys = args
        .get("keys")
        .and_then(Value::as_array)
        .ok_or_else(|| "secrets.getMany requires args.keys".to_string())?;
    keys.iter()
        .map(|key| {
            key.as_str()
                .map(str::to_string)
                .ok_or_else(|| "secrets.getMany keys must be strings".to_string())
        })
        .collect()
}

fn native_plugin_secret_account_id(plugin_id: &str, key: &str) -> Result<String, String> {
    native_plugin_validate_secret_plugin_id(plugin_id)?;
    native_plugin_validate_secret_key(key)?;
    Ok(format!(
        "plugin-secret:{}:{}:{}:{}",
        plugin_id.len(),
        plugin_id,
        key.len(),
        key
    ))
}

fn native_plugin_sftp_response(
    call: plugin_runtime::PluginHostCall,
    permissions: &plugin_runtime::PluginPermissionSet,
    router: &NodeRouter,
    runtime: &Arc<tokio::runtime::Runtime>,
    transfer_manager: Option<&Arc<SftpTransferManager>>,
) -> plugin_runtime::PluginResponse {
    let request_id = call.request_id.clone();
    if let Err(error) = native_plugin_sftp_check_capability(&call.method, permissions) {
        return plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol("plugin_sftp_capability_denied", error),
        );
    }
    let method = call.method.clone();
    let args = call.args.clone();
    let router = router.clone();
    let transfer_manager = transfer_manager.cloned();
    let (response_tx, response_rx) = mpsc::channel();

    // The stdio host-call hook is synchronous, while SFTP is owned by the
    // NodeRouter async runtime. Spawn the real protocol operation on that
    // backend runtime and block only this plugin host-call worker until it
    // finishes, preserving Tauri's Promise-returning ctx.sftp shape.
    runtime.spawn(async move {
        let result = native_plugin_sftp_result(&router, &method, &args, transfer_manager).await;
        let _ = response_tx.send(result);
    });

    match response_rx.recv() {
        Ok(Ok(value)) => plugin_runtime::PluginResponse::ok(request_id, value),
        Ok(Err(error)) => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime("plugin_sftp_error", error),
        ),
        Err(_) => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime(
                "plugin_sftp_unavailable",
                "Native plugin SFTP worker closed before returning a response",
            ),
        ),
    }
}

fn native_plugin_sftp_check_capability(
    method: &str,
    permissions: &plugin_runtime::PluginPermissionSet,
) -> Result<(), String> {
    let required = match method {
        "init" | "listDir" | "stat" | "readFile" | "preview" | "download" | "downloadDir"
        | "tarProbe" | "tarDownload" => NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_READ,
        "writeFile" | "write" | "upload" | "mkdir" | "delete" | "deleteRecursive" | "rename"
        | "uploadDir" | "tarUpload" => NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_WRITE,
        _ => return Ok(()),
    };
    if permissions
        .capabilities
        .iter()
        .any(|capability| capability == required)
    {
        return Ok(());
    }
    Err(format!(
        "Native plugin SFTP host call \"{method}\" requires capability \"{required}\""
    ))
}

async fn native_plugin_sftp_result(
    router: &NodeRouter,
    method: &str,
    args: &Value,
    transfer_manager: Option<Arc<SftpTransferManager>>,
) -> Result<Value, String> {
    match method {
        "init" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let cwd = native_plugin_with_sftp(router, &node_id, |sftp| {
                Box::pin(async move {
                    let sftp = sftp.lock().await;
                    Ok(sftp.cwd().to_string())
                })
            })
            .await?;
            Ok(json!(cwd))
        }
        "listDir" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let path = native_plugin_sftp_path_arg(args, "path")?;
            let filter = native_plugin_sftp_list_filter_arg(args)?;
            let entries = native_plugin_with_sftp_retry(router, &node_id, |sftp| {
                let path = path.clone();
                let filter = filter.clone();
                Box::pin(async move {
                    let sftp = sftp.lock().await;
                    sftp.list_dir(&path, filter).await
                })
            })
            .await?;
            Ok(json!(entries))
        }
        "stat" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let path = native_plugin_sftp_path_arg(args, "path")?;
            let info = native_plugin_with_sftp_retry(router, &node_id, |sftp| {
                let path = path.clone();
                Box::pin(async move {
                    let sftp = sftp.lock().await;
                    sftp.stat(&path).await
                })
            })
            .await?;
            Ok(json!(info))
        }
        "readFile" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let path = native_plugin_sftp_path_arg(args, "path")?;
            let preview = native_plugin_with_sftp_retry(router, &node_id, |sftp| {
                let path = path.clone();
                Box::pin(async move {
                    let sftp = sftp.lock().await;
                    sftp.preview(&path).await
                })
            })
            .await?;
            match preview {
                PreviewContent::Text { data, .. } => Ok(json!(data)),
                _ => Err("File is not a text file or exceeds size limit".to_string()),
            }
        }
        "preview" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let path = native_plugin_sftp_path_arg(args, "path")?;
            let preview = native_plugin_with_sftp_retry(router, &node_id, |sftp| {
                let path = path.clone();
                Box::pin(async move {
                    let sftp = sftp.lock().await;
                    sftp.preview(&path).await
                })
            })
            .await?;
            Ok(json!(preview))
        }
        "writeFile" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let path = native_plugin_sftp_path_arg(args, "path")?;
            let content = native_plugin_sftp_content_arg(args)?;
            native_plugin_with_sftp(router, &node_id, |sftp| {
                let path = path.clone();
                let content = content.clone();
                Box::pin(async move {
                    let sftp = sftp.lock().await;
                    sftp.write_content(&path, content.as_bytes())
                        .await
                        .map(|_| ())
                })
            })
            .await?;
            Ok(Value::Null)
        }
        "write" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let path = native_plugin_sftp_path_arg(args, "path")?;
            let content = native_plugin_sftp_content_arg(args)?;
            let encoding = args
                .get("encoding")
                .and_then(Value::as_str)
                .filter(|encoding| !encoding.is_empty())
                .unwrap_or("UTF-8")
                .to_string();
            let encoded_content = encode_to_encoding(&content, &encoding);
            let file_info = native_plugin_with_sftp(router, &node_id, |sftp| {
                let path = path.clone();
                let encoded_content = encoded_content.clone();
                Box::pin(async move {
                    let sftp = sftp.lock().await;
                    sftp.write_content(&path, &encoded_content).await?;
                    sftp.stat(&path).await
                })
            })
            .await?;
            Ok(json!({
                "mtime": (file_info.modified > 0).then_some(file_info.modified as u64),
                "size": Some(file_info.size),
                "encodingUsed": encoding,
                "atomicWrite": false,
            }))
        }
        "download" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let remote_path = native_plugin_sftp_path_arg(args, "remotePath")?;
            let local_path = native_plugin_sftp_local_path_arg(args, "localPath")?;
            let transfer_id = native_plugin_sftp_transfer_id_arg(args);
            let byte_count = native_plugin_with_sftp(router, &node_id, |sftp| {
                let remote_path = remote_path.clone();
                let local_path = local_path.clone();
                let transfer_id = transfer_id.clone();
                let transfer_manager = transfer_manager.clone();
                Box::pin(async move {
                    let sftp = sftp.lock().await;
                    sftp.download_file(
                        &remote_path,
                        &local_path,
                        &transfer_id,
                        None,
                        transfer_manager,
                    )
                    .await
                })
            })
            .await?;
            Ok(json!(byte_count))
        }
        "upload" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let local_path = native_plugin_sftp_local_path_arg(args, "localPath")?;
            let remote_path = native_plugin_sftp_path_arg(args, "remotePath")?;
            let transfer_id = native_plugin_sftp_transfer_id_arg(args);
            let byte_count = native_plugin_with_sftp(router, &node_id, |sftp| {
                let local_path = local_path.clone();
                let remote_path = remote_path.clone();
                let transfer_id = transfer_id.clone();
                let transfer_manager = transfer_manager.clone();
                Box::pin(async move {
                    let sftp = sftp.lock().await;
                    sftp.upload_file(
                        &local_path,
                        &remote_path,
                        &transfer_id,
                        None,
                        transfer_manager,
                    )
                    .await
                })
            })
            .await?;
            Ok(json!(byte_count))
        }
        "mkdir" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let path = native_plugin_sftp_path_arg(args, "path")?;
            native_plugin_with_sftp(router, &node_id, |sftp| {
                let path = path.clone();
                Box::pin(async move {
                    let sftp = sftp.lock().await;
                    sftp.mkdir(&path).await
                })
            })
            .await?;
            Ok(Value::Null)
        }
        "delete" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let path = native_plugin_sftp_path_arg(args, "path")?;
            native_plugin_with_sftp(router, &node_id, |sftp| {
                let path = path.clone();
                Box::pin(async move {
                    let sftp = sftp.lock().await;
                    sftp.delete(&path).await
                })
            })
            .await?;
            Ok(Value::Null)
        }
        "deleteRecursive" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let path = native_plugin_sftp_path_arg(args, "path")?;
            let deleted_count = native_plugin_with_sftp(router, &node_id, |sftp| {
                let path = path.clone();
                Box::pin(async move {
                    let sftp = sftp.lock().await;
                    sftp.delete_recursive(&path).await
                })
            })
            .await?;
            Ok(json!(deleted_count))
        }
        "downloadDir" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let remote_path = native_plugin_sftp_path_arg(args, "remotePath")?;
            let local_path = native_plugin_sftp_local_path_arg(args, "localPath")?;
            let transfer_id = native_plugin_sftp_transfer_id_arg(args);
            let item_count = native_plugin_with_sftp(router, &node_id, |sftp| {
                let remote_path = remote_path.clone();
                let local_path = local_path.clone();
                let transfer_id = transfer_id.clone();
                let transfer_manager = transfer_manager.clone();
                Box::pin(async move {
                    let sftp = sftp.lock().await;
                    sftp.download_dir(
                        &remote_path,
                        &local_path,
                        &transfer_id,
                        None,
                        transfer_manager,
                    )
                    .await
                })
            })
            .await?;
            Ok(json!(item_count))
        }
        "uploadDir" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let local_path = native_plugin_sftp_local_path_arg(args, "localPath")?;
            let remote_path = native_plugin_sftp_path_arg(args, "remotePath")?;
            let transfer_id = native_plugin_sftp_transfer_id_arg(args);
            let item_count = native_plugin_with_sftp(router, &node_id, |sftp| {
                let local_path = local_path.clone();
                let remote_path = remote_path.clone();
                let transfer_id = transfer_id.clone();
                let transfer_manager = transfer_manager.clone();
                Box::pin(async move {
                    let sftp = sftp.lock().await;
                    sftp.upload_dir(
                        &local_path,
                        &remote_path,
                        &transfer_id,
                        None,
                        transfer_manager,
                    )
                    .await
                })
            })
            .await?;
            Ok(json!(item_count))
        }
        "tarProbe" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let resolved = router
                .resolve_connection(&node_id)
                .await
                .map_err(native_plugin_route_error)?;
            Ok(json!(probe_tar_support(&resolved.handle).await))
        }
        "tarUpload" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let local_path = native_plugin_sftp_local_path_arg(args, "localPath")?;
            let remote_path = native_plugin_sftp_path_arg(args, "remotePath")?;
            let transfer_id = native_plugin_sftp_transfer_id_arg(args);
            let resolved = router
                .resolve_connection(&node_id)
                .await
                .map_err(native_plugin_route_error)?;
            let item_count = tar_upload_directory(
                &resolved.handle,
                &local_path,
                &remote_path,
                &transfer_id,
                None,
                transfer_manager,
                None,
            )
            .await
            .map_err(native_plugin_sftp_error)?;
            Ok(json!(item_count))
        }
        "tarDownload" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let remote_path = native_plugin_sftp_path_arg(args, "remotePath")?;
            let local_path = native_plugin_sftp_local_path_arg(args, "localPath")?;
            let transfer_id = native_plugin_sftp_transfer_id_arg(args);
            let resolved = router
                .resolve_connection(&node_id)
                .await
                .map_err(native_plugin_route_error)?;
            let item_count = tar_download_directory(
                &resolved.handle,
                &remote_path,
                &local_path,
                &transfer_id,
                None,
                transfer_manager,
                None,
            )
            .await
            .map_err(native_plugin_sftp_error)?;
            Ok(json!(item_count))
        }
        "rename" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let old_path = native_plugin_sftp_path_arg(args, "oldPath")?;
            let new_path = native_plugin_sftp_path_arg(args, "newPath")?;
            native_plugin_with_sftp(router, &node_id, |sftp| {
                let old_path = old_path.clone();
                let new_path = new_path.clone();
                Box::pin(async move {
                    let sftp = sftp.lock().await;
                    sftp.rename(&old_path, &new_path).await
                })
            })
            .await?;
            Ok(Value::Null)
        }
        method => Err(format!("Unsupported SFTP host call: {method}")),
    }
}

async fn native_plugin_with_sftp<T, F>(
    router: &NodeRouter,
    node_id: &NodeId,
    operation: F,
) -> Result<T, String>
where
    F: for<'a> Fn(&'a NativePluginSharedSftp) -> NativePluginSftpFuture<'a, T>,
{
    let sftp = router
        .acquire_sftp(node_id)
        .await
        .map_err(native_plugin_route_error)?;
    operation(&sftp).await.map_err(native_plugin_sftp_error)
}

async fn native_plugin_with_sftp_retry<T, F>(
    router: &NodeRouter,
    node_id: &NodeId,
    operation: F,
) -> Result<T, String>
where
    F: for<'a> Fn(&'a NativePluginSharedSftp) -> NativePluginSftpFuture<'a, T>,
{
    let sftp = router
        .acquire_sftp(node_id)
        .await
        .map_err(native_plugin_route_error)?;
    match operation(&sftp).await {
        Ok(value) => Ok(value),
        Err(error) if error.is_channel_recoverable() => {
            // Mirrors Tauri's read-only sftp_with_retry! behavior: stale
            // channels are invalidated at the NodeRouter owner and retried once
            // without tying SFTP lifetime to any terminal pane.
            let sftp = router
                .invalidate_and_reacquire_sftp(node_id)
                .await
                .map_err(native_plugin_route_error)?;
            operation(&sftp).await.map_err(native_plugin_sftp_error)
        }
        Err(error) => Err(native_plugin_sftp_error(error)),
    }
}

fn native_plugin_sftp_node_id_arg(args: &Value) -> Result<NodeId, String> {
    let node_id = args
        .get("nodeId")
        .and_then(Value::as_str)
        .filter(|node_id| !node_id.is_empty())
        .ok_or_else(|| "sftp host call requires args.nodeId".to_string())?;
    Ok(NodeId::new(node_id.to_string()))
}

fn native_plugin_sftp_path_arg(args: &Value, field: &str) -> Result<String, String> {
    let path = args
        .get(field)
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| format!("sftp host call requires args.{field}"))?;
    if path.contains('\0') {
        return Err(format!("sftp args.{field} contains an invalid NUL byte"));
    }
    Ok(path.to_string())
}

fn native_plugin_sftp_local_path_arg(args: &Value, field: &str) -> Result<String, String> {
    let path = native_plugin_sftp_path_arg(args, field)?;
    if std::path::Path::new(&path).is_absolute() {
        return Ok(path);
    }
    Err(format!("sftp args.{field} must be an absolute local path"))
}

fn native_plugin_sftp_transfer_id_arg(args: &Value) -> String {
    args.get("transferId")
        .or_else(|| args.get("transfer_id"))
        .and_then(Value::as_str)
        .filter(|transfer_id| !transfer_id.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
}

fn native_plugin_sftp_list_filter_arg(args: &Value) -> Result<Option<ListFilter>, String> {
    let Some(filter) = args.get("filter") else {
        return Ok(None);
    };
    if filter.is_null() {
        return Ok(None);
    }
    serde_json::from_value(filter.clone())
        .map(Some)
        .map_err(|error| {
            format!("sftp list_dir args.filter does not match the native ListFilter shape: {error}")
        })
}

fn native_plugin_sftp_content_arg(args: &Value) -> Result<String, String> {
    args.get("content")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "sftp.writeFile requires args.content".to_string())
}

fn native_plugin_sftp_error(error: SftpError) -> String {
    error.to_string()
}

fn native_plugin_route_error(error: oxideterm_ssh::RouteError) -> String {
    error.to_string()
}

fn native_plugin_forward_response(
    call: plugin_runtime::PluginHostCall,
    permissions: &plugin_runtime::PluginPermissionSet,
    registry: &ForwardingRegistry,
    runtime: &Arc<tokio::runtime::Runtime>,
    valid_owner_connection_ids: &std::collections::HashSet<String>,
) -> plugin_runtime::PluginResponse {
    let request_id = call.request_id.clone();
    if let Err(error) = native_plugin_forward_check_capability(&call.method, permissions) {
        return plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol("plugin_forward_capability_denied", error),
        );
    }

    match call.method.as_str() {
        "listSavedForwards" => {
            let value = match native_plugin_forward_saved_forwards(registry) {
                Ok(value) => value,
                Err(error) => {
                    return plugin_runtime::PluginResponse::error(
                        request_id,
                        plugin_runtime::PluginError::runtime("plugin_forward_error", error),
                    );
                }
            };
            return plugin_runtime::PluginResponse::ok(request_id, value);
        }
        "exportSavedForwardsSnapshot" => {
            let value = match registry.export_saved_forwards_snapshot() {
                Ok(snapshot) => json!(snapshot),
                Err(error) => {
                    return plugin_runtime::PluginResponse::error(
                        request_id,
                        plugin_runtime::PluginError::runtime(
                            "plugin_forward_error",
                            error.to_string(),
                        ),
                    );
                }
            };
            return plugin_runtime::PluginResponse::ok(request_id, value);
        }
        "applySavedForwardsSnapshot" => {
            let snapshot =
                match native_plugin_forward_snapshot_arg::<SavedForwardsSyncSnapshot>(&call.args) {
                    Ok(snapshot) => snapshot,
                    Err(error) => {
                        return plugin_runtime::PluginResponse::error(
                            request_id,
                            plugin_runtime::PluginError::protocol(
                                "invalid_forward_snapshot",
                                error,
                            ),
                        );
                    }
                };
            let value = match registry
                .apply_saved_forwards_snapshot(snapshot, valid_owner_connection_ids)
            {
                Ok(result) => json!(result),
                Err(error) => {
                    return plugin_runtime::PluginResponse::error(
                        request_id,
                        plugin_runtime::PluginError::runtime(
                            "plugin_forward_error",
                            error.to_string(),
                        ),
                    );
                }
            };
            return plugin_runtime::PluginResponse::ok(request_id, value);
        }
        _ => {}
    }

    let method = call.method.clone();
    let args = call.args.clone();
    let registry = registry.clone();
    let (response_tx, response_rx) = mpsc::channel();
    // Forward listener creation and teardown can await SSH channel operations.
    // Keep those operations on the long-lived forwarding runtime that owns the
    // registry managers instead of the plugin stdio reader.
    runtime.spawn(async move {
        let result = native_plugin_forward_async_result(&registry, &method, &args).await;
        let _ = response_tx.send(result);
    });

    match response_rx.recv() {
        Ok(Ok(value)) => plugin_runtime::PluginResponse::ok(request_id, value),
        Ok(Err(error)) => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime("plugin_forward_error", error),
        ),
        Err(_) => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime(
                "plugin_forward_unavailable",
                "Native plugin forwarding worker closed before returning a response",
            ),
        ),
    }
}

async fn native_plugin_forward_async_result(
    registry: &ForwardingRegistry,
    method: &str,
    args: &Value,
) -> Result<Value, String> {
    match method {
        "list" => {
            let session_id = native_plugin_forward_session_id_arg(args)?;
            let manager = native_plugin_forward_manager(registry, &session_id)?;
            Ok(json!(
                manager
                    .list_forwards()
                    .into_iter()
                    .map(native_plugin_forward_rule_snapshot)
                    .collect::<Vec<_>>()
            ))
        }
        "create" => {
            let request = native_plugin_forward_create_request(args)?;
            let manager = native_plugin_forward_manager(registry, &request.session_id)?;
            let rule = native_plugin_forward_rule_from_request(&request);
            let response = match manager.create_forward(rule).await {
                Ok(rule) => json!({
                    "success": true,
                    "forward": native_plugin_forward_rule_snapshot(rule),
                    "error": Value::Null,
                }),
                Err(error) => json!({
                    "success": false,
                    "forward": Value::Null,
                    "error": error.to_string(),
                }),
            };
            Ok(response)
        }
        "stop" => {
            let session_id = native_plugin_forward_session_id_arg(args)?;
            let forward_id = native_plugin_forward_id_arg(args)?;
            let manager = native_plugin_forward_manager(registry, &session_id)?;
            Ok(match manager.stop_forward(&forward_id).await {
                Ok(_) => native_plugin_forward_response_value(true, Value::Null, None),
                Err(error) => native_plugin_forward_response_value(
                    false,
                    Value::Null,
                    Some(error.to_string()),
                ),
            })
        }
        "delete" => {
            let session_id = native_plugin_forward_session_id_arg(args)?;
            let forward_id = native_plugin_forward_id_arg(args)?;
            let manager = native_plugin_forward_manager(registry, &session_id)?;
            Ok(match manager.delete_forward(&forward_id).await {
                Ok(_) => native_plugin_forward_response_value(true, Value::Null, None),
                Err(error) => native_plugin_forward_response_value(
                    false,
                    Value::Null,
                    Some(error.to_string()),
                ),
            })
        }
        "restart" => {
            let session_id = native_plugin_forward_session_id_arg(args)?;
            let forward_id = native_plugin_forward_id_arg(args)?;
            let manager = native_plugin_forward_manager(registry, &session_id)?;
            Ok(match manager.restart_forward(&forward_id).await {
                Ok(rule) => native_plugin_forward_response_value(
                    true,
                    native_plugin_forward_rule_snapshot(rule),
                    None,
                ),
                Err(error) => native_plugin_forward_response_value(
                    false,
                    Value::Null,
                    Some(error.to_string()),
                ),
            })
        }
        "update" => {
            let session_id = native_plugin_forward_session_id_arg(args)?;
            let forward_id = native_plugin_forward_id_arg(args)?;
            let update = native_plugin_forward_update_arg(args)?;
            let manager = native_plugin_forward_manager(registry, &session_id)?;
            Ok(match manager.update_forward(&forward_id, update) {
                Ok(rule) => native_plugin_forward_response_value(
                    true,
                    native_plugin_forward_rule_snapshot(rule),
                    None,
                ),
                Err(error) => native_plugin_forward_response_value(
                    false,
                    Value::Null,
                    Some(error.to_string()),
                ),
            })
        }
        "stopAll" => {
            let session_id = native_plugin_forward_session_id_arg(args)?;
            if let Some(manager) = registry.get(&session_id) {
                manager.stop_all().await;
            }
            Ok(Value::Null)
        }
        "getStats" => {
            let session_id = native_plugin_forward_session_id_arg(args)?;
            let forward_id = native_plugin_forward_id_arg(args)?;
            let manager = native_plugin_forward_manager(registry, &session_id)?;
            match manager.get_stats(&forward_id) {
                Ok(stats) => Ok(native_plugin_forward_stats_snapshot(stats)),
                Err(oxideterm_forwarding::ForwardingError::NotFound(_)) => Ok(Value::Null),
                Err(error) => Err(error.to_string()),
            }
        }
        "onSavedForwardsChange" => Err(
            "forward.onSavedForwardsChange is registered through the native event subscription bridge, not as a direct host call"
                .to_string(),
        ),
        method => Err(format!("Unsupported forward host call: {method}")),
    }
}

fn native_plugin_forward_check_capability(
    method: &str,
    permissions: &plugin_runtime::PluginPermissionSet,
) -> Result<(), String> {
    let requires_forward = matches!(
        method,
        "create"
            | "stop"
            | "delete"
            | "restart"
            | "update"
            | "stopAll"
            | "list"
            | "getStats"
            | "listSavedForwards"
            | "onSavedForwardsChange"
            | "exportSavedForwardsSnapshot"
            | "applySavedForwardsSnapshot"
    );
    if !requires_forward {
        return Ok(());
    }
    if permissions
        .capabilities
        .iter()
        .any(|capability| capability == NATIVE_PLUGIN_CAPABILITY_NETWORK_FORWARD)
    {
        return Ok(());
    }
    Err(format!(
        "Native plugin forward host call \"{method}\" requires capability \"{NATIVE_PLUGIN_CAPABILITY_NETWORK_FORWARD}\""
    ))
}

fn native_plugin_forward_manager(
    registry: &ForwardingRegistry,
    session_id: &str,
) -> Result<Arc<oxideterm_forwarding::ForwardingManager>, String> {
    registry
        .get(session_id)
        .ok_or_else(|| format!("Session not found: {session_id}"))
}

fn native_plugin_forward_saved_forwards(registry: &ForwardingRegistry) -> Result<Value, String> {
    let snapshot = registry
        .export_saved_forwards_snapshot()
        .map_err(|error| error.to_string())?;
    let forwards = snapshot
        .records
        .into_iter()
        .filter(|record| !record.deleted)
        .filter_map(|record| record.payload)
        .map(|payload| json!(payload))
        .collect::<Vec<_>>();
    Ok(json!(forwards))
}

fn native_plugin_forward_snapshot_arg<T>(args: &Value) -> Result<T, String>
where
    T: serde::de::DeserializeOwned,
{
    let value = args
        .get("snapshot")
        .cloned()
        .unwrap_or_else(|| args.clone());
    serde_json::from_value(value).map_err(|error| error.to_string())
}

#[derive(Clone)]
struct NativePluginForwardCreateRequest {
    session_id: String,
    forward_type: ForwardType,
    bind_address: String,
    bind_port: u16,
    target_host: String,
    target_port: u16,
    description: String,
}

fn native_plugin_forward_create_request(
    args: &Value,
) -> Result<NativePluginForwardCreateRequest, String> {
    let request = args.get("request").unwrap_or(args);
    let session_id = native_plugin_required_string(request, "sessionId")
        .or_else(|_| native_plugin_required_string(request, "session_id"))?;
    let forward_type = native_plugin_forward_type_arg(request)?;
    let bind_address = native_plugin_required_string(request, "bindAddress")
        .or_else(|_| native_plugin_required_string(request, "bind_address"))?;
    let bind_port = native_plugin_port_arg(request, "bindPort")
        .or_else(|_| native_plugin_port_arg(request, "bind_port"))?;
    let target_host = native_plugin_required_string(request, "targetHost")
        .or_else(|_| native_plugin_required_string(request, "target_host"))
        .unwrap_or_default();
    let target_port = native_plugin_port_arg(request, "targetPort")
        .or_else(|_| native_plugin_port_arg(request, "target_port"))
        .unwrap_or_default();
    let description = request
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    Ok(NativePluginForwardCreateRequest {
        session_id,
        forward_type,
        bind_address,
        bind_port,
        target_host,
        target_port,
        description,
    })
}

fn native_plugin_forward_rule_from_request(
    request: &NativePluginForwardCreateRequest,
) -> ForwardRule {
    let mut rule = match request.forward_type {
        ForwardType::Local => ForwardRule::local(
            request.bind_address.clone(),
            request.bind_port,
            request.target_host.clone(),
            request.target_port,
        ),
        ForwardType::Remote => ForwardRule::remote(
            request.bind_address.clone(),
            request.bind_port,
            request.target_host.clone(),
            request.target_port,
        ),
        ForwardType::Dynamic => {
            ForwardRule::dynamic(request.bind_address.clone(), request.bind_port)
        }
    };
    rule.description = request.description.clone();
    rule
}

fn native_plugin_forward_update_arg(args: &Value) -> Result<ForwardUpdate, String> {
    let request = args.get("request").unwrap_or(args);
    Ok(ForwardUpdate {
        forward_type: request
            .get("forwardType")
            .or_else(|| request.get("forward_type"))
            .and_then(Value::as_str)
            .map(native_plugin_forward_type_from_label)
            .transpose()?,
        bind_address: native_plugin_optional_string_arg(request, "bindAddress")
            .or_else(|| native_plugin_optional_string_arg(request, "bind_address")),
        bind_port: native_plugin_optional_port_arg(request, "bindPort")
            .or_else(|| native_plugin_optional_port_arg(request, "bind_port")),
        target_host: native_plugin_optional_string_arg(request, "targetHost")
            .or_else(|| native_plugin_optional_string_arg(request, "target_host")),
        target_port: native_plugin_optional_port_arg(request, "targetPort")
            .or_else(|| native_plugin_optional_port_arg(request, "target_port")),
        description: native_plugin_optional_string_arg(request, "description"),
    })
}

fn native_plugin_forward_response_value(
    success: bool,
    forward: Value,
    error: Option<String>,
) -> Value {
    json!({
        "success": success,
        "forward": forward,
        "error": error,
    })
}

fn native_plugin_forward_session_id_arg(args: &Value) -> Result<String, String> {
    native_plugin_required_string(args, "sessionId")
        .or_else(|_| native_plugin_required_string(args, "session_id"))
}

fn native_plugin_forward_id_arg(args: &Value) -> Result<String, String> {
    native_plugin_required_string(args, "forwardId")
        .or_else(|_| native_plugin_required_string(args, "forward_id"))
}

fn native_plugin_forward_type_arg(args: &Value) -> Result<ForwardType, String> {
    let value = native_plugin_required_string(args, "forwardType")
        .or_else(|_| native_plugin_required_string(args, "forward_type"))?;
    native_plugin_forward_type_from_label(&value)
}

fn native_plugin_forward_type_from_label(value: &str) -> Result<ForwardType, String> {
    match value {
        "local" => Ok(ForwardType::Local),
        "remote" => Ok(ForwardType::Remote),
        "dynamic" => Ok(ForwardType::Dynamic),
        _ => Err(format!("Invalid forward type: {value}")),
    }
}

fn native_plugin_port_arg(args: &Value, field: &str) -> Result<u16, String> {
    let port = args
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("forward host call requires args.{field}"))?;
    u16::try_from(port).map_err(|_| format!("forward args.{field} is outside u16 range"))
}

fn native_plugin_optional_port_arg(args: &Value, field: &str) -> Option<u16> {
    args.get(field)
        .and_then(Value::as_u64)
        .and_then(|port| u16::try_from(port).ok())
}

fn native_plugin_required_string(args: &Value, field: &str) -> Result<String, String> {
    args.get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("forward host call requires args.{field}"))
}

fn native_plugin_forward_rule_snapshot(rule: ForwardRule) -> Value {
    json!({
        "id": rule.id,
        "forward_type": native_plugin_forward_type_label(rule.forward_type),
        "bind_address": rule.bind_address,
        "bind_port": rule.bind_port,
        "target_host": rule.target_host,
        "target_port": rule.target_port,
        "status": native_plugin_forward_status_label(&rule.status),
        "description": if rule.description.is_empty() { Value::Null } else { json!(rule.description) },
    })
}

fn native_plugin_forward_stats_snapshot(stats: ForwardStats) -> Value {
    json!({
        "connectionCount": stats.connection_count,
        "activeConnections": stats.active_connections,
        "bytesSent": stats.bytes_sent,
        "bytesReceived": stats.bytes_received,
    })
}

fn native_plugin_forward_type_label(forward_type: ForwardType) -> &'static str {
    match forward_type {
        ForwardType::Local => "local",
        ForwardType::Remote => "remote",
        ForwardType::Dynamic => "dynamic",
    }
}

fn native_plugin_forward_status_label(status: &ForwardStatus) -> &'static str {
    match status {
        ForwardStatus::Starting => "starting",
        ForwardStatus::Active => "active",
        ForwardStatus::Stopped => "stopped",
        ForwardStatus::Error => "error",
        ForwardStatus::Suspended => "suspended",
    }
}

fn native_plugin_sync_response(
    plugin_id: &str,
    call: plugin_runtime::PluginHostCall,
    connection_store: &oxideterm_connections::ConnectionStore,
    saved_connections: &Value,
    saved_connections_snapshot: Result<&SavedConnectionsSyncSnapshot, &anyhow::Error>,
    local_metadata: Result<&SavedConnectionsLocalSyncMetadata, &anyhow::Error>,
    saved_forwards_revision: Option<&str>,
    plugin_settings: &[oxideterm_connections::oxide_file::EncryptedPluginSetting],
    plugin_settings_revisions: &Map<String, Value>,
    sync_tx: Option<&mpsc::Sender<NativePluginSyncRequest>>,
) -> plugin_runtime::PluginResponse {
    let request_id = call.request_id.clone();
    match call.method.as_str() {
        // These methods expose frozen Workspace snapshots. Mutating calls are
        // forwarded through the Workspace sync bridge so cloned stores cannot
        // acknowledge writes that the app did not really apply.
        "listSavedConnections" | "refreshSavedConnections" => {
            plugin_runtime::PluginResponse::ok(request_id, saved_connections.clone())
        }
        "exportSavedConnectionsSnapshot" => match saved_connections_snapshot {
            Ok(snapshot) => plugin_runtime::PluginResponse::ok(request_id, json!(snapshot)),
            Err(error) => plugin_runtime::PluginResponse::error(
                request_id,
                plugin_runtime::PluginError::runtime("plugin_sync_error", error.to_string()),
            ),
        },
        "applySavedConnectionsSnapshot" => {
            native_plugin_sync_apply_saved_connections_response(request_id, &call.args, sync_tx)
        }
        "getLocalSyncMetadata" => match local_metadata {
            Ok(metadata) => {
                let mut value = json!(metadata);
                if let Value::Object(fields) = &mut value {
                    if let Some(revision) = saved_forwards_revision {
                        fields.insert("savedForwardsRevision".to_string(), json!(revision));
                    }
                    fields.insert(
                        "pluginSettingsRevisions".to_string(),
                        Value::Object(plugin_settings_revisions.clone()),
                    );
                }
                plugin_runtime::PluginResponse::ok(request_id, value)
            }
            Err(error) => plugin_runtime::PluginResponse::error(
                request_id,
                plugin_runtime::PluginError::runtime("plugin_sync_error", error.to_string()),
            ),
        },
        "preflightExport" => {
            match native_plugin_sync_connection_ids(connection_store, &call.args) {
                Ok(connection_ids) => plugin_runtime::PluginResponse::ok(
                    request_id,
                    json!(preflight_export(
                        connection_store,
                        &connection_ids,
                        native_plugin_bool_arg(&call.args, "embedKeys").unwrap_or(false),
                        0,
                    )),
                ),
                Err(error) => plugin_runtime::PluginResponse::error(
                    request_id,
                    plugin_runtime::PluginError::protocol("invalid_sync_preflight_args", error),
                ),
            }
        }
        "exportOxide" => native_plugin_sync_export_oxide_response(
            plugin_id,
            request_id,
            connection_store,
            plugin_settings,
            &call.args,
            sync_tx,
        ),
        "validateOxide" => {
            let bytes = match native_plugin_file_data_arg(&call.args) {
                Ok(bytes) => bytes,
                Err(error) => {
                    return plugin_runtime::PluginResponse::error(
                        request_id,
                        plugin_runtime::PluginError::protocol("invalid_oxide_file_data", error),
                    );
                }
            };
            match OxideFile::from_bytes(&bytes) {
                Ok(file) => plugin_runtime::PluginResponse::ok(request_id, json!(file.metadata)),
                Err(error) => native_plugin_sync_oxide_error(request_id, error),
            }
        }
        "previewImport" => native_plugin_sync_preview_import_response(
            plugin_id,
            request_id,
            connection_store,
            &call.args,
            sync_tx,
        ),
        "importOxide" => {
            native_plugin_sync_import_oxide_response(plugin_id, request_id, &call.args, sync_tx)
        }
        "onSavedConnectionsChange" => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime(
                "plugin_sync_subscription_pending",
                "sync.onSavedConnectionsChange requires the saved-connection event bridge",
            ),
        ),
        method => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime(
                "plugin_sync_pending",
                format!(
                    "Native plugin sync.{method} requires the Workspace mutation/progress bridge"
                ),
            ),
        ),
    }
}

fn native_plugin_sync_export_oxide_response(
    plugin_id: &str,
    request_id: String,
    connection_store: &oxideterm_connections::ConnectionStore,
    plugin_settings: &[oxideterm_connections::oxide_file::EncryptedPluginSetting],
    args: &Value,
    sync_tx: Option<&mpsc::Sender<NativePluginSyncRequest>>,
) -> plugin_runtime::PluginResponse {
    let connection_ids = match native_plugin_sync_connection_ids(connection_store, args) {
        Ok(connection_ids) => connection_ids,
        Err(error) => {
            return plugin_runtime::PluginResponse::error(
                request_id,
                plugin_runtime::PluginError::protocol("invalid_sync_export_args", error),
            );
        }
    };
    let Some(password) = args.get("password").and_then(Value::as_str) else {
        return plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol(
                "invalid_sync_export_args",
                "sync.exportOxide requires args.password",
            ),
        );
    };
    let password = Zeroizing::new(password.to_string());
    let plugin_settings = match native_plugin_selected_plugin_settings(plugin_settings, args) {
        Ok(settings) => settings,
        Err(error) => {
            return plugin_runtime::PluginResponse::error(
                request_id,
                plugin_runtime::PluginError::protocol("invalid_sync_export_args", error),
            );
        }
    };
    let options = OxideExportOptions {
        description: args
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string),
        embed_keys: native_plugin_bool_arg(args, "embedKeys").unwrap_or(false),
        app_settings_json: native_plugin_optional_string_arg(args, "appSettingsJson"),
        quick_commands_json: native_plugin_optional_string_arg(args, "quickCommandsJson"),
        plugin_settings,
        portable_secrets: Vec::new(),
        forwards: Vec::new(),
    };
    let progress_registration_id = native_plugin_sync_progress_registration_id(args);
    let mut report_progress = |stage: &str, current: usize, total: usize| {
        if let Some(registration_id) = progress_registration_id.as_deref() {
            native_plugin_emit_sync_progress(
                sync_tx,
                plugin_id,
                registration_id,
                native_plugin_sync_progress_value("Exporting .oxide", stage, current, total, false),
            );
        }
    };
    let result = if progress_registration_id.is_some() {
        export_connections_to_oxide_with_progress(
            connection_store,
            &connection_ids,
            &password,
            options,
            &mut report_progress,
        )
    } else {
        export_connections_to_oxide(connection_store, &connection_ids, &password, options)
    };
    match result {
        Ok(bytes) => plugin_runtime::PluginResponse::ok(request_id, json!(bytes)),
        Err(error) => native_plugin_sync_oxide_error(request_id, error),
    }
}

fn native_plugin_sync_preview_import_response(
    plugin_id: &str,
    request_id: String,
    connection_store: &oxideterm_connections::ConnectionStore,
    args: &Value,
    sync_tx: Option<&mpsc::Sender<NativePluginSyncRequest>>,
) -> plugin_runtime::PluginResponse {
    let bytes = match native_plugin_file_data_arg(args) {
        Ok(bytes) => bytes,
        Err(error) => {
            return plugin_runtime::PluginResponse::error(
                request_id,
                plugin_runtime::PluginError::protocol("invalid_oxide_file_data", error),
            );
        }
    };
    let Some(password) = args.get("password").and_then(Value::as_str) else {
        return plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol(
                "invalid_sync_import_args",
                "sync.previewImport requires args.password",
            ),
        );
    };
    let password = Zeroizing::new(password.to_string());
    let strategy =
        match ImportConflictStrategy::parse(args.get("conflictStrategy").and_then(Value::as_str)) {
            Ok(strategy) => strategy,
            Err(error) => return native_plugin_sync_oxide_error(request_id, error),
        };
    let progress_registration_id = native_plugin_sync_progress_registration_id(args);
    let mut report_progress = |stage: &str, current: usize, total: usize| {
        if let Some(registration_id) = progress_registration_id.as_deref() {
            native_plugin_emit_sync_progress(
                sync_tx,
                plugin_id,
                registration_id,
                native_plugin_sync_progress_value(
                    "Previewing .oxide import",
                    stage,
                    current,
                    total,
                    false,
                ),
            );
        }
    };
    let result = if progress_registration_id.is_some() {
        preview_oxide_import_with_progress(
            connection_store,
            &bytes,
            &password,
            strategy,
            &mut report_progress,
        )
    } else {
        preview_oxide_import(connection_store, &bytes, &password, strategy)
    };
    match result {
        Ok(preview) => plugin_runtime::PluginResponse::ok(request_id, json!(preview)),
        Err(error) => native_plugin_sync_oxide_error(request_id, error),
    }
}

fn native_plugin_sync_apply_saved_connections_response(
    request_id: String,
    args: &Value,
    sync_tx: Option<&mpsc::Sender<NativePluginSyncRequest>>,
) -> plugin_runtime::PluginResponse {
    let Some(sync_tx) = sync_tx else {
        return plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime(
                "plugin_sync_apply_unavailable",
                "sync.applySavedConnectionsSnapshot requires the Workspace sync mutation bridge",
            ),
        );
    };
    let (snapshot, conflict_strategy) = match native_plugin_sync_apply_saved_connections_args(args)
    {
        Ok(parsed) => parsed,
        Err(error) => {
            return plugin_runtime::PluginResponse::error(
                request_id,
                plugin_runtime::PluginError::protocol("invalid_sync_apply_args", error),
            );
        }
    };
    let (response_tx, response_rx) = mpsc::channel();
    if sync_tx
        .send(NativePluginSyncRequest {
            request_id: request_id.clone(),
            action: NativePluginSyncAction::ApplySavedConnectionsSnapshot {
                snapshot,
                conflict_strategy,
            },
            response_tx,
        })
        .is_err()
    {
        return plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime(
                "plugin_sync_apply_unavailable",
                "Native plugin sync.applySavedConnectionsSnapshot cannot reach the workspace sync host",
            ),
        );
    }

    response_rx.recv().unwrap_or_else(|_| {
        plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime(
                "plugin_sync_apply_response_unavailable",
                "Native plugin sync.applySavedConnectionsSnapshot closed before the workspace answered",
            ),
        )
    })
}

fn native_plugin_sync_apply_saved_connections_args(
    args: &Value,
) -> Result<
    (
        SavedConnectionsSyncSnapshot,
        SavedConnectionsConflictStrategy,
    ),
    String,
> {
    let snapshot = args
        .get("snapshot")
        .cloned()
        .ok_or_else(|| "sync.applySavedConnectionsSnapshot requires args.snapshot".to_string())
        .and_then(|value| serde_json::from_value(value).map_err(|error| error.to_string()))?;
    let conflict_strategy = SavedConnectionsConflictStrategy::parse(
        args.get("conflictStrategy").and_then(Value::as_str),
    )
    .map_err(|error| error.to_string())?;
    Ok((snapshot, conflict_strategy))
}

fn native_plugin_sync_progress_registration_id(args: &Value) -> Option<String> {
    args.get("progressRegistrationId")
        .or_else(|| args.get("progressId"))
        .and_then(Value::as_str)
        .filter(|registration_id| !registration_id.is_empty())
        .map(str::to_string)
}

fn native_plugin_emit_sync_progress(
    sync_tx: Option<&mpsc::Sender<NativePluginSyncRequest>>,
    plugin_id: &str,
    registration_id: &str,
    value: Value,
) {
    let Some(sync_tx) = sync_tx else {
        return;
    };
    let (response_tx, _response_rx) = mpsc::channel();
    // Progress reports are advisory UI updates; the sync operation must not
    // block waiting for the Workspace render loop to acknowledge each stage.
    let _ = sync_tx.send(NativePluginSyncRequest {
        request_id: format!("sync-progress:{plugin_id}:{registration_id}"),
        action: NativePluginSyncAction::ReportProgress {
            plugin_id: plugin_id.to_string(),
            registration_id: registration_id.to_string(),
            value,
        },
        response_tx,
    });
}

fn native_plugin_sync_progress_value(
    title: &str,
    stage: &str,
    current: usize,
    total: usize,
    done: bool,
) -> Value {
    let progress = if total == 0 {
        0.0
    } else {
        ((current.min(total) as f32 / total as f32) * 100.0).min(100.0)
    };
    json!({
        "title": title,
        "message": stage,
        "stage": stage,
        "current": current,
        "total": total,
        "progress": progress,
        "done": done,
    })
}

fn native_plugin_selected_plugin_settings(
    plugin_settings: &[oxideterm_connections::oxide_file::EncryptedPluginSetting],
    args: &Value,
) -> Result<Vec<oxideterm_connections::oxide_file::EncryptedPluginSetting>, String> {
    if !native_plugin_bool_arg(args, "includePluginSettings").unwrap_or(false) {
        return Ok(Vec::new());
    }
    let selected_plugin_ids = native_plugin_optional_string_set_arg(args, "selectedPluginIds")?;
    Ok(plugin_settings
        .iter()
        .filter(|setting| {
            selected_plugin_ids.as_ref().is_none_or(|ids| {
                native_plugin_id_from_setting_storage_key(&setting.storage_key)
                    .is_some_and(|plugin_id| ids.contains(&plugin_id))
            })
        })
        .cloned()
        .collect())
}

fn native_plugin_settings_revision_map(
    plugin_settings: &[oxideterm_connections::oxide_file::EncryptedPluginSetting],
) -> Map<String, Value> {
    let mut grouped = HashMap::<String, Vec<(String, String)>>::new();
    for setting in plugin_settings {
        let Some(plugin_id) = native_plugin_id_from_setting_storage_key(&setting.storage_key)
        else {
            continue;
        };
        grouped.entry(plugin_id).or_default().push((
            setting.storage_key.clone(),
            setting.serialized_value.clone(),
        ));
    }
    let mut plugin_ids = grouped.keys().cloned().collect::<Vec<_>>();
    plugin_ids.sort();
    plugin_ids
        .into_iter()
        .filter_map(|plugin_id| {
            let mut entries = grouped.remove(&plugin_id)?;
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            let text = serde_json::to_string(&entries).ok()?;
            Some((
                plugin_id,
                Value::String(native_plugin_stable_hash_string(&text)),
            ))
        })
        .collect()
}

fn native_plugin_id_from_setting_storage_key(storage_key: &str) -> Option<String> {
    const PREFIX: &str = "oxide-plugin-";
    const SEPARATOR: &str = "-setting-";

    let remainder = storage_key.strip_prefix(PREFIX)?;
    let separator_index = remainder.find(SEPARATOR)?;
    let plugin_id = &remainder[..separator_index];
    let setting_id = &remainder[separator_index + SEPARATOR.len()..];
    if plugin_id.is_empty() || setting_id.is_empty() {
        return None;
    }
    Some(plugin_id.to_string())
}

fn native_plugin_stable_hash_string(text: &str) -> String {
    let mut hash = 2166136261u32;
    for byte in text.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(16777619);
    }
    format!("fnv1a-{hash:x}")
}

fn native_plugin_sync_import_oxide_response(
    plugin_id: &str,
    request_id: String,
    args: &Value,
    sync_tx: Option<&mpsc::Sender<NativePluginSyncRequest>>,
) -> plugin_runtime::PluginResponse {
    let Some(sync_tx) = sync_tx else {
        return plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime(
                "plugin_sync_import_unavailable",
                "sync.importOxide requires the Workspace sync mutation bridge",
            ),
        );
    };
    let (bytes, password, options) = match native_plugin_sync_import_oxide_args(args) {
        Ok(parsed) => parsed,
        Err(error) => {
            return plugin_runtime::PluginResponse::error(
                request_id,
                plugin_runtime::PluginError::protocol("invalid_sync_import_args", error),
            );
        }
    };
    let (response_tx, response_rx) = mpsc::channel();
    if sync_tx
        .send(NativePluginSyncRequest {
            request_id: request_id.clone(),
            action: NativePluginSyncAction::ImportOxide {
                bytes,
                password,
                options,
                progress_registration_id: native_plugin_sync_progress_registration_id(args),
                plugin_id: plugin_id.to_string(),
            },
            response_tx,
        })
        .is_err()
    {
        return plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime(
                "plugin_sync_import_unavailable",
                "Native plugin sync.importOxide cannot reach the workspace sync host",
            ),
        );
    }

    response_rx.recv().unwrap_or_else(|_| {
        plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime(
                "plugin_sync_import_response_unavailable",
                "Native plugin sync.importOxide closed before the workspace answered",
            ),
        )
    })
}

fn native_plugin_sync_import_oxide_args(
    args: &Value,
) -> Result<(Vec<u8>, Zeroizing<String>, NativePluginOxideImportOptions), String> {
    let bytes = native_plugin_file_data_arg(args)?;
    let password = args
        .get("password")
        .and_then(Value::as_str)
        .ok_or_else(|| "sync.importOxide requires args.password".to_string())
        .map(|password| Zeroizing::new(password.to_string()))?;
    let conflict_strategy =
        ImportConflictStrategy::parse(args.get("conflictStrategy").and_then(Value::as_str))
            .map_err(|error| error.to_string())?;
    let options = NativePluginOxideImportOptions {
        oxide_options: OxideImportOptions {
            selected_names: native_plugin_optional_string_array_arg(args, "selectedNames")?,
            conflict_strategy,
            import_forwards: native_plugin_bool_arg(args, "importForwards").unwrap_or(true),
            import_portable_secrets: native_plugin_bool_arg(args, "importPortableSecrets")
                .unwrap_or(false),
        },
        import_app_settings: native_plugin_bool_arg(args, "importAppSettings").unwrap_or(true),
        selected_app_settings_sections: native_plugin_optional_string_set_arg(
            args,
            "selectedAppSettingsSections",
        )?,
        import_plugin_settings: native_plugin_bool_arg(args, "importPluginSettings")
            .unwrap_or(true),
        selected_plugin_ids: native_plugin_optional_string_set_arg(args, "selectedPluginIds")?,
        import_quick_commands: native_plugin_bool_arg(args, "importQuickCommands").unwrap_or(true),
        quick_command_strategy: native_plugin_quick_command_strategy_from_oxide(conflict_strategy),
    };
    Ok((bytes, password, options))
}

#[cfg(test)]
fn native_plugin_apply_oxide_import_core(
    store: &mut oxideterm_connections::ConnectionStore,
    bytes: &[u8],
    password: &str,
    options: OxideImportOptions,
) -> Result<ImportResultEnvelope, String> {
    oxideterm_connections::oxide_file::apply_oxide_import_with_options(
        store, bytes, password, options,
    )
    .map_err(native_plugin_oxide_file_error_message)
}

fn native_plugin_apply_oxide_import_core_with_progress<F>(
    store: &mut oxideterm_connections::ConnectionStore,
    bytes: &[u8],
    password: &str,
    options: OxideImportOptions,
    on_progress: F,
) -> Result<ImportResultEnvelope, String>
where
    F: FnMut(&str, usize, usize),
{
    apply_oxide_import_with_options_with_progress(store, bytes, password, options, on_progress)
        .map_err(native_plugin_oxide_file_error_message)
}

fn native_plugin_quick_command_strategy_from_oxide(
    strategy: ImportConflictStrategy,
) -> QuickCommandImportStrategy {
    match strategy {
        ImportConflictStrategy::Rename => QuickCommandImportStrategy::Rename,
        ImportConflictStrategy::Skip => QuickCommandImportStrategy::Skip,
        ImportConflictStrategy::Replace => QuickCommandImportStrategy::Replace,
        ImportConflictStrategy::Merge => QuickCommandImportStrategy::Merge,
    }
}

fn native_plugin_sync_import_result_value(
    envelope: &ImportResultEnvelope,
    imported_app_settings: bool,
    skipped_app_settings: bool,
    imported_quick_commands: usize,
    skipped_quick_commands: bool,
    quick_commands_errors: Vec<String>,
    imported_plugin_settings: usize,
    skipped_plugin_settings: bool,
) -> Value {
    let mut value = json!(envelope);
    if let Value::Object(fields) = &mut value {
        // PluginContext's importOxide result mirrors oxideClientState.ts: raw
        // side-car payloads are consumed by the host and are not returned.
        fields.remove("appSettingsJson");
        fields.remove("quickCommandsJson");
        fields.remove("pluginSettings");
        fields.insert(
            "importedAppSettings".to_string(),
            json!(imported_app_settings),
        );
        fields.insert(
            "skippedAppSettings".to_string(),
            json!(skipped_app_settings),
        );
        fields.insert(
            "importedQuickCommands".to_string(),
            json!(imported_quick_commands),
        );
        fields.insert(
            "skippedQuickCommands".to_string(),
            json!(skipped_quick_commands),
        );
        fields.insert(
            "quickCommandsErrors".to_string(),
            json!(quick_commands_errors),
        );
        fields.insert(
            "importedPluginSettings".to_string(),
            json!(imported_plugin_settings),
        );
        fields.insert(
            "skippedPluginSettings".to_string(),
            json!(skipped_plugin_settings),
        );
    }
    value
}

fn native_plugin_sync_connection_ids(
    connection_store: &oxideterm_connections::ConnectionStore,
    args: &Value,
) -> Result<Vec<String>, String> {
    if let Some(values) = args.get("connectionIds") {
        if values.is_null() {
            return Ok(native_plugin_all_saved_connection_ids(connection_store));
        }
        let Some(values) = values.as_array() else {
            return Err("sync connectionIds must be an array".to_string());
        };
        return values
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_string)
                    .ok_or_else(|| "sync connectionIds must contain strings".to_string())
            })
            .collect();
    }
    Ok(native_plugin_all_saved_connection_ids(connection_store))
}

fn native_plugin_all_saved_connection_ids(
    connection_store: &oxideterm_connections::ConnectionStore,
) -> Vec<String> {
    connection_store
        .connections()
        .iter()
        .map(|connection| connection.id.clone())
        .collect()
}

fn native_plugin_file_data_arg(args: &Value) -> Result<Vec<u8>, String> {
    let Some(file_data) = args.get("fileData").and_then(Value::as_array) else {
        return Err("oxide fileData must be an array of bytes".to_string());
    };
    native_plugin_u8_array(file_data)
        .ok_or_else(|| "oxide fileData contains a non-byte value".to_string())
}

fn native_plugin_optional_string_array_arg(
    args: &Value,
    field: &str,
) -> Result<Option<Vec<String>>, String> {
    let Some(value) = args.get(field) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let Some(values) = value.as_array() else {
        return Err(format!("sync.{field} must be an array of strings"));
    };
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("sync.{field} must contain only strings"))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

fn native_plugin_optional_string_set_arg(
    args: &Value,
    field: &str,
) -> Result<Option<HashSet<String>>, String> {
    native_plugin_optional_string_array_arg(args, field)
        .map(|values| values.map(|values| values.into_iter().collect()))
}

fn native_plugin_bool_arg(args: &Value, field: &str) -> Option<bool> {
    args.get(field).and_then(Value::as_bool)
}

fn native_plugin_optional_string_arg(args: &Value, field: &str) -> Option<String> {
    args.get(field).and_then(Value::as_str).map(str::to_string)
}

fn native_plugin_sync_oxide_error(
    request_id: String,
    error: oxideterm_connections::oxide_file::OxideFileError,
) -> plugin_runtime::PluginResponse {
    plugin_runtime::PluginResponse::error(
        request_id,
        plugin_runtime::PluginError::runtime("plugin_sync_oxide_error", error.to_string()),
    )
}

fn native_plugin_oxide_file_error_message(
    error: oxideterm_connections::oxide_file::OxideFileError,
) -> String {
    match error {
        oxideterm_connections::oxide_file::OxideFileError::DecryptionFailed => {
            "密码错误，无法解密文件".to_string()
        }
        oxideterm_connections::oxide_file::OxideFileError::ChecksumMismatch => {
            "文件验证失败，数据可能已被篡改".to_string()
        }
        oxideterm_connections::oxide_file::OxideFileError::PasswordTooShort => {
            "密码长度至少为 6 位".to_string()
        }
        other => other.to_string(),
    }
}

fn native_plugin_transfers_response(
    call: plugin_runtime::PluginHostCall,
    manager: &Arc<SftpTransferManager>,
) -> plugin_runtime::PluginResponse {
    let request_id = call.request_id.clone();
    match call.method.as_str() {
        "getAll" => plugin_runtime::PluginResponse::ok(
            request_id,
            native_plugin_transfer_snapshot_array(manager, None),
        ),
        "getByNode" => match native_plugin_transfer_node_id_arg(&call.args) {
            Ok(node_id) => plugin_runtime::PluginResponse::ok(
                request_id,
                native_plugin_transfer_snapshot_array(manager, Some(node_id.as_str())),
            ),
            Err(error) => plugin_runtime::PluginResponse::error(
                request_id,
                plugin_runtime::PluginError::protocol("invalid_transfer_node", error),
            ),
        },
        "onProgress" | "onComplete" | "onError" => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime(
                "plugin_transfer_subscription_bridge",
                "transfer subscriptions are registered through the runtime event bridge",
            ),
        ),
        method => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol(
                "unknown_transfer_method",
                format!("Unknown transfers.{method} host API"),
            ),
        ),
    }
}

fn native_plugin_transfer_snapshot_array(
    manager: &Arc<SftpTransferManager>,
    node_id: Option<&str>,
) -> Value {
    Value::Array(
        manager
            .list_background_transfers(node_id)
            .iter()
            .map(native_plugin_transfer_snapshot)
            .collect(),
    )
}

fn native_plugin_transfer_snapshot(snapshot: &BackgroundTransferSnapshot) -> Value {
    // Match Tauri's TransferSnapshot projection and intentionally omit native
    // implementation details such as transfer strategy, backend speed, and
    // retained item counts.
    json!({
        "id": &snapshot.id,
        "nodeId": &snapshot.node_id,
        "name": &snapshot.name,
        "localPath": &snapshot.local_path,
        "remotePath": &snapshot.remote_path,
        "direction": native_plugin_transfer_direction_label(snapshot.direction),
        "size": snapshot.size,
        "transferred": snapshot.transferred,
        "state": native_plugin_transfer_state_label(snapshot.state),
        "error": &snapshot.error,
        "startTime": snapshot.start_time,
        "endTime": snapshot.end_time,
    })
}

fn native_plugin_transfer_node_id_arg(args: &Value) -> Result<String, String> {
    let node_id = args
        .get("nodeId")
        .and_then(Value::as_str)
        .or_else(|| args.as_str())
        .ok_or_else(|| "transfers.getByNode requires args.nodeId".to_string())?;
    if node_id.trim().is_empty() {
        return Err("transfers.getByNode requires a non-empty node id".to_string());
    }
    Ok(node_id.to_string())
}

fn native_plugin_transfer_direction_label(direction: BackgroundTransferDirection) -> &'static str {
    match direction {
        BackgroundTransferDirection::Upload => "upload",
        BackgroundTransferDirection::Download => "download",
    }
}

fn native_plugin_transfer_state_label(state: BackgroundTransferState) -> &'static str {
    match state {
        BackgroundTransferState::Pending => "pending",
        BackgroundTransferState::Active => "active",
        BackgroundTransferState::Paused => "paused",
        BackgroundTransferState::Completed => "completed",
        BackgroundTransferState::Cancelled => "cancelled",
        BackgroundTransferState::Error => "error",
    }
}

fn native_plugin_transfer_state_map(transfers: &Value) -> HashMap<String, BackgroundTransferState> {
    transfers
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|transfer| {
            let id = transfer.get("id").and_then(Value::as_str)?;
            let state = transfer
                .get("state")
                .and_then(Value::as_str)
                .and_then(native_plugin_transfer_state_from_label)?;
            Some((id.to_string(), state))
        })
        .collect()
}

fn native_plugin_transfer_state_from_label(state: &str) -> Option<BackgroundTransferState> {
    match state {
        "pending" => Some(BackgroundTransferState::Pending),
        "active" => Some(BackgroundTransferState::Active),
        "paused" => Some(BackgroundTransferState::Paused),
        "completed" => Some(BackgroundTransferState::Completed),
        "cancelled" => Some(BackgroundTransferState::Cancelled),
        "error" => Some(BackgroundTransferState::Error),
        _ => None,
    }
}

fn native_plugin_transfer_values_by_state(
    transfers: &Value,
    state: BackgroundTransferState,
) -> Vec<Value> {
    transfers
        .as_array()
        .into_iter()
        .flatten()
        .filter(|transfer| {
            transfer
                .get("state")
                .and_then(Value::as_str)
                .and_then(native_plugin_transfer_state_from_label)
                == Some(state)
        })
        .cloned()
        .collect()
}

fn native_plugin_transfer_transition_values(
    transfers: &Value,
    previous_states: &HashMap<String, BackgroundTransferState>,
    next_states: &HashMap<String, BackgroundTransferState>,
    target_state: BackgroundTransferState,
) -> Vec<Value> {
    transfers
        .as_array()
        .into_iter()
        .flatten()
        .filter(|transfer| {
            let Some(id) = transfer.get("id").and_then(Value::as_str) else {
                return false;
            };
            next_states.get(id) == Some(&target_state)
                && previous_states.get(id) != Some(&target_state)
        })
        .cloned()
        .collect()
}

fn native_plugin_transfer_progress_due(workspace: &WorkspaceApp) -> bool {
    workspace
        .native_plugin_transfer_progress_last_emitted
        .map(|last_emitted| last_emitted.elapsed() >= NATIVE_PLUGIN_TRANSFER_PROGRESS_INTERVAL)
        .unwrap_or(true)
}

fn native_plugin_profiler_response(
    call: plugin_runtime::PluginHostCall,
    registry: &ProfilerRegistry,
    node_connection_ids: &HashMap<String, String>,
) -> plugin_runtime::PluginResponse {
    let request_id = call.request_id.clone();
    match call.method.as_str() {
        "getMetrics" => match native_plugin_profiler_connection_id(&call.args, node_connection_ids)
        {
            Ok(Some(connection_id)) => plugin_runtime::PluginResponse::ok(
                request_id,
                registry
                    .latest(&connection_id)
                    .map(|metrics| native_plugin_profiler_metrics_snapshot(&metrics))
                    .unwrap_or(Value::Null),
            ),
            Ok(None) => plugin_runtime::PluginResponse::ok(request_id, Value::Null),
            Err(error) => native_plugin_profiler_arg_error(request_id, error),
        },
        "getHistory" => match native_plugin_profiler_connection_id(&call.args, node_connection_ids)
        {
            Ok(Some(connection_id)) => {
                let history =
                    native_plugin_profiler_limited_history(registry, &connection_id, &call.args);
                plugin_runtime::PluginResponse::ok(request_id, Value::Array(history))
            }
            Ok(None) => plugin_runtime::PluginResponse::ok(request_id, json!([])),
            Err(error) => native_plugin_profiler_arg_error(request_id, error),
        },
        "isRunning" => {
            match native_plugin_profiler_connection_id(&call.args, node_connection_ids) {
                Ok(Some(connection_id)) => plugin_runtime::PluginResponse::ok(
                    request_id,
                    json!(registry.state(&connection_id) == Some(ProfilerState::Running)),
                ),
                Ok(None) => plugin_runtime::PluginResponse::ok(request_id, json!(false)),
                Err(error) => native_plugin_profiler_arg_error(request_id, error),
            }
        }
        "onMetrics" => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime(
                "plugin_profiler_subscription_bridge",
                "profiler subscriptions are registered through the runtime event bridge",
            ),
        ),
        method => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol(
                "unknown_profiler_method",
                format!("Unknown profiler.{method} host API"),
            ),
        ),
    }
}

fn native_plugin_profiler_arg_error(
    request_id: String,
    error: String,
) -> plugin_runtime::PluginResponse {
    plugin_runtime::PluginResponse::error(
        request_id,
        plugin_runtime::PluginError::protocol("invalid_profiler_node", error),
    )
}

fn native_plugin_profiler_node_connection_ids(workspace: &WorkspaceApp) -> HashMap<String, String> {
    workspace
        .node_runtime_store
        .export_snapshot()
        .nodes
        .into_iter()
        .filter_map(|node| {
            node.connection_id
                .map(|connection_id| (node.id.0, connection_id))
        })
        .collect()
}

fn native_plugin_profiler_connection_id(
    args: &Value,
    node_connection_ids: &HashMap<String, String>,
) -> Result<Option<String>, String> {
    let node_id = native_plugin_profiler_node_id_arg(args)?;
    Ok(node_connection_ids.get(&node_id).cloned())
}

fn native_plugin_profiler_node_id_arg(args: &Value) -> Result<String, String> {
    let node_id = args
        .get("nodeId")
        .and_then(Value::as_str)
        .or_else(|| args.as_str())
        .ok_or_else(|| "profiler host calls require args.nodeId".to_string())?;
    if node_id.trim().is_empty() {
        return Err("profiler host calls require a non-empty node id".to_string());
    }
    Ok(node_id.to_string())
}

fn native_plugin_profiler_limited_history(
    registry: &ProfilerRegistry,
    connection_id: &str,
    args: &Value,
) -> Vec<Value> {
    let mut history = registry
        .history(connection_id)
        .iter()
        .map(native_plugin_profiler_metrics_snapshot)
        .collect::<Vec<_>>();
    if let Some(max_points) = args.get("maxPoints").and_then(Value::as_u64) {
        let max_points = max_points as usize;
        if max_points < history.len() {
            history.drain(0..history.len() - max_points);
        }
    }
    history
}

fn native_plugin_profiler_snapshot_array(
    registry: &ProfilerRegistry,
    node_connection_ids: &HashMap<String, String>,
) -> Value {
    let mut node_entries = node_connection_ids.iter().collect::<Vec<_>>();
    node_entries.sort_by(|left, right| left.0.cmp(right.0));
    Value::Array(
        node_entries
            .into_iter()
            .filter_map(|(node_id, connection_id)| {
                registry.latest(connection_id).map(|metrics| {
                    json!({
                        "nodeId": node_id,
                        "metrics": native_plugin_profiler_metrics_snapshot(&metrics),
                    })
                })
            })
            .collect(),
    )
}

fn native_plugin_profiler_metrics_snapshot(metrics: &ResourceMetrics) -> Value {
    // Tauri's ProfilerMetricsSnapshot intentionally excludes backend-only disk
    // and sampler-source fields; keep the plugin contract stable here.
    json!({
        "timestampMs": metrics.timestamp_ms,
        "cpuPercent": metrics.cpu_percent,
        "memoryUsed": metrics.memory_used,
        "memoryTotal": metrics.memory_total,
        "memoryPercent": metrics.memory_percent,
        "loadAvg1": metrics.load_avg_1,
        "loadAvg5": metrics.load_avg_5,
        "loadAvg15": metrics.load_avg_15,
        "cpuCores": metrics.cpu_cores,
        "netRxBytesPerSec": metrics.net_rx_bytes_per_sec,
        "netTxBytesPerSec": metrics.net_tx_bytes_per_sec,
        "sshRttMs": metrics.ssh_rtt_ms,
    })
}

fn native_plugin_profiler_timestamp_map(metrics: &Value) -> HashMap<String, u64> {
    metrics
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let node_id = entry.get("nodeId").and_then(Value::as_str)?;
            let timestamp = entry
                .get("metrics")
                .and_then(|metrics| metrics.get("timestampMs"))
                .and_then(Value::as_u64)?;
            Some((node_id.to_string(), timestamp))
        })
        .collect()
}

fn native_plugin_profiler_changed_metric_entries(
    metrics: &Value,
    previous_timestamps: &HashMap<String, u64>,
    next_timestamps: &HashMap<String, u64>,
) -> Vec<Value> {
    metrics
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| {
            let Some(node_id) = entry.get("nodeId").and_then(Value::as_str) else {
                return false;
            };
            next_timestamps.get(node_id) != previous_timestamps.get(node_id)
        })
        .cloned()
        .collect()
}

fn native_plugin_profiler_metrics_due(workspace: &WorkspaceApp) -> bool {
    workspace
        .native_plugin_profiler_last_emitted
        .map(|last_emitted| last_emitted.elapsed() >= NATIVE_PLUGIN_PROFILER_METRICS_INTERVAL)
        .unwrap_or(true)
}

fn native_plugin_subscription_allows_node(filter: Option<&Value>, node_id: &str) -> bool {
    filter
        .and_then(|filter| filter.get("nodeId"))
        .and_then(Value::as_str)
        .is_none_or(|filter_node_id| filter_node_id == node_id)
}

fn native_plugin_ide_response(
    call: plugin_runtime::PluginHostCall,
    snapshot: &Value,
) -> plugin_runtime::PluginResponse {
    let request_id = call.request_id.clone();
    match call.method.as_str() {
        "isOpen" => plugin_runtime::PluginResponse::ok(
            request_id,
            json!(
                snapshot
                    .get("isOpen")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            ),
        ),
        "getProject" => plugin_runtime::PluginResponse::ok(
            request_id,
            snapshot.get("project").cloned().unwrap_or(Value::Null),
        ),
        "getOpenFiles" => plugin_runtime::PluginResponse::ok(
            request_id,
            snapshot
                .get("openFiles")
                .cloned()
                .unwrap_or_else(|| json!([])),
        ),
        "getActiveFile" => plugin_runtime::PluginResponse::ok(
            request_id,
            snapshot.get("activeFile").cloned().unwrap_or(Value::Null),
        ),
        "onFileOpen" | "onFileClose" | "onActiveFileChange" => {
            plugin_runtime::PluginResponse::error(
                request_id,
                plugin_runtime::PluginError::runtime(
                    "plugin_ide_subscription_bridge",
                    "IDE subscriptions are registered through the runtime event bridge",
                ),
            )
        }
        method => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol(
                "unknown_ide_method",
                format!("Unknown ide.{method} host API"),
            ),
        ),
    }
}

fn native_plugin_ide_workspace_snapshot(
    workspace: &WorkspaceApp,
    cx: &mut Context<WorkspaceApp>,
) -> Option<IdePluginSnapshot> {
    let active_surface = workspace
        .active_tab_id
        .and_then(|tab_id| workspace.ide_tab_surfaces.get(&tab_id))
        .and_then(|surface| surface.read(cx).plugin_snapshot());
    active_surface.or_else(|| {
        workspace
            .ide_tab_surfaces
            .values()
            .find_map(|surface| surface.read(cx).plugin_snapshot())
    })
}

fn native_plugin_ide_snapshot_value(snapshot: &IdePluginSnapshot) -> Value {
    // This projection mirrors Tauri's ideStore snapshot without exposing file
    // content, tree nodes, agent process state, or reconnect-only metadata.
    json!({
        "isOpen": true,
        "project": {
            "nodeId": &snapshot.project.node_id,
            "rootPath": &snapshot.project.root_path,
            "name": &snapshot.project.name,
            "isGitRepo": snapshot.project.is_git_repo,
            "gitBranch": &snapshot.project.git_branch,
        },
        "openFiles": snapshot
            .open_files
            .iter()
            .map(native_plugin_ide_file_snapshot)
            .collect::<Vec<_>>(),
        "activeFile": snapshot
            .active_file
            .as_ref()
            .map(native_plugin_ide_file_snapshot),
    })
}

fn native_plugin_ide_file_snapshot(file: &IdePluginFileSnapshot) -> Value {
    json!({
        "path": &file.path,
        "name": &file.name,
        "language": &file.language,
        "isDirty": file.is_dirty,
        "isActive": file.is_active,
        "isPinned": file.is_pinned,
    })
}

fn native_plugin_ide_file_map(snapshot: &Value) -> HashMap<String, Value> {
    snapshot
        .get("openFiles")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|file| {
            let path = file.get("path").and_then(Value::as_str)?;
            Some((path.to_string(), file.clone()))
        })
        .collect()
}

fn native_plugin_ide_active_file_path(snapshot: &Value) -> Option<String> {
    snapshot
        .get("activeFile")
        .and_then(|file| file.get("path"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn native_plugin_ai_response(
    call: plugin_runtime::PluginHostCall,
    snapshot: &Value,
) -> plugin_runtime::PluginResponse {
    let request_id = call.request_id.clone();
    match call.method.as_str() {
        "getConversations" => plugin_runtime::PluginResponse::ok(
            request_id,
            snapshot
                .get("conversations")
                .cloned()
                .unwrap_or_else(|| json!([])),
        ),
        "getMessages" => {
            let Some(conversation_id) = call.args.get("conversationId").and_then(Value::as_str)
            else {
                return plugin_runtime::PluginResponse::error(
                    request_id,
                    plugin_runtime::PluginError::protocol(
                        "invalid_ai_conversation",
                        "ai.getMessages requires args.conversationId",
                    ),
                );
            };
            let messages = snapshot
                .get("messagesByConversation")
                .and_then(|messages| messages.get(conversation_id))
                .cloned()
                .unwrap_or_else(|| json!([]));
            plugin_runtime::PluginResponse::ok(request_id, messages)
        }
        "getActiveProvider" => plugin_runtime::PluginResponse::ok(
            request_id,
            snapshot
                .get("activeProvider")
                .cloned()
                .unwrap_or(Value::Null),
        ),
        "getAvailableModels" => plugin_runtime::PluginResponse::ok(
            request_id,
            snapshot
                .get("availableModels")
                .cloned()
                .unwrap_or_else(|| json!([])),
        ),
        "onMessage" => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime(
                "plugin_ai_subscription_bridge",
                "AI subscriptions are registered through the runtime event bridge",
            ),
        ),
        method => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol(
                "unknown_ai_method",
                format!("Unknown ai.{method} host API"),
            ),
        ),
    }
}

fn native_plugin_ai_snapshot_value(
    chat: &oxideterm_ai::AiChatState,
    providers: &[Value],
    active_provider_id: Option<&str>,
    model_context_windows: &Map<String, Value>,
) -> Value {
    let provider_views = oxideterm_ai::provider_views(providers);
    let active_provider = oxideterm_ai::active_provider_view(&provider_views, active_provider_id);
    let active_provider_value = active_provider.map(|provider| {
        json!({
            "type": provider.provider_type,
            "displayName": provider.name,
        })
    });
    let available_models = active_provider_id
        .and_then(|provider_id| model_context_windows.get(provider_id))
        .and_then(Value::as_object)
        .map(|models| {
            models
                .keys()
                .cloned()
                .map(Value::String)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut messages_by_conversation = Map::new();
    for conversation in &chat.conversations {
        messages_by_conversation.insert(
            conversation.id.clone(),
            Value::Array(
                conversation
                    .messages
                    .iter()
                    .filter_map(native_plugin_ai_message_snapshot)
                    .collect(),
            ),
        );
    }
    json!({
        "conversations": chat
            .conversations
            .iter()
            .map(native_plugin_ai_conversation_snapshot)
            .collect::<Vec<_>>(),
        "messagesByConversation": messages_by_conversation,
        "activeProvider": active_provider_value,
        "availableModels": available_models,
    })
}

fn native_plugin_ai_conversation_snapshot(conversation: &oxideterm_ai::AiConversation) -> Value {
    // The plugin API does not expose tool-role messages, so the count follows
    // the sanitized message projection used by getMessages and onMessage.
    let visible_message_count = conversation
        .messages
        .iter()
        .filter_map(native_plugin_ai_message_snapshot)
        .count();
    json!({
        "id": &conversation.id,
        "title": &conversation.title,
        "messageCount": visible_message_count,
        "createdAt": conversation.created_at_ms,
        "updatedAt": conversation.updated_at_ms,
    })
}

fn native_plugin_ai_message_snapshot(message: &oxideterm_ai::AiChatMessage) -> Option<Value> {
    let role = native_plugin_ai_role_label(message.role)?;
    Some(json!({
        "id": &message.id,
        "role": role,
        "content": oxideterm_ai::sanitize_for_ai(&message.content),
        "timestamp": message.timestamp_ms,
    }))
}

fn native_plugin_ai_role_label(role: oxideterm_ai::AiChatRole) -> Option<&'static str> {
    match role {
        oxideterm_ai::AiChatRole::User => Some("user"),
        oxideterm_ai::AiChatRole::Assistant => Some("assistant"),
        oxideterm_ai::AiChatRole::System => Some("system"),
        oxideterm_ai::AiChatRole::Tool => None,
    }
}

fn native_plugin_ai_message_count_map(snapshot: &Value) -> HashMap<String, usize> {
    snapshot
        .get("conversations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|conversation| {
            let id = conversation.get("id").and_then(Value::as_str)?;
            let count = conversation
                .get("messageCount")
                .and_then(Value::as_u64)
                .unwrap_or_default() as usize;
            Some((id.to_string(), count))
        })
        .collect()
}

fn native_plugin_ai_new_message_events(
    snapshot: &Value,
    previous_counts: &HashMap<String, usize>,
) -> Vec<Value> {
    let Some(conversations) = snapshot.get("conversations").and_then(Value::as_array) else {
        return Vec::new();
    };
    conversations
        .iter()
        .filter_map(|conversation| {
            let conversation_id = conversation.get("id").and_then(Value::as_str)?;
            let count = conversation
                .get("messageCount")
                .and_then(Value::as_u64)
                .unwrap_or_default() as usize;
            if count
                <= previous_counts
                    .get(conversation_id)
                    .copied()
                    .unwrap_or_default()
            {
                return None;
            }
            let message = snapshot
                .get("messagesByConversation")
                .and_then(|messages| messages.get(conversation_id))
                .and_then(Value::as_array)
                .and_then(|messages| messages.last())?;
            Some(json!({
                "conversationId": conversation_id,
                "messageId": message.get("id").and_then(Value::as_str).unwrap_or_default(),
                "role": message.get("role").and_then(Value::as_str).unwrap_or_default(),
            }))
        })
        .collect()
}

fn native_plugin_validate_secret_plugin_id(plugin_id: &str) -> Result<(), String> {
    if plugin_id.is_empty() {
        return Err("Plugin ID cannot be empty".to_string());
    }
    if plugin_id.contains('/') || plugin_id.contains('\\') || plugin_id.contains("..") {
        return Err("Plugin ID contains invalid path characters".to_string());
    }
    if plugin_id.bytes().any(|byte| byte < 0x20) {
        return Err("Plugin ID contains invalid characters".to_string());
    }
    Ok(())
}

fn native_plugin_validate_secret_key(key: &str) -> Result<(), String> {
    if key.is_empty() {
        return Err("Plugin secret key cannot be empty".to_string());
    }
    if key.bytes().any(|byte| byte < 0x20) {
        return Err("Plugin secret key contains invalid characters".to_string());
    }
    Ok(())
}

fn native_plugin_terminal_response(
    call: plugin_runtime::PluginHostCall,
    terminal_tx: &mpsc::Sender<NativePluginTerminalRequest>,
) -> plugin_runtime::PluginResponse {
    let request_id = call.request_id.clone();
    let action = match native_plugin_terminal_action_from_call(&call) {
        Ok(action) => action,
        Err(error) => {
            return plugin_runtime::PluginResponse::error(
                request_id,
                plugin_runtime::PluginError::protocol("invalid_terminal_args", error),
            );
        }
    };
    let (response_tx, response_rx) = mpsc::channel();
    if terminal_tx
        .send(NativePluginTerminalRequest {
            request_id: request_id.clone(),
            action,
            response_tx,
        })
        .is_err()
    {
        return plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime(
                "terminal_host_unavailable",
                "Native plugin terminal host is unavailable",
            ),
        );
    }
    response_rx.recv().unwrap_or_else(|_| {
        plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime(
                "terminal_response_unavailable",
                "Native plugin terminal host closed before answering",
            ),
        )
    })
}

fn native_plugin_terminal_action_from_call(
    call: &plugin_runtime::PluginHostCall,
) -> Result<NativePluginTerminalAction, String> {
    match call.method.as_str() {
        "writeToActive" => {
            let text = call
                .args
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| "terminal.writeToActive requires args.text".to_string())?;
            Ok(NativePluginTerminalAction::WriteActive {
                text: text.to_string(),
            })
        }
        "writeToNode" => {
            let node_id = call
                .args
                .get("nodeId")
                .and_then(Value::as_str)
                .ok_or_else(|| "terminal.writeToNode requires args.nodeId".to_string())?;
            let text = call
                .args
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| "terminal.writeToNode requires args.text".to_string())?;
            Ok(NativePluginTerminalAction::WriteNode {
                node_id: node_id.to_string(),
                text: text.to_string(),
            })
        }
        "clearBuffer" => {
            let node_id = call
                .args
                .get("nodeId")
                .and_then(Value::as_str)
                .ok_or_else(|| "terminal.clearBuffer requires args.nodeId".to_string())?;
            Ok(NativePluginTerminalAction::ClearBuffer {
                node_id: node_id.to_string(),
            })
        }
        "openTelnet" => {
            let host = call
                .args
                .get("host")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|host| !host.is_empty())
                .ok_or_else(|| "Telnet host cannot be empty".to_string())?;
            let port = call
                .args
                .get("port")
                .and_then(Value::as_u64)
                .and_then(|port| u16::try_from(port).ok())
                .filter(|port| *port > 0)
                .unwrap_or(23);
            Ok(NativePluginTerminalAction::OpenTelnet {
                host: host.to_string(),
                port,
            })
        }
        method => Err(format!("Unsupported terminal host call: {method}")),
    }
}

fn native_plugin_apply_input_interceptors(
    bytes: &[u8],
    hooks: &[super::plugin_host::NativePluginRuntimeTerminalHookContribution],
    runtime_host: Arc<tokio::sync::Mutex<plugin_runtime::NativePluginRuntimeHost>>,
    runtime: Arc<tokio::runtime::Runtime>,
    host_api_resolver: plugin_runtime::NativeHostApiResolver,
) -> TerminalInputInterceptorResult {
    native_plugin_reduce_input_interceptors(bytes, hooks, |hook, args| {
        // The UI thread waits only for the hook budget. A busy runtime, timeout,
        // transport error, or malformed response falls through to fail-open.
        let dispatch = runtime.block_on(async {
            tokio::time::timeout(NATIVE_PLUGIN_TERMINAL_HOOK_TIMEOUT, async {
                let mut host = runtime_host.lock().await;
                host.set_host_api_resolver(host_api_resolver.clone());
                host.dispatch_command(
                    &hook.plugin_id,
                    hook.command.clone(),
                    args.clone(),
                    NATIVE_PLUGIN_TERMINAL_HOOK_TIMEOUT,
                )
                .await
            })
            .await
        });
        let Ok(Ok(dispatch)) = dispatch else {
            return None;
        };
        match dispatch.response.result {
            PluginResponseResult::Ok { value } => Some(value),
            PluginResponseResult::Error { .. } => None,
        }
    })
}

fn native_plugin_apply_output_processors(
    bytes: &[u8],
    hooks: &[super::plugin_host::NativePluginRuntimeTerminalHookContribution],
    runtime_host: Arc<tokio::sync::Mutex<plugin_runtime::NativePluginRuntimeHost>>,
    runtime: Arc<tokio::runtime::Runtime>,
    host_api_resolver: plugin_runtime::NativeHostApiResolver,
) -> Vec<u8> {
    native_plugin_reduce_output_processors(bytes, hooks, |hook, args| {
        // Output processors are allowed to transform display bytes, but timeout
        // and error semantics preserve the current byte stream to avoid terminal
        // corruption.
        let dispatch = runtime.block_on(async {
            tokio::time::timeout(NATIVE_PLUGIN_TERMINAL_HOOK_TIMEOUT, async {
                let mut host = runtime_host.lock().await;
                host.set_host_api_resolver(host_api_resolver.clone());
                host.dispatch_command(
                    &hook.plugin_id,
                    hook.command.clone(),
                    args.clone(),
                    NATIVE_PLUGIN_TERMINAL_HOOK_TIMEOUT,
                )
                .await
            })
            .await
        });
        let Ok(Ok(dispatch)) = dispatch else {
            return None;
        };
        match dispatch.response.result {
            PluginResponseResult::Ok { value } => Some(value),
            PluginResponseResult::Error { .. } => None,
        }
    })
}

fn native_plugin_reduce_input_interceptors<F>(
    bytes: &[u8],
    hooks: &[super::plugin_host::NativePluginRuntimeTerminalHookContribution],
    mut dispatch: F,
) -> TerminalInputInterceptorResult
where
    F: FnMut(
        &super::plugin_host::NativePluginRuntimeTerminalHookContribution,
        Value,
    ) -> Option<Value>,
{
    let mut current = String::from_utf8_lossy(bytes).to_string();
    for hook in hooks {
        let args = json!({
            "registrationId": hook.registration_id.clone(),
            "data": current.clone(),
            "text": current.clone(),
            "bytes": current.as_bytes().to_vec(),
        });
        let Some(value) = dispatch(hook, args) else {
            continue;
        };
        if value.is_null() {
            return TerminalInputInterceptorResult::Suppress;
        }
        if let Some(next) = native_plugin_terminal_hook_text_value(&value) {
            current = next;
        }
    }

    TerminalInputInterceptorResult::Continue(current.into_bytes())
}

fn native_plugin_reduce_output_processors<F>(
    bytes: &[u8],
    hooks: &[super::plugin_host::NativePluginRuntimeTerminalHookContribution],
    mut dispatch: F,
) -> Vec<u8>
where
    F: FnMut(
        &super::plugin_host::NativePluginRuntimeTerminalHookContribution,
        Value,
    ) -> Option<Value>,
{
    let mut current = bytes.to_vec();
    for hook in hooks {
        let args = json!({
            "registrationId": hook.registration_id.clone(),
            "bytes": current.clone(),
            "data": String::from_utf8_lossy(&current).to_string(),
        });
        let Some(value) = dispatch(hook, args) else {
            continue;
        };
        if let Some(next) = native_plugin_terminal_hook_bytes_value(&value) {
            current = next;
        }
    }
    current
}

fn native_plugin_terminal_hook_host_api_resolver() -> plugin_runtime::NativeHostApiResolver {
    Arc::new(|_plugin_id, _permissions, call| {
        // Terminal hooks run on the input budget; host APIs that bounce through
        // Workspace UI queues would make the timeout unenforceable.
        Some(plugin_runtime::PluginResponse::error(
            call.request_id,
            plugin_runtime::PluginError::runtime(
                "terminal_hook_host_api_unavailable",
                "Host APIs are unavailable while a terminal input hook is running",
            ),
        ))
    })
}

fn native_plugin_terminal_hook_text_value(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return Some(text.to_string());
    }
    value
        .get("data")
        .or_else(|| value.get("text"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn native_plugin_terminal_hook_bytes_value(value: &Value) -> Option<Vec<u8>> {
    if value.is_null() {
        return None;
    }
    if let Some(text) = value.as_str() {
        return Some(text.as_bytes().to_vec());
    }
    if let Some(bytes) = value.as_array() {
        return native_plugin_u8_array(bytes);
    }
    if let Some(bytes) = value.get("bytes").and_then(Value::as_array) {
        return native_plugin_u8_array(bytes);
    }
    value
        .get("data")
        .or_else(|| value.get("text"))
        .and_then(Value::as_str)
        .map(|text| text.as_bytes().to_vec())
}

fn native_plugin_u8_array(values: &[Value]) -> Option<Vec<u8>> {
    values
        .iter()
        .map(|value| value.as_u64().and_then(|byte| u8::try_from(byte).ok()))
        .collect()
}

struct NativePluginBackendAdapters<'a> {
    permissions: &'a plugin_runtime::PluginPermissionSet,
    sftp_router: &'a NodeRouter,
    sftp_runtime: &'a Arc<tokio::runtime::Runtime>,
    forwarding_registry: &'a ForwardingRegistry,
    forwarding_runtime: &'a Arc<tokio::runtime::Runtime>,
    transfer_manager: &'a Arc<SftpTransferManager>,
}

fn native_plugin_api_invoke_response(
    snapshot: &NativePluginHostApiSnapshot,
    plugin_id: &str,
    call: plugin_runtime::PluginHostCall,
    adapters: NativePluginBackendAdapters<'_>,
) -> plugin_runtime::PluginResponse {
    let request_id = call.request_id.clone();
    let Some(command) = call.args.get("command").and_then(Value::as_str) else {
        return plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol(
                "invalid_backend_command",
                "Native plugin api.invoke requires args.command",
            ),
        );
    };
    let declared_commands = native_plugin_declared_api_commands(snapshot, plugin_id);
    if !declared_commands.contains(command) {
        return plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol(
                "backend_command_not_whitelisted",
                format!(
                    "Command \"{command}\" not whitelisted in manifest contributes.apiCommands"
                ),
            ),
        );
    }

    native_plugin_backend_command_response(
        snapshot,
        request_id,
        command,
        call.args.get("args"),
        adapters,
    )
}

fn native_plugin_declared_api_commands(
    snapshot: &NativePluginHostApiSnapshot,
    plugin_id: &str,
) -> HashSet<String> {
    snapshot
        .registry
        .contributions()
        .api_commands
        .iter()
        .filter(|command| command.plugin_id == plugin_id)
        .map(|command| command.command.clone())
        .collect()
}

fn native_plugin_backend_command_response(
    snapshot: &NativePluginHostApiSnapshot,
    request_id: String,
    command: &str,
    args: Option<&Value>,
    adapters: NativePluginBackendAdapters<'_>,
) -> plugin_runtime::PluginResponse {
    let backend_args = args.cloned().unwrap_or_else(|| json!({}));
    match command {
        // Tauri permits plugins to invoke declared commands directly. Native
        // exposes only commands that already have a Workspace-owned adapter so
        // the plugin bridge cannot bypass Rust capability checks.
        NATIVE_PLUGIN_API_COMMAND_SSH_POOL_STATS => {
            plugin_runtime::PluginResponse::ok(request_id, snapshot.pool_stats.clone())
        }
        NATIVE_PLUGIN_API_COMMAND_LIST_CONNECTIONS => {
            plugin_runtime::PluginResponse::ok(request_id, json!(snapshot.connections.clone()))
        }
        NATIVE_PLUGIN_API_COMMAND_GET_APP_VERSION => {
            plugin_runtime::PluginResponse::ok(request_id, json!(env!("CARGO_PKG_VERSION")))
        }
        NATIVE_PLUGIN_API_COMMAND_GET_SYSTEM_INFO => {
            plugin_runtime::PluginResponse::ok(request_id, native_plugin_system_info())
        }
        NATIVE_PLUGIN_API_COMMAND_SFTP_CANCEL_TRANSFER
        | NATIVE_PLUGIN_API_COMMAND_SFTP_PAUSE_TRANSFER
        | NATIVE_PLUGIN_API_COMMAND_SFTP_RESUME_TRANSFER
        | NATIVE_PLUGIN_API_COMMAND_SFTP_TRANSFER_STATS => native_plugin_transfer_backend_response(
            request_id,
            command,
            &backend_args,
            adapters.transfer_manager,
        ),
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_INIT
        | NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_LIST_DIR
        | NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_STAT
        | NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_PREVIEW
        | NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_WRITE
        | NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DOWNLOAD
        | NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_UPLOAD
        | NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_MKDIR
        | NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DELETE
        | NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DELETE_RECURSIVE
        | NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_RENAME
        | NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DOWNLOAD_DIR
        | NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_UPLOAD_DIR
        | NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_TAR_PROBE
        | NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_TAR_UPLOAD
        | NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_TAR_DOWNLOAD => native_plugin_sftp_response(
            plugin_runtime::PluginHostCall {
                request_id,
                namespace: "sftp".to_string(),
                method: native_plugin_sftp_backend_method(command).to_string(),
                args: backend_args,
            },
            adapters.permissions,
            adapters.sftp_router,
            adapters.sftp_runtime,
            Some(adapters.transfer_manager),
        ),
        NATIVE_PLUGIN_API_COMMAND_LIST_PORT_FORWARDS
        | NATIVE_PLUGIN_API_COMMAND_CREATE_PORT_FORWARD
        | NATIVE_PLUGIN_API_COMMAND_STOP_PORT_FORWARD
        | NATIVE_PLUGIN_API_COMMAND_DELETE_PORT_FORWARD
        | NATIVE_PLUGIN_API_COMMAND_RESTART_PORT_FORWARD
        | NATIVE_PLUGIN_API_COMMAND_UPDATE_PORT_FORWARD
        | NATIVE_PLUGIN_API_COMMAND_GET_PORT_FORWARD_STATS
        | NATIVE_PLUGIN_API_COMMAND_STOP_ALL_FORWARDS => native_plugin_forward_response(
            plugin_runtime::PluginHostCall {
                request_id,
                namespace: "forward".to_string(),
                method: native_plugin_forward_backend_method(command).to_string(),
                args: backend_args,
            },
            adapters.permissions,
            adapters.forwarding_registry,
            adapters.forwarding_runtime,
            &snapshot.node_connection_ids.values().cloned().collect(),
        ),
        NATIVE_PLUGIN_API_COMMAND_PLUGIN_HTTP_REQUEST => {
            native_plugin_http_request_response(request_id, &backend_args, adapters.sftp_runtime)
        }
        _ => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime(
                "backend_command_not_supported",
                format!("Native plugin backend command \"{command}\" is not exposed"),
            ),
        ),
    }
}

fn native_plugin_system_info() -> Value {
    // Tauri exposes this as a lightweight host snapshot. Native keeps the
    // values synchronous so api.invoke cannot start an unowned background task.
    json!({
        "platform": native_plugin_platform_label(),
        "arch": std::env::consts::ARCH,
        "os": std::env::consts::OS,
        "family": std::env::consts::FAMILY,
    })
}

fn native_plugin_transfer_backend_response(
    request_id: String,
    command: &str,
    args: &Value,
    manager: &Arc<SftpTransferManager>,
) -> plugin_runtime::PluginResponse {
    let transfer_id = || {
        args.get("transferId")
            .or_else(|| args.get("transfer_id"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .ok_or_else(|| format!("{command} requires args.transferId"))
    };
    match command {
        NATIVE_PLUGIN_API_COMMAND_SFTP_CANCEL_TRANSFER => match transfer_id() {
            Ok(transfer_id) => {
                plugin_runtime::PluginResponse::ok(request_id, json!(manager.cancel(&transfer_id)))
            }
            Err(error) => plugin_runtime::PluginResponse::error(
                request_id,
                plugin_runtime::PluginError::protocol("invalid_transfer_id", error),
            ),
        },
        NATIVE_PLUGIN_API_COMMAND_SFTP_PAUSE_TRANSFER => match transfer_id() {
            Ok(transfer_id) => {
                plugin_runtime::PluginResponse::ok(request_id, json!(manager.pause(&transfer_id)))
            }
            Err(error) => plugin_runtime::PluginResponse::error(
                request_id,
                plugin_runtime::PluginError::protocol("invalid_transfer_id", error),
            ),
        },
        NATIVE_PLUGIN_API_COMMAND_SFTP_RESUME_TRANSFER => match transfer_id() {
            Ok(transfer_id) => {
                plugin_runtime::PluginResponse::ok(request_id, json!(manager.resume(&transfer_id)))
            }
            Err(error) => plugin_runtime::PluginResponse::error(
                request_id,
                plugin_runtime::PluginError::protocol("invalid_transfer_id", error),
            ),
        },
        NATIVE_PLUGIN_API_COMMAND_SFTP_TRANSFER_STATS => {
            let stats = manager.transfer_stats();
            plugin_runtime::PluginResponse::ok(
                request_id,
                json!({
                    "active": stats.active,
                    "queued": stats.queued,
                    "completed": stats.completed,
                }),
            )
        }
        _ => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime(
                "backend_command_not_supported",
                format!("Native plugin backend command \"{command}\" is not exposed"),
            ),
        ),
    }
}

fn native_plugin_http_request_response(
    request_id: String,
    args: &Value,
    runtime: &Arc<tokio::runtime::Runtime>,
) -> plugin_runtime::PluginResponse {
    let args = args.clone();
    let (response_tx, response_rx) = mpsc::channel();
    // The plugin host-call worker is synchronous. Run the actual HTTP request
    // on the long-lived async runtime so timeouts and socket cleanup are owned
    // by the backend, matching Tauri's command boundary.
    runtime.spawn(async move {
        let result = native_plugin_http_request_result(&args).await;
        let _ = response_tx.send(result);
    });

    match response_rx.recv() {
        Ok(Ok(value)) => plugin_runtime::PluginResponse::ok(request_id, value),
        Ok(Err(error)) => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime("plugin_http_request_error", error),
        ),
        Err(_) => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime(
                "plugin_http_request_unavailable",
                "Native plugin HTTP worker closed before returning a response",
            ),
        ),
    }
}

async fn native_plugin_http_request_result(args: &Value) -> Result<Value, String> {
    let url = args
        .get("url")
        .and_then(Value::as_str)
        .filter(|url| !url.is_empty())
        .ok_or_else(|| "plugin_http_request requires args.url".to_string())?;
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("Only HTTP and HTTPS URLs are supported".to_string());
    }
    let method = args
        .get("method")
        .and_then(Value::as_str)
        .filter(|method| !method.is_empty())
        .ok_or_else(|| "plugin_http_request requires args.method".to_string())?;
    let headers = native_plugin_http_headers_arg(args)?;
    let body = match args.get("bodyBase64").or_else(|| args.get("body_base64")) {
        Some(Value::String(encoded)) if !encoded.is_empty() => {
            let bytes = STANDARD
                .decode(encoded)
                .map_err(|error| format!("Invalid base64 request body: {error}"))?;
            if bytes.len() > NATIVE_PLUGIN_HTTP_BODY_LIMIT {
                return Err(format!(
                    "Request body too large: {} bytes (max {} bytes)",
                    bytes.len(),
                    NATIVE_PLUGIN_HTTP_BODY_LIMIT
                ));
            }
            Some(bytes)
        }
        _ => None,
    };

    let client = reqwest::Client::new();
    let mut builder = native_plugin_http_request_builder(&client, url, method, &headers, body)?;
    if let Some(timeout_ms) = args
        .get("timeoutMs")
        .or_else(|| args.get("timeout_ms"))
        .and_then(Value::as_u64)
        .filter(|timeout_ms| *timeout_ms > 0)
    {
        builder = builder.timeout(Duration::from_millis(timeout_ms));
    }

    let response = builder
        .send()
        .await
        .map_err(|error| format!("HTTP request failed: {error}"))?;
    let status = response.status().as_u16();
    let response_headers = response
        .headers()
        .iter()
        .map(|(key, value)| {
            (
                key.to_string(),
                value.to_str().unwrap_or_default().to_string(),
            )
        })
        .collect::<HashMap<_, _>>();
    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("Failed to read response body: {error}"))?;
    if bytes.len() > NATIVE_PLUGIN_HTTP_BODY_LIMIT {
        return Err(format!(
            "Response too large: {} bytes (max {} bytes)",
            bytes.len(),
            NATIVE_PLUGIN_HTTP_BODY_LIMIT
        ));
    }
    Ok(json!({
        "status": status,
        "headers": response_headers,
        "bodyBase64": STANDARD.encode(bytes),
    }))
}

fn native_plugin_http_headers_arg(args: &Value) -> Result<HashMap<String, String>, String> {
    let Some(headers) = args.get("headers") else {
        return Ok(HashMap::new());
    };
    if headers.is_null() {
        return Ok(HashMap::new());
    }
    serde_json::from_value(headers.clone())
        .map_err(|error| format!("plugin_http_request args.headers must be a string map: {error}"))
}

fn native_plugin_http_request_builder(
    client: &reqwest::Client,
    url: &str,
    method: &str,
    headers: &HashMap<String, String>,
    body: Option<Vec<u8>>,
) -> Result<reqwest::RequestBuilder, String> {
    let method = match method.to_uppercase().as_str() {
        "GET" => reqwest::Method::GET,
        "POST" => reqwest::Method::POST,
        "PUT" => reqwest::Method::PUT,
        "DELETE" => reqwest::Method::DELETE,
        "PATCH" => reqwest::Method::PATCH,
        "HEAD" => reqwest::Method::HEAD,
        "MKCOL" => reqwest::Method::from_bytes(b"MKCOL").map_err(|error| error.to_string())?,
        "PROPFIND" => {
            reqwest::Method::from_bytes(b"PROPFIND").map_err(|error| error.to_string())?
        }
        other => return Err(format!("Unsupported HTTP method: {other}")),
    };
    let mut builder = client.request(method, url);
    for (key, value) in headers {
        builder = builder.header(key.as_str(), value.as_str());
    }
    if let Some(body) = body {
        builder = builder.body(body);
    }
    Ok(builder)
}

fn native_plugin_sftp_backend_method(command: &str) -> &'static str {
    match command {
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_INIT => "init",
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_LIST_DIR => "listDir",
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_STAT => "stat",
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_PREVIEW => "preview",
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_WRITE => "write",
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DOWNLOAD => "download",
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_UPLOAD => "upload",
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_MKDIR => "mkdir",
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DELETE => "delete",
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DELETE_RECURSIVE => "deleteRecursive",
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_RENAME => "rename",
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DOWNLOAD_DIR => "downloadDir",
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_UPLOAD_DIR => "uploadDir",
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_TAR_PROBE => "tarProbe",
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_TAR_UPLOAD => "tarUpload",
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_TAR_DOWNLOAD => "tarDownload",
        _ => "unsupported",
    }
}

fn native_plugin_forward_backend_method(command: &str) -> &'static str {
    match command {
        NATIVE_PLUGIN_API_COMMAND_LIST_PORT_FORWARDS => "list",
        NATIVE_PLUGIN_API_COMMAND_CREATE_PORT_FORWARD => "create",
        NATIVE_PLUGIN_API_COMMAND_STOP_PORT_FORWARD => "stop",
        NATIVE_PLUGIN_API_COMMAND_DELETE_PORT_FORWARD => "delete",
        NATIVE_PLUGIN_API_COMMAND_RESTART_PORT_FORWARD => "restart",
        NATIVE_PLUGIN_API_COMMAND_UPDATE_PORT_FORWARD => "update",
        NATIVE_PLUGIN_API_COMMAND_GET_PORT_FORWARD_STATS => "getStats",
        NATIVE_PLUGIN_API_COMMAND_STOP_ALL_FORWARDS => "stopAll",
        _ => "unsupported",
    }
}

fn native_plugin_ui_registration_preflight_response(
    snapshot: &NativePluginHostApiSnapshot,
    plugin_id: &str,
    call: plugin_runtime::PluginHostCall,
    kind: plugin_runtime::PluginRegistrationKind,
) -> plugin_runtime::PluginResponse {
    let request_id = call.request_id.clone();
    match native_plugin_ui_registration_from_args(plugin_id, kind, &call.args).and_then(
        |registration| {
            let mut registry = snapshot.registry.clone();
            registry.apply_runtime_registration(registration)
        },
    ) {
        Ok(()) => plugin_runtime::PluginResponse::ok(
            request_id,
            json!({
                "queued": true,
            }),
        ),
        Err(error) => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol("invalid_declarative_ui", error),
        ),
    }
}

fn native_plugin_ui_open_tab_preflight_response(
    snapshot: &NativePluginHostApiSnapshot,
    plugin_id: &str,
    call: plugin_runtime::PluginHostCall,
) -> plugin_runtime::PluginResponse {
    let request_id = call.request_id.clone();
    let Some(tab_id) = native_plugin_ui_tab_id_arg(&call.args) else {
        return plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol(
                "invalid_plugin_tab",
                "Native plugin ui.openTab requires args.tabId",
            ),
        );
    };
    if snapshot
        .registry
        .contributions()
        .tab_contribution(plugin_id, &tab_id)
        .is_none()
    {
        return plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol(
                "plugin_tab_not_declared",
                format!("Tab \"{tab_id}\" not declared in manifest contributes.tabs"),
            ),
        );
    }
    plugin_runtime::PluginResponse::ok(request_id, json!({ "queued": true }))
}

fn native_plugin_ui_registration_from_args(
    plugin_id: &str,
    kind: plugin_runtime::PluginRegistrationKind,
    args: &Value,
) -> Result<plugin_runtime::PluginRegistration, String> {
    let view_id = match kind {
        plugin_runtime::PluginRegistrationKind::Tab => native_plugin_ui_tab_id_arg(args)
            .ok_or_else(|| "Native plugin ui.registerTabView requires args.tabId".to_string())?,
        plugin_runtime::PluginRegistrationKind::SidebarPanel => native_plugin_ui_panel_id_arg(args)
            .ok_or_else(|| {
                "Native plugin ui.registerSidebarPanel requires args.panelId".to_string()
            })?,
        _ => return Err("Unsupported native plugin declarative UI registration kind".to_string()),
    };
    let registration_id = args
        .get("registrationId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| native_plugin_ui_registration_id(kind, &view_id));
    Ok(plugin_runtime::PluginRegistration {
        registration_id,
        plugin_id: plugin_id.to_string(),
        kind,
        metadata: args.clone(),
    })
}

fn native_plugin_ui_tab_id_arg(args: &Value) -> Option<String> {
    args.get("tabId")
        .or_else(|| args.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn native_plugin_ui_panel_id_arg(args: &Value) -> Option<String> {
    args.get("panelId")
        .or_else(|| args.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn native_plugin_ui_registration_id(
    kind: plugin_runtime::PluginRegistrationKind,
    view_id: &str,
) -> String {
    let namespace = match kind {
        plugin_runtime::PluginRegistrationKind::Tab => "tab",
        plugin_runtime::PluginRegistrationKind::SidebarPanel => "sidebar-panel",
        _ => "view",
    };
    format!("ctx.ui.{namespace}:{view_id}")
}

fn native_plugin_returnable_host_api_response(
    snapshot: &NativePluginHostApiSnapshot,
    plugin_id: &str,
    call: plugin_runtime::PluginHostCall,
) -> Option<plugin_runtime::PluginResponse> {
    match (call.namespace.as_str(), call.method.as_str()) {
        ("api", "invoke") => Some(plugin_runtime::PluginResponse::error(
            call.request_id,
            plugin_runtime::PluginError::runtime(
                "backend_adapter_unavailable",
                "api.invoke is resolved by the Workspace backend adapter",
            ),
        )),
        ("ui", "registerTabView") => Some(native_plugin_ui_registration_preflight_response(
            snapshot,
            plugin_id,
            call,
            plugin_runtime::PluginRegistrationKind::Tab,
        )),
        ("ui", "registerSidebarPanel") => Some(native_plugin_ui_registration_preflight_response(
            snapshot,
            plugin_id,
            call,
            plugin_runtime::PluginRegistrationKind::SidebarPanel,
        )),
        ("ui", "openTab") => Some(native_plugin_ui_open_tab_preflight_response(
            snapshot, plugin_id, call,
        )),
        ("app", "getTheme") => Some(plugin_runtime::PluginResponse::ok(
            call.request_id,
            native_plugin_theme_snapshot(&snapshot.theme_name),
        )),
        ("app", "getSettings") => {
            let Some(category) = call.args.get("category").and_then(Value::as_str) else {
                return Some(plugin_runtime::PluginResponse::error(
                    call.request_id,
                    plugin_runtime::PluginError::protocol(
                        "invalid_settings_category",
                        "Native plugin app.getSettings requires args.category",
                    ),
                ));
            };
            Some(plugin_runtime::PluginResponse::ok(
                call.request_id,
                native_plugin_settings_section(&snapshot.settings, category),
            ))
        }
        ("app", "getVersion") => Some(plugin_runtime::PluginResponse::ok(
            call.request_id,
            json!(env!("CARGO_PKG_VERSION")),
        )),
        ("app", "getPlatform") => Some(plugin_runtime::PluginResponse::ok(
            call.request_id,
            json!(native_plugin_platform_label()),
        )),
        ("app", "getLocale") => Some(plugin_runtime::PluginResponse::ok(
            call.request_id,
            json!(snapshot.locale),
        )),
        ("app", "getPoolStats") => Some(plugin_runtime::PluginResponse::ok(
            call.request_id,
            snapshot.pool_stats.clone(),
        )),
        ("connections", "getAll") => Some(plugin_runtime::PluginResponse::ok(
            call.request_id,
            json!(snapshot.connections),
        )),
        ("connections", "get") => {
            let Some(connection_id) = call.args.get("connectionId").and_then(Value::as_str) else {
                return Some(plugin_runtime::PluginResponse::error(
                    call.request_id,
                    plugin_runtime::PluginError::protocol(
                        "invalid_connection_id",
                        "Native plugin connections.get requires args.connectionId",
                    ),
                ));
            };
            let connection = snapshot
                .connections
                .iter()
                .find(|connection| {
                    connection.get("id").and_then(Value::as_str) == Some(connection_id)
                })
                .cloned()
                .unwrap_or(Value::Null);
            Some(plugin_runtime::PluginResponse::ok(
                call.request_id,
                connection,
            ))
        }
        ("connections", "getState") => {
            let Some(connection_id) = call.args.get("connectionId").and_then(Value::as_str) else {
                return Some(plugin_runtime::PluginResponse::error(
                    call.request_id,
                    plugin_runtime::PluginError::protocol(
                        "invalid_connection_id",
                        "Native plugin connections.getState requires args.connectionId",
                    ),
                ));
            };
            let state = snapshot
                .connection_states
                .get(connection_id)
                .cloned()
                .unwrap_or(Value::Null);
            Some(plugin_runtime::PluginResponse::ok(call.request_id, state))
        }
        ("connections", "getByNode") => {
            let Some(node_id) = call.args.get("nodeId").and_then(Value::as_str) else {
                return Some(plugin_runtime::PluginResponse::error(
                    call.request_id,
                    plugin_runtime::PluginError::protocol(
                        "invalid_node_id",
                        "Native plugin connections.getByNode requires args.nodeId",
                    ),
                ));
            };
            let connection = snapshot
                .node_connection_ids
                .get(node_id)
                .and_then(|connection_id| {
                    snapshot.connections.iter().find(|connection| {
                        connection.get("id").and_then(Value::as_str) == Some(connection_id.as_str())
                    })
                })
                .cloned()
                .unwrap_or(Value::Null);
            Some(plugin_runtime::PluginResponse::ok(
                call.request_id,
                connection,
            ))
        }
        ("sessions", "getTree") => Some(plugin_runtime::PluginResponse::ok(
            call.request_id,
            json!(snapshot.session_tree),
        )),
        ("sessions", "getActiveNodes") => Some(plugin_runtime::PluginResponse::ok(
            call.request_id,
            native_plugin_active_session_nodes(&snapshot.session_tree),
        )),
        ("sessions", "getNodeState") => {
            let Some(node_id) = call.args.get("nodeId").and_then(Value::as_str) else {
                return Some(plugin_runtime::PluginResponse::error(
                    call.request_id,
                    plugin_runtime::PluginError::protocol(
                        "invalid_node_id",
                        "Native plugin sessions.getNodeState requires args.nodeId",
                    ),
                ));
            };
            let state = snapshot
                .session_node_states
                .get(node_id)
                .map(|state| json!(state))
                .unwrap_or(Value::Null);
            Some(plugin_runtime::PluginResponse::ok(call.request_id, state))
        }
        ("eventLog", "getEntries") => Some(plugin_runtime::PluginResponse::ok(
            call.request_id,
            native_plugin_filtered_event_log_entries(&snapshot.event_log_entries, &call.args),
        )),
        ("terminal", "getActiveTarget") => Some(plugin_runtime::PluginResponse::ok(
            call.request_id,
            snapshot.active_terminal_target.clone(),
        )),
        ("terminal", "getNodeBuffer") => {
            let Some(node_id) = call.args.get("nodeId").and_then(Value::as_str) else {
                return Some(plugin_runtime::PluginResponse::error(
                    call.request_id,
                    plugin_runtime::PluginError::protocol(
                        "invalid_node_id",
                        "Native plugin terminal.getNodeBuffer requires args.nodeId",
                    ),
                ));
            };
            let value = snapshot
                .terminal_nodes
                .get(node_id)
                .map(|terminal| json!(terminal.buffer))
                .unwrap_or(Value::Null);
            Some(plugin_runtime::PluginResponse::ok(call.request_id, value))
        }
        ("terminal", "getNodeSelection") => {
            let Some(node_id) = call.args.get("nodeId").and_then(Value::as_str) else {
                return Some(plugin_runtime::PluginResponse::error(
                    call.request_id,
                    plugin_runtime::PluginError::protocol(
                        "invalid_node_id",
                        "Native plugin terminal.getNodeSelection requires args.nodeId",
                    ),
                ));
            };
            let value = snapshot
                .terminal_nodes
                .get(node_id)
                .and_then(|terminal| terminal.selection.clone())
                .map(Value::String)
                .unwrap_or(Value::Null);
            Some(plugin_runtime::PluginResponse::ok(call.request_id, value))
        }
        ("terminal", "search") => Some(native_plugin_terminal_search_response(
            call.request_id,
            &snapshot.terminal_nodes,
            call.args,
        )),
        ("terminal", "getScrollBuffer") => Some(native_plugin_terminal_scroll_buffer_response(
            call.request_id,
            &snapshot.terminal_nodes,
            call.args,
        )),
        ("terminal", "getBufferSize") => Some(native_plugin_terminal_buffer_size_response(
            call.request_id,
            &snapshot.terminal_nodes,
            call.args,
        )),
        ("ui", "getLayout") => Some(plugin_runtime::PluginResponse::ok(
            call.request_id,
            snapshot.layout.clone(),
        )),
        ("events", "emit") => Some(
            match native_plugin_custom_event_from_args(plugin_id, call.args) {
                Ok((event_key, _payload)) => plugin_runtime::PluginResponse::ok(
                    call.request_id,
                    json!({
                        "emitted": true,
                        "event": event_key,
                    }),
                ),
                Err(error) => plugin_runtime::PluginResponse::error(
                    call.request_id,
                    plugin_runtime::PluginError::protocol("invalid_plugin_event", error),
                ),
            },
        ),
        ("i18n", "getLanguage") => Some(plugin_runtime::PluginResponse::ok(
            call.request_id,
            json!(snapshot.locale),
        )),
        ("i18n", "t") => {
            let Some(key) = call.args.get("key").and_then(Value::as_str) else {
                return Some(plugin_runtime::PluginResponse::error(
                    call.request_id,
                    plugin_runtime::PluginError::protocol(
                        "invalid_i18n_key",
                        "Native plugin i18n.t requires args.key",
                    ),
                ));
            };
            Some(plugin_runtime::PluginResponse::ok(
                call.request_id,
                json!(native_plugin_i18n_translate(&snapshot.i18n, plugin_id, key)),
            ))
        }
        ("settings", "get") => {
            let Some(key) = call.args.get("key").and_then(Value::as_str) else {
                return Some(plugin_runtime::PluginResponse::error(
                    call.request_id,
                    plugin_runtime::PluginError::protocol(
                        "invalid_plugin_setting_key",
                        "Native plugin settings.get requires args.key",
                    ),
                ));
            };
            // Native plugin settings are declaration-backed. This intentionally
            // uses the same registry path as manifest-rendered settings controls
            // so runtime plugins cannot create a parallel config namespace.
            let value = snapshot
                .registry
                .plugin_setting_value(plugin_id, key)
                .unwrap_or(serde_json::Value::Null);
            Some(plugin_runtime::PluginResponse::ok(call.request_id, value))
        }
        ("settings", "exportSyncableSettings") => {
            let normalized = native_normalize_syncable_settings_payload(
                &native_syncable_settings_payload(&snapshot.settings),
            );
            Some(plugin_runtime::PluginResponse::ok(
                call.request_id,
                json!({
                    "revision": native_syncable_settings_revision(&normalized.payload),
                    "exportedAt": Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
                    "payload": normalized.payload,
                    "warnings": normalized.warnings,
                }),
            ))
        }
        ("settings", "applySyncableSettings") => {
            let normalized = native_normalize_syncable_settings_payload(
                &native_syncable_settings_payload_arg(call.args),
            );
            Some(plugin_runtime::PluginResponse::ok(
                call.request_id,
                json!({
                    "revision": native_syncable_settings_revision(&normalized.payload),
                    "appliedPayload": normalized.payload,
                    "warnings": normalized.warnings,
                }),
            ))
        }
        ("storage", "get") => {
            let Some(key) = call.args.get("key").and_then(serde_json::Value::as_str) else {
                return Some(plugin_runtime::PluginResponse::error(
                    call.request_id,
                    plugin_runtime::PluginError::protocol(
                        "invalid_storage_key",
                        "Native plugin storage.get requires args.key",
                    ),
                ));
            };
            // Tauri localStorage-backed plugin storage returns null for missing
            // or unreadable JSON. Native mirrors that through a scoped registry
            // lookup and returns the raw JSON value to the process runtime.
            let value = snapshot
                .registry
                .plugin_storage_value(plugin_id, key)
                .unwrap_or(serde_json::Value::Null);
            Some(plugin_runtime::PluginResponse::ok(call.request_id, value))
        }
        _ => None,
    }
}

fn native_plugin_connection_snapshot(connection: &ConnectionInfo) -> Value {
    // Tauri pluginUtils.toSnapshot exposes this exact read-only projection from
    // SshConnectionInfo. Native derives terminal ids from registry consumers so
    // the plugin never receives transport handles, auth material, or pool keys.
    json!({
        "id": connection.connection_id,
        "host": connection.host,
        "port": connection.port,
        "username": connection.username,
        "state": native_plugin_connection_state(&connection.state),
        "refCount": connection.ref_count,
        "keepAlive": connection.keep_alive,
        "createdAt": native_plugin_connection_time(connection.created_at),
        "lastActive": native_plugin_connection_time(connection.last_active_at),
        "terminalIds": native_plugin_connection_terminal_ids(&connection.consumers),
        "parentConnectionId": connection.parent_connection_id,
    })
}

fn native_plugin_connection_state(state: &ConnectionState) -> Value {
    match state {
        ConnectionState::Connecting => json!("connecting"),
        ConnectionState::Active => json!("active"),
        ConnectionState::Idle => json!("idle"),
        ConnectionState::LinkDown => json!("link_down"),
        ConnectionState::Reconnecting => json!("reconnecting"),
        ConnectionState::Disconnecting => json!("disconnecting"),
        ConnectionState::Disconnected => json!("disconnected"),
        ConnectionState::Error(error) => json!({ "error": error }),
    }
}

fn native_plugin_connection_time(time: std::time::SystemTime) -> String {
    DateTime::<Utc>::from(time).to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn native_plugin_connection_terminal_ids(consumers: &[ConnectionConsumer]) -> Vec<String> {
    let mut terminal_ids = consumers
        .iter()
        .filter_map(|consumer| match consumer {
            ConnectionConsumer::Terminal(id) => Some(id.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    terminal_ids.sort();
    terminal_ids
}

fn native_plugin_session_tree_from_nodes(
    mut nodes: Vec<NodeTreeSnapshotNode>,
    titles: &HashMap<String, String>,
    terminal_ids_by_node: &HashMap<String, Vec<String>>,
) -> Vec<Value> {
    nodes.sort_by_key(|node| (node.depth, node.created_at_ms, node.id.0.clone()));
    nodes
        .into_iter()
        .map(|node| native_plugin_session_node_snapshot(node, titles, terminal_ids_by_node))
        .collect()
}

fn native_plugin_session_node_snapshot(
    node: NodeTreeSnapshotNode,
    titles: &HashMap<String, String>,
    terminal_ids_by_node: &HashMap<String, Vec<String>>,
) -> Value {
    let node_id = node.id.0.clone();
    let mut terminal_ids = terminal_ids_by_node
        .get(&node_id)
        .cloned()
        .or_else(|| {
            node.terminal_session_id
                .clone()
                .map(|session_id| vec![session_id])
        })
        .unwrap_or_default();
    terminal_ids.sort();
    terminal_ids.dedup();
    let connection_state = native_plugin_session_connection_state(&node.state, terminal_ids.len());
    let label = titles
        .get(&node_id)
        .filter(|title| !title.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| format!("{}@{}", node.config.username, node.config.host));
    let mut value = json!({
        "id": node_id,
        "label": label,
        "host": node.config.host,
        "port": node.config.port,
        "username": node.config.username,
        "parentId": node.parent_id.map(|id| id.0),
        "childIds": node.children_ids.into_iter().map(|id| id.0).collect::<Vec<_>>(),
        "connectionState": connection_state,
        "connectionId": node.connection_id,
        "terminalIds": terminal_ids,
        "sftpSessionId": node.sftp_session_id,
    });
    if let (Some(error), Value::Object(fields)) = (node.state.error, &mut value) {
        fields.insert("errorMessage".to_string(), json!(error));
    }
    value
}

fn native_plugin_session_connection_state(
    state: &oxideterm_ssh::NodeState,
    terminal_count: usize,
) -> &'static str {
    if state.error.as_deref() == Some("Link down") {
        return "link-down";
    }
    match state.readiness {
        NodeReadiness::Ready => {
            if terminal_count > 0 {
                "active"
            } else {
                "connected"
            }
        }
        NodeReadiness::Connecting => "connecting",
        NodeReadiness::Error => "error",
        NodeReadiness::Disconnected => "idle",
    }
}

fn native_plugin_active_session_nodes(session_tree: &[Value]) -> Value {
    let active_nodes = session_tree
        .iter()
        .filter(|node| {
            matches!(
                node.get("connectionState").and_then(Value::as_str),
                Some("active" | "connected")
            )
        })
        .map(|node| {
            json!({
                "nodeId": node.get("id").and_then(Value::as_str).unwrap_or_default(),
                "sessionId": node
                    .get("terminalIds")
                    .and_then(Value::as_array)
                    .and_then(|terminal_ids| terminal_ids.first())
                    .cloned()
                    .unwrap_or(Value::Null),
                "connectionState": node
                    .get("connectionState")
                    .and_then(Value::as_str)
                    .unwrap_or("idle"),
            })
        })
        .collect::<Vec<_>>();
    json!(active_nodes)
}

fn native_plugin_session_state_map(tree: &Value) -> HashMap<String, String> {
    tree.as_array()
        .map(|nodes| native_plugin_session_state_map_from_nodes(nodes))
        .unwrap_or_default()
}

fn native_plugin_session_state_map_from_nodes(nodes: &[Value]) -> HashMap<String, String> {
    nodes
        .iter()
        .filter_map(|node| {
            let node_id = node.get("id").and_then(Value::as_str)?;
            let state = node.get("connectionState").and_then(Value::as_str)?;
            Some((node_id.to_string(), state.to_string()))
        })
        .collect()
}

fn native_plugin_event_log_entries<'a>(
    entries: impl Iterator<Item = &'a EventLogEntry>,
) -> Vec<Value> {
    entries
        .map(native_plugin_event_log_entry_snapshot)
        .collect()
}

fn native_plugin_event_log_entry_snapshot(entry: &EventLogEntry) -> Value {
    let mut snapshot = Map::new();
    snapshot.insert("id".to_string(), json!(entry.id));
    snapshot.insert(
        "timestamp".to_string(),
        json!(native_plugin_unix_ms(entry.timestamp)),
    );
    snapshot.insert(
        "severity".to_string(),
        json!(native_plugin_event_severity(entry.severity)),
    );
    snapshot.insert(
        "category".to_string(),
        json!(native_plugin_event_category(entry.category)),
    );
    if let Some(node_id) = &entry.node_id {
        snapshot.insert("nodeId".to_string(), json!(node_id));
    }
    if let Some(connection_id) = &entry.connection_id {
        snapshot.insert("connectionId".to_string(), json!(connection_id));
    }
    snapshot.insert("title".to_string(), json!(entry.title));
    if let Some(detail) = &entry.detail {
        snapshot.insert("detail".to_string(), json!(detail));
    }
    snapshot.insert("source".to_string(), json!(entry.source));
    Value::Object(snapshot)
}

fn native_plugin_filtered_event_log_entries(entries: &[Value], args: &Value) -> Value {
    let filter = args.get("filter").unwrap_or(args);
    let severity = filter.get("severity").and_then(Value::as_str);
    let category = filter.get("category").and_then(Value::as_str);
    let filtered = entries
        .iter()
        .filter(|entry| {
            severity.is_none_or(|severity| {
                entry.get("severity").and_then(Value::as_str) == Some(severity)
            }) && category.is_none_or(|category| {
                entry.get("category").and_then(Value::as_str) == Some(category)
            })
        })
        .cloned()
        .collect::<Vec<_>>();
    json!(filtered)
}

fn native_plugin_event_severity(severity: EventSeverity) -> &'static str {
    match severity {
        EventSeverity::Info => "info",
        EventSeverity::Warn => "warn",
        EventSeverity::Error => "error",
    }
}

fn native_plugin_event_category(category: EventCategory) -> &'static str {
    match category {
        EventCategory::Connection => "connection",
        EventCategory::Reconnect => "reconnect",
        EventCategory::Node => "node",
    }
}

fn native_plugin_unix_ms(time: std::time::SystemTime) -> u64 {
    time.duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

fn native_plugin_terminal_snapshots(
    workspace: &WorkspaceApp,
    connection_states: &HashMap<String, Value>,
    cx: &mut Context<WorkspaceApp>,
) -> (Value, HashMap<String, NativePluginTerminalNodeSnapshot>) {
    let mut terminal_nodes = HashMap::new();
    for (node_id, node) in &workspace.ssh_nodes {
        let Some(session_id) = node.terminal_ids.first().copied() else {
            continue;
        };
        let Some(pane) = native_plugin_pane_for_session(workspace, session_id) else {
            continue;
        };
        let pane = pane.read(cx);
        terminal_nodes.insert(
            node_id.0.clone(),
            NativePluginTerminalNodeSnapshot {
                buffer: pane.visible_text_snapshot(),
                selection: pane.selected_text_snapshot(),
                current_lines: pane.buffer_line_count(),
            },
        );
    }

    (
        native_plugin_active_terminal_target(workspace, connection_states),
        terminal_nodes,
    )
}

fn native_plugin_pane_for_session(
    workspace: &WorkspaceApp,
    session_id: TerminalSessionId,
) -> Option<gpui::Entity<oxideterm_gpui_terminal::TerminalPane>> {
    for tab in &workspace.tabs {
        let Some(root) = tab.root_pane.as_ref() else {
            continue;
        };
        let mut pane_ids = Vec::new();
        root.collect_pane_ids(&mut pane_ids);
        for pane_id in pane_ids {
            if root.session_id_for_pane(pane_id) == Some(session_id) {
                return workspace.panes.get(&pane_id).cloned();
            }
        }
    }
    None
}

fn native_plugin_active_terminal_target(
    workspace: &WorkspaceApp,
    connection_states: &HashMap<String, Value>,
) -> Value {
    let Some(session_id) = workspace.active_terminal_session_id() else {
        return Value::Null;
    };
    let terminal_type = workspace
        .active_tab()
        .map(|tab| {
            if tab.kind == TabKind::LocalTerminal {
                "local_terminal"
            } else {
                "terminal"
            }
        })
        .unwrap_or("terminal");

    if terminal_type == "local_terminal" {
        return json!({
            "sessionId": session_id.0.to_string(),
            "terminalType": "local_terminal",
            "nodeId": null,
            "connectionId": null,
            "connectionState": "active",
            "label": session_id.0.to_string(),
        });
    }

    let node_id = workspace.terminal_ssh_nodes.get(&session_id).cloned();
    let connection_id = node_id
        .as_ref()
        .and_then(|node_id| workspace.node_runtime_store.connection_id_for_node(node_id));
    let connection_state = connection_id
        .as_ref()
        .and_then(|connection_id| connection_states.get(connection_id))
        .map(native_plugin_terminal_state_label)
        .unwrap_or(Value::Null);
    let label = node_id
        .as_ref()
        .and_then(|node_id| workspace.ssh_nodes.get(node_id))
        .map(|node| node.title.clone())
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| session_id.0.to_string());

    // Tauri derives active terminal target from the pane registry and session
    // tree. Native uses the same visible ids but projects Rust error objects to
    // the plugin-facing `"error"` state string used by pluginContextFactory.
    json!({
        "sessionId": session_id.0.to_string(),
        "terminalType": "terminal",
        "nodeId": node_id.map(|node_id| node_id.0),
        "connectionId": connection_id,
        "connectionState": connection_state,
        "label": label,
    })
}

fn native_plugin_terminal_state_label(state: &Value) -> Value {
    if let Some(state) = state.as_str() {
        return json!(state);
    }
    if state.get("error").is_some() {
        return json!("error");
    }
    Value::Null
}

fn native_plugin_terminal_search_response(
    request_id: String,
    terminal_nodes: &HashMap<String, NativePluginTerminalNodeSnapshot>,
    args: Value,
) -> plugin_runtime::PluginResponse {
    let Some(node_id) = args.get("nodeId").and_then(Value::as_str) else {
        return plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol(
                "invalid_node_id",
                "Native plugin terminal.search requires args.nodeId",
            ),
        );
    };
    let Some(query) = args.get("query").and_then(Value::as_str) else {
        return plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol(
                "invalid_terminal_search_query",
                "Native plugin terminal.search requires args.query",
            ),
        );
    };
    let options = args.get("options").unwrap_or(&Value::Null);
    let search_options = native_plugin_terminal_search_options(query, options);
    let Some(terminal) = terminal_nodes.get(node_id) else {
        return plugin_runtime::PluginResponse::ok(
            request_id,
            json!({ "matches": [], "total_matches": 0 }),
        );
    };
    let search = native_plugin_terminal_search_matches(&terminal.buffer, &search_options);
    plugin_runtime::PluginResponse::ok(
        request_id,
        json!({
            "matches": search.matches,
            "total_matches": search.total_matches,
            "truncated": search.truncated,
            "error": search.error,
        }),
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NativePluginTerminalSearchOptions {
    query: String,
    case_sensitive: bool,
    regex: bool,
    whole_word: bool,
    max_matches: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NativePluginTerminalSearchResult {
    matches: Vec<Value>,
    total_matches: usize,
    truncated: bool,
    error: Option<String>,
}

fn native_plugin_terminal_search_options(
    query: &str,
    options: &Value,
) -> NativePluginTerminalSearchOptions {
    NativePluginTerminalSearchOptions {
        query: query.to_string(),
        case_sensitive: options
            .get("caseSensitive")
            .or_else(|| options.get("case_sensitive"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        regex: options
            .get("regex")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        whole_word: options
            .get("wholeWord")
            .or_else(|| options.get("whole_word"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        // Tauri's Rust SearchOptions defaults max_matches to 100 when the
        // plugin does not specify one through the JS context factory.
        max_matches: options
            .get("maxMatches")
            .or_else(|| options.get("max_matches"))
            .and_then(Value::as_u64)
            .unwrap_or(100) as usize,
    }
}

fn native_plugin_terminal_search_matches(
    buffer: &str,
    options: &NativePluginTerminalSearchOptions,
) -> NativePluginTerminalSearchResult {
    if options.query.is_empty() {
        return NativePluginTerminalSearchResult {
            matches: Vec::new(),
            total_matches: 0,
            truncated: false,
            error: None,
        };
    }

    let pattern = if options.regex {
        options.query.clone()
    } else if options.whole_word {
        format!(r"\b{}\b", regex::escape(&options.query))
    } else {
        regex::escape(&options.query)
    };

    let regex = match regex::RegexBuilder::new(&pattern)
        .case_insensitive(!options.case_sensitive)
        .build()
    {
        Ok(regex) => regex,
        Err(error) => {
            return NativePluginTerminalSearchResult {
                matches: Vec::new(),
                total_matches: 0,
                truncated: false,
                error: Some(format!("Invalid regex: {error}")),
            };
        }
    };

    let limit = if options.max_matches == 0 {
        usize::MAX
    } else {
        options.max_matches
    };
    let mut matches = Vec::new();
    let mut total_matches = 0usize;
    for (line_number, line) in buffer.lines().enumerate() {
        for matched in regex.find_iter(line) {
            total_matches += 1;
            if matches.len() < limit {
                // Tauri returns backend `HistorySearchMatch`/`SearchMatch`
                // payloads with snake_case fields; pluginContextFactory passes
                // them through as unknown values without camel-case mapping.
                matches.push(json!({
                    "line_number": line_number,
                    "column_start": matched.start(),
                    "column_end": matched.end(),
                    "matched_text": matched.as_str(),
                    "line_content": line,
                }));
            }
        }
    }

    NativePluginTerminalSearchResult {
        truncated: total_matches > matches.len(),
        matches,
        total_matches,
        error: None,
    }
}

fn native_plugin_terminal_scroll_buffer_response(
    request_id: String,
    terminal_nodes: &HashMap<String, NativePluginTerminalNodeSnapshot>,
    args: Value,
) -> plugin_runtime::PluginResponse {
    let Some(node_id) = args.get("nodeId").and_then(Value::as_str) else {
        return plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol(
                "invalid_node_id",
                "Native plugin terminal.getScrollBuffer requires args.nodeId",
            ),
        );
    };
    let start_line = args
        .get("startLine")
        .and_then(Value::as_u64)
        .unwrap_or_default() as usize;
    let count = args
        .get("count")
        .and_then(Value::as_u64)
        .unwrap_or(100)
        .min(1000) as usize;
    let Some(terminal) = terminal_nodes.get(node_id) else {
        return plugin_runtime::PluginResponse::ok(request_id, json!([]));
    };
    let lines = terminal
        .buffer
        .lines()
        .enumerate()
        .skip(start_line)
        .take(count)
        .map(|(line_number, text)| json!({ "text": text, "lineNumber": line_number }))
        .collect::<Vec<_>>();
    plugin_runtime::PluginResponse::ok(request_id, json!(lines))
}

fn native_plugin_terminal_buffer_size_response(
    request_id: String,
    terminal_nodes: &HashMap<String, NativePluginTerminalNodeSnapshot>,
    args: Value,
) -> plugin_runtime::PluginResponse {
    let Some(node_id) = args.get("nodeId").and_then(Value::as_str) else {
        return plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol(
                "invalid_node_id",
                "Native plugin terminal.getBufferSize requires args.nodeId",
            ),
        );
    };
    let current_lines = terminal_nodes
        .get(node_id)
        .map(|terminal| terminal.current_lines)
        .unwrap_or_default();
    plugin_runtime::PluginResponse::ok(
        request_id,
        json!({
            "currentLines": current_lines,
            "totalLines": current_lines,
            "maxLines": current_lines,
        }),
    )
}

fn native_plugin_layout_snapshot(
    sidebar_collapsed: bool,
    active_tab_id: Option<String>,
    tab_count: usize,
) -> Value {
    // Tauri exposes this exact app-store shape and freezes it before returning
    // to plugins. Native mirrors the field names so process runtimes can share
    // the same plugin-facing API contract.
    json!({
        "sidebarCollapsed": sidebar_collapsed,
        "activeTabId": active_tab_id,
        "tabCount": tab_count,
    })
}

fn native_plugin_platform_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    }
}

fn native_plugin_theme_is_dark(theme_name: &str) -> bool {
    !theme_name.to_ascii_lowercase().contains("light")
}

pub(super) fn native_plugin_theme_snapshot(theme_name: &str) -> Value {
    json!({
        "name": theme_name,
        "isDark": native_plugin_theme_is_dark(theme_name),
    })
}

fn native_plugin_custom_event_from_args(
    plugin_id: &str,
    args: Value,
) -> Result<(String, Value), String> {
    let event_name = args
        .get("name")
        .or_else(|| args.get("event"))
        .and_then(Value::as_str)
        .ok_or_else(|| "events.emit requires args.name".to_string())?;
    let owner_plugin_id = args
        .get("pluginId")
        .or_else(|| args.get("ownerPluginId"))
        .and_then(Value::as_str)
        .unwrap_or(plugin_id);
    let event_key =
        super::plugin_host::native_plugin_custom_event_key(owner_plugin_id, event_name)?;
    // Custom plugin events are scoped to the emitting plugin by default. The
    // payload names both the owner and public event name so subscribers do not
    // need to parse the internal routing key.
    Ok((
        event_key,
        json!({
            "pluginId": owner_plugin_id,
            "name": event_name,
            "payload": args.get("payload").cloned().unwrap_or(Value::Null),
        }),
    ))
}

#[derive(Clone, Debug, PartialEq)]
struct NativeSyncableSettingsNormalization {
    payload: Value,
    warnings: Vec<Value>,
}

fn native_syncable_settings_payload(settings: &Value) -> Value {
    let mut payload = Map::new();
    let mut appearance = Map::new();
    let mut terminal = Map::new();
    let mut reconnect = Map::new();

    if let Some(language) = settings
        .get("general")
        .and_then(|general| general.get("language"))
        .and_then(Value::as_str)
    {
        appearance.insert("language".to_string(), json!(language));
    }
    if let Some(ui_density) = settings
        .get("appearance")
        .and_then(|appearance| appearance.get("uiDensity"))
        .and_then(Value::as_str)
    {
        appearance.insert("uiDensity".to_string(), json!(ui_density));
    }
    if let Some(font_size) = settings
        .get("terminal")
        .and_then(|terminal| terminal.get("fontSize"))
        .and_then(Value::as_i64)
    {
        terminal.insert("fontSize".to_string(), json!(font_size));
    }
    if let Some(theme) = settings
        .get("terminal")
        .and_then(|terminal| terminal.get("theme"))
        .and_then(Value::as_str)
    {
        terminal.insert("theme".to_string(), json!(theme));
    }
    if let Some(auto_reconnect) = settings
        .get("reconnect")
        .and_then(|reconnect| reconnect.get("enabled"))
        .and_then(Value::as_bool)
    {
        reconnect.insert("autoReconnect".to_string(), json!(auto_reconnect));
    }

    if !appearance.is_empty() {
        payload.insert("appearance".to_string(), Value::Object(appearance));
    }
    if !terminal.is_empty() {
        payload.insert("terminal".to_string(), Value::Object(terminal));
    }
    if !reconnect.is_empty() {
        payload.insert("reconnect".to_string(), Value::Object(reconnect));
    }
    Value::Object(payload)
}

fn native_syncable_settings_payload_arg(args: Value) -> Value {
    // Process plugins usually pass `{ payload }`; accepting the raw payload too
    // keeps the protocol tolerant for early SDK/demo runtimes.
    args.get("payload")
        .filter(|payload| payload.is_object())
        .cloned()
        .unwrap_or(args)
}

fn native_normalize_syncable_settings_payload(
    payload: &Value,
) -> NativeSyncableSettingsNormalization {
    let mut normalized = Map::new();
    let mut warnings = Vec::new();

    if let Some(source) = payload.get("appearance").and_then(Value::as_object) {
        let mut appearance = Map::new();
        if let Some(language) = source.get("language") {
            if let Some(language) = language
                .as_str()
                .filter(|value| native_language_supported(value))
            {
                appearance.insert("language".to_string(), json!(language));
            } else if !language.is_null() {
                warnings.push(native_syncable_settings_warning(
                    "appearance.language",
                    "unsupported-language",
                    false,
                    format!(
                        "Unsupported language: {}",
                        native_syncable_warning_value(language)
                    ),
                    None,
                ));
            }
        }
        if let Some(ui_density) = source.get("uiDensity") {
            if let Some(ui_density) = ui_density
                .as_str()
                .filter(|value| native_ui_density_supported(value))
            {
                appearance.insert("uiDensity".to_string(), json!(ui_density));
            } else if !ui_density.is_null() {
                warnings.push(native_syncable_settings_warning(
                    "appearance.uiDensity",
                    "invalid-ui-density",
                    false,
                    format!(
                        "Unsupported ui density: {}",
                        native_syncable_warning_value(ui_density)
                    ),
                    None,
                ));
            }
        }
        if !appearance.is_empty() {
            normalized.insert("appearance".to_string(), Value::Object(appearance));
        }
    }

    if let Some(source) = payload.get("terminal").and_then(Value::as_object) {
        let mut terminal = Map::new();
        if let Some(font_size) = source.get("fontSize") {
            if let Some(font_size) = font_size.as_f64().filter(|value| value.is_finite()) {
                let normalized_font_size = (font_size.round() as i64).clamp(8, 32);
                terminal.insert("fontSize".to_string(), json!(normalized_font_size));
                if (normalized_font_size as f64 - font_size).abs() > f64::EPSILON {
                    warnings.push(native_syncable_settings_warning(
                        "terminal.fontSize",
                        "font-size-clamped",
                        true,
                        format!("Font size was clamped to {normalized_font_size}"),
                        Some(json!(normalized_font_size)),
                    ));
                }
            } else {
                warnings.push(native_syncable_settings_warning(
                    "terminal.fontSize",
                    "invalid-font-size",
                    false,
                    "Font size must be a finite number".to_string(),
                    None,
                ));
            }
        }
        if let Some(theme) = source.get("theme") {
            let theme = theme.as_str().map(str::trim).unwrap_or_default();
            if theme.is_empty() {
                warnings.push(native_syncable_settings_warning(
                    "terminal.theme",
                    "missing-theme",
                    false,
                    "Theme id cannot be empty".to_string(),
                    None,
                ));
            } else {
                terminal.insert("theme".to_string(), json!(theme));
            }
        }
        if !terminal.is_empty() {
            normalized.insert("terminal".to_string(), Value::Object(terminal));
        }
    }

    if let Some(source) = payload.get("reconnect").and_then(Value::as_object) {
        let mut reconnect = Map::new();
        if let Some(auto_reconnect) = source.get("autoReconnect") {
            if let Some(auto_reconnect) = auto_reconnect.as_bool() {
                reconnect.insert("autoReconnect".to_string(), json!(auto_reconnect));
            } else {
                warnings.push(native_syncable_settings_warning(
                    "reconnect.autoReconnect",
                    "invalid-auto-reconnect",
                    false,
                    "autoReconnect must be a boolean".to_string(),
                    None,
                ));
            }
        }
        if !reconnect.is_empty() {
            normalized.insert("reconnect".to_string(), Value::Object(reconnect));
        }
    }

    NativeSyncableSettingsNormalization {
        payload: Value::Object(normalized),
        warnings,
    }
}

fn native_apply_syncable_settings_payload(
    workspace: &mut WorkspaceApp,
    payload: &Value,
    cx: &mut Context<WorkspaceApp>,
) -> Result<(), String> {
    let language = payload
        .pointer("/appearance/language")
        .and_then(Value::as_str)
        .map(native_parse_language)
        .transpose()?;
    let ui_density = payload
        .pointer("/appearance/uiDensity")
        .and_then(Value::as_str)
        .map(native_parse_ui_density)
        .transpose()?;
    let font_size = payload
        .pointer("/terminal/fontSize")
        .and_then(Value::as_i64);
    let theme = payload
        .pointer("/terminal/theme")
        .and_then(Value::as_str)
        .map(str::to_string);
    let auto_reconnect = payload
        .pointer("/reconnect/autoReconnect")
        .and_then(Value::as_bool);

    if language.is_none()
        && ui_density.is_none()
        && font_size.is_none()
        && theme.is_none()
        && auto_reconnect.is_none()
    {
        return Ok(());
    }

    workspace.edit_settings(
        |settings| {
            if let Some(language) = language {
                settings.general.language = language;
            }
            if let Some(ui_density) = ui_density {
                settings.appearance.ui_density = ui_density;
            }
            if let Some(font_size) = font_size {
                settings.terminal.font_size = font_size;
            }
            if let Some(theme) = theme {
                settings.terminal.theme = theme;
            }
            if let Some(auto_reconnect) = auto_reconnect {
                settings.reconnect.enabled = auto_reconnect;
            }
        },
        cx,
    );
    Ok(())
}

fn native_syncable_settings_warning(
    path: &str,
    code: &str,
    applied: bool,
    message: String,
    normalized_value: Option<Value>,
) -> Value {
    let mut warning = Map::new();
    warning.insert("path".to_string(), json!(path));
    warning.insert("code".to_string(), json!(code));
    warning.insert("applied".to_string(), json!(applied));
    warning.insert("message".to_string(), json!(message));
    if let Some(normalized_value) = normalized_value {
        warning.insert("normalizedValue".to_string(), normalized_value);
    }
    Value::Object(warning)
}

fn native_language_supported(language: &str) -> bool {
    native_parse_language(language).is_ok()
}

fn native_ui_density_supported(ui_density: &str) -> bool {
    native_parse_ui_density(ui_density).is_ok()
}

fn native_parse_language(language: &str) -> Result<Language, String> {
    serde_json::from_value::<Language>(json!(language))
        .map_err(|_| format!("Unsupported language: {language}"))
}

fn native_parse_ui_density(ui_density: &str) -> Result<UiDensity, String> {
    serde_json::from_value::<UiDensity>(json!(ui_density))
        .map_err(|_| format!("Unsupported ui density: {ui_density}"))
}

fn native_syncable_warning_value(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string())
}

fn native_syncable_settings_revision(payload: &Value) -> String {
    let text = native_syncable_settings_json_string(payload);
    let mut hash = 2166136261u32;
    for byte in text.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(16777619);
    }
    format!("fnv1a-{hash:x}")
}

fn native_syncable_settings_json_string(value: &Value) -> String {
    match value {
        Value::Object(object) => {
            let keys = [
                "appearance",
                "language",
                "uiDensity",
                "terminal",
                "fontSize",
                "theme",
                "reconnect",
                "autoReconnect",
            ];
            let ordered = keys
                .iter()
                .filter_map(|key| object.get(*key).map(|value| (*key, value)))
                .chain(
                    object
                        .iter()
                        .filter(|(key, _)| !keys.contains(&key.as_str()))
                        .map(|(key, value)| (key.as_str(), value)),
                )
                .map(|(key, value)| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(key).unwrap_or_else(|_| "\"\"".to_string()),
                        native_syncable_settings_json_string(value)
                    )
                })
                .collect::<Vec<_>>();
            format!("{{{}}}", ordered.join(","))
        }
        Value::Array(values) => {
            let values = values
                .iter()
                .map(native_syncable_settings_json_string)
                .collect::<Vec<_>>();
            format!("[{}]", values.join(","))
        }
        Value::String(value) => serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string()),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Null => "null".to_string(),
    }
}

fn native_plugin_settings_section(settings: &Value, category: &str) -> Value {
    settings
        .get(category)
        .filter(|value| value.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}))
}

fn native_plugin_i18n_translate(i18n: &I18n, plugin_id: &str, key: &str) -> String {
    let full_key = format!("plugin.{plugin_id}.{key}");
    let translated = i18n.t(&full_key);
    // Tauri pluginI18nManager auto-prefixes plugin keys and falls back to the
    // raw plugin key when no bundle is loaded. Native keeps that contract while
    // plugin locale-bundle loading is completed in the rest of Phase 4.
    if translated == full_key {
        key.to_string()
    } else {
        translated
    }
}

fn native_plugin_notification_variant(severity: &str) -> TerminalNoticeVariant {
    match severity {
        "error" => TerminalNoticeVariant::Error,
        "warning" => TerminalNoticeVariant::Warning,
        _ => TerminalNoticeVariant::Default,
    }
}

fn native_plugin_progress_key(plugin_id: &str, registration_id: &str) -> String {
    format!("{plugin_id}:{registration_id}")
}

fn native_plugin_progress_is_done(value: &serde_json::Value) -> bool {
    ["done", "completed", "dismissed"]
        .iter()
        .any(|key| value.get(*key).and_then(serde_json::Value::as_bool) == Some(true))
}

fn native_plugin_progress_notice(
    plugin_id: &str,
    registration_id: &str,
    value: serde_json::Value,
) -> TerminalNotice {
    let title = value
        .get("title")
        .and_then(|value| value.as_str())
        .unwrap_or("Plugin progress")
        .to_string();
    let description = value
        .get("message")
        .or_else(|| value.get("description"))
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let progress = native_plugin_progress_percent(&value);
    let status_text = value
        .get("statusText")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .or_else(|| progress.map(|percent| format!("{percent:.0}%")));

    TerminalNotice {
        title: native_plugin_notice_title(plugin_id, title),
        description: description.or_else(|| Some(registration_id.to_string())),
        status_text,
        progress,
        variant: TerminalNoticeVariant::Default,
    }
}

fn native_plugin_progress_percent(value: &serde_json::Value) -> Option<f32> {
    if let Some(percent) = value
        .get("progress")
        .or_else(|| value.get("percent"))
        .and_then(serde_json::Value::as_f64)
    {
        return Some((percent as f32).clamp(0.0, 100.0));
    }

    let current = value
        .get("value")
        .or_else(|| value.get("current"))
        .and_then(serde_json::Value::as_f64)?;
    let total = value
        .get("total")
        .and_then(serde_json::Value::as_f64)
        .filter(|total| *total > 0.0)?;
    Some(((current / total) as f32 * 100.0).clamp(0.0, 100.0))
}

fn native_plugin_notice_title(plugin_id: &str, title: String) -> String {
    format!("{title} ({plugin_id})")
}

fn native_plugin_dialog_title(plugin_id: &str, title: &str) -> String {
    native_plugin_notice_title(plugin_id, title.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_plugin_permissions_cover_implemented_host_api_namespaces() {
        let permissions = native_process_plugin_permissions();
        assert!(
            permissions
                .capabilities
                .contains(&NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_READ.to_string())
        );
        assert!(
            permissions
                .capabilities
                .contains(&NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_WRITE.to_string())
        );
        assert!(
            permissions
                .capabilities
                .contains(&NATIVE_PLUGIN_CAPABILITY_NETWORK_FORWARD.to_string())
        );
        assert!(
            permissions
                .allowed_host_apis
                .contains(&"ui.showToast".to_string())
        );
        assert!(
            permissions
                .allowed_host_apis
                .contains(&"ui.showConfirm".to_string())
        );
        assert!(
            permissions
                .allowed_host_apis
                .contains(&"ui.showProgress".to_string())
        );
        assert!(
            permissions
                .allowed_host_apis
                .contains(&"ui.showNotification".to_string())
        );
        assert!(
            permissions
                .allowed_host_apis
                .contains(&"storage.set".to_string())
        );
        assert!(
            permissions
                .allowed_host_apis
                .contains(&"storage.remove".to_string())
        );
        assert!(
            permissions
                .allowed_host_apis
                .contains(&"storage.get".to_string())
        );
        assert!(
            permissions
                .allowed_host_apis
                .contains(&"app.getVersion".to_string())
        );
        assert!(
            permissions
                .allowed_host_apis
                .contains(&"app.getSettings".to_string())
        );
        assert!(
            permissions
                .allowed_host_apis
                .contains(&"app.refreshAfterExternalSync".to_string())
        );
        assert!(
            permissions
                .allowed_host_apis
                .contains(&"api.invoke".to_string())
        );
        assert!(
            permissions
                .allowed_host_apis
                .contains(&"connections.getAll".to_string())
        );
        assert!(
            permissions
                .allowed_host_apis
                .contains(&"connections.getByNode".to_string())
        );
        assert!(
            permissions
                .allowed_host_apis
                .contains(&"sessions.getTree".to_string())
        );
        assert!(
            permissions
                .allowed_host_apis
                .contains(&"sessions.getNodeState".to_string())
        );
        assert!(
            permissions
                .allowed_host_apis
                .contains(&"eventLog.getEntries".to_string())
        );
        assert!(
            permissions
                .allowed_host_apis
                .contains(&"terminal.getActiveTarget".to_string())
        );
        assert!(
            permissions
                .allowed_host_apis
                .contains(&"terminal.getBufferSize".to_string())
        );
        assert!(
            permissions
                .allowed_host_apis
                .contains(&"terminal.writeToActive".to_string())
        );
        assert!(
            permissions
                .allowed_host_apis
                .contains(&"terminal.clearBuffer".to_string())
        );
        assert!(
            permissions
                .allowed_host_apis
                .contains(&"terminal.openTelnet".to_string())
        );
        for api in [
            "sftp.listDir",
            "sftp.stat",
            "sftp.readFile",
            "sftp.writeFile",
            "sftp.mkdir",
            "sftp.delete",
            "sftp.rename",
            "forward.list",
            "forward.listSavedForwards",
            "forward.onSavedForwardsChange",
            "forward.exportSavedForwardsSnapshot",
            "forward.applySavedForwardsSnapshot",
            "forward.create",
            "forward.stop",
            "forward.stopAll",
            "forward.getStats",
            "sync.listSavedConnections",
            "sync.refreshSavedConnections",
            "sync.exportSavedConnectionsSnapshot",
            "sync.applySavedConnectionsSnapshot",
            "sync.getLocalSyncMetadata",
            "sync.preflightExport",
            "sync.exportOxide",
            "sync.validateOxide",
            "sync.previewImport",
            "sync.importOxide",
            "transfers.getAll",
            "transfers.getByNode",
            "transfers.onProgress",
            "transfers.onComplete",
            "transfers.onError",
            "profiler.getMetrics",
            "profiler.getHistory",
            "profiler.isRunning",
            "profiler.onMetrics",
            "ide.isOpen",
            "ide.getProject",
            "ide.getOpenFiles",
            "ide.getActiveFile",
            "ide.onFileOpen",
            "ide.onFileClose",
            "ide.onActiveFileChange",
            "ai.getConversations",
            "ai.getMessages",
            "ai.getActiveProvider",
            "ai.getAvailableModels",
            "ai.onMessage",
        ] {
            assert!(permissions.allowed_host_apis.contains(&api.to_string()));
        }
        assert!(
            permissions
                .allowed_host_apis
                .contains(&"events.emit".to_string())
        );
        assert!(
            permissions
                .allowed_host_apis
                .contains(&"i18n.t".to_string())
        );
        assert!(
            permissions
                .allowed_host_apis
                .contains(&"settings.get".to_string())
        );
        assert!(
            permissions
                .allowed_host_apis
                .contains(&"settings.set".to_string())
        );
        assert!(
            permissions
                .allowed_host_apis
                .contains(&"settings.exportSyncableSettings".to_string())
        );
        assert!(
            permissions
                .allowed_host_apis
                .contains(&"settings.applySyncableSettings".to_string())
        );
        assert!(
            permissions
                .allowed_host_apis
                .contains(&"ui.getLayout".to_string())
        );
        for api in [
            "ui.registerTabView",
            "ui.registerSidebarPanel",
            "ui.openTab",
        ] {
            assert!(permissions.allowed_host_apis.contains(&api.to_string()));
        }
        assert!(
            permissions
                .allowed_host_apis
                .contains(&"secrets.get".to_string())
        );
        assert!(
            permissions
                .allowed_host_apis
                .contains(&"secrets.getMany".to_string())
        );
        assert!(
            permissions
                .allowed_host_apis
                .contains(&"secrets.set".to_string())
        );
        assert!(
            permissions
                .allowed_host_apis
                .contains(&"secrets.has".to_string())
        );
        assert!(
            permissions
                .allowed_host_apis
                .contains(&"secrets.delete".to_string())
        );
    }

    #[test]
    fn sync_host_call_returns_saved_connection_snapshots_and_metadata() {
        let connection_store = test_connection_store("sync-readonly");
        let saved_connections = serde_json::json!([
            {
                "id": "conn-1",
                "name": "Production",
                "host": "example.test"
            }
        ]);
        let saved_connections_snapshot = SavedConnectionsSyncSnapshot {
            revision: "rev-connections".to_string(),
            exported_at: "2026-05-25T00:00:00Z".to_string(),
            records: Vec::new(),
        };
        let local_metadata = SavedConnectionsLocalSyncMetadata {
            saved_connections_revision: "rev-connections".to_string(),
            saved_connections_updated_at: "2026-05-25T00:00:00Z".to_string(),
        };
        let plugin_settings = Vec::new();
        let plugin_settings_revisions = Map::new();

        let list_response = native_plugin_sync_response(
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "sync-list-1".to_string(),
                namespace: "sync".to_string(),
                method: "listSavedConnections".to_string(),
                args: serde_json::json!({}),
            },
            &connection_store,
            &saved_connections,
            Ok(&saved_connections_snapshot),
            Ok(&local_metadata),
            Some("rev-forwards"),
            &plugin_settings,
            &plugin_settings_revisions,
            None,
        );
        assert_eq!(
            list_response.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: saved_connections.clone()
            }
        );

        let metadata_response = native_plugin_sync_response(
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "sync-meta-1".to_string(),
                namespace: "sync".to_string(),
                method: "getLocalSyncMetadata".to_string(),
                args: serde_json::json!({}),
            },
            &connection_store,
            &saved_connections,
            Ok(&saved_connections_snapshot),
            Ok(&local_metadata),
            Some("rev-forwards"),
            &plugin_settings,
            &plugin_settings_revisions,
            None,
        );
        assert_eq!(
            metadata_response.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!({
                    "savedConnectionsRevision": "rev-connections",
                    "savedConnectionsUpdatedAt": "2026-05-25T00:00:00Z",
                    "savedForwardsRevision": "rev-forwards",
                    "pluginSettingsRevisions": {}
                })
            }
        );
    }

    #[test]
    fn sync_apply_saved_connections_requires_workspace_bridge() {
        let connection_store = test_connection_store("sync-pending");
        let saved_connections = serde_json::json!([]);
        let saved_connections_snapshot = SavedConnectionsSyncSnapshot {
            revision: "rev-connections".to_string(),
            exported_at: "2026-05-25T00:00:00Z".to_string(),
            records: Vec::new(),
        };
        let local_metadata = SavedConnectionsLocalSyncMetadata {
            saved_connections_revision: "rev-connections".to_string(),
            saved_connections_updated_at: "2026-05-25T00:00:00Z".to_string(),
        };
        let plugin_settings = Vec::new();
        let plugin_settings_revisions = Map::new();

        let response = native_plugin_sync_response(
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "sync-apply-1".to_string(),
                namespace: "sync".to_string(),
                method: "applySavedConnectionsSnapshot".to_string(),
                args: serde_json::json!({}),
            },
            &connection_store,
            &saved_connections,
            Ok(&saved_connections_snapshot),
            Ok(&local_metadata),
            None,
            &plugin_settings,
            &plugin_settings_revisions,
            None,
        );

        assert!(matches!(
            response.result,
            plugin_runtime::PluginResponseResult::Error {
                error: plugin_runtime::PluginError {
                    ref code,
                    recoverable: true,
                    ..
                }
            } if code == "plugin_sync_apply_unavailable"
        ));
    }

    #[test]
    fn sync_apply_saved_connections_args_parse_snapshot_and_strategy() {
        let snapshot = SavedConnectionsSyncSnapshot {
            revision: "rev-connections".to_string(),
            exported_at: "2026-05-25T00:00:00Z".to_string(),
            records: Vec::new(),
        };

        let (parsed_snapshot, strategy) =
            native_plugin_sync_apply_saved_connections_args(&serde_json::json!({
                "snapshot": snapshot,
                "conflictStrategy": "merge"
            }))
            .unwrap();

        assert_eq!(parsed_snapshot.revision, "rev-connections");
        assert_eq!(strategy, SavedConnectionsConflictStrategy::Merge);
    }

    #[test]
    fn sync_oxide_host_calls_export_validate_and_preview_without_workspace_mutation() {
        let connection_store = test_connection_store_with_agent_connection("sync-oxide");
        let saved_connections = serde_json::json!([]);
        let saved_connections_snapshot = SavedConnectionsSyncSnapshot {
            revision: "rev-connections".to_string(),
            exported_at: "2026-05-25T00:00:00Z".to_string(),
            records: Vec::new(),
        };
        let local_metadata = SavedConnectionsLocalSyncMetadata {
            saved_connections_revision: "rev-connections".to_string(),
            saved_connections_updated_at: "2026-05-25T00:00:00Z".to_string(),
        };
        let plugin_settings = Vec::new();
        let plugin_settings_revisions = Map::new();

        let preflight = native_plugin_sync_response(
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "sync-preflight-1".to_string(),
                namespace: "sync".to_string(),
                method: "preflightExport".to_string(),
                args: serde_json::json!({ "connectionIds": null, "embedKeys": false }),
            },
            &connection_store,
            &saved_connections,
            Ok(&saved_connections_snapshot),
            Ok(&local_metadata),
            None,
            &plugin_settings,
            &plugin_settings_revisions,
            None,
        );
        assert_eq!(
            preflight.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!({
                    "totalConnections": 1,
                    "missingKeys": [],
                    "connectionsWithKeys": 0,
                    "connectionsWithPasswords": 0,
                    "connectionsWithAgent": 1,
                    "totalKeyBytes": 0,
                    "canExport": true,
                    "portableSecretCount": 0,
                })
            }
        );

        let (progress_tx, progress_rx) = mpsc::channel();
        let export_response = native_plugin_sync_response(
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "sync-export-1".to_string(),
                namespace: "sync".to_string(),
                method: "exportOxide".to_string(),
                args: serde_json::json!({
                    "connectionIds": ["conn-1"],
                    "password": "StrongPass!123",
                    "description": "Plugin export",
                    "embedKeys": false,
                    "progressRegistrationId": "sync-progress-1"
                }),
            },
            &connection_store,
            &saved_connections,
            Ok(&saved_connections_snapshot),
            Ok(&local_metadata),
            None,
            &plugin_settings,
            &plugin_settings_revisions,
            Some(&progress_tx),
        );
        let plugin_runtime::PluginResponseResult::Ok { value: exported } = export_response.result
        else {
            panic!("expected sync.exportOxide to return .oxide bytes");
        };
        let progress_messages = progress_rx.try_iter().collect::<Vec<_>>();
        assert!(progress_messages.iter().any(|request| matches!(
            &request.action,
            NativePluginSyncAction::ReportProgress {
                plugin_id,
                registration_id,
                ..
            } if plugin_id == "com.example.demo" && registration_id == "sync-progress-1"
        )));
        let exported_bytes = native_plugin_u8_array(exported.as_array().unwrap()).unwrap();

        let validate_response = native_plugin_sync_response(
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "sync-validate-1".to_string(),
                namespace: "sync".to_string(),
                method: "validateOxide".to_string(),
                args: serde_json::json!({ "fileData": exported_bytes.clone() }),
            },
            &connection_store,
            &saved_connections,
            Ok(&saved_connections_snapshot),
            Ok(&local_metadata),
            None,
            &plugin_settings,
            &plugin_settings_revisions,
            None,
        );
        let plugin_runtime::PluginResponseResult::Ok { value: metadata } = validate_response.result
        else {
            panic!("expected sync.validateOxide to return metadata");
        };
        assert_eq!(metadata["description"], "Plugin export");
        assert_eq!(metadata["connection_names"], serde_json::json!(["Home"]));

        let preview_response = native_plugin_sync_response(
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "sync-preview-1".to_string(),
                namespace: "sync".to_string(),
                method: "previewImport".to_string(),
                args: serde_json::json!({
                    "fileData": exported_bytes,
                    "password": "StrongPass!123",
                    "conflictStrategy": "skip"
                }),
            },
            &connection_store,
            &saved_connections,
            Ok(&saved_connections_snapshot),
            Ok(&local_metadata),
            None,
            &plugin_settings,
            &plugin_settings_revisions,
            None,
        );
        let plugin_runtime::PluginResponseResult::Ok { value: preview } = preview_response.result
        else {
            panic!("expected sync.previewImport to return an import preview");
        };
        assert_eq!(preview["totalConnections"], 1);
        assert_eq!(preview["willSkip"], serde_json::json!(["Home"]));
    }

    #[test]
    fn sync_import_oxide_args_and_core_import_match_tauri_defaults() {
        let source_store = test_connection_store_with_agent_connection("sync-import-source");
        let bytes = export_connections_to_oxide(
            &source_store,
            &["conn-1".to_string()],
            "StrongPass!123",
            OxideExportOptions::default(),
        )
        .unwrap();
        let (parsed_bytes, password, options) =
            native_plugin_sync_import_oxide_args(&serde_json::json!({
                "fileData": bytes,
                "password": "StrongPass!123",
                "conflictStrategy": "rename",
                "selectedPluginIds": []
            }))
            .unwrap();
        assert!(options.oxide_options.import_forwards);
        assert!(!options.oxide_options.import_portable_secrets);
        assert!(options.import_app_settings);
        assert!(options.import_plugin_settings);
        assert_eq!(options.selected_plugin_ids, Some(HashSet::new()));

        let mut target_store = test_connection_store("sync-import-target");
        let envelope = native_plugin_apply_oxide_import_core(
            &mut target_store,
            &parsed_bytes,
            &password,
            options.oxide_options,
        )
        .unwrap();
        assert_eq!(envelope.imported, 1);
        assert!(target_store.get("conn-1").is_none());
        assert!(target_store.connections().iter().any(|connection| {
            connection.name == "Home"
                && matches!(connection.auth, oxideterm_connections::SavedAuth::Agent)
        }));
    }

    #[test]
    fn sync_import_result_omits_consumed_sidecar_payloads() {
        let envelope = ImportResultEnvelope {
            imported: 1,
            app_settings_json: Some("{}".to_string()),
            quick_commands_json: Some("[]".to_string()),
            plugin_settings: vec![oxideterm_connections::oxide_file::EncryptedPluginSetting {
                storage_key: "oxide-plugin-com.example.demo-setting-mode".to_string(),
                serialized_value: "\"auto\"".to_string(),
            }],
            ..ImportResultEnvelope::default()
        };

        let value = native_plugin_sync_import_result_value(
            &envelope,
            true,
            false,
            2,
            false,
            Vec::new(),
            1,
            false,
        );

        assert_eq!(value["imported"], 1);
        assert_eq!(value["importedAppSettings"], true);
        assert_eq!(value["importedQuickCommands"], 2);
        assert_eq!(value["importedPluginSettings"], 1);
        assert!(value.get("appSettingsJson").is_none());
        assert!(value.get("quickCommandsJson").is_none());
        assert!(value.get("pluginSettings").is_none());
    }

    #[test]
    fn sync_plugin_settings_export_filters_selected_plugins_and_revisions() {
        let connection_store = test_connection_store("sync-plugin-settings");
        let plugin_settings = vec![
            oxideterm_connections::oxide_file::EncryptedPluginSetting {
                storage_key: "oxide-plugin-com.example.demo-setting-mode".to_string(),
                serialized_value: "\"auto\"".to_string(),
            },
            oxideterm_connections::oxide_file::EncryptedPluginSetting {
                storage_key: "oxide-plugin-com.example.other-setting-mode".to_string(),
                serialized_value: "\"manual\"".to_string(),
            },
        ];

        let response = native_plugin_sync_export_oxide_response(
            "com.example.demo",
            "sync-plugin-export-1".to_string(),
            &connection_store,
            &plugin_settings,
            &serde_json::json!({
                "connectionIds": [],
                "password": "StrongPass!123",
                "includePluginSettings": true,
                "selectedPluginIds": ["com.example.demo"]
            }),
            None,
        );
        let plugin_runtime::PluginResponseResult::Ok { value } = response.result else {
            panic!("expected sync.exportOxide to include selected plugin settings");
        };
        let bytes = native_plugin_u8_array(value.as_array().unwrap()).unwrap();
        let file = OxideFile::from_bytes(&bytes).unwrap();
        assert_eq!(file.metadata.plugin_settings_count, Some(1));

        let revisions = native_plugin_settings_revision_map(&plugin_settings);
        assert!(
            revisions
                .get("com.example.demo")
                .and_then(Value::as_str)
                .is_some_and(|revision| revision.starts_with("fnv1a-"))
        );
        assert!(revisions.contains_key("com.example.other"));
    }

    #[test]
    fn transfers_host_calls_return_tauri_snapshot_shape_and_filter_by_node() {
        let manager = Arc::new(SftpTransferManager::new());
        let first_transfer = BackgroundTransferSnapshot::new(
            "tx-1".to_string(),
            "node-a".to_string(),
            "Upload logs".to_string(),
            "/local/logs".to_string(),
            "/remote/logs".to_string(),
            BackgroundTransferDirection::Upload,
            oxideterm_sftp::BackgroundTransferKind::Directory,
            oxideterm_sftp::TransferStrategy::DirectoryRecursive,
            2048,
            512,
        );
        let second_transfer = BackgroundTransferSnapshot::new(
            "tx-2".to_string(),
            "node-b".to_string(),
            "Download report".to_string(),
            "/local/report.txt".to_string(),
            "/remote/report.txt".to_string(),
            BackgroundTransferDirection::Download,
            oxideterm_sftp::BackgroundTransferKind::File,
            oxideterm_sftp::TransferStrategy::File,
            64,
            64,
        );
        manager.register_background_transfer(first_transfer);
        manager.register_background_transfer(second_transfer);
        manager.mark_background_transfer_active("tx-1");
        manager.finish_background_transfer("tx-2", BackgroundTransferState::Completed, None, None);

        let all_response = native_plugin_transfers_response(
            plugin_runtime::PluginHostCall {
                request_id: "transfers-all-1".to_string(),
                namespace: "transfers".to_string(),
                method: "getAll".to_string(),
                args: serde_json::json!({}),
            },
            &manager,
        );
        let plugin_runtime::PluginResponseResult::Ok { value: all_value } = all_response.result
        else {
            panic!("expected transfers.getAll to return snapshots");
        };
        let all = all_value.as_array().unwrap();
        assert_eq!(all.len(), 2);
        let first = all
            .iter()
            .find(|transfer| transfer["id"] == "tx-1")
            .unwrap();
        assert_eq!(first["nodeId"], "node-a");
        assert_eq!(first["direction"], "upload");
        assert_eq!(first["state"], "active");
        assert!(first.get("strategy").is_none());

        let by_node_response = native_plugin_transfers_response(
            plugin_runtime::PluginHostCall {
                request_id: "transfers-node-1".to_string(),
                namespace: "transfers".to_string(),
                method: "getByNode".to_string(),
                args: serde_json::json!({ "nodeId": "node-b" }),
            },
            &manager,
        );
        assert_eq!(
            by_node_response.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!([
                    {
                        "id": "tx-2",
                        "nodeId": "node-b",
                        "name": "Download report",
                        "localPath": "/local/report.txt",
                        "remotePath": "/remote/report.txt",
                        "direction": "download",
                        "size": 64,
                        "transferred": 64,
                        "state": "completed",
                        "error": null,
                        "startTime": all
                            .iter()
                            .find(|transfer| transfer["id"] == "tx-2")
                            .unwrap()["startTime"],
                        "endTime": all
                            .iter()
                            .find(|transfer| transfer["id"] == "tx-2")
                            .unwrap()["endTime"],
                    }
                ])
            }
        );
    }

    #[test]
    fn transfer_state_helpers_detect_complete_and_error_transitions() {
        let previous = serde_json::json!([
            { "id": "tx-1", "state": "active" },
            { "id": "tx-2", "state": "pending" }
        ]);
        let next = serde_json::json!([
            { "id": "tx-1", "state": "completed" },
            { "id": "tx-2", "state": "error" }
        ]);
        let previous_states = native_plugin_transfer_state_map(&previous);
        let next_states = native_plugin_transfer_state_map(&next);

        let completed = native_plugin_transfer_transition_values(
            &next,
            &previous_states,
            &next_states,
            BackgroundTransferState::Completed,
        );
        let errored = native_plugin_transfer_transition_values(
            &next,
            &previous_states,
            &next_states,
            BackgroundTransferState::Error,
        );

        assert_eq!(completed[0]["id"], "tx-1");
        assert_eq!(errored[0]["id"], "tx-2");
    }

    #[test]
    fn profiler_host_calls_map_node_ids_to_tauri_metrics_shape() {
        let registry = ProfilerRegistry::new();
        registry.start("conn-1");
        registry.record_metrics(oxideterm_connection_monitor::ProfilerUpdate {
            connection_id: "conn-1".to_string(),
            metrics: ResourceMetrics {
                timestamp_ms: 42,
                cpu_percent: Some(12.5),
                memory_used: Some(1024),
                memory_total: Some(2048),
                memory_percent: Some(50.0),
                disk_used: Some(10),
                disk_total: Some(20),
                disk_percent: Some(50.0),
                load_avg_1: Some(0.1),
                load_avg_5: Some(0.2),
                load_avg_15: Some(0.3),
                cpu_cores: Some(8),
                net_rx_bytes_per_sec: Some(100),
                net_tx_bytes_per_sec: Some(200),
                ssh_rtt_ms: Some(9),
                source: oxideterm_connection_monitor::MetricsSource::Full,
            },
        });
        let node_connection_ids = HashMap::from([("node-1".to_string(), "conn-1".to_string())]);

        let response = native_plugin_profiler_response(
            plugin_runtime::PluginHostCall {
                request_id: "profiler-metrics-1".to_string(),
                namespace: "profiler".to_string(),
                method: "getMetrics".to_string(),
                args: serde_json::json!({ "nodeId": "node-1" }),
            },
            &registry,
            &node_connection_ids,
        );
        assert_eq!(
            response.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!({
                    "timestampMs": 42,
                    "cpuPercent": 12.5,
                    "memoryUsed": 1024,
                    "memoryTotal": 2048,
                    "memoryPercent": 50.0,
                    "loadAvg1": 0.1,
                    "loadAvg5": 0.2,
                    "loadAvg15": 0.3,
                    "cpuCores": 8,
                    "netRxBytesPerSec": 100,
                    "netTxBytesPerSec": 200,
                    "sshRttMs": 9,
                })
            }
        );

        let running_response = native_plugin_profiler_response(
            plugin_runtime::PluginHostCall {
                request_id: "profiler-running-1".to_string(),
                namespace: "profiler".to_string(),
                method: "isRunning".to_string(),
                args: serde_json::json!({ "nodeId": "node-1" }),
            },
            &registry,
            &node_connection_ids,
        );
        assert_eq!(
            running_response.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!(true)
            }
        );
    }

    #[test]
    fn profiler_history_limits_and_subscription_filters_are_node_scoped() {
        let registry = ProfilerRegistry::new();
        registry.start("conn-1");
        for timestamp_ms in [1, 2, 3] {
            registry.record_metrics(oxideterm_connection_monitor::ProfilerUpdate {
                connection_id: "conn-1".to_string(),
                metrics: ResourceMetrics::empty(
                    timestamp_ms,
                    oxideterm_connection_monitor::MetricsSource::Full,
                ),
            });
        }
        let node_connection_ids = HashMap::from([("node-1".to_string(), "conn-1".to_string())]);

        let history_response = native_plugin_profiler_response(
            plugin_runtime::PluginHostCall {
                request_id: "profiler-history-1".to_string(),
                namespace: "profiler".to_string(),
                method: "getHistory".to_string(),
                args: serde_json::json!({ "nodeId": "node-1", "maxPoints": 2 }),
            },
            &registry,
            &node_connection_ids,
        );
        let plugin_runtime::PluginResponseResult::Ok { value } = history_response.result else {
            panic!("expected profiler.getHistory to return a history array");
        };
        let history = value.as_array().unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0]["timestampMs"], 2);
        assert!(native_plugin_subscription_allows_node(
            Some(&serde_json::json!({ "nodeId": "node-1" })),
            "node-1"
        ));
        assert!(!native_plugin_subscription_allows_node(
            Some(&serde_json::json!({ "nodeId": "node-2" })),
            "node-1"
        ));
    }

    #[test]
    fn ide_host_calls_return_project_open_files_and_active_file() {
        let ide_snapshot = native_plugin_ide_snapshot_value(&IdePluginSnapshot {
            project: oxideterm_gpui_ide::IdePluginProjectSnapshot {
                node_id: "node-1".to_string(),
                root_path: "/srv/app".to_string(),
                name: "app".to_string(),
                is_git_repo: true,
                git_branch: Some("main".to_string()),
            },
            open_files: vec![
                IdePluginFileSnapshot {
                    path: "/srv/app/src/main.rs".to_string(),
                    name: "main.rs".to_string(),
                    language: "Rust".to_string(),
                    is_dirty: false,
                    is_active: true,
                    is_pinned: false,
                },
                IdePluginFileSnapshot {
                    path: "/srv/app/README.md".to_string(),
                    name: "README.md".to_string(),
                    language: "Markdown".to_string(),
                    is_dirty: true,
                    is_active: false,
                    is_pinned: true,
                },
            ],
            active_file: Some(IdePluginFileSnapshot {
                path: "/srv/app/src/main.rs".to_string(),
                name: "main.rs".to_string(),
                language: "Rust".to_string(),
                is_dirty: false,
                is_active: true,
                is_pinned: false,
            }),
        });

        let project_response = native_plugin_ide_response(
            plugin_runtime::PluginHostCall {
                request_id: "ide-project-1".to_string(),
                namespace: "ide".to_string(),
                method: "getProject".to_string(),
                args: serde_json::json!({}),
            },
            &ide_snapshot,
        );
        assert_eq!(
            project_response.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!({
                    "nodeId": "node-1",
                    "rootPath": "/srv/app",
                    "name": "app",
                    "isGitRepo": true,
                    "gitBranch": "main",
                })
            }
        );

        let active_response = native_plugin_ide_response(
            plugin_runtime::PluginHostCall {
                request_id: "ide-active-1".to_string(),
                namespace: "ide".to_string(),
                method: "getActiveFile".to_string(),
                args: serde_json::json!({}),
            },
            &ide_snapshot,
        );
        assert_eq!(
            active_response.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!({
                    "path": "/srv/app/src/main.rs",
                    "name": "main.rs",
                    "language": "Rust",
                    "isDirty": false,
                    "isActive": true,
                    "isPinned": false,
                })
            }
        );
    }

    #[test]
    fn ide_file_maps_detect_open_close_and_active_changes() {
        let previous = serde_json::json!({
            "openFiles": [
                { "path": "/a.rs", "name": "a.rs", "isActive": true }
            ],
            "activeFile": { "path": "/a.rs" }
        });
        let next = serde_json::json!({
            "openFiles": [
                { "path": "/b.rs", "name": "b.rs", "isActive": true }
            ],
            "activeFile": { "path": "/b.rs" }
        });

        let previous_files = native_plugin_ide_file_map(&previous);
        let next_files = native_plugin_ide_file_map(&next);
        assert!(previous_files.contains_key("/a.rs"));
        assert!(!next_files.contains_key("/a.rs"));
        assert!(next_files.contains_key("/b.rs"));
        assert_ne!(
            native_plugin_ide_active_file_path(&previous),
            native_plugin_ide_active_file_path(&next)
        );
    }

    #[test]
    fn ai_host_calls_return_sanitized_messages_and_provider_info() {
        let chat = oxideterm_ai::AiChatState {
            conversations: vec![oxideterm_ai::AiConversation {
                id: "conversation-1".to_string(),
                title: "Deploy help".to_string(),
                messages: vec![
                    oxideterm_ai::AiChatMessage {
                        id: "message-user-1".to_string(),
                        role: oxideterm_ai::AiChatRole::User,
                        content: "Authorization: Bearer secret-token-value".to_string(),
                        timestamp_ms: 10,
                        model: None,
                        context: None,
                        thinking_content: None,
                        is_streaming: false,
                        metadata: None,
                        tool_call_id: None,
                        tool_calls: Vec::new(),
                        turn: None,
                        transcript_ref: None,
                        summary_ref: None,
                        branches: None,
                        suggestions: Vec::new(),
                    },
                    oxideterm_ai::AiChatMessage {
                        id: "message-tool-1".to_string(),
                        role: oxideterm_ai::AiChatRole::Tool,
                        content: "{\"token\":\"tool-secret-value\"}".to_string(),
                        timestamp_ms: 11,
                        model: None,
                        context: None,
                        thinking_content: None,
                        is_streaming: false,
                        metadata: None,
                        tool_call_id: None,
                        tool_calls: Vec::new(),
                        turn: None,
                        transcript_ref: None,
                        summary_ref: None,
                        branches: None,
                        suggestions: Vec::new(),
                    },
                ],
                created_at_ms: 1,
                updated_at_ms: 12,
                origin: "sidebar".to_string(),
                profile_id: None,
                message_count: 2,
                session_id: None,
                session_metadata: None,
                messages_loaded: true,
            }],
            active_conversation_id: Some("conversation-1".to_string()),
        };
        let mut model_context_windows = Map::new();
        model_context_windows.insert(
            "provider-1".to_string(),
            serde_json::json!({
                "gpt-4o-mini": { "contextWindow": 128000 },
                "gpt-4.1": { "contextWindow": 1048576 }
            }),
        );
        let providers = vec![serde_json::json!({
            "id": "provider-1",
            "type": "openai",
            "name": "OpenAI",
            "models": ["gpt-4o-mini"],
            "defaultModel": "gpt-4o-mini"
        })];
        let snapshot = native_plugin_ai_snapshot_value(
            &chat,
            &providers,
            Some("provider-1"),
            &model_context_windows,
        );

        let conversations_response = native_plugin_ai_response(
            plugin_runtime::PluginHostCall {
                request_id: "ai-conversations-1".to_string(),
                namespace: "ai".to_string(),
                method: "getConversations".to_string(),
                args: serde_json::json!({}),
            },
            &snapshot,
        );
        assert_eq!(
            conversations_response.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!([{
                    "id": "conversation-1",
                    "title": "Deploy help",
                    "messageCount": 1,
                    "createdAt": 1,
                    "updatedAt": 12,
                }])
            }
        );

        let messages_response = native_plugin_ai_response(
            plugin_runtime::PluginHostCall {
                request_id: "ai-messages-1".to_string(),
                namespace: "ai".to_string(),
                method: "getMessages".to_string(),
                args: serde_json::json!({ "conversationId": "conversation-1" }),
            },
            &snapshot,
        );
        let plugin_runtime::PluginResponseResult::Ok { value: messages } = messages_response.result
        else {
            panic!("expected ai.getMessages to return sanitized message snapshots");
        };
        assert_eq!(messages.as_array().unwrap().len(), 1);
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[0]["content"], "Authorization: Bearer [REDACTED]");

        let active_provider_response = native_plugin_ai_response(
            plugin_runtime::PluginHostCall {
                request_id: "ai-provider-1".to_string(),
                namespace: "ai".to_string(),
                method: "getActiveProvider".to_string(),
                args: serde_json::json!({}),
            },
            &snapshot,
        );
        assert_eq!(
            active_provider_response.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!({
                    "type": "openai",
                    "displayName": "OpenAI"
                })
            }
        );

        let models_response = native_plugin_ai_response(
            plugin_runtime::PluginHostCall {
                request_id: "ai-models-1".to_string(),
                namespace: "ai".to_string(),
                method: "getAvailableModels".to_string(),
                args: serde_json::json!({}),
            },
            &snapshot,
        );
        let plugin_runtime::PluginResponseResult::Ok { value: models } = models_response.result
        else {
            panic!("expected ai.getAvailableModels to return configured model keys");
        };
        assert!(models.as_array().unwrap().contains(&json!("gpt-4o-mini")));
        assert!(models.as_array().unwrap().contains(&json!("gpt-4.1")));
    }

    #[test]
    fn ai_new_message_events_omit_message_content() {
        let snapshot = serde_json::json!({
            "conversations": [
                {
                    "id": "conversation-1",
                    "title": "Deploy help",
                    "messageCount": 2,
                    "createdAt": 1,
                    "updatedAt": 20
                }
            ],
            "messagesByConversation": {
                "conversation-1": [
                    {
                        "id": "message-user-1",
                        "role": "user",
                        "content": "safe prompt",
                        "timestamp": 10
                    },
                    {
                        "id": "message-assistant-1",
                        "role": "assistant",
                        "content": "answer with sanitized details",
                        "timestamp": 20
                    }
                ]
            }
        });
        let previous_counts = HashMap::from([("conversation-1".to_string(), 1)]);

        let events = native_plugin_ai_new_message_events(&snapshot, &previous_counts);

        assert_eq!(
            events,
            vec![serde_json::json!({
                "conversationId": "conversation-1",
                "messageId": "message-assistant-1",
                "role": "assistant"
            })]
        );
        // Tauri's onMessage payload is metadata-only; native keeps content out
        // of the event and requires plugins to call getMessages for sanitized text.
        assert!(events[0].get("content").is_none());
    }

    #[test]
    fn sftp_host_call_args_reject_missing_or_invalid_paths() {
        let missing_node = serde_json::json!({ "path": "/tmp/file" });
        assert!(native_plugin_sftp_node_id_arg(&missing_node).is_err());

        let empty_path = serde_json::json!({ "nodeId": "node-1", "path": "" });
        assert!(native_plugin_sftp_path_arg(&empty_path, "path").is_err());

        let nul_path = serde_json::json!({ "nodeId": "node-1", "path": "/tmp/a\0b" });
        assert!(native_plugin_sftp_path_arg(&nul_path, "path").is_err());
    }

    #[test]
    fn sftp_host_calls_require_matching_filesystem_capability() {
        let read_only = plugin_runtime::PluginPermissionSet {
            capabilities: vec![NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_READ.to_string()],
            allowed_host_apis: Vec::new(),
        };
        assert!(native_plugin_sftp_check_capability("listDir", &read_only).is_ok());
        assert!(native_plugin_sftp_check_capability("readFile", &read_only).is_ok());
        assert!(native_plugin_sftp_check_capability("writeFile", &read_only).is_err());
        assert!(native_plugin_sftp_check_capability("delete", &read_only).is_err());

        let write_enabled = plugin_runtime::PluginPermissionSet {
            capabilities: vec![NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_WRITE.to_string()],
            allowed_host_apis: Vec::new(),
        };
        assert!(native_plugin_sftp_check_capability("rename", &write_enabled).is_ok());
    }

    #[test]
    fn forward_host_calls_require_network_forward_capability() {
        let denied = plugin_runtime::PluginPermissionSet::default();
        assert!(native_plugin_forward_check_capability("create", &denied).is_err());
        assert!(native_plugin_forward_check_capability("list", &denied).is_err());

        let allowed = plugin_runtime::PluginPermissionSet {
            capabilities: vec![NATIVE_PLUGIN_CAPABILITY_NETWORK_FORWARD.to_string()],
            allowed_host_apis: Vec::new(),
        };
        assert!(native_plugin_forward_check_capability("create", &allowed).is_ok());
        assert!(
            native_plugin_forward_check_capability("exportSavedForwardsSnapshot", &allowed).is_ok()
        );
    }

    #[test]
    fn forward_create_request_accepts_tauri_camel_case_shape() {
        let request = native_plugin_forward_create_request(&serde_json::json!({
            "sessionId": "node:abc",
            "forwardType": "local",
            "bindAddress": "127.0.0.1",
            "bindPort": 8080,
            "targetHost": "localhost",
            "targetPort": 80,
            "description": "plugin forward",
        }))
        .unwrap();

        assert_eq!(request.session_id, "node:abc");
        assert_eq!(request.forward_type, ForwardType::Local);
        assert_eq!(request.bind_port, 8080);
        assert_eq!(request.target_port, 80);
    }

    #[test]
    fn forward_rule_snapshot_matches_plugin_forward_rule_shape() {
        let mut rule = ForwardRule::local("127.0.0.1", 8080, "localhost", 80);
        rule.id = "forward-1".to_string();
        rule.status = ForwardStatus::Active;
        rule.description = "plugin forward".to_string();

        let snapshot = native_plugin_forward_rule_snapshot(rule);
        assert_eq!(snapshot["id"], "forward-1");
        assert_eq!(snapshot["forward_type"], "local");
        assert_eq!(snapshot["bind_address"], "127.0.0.1");
        assert_eq!(snapshot["status"], "active");
        assert_eq!(snapshot["description"], "plugin forward");
    }

    #[test]
    fn notification_severity_maps_to_workspace_toast_variant() {
        assert_eq!(
            native_plugin_notification_variant("error"),
            TerminalNoticeVariant::Error
        );
        assert_eq!(
            native_plugin_notification_variant("warning"),
            TerminalNoticeVariant::Warning
        );
        assert_eq!(
            native_plugin_notification_variant("info"),
            TerminalNoticeVariant::Default
        );
    }

    #[test]
    fn progress_effect_updates_host_owned_toast_payload() {
        let notice = native_plugin_progress_notice(
            "com.example.demo",
            "progress-1",
            serde_json::json!({
                "title": "Indexing",
                "value": 2,
                "total": 4,
                "message": "Half done",
            }),
        );

        assert_eq!(notice.title, "Indexing (com.example.demo)");
        assert_eq!(notice.description.as_deref(), Some("Half done"));
        assert_eq!(notice.status_text.as_deref(), Some("50%"));
        assert_eq!(notice.progress, Some(50.0));
        assert!(native_plugin_progress_is_done(
            &serde_json::json!({"done": true})
        ));
    }

    #[test]
    fn show_progress_returnable_host_api_creates_host_owned_reporter() {
        let (progress_tx, progress_rx) = mpsc::channel();
        let response = native_plugin_show_progress_response(
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "progress-1".to_string(),
                namespace: "ui".to_string(),
                method: "showProgress".to_string(),
                args: serde_json::json!({
                    "title": "Syncing",
                    "registrationId": "progress-sync-1",
                }),
            },
            Some(&progress_tx),
        );

        assert_eq!(
            response.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!({
                    "id": "progress-sync-1",
                    "registrationId": "progress-sync-1",
                })
            }
        );
        let request = progress_rx.recv().unwrap();
        assert!(matches!(
            request.action,
            NativePluginSyncAction::ReportProgress {
                plugin_id,
                registration_id,
                ..
            } if plugin_id == "com.example.demo" && registration_id == "progress-sync-1"
        ));
    }

    #[test]
    fn show_confirm_returnable_host_api_resolves_user_choice() {
        let (confirm_tx, confirm_rx) = mpsc::channel::<NativePluginConfirmRequest>();
        let handle = std::thread::spawn(move || {
            let request = confirm_rx.recv().unwrap();
            assert_eq!(request.plugin_id, "com.example.demo");
            assert_eq!(request.request_id, "confirm-1");
            assert_eq!(request.title, "Delete cache?");
            assert_eq!(request.description, "This cannot be undone.");
            request.response_tx.send(true).unwrap();
        });

        let response = native_plugin_show_confirm_response(
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "confirm-1".to_string(),
                namespace: "ui".to_string(),
                method: "showConfirm".to_string(),
                args: serde_json::json!({
                    "title": "Delete cache?",
                    "description": "This cannot be undone.",
                }),
            },
            &confirm_tx,
        );

        handle.join().unwrap();
        assert_eq!(
            response.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!(true)
            }
        );
    }

    #[test]
    fn show_confirm_returnable_host_api_rejects_missing_description() {
        let (confirm_tx, _confirm_rx) = mpsc::channel();
        let response = native_plugin_show_confirm_response(
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "confirm-2".to_string(),
                namespace: "ui".to_string(),
                method: "showConfirm".to_string(),
                args: serde_json::json!({
                    "title": "Missing body",
                }),
            },
            &confirm_tx,
        );

        assert!(matches!(
            response.result,
            plugin_runtime::PluginResponseResult::Error { .. }
        ));
    }

    #[test]
    fn storage_get_returnable_host_api_returns_json_or_null() {
        let snapshot = test_host_api_snapshot();
        let response = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "storage-get-1".to_string(),
                namespace: "storage".to_string(),
                method: "get".to_string(),
                args: serde_json::json!({ "key": "missing" }),
            },
        )
        .unwrap();
        assert_eq!(
            response.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::Value::Null
            }
        );

        let error = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "storage-get-2".to_string(),
                namespace: "storage".to_string(),
                method: "get".to_string(),
                args: serde_json::json!({}),
            },
        )
        .unwrap();
        assert!(matches!(
            error.result,
            plugin_runtime::PluginResponseResult::Error { .. }
        ));
    }

    #[test]
    fn app_returnable_host_apis_match_tauri_snapshot_shape() {
        let snapshot = test_host_api_snapshot();

        let theme = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "app-theme".to_string(),
                namespace: "app".to_string(),
                method: "getTheme".to_string(),
                args: serde_json::json!({}),
            },
        )
        .unwrap();
        assert_eq!(
            theme.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!({
                    "name": "default",
                    "isDark": true,
                })
            }
        );

        let settings = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "app-settings".to_string(),
                namespace: "app".to_string(),
                method: "getSettings".to_string(),
                args: serde_json::json!({ "category": "general" }),
            },
        )
        .unwrap();
        assert!(matches!(
            settings.result,
            plugin_runtime::PluginResponseResult::Ok { .. }
        ));
        if let plugin_runtime::PluginResponseResult::Ok { value } = settings.result {
            assert_eq!(value["language"], "zh-CN");
        }

        let pool_stats = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "app-pool".to_string(),
                namespace: "app".to_string(),
                method: "getPoolStats".to_string(),
                args: serde_json::json!({}),
            },
        )
        .unwrap();
        assert_eq!(
            pool_stats.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!({
                    "activeConnections": 0,
                    "totalSessions": 0,
                })
            }
        );

        for (method, expected) in [
            ("getVersion", serde_json::json!(env!("CARGO_PKG_VERSION"))),
            (
                "getPlatform",
                serde_json::json!(native_plugin_platform_label()),
            ),
            ("getLocale", serde_json::json!("zh-CN")),
        ] {
            let response = native_plugin_returnable_host_api_response(
                &snapshot,
                "com.example.demo",
                plugin_runtime::PluginHostCall {
                    request_id: format!("app-{method}"),
                    namespace: "app".to_string(),
                    method: method.to_string(),
                    args: serde_json::json!({}),
                },
            )
            .unwrap();
            assert_eq!(
                response.result,
                plugin_runtime::PluginResponseResult::Ok { value: expected }
            );
        }
    }

    #[test]
    fn api_invoke_rejects_undeclared_commands_and_runs_supported_whitelisted_commands() {
        let snapshot = test_host_api_snapshot_with_declared_api_commands();
        let permissions = plugin_runtime::PluginPermissionSet {
            capabilities: Vec::new(),
            allowed_host_apis: Vec::new(),
        };
        let sftp_router = NodeRouter::new(oxideterm_ssh::SshConnectionRegistry::new(
            oxideterm_ssh::ConnectionPoolConfig::default(),
        ));
        let runtime = Arc::new(tokio::runtime::Runtime::new().unwrap());
        let forwarding_registry = ForwardingRegistry::new();
        let transfer_manager = Arc::new(SftpTransferManager::new());
        let allowed = native_plugin_api_invoke_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "api-pool-stats".to_string(),
                namespace: "api".to_string(),
                method: "invoke".to_string(),
                args: serde_json::json!({
                    "command": NATIVE_PLUGIN_API_COMMAND_SSH_POOL_STATS,
                    "args": {}
                }),
            },
            NativePluginBackendAdapters {
                permissions: &permissions,
                sftp_router: &sftp_router,
                sftp_runtime: &runtime,
                forwarding_registry: &forwarding_registry,
                forwarding_runtime: &runtime,
                transfer_manager: &transfer_manager,
            },
        );
        assert_eq!(
            allowed.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!({
                    "activeConnections": 0,
                    "totalSessions": 0,
                })
            }
        );

        let denied = native_plugin_api_invoke_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "api-denied".to_string(),
                namespace: "api".to_string(),
                method: "invoke".to_string(),
                args: serde_json::json!({ "command": "read_plugin_file" }),
            },
            NativePluginBackendAdapters {
                permissions: &permissions,
                sftp_router: &sftp_router,
                sftp_runtime: &runtime,
                forwarding_registry: &forwarding_registry,
                forwarding_runtime: &runtime,
                transfer_manager: &transfer_manager,
            },
        );
        assert!(matches!(
            denied.result,
            plugin_runtime::PluginResponseResult::Error {
                error: plugin_runtime::PluginError { ref code, .. }
            } if code == "backend_command_not_whitelisted"
        ));

        let unsupported = native_plugin_api_invoke_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "api-unsupported".to_string(),
                namespace: "api".to_string(),
                method: "invoke".to_string(),
                args: serde_json::json!({ "command": "custom_declared_command" }),
            },
            NativePluginBackendAdapters {
                permissions: &permissions,
                sftp_router: &sftp_router,
                sftp_runtime: &runtime,
                forwarding_registry: &forwarding_registry,
                forwarding_runtime: &runtime,
                transfer_manager: &transfer_manager,
            },
        );
        assert!(matches!(
            unsupported.result,
            plugin_runtime::PluginResponseResult::Error {
                error: plugin_runtime::PluginError { ref code, .. }
            } if code == "backend_command_not_supported"
        ));
    }

    #[test]
    fn api_invoke_native_adapters_cover_system_transfer_and_capability_paths() {
        let snapshot = test_host_api_snapshot_with_declared_api_commands();
        let supported_commands = native_plugin_supported_backend_commands()
            .iter()
            .copied()
            .collect::<HashSet<_>>();
        assert_eq!(
            supported_commands.len(),
            native_plugin_supported_backend_commands().len()
        );

        let permissions = plugin_runtime::PluginPermissionSet {
            capabilities: Vec::new(),
            allowed_host_apis: Vec::new(),
        };
        let sftp_router = NodeRouter::new(oxideterm_ssh::SshConnectionRegistry::new(
            oxideterm_ssh::ConnectionPoolConfig::default(),
        ));
        let runtime = Arc::new(tokio::runtime::Runtime::new().unwrap());
        let forwarding_registry = ForwardingRegistry::new();
        let transfer_manager = Arc::new(SftpTransferManager::new());

        let version = native_plugin_api_invoke_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "api-version".to_string(),
                namespace: "api".to_string(),
                method: "invoke".to_string(),
                args: serde_json::json!({ "command": NATIVE_PLUGIN_API_COMMAND_GET_APP_VERSION }),
            },
            NativePluginBackendAdapters {
                permissions: &permissions,
                sftp_router: &sftp_router,
                sftp_runtime: &runtime,
                forwarding_registry: &forwarding_registry,
                forwarding_runtime: &runtime,
                transfer_manager: &transfer_manager,
            },
        );
        assert_eq!(
            version.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!(env!("CARGO_PKG_VERSION"))
            }
        );

        let transfer_stats = native_plugin_api_invoke_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "api-transfer-stats".to_string(),
                namespace: "api".to_string(),
                method: "invoke".to_string(),
                args: serde_json::json!({
                    "command": NATIVE_PLUGIN_API_COMMAND_SFTP_TRANSFER_STATS
                }),
            },
            NativePluginBackendAdapters {
                permissions: &permissions,
                sftp_router: &sftp_router,
                sftp_runtime: &runtime,
                forwarding_registry: &forwarding_registry,
                forwarding_runtime: &runtime,
                transfer_manager: &transfer_manager,
            },
        );
        assert_eq!(
            transfer_stats.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!({
                    "active": 0,
                    "queued": 0,
                    "completed": 0,
                })
            }
        );

        let invalid_http = native_plugin_api_invoke_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "api-http-invalid-url".to_string(),
                namespace: "api".to_string(),
                method: "invoke".to_string(),
                args: serde_json::json!({
                    "command": NATIVE_PLUGIN_API_COMMAND_PLUGIN_HTTP_REQUEST,
                    "args": {
                        "url": "file:///tmp/not-allowed",
                        "method": "GET",
                        "headers": {}
                    }
                }),
            },
            NativePluginBackendAdapters {
                permissions: &permissions,
                sftp_router: &sftp_router,
                sftp_runtime: &runtime,
                forwarding_registry: &forwarding_registry,
                forwarding_runtime: &runtime,
                transfer_manager: &transfer_manager,
            },
        );
        assert!(matches!(
            invalid_http.result,
            plugin_runtime::PluginResponseResult::Error {
                error: plugin_runtime::PluginError { ref code, .. }
            } if code == "plugin_http_request_error"
        ));

        let denied_sftp = native_plugin_api_invoke_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "api-sftp-denied".to_string(),
                namespace: "api".to_string(),
                method: "invoke".to_string(),
                args: serde_json::json!({
                    "command": NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_LIST_DIR,
                    "args": { "nodeId": "node-a", "path": "/" }
                }),
            },
            NativePluginBackendAdapters {
                permissions: &permissions,
                sftp_router: &sftp_router,
                sftp_runtime: &runtime,
                forwarding_registry: &forwarding_registry,
                forwarding_runtime: &runtime,
                transfer_manager: &transfer_manager,
            },
        );
        assert!(matches!(
            denied_sftp.result,
            plugin_runtime::PluginResponseResult::Error {
                error: plugin_runtime::PluginError { ref code, .. }
            } if code == "plugin_sftp_capability_denied"
        ));
    }

    #[test]
    fn ui_get_layout_returnable_host_api_matches_tauri_snapshot_shape() {
        let snapshot = test_host_api_snapshot();
        let response = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "ui-layout".to_string(),
                namespace: "ui".to_string(),
                method: "getLayout".to_string(),
                args: serde_json::json!({}),
            },
        )
        .unwrap();

        assert_eq!(
            response.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!({
                    "sidebarCollapsed": false,
                    "activeTabId": null,
                    "tabCount": 0,
                })
            }
        );
        assert_eq!(
            native_plugin_layout_snapshot(true, Some("7".to_string()), 3),
            serde_json::json!({
                "sidebarCollapsed": true,
                "activeTabId": "7",
                "tabCount": 3,
            })
        );
    }

    #[test]
    fn connections_returnable_host_apis_match_tauri_snapshot_shape() {
        let snapshot = test_host_api_snapshot_with_connections();
        let all = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "connections-all".to_string(),
                namespace: "connections".to_string(),
                method: "getAll".to_string(),
                args: serde_json::json!({}),
            },
        )
        .unwrap();
        assert_eq!(
            all.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!([{
                    "id": "conn-1",
                    "host": "example.test",
                    "port": 22,
                    "username": "deploy",
                    "state": "active",
                    "refCount": 2,
                    "keepAlive": true,
                    "createdAt": "1970-01-01T00:00:01.000Z",
                    "lastActive": "1970-01-01T00:00:02.000Z",
                    "terminalIds": ["term-1"],
                    "parentConnectionId": null,
                }])
            }
        );

        let by_id = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "connections-get".to_string(),
                namespace: "connections".to_string(),
                method: "get".to_string(),
                args: serde_json::json!({ "connectionId": "conn-1" }),
            },
        )
        .unwrap();
        if let plugin_runtime::PluginResponseResult::Ok { value } = by_id.result {
            assert_eq!(value["host"], "example.test");
            assert_eq!(value["terminalIds"], serde_json::json!(["term-1"]));
        } else {
            panic!("connections.get returned an error");
        }

        let state = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "connections-state".to_string(),
                namespace: "connections".to_string(),
                method: "getState".to_string(),
                args: serde_json::json!({ "connectionId": "conn-1" }),
            },
        )
        .unwrap();
        assert_eq!(
            state.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!("active")
            }
        );

        let by_node = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "connections-node".to_string(),
                namespace: "connections".to_string(),
                method: "getByNode".to_string(),
                args: serde_json::json!({ "nodeId": "node-1" }),
            },
        )
        .unwrap();
        if let plugin_runtime::PluginResponseResult::Ok { value } = by_node.result {
            assert_eq!(value["id"], "conn-1");
        } else {
            panic!("connections.getByNode returned an error");
        }
    }

    #[test]
    fn connections_returnable_host_apis_return_null_for_missing_ids() {
        let snapshot = test_host_api_snapshot_with_connections();
        for (method, args) in [
            ("get", serde_json::json!({ "connectionId": "missing" })),
            ("getState", serde_json::json!({ "connectionId": "missing" })),
            ("getByNode", serde_json::json!({ "nodeId": "missing" })),
        ] {
            let response = native_plugin_returnable_host_api_response(
                &snapshot,
                "com.example.demo",
                plugin_runtime::PluginHostCall {
                    request_id: format!("connections-{method}-missing"),
                    namespace: "connections".to_string(),
                    method: method.to_string(),
                    args,
                },
            )
            .unwrap();
            assert_eq!(
                response.result,
                plugin_runtime::PluginResponseResult::Ok {
                    value: serde_json::Value::Null
                }
            );
        }
    }

    #[test]
    fn sessions_returnable_host_apis_match_tauri_snapshot_shape() {
        let snapshot = test_host_api_snapshot_with_sessions();
        let tree = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "sessions-tree".to_string(),
                namespace: "sessions".to_string(),
                method: "getTree".to_string(),
                args: serde_json::json!({}),
            },
        )
        .unwrap();
        assert_eq!(
            tree.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!([{
                    "id": "node-1",
                    "label": "Production",
                    "host": "example.test",
                    "port": 22,
                    "username": "deploy",
                    "parentId": null,
                    "childIds": ["node-2"],
                    "connectionState": "active",
                    "connectionId": "conn-1",
                    "terminalIds": ["term-1"],
                    "sftpSessionId": null,
                }, {
                    "id": "node-2",
                    "label": "root@child.test",
                    "host": "child.test",
                    "port": 2222,
                    "username": "root",
                    "parentId": "node-1",
                    "childIds": [],
                    "connectionState": "connecting",
                    "connectionId": null,
                    "terminalIds": [],
                    "sftpSessionId": "sftp-2",
                }])
            }
        );

        let active = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "sessions-active".to_string(),
                namespace: "sessions".to_string(),
                method: "getActiveNodes".to_string(),
                args: serde_json::json!({}),
            },
        )
        .unwrap();
        assert_eq!(
            active.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!([{
                    "nodeId": "node-1",
                    "sessionId": "term-1",
                    "connectionState": "active",
                }])
            }
        );

        let state = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "sessions-state".to_string(),
                namespace: "sessions".to_string(),
                method: "getNodeState".to_string(),
                args: serde_json::json!({ "nodeId": "node-2" }),
            },
        )
        .unwrap();
        assert_eq!(
            state.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!("connecting")
            }
        );
    }

    #[test]
    fn sessions_returnable_host_apis_return_null_for_missing_node() {
        let snapshot = test_host_api_snapshot_with_sessions();
        let state = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "sessions-state-missing".to_string(),
                namespace: "sessions".to_string(),
                method: "getNodeState".to_string(),
                args: serde_json::json!({ "nodeId": "missing" }),
            },
        )
        .unwrap();
        assert_eq!(
            state.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::Value::Null
            }
        );
    }

    #[test]
    fn session_connection_state_maps_link_down_to_tauri_status() {
        let state = oxideterm_ssh::NodeState {
            readiness: NodeReadiness::Error,
            error: Some("Link down".to_string()),
            sftp_ready: false,
            sftp_cwd: None,
            ws_endpoint: None,
        };
        assert_eq!(
            native_plugin_session_connection_state(&state, 0),
            "link-down"
        );
    }

    #[test]
    fn event_log_get_entries_filters_tauri_snapshot_shape() {
        let snapshot = test_host_api_snapshot_with_event_log_entries();
        let all = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "event-log-all".to_string(),
                namespace: "eventLog".to_string(),
                method: "getEntries".to_string(),
                args: serde_json::json!({}),
            },
        )
        .unwrap();
        assert_eq!(
            all.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!([{
                    "id": 1,
                    "timestamp": 1000,
                    "severity": "info",
                    "category": "connection",
                    "nodeId": "node-1",
                    "connectionId": "conn-1",
                    "title": "Connected",
                    "detail": "ready",
                    "source": "connection_status_changed",
                }, {
                    "id": 2,
                    "timestamp": 2000,
                    "severity": "error",
                    "category": "node",
                    "title": "Failed",
                    "source": "node_state_changed",
                }])
            }
        );

        let filtered = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "event-log-filtered".to_string(),
                namespace: "eventLog".to_string(),
                method: "getEntries".to_string(),
                args: serde_json::json!({
                    "filter": {
                        "severity": "error",
                        "category": "node",
                    }
                }),
            },
        )
        .unwrap();
        assert_eq!(
            filtered.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!([{
                    "id": 2,
                    "timestamp": 2000,
                    "severity": "error",
                    "category": "node",
                    "title": "Failed",
                    "source": "node_state_changed",
                }])
            }
        );
    }

    #[test]
    fn terminal_readonly_returnable_host_apis_use_node_snapshots() {
        let snapshot = test_host_api_snapshot_with_terminal();
        let active = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "terminal-active".to_string(),
                namespace: "terminal".to_string(),
                method: "getActiveTarget".to_string(),
                args: serde_json::json!({}),
            },
        )
        .unwrap();
        assert_eq!(
            active.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!({
                    "sessionId": "term-1",
                    "terminalType": "terminal",
                    "nodeId": "node-1",
                    "connectionId": "conn-1",
                    "connectionState": "active",
                    "label": "Production",
                })
            }
        );

        let buffer = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "terminal-buffer".to_string(),
                namespace: "terminal".to_string(),
                method: "getNodeBuffer".to_string(),
                args: serde_json::json!({ "nodeId": "node-1" }),
            },
        )
        .unwrap();
        assert_eq!(
            buffer.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!("alpha\nbeta\nAlpha")
            }
        );

        let selection = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "terminal-selection".to_string(),
                namespace: "terminal".to_string(),
                method: "getNodeSelection".to_string(),
                args: serde_json::json!({ "nodeId": "node-1" }),
            },
        )
        .unwrap();
        assert_eq!(
            selection.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!("beta")
            }
        );
    }

    #[test]
    fn terminal_search_scroll_and_size_are_bounded() {
        let snapshot = test_host_api_snapshot_with_terminal();
        let search = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "terminal-search".to_string(),
                namespace: "terminal".to_string(),
                method: "search".to_string(),
                args: serde_json::json!({
                    "nodeId": "node-1",
                    "query": "alpha",
                    "options": { "caseSensitive": false },
                }),
            },
        )
        .unwrap();
        assert_eq!(
            search.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!({
                    "matches": [
                        {
                            "line_number": 0,
                            "column_start": 0,
                            "column_end": 5,
                            "matched_text": "alpha",
                            "line_content": "alpha",
                        },
                        {
                            "line_number": 2,
                            "column_start": 0,
                            "column_end": 5,
                            "matched_text": "Alpha",
                            "line_content": "Alpha",
                        },
                    ],
                    "total_matches": 2,
                    "truncated": false,
                    "error": null,
                })
            }
        );

        let scroll = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "terminal-scroll".to_string(),
                namespace: "terminal".to_string(),
                method: "getScrollBuffer".to_string(),
                args: serde_json::json!({
                    "nodeId": "node-1",
                    "startLine": 1,
                    "count": 1,
                }),
            },
        )
        .unwrap();
        assert_eq!(
            scroll.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!([{ "text": "beta", "lineNumber": 1 }])
            }
        );

        let size = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "terminal-size".to_string(),
                namespace: "terminal".to_string(),
                method: "getBufferSize".to_string(),
                args: serde_json::json!({ "nodeId": "node-1" }),
            },
        )
        .unwrap();
        assert_eq!(
            size.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!({
                    "currentLines": 3,
                    "totalLines": 3,
                    "maxLines": 3,
                })
            }
        );
    }

    #[test]
    fn terminal_search_supports_regex_whole_word_and_invalid_regex() {
        let snapshot = test_host_api_snapshot_with_terminal();
        let whole_word = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "terminal-search-whole-word".to_string(),
                namespace: "terminal".to_string(),
                method: "search".to_string(),
                args: serde_json::json!({
                    "nodeId": "node-1",
                    "query": "alpha",
                    "options": { "wholeWord": true, "caseSensitive": false },
                }),
            },
        )
        .unwrap();
        assert_eq!(
            whole_word.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!({
                    "matches": [
                        {
                            "line_number": 0,
                            "column_start": 0,
                            "column_end": 5,
                            "matched_text": "alpha",
                            "line_content": "alpha",
                        },
                        {
                            "line_number": 2,
                            "column_start": 0,
                            "column_end": 5,
                            "matched_text": "Alpha",
                            "line_content": "Alpha",
                        },
                    ],
                    "total_matches": 2,
                    "truncated": false,
                    "error": null,
                })
            }
        );

        let regex = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "terminal-search-regex".to_string(),
                namespace: "terminal".to_string(),
                method: "search".to_string(),
                args: serde_json::json!({
                    "nodeId": "node-1",
                    "query": "^b.*a$",
                    "options": { "regex": true, "caseSensitive": true },
                }),
            },
        )
        .unwrap();
        if let plugin_runtime::PluginResponseResult::Ok { value } = regex.result {
            assert_eq!(value["total_matches"], 1);
            assert_eq!(value["matches"][0]["matched_text"], "beta");
        } else {
            panic!("terminal regex search returned an error response");
        }

        let invalid = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "terminal-search-invalid".to_string(),
                namespace: "terminal".to_string(),
                method: "search".to_string(),
                args: serde_json::json!({
                    "nodeId": "node-1",
                    "query": "[invalid(",
                    "options": { "regex": true },
                }),
            },
        )
        .unwrap();
        if let plugin_runtime::PluginResponseResult::Ok { value } = invalid.result {
            assert_eq!(value["total_matches"], 0);
            assert_eq!(value["matches"], serde_json::json!([]));
            assert!(value["error"].as_str().unwrap().contains("Invalid regex"));
        } else {
            panic!("invalid terminal regex search returned a protocol error");
        }
    }

    #[test]
    fn terminal_write_host_calls_parse_text_and_node_id() {
        let active = native_plugin_terminal_action_from_call(&plugin_runtime::PluginHostCall {
            request_id: "terminal-write-active".to_string(),
            namespace: "terminal".to_string(),
            method: "writeToActive".to_string(),
            args: serde_json::json!({ "text": "ls\n" }),
        })
        .unwrap();
        assert!(matches!(
            active,
            NativePluginTerminalAction::WriteActive { ref text } if text == "ls\n"
        ));

        let node = native_plugin_terminal_action_from_call(&plugin_runtime::PluginHostCall {
            request_id: "terminal-write-node".to_string(),
            namespace: "terminal".to_string(),
            method: "writeToNode".to_string(),
            args: serde_json::json!({ "nodeId": "node-1", "text": "pwd\n" }),
        })
        .unwrap();
        assert!(matches!(
            node,
            NativePluginTerminalAction::WriteNode { ref node_id, ref text }
                if node_id == "node-1" && text == "pwd\n"
        ));

        let clear = native_plugin_terminal_action_from_call(&plugin_runtime::PluginHostCall {
            request_id: "terminal-clear-buffer".to_string(),
            namespace: "terminal".to_string(),
            method: "clearBuffer".to_string(),
            args: serde_json::json!({ "nodeId": "node-1" }),
        })
        .unwrap();
        assert!(matches!(
            clear,
            NativePluginTerminalAction::ClearBuffer { ref node_id } if node_id == "node-1"
        ));

        let telnet = native_plugin_terminal_action_from_call(&plugin_runtime::PluginHostCall {
            request_id: "terminal-open-telnet".to_string(),
            namespace: "terminal".to_string(),
            method: "openTelnet".to_string(),
            args: serde_json::json!({ "host": " example.com ", "port": 2323 }),
        })
        .unwrap();
        assert!(matches!(
            telnet,
            NativePluginTerminalAction::OpenTelnet { ref host, port }
                if host == "example.com" && port == 2323
        ));
    }

    #[test]
    fn terminal_hook_response_values_parse_text_and_bytes() {
        assert_eq!(
            native_plugin_terminal_hook_text_value(&serde_json::json!({ "data": "cd /tmp\n" })),
            Some("cd /tmp\n".to_string())
        );
        assert_eq!(
            native_plugin_terminal_hook_bytes_value(&serde_json::json!([65, 66, 10])),
            Some(b"AB\n".to_vec())
        );
        assert_eq!(
            native_plugin_terminal_hook_bytes_value(&serde_json::json!({ "bytes": [120, 121] })),
            Some(b"xy".to_vec())
        );
        assert_eq!(
            native_plugin_terminal_hook_bytes_value(&serde_json::json!({ "bytes": [256] })),
            None
        );
        assert_eq!(native_plugin_terminal_hook_bytes_value(&Value::Null), None);
    }

    #[test]
    fn terminal_input_interceptors_run_in_order_and_fail_open() {
        let hooks = vec![
            test_terminal_hook("first", "demo.first"),
            test_terminal_hook("timeout", "demo.timeout"),
            test_terminal_hook("second", "demo.second"),
        ];
        let result =
            native_plugin_reduce_input_interceptors(b"ls", &hooks, |hook, args| {
                match hook.registration_id.as_str() {
                    "first" => {
                        assert_eq!(args["data"], "ls");
                        Some(json!({ "data": "sudo ls" }))
                    }
                    "timeout" => None,
                    "second" => {
                        assert_eq!(args["data"], "sudo ls");
                        Some(json!("sudo ls -la"))
                    }
                    _ => unreachable!(),
                }
            });

        match result {
            TerminalInputInterceptorResult::Continue(bytes) => {
                assert_eq!(bytes, b"sudo ls -la");
            }
            TerminalInputInterceptorResult::Suppress => panic!("input should not be suppressed"),
        }
    }

    #[test]
    fn terminal_input_interceptor_null_suppresses_input() {
        let hooks = vec![test_terminal_hook("suppress", "demo.suppress")];
        let result =
            native_plugin_reduce_input_interceptors(b"rm -rf /tmp/demo", &hooks, |_, _| {
                Some(Value::Null)
            });
        assert!(matches!(result, TerminalInputInterceptorResult::Suppress));
    }

    #[test]
    fn terminal_output_processors_preserve_bytes_on_failure() {
        let hooks = vec![
            test_terminal_hook("first", "demo.first"),
            test_terminal_hook("error", "demo.error"),
            test_terminal_hook("second", "demo.second"),
        ];
        let output =
            native_plugin_reduce_output_processors(b"abc", &hooks, |hook, args| {
                match hook.registration_id.as_str() {
                    "first" => {
                        assert_eq!(args["bytes"], json!([97, 98, 99]));
                        Some(json!({ "bytes": [65, 66, 67] }))
                    }
                    "error" => None,
                    "second" => {
                        assert_eq!(args["bytes"], json!([65, 66, 67]));
                        Some(json!("done"))
                    }
                    _ => unreachable!(),
                }
            });
        assert_eq!(output, b"done");
    }

    #[test]
    fn plugin_secret_account_ids_are_plugin_scoped_and_validated() {
        assert_eq!(
            native_plugin_secret_account_id("com.example.alpha", "token").unwrap(),
            "plugin-secret:17:com.example.alpha:5:token"
        );
        assert_ne!(
            native_plugin_secret_account_id("com.example.alpha", "token").unwrap(),
            native_plugin_secret_account_id("com.example.beta", "token").unwrap()
        );
        assert!(native_plugin_secret_account_id("com.example.alpha", "").is_err());
        assert!(native_plugin_secret_account_id("com.example.alpha", "bad\nkey").is_err());
        assert!(native_plugin_secret_account_id("../escape", "token").is_err());
    }

    #[test]
    fn i18n_returnable_host_apis_use_plugin_scoped_fallback() {
        let snapshot = test_host_api_snapshot();
        let language = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "i18n-language".to_string(),
                namespace: "i18n".to_string(),
                method: "getLanguage".to_string(),
                args: serde_json::json!({}),
            },
        )
        .unwrap();
        assert_eq!(
            language.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!("zh-CN")
            }
        );

        let translated = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "i18n-t".to_string(),
                namespace: "i18n".to_string(),
                method: "t".to_string(),
                args: serde_json::json!({ "key": "missing.title" }),
            },
        )
        .unwrap();
        assert_eq!(
            translated.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!("missing.title")
            }
        );
    }

    #[test]
    fn settings_get_returnable_host_api_uses_declared_defaults() {
        let snapshot = test_host_api_snapshot_with_declared_setting();
        let value = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "settings-get".to_string(),
                namespace: "settings".to_string(),
                method: "get".to_string(),
                args: serde_json::json!({ "key": "mode" }),
            },
        )
        .unwrap();
        assert_eq!(
            value.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!("auto")
            }
        );

        let undeclared = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "settings-get-undeclared".to_string(),
                namespace: "settings".to_string(),
                method: "get".to_string(),
                args: serde_json::json!({ "key": "unknown" }),
            },
        )
        .unwrap();
        assert_eq!(
            undeclared.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::Value::Null
            }
        );
    }

    #[test]
    fn syncable_settings_export_returns_tauri_shaped_payload() {
        let snapshot = test_host_api_snapshot();
        let response = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "settings-export".to_string(),
                namespace: "settings".to_string(),
                method: "exportSyncableSettings".to_string(),
                args: serde_json::json!({}),
            },
        )
        .unwrap();

        let plugin_runtime::PluginResponseResult::Ok { value } = response.result else {
            panic!("expected exportSyncableSettings to succeed");
        };
        assert_eq!(value["payload"]["appearance"]["language"], "zh-CN");
        assert_eq!(value["payload"]["appearance"]["uiDensity"], "comfortable");
        assert_eq!(value["payload"]["terminal"]["fontSize"], 14);
        assert_eq!(value["payload"]["terminal"]["theme"], "default");
        assert_eq!(value["payload"]["reconnect"]["autoReconnect"], true);
        assert_eq!(value["warnings"], serde_json::json!([]));
        assert!(
            value["revision"]
                .as_str()
                .is_some_and(|revision| { revision.starts_with("fnv1a-") })
        );
        assert!(
            value["exportedAt"]
                .as_str()
                .is_some_and(|exported_at| { exported_at.ends_with('Z') })
        );
    }

    #[test]
    fn syncable_settings_apply_normalizes_payload_and_warnings() {
        let normalized = native_normalize_syncable_settings_payload(&serde_json::json!({
            "appearance": {
                "language": "xx-XX",
                "uiDensity": "wide",
            },
            "terminal": {
                "fontSize": 100.4,
                "theme": "   ",
            },
            "reconnect": {
                "autoReconnect": "yes",
            },
        }));

        assert_eq!(
            normalized.payload,
            serde_json::json!({
                "terminal": { "fontSize": 32 }
            })
        );
        assert_eq!(
            normalized.warnings,
            vec![
                serde_json::json!({
                    "path": "appearance.language",
                    "code": "unsupported-language",
                    "applied": false,
                    "message": "Unsupported language: xx-XX",
                }),
                serde_json::json!({
                    "path": "appearance.uiDensity",
                    "code": "invalid-ui-density",
                    "applied": false,
                    "message": "Unsupported ui density: wide",
                }),
                serde_json::json!({
                    "path": "terminal.fontSize",
                    "code": "font-size-clamped",
                    "applied": true,
                    "message": "Font size was clamped to 32",
                    "normalizedValue": 32,
                }),
                serde_json::json!({
                    "path": "terminal.theme",
                    "code": "missing-theme",
                    "applied": false,
                    "message": "Theme id cannot be empty",
                }),
                serde_json::json!({
                    "path": "reconnect.autoReconnect",
                    "code": "invalid-auto-reconnect",
                    "applied": false,
                    "message": "autoReconnect must be a boolean",
                }),
            ]
        );
    }

    #[test]
    fn syncable_settings_apply_returnable_host_api_reports_applied_payload() {
        let snapshot = test_host_api_snapshot();
        let response = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "settings-apply".to_string(),
                namespace: "settings".to_string(),
                method: "applySyncableSettings".to_string(),
                args: serde_json::json!({
                    "payload": {
                        "appearance": { "language": "ja", "uiDensity": "compact" },
                        "terminal": { "fontSize": 16, "theme": "solarized-dark" },
                        "reconnect": { "autoReconnect": false },
                    }
                }),
            },
        )
        .unwrap();

        let expected_payload = serde_json::json!({
            "appearance": { "language": "ja", "uiDensity": "compact" },
            "terminal": { "fontSize": 16, "theme": "solarized-dark" },
            "reconnect": { "autoReconnect": false },
        });
        assert_eq!(
            response.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!({
                    "revision": native_syncable_settings_revision(&expected_payload),
                    "appliedPayload": expected_payload,
                    "warnings": [],
                })
            }
        );
    }

    #[test]
    fn custom_event_emit_returnable_host_api_is_plugin_scoped() {
        let snapshot = test_host_api_snapshot();
        let response = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: "events-emit".to_string(),
                namespace: "events".to_string(),
                method: "emit".to_string(),
                args: serde_json::json!({
                    "name": "build.done",
                    "payload": { "ok": true },
                }),
            },
        )
        .unwrap();

        assert_eq!(
            response.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::json!({
                    "emitted": true,
                    "event": "plugin.com.example.demo:build.done",
                })
            }
        );
        let (event_key, payload) = native_plugin_custom_event_from_args(
            "com.example.demo",
            serde_json::json!({
                "name": "build.done",
                "payload": { "ok": true },
            }),
        )
        .unwrap();
        assert_eq!(event_key, "plugin.com.example.demo:build.done");
        assert_eq!(payload["pluginId"], "com.example.demo");
        assert_eq!(payload["name"], "build.done");
        assert_eq!(payload["payload"], serde_json::json!({ "ok": true }));
    }

    fn test_terminal_hook(
        registration_id: &str,
        command: &str,
    ) -> super::super::plugin_host::NativePluginRuntimeTerminalHookContribution {
        super::super::plugin_host::NativePluginRuntimeTerminalHookContribution {
            plugin_id: "com.example.demo".to_string(),
            plugin_name: "Demo".to_string(),
            registration_id: registration_id.to_string(),
            command: command.to_string(),
        }
    }

    fn test_host_api_snapshot() -> NativePluginHostApiSnapshot {
        NativePluginHostApiSnapshot {
            registry: super::super::plugin_host::NativePluginRegistry::default(),
            i18n: I18n::new(oxideterm_i18n::Locale::ZhCn),
            settings: serde_json::to_value(oxideterm_settings::PersistedSettings::default())
                .unwrap(),
            locale: "zh-CN".to_string(),
            theme_name: "default".to_string(),
            pool_stats: serde_json::json!({
                "activeConnections": 0,
                "totalSessions": 0,
            }),
            layout: native_plugin_layout_snapshot(false, None, 0),
            connections: Vec::new(),
            connection_states: HashMap::new(),
            node_connection_ids: HashMap::new(),
            session_tree: Vec::new(),
            session_node_states: HashMap::new(),
            event_log_entries: Vec::new(),
            active_terminal_target: Value::Null,
            terminal_nodes: HashMap::new(),
        }
    }

    fn test_host_api_snapshot_with_connections() -> NativePluginHostApiSnapshot {
        let connection = ConnectionInfo {
            connection_id: "conn-1".to_string(),
            key: "redacted-key".to_string(),
            host: "example.test".to_string(),
            port: 22,
            username: "deploy".to_string(),
            parent_connection_id: None,
            state: ConnectionState::Active,
            ref_count: 2,
            keep_alive: true,
            consumers: vec![
                ConnectionConsumer::Sftp("sftp-1".to_string()),
                ConnectionConsumer::Terminal("term-1".to_string()),
            ],
            created_at: std::time::UNIX_EPOCH + Duration::from_secs(1),
            last_active_at: std::time::UNIX_EPOCH + Duration::from_secs(2),
            idle_timeout_secs: Some(1800),
        };
        let connections = vec![native_plugin_connection_snapshot(&connection)];
        let connection_states = HashMap::from([(
            connection.connection_id.clone(),
            native_plugin_connection_state(&connection.state),
        )]);
        let node_connection_ids =
            HashMap::from([("node-1".to_string(), connection.connection_id.clone())]);
        NativePluginHostApiSnapshot {
            connections,
            connection_states,
            node_connection_ids,
            ..test_host_api_snapshot()
        }
    }

    fn test_host_api_snapshot_with_event_log_entries() -> NativePluginHostApiSnapshot {
        let entries = vec![
            EventLogEntry {
                id: 1,
                timestamp: std::time::UNIX_EPOCH + Duration::from_secs(1),
                severity: EventSeverity::Info,
                category: EventCategory::Connection,
                node_id: Some("node-1".to_string()),
                connection_id: Some("conn-1".to_string()),
                title: "Connected".to_string(),
                detail: Some("ready".to_string()),
                source: "connection_status_changed",
            },
            EventLogEntry {
                id: 2,
                timestamp: std::time::UNIX_EPOCH + Duration::from_secs(2),
                severity: EventSeverity::Error,
                category: EventCategory::Node,
                node_id: None,
                connection_id: None,
                title: "Failed".to_string(),
                detail: None,
                source: "node_state_changed",
            },
        ];
        let event_log_entries = native_plugin_event_log_entries(entries.iter());
        NativePluginHostApiSnapshot {
            event_log_entries,
            ..test_host_api_snapshot()
        }
    }

    fn test_host_api_snapshot_with_terminal() -> NativePluginHostApiSnapshot {
        NativePluginHostApiSnapshot {
            active_terminal_target: serde_json::json!({
                "sessionId": "term-1",
                "terminalType": "terminal",
                "nodeId": "node-1",
                "connectionId": "conn-1",
                "connectionState": "active",
                "label": "Production",
            }),
            terminal_nodes: HashMap::from([(
                "node-1".to_string(),
                NativePluginTerminalNodeSnapshot {
                    buffer: "alpha\nbeta\nAlpha".to_string(),
                    selection: Some("beta".to_string()),
                    current_lines: 3,
                },
            )]),
            ..test_host_api_snapshot()
        }
    }

    fn test_host_api_snapshot_with_sessions() -> NativePluginHostApiSnapshot {
        let root_id = oxideterm_ssh::NodeId::new("node-1");
        let child_id = oxideterm_ssh::NodeId::new("node-2");
        let nodes = vec![
            NodeTreeSnapshotNode {
                id: root_id.clone(),
                parent_id: None,
                children_ids: vec![child_id.clone()],
                depth: 0,
                config: oxideterm_ssh::SshConfig {
                    host: "example.test".to_string(),
                    port: 22,
                    username: "deploy".to_string(),
                    ..oxideterm_ssh::SshConfig::default()
                },
                origin: oxideterm_ssh::NodeOrigin::Direct,
                state: oxideterm_ssh::NodeState {
                    readiness: NodeReadiness::Ready,
                    error: None,
                    sftp_ready: false,
                    sftp_cwd: None,
                    ws_endpoint: None,
                },
                connection_id: Some("conn-1".to_string()),
                terminal_session_id: Some("term-legacy".to_string()),
                sftp_session_id: None,
                created_at_ms: 1,
                generation: 1,
            },
            NodeTreeSnapshotNode {
                id: child_id,
                parent_id: Some(root_id),
                children_ids: Vec::new(),
                depth: 1,
                config: oxideterm_ssh::SshConfig {
                    host: "child.test".to_string(),
                    port: 2222,
                    username: "root".to_string(),
                    ..oxideterm_ssh::SshConfig::default()
                },
                origin: oxideterm_ssh::NodeOrigin::DrillDown { timestamp: 1 },
                state: oxideterm_ssh::NodeState {
                    readiness: NodeReadiness::Connecting,
                    error: None,
                    sftp_ready: false,
                    sftp_cwd: None,
                    ws_endpoint: None,
                },
                connection_id: None,
                terminal_session_id: None,
                sftp_session_id: Some("sftp-2".to_string()),
                created_at_ms: 2,
                generation: 1,
            },
        ];
        let titles = HashMap::from([("node-1".to_string(), "Production".to_string())]);
        let terminal_ids = HashMap::from([("node-1".to_string(), vec!["term-1".to_string()])]);
        let session_tree = native_plugin_session_tree_from_nodes(nodes, &titles, &terminal_ids);
        let session_node_states = native_plugin_session_state_map_from_nodes(&session_tree);
        NativePluginHostApiSnapshot {
            session_tree,
            session_node_states,
            ..test_host_api_snapshot()
        }
    }

    fn test_host_api_snapshot_with_declared_setting() -> NativePluginHostApiSnapshot {
        let temp_dir = std::env::temp_dir().join(format!(
            "oxideterm-plugin-lifecycle-settings-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let settings_path = temp_dir.join("settings.json");
        let plugin_dir = super::super::plugin_host::native_plugins_dir(&settings_path).join("demo");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        let manifest_path = plugin_dir.join("plugin.json");
        let manifest = serde_json::json!({
            "id": "com.example.demo",
            "name": "Demo",
            "version": "1.0.0",
            "runtime": { "kind": "manifest-only", "entry": "plugin.json" },
            "contributes": {
                "settings": [{
                    "id": "mode",
                    "type": "select",
                    "default": "auto",
                    "title": "Mode",
                    "options": [{ "label": "Auto", "value": "auto" }]
                }]
            }
        });
        std::fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        let registry = super::super::plugin_host::NativePluginRegistry::discover(&settings_path);
        let _ = std::fs::remove_dir_all(&temp_dir);
        NativePluginHostApiSnapshot {
            registry,
            ..test_host_api_snapshot()
        }
    }

    fn test_host_api_snapshot_with_declared_api_commands() -> NativePluginHostApiSnapshot {
        let temp_dir = std::env::temp_dir().join(format!(
            "oxideterm-plugin-lifecycle-api-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let settings_path = temp_dir.join("settings.json");
        let plugin_dir = super::super::plugin_host::native_plugins_dir(&settings_path).join("demo");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        let manifest_path = plugin_dir.join("plugin.json");
        let mut api_commands = native_plugin_supported_backend_commands().to_vec();
        api_commands.push("custom_declared_command");
        let manifest = serde_json::json!({
            "id": "com.example.demo",
            "name": "Demo",
            "version": "1.0.0",
            "runtime": { "kind": "manifest-only", "entry": "plugin.json" },
            "contributes": {
                "apiCommands": api_commands
            }
        });
        std::fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        let registry = super::super::plugin_host::NativePluginRegistry::discover(&settings_path);
        let _ = std::fs::remove_dir_all(&temp_dir);
        NativePluginHostApiSnapshot {
            registry,
            ..test_host_api_snapshot()
        }
    }

    fn test_connection_store(name: &str) -> oxideterm_connections::ConnectionStore {
        let path = std::env::temp_dir().join(format!(
            "oxideterm-plugin-lifecycle-{name}-{}.json",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        oxideterm_connections::ConnectionStore::load(path).unwrap()
    }

    fn test_connection_store_with_agent_connection(
        name: &str,
    ) -> oxideterm_connections::ConnectionStore {
        let mut store = test_connection_store(name);
        store
            .upsert(oxideterm_connections::SaveConnectionRequest {
                id: Some("conn-1".to_string()),
                name: "Home".to_string(),
                group: None,
                host: "192.168.1.2".to_string(),
                port: 22,
                username: "me".to_string(),
                auth: oxideterm_connections::SavedAuth::Agent,
                proxy_chain: Vec::new(),
                color: None,
                tags: Vec::new(),
                agent_forwarding: false,
                post_connect_command: None,
            })
            .unwrap();
        store
    }
}
