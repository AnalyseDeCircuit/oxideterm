use gpui::{
    Div, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement, Stateful,
    StatefulInteractiveElement, Styled, div, prelude::*, px, rgb,
};
use oxideterm_theme::ThemeTokens;

use crate::modal::rounded_shell_child_radius;

use super::tokens::*;

const AI_AGENT_HEADER_PX: f32 = 16.0; // Tauri AgentPanel px-4.
const AI_AGENT_HEADER_PY: f32 = 12.0; // Tauri AgentPanel py-3.
const AI_AGENT_TOOLBAR_PY: f32 = 8.0; // Tauri autonomy/roles strip py-2.
const AI_AGENT_TASK_TEXTAREA_MAX_HEIGHT: f32 = 200.0; // Tauri auto-resize clamp at 200px.
const AI_AGENT_APPROVAL_ARGS_MAX_HEIGHT: f32 = 128.0; // Tauri max-h-32.
const AI_AGENT_HISTORY_ACTION_HIDDEN_ALPHA: f32 = 0.0; // Tauri group-hover starts hidden.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiAgentAutonomy {
    Supervised,
    Balanced,
    Autonomous,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiAgentStepStatus {
    Pending,
    Running,
    Completed,
    Error,
    Skipped,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiAgentSummaryKind {
    Completed,
    Failed,
    HandedOff,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiAgentReviewAssessment {
    Pass,
    NeedsCorrection,
    ResetRequired,
    CriticalFailure,
}

fn autonomy_tone(level: AiAgentAutonomy) -> AiTone {
    match level {
        AiAgentAutonomy::Supervised => AiTone::Blue,
        AiAgentAutonomy::Balanced => AiTone::Amber,
        AiAgentAutonomy::Autonomous => AiTone::Green,
    }
}

fn step_status_tone(status: AiAgentStepStatus) -> AiTone {
    match status {
        AiAgentStepStatus::Pending | AiAgentStepStatus::Skipped => AiTone::Muted,
        AiAgentStepStatus::Running => AiTone::Accent,
        AiAgentStepStatus::Completed => AiTone::Green,
        AiAgentStepStatus::Error => AiTone::Red,
    }
}

fn summary_tone(kind: AiAgentSummaryKind) -> AiTone {
    match kind {
        AiAgentSummaryKind::Completed => AiTone::Green,
        AiAgentSummaryKind::Failed => AiTone::Red,
        AiAgentSummaryKind::HandedOff => AiTone::Amber,
    }
}

fn review_tone(assessment: AiAgentReviewAssessment) -> AiTone {
    match assessment {
        AiAgentReviewAssessment::Pass => AiTone::Green,
        AiAgentReviewAssessment::NeedsCorrection => AiTone::Amber,
        AiAgentReviewAssessment::ResetRequired => AiTone::Orange,
        AiAgentReviewAssessment::CriticalFailure => AiTone::Red,
    }
}

pub fn ai_agent_panel_shell(tokens: &ThemeTokens) -> Div {
    div()
        .h_full()
        .flex()
        .flex_col()
        .bg(rgb(tokens.ui.bg))
        .text_color(rgb(tokens.ui.text))
}

pub fn ai_agent_header(
    tokens: &ThemeTokens,
    icon: impl IntoElement,
    title: impl Into<String>,
    model_selector: impl IntoElement,
) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(tokens.spacing.three))
        .px(px(AI_AGENT_HEADER_PX))
        .py(px(AI_AGENT_HEADER_PY))
        .border_b_1()
        .border_color(rgb(tokens.ui.border))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(tokens.spacing.two))
                .child(icon)
                .child(
                    div()
                        .text_size(px(tokens.metrics.ui_text_sm))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(tokens.ui.text))
                        .child(title.into()),
                ),
        )
        .child(model_selector)
}

pub fn ai_agent_toolbar(tokens: &ThemeTokens) -> Div {
    div()
        .px(px(AI_AGENT_HEADER_PX))
        .py(px(AI_AGENT_TOOLBAR_PY))
        .border_b_1()
        .border_color(rgb(tokens.ui.border))
        .flex()
        .flex_col()
        .gap(px(tokens.spacing.two))
}

pub fn ai_agent_scroll_area(tokens: &ThemeTokens, id: impl Into<ElementId>) -> Stateful<Div> {
    div()
        .id(id)
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .px(px(AI_AGENT_HEADER_PX))
        .py(px(tokens.spacing.three))
        .flex()
        .flex_col()
        .gap(px(tokens.spacing.three))
}

pub fn ai_agent_footer(tokens: &ThemeTokens) -> Div {
    div()
        .px(px(AI_AGENT_HEADER_PX))
        .py(px(AI_AGENT_HEADER_PY))
        .border_t_1()
        .border_color(rgb(tokens.ui.border))
        .flex()
        .flex_col()
        .gap(px(tokens.spacing.three))
}

pub fn ai_agent_autonomy_group(tokens: &ThemeTokens) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(tokens.spacing.one))
        .rounded(px(tokens.radii.lg))
        .bg(rgb(tokens.ui.bg_hover))
        .p(px(tokens.spacing.one / 2.0))
}

pub fn ai_agent_autonomy_option(
    tokens: &ThemeTokens,
    level: AiAgentAutonomy,
    label: impl Into<String>,
    active: bool,
    disabled: bool,
    icon: impl IntoElement,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(tokens.spacing.one + tokens.spacing.one / 2.0))
        .rounded(px(tokens.radii.md))
        .px(px(tokens.spacing.two + tokens.spacing.one / 2.0))
        .py(px(tokens.spacing.one))
        .text_size(px(AI_TEXT_12))
        .font_weight(FontWeight::MEDIUM)
        .text_color(if active {
            rgb(tokens.ui.text)
        } else {
            rgb(tokens.ui.text_muted)
        })
        .when(active, |option| {
            option.bg(rgb(tokens.ui.bg_active)).shadow_sm()
        })
        .opacity(if disabled { 0.5 } else { 1.0 })
        .cursor_pointer()
        .hover(|style| style.text_color(rgb(tokens.ui.text)))
        .child(
            div()
                .text_color(if active {
                    rgb(tone_color(tokens, autonomy_tone(level)))
                } else {
                    rgb(tokens.ui.text_muted)
                })
                .child(icon),
        )
        .child(label.into())
}

pub fn ai_agent_role_toggle(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    expanded: bool,
    active_dot: bool,
    chevron: impl IntoElement,
) -> Div {
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(px(tokens.spacing.one + tokens.spacing.one / 2.0))
        .text_size(px(AI_TEXT_11))
        .text_color(rgb(tokens.ui.text_muted))
        .cursor_pointer()
        .hover(|style| style.text_color(rgb(tokens.ui.text)))
        .child(chevron)
        .child(div().font_weight(FontWeight::MEDIUM).child(label.into()))
        .when(active_dot, |row| {
            row.child(div().size(px(6.0)).rounded_full().bg(rgb(tokens.ui.accent)))
        })
        .when(expanded, |row| row.text_color(rgb(tokens.ui.text)))
}

pub fn ai_agent_task_input_frame(tokens: &ThemeTokens) -> Div {
    div().flex().flex_col().gap(px(tokens.spacing.two))
}

pub fn ai_agent_task_textarea(
    tokens: &ThemeTokens,
    editor: impl IntoElement,
    disabled: bool,
) -> Div {
    div()
        .w_full()
        .max_h(px(AI_AGENT_TASK_TEXTAREA_MAX_HEIGHT))
        .rounded(px(tokens.radii.lg))
        .border_1()
        .border_color(rgb(tokens.ui.border))
        .bg(rgb(tokens.ui.bg_hover))
        .px(px(tokens.spacing.three))
        .py(px(tokens.spacing.two))
        .text_size(px(tokens.metrics.ui_text_sm))
        .text_color(rgb(tokens.ui.text))
        .opacity(if disabled { 0.5 } else { 1.0 })
        .child(editor)
}

pub fn ai_agent_task_footer(
    tokens: &ThemeTokens,
    shortcut: impl Into<String>,
    action: impl IntoElement,
) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(tokens.spacing.two))
        .child(
            div()
                .text_size(px(tokens.metrics.ui_text_xs))
                .text_color(rgb(tokens.ui.text_muted))
                .child(shortcut.into()),
        )
        .child(action)
}

pub fn ai_agent_start_button(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    disabled: bool,
    icon: impl IntoElement,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(tokens.spacing.one + tokens.spacing.one / 2.0))
        .rounded(px(tokens.radii.md))
        .bg(rgb(tokens.ui.accent))
        .px(px(tokens.spacing.three))
        .py(px(tokens.spacing.one + tokens.spacing.one / 2.0))
        .text_size(px(AI_TEXT_12))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(tokens.ui.bg))
        .opacity(if disabled { 0.45 } else { 1.0 })
        .cursor_pointer()
        .child(icon)
        .child(label.into())
}

pub fn ai_agent_collapsible_panel(tokens: &ThemeTokens) -> Div {
    div()
        .overflow_hidden()
        .rounded(px(tokens.radii.lg))
        .border_1()
        .border_color(rgb(tokens.ui.border))
}

pub fn ai_agent_panel_header(
    tokens: &ThemeTokens,
    title: impl Into<String>,
    meta: impl Into<String>,
    chevron: impl IntoElement,
    icon: impl IntoElement,
) -> Div {
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(px(tokens.spacing.two))
        // Browser disclosure panels clip the painted header into the parent
        // radius. Keep the native header from leaving square corner pixels.
        .rounded_t(px(rounded_shell_child_radius(tokens.radii.lg)))
        .bg(rgb(tokens.ui.bg_hover))
        .px(px(tokens.spacing.three))
        .py(px(tokens.spacing.two))
        .text_size(px(tokens.metrics.ui_text_sm))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(tokens.ui.text))
        .cursor_pointer()
        .hover(|style| style.bg(rgb(tokens.ui.bg_active)))
        .child(chevron)
        .child(icon)
        .child(title.into())
        .child(
            div()
                .ml_auto()
                .text_size(px(tokens.metrics.ui_text_xs))
                .font_weight(FontWeight::NORMAL)
                .text_color(rgb(tokens.ui.text_muted))
                .child(meta.into()),
        )
}

pub fn ai_agent_plan_body(tokens: &ThemeTokens) -> Div {
    div()
        .px(px(tokens.spacing.three))
        .py(px(tokens.spacing.two))
        .flex()
        .flex_col()
        .gap(px(tokens.spacing.one))
}

pub fn ai_agent_plan_step(
    tokens: &ThemeTokens,
    description: impl Into<String>,
    status: AiAgentStepStatus,
    active: bool,
    skip_action: Option<impl IntoElement>,
    icon: impl IntoElement,
) -> Div {
    let muted = status == AiAgentStepStatus::Skipped || status == AiAgentStepStatus::Completed;
    div()
        .flex()
        .items_start()
        .gap(px(tokens.spacing.two))
        .py(px(tokens.spacing.one))
        .text_size(px(AI_TEXT_12))
        .text_color(if active {
            rgb(tokens.ui.text)
        } else {
            rgb(tokens.ui.text_muted)
        })
        .font_weight(if active {
            FontWeight::MEDIUM
        } else {
            FontWeight::NORMAL
        })
        .when(muted, |step| step.opacity(0.8))
        .child(
            div()
                .mt(px(tokens.spacing.one / 2.0))
                .text_color(rgb(tone_color(tokens, step_status_tone(status))))
                .child(icon),
        )
        .child(div().flex_1().child(description.into()))
        .when_some(skip_action, |step, action| step.child(action))
}

pub fn ai_agent_card(
    tokens: &ThemeTokens,
    icon: impl IntoElement,
    title: impl Into<String>,
    content: impl IntoElement,
) -> Div {
    div()
        .rounded(px(tokens.radii.lg))
        .border_1()
        .border_color(rgb(tokens.ui.border))
        .bg(bg_alpha(tokens, tokens.ui.bg_hover, 0x99))
        .p(px(tokens.spacing.three))
        .flex()
        .flex_col()
        .gap(px(tokens.spacing.two))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(tokens.spacing.two))
                .text_size(px(tokens.metrics.ui_text_sm))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(tokens.ui.text))
                .child(icon)
                .child(title.into()),
        )
        .child(content)
}

pub fn ai_agent_metric_cell(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    value: impl IntoElement,
) -> Div {
    div()
        .rounded(px(tokens.radii.md))
        .bg(rgb(tokens.ui.bg))
        .px(px(tokens.spacing.two + tokens.spacing.one / 2.0))
        .py(px(tokens.spacing.two))
        .text_size(px(AI_TEXT_12))
        .child(
            div()
                .mb(px(tokens.spacing.one))
                .text_color(rgb(tokens.ui.text_muted))
                .child(label.into()),
        )
        .child(div().text_color(rgb(tokens.ui.text)).child(value))
}

pub fn ai_agent_review_badge(
    tokens: &ThemeTokens,
    assessment: AiAgentReviewAssessment,
    label: impl Into<String>,
) -> Div {
    let tone = review_tone(assessment);
    div()
        .rounded_full()
        .border_1()
        .border_color(tone_border(tokens, tone, AI_CHIP_BORDER_ALPHA))
        .bg(tone_bg(tokens, tone, AI_CHIP_BG_ALPHA))
        .px(px(tokens.spacing.two))
        .py(px(tokens.spacing.one / 2.0))
        .text_size(px(AI_TEXT_10))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(tone_color(tokens, tone)))
        .child(label.into())
}

pub fn ai_agent_step_entry(
    tokens: &ThemeTokens,
    status: AiAgentStepStatus,
    header: impl IntoElement,
    body: Option<impl IntoElement>,
) -> Div {
    div()
        .border_l_2()
        .border_color(match status {
            AiAgentStepStatus::Completed => tone_border(tokens, AiTone::Green, 0x66),
            AiAgentStepStatus::Running => rgb(tokens.ui.accent),
            AiAgentStepStatus::Error => tone_border(tokens, AiTone::Red, 0x66),
            AiAgentStepStatus::Skipped => rgb(tokens.ui.border),
            AiAgentStepStatus::Pending => {
                bg_alpha(tokens, tokens.ui.border, AI_CHAT_INPUT_BORDER_ALPHA)
            }
        })
        .pl(px(tokens.spacing.three))
        .py(px(tokens.spacing.one + tokens.spacing.one / 2.0))
        .child(header)
        .when_some(body, |entry, body| {
            entry.child(div().mt(px(tokens.spacing.one)).child(body))
        })
}

pub fn ai_agent_step_header(
    tokens: &ThemeTokens,
    icon: impl IntoElement,
    label: impl Into<String>,
    tool_badge: Option<impl IntoElement>,
    duration: Option<impl IntoElement>,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(tokens.spacing.two))
        .child(icon)
        .child(
            div()
                .text_size(px(AI_TEXT_12))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(tokens.ui.text_muted))
                .child(label.into()),
        )
        .when_some(tool_badge, |row, badge| row.child(badge))
        .when_some(duration, |row, duration| {
            row.child(div().ml_auto().child(duration))
        })
}

pub fn ai_agent_code_badge(tokens: &ThemeTokens, label: impl Into<String>) -> Div {
    div()
        .rounded(px(tokens.radii.md))
        .bg(rgb(tokens.ui.bg_hover))
        .px(px(tokens.spacing.one + tokens.spacing.one / 2.0))
        .py(px(tokens.spacing.one / 2.0))
        .text_size(px(AI_TEXT_12))
        .font_family(ai_font_family())
        .text_color(rgb(tokens.ui.accent))
        .child(label.into())
}

pub fn ai_agent_pre(tokens: &ThemeTokens, content: impl Into<String>, clamped: bool) -> Div {
    div()
        .text_size(px(AI_TEXT_12))
        .font_family(ai_font_family())
        .text_color(rgb(tokens.ui.text_muted))
        .whitespace_normal()
        .when(clamped, |pre| pre.max_h(px(80.0)).overflow_hidden())
        .child(content.into())
}

pub fn ai_agent_empty_log(
    tokens: &ThemeTokens,
    icon: impl IntoElement,
    label: impl Into<String>,
) -> Div {
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .py(px(48.0))
        .text_color(rgb(tokens.ui.text_muted))
        .child(div().mb(px(tokens.spacing.three)).opacity(0.2).child(icon))
        .child(
            div()
                .text_size(px(tokens.metrics.ui_text_sm))
                .child(label.into()),
        )
}

pub fn ai_agent_approval_bar(tokens: &ThemeTokens, title: impl IntoElement) -> Div {
    div()
        .rounded(px(tokens.radii.lg))
        .border_1()
        .border_color(tone_border(tokens, AiTone::Amber, AI_CHIP_BORDER_ALPHA))
        .bg(tone_bg(tokens, AiTone::Amber, 0x0d))
        .p(px(tokens.spacing.three))
        .flex()
        .flex_col()
        .gap(px(tokens.spacing.two))
        .child(title)
}

pub fn ai_agent_approval_item(tokens: &ThemeTokens, header: impl IntoElement) -> Div {
    div()
        .rounded(px(tokens.radii.md))
        .bg(rgb(tokens.ui.bg_hover))
        .px(px(tokens.spacing.three))
        .py(px(tokens.spacing.two))
        .flex()
        .flex_col()
        .gap(px(tokens.spacing.one + tokens.spacing.one / 2.0))
        .child(header)
}

pub fn ai_agent_approval_args(
    tokens: &ThemeTokens,
    id: impl Into<ElementId>,
    expanded: bool,
    content: impl Into<String>,
) -> Stateful<Div> {
    div()
        .id(id)
        .max_h(px(if expanded {
            AI_AGENT_APPROVAL_ARGS_MAX_HEIGHT
        } else {
            20.0
        }))
        .overflow_y_scroll()
        .rounded(px(tokens.radii.md))
        .bg(rgb(tokens.ui.bg))
        .p(px(tokens.spacing.one + tokens.spacing.one / 2.0))
        .text_size(px(AI_TEXT_10))
        .font_family(ai_font_family())
        .text_color(rgb(tokens.ui.text_muted))
        .whitespace_normal()
        .child(content.into())
}

pub fn ai_agent_control_bar(tokens: &ThemeTokens) -> Div {
    div()
        .pt(px(tokens.spacing.two))
        .border_t_1()
        .border_color(rgb(tokens.ui.border))
        .flex()
        .items_center()
        .gap(px(tokens.spacing.three))
}

pub fn ai_agent_progress_stack(
    tokens: &ThemeTokens,
    status: impl Into<String>,
    round: impl Into<String>,
    fraction: f32,
    failed: bool,
) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .child(
            div()
                .mb(px(tokens.spacing.one))
                .flex()
                .items_center()
                .justify_between()
                .text_size(px(tokens.metrics.ui_text_xs))
                .text_color(rgb(tokens.ui.text_muted))
                .child(status.into())
                .child(round.into()),
        )
        .child(
            div()
                .h(px(4.0))
                .overflow_hidden()
                .rounded_full()
                .bg(rgb(tokens.ui.bg_hover))
                .child(
                    div()
                        .h_full()
                        .rounded_full()
                        .bg(if failed {
                            rgb(AI_TW_RED)
                        } else {
                            rgb(tokens.ui.accent)
                        })
                        .w(gpui::relative(fraction.clamp(0.0, 1.0))),
                ),
        )
}

pub fn ai_agent_icon_button(
    tokens: &ThemeTokens,
    icon: impl IntoElement,
    tone: Option<AiTone>,
) -> Div {
    div()
        .size(px(28.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(tokens.radii.md))
        .text_color(if let Some(tone) = tone {
            rgb(tone_color(tokens, tone))
        } else {
            rgb(tokens.ui.text_muted)
        })
        .cursor_pointer()
        .hover(|style| style.bg(rgb(tokens.ui.bg_hover)))
        .child(icon)
}

pub fn ai_agent_summary_block(
    tokens: &ThemeTokens,
    kind: AiAgentSummaryKind,
    header: impl IntoElement,
    body: Option<impl IntoElement>,
) -> Div {
    let tone = summary_tone(kind);
    div()
        .rounded(px(tokens.radii.lg))
        .border_1()
        .border_color(tone_border(tokens, tone, AI_CHIP_BORDER_ALPHA))
        .bg(tone_bg(tokens, tone, 0x0d))
        .p(px(tokens.spacing.three))
        .child(header)
        .when_some(body, |block, body| {
            block.child(div().mt(px(tokens.spacing.one)).child(body))
        })
}

pub fn ai_agent_history_section(tokens: &ThemeTokens) -> Div {
    div()
        .border_t_1()
        .border_color(rgb(tokens.ui.border))
        .pt(px(tokens.spacing.three))
}

pub fn ai_agent_history_row(
    tokens: &ThemeTokens,
    status: AiAgentSummaryKind,
    icon: impl IntoElement,
    title: impl Into<String>,
    meta: impl Into<String>,
    actions: Option<impl IntoElement>,
    actions_visible: bool,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(tokens.spacing.two))
        .rounded(px(tokens.radii.md))
        .bg(rgb(tokens.ui.bg_hover))
        .px(px(tokens.spacing.two + tokens.spacing.one / 2.0))
        .py(px(tokens.spacing.one + tokens.spacing.one / 2.0))
        .text_size(px(AI_TEXT_12))
        .text_color(rgb(tokens.ui.text_muted))
        .cursor_pointer()
        .hover(|style| style.bg(rgb(tokens.ui.bg_active)))
        .child(
            div()
                .text_color(rgb(tone_color(tokens, summary_tone(status))))
                .child(icon),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .child(div().truncate().child(title.into()))
                .child(
                    div()
                        .truncate()
                        .text_size(px(AI_TEXT_10))
                        .text_color(rgb(tokens.ui.text_muted))
                        .child(meta.into()),
                ),
        )
        .when_some(actions, |row, actions| {
            row.child(
                div()
                    .opacity(if actions_visible {
                        1.0
                    } else {
                        AI_AGENT_HISTORY_ACTION_HIDDEN_ALPHA
                    })
                    .child(actions),
            )
        })
}

pub fn ai_agent_role_editor_card(tokens: &ThemeTokens) -> Div {
    div()
        .rounded(px(tokens.radii.lg))
        .border_1()
        .border_color(rgb(tokens.ui.border))
        .bg(rgb(tokens.ui.bg_hover))
        .p(px(tokens.spacing.three))
        .flex()
        .flex_col()
        .gap(px(tokens.spacing.three))
}

pub fn ai_agent_form_label(tokens: &ThemeTokens, label: impl Into<String>) -> Div {
    div()
        .mb(px(tokens.spacing.one / 2.0))
        .text_size(px(AI_TEXT_10))
        .text_color(rgb(tokens.ui.text_muted))
        .child(label.into())
}

pub fn ai_agent_form_input(tokens: &ThemeTokens, input: impl IntoElement) -> Div {
    div()
        .h(px(28.0))
        .w_full()
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(rgb(tokens.ui.border))
        .bg(rgb(tokens.ui.bg))
        .px(px(tokens.spacing.two))
        .text_size(px(AI_TEXT_12))
        .text_color(rgb(tokens.ui.text))
        .child(input)
}

pub fn ai_agent_template_chip(tokens: &ThemeTokens, label: impl Into<String>) -> Div {
    div()
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(rgb(tokens.ui.border))
        .bg(rgb(tokens.ui.bg))
        .px(px(tokens.spacing.one))
        .py(px(tokens.spacing.one / 2.0))
        .text_size(px(AI_TEXT_9))
        .text_color(rgb(tokens.ui.text_muted))
        .cursor_pointer()
        .hover(|style| style.text_color(rgb(tokens.ui.accent)))
        .child(label.into())
}
