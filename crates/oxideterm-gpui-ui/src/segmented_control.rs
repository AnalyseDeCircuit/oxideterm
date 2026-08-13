use std::time::Duration;

use gpui::{
    Animation, AnimationExt, AnyElement, Div, ElementId, IntoElement, Styled, div, prelude::*, px,
    relative, rgb, rgba,
};
use oxideterm_theme::ThemeTokens;

use crate::{color_for_background, theme_card_shadow, theme_glass_card_background_alpha};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SegmentedControlOptions {
    pub active_index: usize,
    pub previous_index: usize,
    pub item_count: usize,
    pub user_transition_active: bool,
    pub has_background_image: bool,
    pub layout: SegmentedControlLayout,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SegmentedControlLayout {
    Fill,
    Compact { width: f32 },
}

impl SegmentedControlOptions {
    pub const fn new(active_index: usize, previous_index: usize, item_count: usize) -> Self {
        Self {
            active_index,
            previous_index,
            item_count,
            user_transition_active: false,
            has_background_image: false,
            layout: SegmentedControlLayout::Fill,
        }
    }

    pub const fn user_transition_active(mut self, user_transition_active: bool) -> Self {
        self.user_transition_active = user_transition_active;
        self
    }

    pub const fn has_background_image(mut self, has_background_image: bool) -> Self {
        self.has_background_image = has_background_image;
        self
    }

    pub const fn compact(mut self, width: f32) -> Self {
        self.layout = SegmentedControlLayout::Compact { width };
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SegmentedControlMotion {
    pub duration: Duration,
    pub spatial: bool,
}

/// Disabled motion returns no transition at all. Callers must not construct a
/// zero-duration GPUI animation because it still participates in scheduling.
pub fn segmented_control_motion(tokens: &ThemeTokens) -> Option<SegmentedControlMotion> {
    tokens.motion.enabled.then_some(SegmentedControlMotion {
        duration: Duration::from_millis(tokens.motion.normal_duration_ms),
        spatial: tokens.motion.spatial_enabled,
    })
}

fn should_animate_selection(
    options: SegmentedControlOptions,
    active_index: usize,
    previous_index: usize,
) -> bool {
    options.user_transition_active && previous_index != active_index
}

pub fn segmented_control(
    tokens: &ThemeTokens,
    id: impl Into<ElementId>,
    options: SegmentedControlOptions,
    items: Vec<AnyElement>,
) -> Div {
    let item_count = options.item_count.max(1);
    let active_index = options.active_index.min(item_count - 1);
    let previous_index = options.previous_index.min(item_count - 1);
    let item_width = 1.0 / item_count as f32;
    let active_left = active_index as f32 * item_width;
    let previous_left = previous_index as f32 * item_width;
    // Stale previous indices are presentation history, not animation intent.
    // Only an explicit user transition may turn that history into movement.
    let motion = should_animate_selection(options, active_index, previous_index)
        .then(|| segmented_control_motion(tokens))
        .flatten();
    let animation_id = (id.into(), format!("{previous_index}-to-{active_index}"));
    // The outer control already supplies its inset. The moving highlight must
    // fill the option cell so its size matches the pre-animation selection.
    let indicator = div()
        .absolute()
        .top(px(0.0))
        .bottom(px(0.0))
        .w(relative(item_width))
        .rounded(px(tokens.radii.md));
    let indicator = match options.layout {
        SegmentedControlLayout::Fill => indicator.bg(rgba((tokens.ui.accent << 8) | 0x26)),
        SegmentedControlLayout::Compact { .. } => indicator
            .border_1()
            .border_color(color_for_background(
                tokens.ui.border,
                options.has_background_image,
                0xb3,
            ))
            .bg(color_for_background(
                tokens.ui.bg_panel,
                options.has_background_image,
                theme_glass_card_background_alpha(tokens),
            ))
            .shadow(theme_card_shadow(tokens)),
    };
    let indicator: AnyElement = match motion {
        None => indicator.left(relative(active_left)).into_any_element(),
        Some(motion) if !motion.spatial => indicator
            .left(relative(active_left))
            .with_animation(
                animation_id,
                Animation::new(motion.duration),
                |indicator, progress| indicator.opacity(progress),
            )
            .into_any_element(),
        Some(motion) => indicator
            .with_animation(
                animation_id,
                Animation::new(motion.duration).with_easing(crate::motion::ease_in_out_cubic),
                move |indicator, progress| {
                    indicator.left(relative(crate::motion::lerp(
                        previous_left,
                        active_left,
                        progress,
                    )))
                },
            )
            .into_any_element(),
    };

    let mut inner = div().relative().w_full().flex().flex_row().child(indicator);
    for item in items {
        inner = inner.child(item);
    }

    match options.layout {
        SegmentedControlLayout::Fill => div()
            .w_full()
            .rounded(px(tokens.radii.lg))
            .border_1()
            .border_color(rgb(tokens.ui.border))
            .bg(color_for_background(
                tokens.ui.bg_card,
                options.has_background_image,
                theme_glass_card_background_alpha(tokens),
            ))
            .shadow(theme_card_shadow(tokens))
            .p(px(8.0))
            .child(inner),
        SegmentedControlLayout::Compact { width } => {
            div().flex_none().w(px(width)).max_w_full().child(inner)
        }
    }
}

pub fn segmented_control_item(tokens: &ThemeTokens, label: String, active: bool) -> Div {
    segmented_control_item_content(
        tokens,
        active,
        div()
            .w_full()
            .text_align(gpui::TextAlign::Center)
            .child(label)
            .into_any_element(),
    )
}

pub fn segmented_control_item_content(
    tokens: &ThemeTokens,
    active: bool,
    child: AnyElement,
) -> Div {
    let theme = tokens.ui;
    div()
        .relative()
        .flex()
        .flex_1()
        .min_w(px(0.0))
        .items_center()
        .justify_center()
        .rounded(px(tokens.radii.md))
        .overflow_hidden()
        .px(px(12.0))
        .py(px(6.0))
        .whitespace_nowrap()
        .text_align(gpui::TextAlign::Center)
        .text_size(px(tokens.metrics.ui_text_sm))
        .text_color(if active {
            rgb(theme.accent)
        } else {
            rgb(theme.text_muted)
        })
        .cursor_pointer()
        .hover(move |style| {
            if active {
                style
            } else {
                style.bg(rgb(theme.bg_hover)).text_color(rgb(theme.text))
            }
        })
        .child(child)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settled_transition_suppresses_stale_previous_index_motion() {
        let settled = SegmentedControlOptions::new(3, 0, 5);
        let user_transition = settled.user_transition_active(true);

        assert!(!should_animate_selection(settled, 3, 0));
        assert!(should_animate_selection(user_transition, 3, 0));
    }
}
