use gpui::{
    AnyElement, Div, IntoElement, MouseButton, ParentElement, Rgba, Styled, div, prelude::*, px,
    rgb, rgba,
};
use oxideterm_theme::ThemeTokens;

const TW_BLACK: u32 = 0x000000;
const DIALOG_BACKDROP_ALPHA: u32 = 0x99; // Tauri DialogOverlay bg-black/60.
const QUICKLOOK_BACKDROP_ALPHA: u32 = 0xcc; // Tauri QuickLook bg-black/80.

pub fn dialog_backdrop_color() -> Rgba {
    rgba((TW_BLACK << 8) | DIALOG_BACKDROP_ALPHA)
}

pub fn quicklook_backdrop_color() -> Rgba {
    rgba((TW_BLACK << 8) | QUICKLOOK_BACKDROP_ALPHA)
}

pub fn modal_overlay(tokens: &ThemeTokens, dialog: impl IntoElement) -> AnyElement {
    dialog_overlay(tokens, dialog)
}

pub fn dialog_overlay(_tokens: &ThemeTokens, dialog: impl IntoElement) -> AnyElement {
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(dialog_backdrop_color())
        .occlude()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
        .child(dialog)
        .into_any_element()
}

pub fn modal_container(tokens: &ThemeTokens) -> Div {
    dialog_content(tokens)
}

pub fn dialog_content(tokens: &ThemeTokens) -> Div {
    let theme = tokens.ui;
    div()
        .w(px(tokens.metrics.modal_width))
        .rounded(px(tokens.radii.md))
        .overflow_hidden()
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.bg_elevated))
}

pub fn modal_header(tokens: &ThemeTokens, title: String, subtitle: String) -> AnyElement {
    dialog_header(tokens)
        .child(dialog_title(tokens, title))
        .child(dialog_description(tokens, subtitle))
        .into_any_element()
}

pub fn dialog_header(tokens: &ThemeTokens) -> Div {
    let theme = tokens.ui;
    div()
        .flex()
        .flex_col()
        .flex_none()
        .justify_center()
        .px(px(tokens.metrics.modal_header_padding_x))
        .py(px(tokens.metrics.modal_header_padding_y))
        .bg(rgb(theme.bg_panel))
        .border_b_1()
        .border_color(rgb(theme.border))
}

pub fn dialog_title(tokens: &ThemeTokens, title: String) -> Div {
    div()
        .text_size(px(tokens.metrics.ui_text_sm))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .line_height(px(tokens.metrics.ui_text_sm))
        .text_color(rgb(tokens.ui.text_heading))
        .child(title)
}

pub fn dialog_description(tokens: &ThemeTokens, description: String) -> Div {
    div()
        .mt(px(tokens.spacing.one + tokens.spacing.one / 2.0))
        .text_size(px(tokens.metrics.ui_text_sm))
        .text_color(rgb(tokens.ui.text_muted))
        .child(description)
}

pub fn modal_body(tokens: &ThemeTokens) -> Div {
    div()
        .p(px(tokens.metrics.modal_body_padding))
        .flex()
        .flex_col()
        .gap(px(tokens.metrics.modal_body_gap))
}

pub fn modal_footer(tokens: &ThemeTokens) -> Div {
    dialog_footer(tokens)
}

pub fn dialog_footer(tokens: &ThemeTokens) -> Div {
    let theme = tokens.ui;
    div()
        .h(px(tokens.metrics.modal_footer_height))
        .px(px(tokens.metrics.modal_footer_padding_x))
        .flex()
        .flex_row()
        .items_center()
        .justify_end()
        .gap_2()
        .border_t_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.bg_panel))
}
