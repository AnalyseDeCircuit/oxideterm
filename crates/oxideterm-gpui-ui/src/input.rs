use std::ops::Range;

use gpui::{CursorStyle, Div, Styled};
use oxideterm_theme::ThemeTokens;

use super::text_input::{TextInputView, text_input};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputType {
    Text,
    Password,
}

pub struct InputView<'a> {
    pub value: &'a str,
    pub placeholder: String,
    pub focused: bool,
    pub caret_visible: bool,
    pub input_type: InputType,
    pub selected_all: bool,
    pub selected_range: Option<Range<usize>>,
    pub marked_text: Option<&'a str>,
    pub disabled: bool,
}

impl<'a> InputView<'a> {
    pub fn text(value: &'a str, placeholder: String, focused: bool) -> Self {
        Self {
            value,
            placeholder,
            focused,
            caret_visible: false,
            input_type: InputType::Text,
            selected_all: false,
            selected_range: None,
            marked_text: None,
            disabled: false,
        }
    }
}

pub fn input(tokens: &ThemeTokens, view: InputView<'_>) -> Div {
    let disabled = view.disabled;
    text_input(
        tokens,
        TextInputView {
            value: view.value,
            placeholder: view.placeholder,
            // Browser disabled inputs cannot become the text owner. Keep the
            // shared primitive from drawing a caret/focus ring even if feature
            // state is stale for one frame after a disabled transition.
            focused: input_effective_focus(view.focused, disabled),
            caret_visible: input_effective_focus(view.caret_visible, disabled),
            secret: view.input_type == InputType::Password,
            selected_all: view.selected_all && !disabled,
            selected_range: view.selected_range,
            marked_text: view.marked_text,
        },
    )
    .opacity(if disabled { 0.5 } else { 1.0 })
    .cursor(if disabled {
        CursorStyle::OperationNotAllowed
    } else {
        CursorStyle::IBeam
    })
}

fn input_effective_focus(focused: bool, disabled: bool) -> bool {
    focused && !disabled
}

#[cfg(test)]
mod tests {
    use super::input_effective_focus;

    #[test]
    fn disabled_input_does_not_expose_browser_text_focus() {
        assert!(!input_effective_focus(true, true));
        assert!(!input_effective_focus(false, true));
        assert!(input_effective_focus(true, false));
    }
}
