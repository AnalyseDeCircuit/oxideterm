use std::rc::Rc;

use gpui::{AnyElement, App, MouseButton, Window, div, prelude::*, px, rgb, rgba};
use oxideterm_gpui_ui::button::{
    ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonOptions, toolbar_button,
};
use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;

use crate::{components::SettingsControls, surface::nav::render_icon};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppLockDialog {
    Configure,
    Change,
    Remove,
}

pub fn render_app_lock_settings(
    controls: SettingsControls<'_>,
    configured: bool,
    pending: bool,
    error: Option<String>,
    open: Rc<dyn Fn(AppLockDialog, &mut Window, &mut App)>,
    cx: &App,
) -> AnyElement {
    let button = |key: &str, variant, dialog| {
        let open = open.clone();
        toolbar_button(
            controls.tokens,
            controls.i18n.t(key),
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: pending,
                },
                ..ToolbarButtonOptions::default()
            },
        )
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            if !pending {
                open(dialog, window, cx);
            }
            cx.stop_propagation();
        })
        .into_any_element()
    };
    let status_label = controls.i18n.t(if configured {
        "settings_view.general.app_lock_configured"
    } else {
        "settings_view.general.app_lock_not_configured"
    });
    let status_color = if configured {
        controls.tokens.ui.success
    } else {
        controls.tokens.ui.text_muted
    };
    let mut actions = div().flex().flex_row().flex_wrap().gap(px(8.0));
    if configured {
        actions = actions
            .child(button(
                "settings_view.general.app_lock_change_password",
                ButtonVariant::Outline,
                AppLockDialog::Change,
            ))
            .child(button(
                "settings_view.general.app_lock_remove",
                ButtonVariant::Destructive,
                AppLockDialog::Remove,
            ));
    } else {
        actions = actions.child(button(
            "settings_view.general.app_lock_set_password",
            ButtonVariant::Default,
            AppLockDialog::Configure,
        ));
    }

    let mut rows = vec![
        crate::settings_row(
            &controls.tokens,
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(8.0))
                .child(render_icon(
                    "lucide/lock.svg",
                    16.0,
                    rgb(controls.tokens.ui.text),
                ))
                .child(
                    div()
                        .text_size(px(controls.tokens.metrics.ui_text_sm))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(controls.i18n.t("settings_view.general.app_lock_label")),
                )
                .child(controls.text_badge(status_label, status_color))
                .into_any_element(),
            Some(
                div()
                    .child(
                        controls
                            .i18n
                            .t("settings_view.general.app_lock_description"),
                    )
                    .into_any_element(),
            ),
            actions.into_any_element(),
        ),
        controls.card_separator(),
        controls.bool_row(
            "settings_view.general.app_lock_show_sidebar_icon",
            "settings_view.general.app_lock_show_sidebar_icon_hint",
            controls.settings.sidebar_ui.show_app_lock_icon,
            |settings, enabled| settings.sidebar_ui.show_app_lock_icon = enabled,
            cx,
        ),
    ];
    if let Some(error) = error {
        rows.push(render_app_lock_error(controls.tokens, controls.i18n, error));
    }

    controls.settings_card("settings_view.general.app_lock_title", "", rows)
}

pub fn render_app_lock_error(tokens: &ThemeTokens, i18n: &I18n, error: String) -> AnyElement {
    let message = if error.starts_with("settings_view.") {
        i18n.t(&error)
    } else {
        error
    };
    div()
        .w_full()
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(rgba((tokens.ui.error << 8) | 0x4d))
        .bg(rgba((tokens.ui.error << 8) | 0x1a))
        .px(px(10.0))
        .py(px(8.0))
        .text_size(px(tokens.metrics.ui_text_sm))
        .text_color(rgb(tokens.ui.error))
        .child(message)
        .into_any_element()
}
