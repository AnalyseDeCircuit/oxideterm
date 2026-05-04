use gpui::{Div, ParentElement, Styled, div, px};
use oxideterm_theme::ThemeTokens;

use super::toast::{ToastView, toast};

pub(crate) fn toaster(tokens: &ThemeTokens, toasts: impl IntoIterator<Item = ToastView>) -> Div {
    let mut viewport = div()
        .absolute()
        .right_0()
        .bottom_0()
        .flex()
        .flex_col()
        .gap(px(tokens.spacing.two))
        .p(px(tokens.metrics.ui_toast_padding))
        .max_w(px(tokens.metrics.ui_toast_width));
    for item in toasts {
        viewport = viewport.child(toast(tokens, item));
    }
    viewport
}
