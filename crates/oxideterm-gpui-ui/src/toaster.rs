use gpui::{Div, ParentElement, Styled, div, px};
use oxideterm_theme::ThemeTokens;

use super::toast::{ToastView, toast};

pub fn toaster(tokens: &ThemeTokens, toasts: impl IntoIterator<Item = ToastView>) -> Div {
    let mut viewport = div()
        .absolute()
        .right_0()
        .bottom_0()
        .flex()
        .flex_col()
        .gap(px(tokens.spacing.two))
        .p(px(tokens.metrics.ui_toast_padding))
        // Tauri ToastViewport is `w-full md:max-w-[420px]`, and each toast is
        // `w-full`; the native viewport needs an explicit stack width so short
        // messages do not shrink to different pill sizes.
        .w(px(tokens.metrics.ui_toast_width))
        .max_w(px(tokens.metrics.ui_toast_width));
    for item in toasts {
        viewport = viewport.child(toast(tokens, item));
    }
    viewport
}
