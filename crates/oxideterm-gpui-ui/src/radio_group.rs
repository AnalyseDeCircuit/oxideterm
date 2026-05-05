use gpui::{Div, ParentElement, Styled, div, px, rgb};
use oxideterm_theme::ThemeTokens;

pub fn radio_group(tokens: &ThemeTokens) -> Div {
    div().grid().gap(px(tokens.spacing.two))
}

pub fn radio_group_item(tokens: &ThemeTokens, checked: bool, disabled: bool) -> Div {
    div()
        .size(px(tokens.metrics.ui_radio_size))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1()
        .border_color(rgb(tokens.ui.border))
        .text_color(rgb(tokens.ui.accent))
        .opacity(if disabled { 0.5 } else { 1.0 })
        .child(
            div()
                .size(px(tokens.metrics.ui_radio_dot_size))
                .rounded_full()
                .bg(if checked {
                    rgb(tokens.ui.accent)
                } else {
                    rgb(tokens.ui.bg)
                }),
        )
}
