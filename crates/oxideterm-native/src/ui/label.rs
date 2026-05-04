use gpui::{Div, FontWeight, ParentElement, Styled, div, px, rgb};
use oxideterm_theme::ThemeTokens;

pub(crate) fn label(tokens: &ThemeTokens, text: impl Into<String>) -> Div {
    div()
        .text_size(px(tokens.metrics.ui_text_sm))
        .font_weight(FontWeight::MEDIUM)
        .line_height(px(tokens.metrics.ui_text_sm))
        .text_color(rgb(tokens.ui.text))
        .child(text.into())
}
