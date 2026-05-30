// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::sync::mpsc;

use oxideterm_connections::{
    LocalSyncMetadata as SavedConnectionsLocalSyncMetadata, SavedConnectionsSyncSnapshot,
    oxide_file::{
        ImportConflictStrategy, OxideExportOptions, OxideFile, export_connections_to_oxide,
        export_connections_to_oxide_with_progress, preflight_export, preview_oxide_import,
        preview_oxide_import_with_progress,
    },
};
#[cfg(test)]
pub(super) use oxideterm_plugin_host_api::sync::native_plugin_apply_oxide_import_core;
pub(super) use oxideterm_plugin_host_api::sync::{
    NativePluginOxideImportOptions, NativePluginQuickCommandImportStrategy,
    native_plugin_apply_oxide_import_core_with_progress, native_plugin_bool_arg,
    native_plugin_file_data_arg, native_plugin_optional_string_arg,
    native_plugin_selected_plugin_settings, native_plugin_settings_revision_map,
    native_plugin_sync_apply_saved_connections_args, native_plugin_sync_connection_ids,
    native_plugin_sync_import_oxide_args, native_plugin_sync_import_result_value,
    native_plugin_sync_oxide_error, native_plugin_sync_progress_registration_id,
    native_plugin_sync_progress_value,
};
use serde_json::{Map, Value, json};
use zeroize::Zeroizing;

use super::types::{NativePluginSyncAction, NativePluginSyncRequest};
use crate::workspace::{plugin_runtime, quick_commands::QuickCommandImportStrategy};

// Sync owns the plugin-facing .oxide and saved-connection protocol. Mutating
// operations are routed back through Workspace so cloned snapshots cannot
// accidentally acknowledge state the application did not apply.
pub(super) fn native_plugin_sync_response(
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

pub(super) fn native_plugin_sync_export_oxide_response(
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
        ..OxideExportOptions::default()
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

pub(super) fn native_plugin_emit_sync_progress(
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

pub(super) fn native_plugin_quick_command_import_strategy(
    strategy: NativePluginQuickCommandImportStrategy,
) -> QuickCommandImportStrategy {
    match strategy {
        NativePluginQuickCommandImportStrategy::Rename => QuickCommandImportStrategy::Rename,
        NativePluginQuickCommandImportStrategy::Skip => QuickCommandImportStrategy::Skip,
        NativePluginQuickCommandImportStrategy::Replace => QuickCommandImportStrategy::Replace,
        NativePluginQuickCommandImportStrategy::Merge => QuickCommandImportStrategy::Merge,
    }
}
