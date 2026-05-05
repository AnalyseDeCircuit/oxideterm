use gpui::{Div, ParentElement, Styled, div, prelude::*, px, rgb, rgba};
use oxideterm_theme::ThemeTokens;

pub fn tooltip_content(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    shortcut: Option<String>,
) -> Div {
    div()
        .rounded(px(tokens.radii.xs))
        .border_1()
        .border_color(rgb(tokens.ui.border))
        .bg(rgba((tokens.ui.bg_elevated << 8) | 0xf2))
        .px(px(tokens.metrics.ui_tooltip_padding_x))
        .py(px(tokens.metrics.ui_tooltip_padding_y))
        .text_size(px(tokens.metrics.ui_text_xs))
        .text_color(rgb(tokens.ui.text))
        .shadow_lg()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(tokens.spacing.two))
                .child(label.into())
                .when_some(shortcut, |row, shortcut| {
                    row.child(
                        div()
                            .ml_auto()
                            .rounded(px(tokens.radii.xs))
                            .border_1()
                            .border_color(rgb(tokens.ui.border))
                            .bg(rgb(tokens.ui.bg))
                            .px(px(tokens.spacing.one))
                            .py(px(tokens.spacing.one / 2.0))
                            .text_size(px(tokens.metrics.ui_tooltip_shortcut_font_size))
                            .text_color(rgb(tokens.ui.text_muted))
                            .child(shortcut),
                    )
                }),
        )
}
