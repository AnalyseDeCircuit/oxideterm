// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::sync::mpsc;

use oxideterm_connections::{
    SavedConnectionsConflictStrategy, SavedConnectionsSyncSnapshot,
    oxide_file::ImportResultEnvelope,
};
use oxideterm_plugin_host_api::sync::{
    NativePluginOxideImportOptions, NativePluginQuickCommandImportStrategy,
};
use serde_json::Value;
use zeroize::Zeroizing;

use crate::workspace::plugin_runtime;

pub(in crate::workspace) enum NativePluginRuntimeDelivery {
    Activation {
        plugin_id: String,
        result: Result<plugin_runtime::NativePluginRuntimeActivation, plugin_runtime::PluginError>,
    },
    Deactivation {
        plugin_id: String,
        result: Result<plugin_runtime::PluginResponse, plugin_runtime::PluginError>,
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
    pub(in crate::workspace) store: oxideterm_connections::ConnectionStore,
    pub(in crate::workspace) envelope: ImportResultEnvelope,
}

/// Options applied on the GPUI owner after the background import commits.
pub(in crate::workspace) struct NativePluginOxidePostImportOptions {
    pub(in crate::workspace) import_app_settings: bool,
    pub(in crate::workspace) selected_app_settings_sections:
        Option<std::collections::HashSet<String>>,
    pub(in crate::workspace) import_plugin_settings: bool,
    pub(in crate::workspace) selected_plugin_ids: Option<std::collections::HashSet<String>>,
    pub(in crate::workspace) import_quick_commands: bool,
    pub(in crate::workspace) quick_command_strategy: NativePluginQuickCommandImportStrategy,
}

pub(in crate::workspace) enum NativePluginOxideImportWorkerMessage {
    Progress {
        operation_id: u64,
        stage: String,
        current: usize,
        total: usize,
    },
    Done {
        operation_id: u64,
        result: Result<NativePluginOxideImportCoreResult, ()>,
    },
}
