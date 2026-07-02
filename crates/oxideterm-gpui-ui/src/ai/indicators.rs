use gpui::{
    Div, FontWeight, InteractiveElement, IntoElement, ParentElement, Styled, div, prelude::*, px,
    relative, rgb, rgba,
};
use oxideterm_theme::ThemeTokens;

use super::tokens::*;

pub fn ai_status_indicator(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    icon: impl IntoElement,
    active: bool,
) -> Div {
    div()
        .flex()
        .flex_none()
        .max_w(px(AI_STATUS_INDICATOR_MAX_WIDTH))
        .min_w_0()
        .overflow_hidden()
        .items_center()
        .gap(px(tokens.spacing.one))
        .rounded(px(tokens.radii.md))
        .px(px(tokens.spacing.one))
        .py(px(tokens.spacing.one / 2.0))
        .text_size(px(AI_TEXT_10))
        .font_weight(FontWeight::MEDIUM)
        .text_color(if active {
            rgb(tokens.ui.text)
        } else {
            rgb(tokens.ui.text_muted)
        })
        .opacity(if active { 1.0 } else { 0.7 })
        .cursor_pointer()
        .hover(|style| {
            style
                .bg(bg_alpha(tokens, tokens.ui.accent, AI_CHIP_BG_ALPHA))
                .text_color(rgb(tokens.ui.text))
        })
        .child(div().flex_none().child(icon))
        .child(div().min_w_0().truncate().child(label.into()))
}

pub fn ai_safety_indicator(
    tokens: &ThemeTokens,
    mode: AiSafetyMode,
    label: impl Into<String>,
    icon: impl IntoElement,
) -> Div {
    let bypass = mode == AiSafetyMode::Bypass;
    div()
        .flex()
        .flex_none()
        .max_w(px(AI_SAFETY_INDICATOR_MAX_WIDTH))
        .min_w_0()
        .overflow_hidden()
        .items_center()
        .gap(px(tokens.spacing.one))
        .rounded(px(tokens.radii.md))
        .px(px(tokens.spacing.one))
        .py(px(tokens.spacing.one / 2.0))
        .text_size(px(AI_TEXT_10))
        .font_weight(FontWeight::MEDIUM)
        .text_color(if bypass {
            rgb(AI_TW_AMBER)
        } else {
            rgb(tokens.ui.text_muted)
        })
        .when(bypass, |indicator| {
            indicator
                .border_1()
                .border_color(tone_border(tokens, AiTone::Amber, AI_CHIP_BORDER_ALPHA))
                .bg(tone_bg(tokens, AiTone::Amber, AI_CHIP_BG_ALPHA))
        })
        .when(!bypass, |indicator| {
            indicator.hover(|style| {
                style
                    .bg(bg_alpha(tokens, tokens.ui.accent, AI_CHIP_BG_ALPHA))
                    .text_color(rgb(tokens.ui.text))
            })
        })
        .child(div().flex_none().child(icon))
        .child(div().min_w_0().truncate().child(label.into()))
}

pub fn ai_profile_button(tokens: &ThemeTokens, icon: impl IntoElement) -> Div {
    div()
        .size(px(28.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1()
        .border_color(bg_alpha(
            tokens,
            tokens.ui.border,
            AI_CHAT_INPUT_BORDER_ALPHA,
        ))
        .bg(bg_alpha(tokens, tokens.ui.bg_card, 0x99))
        .text_color(bg_alpha(tokens, tokens.ui.accent, 0xcc))
        .cursor_pointer()
        .hover(|style| {
            style
                .bg(bg_alpha(tokens, tokens.ui.bg_hover, 0xb3))
                .text_color(rgb(tokens.ui.accent))
        })
        .child(icon)
}

pub fn ai_context_usage_indicator(
    tokens: &ThemeTokens,
    usage: AiContextUsage,
    label: impl Into<String>,
    icon: impl IntoElement,
) -> Div {
    let tone = if usage.danger {
        AiTone::Red
    } else if usage.warning {
        AiTone::Amber
    } else {
        AiTone::Accent
    };
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(tokens.spacing.two))
        .cursor_pointer()
        .text_color(if usage.danger || usage.warning {
            rgb(tone_color(tokens, tone))
        } else {
            rgb(tokens.ui.text_muted)
        })
        .child(icon)
        .child(
            div()
                .w(px(AI_CONTEXT_MINI_BAR_WIDTH))
                .h(px(AI_CONTEXT_MINI_BAR_HEIGHT))
                .overflow_hidden()
                .rounded_full()
                .bg(bg_alpha(tokens, tokens.ui.border, AI_CONTEXT_BAR_BG_ALPHA))
                .child(
                    div()
                        .h_full()
                        .rounded_full()
                        .bg(rgb(tone_color(tokens, tone)))
                        .w(relative((usage.percentage / 100.0).clamp(0.0, 1.0))),
                ),
        )
        .child(
            div()
                .text_size(px(AI_TEXT_9))
                .font_family(ai_font_family())
                .opacity(0.6)
                .child(label.into()),
        )
}

pub fn ai_context_popover(tokens: &ThemeTokens) -> Div {
    div()
        .w(px(AI_CONTEXT_POPOVER_WIDTH))
        .overflow_hidden()
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(bg_alpha(tokens, tokens.ui.border, AI_HEADER_BORDER_ALPHA))
        .bg(rgb(tokens.ui.bg_panel))
        .shadow_lg()
        // Keep wheel input local to the popover, matching browser popover
        // scroll chaining rules even when the compact panel has no scrollbar.
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
}

pub fn ai_context_popover_header(
    tokens: &ThemeTokens,
    title: impl Into<String>,
    usage: AiContextUsage,
    value_label: impl Into<String>,
) -> Div {
    let tone = if usage.danger {
        AiTone::Red
    } else if usage.warning {
        AiTone::Amber
    } else {
        AiTone::Accent
    };
    div()
        .px(px(tokens.spacing.three))
        .pt(px(tokens.spacing.three))
        .pb(px(tokens.spacing.two))
        .child(
            div()
                .mb(px(tokens.spacing.one / 2.0))
                .text_size(px(AI_TEXT_11))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(tokens.ui.text))
                .child(title.into()),
        )
        .child(
            div()
                .mb(px(tokens.spacing.one + tokens.spacing.one / 2.0))
                .flex()
                .items_baseline()
                .justify_between()
                .child(
                    div()
                        .text_size(px(AI_TEXT_12))
                        .font_family(ai_font_family())
                        .text_color(rgb(tokens.ui.text))
                        .child(value_label.into()),
                )
                .child(
                    div()
                        .text_size(px(AI_TEXT_11))
                        .font_family(ai_font_family())
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(tone_color(tokens, tone)))
                        .child(format!("{}%", usage.percentage.round() as i64)),
                ),
        )
        .child(
            div()
                .h(px(AI_CONTEXT_MINI_BAR_HEIGHT))
                .w_full()
                .overflow_hidden()
                .rounded_full()
                .bg(bg_alpha(tokens, tokens.ui.border, AI_CONTEXT_BAR_BG_ALPHA))
                .child(
                    div()
                        .h_full()
                        .rounded_full()
                        .bg(rgb(tone_color(tokens, tone)))
                        .w(relative((usage.percentage / 100.0).clamp(0.0, 1.0))),
                ),
        )
}

pub fn ai_model_selector_trigger(
    tokens: &ThemeTokens,
    provider_label: impl Into<String>,
    model_label: impl Into<String>,
    icon: impl IntoElement,
    chevron: impl IntoElement,
    ready: bool,
) -> Div {
    div()
        .flex()
        .min_w_0()
        .items_center()
        .gap(px(tokens.spacing.two))
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(bg_alpha(
            tokens,
            tokens.ui.border,
            AI_CHAT_INPUT_BORDER_ALPHA,
        ))
        .bg(bg_alpha(tokens, tokens.ui.bg_card, 0x99))
        .px(px(tokens.spacing.two))
        .py(px(tokens.spacing.one))
        .cursor_pointer()
        .child(icon)
        .child(
            div()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(tokens.spacing.one / 2.0))
                .child(
                    div()
                        .truncate()
                        .text_size(px(AI_TEXT_10))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(tokens.ui.text))
                        .child(provider_label.into()),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(px(AI_TEXT_9))
                        .text_color(muted_text(tokens, AI_MUTED_TEXT_60_ALPHA))
                        .child(model_label.into()),
                ),
        )
        .child(div().size(px(6.0)).rounded_full().bg(if ready {
            rgb(AI_TW_GREEN)
        } else {
            rgb(tokens.ui.text_muted)
        }))
        .child(chevron)
}

pub fn ai_model_selector_panel(tokens: &ThemeTokens, up: bool) -> Div {
    div()
        .absolute()
        .left_0()
        .right_0()
        .when(up, |panel| panel.bottom_full().mb(px(tokens.spacing.one)))
        .when(!up, |panel| panel.top_full().mt(px(tokens.spacing.one)))
        .overflow_hidden()
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(bg_alpha(
            tokens,
            tokens.ui.border,
            AI_CHAT_INPUT_BORDER_ALPHA,
        ))
        .bg(rgb(tokens.ui.bg_panel))
        .shadow_lg()
}

pub fn ai_model_selector_row(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    detail: impl Into<String>,
    selected: bool,
    icon: impl IntoElement,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(tokens.spacing.two))
        .px(px(tokens.spacing.three))
        .py(px(tokens.spacing.two))
        .bg(if selected {
            bg_alpha(tokens, tokens.ui.accent, 0x26)
        } else {
            rgba(0x00000000)
        })
        .text_color(if selected {
            rgb(tokens.ui.accent)
        } else {
            rgb(tokens.ui.text)
        })
        .cursor_pointer()
        .child(icon)
        .child(
            div()
                .min_w_0()
                .flex_1()
                .child(
                    div()
                        .truncate()
                        .text_size(px(AI_TEXT_12))
                        .font_weight(FontWeight::MEDIUM)
                        .child(label.into()),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(px(AI_TEXT_10))
                        .text_color(muted_text(tokens, AI_MUTED_TEXT_60_ALPHA))
                        .child(detail.into()),
                ),
        )
}
