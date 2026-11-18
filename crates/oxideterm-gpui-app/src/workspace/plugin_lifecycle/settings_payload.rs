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
use oxideterm_settings::PersistedSettings;
use oxideterm_settings_model::ThemeTarget;

use crate::workspace::WorkspaceApp;
use super::plugin_host;

impl WorkspaceApp {
    pub(in crate::workspace) fn emit_native_plugin_settings_events(
        &mut self,
        previous_settings: &PersistedSettings,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) {
        let previous_theme =
            ThemeTarget::Application.resolved_id(previous_settings, self.system_dark);
        let theme = ThemeTarget::Application.resolved_id(settings, self.system_dark);
        if previous_theme != theme {
            self.emit_native_plugin_event_to_subscribers(
                plugin_host::NATIVE_PLUGIN_APP_THEME_CHANGED_EVENT,
                serde_json::json!({
                    "theme": crate::workspace::plugin_lifecycle::native_plugin_theme_snapshot(
                        theme
                    ),
                }),
                cx,
            );
        }

        if previous_settings.general.language != settings.general.language {
            let language = settings.general.language.as_str();
            self.emit_native_plugin_event_to_subscribers(
                plugin_host::NATIVE_PLUGIN_I18N_LANGUAGE_CHANGED_EVENT,
                serde_json::json!({ "language": language }),
                cx,
            );
        }

        let previous_value =
            serde_json::to_value(previous_settings).unwrap_or_else(|_| serde_json::json!({}));
        let current_value =
            serde_json::to_value(settings).unwrap_or_else(|_| serde_json::json!({}));
        if previous_value != current_value {
            self.emit_native_plugin_app_settings_changed(current_value, cx);
        }
    }

    pub(in crate::workspace) fn emit_native_plugin_app_settings_changed(
        &mut self,
        settings: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        // Native plugins receive the same immutable settings snapshot as app.onSettingsChange.
        self.emit_native_plugin_event_to_subscribers(
            plugin_host::NATIVE_PLUGIN_APP_SETTINGS_CHANGED_EVENT,
            serde_json::json!({ "settings": settings }),
            cx,
        );
    }
}

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
            if (plan.theme.is_some() || plan.application_theme.is_some())
                && !settings.appearance.follow_system_appearance
                && let Some(scheme) = settings.appearance.manual_scheme
            {
                oxideterm_settings_model::apply_appearance_mode(settings, Some(scheme));
                settings.appearance.manual_scheme = None;
            }
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
            if let Some(theme) = plan.application_theme {
                settings.appearance.theme = theme;
            }
            if let Some(auto_reconnect) = plan.auto_reconnect {
                settings.reconnect.enabled = auto_reconnect;
            }
        },
        cx,
    );
    Ok(())
}
