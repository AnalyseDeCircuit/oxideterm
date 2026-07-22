// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::VecDeque,
    sync::{Arc, mpsc},
    time::Instant,
};

use oxideterm_connections::{
    SavedConnectionsConflictStrategy, SavedConnectionsSyncSnapshot,
    oxide_file::ImportResultEnvelope,
};
use oxideterm_plugin_host_api::sync::NativePluginOxideImportOptions;
use serde_json::Value;
use zeroize::Zeroizing;

use crate::workspace::{plugin_host, plugin_runtime};

/// Owns native plugin runtime coordination and emitted host snapshots.
pub(in crate::workspace) struct NativePluginRuntimeState {
    pub(in crate::workspace) registry: plugin_host::NativePluginRegistry,
    pub(in crate::workspace) host: Arc<tokio::sync::Mutex<plugin_runtime::NativePluginRuntimeHost>>,
    pub(in crate::workspace) confirm_tx: mpsc::Sender<NativePluginConfirmRequest>,
    pub(in crate::workspace) confirm_rx: mpsc::Receiver<NativePluginConfirmRequest>,
    pub(in crate::workspace) confirm: Option<NativePluginConfirmDialog>,
    pub(in crate::workspace) confirm_presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub(in crate::workspace) confirm_polling: bool,
    pub(in crate::workspace) terminal_tx: mpsc::Sender<NativePluginTerminalRequest>,
    pub(in crate::workspace) terminal_rx: mpsc::Receiver<NativePluginTerminalRequest>,
    pub(in crate::workspace) terminal_ui_requests: VecDeque<NativePluginTerminalRequest>,
    pub(in crate::workspace) terminal_polling: bool,
    pub(in crate::workspace) product_ui_effects: VecDeque<NativePluginProductUiEffect>,
    pub(in crate::workspace) sync_tx: mpsc::Sender<NativePluginSyncRequest>,
    pub(in crate::workspace) sync_rx: mpsc::Receiver<NativePluginSyncRequest>,
    pub(in crate::workspace) sync_polling: bool,
    pub(in crate::workspace) services_started: bool,
    pub(in crate::workspace) layout_snapshot: Value,
    pub(in crate::workspace) layout_polling: bool,
    pub(in crate::workspace) session_tree_snapshot: Value,
    pub(in crate::workspace) session_polling: bool,
    pub(in crate::workspace) saved_forwards_snapshot: Value,
    pub(in crate::workspace) saved_forwards_polling: bool,
    pub(in crate::workspace) transfer_snapshot: Value,
    pub(in crate::workspace) transfer_polling: bool,
    pub(in crate::workspace) transfer_progress_last_emitted: Option<Instant>,
    pub(in crate::workspace) profiler_snapshot: Value,
    pub(in crate::workspace) profiler_polling: bool,
    pub(in crate::workspace) profiler_last_emitted: Option<Instant>,
    pub(in crate::workspace) ide_snapshot: Value,
    pub(in crate::workspace) ide_polling: bool,
    pub(in crate::workspace) ai_snapshot: Value,
    pub(in crate::workspace) ai_polling: bool,
    pub(in crate::workspace) event_log_last_id: u64,
    pub(in crate::workspace) event_log_polling: bool,
}

impl NativePluginRuntimeState {
    pub(in crate::workspace) fn new(registry: plugin_host::NativePluginRegistry) -> Self {
        // Runtime request channels are created together so every endpoint has
        // the same lifetime as the registry and runtime host that use it.
        let (confirm_tx, confirm_rx) = mpsc::channel();
        let (terminal_tx, terminal_rx) = mpsc::channel();
        let (sync_tx, sync_rx) = mpsc::channel();
        Self {
            registry,
            host: Arc::new(tokio::sync::Mutex::new(
                plugin_runtime::NativePluginRuntimeHost::default(),
            )),
            confirm_tx,
            confirm_rx,
            confirm: None,
            confirm_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            confirm_polling: false,
            terminal_tx,
            terminal_rx,
            terminal_ui_requests: VecDeque::new(),
            terminal_polling: false,
            product_ui_effects: VecDeque::new(),
            sync_tx,
            sync_rx,
            sync_polling: false,
            services_started: false,
            layout_snapshot: Value::Null,
            layout_polling: false,
            session_tree_snapshot: Value::Null,
            session_polling: false,
            saved_forwards_snapshot: Value::Null,
            saved_forwards_polling: false,
            transfer_snapshot: Value::Null,
            transfer_polling: false,
            transfer_progress_last_emitted: None,
            profiler_snapshot: Value::Null,
            profiler_polling: false,
            profiler_last_emitted: None,
            ide_snapshot: Value::Null,
            ide_polling: false,
            ai_snapshot: Value::Null,
            ai_polling: false,
            event_log_last_id: 0,
            event_log_polling: false,
        }
    }
}

pub(in crate::workspace) enum NativePluginRuntimeDelivery {
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

pub(in crate::workspace) struct NativePluginConfirmRequest {
    pub(super) plugin_id: String,
    pub(super) request_id: String,
    pub(super) title: String,
    pub(super) description: String,
    pub(super) response_tx: mpsc::Sender<bool>,
}

pub(in crate::workspace) struct NativePluginConfirmDialog {
    pub(super) plugin_id: String,
    pub(super) request_id: String,
    pub(super) title: String,
    pub(super) description: String,
    pub(super) response_tx: mpsc::Sender<bool>,
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
    pub(in crate::workspace) fn respond(&self, confirmed: bool) {
        // Keep the request identity alive with the retained exit-frame payload.
        let _request_id = &self.request_id;
        let _ = self.response_tx.send(confirmed);
    }
}

pub(in crate::workspace) struct NativePluginTerminalRequest {
    pub(super) request_id: String,
    pub(super) action: NativePluginTerminalAction,
    pub(super) response_tx: mpsc::Sender<plugin_runtime::PluginResponse>,
}

pub(in crate::workspace) enum NativePluginTerminalAction {
    WriteActive { text: String },
    WriteNode { node_id: String, text: String },
    ClearBuffer { node_id: String },
    OpenTelnet { host: String, port: u16 },
}

/// Holds window-owned plugin effects until the next workspace render pass.
pub(in crate::workspace) struct NativePluginProductUiEffect {
    pub(super) plugin_id: String,
    pub(super) namespace: String,
    pub(super) method: String,
    pub(super) args: Value,
}

pub(in crate::workspace) struct NativePluginSyncRequest {
    pub(super) request_id: String,
    pub(super) action: NativePluginSyncAction,
    pub(super) response_tx: mpsc::Sender<plugin_runtime::PluginResponse>,
}

pub(in crate::workspace) enum NativePluginSyncAction {
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

pub(in crate::workspace) struct NativePluginOxideImportCoreResult {
    pub(super) store: oxideterm_connections::ConnectionStore,
    pub(super) envelope: ImportResultEnvelope,
}

pub(in crate::workspace) enum NativePluginOxideImportWorkerMessage {
    Progress {
        stage: String,
        current: usize,
        total: usize,
    },
    Done(Result<NativePluginOxideImportCoreResult, String>),
}
