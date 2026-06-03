use gpui::{
    Div, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement, Stateful,
    StatefulInteractiveElement, Styled, div, prelude::*, px, rgb,
};
use oxideterm_theme::ThemeTokens;

use crate::modal::rounded_shell_child_radius;

use super::tokens::*;

const AI_INLINE_PANEL_WIDTH: f32 = 520.0; // Tauri AiInlinePanel w-[520px].
const AI_INLINE_LOADING_BAR_HEIGHT: f32 = 2.0; // Tauri h-[2px].
const AI_INLINE_COMMAND_MAX_HEIGHT: f32 = 120.0; // Tauri max-h-[120px].
const AI_INLINE_CURSOR_WIDTH: f32 = 2.0; // Tauri streaming cursor w-[2px].
const AI_INLINE_CURSOR_HEIGHT: f32 = 14.0; // Tauri streaming cursor h-[14px].
const AI_INLINE_KBD_TEXT_SIZE: f32 = 9.0; // Tauri kbd text-[9px].
const AI_INLINE_PRIMARY_TEXT: u32 = 0xffffff; // Tauri primary action text-white.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiInlineNoticeKind {
    Warning,
    Error,
}

pub fn ai_inline_panel_shell(tokens: &ThemeTokens, left: f32, top: f32) -> Div {
    div()
        .absolute()
        .w(px(AI_INLINE_PANEL_WIDTH))
        .left(px(left))
        .top(px(top))
        .overflow_hidden()
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(rgb(tokens.ui.border))
        .bg(rgb(tokens.ui.bg_elevated))
        .shadow_xl()
}

pub fn ai_inline_loading_bar(tokens: &ThemeTokens) -> Div {
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .h(px(AI_INLINE_LOADING_BAR_HEIGHT))
        .overflow_hidden()
        // The loading strip is absolutely positioned against the rounded
        // inline panel edge; own the top corners instead of relying on parent
        // masking to hide square accent pixels.
        .rounded_t(px(rounded_shell_child_radius(tokens.radii.md)))
        // Tauri uses an animated transparent/accent/transparent shimmer. GPUI
        // keeps the same 2px loading affordance; animation belongs in the owner.
        .child(div().h_full().w_full().bg(rgb(tokens.ui.accent)))
}

pub fn ai_inline_input_row(tokens: &ThemeTokens) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(tokens.spacing.two))
        .px(px(tokens.spacing.three))
        .py(px(tokens.spacing.two))
}

pub fn ai_inline_prompt_slot(tokens: &ThemeTokens, input: impl IntoElement) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .text_size(px(tokens.metrics.ui_text_sm))
        .font_family(ai_font_family())
        .text_color(rgb(tokens.ui.text))
        .child(input)
}

pub fn ai_inline_hint_group(tokens: &ThemeTokens) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(tokens.spacing.one + tokens.spacing.one / 2.0))
        .text_size(px(AI_TEXT_10))
        .text_color(rgb(tokens.ui.text_muted))
}

pub fn ai_inline_kbd_hint(
    tokens: &ThemeTokens,
    key: impl Into<String>,
    label: impl Into<String>,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(tokens.spacing.one))
        .child(
            div()
                .rounded(px(tokens.radii.sm))
                .bg(rgb(tokens.ui.bg_hover))
                .px(px(tokens.spacing.one))
                .py(px(tokens.spacing.one / 2.0))
                .text_size(px(AI_INLINE_KBD_TEXT_SIZE))
                .font_family(ai_font_family())
                .child(key.into()),
        )
        .child(label.into())
}

pub fn ai_inline_close_button(tokens: &ThemeTokens, icon: impl IntoElement) -> Div {
    div()
        .p(px(tokens.spacing.one))
        .text_color(rgb(tokens.ui.text_muted))
        .cursor_pointer()
        .hover(|style| style.text_color(rgb(tokens.ui.text)))
        .child(icon)
}

pub fn ai_inline_notice(
    tokens: &ThemeTokens,
    kind: AiInlineNoticeKind,
    icon: impl IntoElement,
    message: impl Into<String>,
) -> Div {
    let tone = match kind {
        AiInlineNoticeKind::Warning => AiTone::Yellow,
        AiInlineNoticeKind::Error => AiTone::Red,
    };
    div()
        .mx(px(tokens.spacing.three))
        .mb(px(tokens.spacing.two))
        .flex()
        .items_center()
        .gap(px(tokens.spacing.two))
        .rounded(px(tokens.radii.sm))
        .border_1()
        .border_color(tone_border(tokens, tone, AI_CHIP_BORDER_ALPHA))
        .bg(tone_bg(tokens, tone, AI_CHIP_BG_ALPHA))
        .px(px(tokens.spacing.two))
        .py(px(tokens.spacing.one + tokens.spacing.one / 2.0))
        .text_size(px(AI_TEXT_12))
        .text_color(rgb(tone_color(tokens, tone)))
        .child(icon)
        .child(div().min_w_0().truncate().child(message.into()))
}

pub fn ai_inline_response_section(tokens: &ThemeTokens) -> Div {
    div()
        .border_t_1()
        .border_color(rgb(tokens.ui.border))
        .flex()
        .flex_col()
}

pub fn ai_inline_command_preview(
    tokens: &ThemeTokens,
    id: impl Into<ElementId>,
    content: impl IntoElement,
    loading_cursor: bool,
) -> Stateful<Div> {
    div()
        .id(id)
        .max_h(px(AI_INLINE_COMMAND_MAX_HEIGHT))
        .overflow_y_scroll()
        .bg(rgb(tokens.ui.bg_sunken))
        .px(px(tokens.spacing.three))
        .py(px(tokens.spacing.two))
        .text_size(px(tokens.metrics.ui_text_sm))
        .font_family(ai_font_family())
        .text_color(rgb(tokens.ui.accent))
        .whitespace_normal()
        .child(content)
        .when(loading_cursor, |preview| {
            preview.child(
                div()
                    .flex_none()
                    .w(px(AI_INLINE_CURSOR_WIDTH))
                    .h(px(AI_INLINE_CURSOR_HEIGHT))
                    .ml(px(tokens.spacing.one / 2.0))
                    .bg(rgb(tokens.ui.accent)),
            )
        })
}

pub fn ai_inline_actions_bar(tokens: &ThemeTokens) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(tokens.spacing.one))
        .border_t_1()
        .border_color(rgb(tokens.ui.border))
        .bg(rgb(tokens.ui.bg_elevated))
        .px(px(tokens.spacing.two))
        .py(px(tokens.spacing.one + tokens.spacing.one / 2.0))
}

pub fn ai_inline_action_button(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    icon: impl IntoElement,
    primary: bool,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(tokens.spacing.one))
        .rounded(px(tokens.radii.sm))
        .px(px(tokens.spacing.two))
        .py(px(tokens.spacing.one))
        .text_size(px(AI_TEXT_11))
        .font_weight(FontWeight::MEDIUM)
        .bg(if primary {
            rgb(tokens.ui.accent)
        } else {
            gpui::rgba(0x00000000)
        })
        .text_color(if primary {
            rgb(AI_INLINE_PRIMARY_TEXT)
        } else {
            rgb(tokens.ui.text)
        })
        .cursor_pointer()
        .hover(|style| {
            if primary {
                style.bg(rgb(tokens.ui.accent_hover))
            } else {
                style.bg(rgb(tokens.ui.bg_hover))
            }
        })
        .child(icon)
        .child(label.into())
}

pub fn ai_inline_model_selector_slot(tokens: &ThemeTokens, selector: impl IntoElement) -> Div {
    div()
        .min_w_0()
        .flex_none()
        .child(selector)
        .text_color(rgb(tokens.ui.text_muted))
}

pub fn ai_inline_icon(tokens: &ThemeTokens, icon: impl IntoElement) -> Div {
    div()
        .flex_none()
        .text_color(rgb(tokens.ui.accent))
        .child(icon)
}
