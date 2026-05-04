use gpui::{Div, ParentElement, Styled, div, px, rgb, rgba};
use oxideterm_theme::ThemeTokens;

pub(crate) fn select_trigger(
    tokens: &ThemeTokens,
    value: impl Into<String>,
    placeholder: bool,
    disabled: bool,
) -> Div {
    div()
        .h(px(tokens.metrics.ui_control_height))
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(rgba((tokens.ui.border << 8) | 0x80))
        .bg(rgba((tokens.ui.bg << 8) | 0x80))
        .px(px(tokens.metrics.ui_control_padding_x))
        .text_size(px(tokens.metrics.ui_text_sm))
        .text_color(rgb(if placeholder {
            tokens.ui.text_muted
        } else {
            tokens.ui.text
        }))
        .opacity(if disabled { 0.5 } else { 1.0 })
        .child(div().flex_1().overflow_hidden().child(value.into()))
        .child(
            div()
                .ml(px(tokens.spacing.two))
                .text_color(rgb(tokens.ui.text_muted))
                .opacity(0.4)
                .child("⌄"),
        )
}

pub(crate) fn select_content(tokens: &ThemeTokens) -> Div {
    div()
        .relative()
        .max_h(px(tokens.metrics.ui_select_max_height))
        .min_w(px(tokens.metrics.ui_select_min_width))
        .overflow_hidden()
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(rgb(tokens.ui.border))
        .bg(rgba((tokens.ui.bg_elevated << 8) | 0xf2))
        .text_color(rgb(tokens.ui.text))
        .shadow_lg()
        .child(div().p(px(tokens.metrics.ui_menu_padding)))
}

pub(crate) fn select_item(tokens: &ThemeTokens, label: impl Into<String>, selected: bool) -> Div {
    div()
        .relative()
        .flex()
        .w_full()
        .items_center()
        .rounded(px(tokens.radii.xs))
        .py(px(tokens.metrics.ui_menu_item_padding_y))
        .pl(px(tokens.metrics.ui_menu_item_padding_x))
        .pr(px(tokens.metrics.ui_menu_inset_padding_left))
        .text_size(px(tokens.metrics.ui_text_sm))
        .text_color(rgb(tokens.ui.text))
        .child(
            div()
                .absolute()
                .right(px(tokens.metrics.ui_menu_item_padding_x))
                .size(px(tokens.metrics.ui_menu_icon_size))
                .flex()
                .items_center()
                .justify_center()
                .child(if selected { "✓" } else { "" }),
        )
        .child(label.into())
}

pub(crate) fn select_label(tokens: &ThemeTokens, label: impl Into<String>) -> Div {
    div()
        .px(px(tokens.metrics.ui_menu_item_padding_x))
        .py(px(tokens.metrics.ui_menu_item_padding_y))
        .text_size(px(tokens.metrics.ui_text_sm))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .child(label.into())
}

pub(crate) fn select_separator(tokens: &ThemeTokens) -> Div {
    div()
        .mx(px(-tokens.metrics.ui_menu_padding))
        .my(px(tokens.metrics.ui_menu_padding))
        .h(px(1.0))
        .bg(rgb(tokens.ui.border))
}
