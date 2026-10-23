use gpui::{
    Div, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled, div,
    px, relative, rgb, rgba, svg,
};
use oxideterm_theme::ThemeTokens;

use crate::{
    button::{IconButtonOptions, icon_button},
    tooltip::tooltip_view,
};

pub fn search_panel(tokens: &ThemeTokens) -> Div {
    div()
        .absolute()
        .top(px(12.0))
        .right(px(12.0))
        .w(px(480.0))
        .max_w(relative(0.92))
        .flex()
        .flex_col()
        .overflow_hidden()
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(rgba((tokens.ui.border << 8) | 0xcc))
        .bg(rgba((tokens.ui.bg_elevated << 8) | 0xf5))
        .shadow_lg()
        .occlude()
        .text_size(px(tokens.metrics.ui_text_sm))
        .text_color(rgb(tokens.ui.text))
        .font_family(tokens.metrics.font_family)
}

pub fn search_row() -> Div {
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(8.0))
        .px(px(12.0))
        .py(px(8.0))
}

pub fn search_input_height(line_count: usize, line_height: f32) -> f32 {
    (line_height * line_count.clamp(1, 4) as f32).max(28.0)
}

pub fn search_mode_button(tokens: &ThemeTokens, text: &'static str, selected: bool) -> Div {
    let accent = tokens.ui.accent;
    icon_button(
        tokens,
        div()
            .text_color(rgb(if selected {
                accent
            } else {
                tokens.ui.text_muted
            }))
            .child(text)
            .into_any_element(),
        IconButtonOptions {
            background: selected.then_some(rgba((accent << 8) | 0x33)),
            hover_background: selected.then_some(rgba((accent << 8) | 0x4d)),
            border: Some(if selected {
                rgb(accent)
            } else {
                rgba(0x00000000)
            }),
            ..IconButtonOptions::compact(24.0)
        },
    )
    .flex_none()
}

pub fn search_mode_group() -> Div {
    div().flex().items_center().flex_none().gap(px(4.0))
}

pub fn search_action_button(
    tokens: &ThemeTokens,
    id: impl Into<gpui::ElementId>,
    path: &'static str,
    label: String,
    disabled: bool,
) -> gpui::Stateful<Div> {
    let tokens = *tokens;
    icon_button(
        &tokens,
        svg()
            .path(path)
            .size(px(14.0))
            .text_color(rgb(tokens.ui.text_muted))
            .into_any_element(),
        IconButtonOptions {
            disabled,
            ..IconButtonOptions::compact(28.0)
        },
    )
    .id(id)
    .flex_none()
    .aria_label(label.clone())
    .tooltip(move |_, cx| tooltip_view(tokens, label.clone(), None, cx))
}
