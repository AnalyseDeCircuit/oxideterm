use gpui::{
    Div, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement, Stateful,
    StatefulInteractiveElement, Styled, div, prelude::*, px, rgb, rgba,
};
use oxideterm_theme::ThemeTokens;

use super::tokens::*;

pub fn ai_sidebar_shell(tokens: &ThemeTokens, width: f32, content: impl IntoElement) -> Div {
    div()
        .relative()
        .w(px(width))
        .h_full()
        .flex()
        .flex_col()
        .bg(rgb(tokens.ui.bg))
        .border_l_1()
        .border_color(bg_alpha(tokens, tokens.ui.border, AI_PANEL_BORDER_ALPHA))
        .child(content)
}

pub fn ai_sidebar_header(
    tokens: &ThemeTokens,
    title: impl Into<String>,
    icon: impl IntoElement,
    action: impl IntoElement,
) -> Div {
    div()
        .h(px(AI_SIDEBAR_HEADER_HEIGHT))
        .flex()
        .items_center()
        .justify_between()
        .px(px(tokens.spacing.three))
        .py(px(tokens.spacing.two))
        .border_b_1()
        .border_color(bg_alpha(tokens, tokens.ui.border, AI_HEADER_BORDER_ALPHA))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(tokens.spacing.two))
                .child(icon)
                .child(
                    div()
                        .text_size(px(tokens.metrics.ui_text_sm))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(tokens.ui.text))
                        .child(title.into()),
                ),
        )
        .child(action)
}

pub fn ai_chat_panel(tokens: &ThemeTokens) -> Div {
    div()
        .h_full()
        .flex()
        .flex_col()
        .bg(rgb(tokens.ui.bg))
        .text_color(rgb(tokens.ui.text))
}

pub fn ai_chat_scroll_area(tokens: &ThemeTokens, id: impl Into<ElementId>) -> Stateful<Div> {
    div()
        .id(id)
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .px(px(tokens.spacing.three))
        .py(px(tokens.spacing.two))
}

pub fn ai_header_action_button(tokens: &ThemeTokens, icon: impl IntoElement) -> Div {
    div()
        .size(px(28.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(tokens.radii.md))
        .text_color(rgb(tokens.ui.text_muted))
        .cursor_pointer()
        .hover(|style| style.bg(bg_alpha(tokens, tokens.ui.bg_hover, AI_HOVER_BG_ALPHA)))
        .child(icon)
}

pub fn ai_context_chip(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    tone: AiTone,
    selected: bool,
    icon: impl IntoElement,
) -> Div {
    let border = if selected {
        tone_border(tokens, tone, AI_CHIP_BORDER_ALPHA)
    } else {
        bg_alpha(tokens, tokens.ui.border, AI_HEADER_BORDER_ALPHA)
    };
    let bg = if selected {
        tone_bg(tokens, tone, AI_CHIP_BG_ALPHA)
    } else {
        rgba(0x00000000)
    };
    let text = if selected {
        rgb(tone_color(tokens, tone))
    } else {
        rgb(tokens.ui.text_muted)
    };

    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(tokens.spacing.one))
        .px(px(tokens.spacing.two))
        .py(px(tokens.spacing.one / 2.0))
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(border)
        .bg(bg)
        .text_size(px(AI_TEXT_10))
        .font_weight(FontWeight::BOLD)
        .text_color(text)
        .child(icon)
        .child(label.into())
}

pub fn ai_chat_input_root(tokens: &ThemeTokens) -> Div {
    div()
        .bg(rgb(tokens.ui.bg))
        .border_t_1()
        .border_color(bg_alpha(
            tokens,
            tokens.ui.border,
            AI_CHAT_INPUT_BORDER_ALPHA,
        ))
        .px(px(tokens.spacing.three))
        .py(px(tokens.spacing.two + tokens.spacing.one / 2.0))
}

pub fn ai_chat_input_chips(tokens: &ThemeTokens) -> Div {
    div()
        .mb(px(tokens.spacing.two))
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(tokens.spacing.one + tokens.spacing.one / 2.0))
}

pub fn ai_chat_input_frame(tokens: &ThemeTokens, focused: bool) -> Div {
    div()
        .relative()
        .flex()
        .flex_col()
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(if focused {
            bg_alpha(tokens, tokens.ui.accent, AI_CHAT_INPUT_BORDER_ALPHA)
        } else {
            bg_alpha(tokens, tokens.ui.border, AI_CHAT_INPUT_BORDER_ALPHA)
        })
        .bg(bg_alpha(
            tokens,
            tokens.ui.bg_panel,
            AI_CHAT_INPUT_PANEL_ALPHA,
        ))
        .shadow_sm()
        .when(focused, |frame| frame.shadow_md())
}

pub fn ai_chat_input_editor(tokens: &ThemeTokens, editor: impl IntoElement) -> Div {
    div()
        .w_full()
        .min_h(px(AI_CHAT_INPUT_MIN_HEIGHT))
        .px(px(tokens.spacing.three))
        .py(px(tokens.spacing.two))
        .text_size(px(AI_INPUT_TEXT_SIZE))
        .line_height(px(20.0))
        .text_color(rgb(tokens.ui.text))
        .child(editor)
}

pub fn ai_chat_input_footer(
    tokens: &ThemeTokens,
    leading: impl IntoElement,
    trailing: impl IntoElement,
) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .px(px(tokens.spacing.two))
        .py(px(tokens.spacing.one))
        .border_t_1()
        .border_color(bg_alpha(
            tokens,
            tokens.ui.border,
            AI_CHAT_INPUT_FOOTER_BORDER_ALPHA,
        ))
        .child(leading)
        .child(trailing)
}

pub fn ai_chat_input_status(tokens: &ThemeTokens, label: impl Into<String>, active: bool) -> Div {
    div()
        .flex()
        .min_w_0()
        .items_center()
        .gap(px(tokens.spacing.two))
        .text_size(px(AI_TEXT_9))
        .font_weight(FontWeight::BOLD)
        .text_color(if active {
            rgb(tokens.ui.accent)
        } else {
            muted_text(tokens, AI_MUTED_TEXT_30_ALPHA)
        })
        .child(label.into())
}

pub fn ai_send_button(tokens: &ThemeTokens, label: impl Into<String>, disabled: bool) -> Div {
    div()
        .px(px(tokens.spacing.two + tokens.spacing.one / 2.0))
        .py(px(tokens.spacing.one / 2.0))
        .rounded(px(tokens.radii.md))
        .bg(rgb(tokens.ui.accent))
        .text_color(rgb(tokens.ui.bg))
        .text_size(px(AI_TEXT_10))
        .font_weight(FontWeight::BOLD)
        .opacity(if disabled { 0.2 } else { 1.0 })
        .cursor_pointer()
        .child(label.into())
}

pub fn ai_stop_button(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    icon: impl IntoElement,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(tokens.spacing.one))
        .px(px(tokens.spacing.two))
        .py(px(tokens.spacing.one / 2.0))
        .rounded(px(tokens.radii.md))
        .bg(tone_bg(tokens, AiTone::Red, AI_CHIP_BG_ALPHA))
        .text_color(rgb(AI_TW_RED))
        .text_size(px(AI_TEXT_10))
        .font_weight(FontWeight::BOLD)
        .child(icon)
        .child(label.into())
}

pub fn ai_autocomplete_popup(tokens: &ThemeTokens, id: impl Into<ElementId>) -> Stateful<Div> {
    div()
        .id(id)
        .absolute()
        .left_0()
        .right_0()
        .bottom_full()
        .mb(px(tokens.spacing.one))
        .max_h(px(AI_AUTOCOMPLETE_MAX_HEIGHT))
        .overflow_y_scroll()
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(bg_alpha(tokens, tokens.ui.border, 0x99))
        .bg(rgb(tokens.ui.bg))
        .shadow_lg()
}

pub fn ai_autocomplete_item(
    tokens: &ThemeTokens,
    prefix: impl Into<String>,
    label: impl Into<String>,
    description: impl Into<String>,
    active: bool,
) -> Div {
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(px(tokens.spacing.two))
        .px(px(tokens.spacing.three))
        .py(px(tokens.spacing.one + tokens.spacing.one / 2.0))
        .text_size(px(AI_TEXT_12))
        .text_color(if active {
            rgb(tokens.ui.accent)
        } else {
            rgb(tokens.ui.text)
        })
        .bg(if active {
            bg_alpha(tokens, tokens.ui.accent, 0x26)
        } else {
            rgba(0x00000000)
        })
        .cursor_pointer()
        .child(
            div()
                .flex_none()
                .font_family(ai_font_family())
                .text_color(bg_alpha(tokens, tokens.ui.accent, 0x99))
                .child(format!("{}{}", prefix.into(), label.into())),
        )
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_size(px(AI_TEXT_11))
                .text_color(muted_text(tokens, AI_MUTED_TEXT_50_ALPHA))
                .child(description.into()),
        )
}

pub fn ai_message(tokens: &ThemeTokens, role: AiMessageRole) -> Div {
    div()
        .px(px(tokens.spacing.three))
        .py(px(tokens.spacing.three))
        .child(ai_message_header_row(tokens, role))
}

pub fn ai_message_header_row(tokens: &ThemeTokens, role: AiMessageRole) -> Div {
    div()
        .mb(px(tokens.spacing.one / 2.0))
        .flex()
        .items_center()
        .gap(px(tokens.spacing.one + tokens.spacing.one / 2.0))
        .when(role == AiMessageRole::User, |row| row.flex_row_reverse())
}

pub fn ai_message_author(tokens: &ThemeTokens, label: impl Into<String>) -> Div {
    div()
        .text_size(px(AI_TEXT_11))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(muted_text(tokens, AI_MUTED_TEXT_50_ALPHA))
        .child(label.into())
}

pub fn ai_message_time(tokens: &ThemeTokens, label: impl Into<String>, user: bool) -> Div {
    div()
        .flex_none()
        .text_size(px(AI_TEXT_10))
        .font_family(ai_font_family())
        .text_color(muted_text(tokens, AI_MUTED_TEXT_25_ALPHA))
        .when(user, |time| time.mr_auto())
        .when(!user, |time| time.ml_auto())
        .child(label.into())
}

pub fn ai_message_model_badge(tokens: &ThemeTokens, label: impl Into<String>) -> Div {
    div()
        .max_w(px(180.0))
        .truncate()
        .rounded(px(tokens.radii.sm))
        .border_1()
        .border_color(bg_alpha(
            tokens,
            tokens.ui.border,
            AI_CHAT_INPUT_BORDER_ALPHA,
        ))
        .bg(bg_alpha(
            tokens,
            tokens.ui.bg_panel,
            AI_MODEL_BADGE_BG_ALPHA,
        ))
        .px(px(tokens.spacing.one + tokens.spacing.one / 2.0))
        .py(px(tokens.spacing.one / 2.0))
        .text_size(px(AI_TEXT_10))
        .font_weight(FontWeight::MEDIUM)
        .text_color(muted_text(tokens, 0.45))
        .child(label.into())
}

pub fn ai_message_body(
    tokens: &ThemeTokens,
    role: AiMessageRole,
    content: impl IntoElement,
) -> Div {
    div()
        .mt(px(tokens.spacing.one))
        .when(role == AiMessageRole::User, |body| {
            body.rounded(px(tokens.radii.md))
                .border_1()
                .border_color(bg_alpha(
                    tokens,
                    tokens.ui.accent,
                    AI_USER_BUBBLE_BORDER_ALPHA,
                ))
                .bg(bg_alpha(tokens, tokens.ui.accent, AI_USER_BUBBLE_BG_ALPHA))
                .px(px(tokens.spacing.three))
                .py(px(tokens.spacing.two))
                .shadow_sm()
        })
        .child(content)
}

pub fn ai_message_action(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    icon: impl IntoElement,
    danger: bool,
) -> Div {
    let color = if danger {
        rgb(AI_TW_RED)
    } else {
        muted_text(tokens, AI_MUTED_TEXT_40_ALPHA)
    };
    div()
        .flex()
        .items_center()
        .gap(px(tokens.spacing.one))
        .px(px(tokens.spacing.one + tokens.spacing.one / 2.0))
        .py(px(tokens.spacing.one / 2.0))
        .rounded(px(tokens.radii.md))
        .text_size(px(AI_TEXT_11))
        .text_color(color)
        .cursor_pointer()
        .hover(|style| {
            style.bg(if danger {
                tone_bg(tokens, AiTone::Red, 0x0d)
            } else {
                bg_alpha(tokens, tokens.ui.border, AI_CHAT_INPUT_FOOTER_BORDER_ALPHA)
            })
        })
        .child(icon)
        .child(label.into())
}

pub fn ai_suggestion_chip(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    icon: impl IntoElement,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(tokens.spacing.one))
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(bg_alpha(tokens, tokens.ui.border, AI_HEADER_BORDER_ALPHA))
        .px(px(tokens.spacing.two))
        .py(px(tokens.spacing.one))
        .text_size(px(AI_TEXT_11))
        .text_color(muted_text(tokens, AI_MUTED_TEXT_70_ALPHA))
        .cursor_pointer()
        .hover(|style| {
            style
                .border_color(bg_alpha(
                    tokens,
                    tokens.ui.accent,
                    AI_CHAT_INPUT_BORDER_ALPHA,
                ))
                .bg(bg_alpha(tokens, tokens.ui.accent, 0x0d))
                .text_color(rgb(tokens.ui.accent))
        })
        .child(icon)
        .child(label.into())
}

pub fn ai_thinking_compact(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    brain_icon: impl IntoElement,
    chevron_icon: impl IntoElement,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(tokens.spacing.one + tokens.spacing.one / 2.0))
        .rounded(px(tokens.radii.md))
        .px(px(tokens.spacing.two))
        .py(px(tokens.spacing.one))
        .text_size(px(AI_TEXT_11))
        .text_color(muted_text(tokens, AI_MUTED_TEXT_60_ALPHA))
        .cursor_pointer()
        .hover(|style| {
            style
                .bg(rgb(tokens.ui.bg_sunken))
                .text_color(rgb(tokens.ui.text_muted))
        })
        .child(brain_icon)
        .child(label.into())
        .child(chevron_icon)
}

pub fn ai_thinking_block(tokens: &ThemeTokens, expanded: bool) -> Div {
    div()
        .mb(px(tokens.spacing.three))
        .overflow_hidden()
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(bg_alpha(tokens, tokens.ui.border, 0x33))
        .bg(bg_alpha(tokens, tokens.ui.bg_sunken, 0x80))
        .when(!expanded, |block| block)
}

pub fn ai_thinking_header(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    streaming: bool,
    chevron_icon: impl IntoElement,
    brain_icon: impl IntoElement,
) -> Div {
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(px(tokens.spacing.two))
        .px(px(tokens.spacing.three))
        .py(px(tokens.spacing.one + tokens.spacing.one / 2.0))
        .text_size(px(AI_TEXT_11))
        .font_weight(FontWeight::MEDIUM)
        .text_color(muted_text(tokens, AI_MUTED_TEXT_70_ALPHA))
        .cursor_pointer()
        .hover(|style| {
            style
                .bg(bg_alpha(tokens, tokens.ui.bg_sunken, 0xcc))
                .text_color(rgb(tokens.ui.text_muted))
        })
        .child(chevron_icon)
        .child(brain_icon)
        .child(label.into())
        .when(streaming, |header| {
            header.child(
                div()
                    .ml_auto()
                    .text_size(px(AI_TEXT_10))
                    .font_family(ai_font_family())
                    .text_color(bg_alpha(tokens, tokens.ui.accent, 0x99))
                    .child("..."),
            )
        })
}

pub fn ai_thinking_content(
    tokens: &ThemeTokens,
    id: impl Into<ElementId>,
    content: impl Into<String>,
) -> Stateful<Div> {
    div()
        .id(id)
        .max_h(px(AI_THINKING_MAX_HEIGHT))
        .overflow_y_scroll()
        .px(px(tokens.spacing.three))
        .pb(px(tokens.spacing.three))
        .text_size(px(AI_TEXT_12))
        .line_height(px(20.0))
        .font_family(ai_font_family())
        .text_color(muted_text(tokens, AI_MUTED_TEXT_80_ALPHA))
        .whitespace_normal()
        .child(content.into())
}

pub fn ai_warning_block(
    tokens: &ThemeTokens,
    kind: AiWarningKind,
    message: impl Into<String>,
    icon: impl IntoElement,
) -> Div {
    let tone = if kind == AiWarningKind::Error {
        AiTone::Red
    } else {
        AiTone::Yellow
    };
    div()
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(tone_border(tokens, tone, AI_BLOCK_BORDER_ALPHA))
        .bg(tone_bg(tokens, tone, AI_BLOCK_BG_ALPHA))
        .px(px(tokens.spacing.three))
        .py(px(tokens.spacing.two))
        .text_color(rgb(tone_color(tokens, tone)))
        .child(
            div()
                .flex()
                .items_start()
                .gap(px(tokens.spacing.two))
                .child(icon)
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .text_size(px(AI_TEXT_12))
                        .line_height(px(20.0))
                        .child(message.into()),
                ),
        )
}

pub fn ai_guardrail_block(
    tokens: &ThemeTokens,
    message: impl Into<String>,
    strong: bool,
    icon: impl IntoElement,
) -> Div {
    let tone = if strong { AiTone::Amber } else { AiTone::Muted };
    div()
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(if strong {
            tone_border(tokens, tone, AI_BLOCK_BORDER_ALPHA)
        } else {
            bg_alpha(tokens, tokens.ui.border, AI_BLOCK_BORDER_ALPHA)
        })
        .bg(if strong {
            tone_bg(tokens, tone, AI_BLOCK_BG_ALPHA)
        } else {
            bg_alpha(tokens, tokens.ui.bg, AI_CHAT_INPUT_BORDER_ALPHA)
        })
        .px(px(tokens.spacing.three))
        .py(px(tokens.spacing.two))
        .text_color(if strong {
            rgb(0xfef3c7)
        } else {
            rgb(tokens.ui.text_muted)
        })
        .child(
            div()
                .flex()
                .items_start()
                .gap(px(tokens.spacing.two))
                .child(icon)
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .text_size(px(AI_TEXT_12))
                        .line_height(px(20.0))
                        .text_color(muted_text(tokens, AI_MUTED_TEXT_85_ALPHA))
                        .child(message.into()),
                ),
        )
}

pub fn ai_raw_block(
    tokens: &ThemeTokens,
    id: impl Into<ElementId>,
    max_height: Option<f32>,
    content: impl Into<String>,
) -> Stateful<Div> {
    div()
        .id(id)
        .mt(px(tokens.spacing.two))
        .max_h(px(max_height.unwrap_or(AI_GUARDRAIL_RAW_MAX_HEIGHT)))
        .overflow_x_scroll()
        .overflow_y_scroll()
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(bg_alpha(tokens, tokens.ui.border, 0x26))
        .bg(bg_alpha(tokens, tokens.ui.bg, AI_PRE_BG_ALPHA))
        .px(px(tokens.spacing.two))
        .py(px(tokens.spacing.one + tokens.spacing.one / 2.0))
        .text_size(px(AI_TEXT_10))
        .font_family(ai_font_family())
        .text_color(muted_text(tokens, 0.65))
        .whitespace_normal()
        .child(content.into())
}
