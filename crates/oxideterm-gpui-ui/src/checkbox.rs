use gpui::{Div, ParentElement, Styled, div, px, rgb};
use oxideterm_theme::ThemeTokens;

pub fn checkbox(tokens: &ThemeTokens, label: String, checked: bool) -> Div {
    let theme = tokens.ui;
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .cursor_pointer()
        .child(
            div()
                .size(px(tokens.metrics.ui_checkbox_size))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(tokens.radii.xs))
                .border_1()
                .border_color(if checked {
                    rgb(theme.accent)
                } else {
                    rgb(theme.border)
                })
                .bg(if checked {
                    rgb(theme.accent)
                } else {
                    rgb(theme.bg)
                })
                .text_size(px(tokens.metrics.ui_checkbox_icon_size))
                .text_color(rgb(theme.accent_text))
                .child(if checked { "✓" } else { "" }),
        )
        .child(
            div()
                .text_size(px(tokens.metrics.ui_text_sm))
                .text_color(rgb(theme.text))
                .child(label),
        )
}
