use gpui::{Div, Styled};
use oxideterm_theme::ThemeTokens;

use super::text_input::{TextInputView, text_input};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InputType {
    Text,
    Password,
}

pub(crate) struct InputView<'a> {
    pub value: &'a str,
    pub placeholder: String,
    pub focused: bool,
    pub caret_visible: bool,
    pub input_type: InputType,
    pub selected_all: bool,
    pub disabled: bool,
}

impl<'a> InputView<'a> {
    pub(crate) fn text(value: &'a str, placeholder: String, focused: bool) -> Self {
        Self {
            value,
            placeholder,
            focused,
            caret_visible: false,
            input_type: InputType::Text,
            selected_all: false,
            disabled: false,
        }
    }
}

pub(crate) fn input(tokens: &ThemeTokens, view: InputView<'_>) -> Div {
    text_input(
        tokens,
        TextInputView {
            value: view.value,
            placeholder: view.placeholder,
            focused: view.focused,
            caret_visible: view.caret_visible,
            secret: view.input_type == InputType::Password,
            selected_all: view.selected_all,
        },
    )
    .opacity(if view.disabled { 0.5 } else { 1.0 })
}
