// Hallmark · pre-emit critique: P5 H5 E5 S5 R5 V5
use gpui::{
    BoxShadow, CursorStyle, Div, IntoColor, ParentElement, Styled, div, point, prelude::*, px, rgb,
    rgba, svg,
};
use oxideterm_theme::ThemeTokens;

const CHECKBOX_UNCHECKED_BG_ALPHA: u32 = 0x00; // Tauri unchecked root has no background class.
const CHECKBOX_CHECKED_BG_ALPHA: u32 = 0xff; // Tauri data-[state=checked]:bg-theme-accent.
const CHECKBOX_CHECKED_TEXT: u32 = 0xffffff; // Tauri data-[state=checked]:text-white.
const CHECKBOX_DISABLED_OPACITY: f32 = 0.5; // Tauri disabled:opacity-50.
const CHECKBOX_ENABLED_OPACITY: f32 = 1.0;
const CHECKBOX_FOCUS_RING_ALPHA: u32 = 0xb3; // Tauri focus-visible:ring-theme-accent/70.
const CHECKBOX_FOCUS_RING_WIDTH: f32 = 2.0; // Tauri focus-visible:ring-2.
const CHECKBOX_FOCUS_RING_OFFSET: f32 = 1.0; // Tauri focus-visible:ring-offset-1.
const CHECKBOX_ICON_PATH: &str = "lucide/check.svg";
const CHECKBOX_INDETERMINATE_MARK_WIDTH: f32 = 8.0;
const CHECKBOX_INDETERMINATE_MARK_HEIGHT: f32 = 2.0;

/// Visual mark state for binary and group-level checkbox controls.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CheckboxState {
    #[default]
    Unchecked,
    Checked,
    Indeterminate,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CheckboxOptions {
    pub focused: bool,
    pub disabled: bool,
}

pub fn checkbox(tokens: &ThemeTokens, label: String, checked: bool) -> Div {
    checkbox_with(tokens, label, checked, CheckboxOptions::default())
}

pub fn checkbox_with(
    tokens: &ThemeTokens,
    label: String,
    checked: bool,
    options: CheckboxOptions,
) -> Div {
    checkbox_with_state(
        tokens,
        label,
        if checked {
            CheckboxState::Checked
        } else {
            CheckboxState::Unchecked
        },
        options,
    )
}

/// Renders a checkbox that can communicate a partially selected group.
pub fn checkbox_with_state(
    tokens: &ThemeTokens,
    label: String,
    state: CheckboxState,
    options: CheckboxOptions,
) -> Div {
    let theme = tokens.ui;
    let has_label = !label.is_empty();
    let selected = state != CheckboxState::Unchecked;
    let mark_animation_id = (
        gpui::SharedString::from(format!("checkbox-mark-{label}")),
        state as usize,
    );
    let mark = match state {
        CheckboxState::Indeterminate => div()
            .w(px(CHECKBOX_INDETERMINATE_MARK_WIDTH))
            .h(px(CHECKBOX_INDETERMINATE_MARK_HEIGHT))
            .rounded_full()
            .bg(rgb(CHECKBOX_CHECKED_TEXT))
            .into_any_element(),
        CheckboxState::Checked | CheckboxState::Unchecked => {
            // Keep the check mounted so both checking and unchecking can
            // animate without delaying the input state transition.
            crate::motion::animated_checkmark(
                tokens,
                mark_animation_id,
                svg()
                    .path(CHECKBOX_ICON_PATH)
                    .size(px(tokens.metrics.ui_checkbox_icon_size))
                    .text_color(rgb(CHECKBOX_CHECKED_TEXT)),
                state == CheckboxState::Checked,
            )
        }
    };
    let checkbox = div()
        .flex()
        .flex_row()
        .items_center()
        .cursor(if options.disabled {
            CursorStyle::OperationNotAllowed
        } else {
            CursorStyle::PointingHand
        })
        .opacity(if options.disabled {
            CHECKBOX_DISABLED_OPACITY
        } else {
            CHECKBOX_ENABLED_OPACITY
        })
        .child(
            div()
                .size(px(tokens.metrics.ui_checkbox_size))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(tokens.radii.xs))
                .border_1()
                .border_color(if selected {
                    rgb(theme.accent)
                } else {
                    rgb(theme.border)
                })
                .bg(if selected {
                    rgba((theme.accent << 8) | CHECKBOX_CHECKED_BG_ALPHA)
                } else {
                    rgba((theme.bg << 8) | CHECKBOX_UNCHECKED_BG_ALPHA)
                })
                .when(options.focused, |box_el| {
                    box_el.shadow(checkbox_focus_ring(tokens))
                })
                .child(mark),
        );

    if has_label {
        checkbox.gap_2().child(
            div()
                .text_size(px(tokens.metrics.ui_text_sm))
                .text_color(rgb(theme.text))
                .child(label),
        )
    } else {
        checkbox
    }
}

fn checkbox_focus_ring(tokens: &ThemeTokens) -> Vec<BoxShadow> {
    let zero = point(px(0.0), px(0.0));
    vec![
        BoxShadow {
            color: rgb(tokens.ui.bg).into_color(),
            offset: zero,
            blur_radius: px(0.0),
            spread_radius: px(CHECKBOX_FOCUS_RING_OFFSET),
            inset: false,
        },
        BoxShadow {
            color: rgba((tokens.ui.accent << 8) | CHECKBOX_FOCUS_RING_ALPHA).into_color(),
            offset: zero,
            blur_radius: px(0.0),
            spread_radius: px(CHECKBOX_FOCUS_RING_OFFSET + CHECKBOX_FOCUS_RING_WIDTH),
            inset: false,
        },
    ]
}
