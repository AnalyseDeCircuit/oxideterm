// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::Context;
#[cfg(test)]
pub(super) use oxideterm_plugin_host_api::settings::native_syncable_settings_revision;
pub(super) use oxideterm_plugin_host_api::settings::{
    native_normalize_syncable_settings_payload, native_syncable_settings_apply_plan,
    native_syncable_settings_payload_arg,
};
use serde_json::Value;

use crate::workspace::WorkspaceApp;

pub(super) fn native_apply_syncable_settings_payload(
    workspace: &mut WorkspaceApp,
    payload: &Value,
    cx: &mut Context<WorkspaceApp>,
) -> Result<(), String> {
    let plan = native_syncable_settings_apply_plan(payload)?;
    if plan.is_empty() {
        return Ok(());
    }

    workspace.edit_settings(
        |settings| {
            if let Some(language) = plan.language {
                settings.general.language = language;
            }
            if let Some(ui_density) = plan.ui_density {
                settings.appearance.ui_density = ui_density;
            }
            if let Some(font_size) = plan.font_size {
                settings.terminal.font_size = font_size;
            }
            if let Some(theme) = plan.theme {
                settings.terminal.theme = theme;
            }
            if let Some(auto_reconnect) = plan.auto_reconnect {
                settings.reconnect.enabled = auto_reconnect;
            }
        },
        cx,
    );
    Ok(())
}
