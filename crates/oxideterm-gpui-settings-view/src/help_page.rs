#[cfg(not(target_family = "wasm"))]
mod desktop;
#[cfg(not(target_family = "wasm"))]
pub use desktop::HelpPage;

mod documents;
pub use documents::SettingsMarkdownDialog;

use std::{ops::Deref, rc::Rc};

use gpui::{AnyElement, App, MouseButton, MouseDownEvent, Window, div, prelude::*, px, rgb, rgba};
use oxideterm_gpui_ui::button::{
    ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonIconPosition,
    ToolbarButtonOptions, toolbar_button,
};
use oxideterm_settings_model::SettingsSectionId;

use crate::{
    SettingsSelect,
    components::{SettingsControls, settings_mono_font_family},
    set_diagnostics_debug_logging, set_show_terminal_performance_overlay,
    surface::nav::render_icon,
    update_channel_label,
};

const HELP_WEBSITE_URL: &str = "https://oxideterm.app";
const HELP_DOCUMENTATION_URL: &str = "https://oxideterm.app/docs";
const HELP_GITHUB_URL: &str = "https://github.com/AnalyseDeCircuit/oxideterm";
const HELP_ISSUES_URL: &str = "https://github.com/AnalyseDeCircuit/oxideterm/issues";
// Keep the in-app legal link aligned with the repository-level multilingual notice.
const HELP_LEGAL_URL: &str = "https://github.com/AnalyseDeCircuit/oxideterm/blob/main/LEGAL.md";
// Product and library names stay untranslated so every locale uses their canonical spelling.
const HELP_TECH_BADGES: [(&str, u32); 9] = [
    ("Rust", 0xf97316),
    ("GPUI", 0x38bdf8),
    ("Tokio", 0x3b82f6),
    ("alacritty_terminal", 0xf46d01),
    ("russh", 0xeab308),
    ("tree-sitter", 0x14b8a6),
    ("Wasmtime / WASI", 0xa855f7),
    ("redb", 0x22c55e),
    ("IronRDP", 0x6366f1),
];

#[derive(Clone, Copy)]
pub enum HelpRequest {
    Check,
    Download,
    Install,
    Cancel,
    ReleaseNotes,
    Legal,
    ThirdParty,
    Logs,
    Url(&'static str),
}

pub(crate) struct HelpPresentation<'a> {
    pub controls: SettingsControls<'a>,
    pub year: &'a str,
    pub dispatch: Rc<dyn Fn(HelpRequest, &mut App)>,
}

impl<'a> Deref for HelpPresentation<'a> {
    type Target = SettingsControls<'a>;

    fn deref(&self) -> &Self::Target {
        &self.controls
    }
}

impl HelpPresentation<'_> {
    fn action(
        &self,
        request: HelpRequest,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + use<> {
        let dispatch = self.dispatch.clone();
        move |_, _, cx| {
            dispatch(request, cx);
            cx.stop_propagation();
        }
    }
    pub(crate) fn section(&self, section: SettingsSectionId, cx: &App) -> AnyElement {
        match section {
            SettingsSectionId::HelpVersion => self.help_version_card(false, None, cx),
            SettingsSectionId::HelpDiagnostics => self.help_diagnostics_card(false, cx),
            SettingsSectionId::HelpTechStack => self.help_tech_stack_card(),
            SettingsSectionId::HelpResources => self.help_resources_card(cx),
            SettingsSectionId::HelpSafety => self.help_safety_card(),
            SettingsSectionId::HelpLegal => self.help_legal_card(cx),
            _ => div().into_any_element(),
        }
    }

    fn help_version_card(
        &self,
        is_portable: bool,
        update_footer: Option<AnyElement>,
        cx: &App,
    ) -> AnyElement {
        let channel_label = update_channel_label(self.settings.general.update_channel, &self.i18n);
        let version_rows = div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(self.help_key_value_row(
                "settings_view.help.app_name",
                "OxideTerm".to_string(),
                false,
                cx,
            ))
            .child(self.help_key_value_row(
                "settings_view.help.version",
                env!("CARGO_PKG_VERSION").to_string(),
                true,
                cx,
            ))
            .child(self.help_portable_or_channel_row(is_portable, channel_label, cx));

        let mut rows = vec![version_rows.into_any_element()];
        rows.extend(update_footer);
        self.settings_card("settings_view.help.version_info", "", rows)
    }

    fn help_diagnostics_card(&self, open_logs: bool, cx: &App) -> AnyElement {
        let mut rows = vec![
            self.bool_row(
                "settings_view.help.debug_logs",
                "settings_view.help.debug_logs_hint",
                self.settings.diagnostics.debug_logging,
                set_diagnostics_debug_logging,
                cx,
            ),
            self.card_separator(),
            self.bool_row(
                "settings_view.terminal.show_performance_overlay",
                "settings_view.terminal.show_performance_overlay_hint",
                self.settings.terminal.show_fps_overlay,
                set_show_terminal_performance_overlay,
                cx,
            ),
        ];
        if open_logs {
            rows.push(self.card_separator());
            rows.push(self.help_action_row(
                "settings_view.help.open_logs",
                "settings_view.help.open_logs_hint",
                self.i18n.t("settings_view.help.open"),
                "lucide/folder-open.svg",
                HelpRequest::Logs,
                cx,
            ));
        }
        self.settings_card("settings_view.help.diagnostics", "", rows)
    }

    fn help_tech_stack_card(&self) -> AnyElement {
        let mut badges = div().flex().flex_row().flex_wrap().gap(px(8.0));
        for (label, color) in HELP_TECH_BADGES {
            badges = badges.child(
                div()
                    .rounded_full()
                    .bg(rgba((color << 8) | 0x26))
                    .px(px(10.0))
                    .py(px(4.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(color))
                    .child(label),
            );
        }

        self.settings_card(
            "settings_view.help.tech_stack",
            "",
            vec![badges.into_any_element()],
        )
    }

    fn help_resources_card(&self, cx: &App) -> AnyElement {
        let links = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(self.help_resource_link(
                "settings_view.help.website",
                HELP_WEBSITE_URL,
                "lucide/external-link.svg",
                cx,
            ))
            .child(self.help_resource_link(
                "settings_view.help.documentation",
                HELP_DOCUMENTATION_URL,
                "lucide/book-open.svg",
                cx,
            ))
            .child(self.help_resource_link(
                "settings_view.help.github",
                HELP_GITHUB_URL,
                "lucide/git-fork.svg",
                cx,
            ))
            .child(self.help_resource_link(
                "settings_view.help.issues",
                HELP_ISSUES_URL,
                "lucide/help-circle.svg",
                cx,
            ))
            .child(self.help_resource_link(
                "settings_view.help.disclaimer",
                HELP_LEGAL_URL,
                "lucide/shield.svg",
                cx,
            ))
            .into_any_element();

        self.settings_card("settings_view.help.resources", "", vec![links])
    }

    fn help_safety_card(&self) -> AnyElement {
        // Keep product guardrails visible without turning them into a blocking legal agreement.
        let safety_items = [
            "settings_view.help.safety_authorized",
            "settings_view.help.safety_connections",
            "settings_view.help.safety_prohibited",
            "settings_view.help.safety_privacy",
            "settings_view.help.safety_secrets",
            "settings_view.help.safety_ai",
        ];
        let mut safety_rows = div().flex().flex_col().gap(px(10.0));
        for key in safety_items {
            safety_rows = safety_rows.child(self.help_safety_row(key));
        }

        self.settings_card(
            "settings_view.help.safety_title",
            "",
            vec![safety_rows.into_any_element()],
        )
    }

    fn help_safety_row(&self, key: &str) -> AnyElement {
        div()
            .flex()
            .items_start()
            .gap(px(10.0))
            .child(
                div()
                    .mt(px(7.0))
                    .size(px(5.0))
                    .rounded_full()
                    .bg(rgb(self.tokens.ui.accent)),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .line_height(px(20.0))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(key)),
            )
            .into_any_element()
    }

    fn help_legal_card(&self, cx: &App) -> AnyElement {
        let copyright = self
            .i18n
            .t("settings_view.help.copyright")
            .replace("{{year}}", self.year)
            .replace("{{author}}", "AnalyseDeCircuit");

        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(4.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child((self.label)(
                "settings-help-legal",
                "copyright",
                copyright,
                self.tokens.ui.text_muted,
                cx,
            ))
            .child((self.label)(
                "settings-help-legal",
                "license",
                self.i18n.t("settings_view.help.license"),
                self.tokens.ui.text_muted,
                cx,
            ))
            .child(self.help_outline_button(
                self.i18n.t("settings_view.help.third_party_notices"),
                "lucide/book-open.svg",
                HelpRequest::ThirdParty,
                cx,
            ))
            .into_any_element()
    }

    fn help_portable_or_channel_row(
        &self,
        is_portable: bool,
        channel_label: String,
        cx: &App,
    ) -> AnyElement {
        if is_portable {
            return self.setting_row(
                "settings_view.help.portable_mode",
                "settings_view.help.portable_mode_hint",
                self.help_pill_badge(
                    self.i18n.t("settings_view.help.portable_updates"),
                    self.tokens.ui.text,
                ),
                cx,
            );
        }

        self.setting_row(
            "settings_view.help.update_channel",
            "settings_view.help.update_channel_hint",
            self.settings_select_control(
                SettingsSelect::UpdateChannel,
                channel_label,
                false,
                Some(self.tokens.metrics.settings_select_width),
                cx,
            ),
            cx,
        )
    }

    fn help_key_value_row(
        &self,
        label_key: &str,
        value: String,
        mono: bool,
        cx: &App,
    ) -> AnyElement {
        let mut value_el = div()
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.text))
            .child((self.label)(
                "settings-help-value",
                label_key,
                value,
                self.tokens.ui.text,
                cx,
            ));
        if mono {
            value_el = value_el.font_family(settings_mono_font_family(self.settings));
        }

        self.setting_row(label_key, "", value_el.max_w_full().into_any_element(), cx)
    }

    fn help_action_row(
        &self,
        label_key: &str,
        hint_key: &str,
        button_label: String,
        icon: &'static str,
        request: HelpRequest,
        cx: &App,
    ) -> AnyElement {
        self.setting_row(
            label_key,
            hint_key,
            self.help_outline_button(button_label, icon, request, cx),
            cx,
        )
    }

    fn help_outline_button(
        &self,
        label: String,
        icon: &'static str,
        request: HelpRequest,
        cx: &App,
    ) -> AnyElement {
        self.help_outline_button_with_disabled(label, icon, false, request, cx)
    }

    fn help_outline_button_with_disabled(
        &self,
        label: String,
        icon: &'static str,
        disabled: bool,
        request: HelpRequest,
        _: &App,
    ) -> AnyElement {
        let action = self.action(request);
        toolbar_button(
            self.tokens,
            label,
            Some(render_icon(icon, 14.0, rgb(self.tokens.ui.text))),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled,
                },
                icon_position: ToolbarButtonIconPosition::Leading,
                ..ToolbarButtonOptions::default()
            },
        )
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            if !disabled {
                action(event, window, cx);
            }
            cx.stop_propagation();
        })
        .into_any_element()
    }

    fn help_pill_badge(&self, label: String, color: u32) -> AnyElement {
        div()
            .rounded_full()
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_elevated))
            .px(px(12.0))
            .py(px(4.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(color))
            .child(label)
            .into_any_element()
    }

    fn help_resource_link(
        &self,
        label_key: &str,
        url: &'static str,
        icon: &'static str,
        _: &App,
    ) -> AnyElement {
        div()
            .rounded(px(self.tokens.radii.md))
            .px(px(12.0))
            .py(px(10.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .cursor_pointer()
            .hover(|row| row.bg(rgb(self.tokens.ui.bg_hover)))
            .on_mouse_down(
                MouseButton::Left,
                self.action(if url == HELP_LEGAL_URL {
                    HelpRequest::Legal
                } else {
                    HelpRequest::Url(url)
                }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(render_icon(icon, 18.0, rgb(self.tokens.ui.text_muted)))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t(label_key)),
                    ),
            )
            .child(render_icon(
                "lucide/external-link.svg",
                14.0,
                rgb(self.tokens.ui.text_muted),
            ))
            .into_any_element()
    }
}
