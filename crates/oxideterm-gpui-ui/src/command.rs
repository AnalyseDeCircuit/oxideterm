use gpui::{Div, IntoElement, ParentElement, Styled, TextAlign, div, prelude::*, px, rgb};
use oxideterm_theme::ThemeTokens;

pub fn command(tokens: &ThemeTokens) -> Div {
    div()
        .flex()
        .h_full()
        .w_full()
        .flex_col()
        .overflow_hidden()
        .rounded(px(tokens.radii.md))
        .bg(rgb(tokens.ui.bg))
        .text_color(rgb(tokens.ui.text))
}

pub fn command_input(tokens: &ThemeTokens, input: impl IntoElement) -> Div {
    div()
        .flex()
        .items_center()
        .flex_1()
        .min_w_0()
        .child(
            div()
                .mr(px(tokens.spacing.two))
                .size(px(tokens.metrics.ui_menu_icon_size))
                .text_color(rgb(tokens.ui.text_muted))
                .child("⌕"),
        )
        .child(
            div()
                .h(px(tokens.metrics.ui_command_input_height))
                .w_full()
                .flex()
                .items_center()
                .child(input),
        )
}

pub fn command_list(tokens: &ThemeTokens) -> Div {
    div()
        .max_h(px(tokens.metrics.ui_command_list_max_height))
        .overflow_hidden()
}

pub fn command_empty(tokens: &ThemeTokens, label: impl Into<String>) -> Div {
    div()
        .py(px(tokens.spacing.three * 2.0))
        .text_align(TextAlign::Center)
        .text_size(px(tokens.metrics.ui_text_sm))
        .text_color(rgb(tokens.ui.text_muted))
        .child(label.into())
}

pub fn command_group(tokens: &ThemeTokens, heading: Option<String>) -> Div {
    div()
        .overflow_hidden()
        .py(px(tokens.spacing.one))
        .when_some(heading, |group, heading| {
            group.child(
                div()
                    .px(px(tokens.spacing.three))
                    .py(px(tokens.spacing.one + tokens.spacing.one / 2.0))
                    .text_size(px(tokens.metrics.ui_text_xs))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(tokens.ui.text_muted))
                    .child(heading),
            )
        })
}

pub fn command_item(tokens: &ThemeTokens, selected: bool, label: impl IntoElement) -> Div {
    div()
        .relative()
        .flex()
        .cursor_pointer()
        .items_center()
        .gap(px(tokens.spacing.two + tokens.spacing.one / 2.0))
        .px(px(tokens.spacing.three))
        .py(px(tokens.spacing.one + tokens.spacing.one / 2.0))
        .text_size(px(tokens.metrics.ui_text_sm))
        .text_color(rgb(if selected {
            tokens.ui.accent
        } else {
            tokens.ui.text
        }))
        .bg(if selected {
            gpui::rgba((tokens.ui.accent << 8) | 0x26)
        } else {
            gpui::rgba(0x00000000)
        })
        .child(label)
}

pub fn command_separator(tokens: &ThemeTokens) -> Div {
    div()
        .mx(px(-tokens.metrics.ui_menu_padding))
        .h(px(1.0))
        .bg(rgb(tokens.ui.border))
}

pub fn command_shortcut(tokens: &ThemeTokens, shortcut: impl Into<String>) -> Div {
    div()
        .ml_auto()
        .flex_none()
        .text_size(px(tokens.metrics.ui_text_xs))
        .text_color(rgb(tokens.ui.text_muted))
        .child(shortcut.into())
}
