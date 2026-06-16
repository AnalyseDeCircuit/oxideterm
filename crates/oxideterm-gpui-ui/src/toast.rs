use gpui::{
    AnyElement, Div, FontWeight, ParentElement, Styled, div, prelude::*, px, rgb, rgba, svg,
};
use oxideterm_theme::ThemeTokens;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToastVariant {
    Default,
    Success,
    Error,
    Warning,
}

pub struct ToastView {
    pub title: String,
    pub description: Option<String>,
    pub status_text: Option<String>,
    pub progress: Option<f32>,
    pub variant: ToastVariant,
    pub close: Option<AnyElement>,
}

fn toast_content_width(tokens: &ThemeTokens) -> f32 {
    // Tauri toast root is 420px wide with p-4 and pr-8. Keep the text column
    // inside that same usable width so GPUI's line wrapper can wrap long text.
    (tokens.metrics.ui_toast_width - tokens.metrics.ui_toast_padding * 3.0).max(0.0)
}

fn toast_text(tokens: &ThemeTokens, text: String) -> Div {
    div()
        .w_full()
        .max_w(px(toast_content_width(tokens)))
        .min_w_0()
        .whitespace_normal()
        .text_size(px(tokens.metrics.ui_text_sm))
        .child(text)
}

pub fn toast(tokens: &ThemeTokens, view: ToastView) -> Div {
    let (border, bg, text) = match view.variant {
        ToastVariant::Default => (tokens.ui.border, tokens.ui.bg_elevated, tokens.ui.text),
        ToastVariant::Success => (tokens.ui.success, tokens.ui.success, tokens.ui.success),
        ToastVariant::Error => (tokens.ui.error, tokens.ui.error, tokens.ui.error),
        ToastVariant::Warning => (tokens.ui.warning, tokens.ui.warning, tokens.ui.warning),
    };
    div()
        .relative()
        .flex()
        .w_full()
        .max_w(px(tokens.metrics.ui_toast_width))
        .items_center()
        .justify_between()
        .overflow_hidden()
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(if view.variant == ToastVariant::Default {
            rgb(border)
        } else {
            rgba((border << 8) | 0x80)
        })
        .bg(if view.variant == ToastVariant::Default {
            rgba((bg << 8) | 0xf2)
        } else {
            rgba((bg << 8) | 0x1a)
        })
        .p(px(tokens.metrics.ui_toast_padding))
        .pr(px(tokens.metrics.ui_toast_padding * 2.0))
        .text_color(rgb(text))
        .shadow_lg()
        .child(
            div()
                .grid()
                .w_full()
                .max_w(px(toast_content_width(tokens)))
                .min_w_0()
                .flex_1()
                .gap(px(tokens.spacing.one))
                .child(toast_text(tokens, view.title).font_weight(FontWeight::SEMIBOLD))
                .when_some(view.description, |content, description| {
                    content.child(toast_text(tokens, description).opacity(0.9))
                })
                .when_some(view.status_text, |content, status_text| {
                    content.child(toast_text(tokens, status_text).opacity(0.9))
                })
                .when_some(view.progress, |content, progress| {
                    content.child(
                        div()
                            .mt(px(tokens.spacing.two))
                            .h(px(tokens.spacing.one))
                            .w_full()
                            .overflow_hidden()
                            .rounded_full()
                            .bg(rgba((tokens.ui.border << 8) | 0x80))
                            .child(
                                div()
                                    .h_full()
                                    .rounded_full()
                                    .bg(rgb(tokens.ui.accent))
                                    .w(gpui::relative((progress / 100.0).clamp(0.0, 1.0))),
                            ),
                    )
                }),
        )
        .when_some(view.close, |toast, close| toast.child(close))
}

pub fn toast_action(tokens: &ThemeTokens, label: impl Into<String>) -> Div {
    div()
        .h(px(tokens.metrics.ui_button_sm_height))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(tokens.radii.sm))
        .border_1()
        .border_color(rgb(tokens.ui.border))
        .bg(rgba(0x00000000))
        .px(px(tokens.metrics.ui_button_sm_padding_x))
        .text_size(px(tokens.metrics.ui_text_sm))
        .font_weight(gpui::FontWeight::MEDIUM)
        .child(label.into())
}

pub fn toast_close(tokens: &ThemeTokens) -> Div {
    let button_size = tokens.metrics.ui_toast_close_size + tokens.spacing.two;

    div()
        .absolute()
        .right(px(tokens.spacing.two))
        .top(px(tokens.spacing.two))
        .flex()
        .size(px(button_size))
        .items_center()
        .justify_center()
        .cursor_pointer()
        .rounded(px(tokens.radii.xs))
        .text_color(rgb(tokens.ui.text_muted))
        .opacity(0.75)
        .hover(|button| button.opacity(1.0).text_color(rgb(tokens.ui.text)))
        // Tauri's Radix ToastClose is an absolute top-right affordance with a
        // 16px X icon and 4px padding. GPUI uses the same visual footprint here.
        .child(
            svg()
                .path("lucide/x.svg")
                .size(px(tokens.metrics.ui_toast_close_size)),
        )
}
