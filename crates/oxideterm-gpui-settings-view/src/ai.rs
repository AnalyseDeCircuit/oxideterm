// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Presentational builders for AI settings.
//!
//! These helpers own reusable GPUI layout and visual treatment. The app crate
//! supplies translated labels, controls, and mouse handlers so workspace state
//! and async side effects do not leak into this view crate.

use gpui::{
    AnyElement, CursorStyle, IntoElement, ParentElement, Rgba, SharedString, Styled, div,
    prelude::*, px, rgb, rgba,
};
use oxideterm_ai::ContextWindowSource;
use oxideterm_theme::ThemeTokens;

const AI_TOOL_POLICY_DIVIDER_ALPHA: u32 = 0x4d;
const AI_CONTEXT_PROVIDER_ROW_BORDER_ALPHA: u32 = 0x4d;
const AI_CONTEXT_PROVIDER_ROW_TOP_BORDER_ALPHA: u32 = 0x33;
const AI_CONTEXT_PROVIDER_HOVER_ALPHA: u32 = 0x66;
const AI_CONTEXT_USER_OVERRIDE_BG_ALPHA: u32 = 0x0d;
const AI_CONTEXT_SOURCE_BADGE_TEXT_SIZE: f32 = 9.0;
const AI_CONTEXT_SOURCE_BADGE_PX: f32 = 6.0;
const AI_CONTEXT_SOURCE_BADGE_PY: f32 = 2.0;
const AI_CONTEXT_RESET_SLOT_W: f32 = 16.0;
const AI_CONTEXT_SOURCE_USER_COLOR: u32 = 0x60a5fa;
const AI_CONTEXT_SOURCE_API_COLOR: u32 = 0x34d399;
const AI_CONTEXT_SOURCE_NAME_COLOR: u32 = 0x22d3ee;
const AI_CONTEXT_SOURCE_BADGE_BG_ALPHA: u32 = 0x1a;
const AI_CONTEXT_SOURCE_DEFAULT_TEXT_ALPHA: u32 = 0xb3;
const AI_CONTEXT_SOURCE_DEFAULT_BG_ALPHA: u32 = 0x33;

pub fn settings_ai_tool_number_input_row(tokens: &ThemeTokens, row: AnyElement) -> AnyElement {
    // The expanded tool-use section already owns the containing surface, so
    // numeric controls remain plain rows instead of introducing another card.
    div()
        .w_full()
        .min_w(px(0.0))
        .pb(px(16.0))
        .border_b_1()
        .border_color(rgba((tokens.ui.border << 8) | AI_TOOL_POLICY_DIVIDER_ALPHA))
        .child(row)
        .into_any_element()
}

pub fn settings_ai_context_select_field(
    tokens: &ThemeTokens,
    label: String,
    control: AnyElement,
    hint: String,
) -> AnyElement {
    // Context select rows are pure form layout; the select trigger remains
    // app-owned because it is anchored to WorkspaceApp select state.
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(tokens.metrics.ui_text_sm))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgb(tokens.ui.text))
                .child(label),
        )
        .child(control)
        .child(
            div()
                .text_size(px(tokens.metrics.ui_text_xs))
                .text_color(rgb(tokens.ui.text_muted))
                .child(hint),
        )
        .into_any_element()
}

pub fn settings_ai_context_controls_section(
    _max_width: f32,
    title: AnyElement,
    fields: Vec<AnyElement>,
    sources: AnyElement,
) -> AnyElement {
    // Context controls are a two-column form followed by source toggles. The
    // controls themselves stay app-owned because select/list state lives there.
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(16.0))
        .child(title)
        .child(div().grid().grid_cols(2).gap(px(24.0)).children(fields))
        .child(sources)
        .into_any_element()
}

pub fn settings_ai_editor_summary_section(
    tokens: &ThemeTokens,
    title: AnyElement,
    description: String,
    option_row: Option<AnyElement>,
    status: String,
    edit_action: AnyElement,
) -> AnyElement {
    // Long-form text stays outside the settings list. This compact summary
    // keeps the card scannable while the app owns modal state and persistence.
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(16.0))
        .child(title)
        .child(
            div()
                .text_size(px(tokens.metrics.ui_text_xs))
                .text_color(rgb(tokens.ui.text_muted))
                .child(description),
        )
        .children(option_row)
        .child(
            div()
                .w_full()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(16.0))
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .text_size(px(tokens.metrics.ui_text_xs))
                        .text_color(rgb(tokens.ui.text_muted))
                        .child(status),
                )
                .child(edit_action),
        )
        .into_any_element()
}

pub fn settings_ai_reasoning_section(
    global_default: AnyElement,
    model_overrides: AnyElement,
    max_response_tokens: AnyElement,
) -> AnyElement {
    // Reasoning controls form one policy card, separate from prompt content
    // and context-window sizing so each card has one clear responsibility.
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(16.0))
        .child(global_default)
        .child(model_overrides)
        .child(max_response_tokens)
        .into_any_element()
}

pub fn settings_ai_icon_heading(icon: AnyElement, title: AnyElement) -> AnyElement {
    // Icon headings are shared mini-section headers. The icon comes from the
    // app so this crate does not need an icon mapping dependency.
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(icon)
        .child(title)
        .into_any_element()
}

pub fn settings_ai_tool_use_section(
    _max_width: f32,
    heading: AnyElement,
    expand_button: AnyElement,
    enabled_row: AnyElement,
    collapsed_summary: Option<AnyElement>,
    expanded_body: Option<AnyElement>,
) -> AnyElement {
    // The tool-use section shell owns header/body ordering. Policy controls
    // and settings writes stay in the app and settings-model crates.
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(16.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .child(heading)
                .child(expand_button),
        )
        .child(enabled_row)
        .children(collapsed_summary)
        .children(expanded_body)
        .into_any_element()
}

pub fn settings_ai_context_sources_group(
    tokens: &ThemeTokens,
    title: String,
    rows: Vec<AnyElement>,
) -> AnyElement {
    // Source toggles share one small titled group under the context selectors.
    div()
        .w_full()
        .min_w(px(0.0))
        .mt(px(8.0))
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .text_size(px(tokens.metrics.ui_text_xs))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgb(tokens.ui.text_muted))
                .child(title.to_uppercase()),
        )
        .children(rows)
        .into_any_element()
}

pub fn settings_ai_context_source_row(
    tokens: &ThemeTokens,
    label: String,
    hint: String,
    checkbox: AnyElement,
) -> gpui::Div {
    // The row chrome is reusable, while caller-provided handlers keep settings
    // mutation and event propagation in the app crate.
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .items_center()
        .gap(px(12.0))
        .cursor_pointer()
        .child(checkbox)
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(tokens.metrics.ui_text_sm))
                        .text_color(rgb(tokens.ui.text))
                        .child(label),
                )
                .child(
                    div()
                        .text_size(px(tokens.metrics.ui_text_xs))
                        .text_color(rgb(tokens.ui.text_muted))
                        .child(hint),
                ),
        )
}

pub fn settings_ai_global_reasoning_section(
    tokens: &ThemeTokens,
    title: String,
    control: AnyElement,
    hint: String,
    _max_width: f32,
) -> AnyElement {
    // The global reasoning section is a simple labeled select. Option content
    // and writes stay app/model-owned.
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .mb(px(8.0))
                .text_size(px(tokens.metrics.ui_text_sm))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgb(tokens.ui.text))
                .child(title.to_uppercase()),
        )
        .child(control)
        .child(
            div()
                .text_size(px(tokens.metrics.ui_text_xs))
                .text_color(rgb(tokens.ui.text_muted))
                .child(hint),
        )
        .into_any_element()
}

pub fn settings_ai_textarea_surface(
    tokens: &ThemeTokens,
    min_height: f32,
    focused: bool,
    display_value: &str,
    placeholder: &str,
    marked_text: Option<String>,
    caret: Option<AnyElement>,
) -> gpui::Div {
    let theme = tokens.ui;
    let mut textarea = div()
        .w_full()
        .min_w(px(0.0))
        .min_h(px(min_height))
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(if focused {
            rgba((theme.accent << 8) | 0x66)
        } else {
            rgb(theme.border)
        })
        .bg(rgb(theme.bg))
        .px(px(12.0))
        .py(px(8.0))
        .flex()
        .flex_col()
        .items_start()
        .gap(px(2.0))
        .cursor(CursorStyle::IBeam)
        .text_size(px(tokens.metrics.ui_text_sm))
        .line_height(px(20.0))
        .text_color(rgb(theme.text));

    // Textarea rendering is display-only here; app code still owns editing,
    // IME selection, and anchor updates.
    if display_value.is_empty() {
        for line in placeholder.split('\n') {
            textarea = textarea.child(
                div()
                    .min_h(px(20.0))
                    .text_color(rgba((theme.text_muted << 8) | 0x66))
                    .child(line.to_string()),
            );
        }
    } else {
        for line in display_value.split('\n') {
            textarea = textarea.child(div().min_h(px(20.0)).child(line.to_string()));
        }
    }

    if let Some(marked_text) = marked_text {
        textarea = textarea.child(
            div()
                .underline()
                .text_color(rgb(theme.text))
                .child(marked_text),
        );
    }
    if let Some(caret) = caret {
        textarea = textarea.child(caret);
    }
    textarea
}

pub fn settings_ai_textarea_row(
    tokens: &ThemeTokens,
    label: String,
    control: AnyElement,
    hint: String,
) -> AnyElement {
    // Label and hint layout is presentational; the control can be a GPUI anchor
    // probe or any other app-owned multiline input.
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .when(!label.is_empty(), |row| {
            row.child(
                div()
                    .text_size(px(tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(tokens.ui.text))
                    .child(label),
            )
        })
        .child(control)
        .child(
            div()
                .text_size(px(tokens.metrics.ui_text_xs))
                .text_color(rgb(tokens.ui.text_muted))
                .line_height(px(18.0))
                .child(hint),
        )
        .into_any_element()
}

pub fn settings_ai_section_heading(
    tokens: &ThemeTokens,
    title: String,
    hint: String,
) -> AnyElement {
    // Section headings are pure copy layout; the app supplies translated text.
    div()
        .w_full()
        .min_w(px(0.0))
        .flex_1()
        .child(
            div()
                .text_size(px(tokens.metrics.ui_text_sm))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgb(tokens.ui.text))
                .whitespace_nowrap()
                .child(title.to_uppercase()),
        )
        .child(
            div()
                .mt(px(4.0))
                .text_size(px(tokens.metrics.ui_text_xs))
                .text_color(rgb(tokens.ui.text_muted))
                .child(hint),
        )
        .into_any_element()
}

pub fn settings_ai_collapsible_header(
    tokens: &ThemeTokens,
    title: String,
    summary: String,
    chevron: AnyElement,
) -> gpui::Div {
    // Collapsible header owns the Radix-like trigger chrome. The app attaches
    // click handling because expansion state is part of WorkspaceApp.
    div()
        .mb(px(16.0))
        .w_full()
        .rounded(px(tokens.radii.md))
        .px(px(4.0))
        .py(px(4.0))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .text_color(rgb(tokens.ui.text_muted))
        .cursor_pointer()
        .hover(|style| style.bg(rgba((tokens.ui.bg_hover << 8) | 0x80)))
        .child(
            div()
                // Match the Tauri full-width trigger: the text column must take
                // remaining space or CJK titles collapse to one glyph per line.
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(tokens.metrics.ui_text_sm))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(tokens.ui.text))
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(tokens.metrics.ui_text_xs))
                        .text_color(rgb(tokens.ui.text_muted))
                        .child(summary),
                ),
        )
        .child(chevron)
}

pub fn settings_ai_model_reasoning_header(
    tokens: &ThemeTokens,
    title: String,
    hint: String,
    chevron: AnyElement,
) -> gpui::Div {
    // Reasoning override header uses the compact hoverable trigger from the
    // React settings card. The app attaches the expansion toggle.
    div()
        .w_full()
        .min_w(px(0.0))
        .mb(px(12.0))
        .rounded(px(tokens.radii.md))
        .px(px(4.0))
        .py(px(4.0))
        .flex()
        .items_start()
        .justify_between()
        .gap(px(12.0))
        .text_color(rgb(tokens.ui.text_muted))
        .cursor_pointer()
        .hover(|style| {
            style
                .bg(rgba(
                    (tokens.ui.bg_hover << 8) | AI_CONTEXT_PROVIDER_HOVER_ALPHA,
                ))
                .text_color(rgb(tokens.ui.text))
        })
        .child(
            div()
                .min_w(px(0.0))
                .flex_1()
                .child(
                    div()
                        .text_size(px(tokens.metrics.ui_text_xs))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(tokens.ui.text))
                        .child(title.to_uppercase()),
                )
                .child(
                    div()
                        .mt(px(4.0))
                        .text_size(px(tokens.metrics.ui_text_xs))
                        .text_color(rgb(tokens.ui.text_muted))
                        .child(hint),
                ),
        )
        .child(div().mt(px(2.0)).child(chevron))
}

pub fn settings_ai_context_windows_header(
    tokens: &ThemeTokens,
    title: String,
    hint: String,
    chevron: AnyElement,
    _max_width: f32,
) -> gpui::Div {
    // Context-window header is wider and less card-like than the reasoning
    // override trigger, matching the original page hierarchy.
    div()
        .mb(px(16.0))
        .w_full()
        .min_w(px(0.0))
        .flex()
        .items_start()
        .justify_between()
        .gap(px(12.0))
        .text_color(rgb(tokens.ui.text_muted))
        .cursor_pointer()
        .child(
            div()
                .min_w(px(0.0))
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .text_size(px(tokens.metrics.ui_text_sm))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(tokens.ui.text))
                        .child(title.to_uppercase()),
                )
                .child(
                    div()
                        .text_size(px(tokens.metrics.ui_text_xs))
                        .text_color(rgb(tokens.ui.text_muted))
                        .child(hint),
                ),
        )
        .child(div().mt(px(2.0)).child(chevron))
}

pub fn settings_ai_model_empty_text(tokens: &ThemeTokens, label: String) -> AnyElement {
    // Empty model groups use muted italic copy rather than a full empty-state.
    div()
        .text_size(px(tokens.metrics.ui_text_xs))
        .text_color(rgb(tokens.ui.text_muted))
        .italic()
        .child(label)
        .into_any_element()
}

pub fn settings_ai_model_provider_header(
    tokens: &ThemeTokens,
    provider_name: String,
    summary: String,
    chevron: AnyElement,
) -> gpui::Div {
    // Provider headers are compact table group toggles. The actual provider id
    // and expansion mutation remain in the app state.
    div()
        .w_full()
        .min_w(px(0.0))
        .mb(px(4.0))
        .rounded(px(tokens.radii.sm))
        .px(px(4.0))
        .py(px(4.0))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .cursor_pointer()
        .text_size(px(10.0))
        .font_weight(gpui::FontWeight::BOLD)
        .text_color(rgb(tokens.ui.text_muted))
        .child(
            div()
                .min_w(px(0.0))
                .flex_1()
                .child(provider_name.to_uppercase()),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(summary)
                .child(chevron),
        )
        .hover(|style| {
            style
                .bg(rgba(
                    (tokens.ui.bg_hover << 8) | AI_CONTEXT_PROVIDER_HOVER_ALPHA,
                ))
                .text_color(rgb(tokens.ui.text))
        })
}

pub fn settings_ai_model_provider_section(
    provider_header: gpui::Div,
    rows: Option<AnyElement>,
) -> AnyElement {
    // A provider section owns only vertical grouping; row virtualization is
    // supplied by the app because it owns ListState caches.
    let section = div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(provider_header);
    if let Some(rows) = rows {
        section.child(rows).into_any_element()
    } else {
        section.into_any_element()
    }
}

pub fn settings_ai_model_row_list_frame(
    tokens: &ThemeTokens,
    list_height: f32,
    rows: AnyElement,
) -> AnyElement {
    // Virtualized model rows need a fixed-height bordered frame so GPUI's list
    // measurements do not resize the surrounding settings layout.
    div()
        .w_full()
        .min_w(px(0.0))
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(rgba(
            (tokens.ui.border << 8) | AI_CONTEXT_PROVIDER_ROW_BORDER_ALPHA,
        ))
        .overflow_hidden()
        .h(px(list_height))
        .child(rows)
        .into_any_element()
}

pub fn settings_ai_model_reasoning_row(
    tokens: &ThemeTokens,
    mono_font_family: SharedString,
    model: String,
    select: AnyElement,
    is_first: bool,
) -> AnyElement {
    // The select is caller-provided because it is backed by app-local select
    // anchors; this helper owns the stable row layout and typography.
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .px(px(12.0))
        .py(px(6.0))
        .when(!is_first, |row| {
            row.border_t_1().border_color(rgba(
                (tokens.ui.border << 8) | AI_CONTEXT_PROVIDER_ROW_TOP_BORDER_ALPHA,
            ))
        })
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .text_size(px(tokens.metrics.ui_text_xs))
                .font_family(mono_font_family)
                .text_color(rgb(tokens.ui.text_muted))
                .overflow_hidden()
                .child(model),
        )
        .child(select)
        .into_any_element()
}

pub fn settings_ai_context_window_row(
    tokens: &ThemeTokens,
    mono_font_family: SharedString,
    model: String,
    source_badge: AnyElement,
    input: AnyElement,
    reset: Option<AnyElement>,
    has_override: bool,
    is_first: bool,
) -> AnyElement {
    // Context rows expose current source, editable override, and an optional
    // reset action. App code still owns text input focus and reset mutation.
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .items_center()
        .gap(px(8.0))
        .px(px(12.0))
        .py(px(6.0))
        .bg(if has_override {
            rgba((tokens.ui.accent << 8) | AI_CONTEXT_USER_OVERRIDE_BG_ALPHA)
        } else {
            rgba((tokens.ui.bg << 8) | 0x00)
        })
        .when(!is_first, |row| {
            row.border_t_1().border_color(rgba(
                (tokens.ui.border << 8) | AI_CONTEXT_PROVIDER_ROW_TOP_BORDER_ALPHA,
            ))
        })
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .text_size(px(tokens.metrics.ui_text_xs))
                .font_family(mono_font_family)
                .text_color(rgb(tokens.ui.text_muted))
                .truncate()
                .child(model),
        )
        .child(div().flex_none().child(source_badge))
        .child(div().flex_none().child(input))
        .child(
            div()
                .w(px(AI_CONTEXT_RESET_SLOT_W))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .children(reset),
        )
        .into_any_element()
}

pub fn settings_ai_context_source_badge(
    tokens: &ThemeTokens,
    label: String,
    text_color: Rgba,
    bg_color: Rgba,
) -> AnyElement {
    // Badge colors come from the app/model source classification; this helper
    // owns the tiny pill geometry.
    div()
        .flex_none()
        .rounded(px(tokens.radii.sm))
        .px(px(AI_CONTEXT_SOURCE_BADGE_PX))
        .py(px(AI_CONTEXT_SOURCE_BADGE_PY))
        .text_size(px(AI_CONTEXT_SOURCE_BADGE_TEXT_SIZE))
        .font_weight(gpui::FontWeight::MEDIUM)
        .whitespace_nowrap()
        .text_color(text_color)
        .bg(bg_color)
        .child(label)
        .into_any_element()
}

pub fn settings_ai_context_source_badge_for_source(
    tokens: &ThemeTokens,
    label: String,
    source: ContextWindowSource,
) -> AnyElement {
    let (text_color, bg_color) = settings_ai_context_source_badge_colors(tokens, source);
    settings_ai_context_source_badge(tokens, label, text_color, bg_color)
}

fn settings_ai_context_source_badge_colors(
    tokens: &ThemeTokens,
    source: ContextWindowSource,
) -> (Rgba, Rgba) {
    match source {
        ContextWindowSource::User => (
            rgb(AI_CONTEXT_SOURCE_USER_COLOR),
            rgba((AI_CONTEXT_SOURCE_USER_COLOR << 8) | AI_CONTEXT_SOURCE_BADGE_BG_ALPHA),
        ),
        ContextWindowSource::Api => (
            rgb(AI_CONTEXT_SOURCE_API_COLOR),
            rgba((AI_CONTEXT_SOURCE_API_COLOR << 8) | AI_CONTEXT_SOURCE_BADGE_BG_ALPHA),
        ),
        ContextWindowSource::Name => (
            rgb(AI_CONTEXT_SOURCE_NAME_COLOR),
            rgba((AI_CONTEXT_SOURCE_NAME_COLOR << 8) | AI_CONTEXT_SOURCE_BADGE_BG_ALPHA),
        ),
        ContextWindowSource::Pattern | ContextWindowSource::Default => (
            rgba((tokens.ui.text_muted << 8) | AI_CONTEXT_SOURCE_DEFAULT_TEXT_ALPHA),
            rgba((tokens.ui.border << 8) | AI_CONTEXT_SOURCE_DEFAULT_BG_ALPHA),
        ),
    }
}

pub fn settings_ai_active_model_max_response_tokens_row(
    tokens: &ThemeTokens,
    title: String,
    hint: String,
    model_label: String,
    input: AnyElement,
    mono_font_family: SharedString,
) -> AnyElement {
    // Active-model token override keeps its input outside the view crate but
    // no longer needs custom layout in the app monolith.
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(tokens.metrics.ui_text_sm))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgb(tokens.ui.text))
                .child(title),
        )
        .child(
            div()
                .text_size(px(tokens.metrics.ui_text_xs))
                .text_color(rgb(tokens.ui.text_muted))
                .child(hint),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(12.0))
                .child(
                    div()
                        .text_size(px(tokens.metrics.ui_text_xs))
                        .text_color(rgb(tokens.ui.text_muted))
                        .font_family(mono_font_family)
                        .child(model_label),
                )
                .child(input),
        )
        .into_any_element()
}

pub fn settings_ai_tool_collapsed_summary(tokens: &ThemeTokens, summary: String) -> AnyElement {
    // Collapsed policy copy mirrors Tauri's left-rule nested summary.
    div()
        .ml(px(16.0))
        .pl(px(16.0))
        .border_l_1()
        .border_color(rgba((tokens.ui.border << 8) | 0x4d))
        .text_size(px(tokens.metrics.ui_text_xs))
        .text_color(rgb(tokens.ui.text_muted))
        .child(summary)
        .into_any_element()
}

pub fn settings_ai_tool_expanded_body(
    tokens: &ThemeTokens,
    enabled: bool,
    children: Vec<AnyElement>,
) -> AnyElement {
    // Expanded policy content is visually nested under the enable switch. The
    // disabled opacity is display-only; app state still controls interaction.
    div()
        .ml(px(16.0))
        .pl(px(16.0))
        .border_l_1()
        .border_color(rgba((tokens.ui.border << 8) | 0x4d))
        .flex()
        .flex_col()
        .gap(px(20.0))
        .opacity(if enabled { 1.0 } else { 0.4 })
        .children(children)
        .into_any_element()
}

pub fn settings_ai_tool_policy_grid(groups: Vec<AnyElement>) -> AnyElement {
    // Policy categories share one vertical reading order so related approval
    // controls do not jump between columns at different viewport widths.
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(24.0))
        .children(groups)
        .into_any_element()
}

pub fn settings_ai_tool_policy_item(
    tokens: &ThemeTokens,
    label: String,
    control: AnyElement,
) -> AnyElement {
    // Policy items are divider rows inside the section-level surface. The
    // caller owns locked/checked behavior so mutations stay outside this crate.
    div()
        .w_full()
        .min_w(px(0.0))
        .py(px(10.0))
        .border_b_1()
        .border_color(rgba((tokens.ui.border << 8) | AI_TOOL_POLICY_DIVIDER_ALPHA))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .text_size(px(tokens.metrics.ui_text_xs))
        .text_color(rgb(tokens.ui.text_muted))
        .child(div().min_w(px(0.0)).flex_1().truncate().child(label))
        .child(div().flex_none().child(control))
        .into_any_element()
}

pub fn settings_ai_tool_policy_group(
    tokens: &ThemeTokens,
    title: String,
    description: String,
    items: Vec<AnyElement>,
) -> AnyElement {
    // The parent tool-use section owns the surface chrome. Groups contribute
    // only semantic headings and divider rows, avoiding nested card layers.
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .child(
            div()
                .text_size(px(tokens.metrics.ui_text_sm))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgb(tokens.ui.text))
                .child(title),
        )
        .child(
            div()
                .mt(px(4.0))
                .text_size(px(tokens.metrics.ui_text_xs))
                .line_height(px(18.0))
                .text_color(rgb(tokens.ui.text_muted))
                .child(description),
        )
        .child(div().mt(px(8.0)).flex().flex_col().children(items))
        .into_any_element()
}

pub fn settings_ai_disabled_tools_notice(
    tokens: &ThemeTokens,
    label: String,
    action: AnyElement,
) -> AnyElement {
    // The restore action is caller-provided so the app can clear disabled tools
    // through its normal settings edit boundary.
    div()
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(rgba((tokens.ui.warning << 8) | 0x33))
        .bg(rgba((tokens.ui.warning << 8) | 0x1a))
        .p(px(12.0))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .child(
            div()
                .text_size(px(tokens.metrics.ui_text_xs))
                .text_color(rgb(tokens.ui.warning))
                .child(label),
        )
        .child(action)
        .into_any_element()
}

pub fn settings_ai_policy_warning(tokens: &ThemeTokens, label: String) -> AnyElement {
    // Static warning copy shares the warning surface treatment with disabled
    // tools, without owning any policy state.
    div()
        .rounded(px(tokens.radii.sm))
        .border_1()
        .border_color(rgba((tokens.ui.warning << 8) | 0x33))
        .bg(rgba((tokens.ui.warning << 8) | 0x1a))
        .p(px(12.0))
        .text_size(px(tokens.metrics.ui_text_xs))
        .line_height(px(18.0))
        .text_color(rgb(tokens.ui.warning))
        .child(label)
        .into_any_element()
}
