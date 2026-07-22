// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Manifest, runtime metadata, and contribution validation rules.

use super::*;
use std::collections::HashSet;

const NATIVE_PLUGIN_MAX_HOST_MONITORS: usize = 16;
const NATIVE_PLUGIN_MAX_HOST_MONITOR_COMMAND_BYTES: usize = 16 * 1024;
const NATIVE_PLUGIN_MIN_HOST_MONITOR_OUTPUT_BYTES: usize = 1024;
const NATIVE_PLUGIN_MAX_HOST_MONITOR_OUTPUT_BYTES: usize = 1024 * 1024;
const NATIVE_PLUGIN_MAX_HOST_MONITOR_TIMEOUT_SECONDS: u64 = 30;
const NATIVE_PLUGIN_MAX_HOST_MONITOR_ROWS: usize = 2_000;
const NATIVE_PLUGIN_MAX_HOST_MONITOR_COLUMNS: usize = 64;
const NATIVE_PLUGIN_MAX_ACTIVITY_BAR_ITEMS: usize = 16;
const NATIVE_PLUGIN_DECLARATIVE_UI_COMPONENT_VERSION: u8 = 1;
const NATIVE_PLUGIN_MAX_DECLARATIVE_UI_DEPTH: usize = 8;
const NATIVE_PLUGIN_MAX_DECLARATIVE_UI_CONTROLS: usize = 256;
const NATIVE_PLUGIN_MAX_DECLARATIVE_UI_CHILDREN: usize = 64;
const NATIVE_PLUGIN_MAX_DECLARATIVE_UI_OPTIONS: usize = 128;
const NATIVE_PLUGIN_MAX_DECLARATIVE_UI_ROWS: usize = 2_000;
const NATIVE_PLUGIN_MAX_DECLARATIVE_UI_COLUMNS: usize = 64;
const NATIVE_PLUGIN_MAX_DECLARATIVE_UI_BYTES: usize = 2 * 1024 * 1024;

pub(crate) fn validate_native_plugin_manifest(
    manifest: &NativePluginManifest,
) -> Result<(), String> {
    validate_native_plugin_id(&manifest.id)?;
    validate_manifest_text_field("name", &manifest.name)?;
    validate_manifest_text_field("version", &manifest.version)?;
    if let Some(main) = &manifest.main {
        validate_plugin_relative_path(main)?;
    }
    if let Some(runtime) = &manifest.runtime {
        validate_plugin_relative_path(&runtime.entry)?;
    }
    if let Some(assets) = &manifest.assets {
        validate_plugin_relative_path(assets)?;
    }
    if let Some(styles) = &manifest.styles {
        for style_path in styles {
            validate_plugin_relative_path(style_path)?;
        }
    }
    if let Some(locales) = &manifest.locales {
        validate_plugin_relative_path(locales)?;
    }
    // Permission declarations cover only sensitive data and side effects; safe
    // redacted host projections remain available without declarations.
    normalize_native_plugin_capabilities(&manifest.permissions.capabilities)?;
    if let Some(contributes) = &manifest.contributes {
        validate_native_plugin_contributions(contributes)?;
    }
    Ok(())
}

pub(crate) fn validate_native_plugin_contributions(
    contributes: &NativePluginContributes,
) -> Result<(), String> {
    if let Some(tabs) = &contributes.tabs {
        for tab in tabs {
            validate_manifest_text_field("contributes.tabs.id", &tab.id)?;
            validate_manifest_text_field("contributes.tabs.title", &tab.title)?;
            validate_manifest_text_field("contributes.tabs.icon", &tab.icon)?;
        }
    }
    if let Some(sidebar_panels) = &contributes.sidebar_panels {
        for panel in sidebar_panels {
            validate_manifest_text_field("contributes.sidebarPanels.id", &panel.id)?;
            validate_manifest_text_field("contributes.sidebarPanels.title", &panel.title)?;
            validate_manifest_text_field("contributes.sidebarPanels.icon", &panel.icon)?;
            validate_one_of(
                "contributes.sidebarPanels.position",
                &panel.position,
                &["top", "bottom"],
            )?;
        }
    }
    if let Some(activity_bar_items) = &contributes.activity_bar_items {
        if activity_bar_items.len() > NATIVE_PLUGIN_MAX_ACTIVITY_BAR_ITEMS {
            return Err(format!(
                "Plugin contributes at most {NATIVE_PLUGIN_MAX_ACTIVITY_BAR_ITEMS} activity-bar items"
            ));
        }
        let mut item_ids = HashSet::new();
        for item in activity_bar_items {
            validate_manifest_text_field("contributes.activityBarItems.id", &item.id)?;
            validate_manifest_text_field("contributes.activityBarItems.title", &item.title)?;
            validate_manifest_text_field("contributes.activityBarItems.icon", &item.icon)?;
            validate_manifest_text_field("contributes.activityBarItems.command", &item.command)?;
            validate_one_of(
                "contributes.activityBarItems.position",
                &item.position,
                &["top", "bottom"],
            )?;
            if !item_ids.insert(item.id.as_str()) {
                return Err(format!(
                    "Duplicate contributes.activityBarItems id \"{}\"",
                    item.id
                ));
            }
        }
    }
    if let Some(settings) = &contributes.settings {
        for setting in settings {
            validate_manifest_text_field("contributes.settings.id", &setting.id)?;
            validate_manifest_text_field("contributes.settings.title", &setting.title)?;
            validate_one_of(
                "contributes.settings.type",
                &setting.setting_type,
                &["string", "number", "boolean", "select"],
            )?;
            if setting.setting_type == "select" {
                let options = setting.options.as_ref().ok_or_else(|| {
                    "Select plugin settings require contributes.settings.options".to_string()
                })?;
                for option in options {
                    validate_manifest_text_field(
                        "contributes.settings.options.label",
                        &option.label,
                    )?;
                    if !(option.value.is_string() || option.value.is_number()) {
                        return Err(
                            "Select plugin setting option values must be strings or numbers"
                                .to_string(),
                        );
                    }
                }
            }
            validate_plugin_setting_value(setting, &setting.default)?;
        }
    }
    if let Some(hooks) = &contributes.terminal_hooks
        && let Some(shortcuts) = &hooks.shortcuts
    {
        for shortcut in shortcuts {
            validate_manifest_text_field("contributes.terminalHooks.shortcuts.key", &shortcut.key)?;
            validate_manifest_text_field(
                "contributes.terminalHooks.shortcuts.command",
                &shortcut.command,
            )?;
        }
    }
    if let Some(transports) = &contributes.terminal_transports {
        for transport in transports {
            validate_one_of("contributes.terminalTransports", transport, &["telnet"])?;
        }
    }
    if let Some(connection_hooks) = &contributes.connection_hooks {
        for hook in connection_hooks {
            validate_one_of(
                "contributes.connectionHooks",
                hook,
                &["onConnect", "onDisconnect", "onReconnect", "onLinkDown"],
            )?;
        }
    }
    if let Some(ai_tools) = &contributes.ai_tools {
        for tool in ai_tools {
            validate_manifest_text_field("contributes.aiTools.name", &tool.name)?;
            validate_manifest_text_field("contributes.aiTools.description", &tool.description)?;
            if let Some(capabilities) = &tool.capabilities {
                for capability in capabilities {
                    validate_one_of(
                        "contributes.aiTools.capabilities",
                        capability,
                        &[
                            "command.run",
                            "terminal.send",
                            "terminal.observe",
                            "terminal.wait",
                            "filesystem.read",
                            "filesystem.write",
                            "filesystem.search",
                            "navigation.open",
                            "state.list",
                            "network.forward",
                            "settings.read",
                            "settings.write",
                            "plugin.invoke",
                            "mcp.invoke",
                        ],
                    )?;
                }
            }
            if let Some(risk) = &tool.risk {
                validate_one_of(
                    "contributes.aiTools.risk",
                    risk,
                    &[
                        "read",
                        "write-file",
                        "execute-command",
                        "interactive-input",
                        "destructive",
                        "network-expose",
                        "settings-change",
                        "credential-sensitive",
                    ],
                )?;
            }
            if let Some(target_kinds) = &tool.target_kinds {
                for target_kind in target_kinds {
                    validate_one_of(
                        "contributes.aiTools.targetKinds",
                        target_kind,
                        &[
                            "local-shell",
                            "ssh-node",
                            "terminal-session",
                            "sftp-session",
                            "ide-workspace",
                            "app-tab",
                            "mcp-server",
                            "rag-index",
                        ],
                    )?;
                }
            }
        }
    }
    if let Some(api_commands) = &contributes.api_commands {
        for command in api_commands {
            validate_manifest_text_field("contributes.apiCommands", command)?;
        }
    }
    if let Some(host_monitors) = &contributes.host_monitors {
        validate_native_plugin_host_monitors(host_monitors)?;
    }
    Ok(())
}

fn validate_native_plugin_host_monitors(
    host_monitors: &[NativePluginHostMonitorDef],
) -> Result<(), String> {
    if host_monitors.len() > NATIVE_PLUGIN_MAX_HOST_MONITORS {
        return Err(format!(
            "Plugin contributes at most {NATIVE_PLUGIN_MAX_HOST_MONITORS} Host Tools monitors"
        ));
    }
    let mut monitor_ids = HashSet::new();
    for monitor in host_monitors {
        validate_manifest_text_field("contributes.hostMonitors.id", &monitor.id)?;
        validate_manifest_text_field("contributes.hostMonitors.title", &monitor.title)?;
        if !monitor_ids.insert(monitor.id.as_str()) {
            return Err(format!(
                "Duplicate contributes.hostMonitors id \"{}\"",
                monitor.id
            ));
        }
        if monitor.commands.is_empty() {
            return Err(format!(
                "Host Tools monitor \"{}\" requires at least one platform command",
                monitor.id
            ));
        }
        for (platform, command) in &monitor.commands {
            validate_one_of(
                "contributes.hostMonitors.commands",
                platform,
                &["linux", "macos", "bsd", "windows", "default"],
            )?;
            if command.trim().is_empty() {
                return Err(format!(
                    "Host Tools monitor \"{}\" command for \"{platform}\" cannot be empty",
                    monitor.id
                ));
            }
            if command.len() > NATIVE_PLUGIN_MAX_HOST_MONITOR_COMMAND_BYTES {
                return Err(format!(
                    "Host Tools monitor \"{}\" command exceeds {NATIVE_PLUGIN_MAX_HOST_MONITOR_COMMAND_BYTES} bytes",
                    monitor.id
                ));
            }
            if command.contains('\0') {
                return Err(format!(
                    "Host Tools monitor \"{}\" command contains an invalid null byte",
                    monitor.id
                ));
            }
        }
        if !(1..=NATIVE_PLUGIN_MAX_HOST_MONITOR_TIMEOUT_SECONDS).contains(&monitor.timeout_seconds)
        {
            return Err(format!(
                "Host Tools monitor \"{}\" timeoutSeconds must be between 1 and {NATIVE_PLUGIN_MAX_HOST_MONITOR_TIMEOUT_SECONDS}",
                monitor.id
            ));
        }
        if !(NATIVE_PLUGIN_MIN_HOST_MONITOR_OUTPUT_BYTES
            ..=NATIVE_PLUGIN_MAX_HOST_MONITOR_OUTPUT_BYTES)
            .contains(&monitor.max_output_bytes)
        {
            return Err(format!(
                "Host Tools monitor \"{}\" maxOutputBytes must be between {NATIVE_PLUGIN_MIN_HOST_MONITOR_OUTPUT_BYTES} and {NATIVE_PLUGIN_MAX_HOST_MONITOR_OUTPUT_BYTES}",
                monitor.id
            ));
        }
        if !(1..=NATIVE_PLUGIN_MAX_HOST_MONITOR_ROWS).contains(&monitor.output.max_rows) {
            return Err(format!(
                "Host Tools monitor \"{}\" output.maxRows must be between 1 and {NATIVE_PLUGIN_MAX_HOST_MONITOR_ROWS}",
                monitor.id
            ));
        }
        validate_native_plugin_host_monitor_columns(monitor)?;
    }
    Ok(())
}

fn validate_native_plugin_host_monitor_columns(
    monitor: &NativePluginHostMonitorDef,
) -> Result<(), String> {
    let columns = &monitor.output.columns;
    if monitor.output.format != NativePluginHostMonitorOutputFormat::Tsv {
        if !columns.is_empty() {
            return Err(format!(
                "Host Tools monitor \"{}\" output.columns is only valid for tsv output",
                monitor.id
            ));
        }
        return Ok(());
    }
    if columns.is_empty() || columns.len() > NATIVE_PLUGIN_MAX_HOST_MONITOR_COLUMNS {
        return Err(format!(
            "Host Tools monitor \"{}\" tsv output requires 1 to {NATIVE_PLUGIN_MAX_HOST_MONITOR_COLUMNS} columns",
            monitor.id
        ));
    }
    let mut unique_columns = HashSet::new();
    for column in columns {
        validate_manifest_text_field("contributes.hostMonitors.output.columns", column)?;
        if !unique_columns.insert(column.as_str()) {
            return Err(format!(
                "Host Tools monitor \"{}\" contains duplicate output column \"{column}\"",
                monitor.id
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_runtime_entry_exists(
    plugin_dir: &Path,
    runtime_plan: &NativePluginRuntimePlan,
) -> Result<(), String> {
    let entry = match runtime_plan {
        NativePluginRuntimePlan::Wasm { entry } | NativePluginRuntimePlan::Process { entry } => {
            entry
        }
        NativePluginRuntimePlan::ManifestOnly
        | NativePluginRuntimePlan::UnsupportedLegacyJs { .. } => return Ok(()),
    };
    let entry_path = plugin_dir.join(entry);
    if !entry_path.is_file() {
        return Err(format!(
            "Native plugin runtime entry \"{entry}\" does not exist"
        ));
    }
    Ok(())
}

pub(crate) fn quarantine_corrupt_native_plugin_config(config_path: &Path) {
    let Some(file_name) = config_path.file_name().and_then(|name| name.to_str()) else {
        return;
    };
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let backup_name = format!("{file_name}.{PLUGIN_CONFIG_CORRUPT_MARKER}-{timestamp_ms}");
    let backup_path = config_path.with_file_name(backup_name);
    // Bad plugin config should not keep breaking startup. Preserve the raw file
    // next to the original path for manual inspection, then let discovery save
    // a fresh schema-valid config.
    let _ = fs::rename(config_path, backup_path);
}

#[allow(dead_code)]

pub(crate) fn validate_one_of(field: &str, value: &str, allowed: &[&str]) -> Result<(), String> {
    if allowed.contains(&value) {
        return Ok(());
    }
    Err(format!(
        "Plugin manifest field \"{field}\" has unsupported value \"{value}\""
    ))
}

pub(crate) fn validate_plugin_setting_value(
    setting: &NativePluginSettingDef,
    value: &Value,
) -> Result<(), String> {
    match setting.setting_type.as_str() {
        "string" => {
            if value.is_string() {
                Ok(())
            } else {
                Err(format!(
                    "Plugin setting \"{}\" requires a string",
                    setting.id
                ))
            }
        }
        "number" => {
            if value.is_number() {
                Ok(())
            } else {
                Err(format!(
                    "Plugin setting \"{}\" requires a number",
                    setting.id
                ))
            }
        }
        "boolean" => {
            if value.is_boolean() {
                Ok(())
            } else {
                Err(format!(
                    "Plugin setting \"{}\" requires a boolean",
                    setting.id
                ))
            }
        }
        "select" => {
            let allowed = setting
                .options
                .as_ref()
                .is_some_and(|options| options.iter().any(|option| option.value == *value));
            if allowed {
                Ok(())
            } else {
                Err(format!(
                    "Plugin setting \"{}\" requires one of its declared select options",
                    setting.id
                ))
            }
        }
        _ => Err(format!(
            "Plugin setting \"{}\" has unsupported type \"{}\"",
            setting.id, setting.setting_type
        )),
    }
}

pub(crate) fn validate_plugin_storage_key(key: &str) -> Result<(), String> {
    if key.trim().is_empty() {
        return Err("Plugin storage key cannot be empty".to_string());
    }
    if key.len() > PLUGIN_STORAGE_MAX_KEY_BYTES {
        return Err(format!(
            "Plugin storage key exceeds {} bytes",
            PLUGIN_STORAGE_MAX_KEY_BYTES
        ));
    }
    if key.bytes().any(|byte| byte < 0x20) {
        return Err("Plugin storage key contains invalid characters".to_string());
    }
    Ok(())
}

pub(crate) fn validate_plugin_storage_size(values: &HashMap<String, Value>) -> Result<(), String> {
    let encoded = serde_json::to_vec(values).map_err(|error| error.to_string())?;
    if encoded.len() > PLUGIN_STORAGE_MAX_PLUGIN_BYTES {
        return Err(format!(
            "Plugin storage exceeds {} bytes",
            PLUGIN_STORAGE_MAX_PLUGIN_BYTES
        ));
    }
    Ok(())
}

pub(crate) fn validate_manifest_text_field(field: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("Plugin manifest field \"{field}\" cannot be empty"));
    }
    Ok(())
}

#[allow(dead_code)]
pub(crate) fn runtime_metadata_string(metadata: &Value, key: &str) -> Option<String> {
    metadata
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

pub(crate) fn runtime_metadata_node_filter(metadata: &Value) -> Option<Value> {
    runtime_metadata_string(metadata, "nodeId")
        .map(|node_id| serde_json::json!({ "nodeId": node_id }))
}

pub(crate) fn manifest_declared_tab<'a>(
    manifest: &'a NativePluginManifest,
    tab_id: &str,
) -> Option<&'a NativePluginTabDef> {
    manifest
        .contributes
        .as_ref()
        .and_then(|contributes| contributes.tabs.as_ref())
        .and_then(|tabs| tabs.iter().find(|tab| tab.id == tab_id))
}

pub(crate) fn manifest_declared_sidebar_panel<'a>(
    manifest: &'a NativePluginManifest,
    panel_id: &str,
) -> Option<&'a NativePluginSidebarDef> {
    manifest
        .contributes
        .as_ref()
        .and_then(|contributes| contributes.sidebar_panels.as_ref())
        .and_then(|panels| panels.iter().find(|panel| panel.id == panel_id))
}

pub(crate) fn manifest_declared_activity_bar_item<'a>(
    manifest: &'a NativePluginManifest,
    item_id: &str,
) -> Option<&'a NativePluginActivityBarItemDef> {
    manifest
        .contributes
        .as_ref()
        .and_then(|contributes| contributes.activity_bar_items.as_ref())
        .and_then(|items| items.iter().find(|item| item.id == item_id))
}

pub(crate) fn runtime_declarative_ui_schema(
    metadata: &Value,
) -> Result<NativePluginDeclarativeUiSchema, String> {
    let schema = metadata.get("schema").unwrap_or(metadata);
    serde_json::from_value(schema.clone())
        .map_err(|error| format!("Runtime declarative UI schema is invalid: {error}"))
}

pub(crate) fn validate_native_plugin_declarative_ui_schema(
    schema: &NativePluginDeclarativeUiSchema,
) -> Result<(), String> {
    let encoded_size = serde_json::to_vec(schema)
        .map_err(|error| format!("Runtime declarative UI schema cannot be encoded: {error}"))?
        .len();
    if encoded_size > NATIVE_PLUGIN_MAX_DECLARATIVE_UI_BYTES {
        return Err(format!(
            "Runtime declarative UI schema exceeds {NATIVE_PLUGIN_MAX_DECLARATIVE_UI_BYTES} bytes"
        ));
    }
    if schema.component_version != NATIVE_PLUGIN_DECLARATIVE_UI_COMPONENT_VERSION {
        return Err(format!(
            "Runtime declarative UI componentVersion must be {NATIVE_PLUGIN_DECLARATIVE_UI_COMPONENT_VERSION}"
        ));
    }
    validate_one_of(
        "runtime.declarativeUi.kind",
        &schema.kind,
        &[NATIVE_PLUGIN_DECLARATIVE_UI_FORM_KIND],
    )?;
    if schema.sections.is_empty() && schema.controls.is_empty() {
        return Err("Runtime declarative UI schema requires sections or controls".to_string());
    }
    let mut section_ids = HashSet::new();
    let mut control_count = 0;
    for section in &schema.sections {
        validate_manifest_text_field("runtime.declarativeUi.sections.id", &section.id)?;
        if section.id == "root" {
            return Err(
                "Runtime declarative UI section id \"root\" is reserved by the host".to_string(),
            );
        }
        if !section_ids.insert(section.id.as_str()) {
            return Err(format!(
                "Duplicate runtime declarative UI section id \"{}\"",
                section.id
            ));
        }
        let mut control_ids = HashSet::new();
        validate_native_plugin_declarative_controls(
            &section.controls,
            1,
            &mut control_count,
            &mut control_ids,
        )?;
    }
    let mut root_control_ids = HashSet::new();
    validate_native_plugin_declarative_controls(
        &schema.controls,
        1,
        &mut control_count,
        &mut root_control_ids,
    )
}

pub(crate) fn validate_native_plugin_declarative_controls(
    controls: &[NativePluginDeclarativeUiControl],
    depth: usize,
    control_count: &mut usize,
    control_ids: &mut HashSet<String>,
) -> Result<(), String> {
    if depth > NATIVE_PLUGIN_MAX_DECLARATIVE_UI_DEPTH {
        return Err(format!(
            "Runtime declarative UI supports at most {NATIVE_PLUGIN_MAX_DECLARATIVE_UI_DEPTH} nested component levels"
        ));
    }
    if controls.len() > NATIVE_PLUGIN_MAX_DECLARATIVE_UI_CHILDREN {
        return Err(format!(
            "Runtime declarative UI containers support at most {NATIVE_PLUGIN_MAX_DECLARATIVE_UI_CHILDREN} direct children"
        ));
    }
    for control in controls {
        *control_count += 1;
        if *control_count > NATIVE_PLUGIN_MAX_DECLARATIVE_UI_CONTROLS {
            return Err(format!(
                "Runtime declarative UI supports at most {NATIVE_PLUGIN_MAX_DECLARATIVE_UI_CONTROLS} controls"
            ));
        }
        validate_one_of(
            "runtime.declarativeUi.controls.kind",
            &control.kind,
            NATIVE_PLUGIN_DECLARATIVE_UI_CONTROL_KINDS,
        )?;
        if native_plugin_declarative_control_requires_id(&control.kind)
            && control.id.as_deref().is_none_or(str::is_empty)
        {
            return Err(format!(
                "Runtime declarative UI control kind \"{}\" requires id",
                control.kind
            ));
        }
        if let Some(control_id) = control.id.as_ref()
            && !control_ids.insert(control_id.clone())
        {
            return Err(format!(
                "Duplicate runtime declarative UI control id \"{control_id}\""
            ));
        }
        if let Some(options) = &control.options {
            if options.len() > NATIVE_PLUGIN_MAX_DECLARATIVE_UI_OPTIONS {
                return Err(format!(
                    "Runtime declarative UI controls support at most {NATIVE_PLUGIN_MAX_DECLARATIVE_UI_OPTIONS} options"
                ));
            }
            for option in options {
                validate_manifest_text_field(
                    "runtime.declarativeUi.controls.options.label",
                    &option.label,
                )?;
            }
        }
        if control
            .rows
            .as_ref()
            .is_some_and(|rows| rows.len() > NATIVE_PLUGIN_MAX_DECLARATIVE_UI_ROWS)
        {
            return Err(format!(
                "Runtime declarative UI controls support at most {NATIVE_PLUGIN_MAX_DECLARATIVE_UI_ROWS} rows"
            ));
        }
        let column_count = control
            .column_defs
            .as_ref()
            .map(Vec::len)
            .or_else(|| control.columns.as_ref().map(Vec::len))
            .unwrap_or(0);
        if column_count > NATIVE_PLUGIN_MAX_DECLARATIVE_UI_COLUMNS {
            return Err(format!(
                "Runtime declarative UI tables support at most {NATIVE_PLUGIN_MAX_DECLARATIVE_UI_COLUMNS} columns"
            ));
        }
        if let Some(columns) = &control.column_defs {
            for column in columns {
                validate_manifest_text_field(
                    "runtime.declarativeUi.controls.columnDefs.key",
                    &column.key,
                )?;
                validate_manifest_text_field(
                    "runtime.declarativeUi.controls.columnDefs.label",
                    &column.label,
                )?;
                if let Some(style) = column.style.as_deref() {
                    validate_one_of(
                        "runtime.declarativeUi.controls.columnDefs.style",
                        style,
                        &["primary", "meta", "mono"],
                    )?;
                }
            }
        }
        validate_native_plugin_declarative_control_options(control)?;
        if !control.children.is_empty() {
            validate_native_plugin_declarative_controls(
                &control.children,
                depth.saturating_add(1),
                control_count,
                control_ids,
            )?;
        }
    }
    Ok(())
}

fn validate_native_plugin_declarative_control_options(
    control: &NativePluginDeclarativeUiControl,
) -> Result<(), String> {
    if let (Some(min), Some(max)) = (control.min, control.max)
        && max < min
    {
        return Err(
            "Runtime declarative UI control max must be greater than or equal to min".to_string(),
        );
    }
    if control.step.is_some_and(|step| step <= 0.0) {
        return Err("Runtime declarative UI control step must be greater than zero".to_string());
    }
    if control.kind == "password" && control.value.as_ref().is_some_and(|value| !value.is_null()) {
        return Err(
            "Runtime declarative UI password controls cannot declare an initial value".to_string(),
        );
    }
    if let Some(variant) = control.variant.as_deref() {
        let allowed = match control.kind.as_str() {
            "button" | "iconButton" | "icon-button" => &[
                "default",
                "secondary",
                "outline",
                "ghost",
                "destructive",
                "link",
            ][..],
            "card" => &["panel", "inset", "inspector"][..],
            _ => &["default"][..],
        };
        validate_one_of("runtime.declarativeUi.controls.variant", variant, allowed)?;
    }
    if let Some(tone) = control.tone.as_deref() {
        validate_one_of(
            "runtime.declarativeUi.controls.tone",
            tone,
            &["neutral", "accent", "success", "warning", "error", "info"],
        )?;
    }
    if let Some(size) = control.size.as_deref() {
        validate_one_of(
            "runtime.declarativeUi.controls.size",
            size,
            &["small", "default", "large", "icon"],
        )?;
    }
    if let Some(gap) = control.gap.as_deref() {
        validate_one_of(
            "runtime.declarativeUi.controls.gap",
            gap,
            &["none", "compact", "normal", "spacious"],
        )?;
    }
    if matches!(control.kind.as_str(), "iconButton" | "icon-button")
        && control.icon.as_deref().is_none_or(str::is_empty)
    {
        return Err("Runtime declarative UI iconButton requires icon".to_string());
    }
    if matches!(control.kind.as_str(), "iconButton" | "icon-button")
        && control.label.as_deref().is_none_or(str::is_empty)
    {
        return Err("Runtime declarative UI iconButton requires label for its tooltip".to_string());
    }
    if matches!(
        control.kind.as_str(),
        "select" | "radioGroup" | "radio-group" | "segmentedControl" | "segmented-control"
    ) && control.options.as_ref().is_none_or(Vec::is_empty)
    {
        return Err(format!(
            "Runtime declarative UI control kind \"{}\" requires options",
            control.kind
        ));
    }
    if !control.children.is_empty()
        && !matches!(control.kind.as_str(), "stack" | "row" | "card" | "toolbar")
    {
        return Err(format!(
            "Runtime declarative UI control kind \"{}\" cannot contain children",
            control.kind
        ));
    }
    Ok(())
}

pub fn native_plugin_declarative_control_is_actionable(
    control: &NativePluginDeclarativeUiControl,
) -> bool {
    matches!(
        control.kind.as_str(),
        "button" | "iconButton" | "icon-button"
    ) && !control.disabled
        && !control.loading
        && control.id.is_some()
}

pub(crate) fn native_plugin_declarative_control_requires_id(kind: &str) -> bool {
    matches!(
        kind,
        "text"
            | "password"
            | "number"
            | "checkbox"
            | "select"
            | "radioGroup"
            | "radio-group"
            | "slider"
            | "segmentedControl"
            | "segmented-control"
            | "button"
            | "iconButton"
            | "icon-button"
    )
}

pub(crate) fn native_plugin_sidebar_position_sort_key(position: &str) -> u8 {
    match position {
        "top" => 0,
        "bottom" => 1,
        _ => 2,
    }
}

#[allow(dead_code)]
pub(crate) fn runtime_context_menu_items(
    metadata: &Value,
) -> Result<Vec<NativePluginRuntimeContextMenuItem>, String> {
    let items = metadata
        .get("items")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "Runtime context menu registration requires metadata.items".to_string())?;
    let mut parsed = Vec::with_capacity(items.len());
    for item in items {
        let label = item
            .get("label")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "Runtime context menu item requires label".to_string())?
            .to_string();
        validate_manifest_text_field("runtime.contextMenu.items.label", &label)?;
        parsed.push(NativePluginRuntimeContextMenuItem {
            label,
            icon: runtime_metadata_string(item, "icon"),
            // Tauri allowed a render-time `when()` predicate. Native cannot run
            // arbitrary plugin code while painting a menu, so runtime plugins
            // must send the current enabled state as data.
            enabled: item
                .get("enabled")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true),
        });
    }
    Ok(parsed)
}

pub(crate) fn runtime_subscription_event(
    metadata: &Value,
    subscriber_plugin_id: &str,
) -> Result<String, String> {
    if runtime_metadata_string(metadata, "namespace").as_deref() == Some("events")
        && runtime_metadata_string(metadata, "method").as_deref() == Some("on")
    {
        let event_name = runtime_metadata_string(metadata, "name")
            .or_else(|| runtime_metadata_string(metadata, "event"))
            .ok_or_else(|| "Runtime events.on subscription requires metadata.name".to_string())?;
        let owner_plugin_id = runtime_metadata_string(metadata, "pluginId")
            .or_else(|| runtime_metadata_string(metadata, "ownerPluginId"))
            .unwrap_or_else(|| subscriber_plugin_id.to_string());
        return native_plugin_custom_event_key(&owner_plugin_id, &event_name);
    }

    let event = runtime_metadata_string(metadata, "event")
        .or_else(|| runtime_subscription_event_from_method(metadata))
        .ok_or_else(|| {
            "Runtime event subscription requires metadata.event or metadata.namespace/method"
                .to_string()
        })?;
    if event.starts_with("plugin.") {
        validate_plugin_event_key(&event)?;
        return Ok(event);
    }
    validate_one_of(
        "runtime.eventSubscription.event",
        &event,
        NATIVE_PLUGIN_PHASE4_SUBSCRIPTION_EVENTS,
    )?;
    Ok(event)
}

/// Resolves runtime subscription metadata to the stable event name used by the host.
pub fn native_plugin_runtime_subscription_event(
    metadata: &Value,
    subscriber_plugin_id: &str,
) -> Result<String, String> {
    runtime_subscription_event(metadata, subscriber_plugin_id)
}

pub(crate) fn runtime_subscription_event_from_method(metadata: &Value) -> Option<String> {
    let namespace = runtime_metadata_string(metadata, "namespace")?;
    let method = runtime_metadata_string(metadata, "method")?;
    // Native replaces JS callback registration methods with stable event names
    // that a process/WASM runtime can receive through PluginEvent frames.
    match (namespace.as_str(), method.as_str()) {
        ("app", "onThemeChange") => Some(NATIVE_PLUGIN_APP_THEME_CHANGED_EVENT.to_string()),
        ("app", "onSettingsChange") => Some(NATIVE_PLUGIN_APP_SETTINGS_CHANGED_EVENT.to_string()),
        ("i18n", "onLanguageChange") => Some(NATIVE_PLUGIN_I18N_LANGUAGE_CHANGED_EVENT.to_string()),
        ("settings", "onChange") => Some(NATIVE_PLUGIN_SETTING_CHANGED_EVENT.to_string()),
        ("ui", "onLayoutChange") => Some(NATIVE_PLUGIN_UI_LAYOUT_CHANGED_EVENT.to_string()),
        ("sessions", "onTreeChange") => Some(NATIVE_PLUGIN_SESSION_TREE_CHANGED_EVENT.to_string()),
        ("sessions", "onNodeStateChange") => {
            Some(NATIVE_PLUGIN_SESSION_NODE_STATE_CHANGED_EVENT.to_string())
        }
        ("eventLog", "onEntry") => Some(NATIVE_PLUGIN_EVENT_LOG_ENTRY_EVENT.to_string()),
        ("forward", "onSavedForwardsChange") => {
            Some(NATIVE_PLUGIN_FORWARD_SAVED_FORWARDS_CHANGED_EVENT.to_string())
        }
        ("transfers", "onProgress") => Some(NATIVE_PLUGIN_TRANSFER_PROGRESS_EVENT.to_string()),
        ("transfers", "onComplete") => Some(NATIVE_PLUGIN_TRANSFER_COMPLETE_EVENT.to_string()),
        ("transfers", "onError") => Some(NATIVE_PLUGIN_TRANSFER_ERROR_EVENT.to_string()),
        ("profiler", "onMetrics") => Some(NATIVE_PLUGIN_PROFILER_METRICS_EVENT.to_string()),
        ("ide", "onFileOpen") => Some(NATIVE_PLUGIN_IDE_FILE_OPEN_EVENT.to_string()),
        ("ide", "onFileClose") => Some(NATIVE_PLUGIN_IDE_FILE_CLOSE_EVENT.to_string()),
        ("ide", "onActiveFileChange") => {
            Some(NATIVE_PLUGIN_IDE_ACTIVE_FILE_CHANGED_EVENT.to_string())
        }
        ("ai", "onMessage") => Some(NATIVE_PLUGIN_AI_MESSAGE_EVENT.to_string()),
        ("events", "onConnect") => Some(NATIVE_PLUGIN_LIFECYCLE_CONNECT_EVENT.to_string()),
        ("events", "onDisconnect") => Some(NATIVE_PLUGIN_LIFECYCLE_DISCONNECT_EVENT.to_string()),
        ("events", "onLinkDown") => Some(NATIVE_PLUGIN_LIFECYCLE_LINK_DOWN_EVENT.to_string()),
        ("events", "onReconnect") => Some(NATIVE_PLUGIN_LIFECYCLE_RECONNECT_EVENT.to_string()),
        _ => None,
    }
}

pub fn native_plugin_custom_event_key(
    owner_plugin_id: &str,
    event_name: &str,
) -> Result<String, String> {
    // Event-key validation belongs to the plugin host API contract crate; the
    // app registry keeps this wrapper so existing call sites stay localized.
    native_plugin_custom_event_key_checked(owner_plugin_id, event_name)
}

pub fn native_plugin_custom_event_key_checked(
    owner_plugin_id: &str,
    event_name: &str,
) -> Result<String, String> {
    validate_native_plugin_id(owner_plugin_id)?;
    validate_plugin_event_name(event_name)?;
    Ok(format!("plugin.{owner_plugin_id}:{event_name}"))
}

pub(crate) fn validate_plugin_event_name(event_name: &str) -> Result<(), String> {
    if event_name.trim().is_empty() {
        return Err("Plugin event name cannot be empty".to_string());
    }
    if event_name.len() > 128 {
        return Err("Plugin event name is too long".to_string());
    }
    if event_name.contains("..") || event_name.contains('/') || event_name.contains('\\') {
        return Err("Plugin event name cannot contain path separators or traversal".to_string());
    }
    if event_name
        .bytes()
        .any(|byte| byte < 0x20 || byte == b'*' || byte == b' ')
    {
        return Err("Plugin event name contains invalid characters".to_string());
    }
    Ok(())
}

pub(crate) fn validate_plugin_event_key(event_key: &str) -> Result<(), String> {
    let Some(rest) = event_key.strip_prefix("plugin.") else {
        return Err("Plugin event key must start with plugin.".to_string());
    };
    let Some((owner_plugin_id, event_name)) = rest.split_once(':') else {
        return Err("Plugin event key requires owner plugin id and event name".to_string());
    };
    native_plugin_custom_event_key(owner_plugin_id, event_name).map(|_| ())
}

pub fn validate_native_plugin_id(plugin_id: &str) -> Result<(), String> {
    if plugin_id.is_empty() {
        return Err("Plugin ID cannot be empty".to_string());
    }
    if plugin_id.contains("..") {
        return Err("Plugin ID cannot contain path traversal (..)".to_string());
    }
    if plugin_id.contains('/') || plugin_id.contains('\\') {
        return Err("Plugin ID cannot contain path separators".to_string());
    }
    if plugin_id.bytes().any(|byte| byte < 0x20) {
        return Err("Plugin ID contains invalid characters".to_string());
    }
    Ok(())
}

pub fn validate_plugin_relative_path(relative_path: &str) -> Result<(), String> {
    if relative_path.trim().is_empty() {
        return Err("Plugin relative path cannot be empty".to_string());
    }
    if relative_path.starts_with('/') || relative_path.starts_with('\\') {
        return Err("Absolute plugin paths are not allowed".to_string());
    }
    for component in relative_path.split(['/', '\\']) {
        if component == ".." {
            return Err("Plugin paths cannot escape the plugin directory".to_string());
        }
    }
    Ok(())
}

pub fn native_runtime_plan_for_manifest(
    manifest: &NativePluginManifest,
) -> Result<NativePluginRuntimePlan, String> {
    if let Some(runtime) = &manifest.runtime {
        validate_plugin_relative_path(&runtime.entry)?;
        return Ok(match runtime.kind {
            NativePluginRuntimeKind::Wasm => NativePluginRuntimePlan::Wasm {
                entry: runtime.entry.clone(),
            },
            NativePluginRuntimeKind::Process => NativePluginRuntimePlan::Process {
                entry: runtime.entry.clone(),
            },
            NativePluginRuntimeKind::ManifestOnly => NativePluginRuntimePlan::ManifestOnly,
        });
    }

    // Tauri plugins use ESM activate(ctx). Native keeps these visible for
    // migration, but never evaluates JavaScript or creates a WebView.
    if let Some(main) = &manifest.main {
        validate_plugin_relative_path(main)?;
        return Ok(NativePluginRuntimePlan::UnsupportedLegacyJs {
            entry: main.clone(),
        });
    }

    Ok(NativePluginRuntimePlan::ManifestOnly)
}

pub fn native_plugin_state_for(
    runtime_plan: &NativePluginRuntimePlan,
    config: &NativePluginConfigEntry,
) -> NativePluginState {
    if config.auto_disabled {
        return NativePluginState::AutoDisabled;
    }
    if !config.enabled {
        return NativePluginState::Disabled;
    }
    if config.last_error.is_some() {
        return NativePluginState::Error;
    }

    match runtime_plan {
        NativePluginRuntimePlan::ManifestOnly => NativePluginState::ReadyManifestOnly,
        NativePluginRuntimePlan::Wasm { .. } => NativePluginState::ReadyWasm,
        NativePluginRuntimePlan::Process { .. } => NativePluginState::ReadyProcess,
        NativePluginRuntimePlan::UnsupportedLegacyJs { .. } => {
            NativePluginState::UnsupportedLegacyJs
        }
    }
}

/// Resolves plugin state while withholding activation until the current
/// sensitive capability request has been approved.
pub fn native_plugin_state_for_manifest(
    manifest: &NativePluginManifest,
    runtime_plan: &NativePluginRuntimePlan,
    config: &NativePluginConfigEntry,
) -> NativePluginState {
    let state = native_plugin_state_for(runtime_plan, config);
    if matches!(
        state,
        NativePluginState::ReadyManifestOnly
            | NativePluginState::ReadyWasm
            | NativePluginState::ReadyProcess
    ) && native_plugin_requires_permission_review(manifest, runtime_plan, config)
    {
        return NativePluginState::Disabled;
    }
    state
}

pub fn native_runtime_kind_label(runtime_plan: &NativePluginRuntimePlan) -> &'static str {
    match runtime_plan {
        NativePluginRuntimePlan::ManifestOnly => "manifest-only",
        NativePluginRuntimePlan::Wasm { .. } => "wasm",
        NativePluginRuntimePlan::Process { .. } => "process",
        NativePluginRuntimePlan::UnsupportedLegacyJs { .. } => "legacy-js",
    }
}
