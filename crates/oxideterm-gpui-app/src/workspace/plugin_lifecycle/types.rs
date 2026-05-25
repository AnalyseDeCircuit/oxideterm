// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::sync::mpsc;

use oxideterm_connections::{
    SavedConnectionsConflictStrategy, SavedConnectionsSyncSnapshot,
    oxide_file::ImportResultEnvelope,
};
use oxideterm_plugin_host_api::sync::NativePluginOxideImportOptions;
use serde_json::Value;
use zeroize::Zeroizing;

use crate::workspace::plugin_runtime;

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
    pub(in crate::workspace) fn respond(self, confirmed: bool) {
        let _request_id = self.request_id;
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
