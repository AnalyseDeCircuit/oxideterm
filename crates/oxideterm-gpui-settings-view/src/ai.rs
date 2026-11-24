// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! AI settings pages, dialogs, and their shared controls.

use gpui::{
    AnyElement, CursorStyle, IntoElement, ParentElement, Rgba, SharedString, Styled, div,
    prelude::*, px, rgb, rgba,
};
#[cfg(not(target_family = "wasm"))]
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
const AI_TEXTAREA_PADDING_X: f32 = 12.0;
const AI_TEXTAREA_PADDING_Y: f32 = 8.0;
const AI_TEXTAREA_LINE_HEIGHT: f32 = 20.0;

fn settings_ai_textarea_shows_placeholder(display_value: &str, marked_text: Option<&str>) -> bool {
    display_value.is_empty() && marked_text.is_none_or(str::is_empty)
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
        .cursor_pointer()
        .child(crate::settings_row(
            tokens,
            div().child(label).into_any_element(),
            Some(div().child(hint).into_any_element()),
            checkbox,
        ))
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
    let mut marked_text = marked_text.filter(|text| !text.is_empty());
    let show_placeholder =
        settings_ai_textarea_shows_placeholder(display_value, marked_text.as_deref());
    let mut caret = caret;
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
        .px(px(AI_TEXTAREA_PADDING_X))
        .py(px(AI_TEXTAREA_PADDING_Y))
        .relative()
        .flex()
        .flex_col()
        .items_start()
        .gap(px(2.0))
        .cursor(CursorStyle::IBeam)
        .text_size(px(tokens.metrics.ui_text_sm))
        .line_height(px(AI_TEXTAREA_LINE_HEIGHT))
        .text_color(rgb(theme.text));

    // Textarea rendering is display-only here; app code still owns editing,
    // IME selection, and anchor updates.
    if show_placeholder {
        // Placeholder copy is visual guidance, not an editable document line.
        // Absolute positioning keeps it out of caret and selection geometry.
        let mut placeholder_layer = div()
            .absolute()
            .top(px(AI_TEXTAREA_PADDING_Y))
            .left(px(AI_TEXTAREA_PADDING_X))
            .right(px(AI_TEXTAREA_PADDING_X))
            .flex()
            .flex_col()
            .items_start()
            .gap(px(2.0))
            .text_color(rgba((theme.text_muted << 8) | 0x66));
        for line in placeholder.split('\n') {
            placeholder_layer = placeholder_layer.child(
                div()
                    .min_h(px(AI_TEXTAREA_LINE_HEIGHT))
                    .child(line.to_string()),
            );
        }
        textarea = textarea.child(placeholder_layer);
    }

    let line_count = display_value.split('\n').count();
    for (index, line) in display_value.split('\n').enumerate() {
        let is_last_line = index + 1 == line_count;
        let mut line_row = div()
            .min_h(px(AI_TEXTAREA_LINE_HEIGHT))
            .flex()
            .flex_row()
            .items_start()
            .child(line.to_string());
        if is_last_line {
            if let Some(marked_text) = marked_text.take() {
                line_row = line_row.child(
                    div()
                        .underline()
                        .text_color(rgb(theme.text))
                        .child(marked_text),
                );
            }
            if let Some(caret) = caret.take() {
                line_row = line_row.child(caret);
            }
        }
        textarea = textarea.child(line_row);
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
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(rgb(tokens.ui.text_heading))
                .whitespace_nowrap()
                .child(title),
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
        .w_full()
        .rounded(px(tokens.radii.md))
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
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(tokens.metrics.ui_text_sm))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(tokens.ui.text_heading))
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

pub fn settings_ai_context_windows_header(
    tokens: &ThemeTokens,
    title: String,
    hint: String,
    chevron: AnyElement,
    _max_width: f32,
) -> gpui::Div {
    // Context-window headers keep provider scope visually prominent.
    div()
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
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(tokens.metrics.ui_text_sm))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(tokens.ui.text_heading))
                        .child(title),
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
        .flex_wrap()
        .text_size(px(tokens.metrics.ui_text_xs))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(rgb(tokens.ui.text_muted))
        .child(div().min_w(px(0.0)).flex_1().child(provider_name))
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
        .flex_wrap()
        .items_center()
        .gap(px(8.0))
        .px(px(12.0))
        .py(px(6.0))
        .bg(if has_override {
            rgba((tokens.ui.accent << 8) | AI_CONTEXT_USER_OVERRIDE_BG_ALPHA)
        } else {
            rgba(tokens.ui.bg << 8)
        })
        .when(!is_first, |row| {
            row.border_t_1().border_color(rgba(
                (tokens.ui.border << 8) | AI_CONTEXT_PROVIDER_ROW_TOP_BORDER_ALPHA,
            ))
        })
        .child(
            div()
                .flex_1()
                .min_w(px(160.0))
                .flex_basis(px(160.0))
                .text_size(px(tokens.metrics.ui_text_xs))
                .font_family(mono_font_family)
                .text_color(rgb(tokens.ui.text_muted))
                .truncate()
                .child(model),
        )
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(8.0))
                .child(div().flex_none().child(source_badge))
                .child(div().flex_none().child(input))
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

#[cfg(not(target_family = "wasm"))]
pub fn settings_ai_context_source_badge_for_source(
    tokens: &ThemeTokens,
    label: String,
    source: ContextWindowSource,
) -> AnyElement {
    let (text_color, bg_color) = settings_ai_context_source_badge_colors(tokens, source);
    settings_ai_context_source_badge(tokens, label, text_color, bg_color)
}

#[cfg(not(target_family = "wasm"))]
fn settings_ai_context_source_badge_colors(
    tokens: &ThemeTokens,
    source: ContextWindowSource,
) -> (Rgba, Rgba) {
    const AI_CONTEXT_SOURCE_USER_COLOR: u32 = 0x60a5fa;
    const AI_CONTEXT_SOURCE_API_COLOR: u32 = 0x34d399;
    const AI_CONTEXT_SOURCE_NAME_COLOR: u32 = 0x22d3ee;
    const AI_CONTEXT_SOURCE_BADGE_BG_ALPHA: u32 = 0x1a;
    const AI_CONTEXT_SOURCE_DEFAULT_TEXT_ALPHA: u32 = 0xb3;
    const AI_CONTEXT_SOURCE_DEFAULT_BG_ALPHA: u32 = 0x33;

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

pub fn settings_ai_tool_collapsed_summary(tokens: &ThemeTokens, summary: String) -> AnyElement {
    div()
        .w_full()
        .min_w_0()
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
    // The disabled opacity is display-only; app state still controls interaction.
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(tokens.metrics.settings_card_gap))
        .opacity(if enabled { 1.0 } else { 0.4 })
        .children(children)
        .into_any_element()
}

pub fn settings_ai_tool_policy_grid(groups: Vec<AnyElement>) -> AnyElement {
    // A compact vertical hierarchy keeps policy families scannable without
    // introducing a second modal or nested card surface.
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(16.0))
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
        .py(px(8.0))
        .border_b_1()
        .border_color(rgba((tokens.ui.border << 8) | AI_TOOL_POLICY_DIVIDER_ALPHA))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .text_size(px(tokens.metrics.ui_text_xs))
        .line_height(px(18.0))
        .text_color(rgb(tokens.ui.text_muted))
        .child(div().min_w(px(0.0)).flex_1().child(label))
        .child(div().flex_none().child(control))
        .into_any_element()
}

pub fn settings_ai_tool_policy_group(
    tokens: &ThemeTokens,
    title: String,
    description: String,
    control: AnyElement,
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
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .text_size(px(tokens.metrics.ui_text_sm))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(tokens.ui.text))
                        .child(title),
                )
                .child(div().flex_none().child(control)),
        )
        .child(
            div()
                .mt(px(4.0))
                .text_size(px(tokens.metrics.ui_text_xs))
                .line_height(px(18.0))
                .text_color(rgb(tokens.ui.text_muted))
                .child(description),
        )
        .child(
            div()
                .mt(px(6.0))
                .ml(px(16.0))
                .flex()
                .flex_col()
                .children(items),
        )
        .into_any_element()
}

pub fn settings_ai_disabled_tools_notice(
    tokens: &ThemeTokens,
    label: String,
    description: String,
    tools: AnyElement,
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
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .child(
                    div()
                        .text_size(px(tokens.metrics.ui_text_sm))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(tokens.ui.warning))
                        .child(label),
                )
                .child(action),
        )
        .child(
            div()
                .text_size(px(tokens.metrics.ui_text_xs))
                .text_color(rgb(tokens.ui.text_muted))
                .child(description),
        )
        .child(tools)
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

#[cfg(test)]
mod layout_tests {
    use super::*;

    #[gpui::test]
    fn context_capacity_controls_wrap_without_clipping(cx: &mut gpui::TestAppContext) {
        let window = cx.add_empty_window();
        let tokens = oxideterm_theme::default_tokens();
        for width in [720.0, 360.0] {
            window.draw(
                gpui::point(px(0.0), px(0.0)),
                gpui::size(px(width), px(180.0)),
                |_, _| {
                    let control = |width, color| {
                        div()
                            .w(px(width))
                            .h(px(28.0))
                            .flex_none()
                            .bg(rgb(color))
                            .into_any_element()
                    };
                    settings_ai_context_window_row(
                        &tokens,
                        "monospace".into(),
                        "long-provider-model-name".into(),
                        control(80.0, 0xff0000),
                        control(112.0, 0x00ff00),
                        Some(control(180.0, 0x0000ff)),
                        true,
                        true,
                    )
                },
            );
            window.update(|window, _| {
                let scale = window.scale_factor();
                let quads = window.painted_quads();
                let bounds = |color| {
                    quads
                        .iter()
                        .find(|quad| quad.background == rgb(color).into())
                        .unwrap()
                        .bounds
                };
                let input = bounds(0x00ff00);
                let reset = bounds(0x0000ff);
                assert_eq!(input.size.width.0 / scale, 112.0);
                assert_eq!(reset.size.width.0 / scale, 180.0);
                for color in [0xff0000, 0x00ff00, 0x0000ff] {
                    let control = bounds(color);
                    assert!(control.left().0 / scale >= 12.0);
                    assert!(control.right().0 / scale <= width - 12.0);
                }
                if width < 400.0 {
                    assert!(reset.top() >= input.bottom());
                } else {
                    assert_eq!(reset.top(), input.top());
                }
            });
        }
    }
}
pub mod confirm;
pub mod text_editor;

#[cfg(not(target_family = "wasm"))]
pub mod acp;
#[cfg(not(target_family = "wasm"))]
pub(crate) mod basic;
#[cfg(not(target_family = "wasm"))]
pub(crate) mod context;
#[cfg(not(target_family = "wasm"))]
pub(crate) mod context_windows;
#[cfg(not(target_family = "wasm"))]
pub mod mcp;
#[cfg(not(target_family = "wasm"))]
pub mod mcp_dialog;
#[cfg(not(target_family = "wasm"))]
pub mod mcp_draft;
#[cfg(not(target_family = "wasm"))]
pub mod providers;
#[cfg(not(target_family = "wasm"))]
pub(crate) mod skills;
#[cfg(not(target_family = "wasm"))]
pub mod subagents;

#[cfg(not(target_family = "wasm"))]
pub(crate) fn available(section: AnyElement, enabled: bool) -> AnyElement {
    if enabled {
        section
    } else {
        div()
            .w_full()
            .min_w_0()
            .relative()
            .child(div().w_full().min_w_0().opacity(0.5).child(section))
            .child(div().absolute().inset_0().block_mouse_except_scroll())
            .into_any_element()
    }
}

#[cfg(not(target_family = "wasm"))]
pub(crate) fn section(
    tokens: &ThemeTokens,
    header: AnyElement,
    body: impl IntoIterator<Item = AnyElement>,
) -> AnyElement {
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(tokens.spacing.two))
        .child(div().w_full().min_w_0().px(px(2.0)).child(header))
        .children(body)
        .into_any_element()
}

#[cfg(not(target_family = "wasm"))]
pub struct AiSkillData {
    pub records: Vec<oxideterm_skills::SkillRecord>,
    pub diagnostics: Vec<oxideterm_skills::SkillDiagnostic>,
}

#[cfg(not(target_family = "wasm"))]
pub enum AiSkillAction {
    Refresh,
    EditAndRefresh(crate::components::SettingsEdit),
}

#[cfg(not(target_family = "wasm"))]
pub fn hash_section_layout(
    section: Option<oxideterm_settings_model::SettingsSectionId>,
    state: &crate::entity::SettingsWorkspaceEntity,
    settings: &oxideterm_settings::PersistedSettings,
    providers: &providers::AiProviderSettingsView,
    registry: &oxideterm_skills::SkillRegistry,
    hasher: &mut impl std::hash::Hasher,
) {
    use oxideterm_settings_model::{SettingsSectionId::*, ai_provider_views};
    use std::hash::Hash;

    format!("{:?}", state.route_snapshot().ai_page).hash(hasher);
    // An expansion only invalidates the virtual row whose height may change.
    match section {
        Some(AiProvidersSettings) => {
            settings.ai.providers.len().hash(hasher);
            settings.ai.web_search.enabled.hash(hasher);
            settings.ai.web_search.provider_id.hash(hasher);
            providers.hash_layout(hasher);
        }
        Some(AiContextModelWindows) => {
            settings.ai.providers.len().hash(hasher);
            state.ai_context_windows.hash_layout(hasher);
        }
        Some(AiToolsToolUse) => {
            settings.ai.tool_use.disabled_tools.hash(hasher);
            state.ai_tool_policy_expanded.hash(hasher);
        }
        Some(AiToolsWebSearch) => {
            settings.ai.web_search.enabled.hash(hasher);
            settings.ai.web_search.provider_id.hash(hasher);
            settings.ai.providers.len().hash(hasher);
        }
        Some(AiToolsAgents) => {
            settings.ai.tool_use.enabled.hash(hasher);
            state.ai_subagent_picker_open.hash(hasher);
            let options = subagents::global_agent_options(settings);
            options.enabled.hash(hasher);
            options
                .default_model
                .as_ref()
                .map(|model| (&model.provider_id, &model.model))
                .hash(hasher);
            for provider in ai_provider_views(settings) {
                provider.id.hash(hasher);
                provider.enabled.hash(hasher);
                provider.models.hash(hasher);
            }
        }
        Some(AiToolsSkills) => {
            state.skill_search_query.hash(hasher);
            format!("{:?}", state.skill_scope_filter).hash(hasher);
            state.skill_diagnostics_expanded.hash(hasher);
            for skill in registry.records() {
                skill.id.hash(hasher);
                skill.skill_path.hash(hasher);
                state
                    .expanded_skill_paths
                    .contains(&skill.skill_path)
                    .hash(hasher);
                skill.enabled.hash(hasher);
                skill.content_hash.hash(hasher);
            }
            for diagnostic in registry.diagnostics() {
                diagnostic.path.hash(hasher);
                diagnostic.message.hash(hasher);
            }
        }
        _ => {}
    }
}

#[cfg(not(target_family = "wasm"))]
pub(crate) fn textarea(
    controls: &crate::components::SettingsControls<'_>,
    input: crate::SettingsInput,
    label: String,
    hint: String,
    placeholder: String,
    value: String,
    min_height: f32,
    cx: &gpui::App,
) -> AnyElement {
    use crate::components::SettingsWindowRequest;
    use oxideterm_gpui_ui::text_input::{text_caret, text_input_anchor_probe};
    let frame = controls.ime.frame(input, &value, cx);
    let caret = frame
        .focused
        .then(|| text_caret(controls.tokens, frame.caret_visible).into_any_element());
    let area = settings_ai_textarea_surface(
        controls.tokens,
        min_height,
        frame.focused,
        frame.value,
        &placeholder,
        frame.marked_text.map(str::to_owned),
        caret,
    )
    .on_mouse_down(gpui::MouseButton::Left, {
        let dispatch = controls.dispatch.clone();
        move |event, window, cx| {
            dispatch(
                SettingsWindowRequest::BeginInputSelection(input, event.clone()),
                window,
                cx,
            );
            cx.stop_propagation();
        }
    })
    .on_mouse_move({
        let dispatch = controls.dispatch.clone();
        move |event, window, cx| {
            dispatch(
                SettingsWindowRequest::DragInputSelection(event.clone()),
                window,
                cx,
            )
        }
    });
    let update = frame.update_anchor;
    let input = text_input_anchor_probe(frame.anchor_id, area, move |anchor, _, _| update(anchor));
    settings_ai_textarea_row(controls.tokens, label, input.into_any_element(), hint)
}
