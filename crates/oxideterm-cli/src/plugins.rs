// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::BTreeMap, fs};

use oxideterm_cloud_sync::plugin_settings::{
    load_plugin_settings, plugin_id_from_setting_storage_key, plugin_settings_path,
    upsert_plugin_settings,
};
use oxideterm_connections::oxide_file::EncryptedPluginSetting;
use serde::{Deserialize, Serialize};

use crate::{
    args::{
        JsonArgs, PluginSettingKeyArgs, PluginSettingSetArgs, PluginSettingUnsetArgs,
        PluginSettingsAction, PluginSettingsCommand, PluginSettingsImportArgs,
        PluginStateWriteArgs, PluginsAction, PluginsCommand, WriteArgs,
    },
    error::{CliError, CliResult},
    output::{self, OutputFormat},
    paths::default_settings_path,
    write_guard,
};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginStateFile {
    enabled: BTreeMap<String, bool>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginSettingsResponse {
    path: String,
    count: usize,
    settings: Vec<EncryptedPluginSetting>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginSettingsListItem {
    storage_key: String,
    plugin_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginSettingsListResponse {
    path: String,
    count: usize,
    settings: Vec<PluginSettingsListItem>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginChange {
    action: &'static str,
    target: String,
    before: Option<String>,
    after: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginWriteResponse {
    path: String,
    applied: bool,
    dry_run: bool,
    backup_path: Option<String>,
    backup_size_bytes: Option<u64>,
    changes: Vec<PluginChange>,
}

pub fn run(command: PluginsCommand) -> CliResult<i32> {
    match command.action {
        PluginsAction::List(args) => {
            list_plugins(args)?;
            Ok(0)
        }
        PluginsAction::Settings(command) => run_settings(command),
        PluginsAction::Enable(args) => write_plugin_state(args, true),
        PluginsAction::Disable(args) => write_plugin_state(args, false),
    }
}

fn run_settings(command: PluginSettingsCommand) -> CliResult<i32> {
    match command.action {
        PluginSettingsAction::List(args) => {
            list_settings(args)?;
            Ok(0)
        }
        PluginSettingsAction::Get(args) => {
            get_setting(args)?;
            Ok(0)
        }
        PluginSettingsAction::Set(args) => set_setting(args),
        PluginSettingsAction::Unset(args) => unset_setting(args),
        PluginSettingsAction::Export(args) => {
            export_settings(args)?;
            Ok(0)
        }
        PluginSettingsAction::Import(args) => import_settings(args),
    }
}

fn list_plugins(args: JsonArgs) -> CliResult<()> {
    let path = plugin_state_path();
    let state = read_plugin_state(args.json)?;
    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&serde_json::json!({
            "path": path.display().to_string(),
            "count": state.enabled.len(),
            "enabled": state.enabled,
        })),
        OutputFormat::Text => {
            if state.enabled.is_empty() {
                output::write_text("No headless plugin state");
            } else {
                for (plugin_id, enabled) in state.enabled {
                    output::write_text(format!("{plugin_id}\tenabled={enabled}"));
                }
            }
            Ok(())
        }
    }
}

fn list_settings(args: JsonArgs) -> CliResult<()> {
    let settings = load_settings(args.json)?;
    let path = plugin_settings_path(&default_settings_path());
    let items = settings
        .into_iter()
        .map(|setting| PluginSettingsListItem {
            plugin_id: plugin_id_from_setting_storage_key(&setting.storage_key),
            storage_key: setting.storage_key,
        })
        .collect::<Vec<_>>();
    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&PluginSettingsListResponse {
            path: path.display().to_string(),
            count: items.len(),
            settings: items,
        }),
        OutputFormat::Text => {
            if items.is_empty() {
                output::write_text("No plugin settings");
            } else {
                for item in items {
                    output::write_text(format!(
                        "{}\tplugin={}",
                        item.storage_key,
                        item.plugin_id.as_deref().unwrap_or("-")
                    ));
                }
            }
            Ok(())
        }
    }
}

fn get_setting(args: PluginSettingKeyArgs) -> CliResult<()> {
    let setting = load_settings(args.json)?
        .into_iter()
        .find(|setting| setting.storage_key == args.key)
        .ok_or_else(|| {
            CliError::new(
                "plugin_setting_not_found",
                format!("plugin setting '{}' was not found", args.key),
                args.json,
            )
        })?;
    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&setting),
        OutputFormat::Text => {
            output::write_text(format!(
                "{}\t{}",
                setting.storage_key, setting.serialized_value
            ));
            Ok(())
        }
    }
}

fn set_setting(args: PluginSettingSetArgs) -> CliResult<i32> {
    let change = PluginChange {
        action: "set",
        target: args.key.clone(),
        before: None,
        after: Some("configured".to_string()),
    };
    let incoming = vec![EncryptedPluginSetting {
        storage_key: args.key,
        serialized_value: args.value_json,
    }];
    finish_write(args.write, vec![change], move || {
        upsert_plugin_settings(&default_settings_path(), &incoming).map(|_| ())
    })
}

fn unset_setting(args: PluginSettingUnsetArgs) -> CliResult<i32> {
    let mut settings = load_settings(args.write.json)?;
    let before_len = settings.len();
    settings.retain(|setting| setting.storage_key != args.key);
    let changes = (settings.len() != before_len)
        .then(|| PluginChange {
            action: "unset",
            target: args.key,
            before: Some("configured".to_string()),
            after: None,
        })
        .into_iter()
        .collect();
    finish_write(args.write, changes, move || save_all_settings(&settings))
}

fn export_settings(args: JsonArgs) -> CliResult<()> {
    let settings = load_settings(args.json)?;
    let path = plugin_settings_path(&default_settings_path());
    let response = PluginSettingsResponse {
        path: path.display().to_string(),
        count: settings.len(),
        settings,
    };
    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&response),
        OutputFormat::Text => {
            output::write_text(serde_json::to_string_pretty(&response).map_err(|error| {
                CliError::new("serialization_failed", error.to_string(), args.json)
            })?);
            Ok(())
        }
    }
}

fn import_settings(args: PluginSettingsImportArgs) -> CliResult<i32> {
    let contents = fs::read_to_string(&args.path).map_err(|error| {
        CliError::new(
            "plugin_settings_import_failed",
            format!("failed to read plugin settings {}: {error}", args.path),
            args.write.json,
        )
    })?;
    let value = serde_json::from_str::<serde_json::Value>(&contents).map_err(|error| {
        CliError::new(
            "plugin_settings_import_failed",
            format!("failed to parse plugin settings {}: {error}", args.path),
            args.write.json,
        )
    })?;
    let settings = if let Some(settings) = value.get("settings") {
        serde_json::from_value::<Vec<EncryptedPluginSetting>>(settings.clone())
    } else {
        serde_json::from_value::<Vec<EncryptedPluginSetting>>(value)
    }
    .map_err(|error| {
        CliError::new(
            "plugin_settings_import_failed",
            format!("failed to decode plugin settings {}: {error}", args.path),
            args.write.json,
        )
    })?;
    let count = settings.len();
    finish_write(
        args.write,
        vec![PluginChange {
            action: "import",
            target: args.path,
            before: None,
            after: Some(format!("settings={count}")),
        }],
        move || upsert_plugin_settings(&default_settings_path(), &settings).map(|_| ()),
    )
}

fn write_plugin_state(args: PluginStateWriteArgs, enabled: bool) -> CliResult<i32> {
    let mut state = read_plugin_state(args.write.json)?;
    let before = state.enabled.get(&args.plugin_id).copied();
    state.enabled.insert(args.plugin_id.clone(), enabled);
    let changes = (before != Some(enabled))
        .then(|| PluginChange {
            action: if enabled { "enable" } else { "disable" },
            target: args.plugin_id,
            before: before.map(|value| value.to_string()),
            after: Some(enabled.to_string()),
        })
        .into_iter()
        .collect();
    finish_write(args.write, changes, move || save_plugin_state(&state))
}

fn finish_write(
    write: WriteArgs,
    changes: Vec<PluginChange>,
    apply: impl FnOnce() -> Result<(), String>,
) -> CliResult<i32> {
    let mut guard = write_guard::prepare_write(&write, !changes.is_empty())?;
    if !write.dry_run && !changes.is_empty() {
        apply().map_err(|error| CliError::new("plugin_write_failed", error, write.json))?;
        write_guard::mark_applied(&mut guard);
    }
    let response = PluginWriteResponse {
        path: default_settings_path().display().to_string(),
        applied: guard.applied,
        dry_run: guard.dry_run,
        backup_path: guard.backup_path,
        backup_size_bytes: guard.backup_size_bytes,
        changes,
    };
    let ok = response.applied || response.dry_run || response.changes.is_empty();
    match output::format_from_flag(write.json) {
        OutputFormat::Json => output::write_json_with_ok(&response, ok),
        OutputFormat::Text => {
            output::write_text(format_write_text(&response));
            Ok(())
        }
    }?;
    Ok(if ok { 0 } else { 1 })
}

fn load_settings(json: bool) -> CliResult<Vec<EncryptedPluginSetting>> {
    load_plugin_settings(&default_settings_path())
        .map_err(|error| CliError::new("plugin_settings_read_failed", error, json))
}

fn save_all_settings(settings: &[EncryptedPluginSetting]) -> Result<(), String> {
    let path = plugin_settings_path(&default_settings_path());
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(
        path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "version": 1,
            "settings": settings,
        }))
        .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}

fn read_plugin_state(json: bool) -> CliResult<PluginStateFile> {
    let path = plugin_state_path();
    if !path.exists() {
        return Ok(PluginStateFile::default());
    }
    let contents = fs::read_to_string(&path).map_err(|error| {
        CliError::new(
            "plugin_state_read_failed",
            format!("failed to read plugin state {}: {error}", path.display()),
            json,
        )
    })?;
    serde_json::from_str(&contents).map_err(|error| {
        CliError::new(
            "plugin_state_read_failed",
            format!("failed to parse plugin state {}: {error}", path.display()),
            json,
        )
    })
}

fn save_plugin_state(state: &PluginStateFile) -> Result<(), String> {
    let path = plugin_state_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(
        path,
        serde_json::to_vec_pretty(state).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}

fn plugin_state_path() -> std::path::PathBuf {
    default_settings_path()
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("plugins-state.json")
}

fn format_write_text(response: &PluginWriteResponse) -> String {
    let mut lines = vec![format!(
        "applied: {} dryRun={} changes={} backup={}",
        response.applied,
        response.dry_run,
        response.changes.len(),
        response.backup_path.as_deref().unwrap_or("-")
    )];
    for change in &response.changes {
        lines.push(format!(
            "{}\t{}\t{}\t=>\t{}",
            change.action,
            change.target,
            change.before.as_deref().unwrap_or("-"),
            change.after.as_deref().unwrap_or("-")
        ));
    }
    lines.join("\n")
}
