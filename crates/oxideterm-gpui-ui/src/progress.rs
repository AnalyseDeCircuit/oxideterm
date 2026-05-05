use gpui::{Div, ParentElement, Styled, div, prelude::*, px, relative, rgb};
use oxideterm_theme::ThemeTokens;

pub fn progress(tokens: &ThemeTokens, value: Option<f32>, indeterminate: bool) -> Div {
    let pct = value.unwrap_or(0.0).clamp(0.0, 100.0);
    div()
        .relative()
        .h(px(tokens.metrics.ui_progress_height))
        .w_full()
        .overflow_hidden()
        .rounded_full()
        .bg(rgb(tokens.ui.bg_panel))
        .border_1()
        .border_color(rgb(tokens.ui.border))
        .child(
            div()
                .h_full()
                .rounded_full()
                .bg(rgb(tokens.ui.accent))
                .when(indeterminate, |bar| bar.w_1_3())
                .when(!indeterminate, |bar| bar.w(relative(pct / 100.0))),
        )
}
