// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::{HashMap, HashSet},
    sync::mpsc,
};

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
use serde_json::{Map, Value, json};
use zeroize::Zeroizing;

use super::{
    native_plugin_u8_array,
    types::{NativePluginOxideImportOptions, NativePluginSyncAction, NativePluginSyncRequest},
};
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

pub(super) fn native_plugin_sync_apply_saved_connections_args(
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

pub(super) fn native_plugin_sync_progress_value(
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

pub(super) fn native_plugin_settings_revision_map(
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

pub(super) fn native_plugin_sync_import_oxide_args(
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
pub(super) fn native_plugin_apply_oxide_import_core(
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

pub(super) fn native_plugin_apply_oxide_import_core_with_progress<F>(
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

pub(super) fn native_plugin_sync_import_result_value(
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
