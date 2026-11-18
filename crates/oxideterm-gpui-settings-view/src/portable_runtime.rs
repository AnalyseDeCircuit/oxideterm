use std::rc::Rc;

pub mod dialog;

use gpui::{AnyElement, App, MouseButton, MouseDownEvent, Window, div, prelude::*, px, rgb, rgba};
use oxideterm_gpui_ui::{
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonIconPosition,
        ToolbarButtonOptions, toolbar_button,
    },
    checkbox::{CheckboxOptions, checkbox_with},
};
use oxideterm_settings_model::SettingsSectionId;

use crate::{
    components::{SettingsCalloutTone, SettingsComponents},
    entity::{PortableStatusSnapshot, SettingsWorkspaceEntity},
    surface::nav::render_icon,
};

const PORTABLE_SETTINGS_PATH_CARD_GAP: f32 = 12.0;
const PORTABLE_SETTINGS_BUTTON_GAP: f32 = 12.0;

fn portable_activation_label(
    i18n: &oxideterm_i18n::I18n,
    activation: oxideterm_portable_runtime::PortableActivationKind,
) -> String {
    match activation {
        oxideterm_portable_runtime::PortableActivationKind::Marker => {
            i18n.t("settings_view.general.portable_activation_marker")
        }
        oxideterm_portable_runtime::PortableActivationKind::Config => {
            i18n.t("settings_view.general.portable_activation_config")
        }
        oxideterm_portable_runtime::PortableActivationKind::Disabled => {
            i18n.t("settings_view.general.portable_activation_disabled")
        }
    }
}

fn portable_status_badge_color(
    status: oxideterm_portable_runtime::PortableBootstrapStatus,
    tokens: &oxideterm_theme::ThemeTokens,
) -> u32 {
    match status {
        oxideterm_portable_runtime::PortableBootstrapStatus::Unlocked => tokens.ui.success,
        oxideterm_portable_runtime::PortableBootstrapStatus::Disabled => tokens.ui.text_muted,
        oxideterm_portable_runtime::PortableBootstrapStatus::NeedsSetup
        | oxideterm_portable_runtime::PortableBootstrapStatus::Locked => tokens.ui.warning,
    }
}

#[derive(Clone, Copy)]
pub enum PortableSettingsRequest {
    ToggleAutoUnlock(bool),
    ChangePassword,
    Export,
    Import,
}

pub struct PortableSettingsPage<'a> {
    pub components: SettingsComponents<'a>,
    pub state: &'a SettingsWorkspaceEntity,
    pub current_data_dir: &'a str,
    pub text: &'a dyn Fn(String, u32) -> AnyElement,
    pub dispatch: Rc<dyn Fn(PortableSettingsRequest, &mut App)>,
}

impl<'a> std::ops::Deref for PortableSettingsPage<'a> {
    type Target = SettingsComponents<'a>;

    fn deref(&self) -> &Self::Target {
        &self.components
    }
}

impl PortableSettingsPage<'_> {
    fn portable_settings_text(
        &self,
        text: String,
        size: f32,
        color: u32,
        weight: Option<gpui::FontWeight>,
    ) -> AnyElement {
        let mut text_block = div()
            .w_full()
            .min_w(px(0.0))
            .text_size(px(size))
            .text_color(rgb(color))
            // Tauri renders these PortableTab labels as normal block text.
            // GPUI can otherwise measure CJK text in a narrow flex column and
            // wrap it one glyph per line.
            .line_height(px((size + 4.0).max(16.0)));
        if let Some(weight) = weight {
            text_block = text_block.font_weight(weight);
        }
        text_block
            .child((self.text)(text, color))
            .into_any_element()
    }

    pub fn render_section(&self, section: SettingsSectionId) -> AnyElement {
        let snapshot = self.state.portable_status_snapshot();
        match section {
            SettingsSectionId::PortableRuntime => div()
                .flex()
                .flex_col()
                .gap(px(self.tokens.metrics.settings_page_gap))
                .child(self.portable_runtime_card(&snapshot))
                .child(self.portable_migration_card(&snapshot))
                .into_any_element(),
            _ => div().into_any_element(),
        }
    }

    fn portable_runtime_card(&self, portable_snapshot: &PortableStatusSnapshot) -> AnyElement {
        let portable_status = portable_snapshot.status.as_ref();
        let is_portable = portable_status.is_some_and(|status| status.is_portable);
        let hint_key = if is_portable {
            "settings_view.general.portable_runtime_hint"
        } else {
            "settings_view.general.portable_runtime_disabled_hint"
        };
        let mut rows = vec![self.portable_runtime_summary_row(portable_status, hint_key)];

        if let Some(status) = portable_status.filter(|status| status.is_portable) {
            rows.push(self.card_separator());
            rows.push(self.portable_path_group(status));
            rows.push(self.card_separator());
            rows.push(self.portable_security_group(status));
        }

        self.settings_card("settings_view.general.portable_runtime", "", rows)
    }

    fn portable_runtime_summary_row(
        &self,
        portable_status: Option<&oxideterm_portable_runtime::PortableStatusSnapshot>,
        hint_key: &str,
    ) -> AnyElement {
        let (badge_label, badge_color) = portable_status
            .map(|status| {
                (
                    self.i18n.t(match status.status {
                        oxideterm_portable_runtime::PortableBootstrapStatus::Disabled => {
                            "settings_view.general.portable_status_disabled"
                        }
                        oxideterm_portable_runtime::PortableBootstrapStatus::NeedsSetup => {
                            "settings_view.general.portable_status_needs_setup"
                        }
                        oxideterm_portable_runtime::PortableBootstrapStatus::Locked => {
                            "settings_view.general.portable_status_locked"
                        }
                        oxideterm_portable_runtime::PortableBootstrapStatus::Unlocked => {
                            "settings_view.general.portable_status_unlocked"
                        }
                    }),
                    portable_status_badge_color(status.status, &self.tokens),
                )
            })
            .unwrap_or_else(|| {
                (
                    self.i18n
                        .t("settings_view.general.portable_activation_disabled"),
                    self.tokens.ui.text_muted,
                )
            });

        div()
            .w_full()
            .flex()
            .flex_row()
            .items_start()
            .justify_between()
            .gap(px(16.0))
            .child(div().flex().flex_col().w_full().min_w(px(0.0)).child(
                self.portable_settings_text(
                    self.i18n.t(hint_key),
                    self.tokens.metrics.ui_text_xs,
                    self.tokens.ui.text_muted,
                    None,
                ),
            ))
            .child(self.text_badge(badge_label, badge_color))
            .into_any_element()
    }

    fn portable_path_group(
        &self,
        status: &oxideterm_portable_runtime::PortableStatusSnapshot,
    ) -> AnyElement {
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(PORTABLE_SETTINGS_PATH_CARD_GAP))
            .child(self.portable_value_box(
                "settings_view.general.portable_root_dir",
                status.portable_root_dir.clone(),
                true,
            ))
            .child(self.portable_value_box(
                "settings_view.general.portable_activation",
                portable_activation_label(&self.i18n, status.activation),
                false,
            ))
            .child(self.portable_value_box(
                "settings_view.general.portable_config_path",
                status.config_path.clone(),
                true,
            ))
            .child(self.portable_value_box(
                "settings_view.general.data_directory",
                status.data_dir.clone(),
                true,
            ))
            .child(self.portable_value_box(
                "settings_view.general.portable_instance_lock_path",
                status.instance_lock_path.clone().unwrap_or_else(|| {
                    self.i18n
                        .t("settings_view.general.portable_instance_lock_unavailable")
                }),
                true,
            ))
            .into_any_element()
    }

    fn portable_value_box(&self, label_key: &str, value: String, mono: bool) -> AnyElement {
        let value_el = div().child(value).into_any_element();
        let value_row = if mono {
            self.settings_mono_box(value_el)
        } else {
            div()
                .w_full()
                .min_w(px(0.0))
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(rgba((self.tokens.ui.border << 8) | 0x66))
                .bg(rgb(self.tokens.ui.bg_sunken))
                .px(px(10.0))
                .py(px(8.0))
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .text_color(rgb(self.tokens.ui.text))
                .line_height(px((self.tokens.metrics.ui_text_xs + 4.0).max(16.0)))
                .child(value_el)
                .into_any_element()
        };

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .min_w(px(0.0))
            .child(self.portable_settings_text(
                self.i18n.t(label_key),
                self.tokens.metrics.ui_text_xs,
                self.tokens.ui.text_muted,
                None,
            ))
            .child(value_row)
            .into_any_element()
    }

    fn portable_security_group(
        &self,
        status: &oxideterm_portable_runtime::PortableStatusSnapshot,
    ) -> AnyElement {
        let can_change_password = status.is_unlocked;
        let auto_unlock_enabled = status.auto_unlock_enabled;
        let auto_unlock_pending = self.state.portable_auto_unlock_pending();
        let can_toggle_auto_unlock = status.is_unlocked && !auto_unlock_pending;
        let action_error = self.state.portable_action_error().map(str::to_owned);
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(PORTABLE_SETTINGS_BUTTON_GAP))
            .child(
                div()
                    .w_full()
                    .child(crate::settings_row(
                        &self.tokens,
                        self.portable_settings_text(
                            self.i18n.t("settings_view.general.portable_auto_unlock"),
                            self.tokens.metrics.ui_text_sm,
                            self.tokens.ui.text,
                            Some(gpui::FontWeight::MEDIUM),
                        ),
                        Some(
                            self.portable_settings_text(
                                self.i18n
                                    .t("settings_view.general.portable_auto_unlock_hint"),
                                self.tokens.metrics.ui_text_xs,
                                self.tokens.ui.text_muted,
                                None,
                            ),
                        ),
                        checkbox_with(
                            &self.tokens,
                            "portable-auto-unlock",
                            String::new(),
                            auto_unlock_enabled,
                            CheckboxOptions {
                                disabled: !can_toggle_auto_unlock,
                                ..CheckboxOptions::default()
                            },
                        )
                        .into_any_element(),
                    ))
                    .when(can_toggle_auto_unlock, |row| {
                        // The label and hint provide a discoverable target around the compact control.
                        row.cursor_pointer().on_mouse_down(
                            MouseButton::Left,
                            self.request(PortableSettingsRequest::ToggleAutoUnlock(
                                !auto_unlock_enabled,
                            )),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap(px(PORTABLE_SETTINGS_BUTTON_GAP))
                    .child(
                        self.portable_action_button(
                            self.i18n
                                .t("settings_view.general.portable_change_password"),
                            "lucide/key.svg",
                            can_change_password,
                            false,
                            self.request(PortableSettingsRequest::ChangePassword),
                        ),
                    ),
            )
            .when_some(action_error, |group, error| {
                group.child(self.settings_callout(
                    SettingsCalloutTone::Error,
                    Some("lucide/alert-circle.svg"),
                    div().child(error).into_any_element(),
                ))
            })
            .into_any_element()
    }

    fn portable_migration_card(&self, portable_snapshot: &PortableStatusSnapshot) -> AnyElement {
        let portable_status = portable_snapshot.status.as_ref();
        let is_portable = portable_status.is_some_and(|status| status.is_portable);
        let current_data_dir = self.current_data_dir.to_owned();
        let portable_data_dir = portable_status
            .map(|status| status.data_dir.clone())
            .unwrap_or_else(|| current_data_dir.clone());
        let secret_count = portable_snapshot.exportable_secret_count.unwrap_or(0);

        let desc_key = if is_portable {
            "settings_view.general.portable_migration_portable_hint"
        } else {
            "settings_view.general.portable_migration_installed_hint"
        };

        self.settings_card(
            "settings_view.general.portable_migration",
            desc_key,
            self.separated_settings_rows(vec![
                div()
                    .flex()
                    .flex_col()
                    .gap(px(PORTABLE_SETTINGS_PATH_CARD_GAP))
                    .child(self.portable_value_box(
                        "settings_view.general.portable_migration_current_dir",
                        current_data_dir,
                        true,
                    ))
                    .child(self.portable_value_box(
                        "settings_view.general.portable_migration_target_dir",
                        portable_data_dir,
                        true,
                    ))
                    .child(
                        self.portable_settings_text(
                            self.i18n
                                .t("settings_view.general.portable_migration_secret_summary")
                                .replace("{{count}}", &secret_count.to_string()),
                            self.tokens.metrics.ui_text_xs,
                            self.tokens.ui.text_muted,
                            None,
                        ),
                    )
                    .into_any_element(),
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap(px(PORTABLE_SETTINGS_BUTTON_GAP))
                    .child(
                        self.portable_action_button(
                            self.i18n
                                .t("settings_view.general.portable_migration_export"),
                            "lucide/upload.svg",
                            true,
                            false,
                            self.request(PortableSettingsRequest::Export),
                        ),
                    )
                    .child(
                        self.portable_action_button(
                            self.i18n
                                .t("settings_view.general.portable_migration_import"),
                            "lucide/download.svg",
                            true,
                            false,
                            self.request(PortableSettingsRequest::Import),
                        ),
                    )
                    .into_any_element(),
            ]),
        )
    }

    fn request(
        &self,
        request: PortableSettingsRequest,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + use<> {
        let dispatch = self.dispatch.clone();
        move |_, _, cx| {
            dispatch(request, cx);
            cx.stop_propagation();
        }
    }

    fn portable_action_button(
        &self,
        label: String,
        icon: &'static str,
        enabled: bool,
        loading: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        toolbar_button(
            self.tokens,
            label,
            Some(render_icon(icon, 14.0, rgb(self.tokens.ui.text))),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: !enabled,
                },
                icon_position: ToolbarButtonIconPosition::Leading,
                loading,
                ..ToolbarButtonOptions::default()
            },
        )
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            if enabled && !loading {
                listener(event, window, cx);
            }
            cx.stop_propagation();
        })
        .into_any_element()
    }
}
