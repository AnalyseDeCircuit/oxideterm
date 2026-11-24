use gpui::{Div, Styled};
use oxideterm_gpui_ui::text_input::{TextInputView, TextInputViewport, text_input_with_viewport};
use oxideterm_theme::ThemeTokens;

#[cfg(not(target_family = "wasm"))]
mod form;
#[cfg(not(target_family = "wasm"))]
pub(crate) use form::{field_value, replace_text};

pub(crate) fn paint_input(
    tokens: &ThemeTokens,
    view: TextInputView<'_>,
    viewport: &TextInputViewport,
    active_offset: Option<usize>,
) -> Div {
    let focused = view.focused;
    let input = text_input_with_viewport(tokens, view, viewport, active_offset);
    if focused {
        input
    } else {
        input.border_color(gpui::rgba((tokens.ui.border << 8) | 0xb8))
    }
}
