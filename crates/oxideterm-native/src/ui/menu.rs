use gpui::{Div, IntoElement, ParentElement, Styled, div, prelude::*, px, rgb, rgba};
use oxideterm_theme::ThemeTokens;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MenuItemKind {
    Plain,
    Checkbox(bool),
    Radio(bool),
    Submenu,
}

pub(crate) fn menu_content(tokens: &ThemeTokens) -> Div {
    div()
        .min_w(px(tokens.metrics.ui_menu_min_width))
        .overflow_hidden()
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(rgb(tokens.ui.border))
        .bg(rgba((tokens.ui.bg_elevated << 8) | 0xf2))
        .p(px(tokens.metrics.ui_menu_padding))
        .text_color(rgb(tokens.ui.text))
        .shadow_lg()
}

pub(crate) fn menu_item(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    kind: MenuItemKind,
    inset: bool,
    disabled: bool,
) -> Div {
    let left_padding =
        if inset || matches!(kind, MenuItemKind::Checkbox(_) | MenuItemKind::Radio(_)) {
            tokens.metrics.ui_menu_inset_padding_left
        } else {
            tokens.metrics.ui_menu_item_padding_x
        };
    div()
        .relative()
        .flex()
        .items_center()
        .rounded(px(tokens.radii.xs))
        .px(px(tokens.metrics.ui_menu_item_padding_x))
        .pl(px(left_padding))
        .py(px(tokens.metrics.ui_menu_item_padding_y))
        .text_size(px(tokens.metrics.ui_text_sm))
        .text_color(rgb(tokens.ui.text))
        .opacity(if disabled { 0.5 } else { 1.0 })
        .cursor_pointer()
        .when(
            matches!(
                kind,
                MenuItemKind::Checkbox(true) | MenuItemKind::Radio(true)
            ),
            |item| {
                item.child(
                    div()
                        .absolute()
                        .left(px(tokens.metrics.ui_menu_item_padding_x))
                        .size(px(tokens.metrics.ui_menu_indicator_size))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(if matches!(kind, MenuItemKind::Radio(true)) {
                            "●"
                        } else {
                            "✓"
                        }),
                )
            },
        )
        .child(label.into())
        .when(kind == MenuItemKind::Submenu, |item| {
            item.child(
                div()
                    .ml_auto()
                    .text_size(px(tokens.metrics.ui_text_sm))
                    .child("›"),
            )
        })
}

pub(crate) fn menu_label(tokens: &ThemeTokens, label: impl Into<String>, inset: bool) -> Div {
    div()
        .px(px(tokens.metrics.ui_menu_item_padding_x))
        .pl(px(if inset {
            tokens.metrics.ui_menu_inset_padding_left
        } else {
            tokens.metrics.ui_menu_item_padding_x
        }))
        .py(px(tokens.metrics.ui_menu_item_padding_y))
        .text_size(px(tokens.metrics.ui_text_sm))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(rgb(tokens.ui.text_muted))
        .child(label.into())
}

pub(crate) fn menu_separator(tokens: &ThemeTokens) -> Div {
    div()
        .mx(px(-tokens.metrics.ui_menu_padding))
        .my(px(tokens.metrics.ui_menu_padding))
        .h(px(1.0))
        .bg(rgb(tokens.ui.border))
}

pub(crate) fn menu_shortcut(tokens: &ThemeTokens, shortcut: impl Into<String>) -> Div {
    div()
        .ml_auto()
        .text_size(px(tokens.metrics.ui_text_xs))
        .opacity(0.6)
        .text_color(rgb(tokens.ui.text_muted))
        .child(shortcut.into())
}

pub(crate) fn menu_item_with_shortcut(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    shortcut: impl IntoElement,
) -> Div {
    menu_item(tokens, label, MenuItemKind::Plain, false, false).child(shortcut)
}
