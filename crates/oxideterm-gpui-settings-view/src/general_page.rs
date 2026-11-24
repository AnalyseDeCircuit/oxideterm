use gpui::{AnyElement, App, div, prelude::*};
use oxideterm_settings_model::SettingsSectionId;

use crate::{SettingsSelect, components::SettingsControls, select_menu::language_label};

#[cfg(not(target_family = "wasm"))]
mod desktop;
#[cfg(not(target_family = "wasm"))]
mod device;
#[cfg(not(target_family = "wasm"))]
pub use desktop::*;
#[cfg(not(target_family = "wasm"))]
pub use device::render_data_directory_confirm;

impl SettingsControls<'_> {
    pub(crate) fn general_preference_section(
        &self,
        section: SettingsSectionId,
        cx: &App,
    ) -> AnyElement {
        let settings = self.settings;
        match section {
            SettingsSectionId::GeneralLanguage => self.settings_card(
                "settings_view.general.language",
                "",
                vec![self.select_setting_row(
                    "settings_view.general.language",
                    "settings_view.general.language_hint",
                    SettingsSelect::Language,
                    language_label(settings.general.language),
                    self.tokens.metrics.settings_select_width,
                    cx,
                )],
            ),
            SettingsSectionId::GeneralExternalConnectionUris => self.settings_card(
                "settings_view.general.connection_uri_integration",
                "settings_view.general.connection_uri_integration_hint",
                vec![self.bool_row(
                    "settings_view.general.external_connection_uris",
                    "settings_view.general.external_connection_uris_hint",
                    settings.general.external_connection_uris_enabled,
                    |settings, enabled| settings.general.external_connection_uris_enabled = enabled,
                    cx,
                )],
            ),
            SettingsSectionId::GeneralWindowBehavior => {
                let (label_key, hint_key) = close_to_background_label_keys();
                self.settings_card(
                    "settings_view.general.window_behavior",
                    "settings_view.general.window_behavior_hint",
                    vec![self.bool_row(
                        label_key,
                        hint_key,
                        settings.general.minimize_to_tray_on_close,
                        |settings, enabled| settings.general.minimize_to_tray_on_close = enabled,
                        cx,
                    )],
                )
            }
            _ => div().into_any_element(),
        }
    }
}

fn close_to_background_label_keys() -> (&'static str, &'static str) {
    if cfg!(target_os = "macos") {
        (
            "settings_view.general.keep_running_on_close",
            "settings_view.general.keep_running_on_close_hint",
        )
    } else {
        (
            "settings_view.general.minimize_to_tray_on_close",
            "settings_view.general.minimize_to_tray_on_close_hint",
        )
    }
}
