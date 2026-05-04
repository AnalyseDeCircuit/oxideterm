// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::HashMap;

use serde_json::{Map, Value, json};

use crate::model::PersistedSettings;

pub const SETTINGS_STORAGE_KEY: &str = "oxide-settings-v2";
pub const LEGACY_SETTINGS_KEY: &str = "oxide-settings";
pub const LEGACY_UI_STATE_KEY: &str = "oxide-ui-state";
pub const LEGACY_TREE_EXPANDED_KEY: &str = "oxide-tree-expanded";
pub const LEGACY_FOCUSED_NODE_KEY: &str = "oxide-focused-node";
pub const APP_LANG_KEY: &str = "app_lang";
pub const KEYBINDINGS_KEY: &str = "oxideterm_keybindings";
pub const CUSTOM_THEMES_KEY: &str = "oxide-custom-themes";
pub const LAUNCHER_ENABLED_KEY: &str = "oxide-launcher-enabled";
pub const AGENT_ROLES_KEY: &str = "oxideterm:agent-roles";
pub const NEW_CONNECTION_SAVE_KEY: &str = "oxideterm.saveConnection";

fn parse_json(raw: Option<&String>) -> Option<Value> {
    raw.and_then(|value| serde_json::from_str(value).ok())
}

fn set_path(root: &mut Value, path: &[&str], value: Value) {
    let mut current = root;
    for segment in &path[..path.len().saturating_sub(1)] {
        if current.get(*segment).and_then(Value::as_object).is_none() {
            current[*segment] = json!({});
        }
        current = current.get_mut(*segment).expect("path segment exists");
    }
    if let Some(last) = path.last()
        && let Some(object) = current.as_object_mut()
    {
        object.insert((*last).to_string(), value);
    }
}

fn bool_string(raw: Option<&String>) -> Option<bool> {
    raw.map(String::as_str).and_then(|value| match value {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    })
}

pub fn legacy_local_storage_value(entries: &HashMap<String, String>) -> Value {
    let mut root = parse_json(entries.get(SETTINGS_STORAGE_KEY))
        .or_else(|| parse_json(entries.get(LEGACY_SETTINGS_KEY)))
        .unwrap_or_else(|| PersistedSettings::default().to_value());

    if let Some(language) = entries.get(APP_LANG_KEY).filter(|value| !value.is_empty()) {
        let has_language = root
            .get("general")
            .and_then(|general| general.get("language"))
            .is_some();
        if !has_language {
            set_path(&mut root, &["general", "language"], json!(language));
        }
    }

    if let Some(expanded) = parse_json(entries.get(LEGACY_TREE_EXPANDED_KEY))
        && expanded.is_array()
    {
        set_path(&mut root, &["treeUI", "expandedIds"], expanded);
    }
    if let Some(focused) = entries.get(LEGACY_FOCUSED_NODE_KEY) {
        if focused.is_empty() {
            set_path(&mut root, &["treeUI", "focusedNodeId"], Value::Null);
        } else {
            set_path(&mut root, &["treeUI", "focusedNodeId"], json!(focused));
        }
    }

    if let Some(ui_state) = parse_json(entries.get(LEGACY_UI_STATE_KEY))
        && let Some(object) = ui_state.as_object()
    {
        if let Some(value) = object
            .get("sidebarCollapsed")
            .or_else(|| object.get("collapsed"))
        {
            set_path(&mut root, &["sidebarUI", "collapsed"], value.clone());
        }
        if let Some(value) = object
            .get("sidebarActiveSection")
            .or_else(|| object.get("activeSection"))
        {
            set_path(&mut root, &["sidebarUI", "activeSection"], value.clone());
        }
        if let Some(value) = object.get("sidebarWidth").or_else(|| object.get("width")) {
            set_path(&mut root, &["sidebarUI", "width"], value.clone());
        }
    }

    if let Some(Value::Object(overrides)) = parse_json(entries.get(KEYBINDINGS_KEY)) {
        set_path(
            &mut root,
            &["keybindings", "overrides"],
            Value::Object(overrides),
        );
    }
    if let Some(Value::Object(themes)) = parse_json(entries.get(CUSTOM_THEMES_KEY)) {
        set_path(&mut root, &["customThemes"], Value::Object(themes));
    }
    if let Some(enabled) = bool_string(entries.get(LAUNCHER_ENABLED_KEY)) {
        set_path(&mut root, &["launcher", "enabled"], json!(enabled));
    }
    if let Some(agent_roles) = parse_json(entries.get(AGENT_ROLES_KEY)) {
        set_path(&mut root, &["agentRoles"], agent_roles);
    }
    if let Some(save_connection) = bool_string(entries.get(NEW_CONNECTION_SAVE_KEY)) {
        set_path(
            &mut root,
            &["newConnection", "saveConnection"],
            json!(save_connection),
        );
    }

    if let Some(object) = root.as_object_mut() {
        object
            .entry("migrationMetadata".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
    }

    root
}
