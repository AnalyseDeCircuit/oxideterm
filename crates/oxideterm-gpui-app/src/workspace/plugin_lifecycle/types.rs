// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
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
        Self {
            registry,
            host: Arc::new(tokio::sync::Mutex::new(
                plugin_runtime::NativePluginRuntimeHost::default(),
            )),
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
    pub(in crate::workspace) plugin_id: String,
    pub(in crate::workspace) request_id: String,
    pub(in crate::workspace) title: String,
    pub(in crate::workspace) description: String,
    pub(in crate::workspace) response_tx: mpsc::Sender<bool>,
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
    pub(in crate::workspace) request_id: String,
    pub(in crate::workspace) action: NativePluginTerminalAction,
    pub(in crate::workspace) response_tx: mpsc::Sender<plugin_runtime::PluginResponse>,
}

pub(in crate::workspace) enum NativePluginTerminalAction {
    WriteActive { text: String },
    WriteNode { node_id: String, text: String },
    ClearBuffer { node_id: String },
    OpenTelnet { host: String, port: u16 },
}

/// Describes a plugin effect that must be applied with a live workspace window.
pub(in crate::workspace) struct NativePluginProductUiEffect {
    pub(in crate::workspace) plugin_id: String,
    pub(in crate::workspace) namespace: String,
    pub(in crate::workspace) method: String,
    pub(in crate::workspace) args: Value,
}

pub(in crate::workspace) struct NativePluginSyncRequest {
    pub(in crate::workspace) request_id: String,
    pub(in crate::workspace) action: NativePluginSyncAction,
    pub(in crate::workspace) response_tx: mpsc::Sender<plugin_runtime::PluginResponse>,
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
