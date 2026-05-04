use gpui::{Div, ParentElement, Styled, div, px, rgb};
use oxideterm_theme::ThemeTokens;

pub(crate) fn segmented_tabs(tokens: &ThemeTokens) -> Div {
    tabs_list(tokens)
}

pub(crate) fn tabs_list(tokens: &ThemeTokens) -> Div {
    div()
        .h(px(tokens.metrics.ui_tabs_list_height))
        .flex()
        .flex_row()
        .items_center()
        .justify_center()
        .p(px(tokens.metrics.ui_tabs_list_padding))
        .rounded(px(tokens.radii.xs))
        .bg(rgb(tokens.ui.bg_panel))
        .text_color(rgb(tokens.ui.text_muted))
}

pub(crate) fn segmented_tab(tokens: &ThemeTokens, label: String, selected: bool) -> Div {
    tabs_trigger(tokens, label, selected)
}

pub(crate) fn tabs_trigger(tokens: &ThemeTokens, label: String, selected: bool) -> Div {
    let theme = tokens.ui;
    div()
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .whitespace_nowrap()
        .cursor_pointer()
        .rounded(px(tokens.radii.xs))
        .px(px(tokens.metrics.ui_tabs_trigger_padding_x))
        .py(px(tokens.metrics.ui_tabs_trigger_padding_y))
        .bg(if selected {
            rgb(theme.bg)
        } else {
            rgb(theme.bg_panel)
        })
        .text_size(px(tokens.metrics.ui_text_sm))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(if selected {
            rgb(theme.text)
        } else {
            rgb(theme.text_muted)
        })
        .child(label)
}

pub(crate) fn tabs_content(tokens: &ThemeTokens, content: impl gpui::IntoElement) -> Div {
    div().mt(px(tokens.spacing.two)).child(content)
}
