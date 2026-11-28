use crate::surface::nav::render_icon;
use gpui::{prelude::*, *};
use oxideterm_gpui_ui::button::{
    ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonOptions,
    toolbar_action_button,
};
use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;

pub fn host_row(
    tokens: &ThemeTokens,
    i18n: &I18n,
    alias: String,
    detail: String,
    selected: bool,
    disabled: bool,
    toggle: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    import: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let theme = tokens.ui;
    div()
        .w_full()
        .mb(px(4.0))
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(rgba(0x00000000))
        .bg(rgba(0x00000000))
        .p(px(12.0))
        .opacity(if disabled { 0.5 } else { 1.0 })
        .hover(|style| {
            if disabled {
                style
            } else {
                style
                    .bg(rgb(tokens.ui.bg_hover))
                    .border_color(rgb(tokens.ui.border))
            }
        })
        .child(
            div()
                .cursor_pointer()
                .min_w(px(0.0))
                .flex_1()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.0))
                .child(check(tokens, selected))
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .gap(px(3.0))
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .text_size(px(tokens.metrics.ui_text_sm))
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(rgb(theme.text))
                                        .child(alias.clone()),
                                )
                                .when(disabled, |row| row.child(imported_badge(tokens, i18n))),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_size(px(tokens.metrics.ui_text_xs))
                                .text_color(rgb(theme.text_muted))
                                .child(detail),
                        ),
                )
                .when(!disabled, |left| {
                    left.on_mouse_down(MouseButton::Left, toggle)
                }),
        )
        .child(import_button(tokens, i18n, disabled, import))
        .into_any_element()
}
fn import_button(
    tokens: &ThemeTokens,
    i18n: &I18n,
    disabled: bool,
    import: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    // Host-row import uses the shared outline action path but preserves
    // Tauri's pill shape with a post-primitive radius override.
    toolbar_action_button(
        tokens,
        i18n.t("settings_view.connections.ssh_config.import"),
        Some(render_icon(
            "lucide/folder-input.svg",
            16.0,
            rgb(tokens.ui.text),
        )),
        ToolbarButtonOptions {
            button: ButtonOptions {
                variant: ButtonVariant::Outline,
                size: ButtonSize::Sm,
                radius: ButtonRadius::Md,
                disabled,
            },
            background: Some(rgba(0x00000000)),
            border: Some(rgb(tokens.ui.border)),
            text_color: Some(rgb(tokens.ui.text)),
            hover_background: Some(rgb(tokens.ui.bg_hover)),
            height: Some(34.0),
            padding_x: Some(14.0),
            font_size: Some(tokens.metrics.ui_text_sm),
            ..ToolbarButtonOptions::default()
        },
        import,
    )
    .rounded_full()
    .into_any_element()
}

pub(crate) fn check(tokens: &ThemeTokens, checked: bool) -> AnyElement {
    div()
        .size(px(20.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(tokens.radii.sm))
        .border_1()
        .border_color(rgb(if checked {
            tokens.ui.accent
        } else {
            tokens.ui.text_muted
        }))
        .bg(if checked {
            rgb(tokens.ui.accent)
        } else {
            rgba(0x00000000)
        })
        .text_size(px(12.0))
        .text_color(rgb(tokens.ui.accent_text))
        .child(if checked { "✓" } else { "" })
        .into_any_element()
}

fn imported_badge(tokens: &ThemeTokens, i18n: &I18n) -> AnyElement {
    div()
        .px(px(6.0))
        .py(px(2.0))
        .rounded(px(tokens.radii.sm))
        .bg(rgba((tokens.ui.accent << 8) | 0x20))
        .text_size(px(tokens.metrics.ui_text_2xs))
        .text_color(rgb(tokens.ui.accent_secondary))
        .child(i18n.t("settings_view.connections.ssh_config.already_imported"))
        .into_any_element()
}
