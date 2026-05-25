// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::Context;
pub(super) use oxideterm_plugin_host_api::settings::{
    native_normalize_syncable_settings_payload, native_syncable_settings_payload,
    native_syncable_settings_payload_arg, native_syncable_settings_revision,
};
use oxideterm_settings::{Language, UiDensity};
use serde_json::{Value, json};

use crate::workspace::WorkspaceApp;

pub(super) fn native_apply_syncable_settings_payload(
    workspace: &mut WorkspaceApp,
    payload: &Value,
    cx: &mut Context<WorkspaceApp>,
) -> Result<(), String> {
    let language = payload
        .pointer("/appearance/language")
        .and_then(Value::as_str)
        .map(native_parse_language)
        .transpose()?;
    let ui_density = payload
        .pointer("/appearance/uiDensity")
        .and_then(Value::as_str)
        .map(native_parse_ui_density)
        .transpose()?;
    let font_size = payload
        .pointer("/terminal/fontSize")
        .and_then(Value::as_i64);
    let theme = payload
        .pointer("/terminal/theme")
        .and_then(Value::as_str)
        .map(str::to_string);
    let auto_reconnect = payload
        .pointer("/reconnect/autoReconnect")
        .and_then(Value::as_bool);

    if language.is_none()
        && ui_density.is_none()
        && font_size.is_none()
        && theme.is_none()
        && auto_reconnect.is_none()
    {
        return Ok(());
    }

    workspace.edit_settings(
        |settings| {
            if let Some(language) = language {
                settings.general.language = language;
            }
            if let Some(ui_density) = ui_density {
                settings.appearance.ui_density = ui_density;
            }
            if let Some(font_size) = font_size {
                settings.terminal.font_size = font_size;
            }
            if let Some(theme) = theme {
                settings.terminal.theme = theme;
            }
            if let Some(auto_reconnect) = auto_reconnect {
                settings.reconnect.enabled = auto_reconnect;
            }
        },
        cx,
    );
    Ok(())
}

fn native_parse_language(language: &str) -> Result<Language, String> {
    serde_json::from_value::<Language>(json!(language))
        .map_err(|_| format!("Unsupported language: {language}"))
}

fn native_parse_ui_density(ui_density: &str) -> Result<UiDensity, String> {
    serde_json::from_value::<UiDensity>(json!(ui_density))
        .map_err(|_| format!("Unsupported ui density: {ui_density}"))
}
