use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, px, rgb};
use oxideterm_theme::ThemeTokens;

pub fn form_field(tokens: &ThemeTokens, label: String, input: impl IntoElement) -> AnyElement {
    let theme = tokens.ui;
    div()
        .flex()
        .flex_col()
        .gap(px(tokens.metrics.modal_field_gap))
        .child(
            div()
                .text_size(px(tokens.metrics.form_label_font_size))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgb(theme.text))
                .child(label),
        )
        .child(input)
        .into_any_element()
}
