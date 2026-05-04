use gpui::{Div, ParentElement, Styled, div, prelude::*, px, rgb, rgba};
use oxideterm_theme::ThemeTokens;

pub(crate) struct TextInputView<'a> {
    pub value: &'a str,
    pub placeholder: String,
    pub focused: bool,
    pub caret_visible: bool,
    pub secret: bool,
    pub selected_all: bool,
}

pub(crate) fn text_input(tokens: &ThemeTokens, view: TextInputView<'_>) -> Div {
    let theme = tokens.ui;
    let empty = view.value.is_empty();
    let display = if empty {
        view.placeholder
    } else if view.secret {
        "•".repeat(view.value.chars().count())
    } else {
        view.value.to_string()
    };
    let show_selection = view.focused && view.selected_all && !empty;

    div()
        .h(px(tokens.metrics.ui_control_height))
        .px(px(tokens.metrics.ui_control_padding_x))
        .flex()
        .items_center()
        .rounded(px(tokens.radii.md))
        .bg(rgba((theme.bg << 8) | 0x80))
        .border_1()
        .border_color(if view.focused {
            rgb(theme.accent)
        } else {
            rgb(theme.border)
        })
        .text_size(px(tokens.metrics.ui_text_sm))
        .text_color(if empty {
            rgb(theme.text_muted)
        } else {
            rgb(theme.text)
        })
        .cursor_pointer()
        .overflow_hidden()
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .when(view.focused && empty, |row| {
                    row.child(text_caret(tokens, view.caret_visible))
                })
                .child(
                    div()
                        .when(show_selection, |text| {
                            text.px(px(tokens.metrics.form_selection_padding_x))
                                .rounded(px(tokens.radii.xs))
                                .bg(rgb(theme.accent))
                                .text_color(rgb(theme.accent_text))
                        })
                        .text_color(if empty {
                            rgb(theme.text_muted)
                        } else if show_selection {
                            rgb(theme.accent_text)
                        } else {
                            rgb(theme.text)
                        })
                        .child(display),
                )
                .when(view.focused && !empty && !show_selection, |row| {
                    row.child(text_caret(tokens, view.caret_visible))
                }),
        )
}

pub(crate) fn text_caret(tokens: &ThemeTokens, visible: bool) -> Div {
    div()
        .w(px(tokens.metrics.form_caret_width))
        .h(px(tokens.metrics.form_caret_height))
        .bg(rgb(tokens.ui.accent))
        .opacity(if visible { 1.0 } else { 0.0 })
}
