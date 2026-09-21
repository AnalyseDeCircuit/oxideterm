use gpui::{
    Div, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement, ScrollHandle,
    ScrollWheelEvent, SharedString, Stateful, StatefulInteractiveElement, Styled, div, prelude::*,
    px, rgb,
};
use oxideterm_theme::ThemeTokens;

use super::tokens::*;

pub fn ai_tool_block(tokens: &ThemeTokens) -> Div {
    div()
        .my(px(tokens.spacing.one))
        .flex()
        .flex_col()
        .gap(px(tokens.spacing.one))
}

pub fn ai_tool_heading(tokens: &ThemeTokens, label: impl Into<String>) -> Div {
    div()
        .px(px(tokens.spacing.one / 2.0))
        .text_size(px(AI_TEXT_10))
        .font_weight(FontWeight::MEDIUM)
        .text_color(muted_text(tokens, AI_MUTED_TEXT_40_ALPHA))
        .child(label.into())
}

pub fn ai_tool_condensed_toggle(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    icon: impl IntoElement,
    expanded: bool,
) -> Div {
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(px(tokens.spacing.one + tokens.spacing.one / 2.0))
        .rounded(px(tokens.radii.md))
        .px(px(tokens.spacing.two))
        .py(px(tokens.spacing.one))
        .bg(bg_alpha(tokens, tokens.ui.bg_hover, 0x33))
        .text_size(px(AI_TEXT_10))
        .text_color(muted_text(tokens, AI_MUTED_TEXT_40_ALPHA))
        .cursor_pointer()
        .hover(|style| {
            style.bg(bg_alpha(
                tokens,
                tokens.ui.bg_hover,
                AI_CHAT_INPUT_BORDER_ALPHA,
            ))
        })
        .child(icon)
        .child(label.into())
        .when(expanded, |toggle| {
            toggle.border_b_1().border_color(bg_alpha(
                tokens,
                tokens.ui.border,
                AI_CHAT_INPUT_FOOTER_BORDER_ALPHA,
            ))
        })
}

pub fn ai_tool_item(tokens: &ThemeTokens, call: &AiToolCallView) -> Div {
    let pending = call.status == AiToolStatus::PendingApproval;
    let pending_tone = if call.pending_denied_command {
        AiTone::Red
    } else {
        AiTone::Amber
    };
    div()
        .min_w_0()
        .overflow_hidden()
        .rounded(px(tokens.radii.sm))
        // Approval remains distinct; ordinary calls share the message surface.
        .when(pending, |item| {
            item.border_1()
                .border_color(tone_border(tokens, pending_tone, 0x66))
                .bg(tone_bg(tokens, pending_tone, AI_TOOL_BG_ALPHA))
        })
}

pub fn ai_tool_item_header(
    tokens: &ThemeTokens,
    call: &AiToolCallView,
    expanded: bool,
    status_icon: impl IntoElement,
    tool_icon: impl IntoElement,
    chevron_icon: impl IntoElement,
) -> Div {
    div()
        .w_full()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(tokens.spacing.one + tokens.spacing.one / 2.0))
        .px(px(tokens.spacing.one))
        .py(px(tokens.spacing.one))
        .rounded(px(tokens.radii.sm))
        .when(expanded, |header| {
            header.bg(bg_alpha(tokens, tokens.ui.bg_hover, AI_CHIP_BG_ALPHA))
        })
        .text_size(px(AI_TEXT_11))
        .cursor_pointer()
        .hover(|style| style.bg(bg_alpha(tokens, tokens.ui.bg_hover, AI_HOVER_BG_ALPHA)))
        .child(chevron_icon)
        .child(status_icon)
        .child(tool_icon)
        .child(
            div()
                .min_w_0()
                .max_w_full()
                .truncate()
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(tokens.ui.text_muted))
                .child(call.name.clone()),
        )
        .child(ai_tool_badge(
            tokens,
            risk_tone(call.risk),
            call.risk_label.clone(),
        ))
        .when(call.bypass_approval, |header| {
            header.child(ai_tool_badge(
                tokens,
                AiTone::Amber,
                call.bypass_label.clone(),
            ))
        })
        .when_some(call.capability.clone(), |header, capability| {
            header.child(ai_tool_neutral_badge(tokens, capability))
        })
        .child(
            div()
                .ml(px(tokens.spacing.one))
                .min_w_0()
                .flex_1()
                .truncate()
                .text_size(px(AI_TEXT_10))
                .text_color(muted_text(tokens, AI_MUTED_TEXT_50_ALPHA))
                .child(call.summary.clone()),
        )
        .when_some(call.duration.clone(), |header, duration| {
            header.child(
                div()
                    .flex_none()
                    .text_size(px(AI_TEXT_9))
                    .text_color(rgb(tokens.ui.text_muted))
                    .child(duration),
            )
        })
}

pub fn ai_tool_badge(tokens: &ThemeTokens, tone: AiTone, label: impl Into<String>) -> Div {
    div()
        .flex()
        .items_center()
        .flex_none()
        .min_w_0()
        .max_w_full()
        .text_size(px(AI_TEXT_10))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(tone_color(tokens, tone)))
        .child(label.into())
}

pub fn ai_tool_neutral_badge(tokens: &ThemeTokens, label: impl Into<String>) -> Div {
    div()
        .flex()
        .items_center()
        .flex_none()
        .min_w_0()
        .max_w_full()
        .text_size(px(AI_TEXT_9))
        .font_weight(FontWeight::MEDIUM)
        .text_color(muted_text(tokens, AI_MUTED_TEXT_60_ALPHA))
        .child(label.into())
}

pub fn ai_tool_approval_bar(
    tokens: &ThemeTokens,
    warning: impl IntoElement,
    approve: impl IntoElement,
    reject: impl IntoElement,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(tokens.spacing.two))
        .px(px(tokens.spacing.two))
        .py(px(tokens.spacing.one + tokens.spacing.one / 2.0))
        .border_t_1()
        .border_color(bg_alpha(tokens, tokens.ui.border, 0x26))
        .child(warning)
        .child(approve)
        .child(reject)
}

pub fn ai_tool_approval_button(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    approve: bool,
    icon: impl IntoElement,
) -> Div {
    let tone = if approve { AiTone::Green } else { AiTone::Red };
    div()
        .flex()
        .items_center()
        .gap(px(tokens.spacing.one))
        .rounded(px(tokens.radii.md))
        .px(px(tokens.spacing.two))
        .py(px(tokens.spacing.one / 2.0))
        .bg(tone_bg(tokens, tone, AI_TOOL_APPROVAL_BG_ALPHA))
        .text_color(rgb(tone_color(tokens, tone)))
        .text_size(px(AI_TEXT_10))
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .hover(|style| style.bg(tone_bg(tokens, tone, AI_TOOL_APPROVAL_HOVER_ALPHA)))
        .child(icon)
        .child(label.into())
}

pub fn ai_tool_details(tokens: &ThemeTokens) -> Div {
    div()
        .border_t_1()
        .border_color(bg_alpha(tokens, tokens.ui.border, 0x26))
        .px(px(tokens.spacing.two))
        .py(px(tokens.spacing.one + tokens.spacing.one / 2.0))
        .flex()
        .flex_col()
        .gap(px(tokens.spacing.one + tokens.spacing.one / 2.0))
}

pub fn ai_tool_section_label(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    tone: Option<AiTone>,
) -> Div {
    div()
        .mb(px(tokens.spacing.one / 2.0))
        .text_size(px(AI_TEXT_9))
        .font_weight(FontWeight::MEDIUM)
        .text_color(if let Some(tone) = tone {
            bg_alpha(tokens, tone_color(tokens, tone), 0xb3)
        } else {
            muted_text(tokens, AI_MUTED_TEXT_40_ALPHA)
        })
        .child(label.into())
}

pub fn ai_tool_pre(
    tokens: &ThemeTokens,
    id: impl Into<ElementId>,
    content: impl IntoElement,
    max_height: f32,
    mono_font_family: SharedString,
    scroll_handle: &ScrollHandle,
) -> Stateful<Div> {
    div()
        .id(id)
        .max_h(px(max_height))
        .overflow_hidden()
        .track_scroll(scroll_handle)
        .bg(bg_alpha(tokens, tokens.ui.bg, 0x80))
        .px(px(tokens.spacing.one + tokens.spacing.one / 2.0))
        .py(px(tokens.spacing.one))
        .text_size(px(AI_TEXT_10))
        // Tauri maps tool argument/output <pre> blocks to var(--terminal-font-family).
        .font_family(mono_font_family)
        .text_color(muted_text(tokens, AI_MUTED_TEXT_60_ALPHA))
        .whitespace_normal()
        .on_scroll_wheel({
            let scroll_handle = scroll_handle.clone();
            move |event: &ScrollWheelEvent, window, cx| {
                let old_offset = scroll_handle.offset();
                let max_offset = scroll_handle.max_offset();
                let delta = event.delta.pixel_delta(window.line_height());
                let mut next_offset = old_offset;

                if max_offset.x > px(0.0) {
                    next_offset.x = (next_offset.x + delta.x).clamp(-max_offset.x, px(0.0));
                }
                if max_offset.y > px(0.0) {
                    next_offset.y = (next_offset.y + delta.y).clamp(-max_offset.y, px(0.0));
                }

                // Stop chaining only while this payload actually consumes the
                // wheel. At the top/bottom edge, let the AI transcript continue.
                if next_offset != old_offset {
                    scroll_handle.set_offset(next_offset);
                    cx.stop_propagation();
                    window.refresh();
                }
            }
        })
        .child(content)
}

pub fn ai_tool_args_pre(
    tokens: &ThemeTokens,
    id: impl Into<ElementId>,
    content: impl IntoElement,
    mono_font_family: SharedString,
    scroll_handle: &ScrollHandle,
) -> Stateful<Div> {
    ai_tool_pre(
        tokens,
        id,
        content,
        AI_TOOL_ARGS_MAX_HEIGHT,
        mono_font_family,
        scroll_handle,
    )
}

pub fn ai_tool_structured_pre(
    tokens: &ThemeTokens,
    id: impl Into<ElementId>,
    content: impl IntoElement,
    mono_font_family: SharedString,
    scroll_handle: &ScrollHandle,
) -> Stateful<Div> {
    ai_tool_pre(
        tokens,
        id,
        content,
        AI_TOOL_STRUCTURED_MAX_HEIGHT,
        mono_font_family,
        scroll_handle,
    )
}

pub fn ai_tool_output_pre(
    tokens: &ThemeTokens,
    id: impl Into<ElementId>,
    content: impl IntoElement,
    mono_font_family: SharedString,
    scroll_handle: &ScrollHandle,
) -> Stateful<Div> {
    ai_tool_pre(
        tokens,
        id,
        content,
        AI_TOOL_OUTPUT_MAX_HEIGHT,
        mono_font_family,
        scroll_handle,
    )
}
