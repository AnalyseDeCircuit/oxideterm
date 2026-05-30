// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Sync host API DTO parsing and plugin-facing response shaping.
//!
//! Workspace mutation stays in `oxideterm-gpui-app`; this module owns the pure
//! `.oxide` argument adapters, revision calculations, and import result payloads.

use std::collections::{HashMap, HashSet};

use oxideterm_connections::{
    SavedConnectionsConflictStrategy, SavedConnectionsSyncSnapshot,
    oxide_file::{
        EncryptedPluginSetting, ImportConflictStrategy, ImportResultEnvelope, OxideFileError,
        OxideImportOptions, apply_oxide_import_with_options_with_progress,
    },
};
use oxideterm_plugin_protocol as plugin_runtime;
use serde_json::{Map, Value, json};
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativePluginQuickCommandImportStrategy {
    Rename,
    Skip,
    Replace,
    Merge,
}

#[derive(Clone, Debug)]
pub struct NativePluginOxideImportOptions {
    pub oxide_options: OxideImportOptions,
    pub import_app_settings: bool,
    pub selected_app_settings_sections: Option<HashSet<String>>,
    pub import_plugin_settings: bool,
    pub selected_plugin_ids: Option<HashSet<String>>,
    pub import_quick_commands: bool,
    pub quick_command_strategy: NativePluginQuickCommandImportStrategy,
}

pub fn native_plugin_sync_apply_saved_connections_args(
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

pub fn native_plugin_sync_progress_registration_id(args: &Value) -> Option<String> {
    args.get("progressRegistrationId")
        .or_else(|| args.get("progressId"))
        .and_then(Value::as_str)
        .filter(|registration_id| !registration_id.is_empty())
        .map(str::to_string)
}

pub fn native_plugin_sync_progress_value(
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

pub fn native_plugin_selected_plugin_settings(
    plugin_settings: &[EncryptedPluginSetting],
    args: &Value,
) -> Result<Vec<EncryptedPluginSetting>, String> {
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

pub fn native_plugin_settings_revision_map(
    plugin_settings: &[EncryptedPluginSetting],
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

pub fn native_plugin_sync_import_oxide_args(
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
            selected_forward_ids: native_plugin_optional_string_array_arg(
                args,
                "selectedForwardIds",
            )?,
            conflict_strategy,
            import_forwards: native_plugin_bool_arg(args, "importForwards").unwrap_or(true),
            import_portable_secrets: native_plugin_bool_arg(args, "importPortableSecrets")
                .unwrap_or(false),
            ..OxideImportOptions::default()
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

pub fn native_plugin_apply_oxide_import_core_with_progress<F>(
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

pub fn native_plugin_apply_oxide_import_core(
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

pub fn native_plugin_sync_import_result_value(
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
        // PluginContext consumes side-car payloads inside the host and returns
        // only application-visible import outcomes to plugin runtimes.
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

pub fn native_plugin_sync_connection_ids(
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

pub fn native_plugin_file_data_arg(args: &Value) -> Result<Vec<u8>, String> {
    let Some(file_data) = args.get("fileData").and_then(Value::as_array) else {
        return Err("oxide fileData must be an array of bytes".to_string());
    };
    native_plugin_u8_array(file_data)
        .ok_or_else(|| "oxide fileData contains a non-byte value".to_string())
}

pub fn native_plugin_sync_oxide_error(
    request_id: String,
    error: OxideFileError,
) -> plugin_runtime::PluginResponse {
    plugin_runtime::PluginResponse::error(
        request_id,
        plugin_runtime::PluginError::runtime("plugin_sync_oxide_error", error.to_string()),
    )
}

pub fn native_plugin_oxide_file_error_message(error: OxideFileError) -> String {
    match error {
        OxideFileError::DecryptionFailed => "密码错误，无法解密文件".to_string(),
        OxideFileError::ChecksumMismatch => "文件验证失败，数据可能已被篡改".to_string(),
        OxideFileError::PasswordTooShort => "密码长度至少为 6 位".to_string(),
        other => other.to_string(),
    }
}

fn native_plugin_quick_command_strategy_from_oxide(
    strategy: ImportConflictStrategy,
) -> NativePluginQuickCommandImportStrategy {
    match strategy {
        ImportConflictStrategy::Rename => NativePluginQuickCommandImportStrategy::Rename,
        ImportConflictStrategy::Skip => NativePluginQuickCommandImportStrategy::Skip,
        ImportConflictStrategy::Replace => NativePluginQuickCommandImportStrategy::Replace,
        ImportConflictStrategy::Merge => NativePluginQuickCommandImportStrategy::Merge,
    }
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

pub fn native_plugin_bool_arg(args: &Value, field: &str) -> Option<bool> {
    args.get(field).and_then(Value::as_bool)
}

pub fn native_plugin_optional_string_arg(args: &Value, field: &str) -> Option<String> {
    args.get(field).and_then(Value::as_str).map(str::to_string)
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

fn native_plugin_u8_array(values: &[Value]) -> Option<Vec<u8>> {
    values
        .iter()
        .map(|value| value.as_u64().and_then(|value| u8::try_from(value).ok()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_progress_value_clamps_percent_and_keeps_stage() {
        let value = native_plugin_sync_progress_value("Importing", "connections", 12, 10, false);

        assert_eq!(
            value.get("title").and_then(Value::as_str),
            Some("Importing")
        );
        assert_eq!(
            value.get("stage").and_then(Value::as_str),
            Some("connections")
        );
        assert_eq!(value.get("progress").and_then(Value::as_f64), Some(100.0));
        assert_eq!(value.get("done").and_then(Value::as_bool), Some(false));
    }

    #[test]
    fn sync_revision_map_groups_plugin_settings_by_plugin_id() {
        let settings = vec![
            EncryptedPluginSetting {
                storage_key: "oxide-plugin-com.example.demo-setting-mode".to_string(),
                serialized_value: "\"auto\"".to_string(),
            },
            EncryptedPluginSetting {
                storage_key: "oxide-plugin-com.example.demo-setting-enabled".to_string(),
                serialized_value: "true".to_string(),
            },
            EncryptedPluginSetting {
                storage_key: "unscoped".to_string(),
                serialized_value: "ignored".to_string(),
            },
        ];

        let revisions = native_plugin_settings_revision_map(&settings);

        assert_eq!(revisions.len(), 1);
        assert!(revisions.contains_key("com.example.demo"));
    }

    #[test]
    fn sync_import_result_omits_consumed_sidecar_payloads() {
        let envelope = ImportResultEnvelope {
            imported: 1,
            app_settings_json: Some("{}".to_string()),
            quick_commands_json: Some("[]".to_string()),
            plugin_settings: vec![EncryptedPluginSetting {
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

        assert!(value.get("appSettingsJson").is_none());
        assert!(value.get("quickCommandsJson").is_none());
        assert!(value.get("pluginSettings").is_none());
        assert_eq!(
            value.get("importedPluginSettings").and_then(Value::as_u64),
            Some(1)
        );
    }

    #[test]
    fn sync_file_data_arg_rejects_non_byte_values() {
        let bytes = native_plugin_file_data_arg(&json!({ "fileData": [0, 255] })).unwrap();
        assert_eq!(bytes, vec![0, 255]);

        let error = native_plugin_file_data_arg(&json!({ "fileData": [256] })).unwrap_err();
        assert_eq!(error, "oxide fileData contains a non-byte value");
    }
}
