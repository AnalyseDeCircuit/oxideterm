use gpui::{Div, ParentElement, Styled, div, prelude::FluentBuilder as _, px, rgb, rgba};
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
        .border_color(if checked {
            rgb(tokens.ui.accent)
        } else {
            // Keep unchecked rings crisp against both card surfaces and elevated dialogs.
            rgba((tokens.ui.text_muted << 8) | 0x73)
        })
        .text_color(rgb(tokens.ui.accent))
        .opacity(if disabled { 0.5 } else { 1.0 })
        .when(checked, |item| {
            item.child(
                div()
                    .size(px(tokens.metrics.ui_radio_dot_size))
                    .rounded_full()
                    .bg(rgb(tokens.ui.accent)),
            )
        })
}
