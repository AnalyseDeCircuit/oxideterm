use std::time::Duration;

use gpui::{
    AnimationExt, AnyElement, Div, ElementId, IntoElement, Styled, div, prelude::*, px, relative,
    rgb, rgba,
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
    Underline { width: f32 },
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

    pub const fn underline(mut self, width: f32) -> Self {
        self.layout = SegmentedControlLayout::Underline { width };
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
    let animate = should_animate_selection(options, active_index, previous_index);
    let id = id.into();
    // The outer control already supplies its inset. The moving highlight must
    // fill the option cell so its size matches the pre-animation selection.
    let indicator = div()
        .absolute()
        .top(px(0.0))
        .bottom(px(0.0))
        .w(relative(item_width))
        .rounded(px(tokens.radii.md));
    let indicator = match options.layout {
        SegmentedControlLayout::Underline { .. } => indicator
            .top(gpui::auto())
            .h(px(2.0))
            .rounded_none()
            .bg(rgb(tokens.ui.accent)),
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
    let indicator = selection_indicator(tokens, id, indicator, previous_left, active_left, animate);

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
        SegmentedControlLayout::Compact { width } | SegmentedControlLayout::Underline { width } => {
            div().flex_none().w(px(width)).max_w_full().child(inner)
        }
    }
}

/// A stable position track carries velocity through repeated selections. Only
/// user navigation animates; restoring a page or changing policy settles it.
pub fn selection_indicator(
    tokens: &ThemeTokens,
    id: impl Into<ElementId>,
    indicator: Div,
    previous_left: f32,
    active_left: f32,
    animate: bool,
) -> AnyElement {
    let id = id.into();
    let mut policy =
        crate::motion::spatial_spring(tokens, crate::motion::MotionDuration::Control, active_left);
    if !animate {
        policy = policy.playback(gpui::SpringPlayback::Completed);
    }
    let reduced = tokens.motion.enabled && !tokens.motion.spatial_enabled && animate;
    let indicator = if reduced {
        crate::motion::fade_in(
            tokens,
            (id.clone(), format!("fade-{active_left}")),
            indicator,
            crate::motion::MotionDuration::Control,
        )
    } else {
        indicator.into_any_element()
    };
    // The outer positioned element remains the same in all four profiles.
    div()
        .absolute()
        .top_0()
        .bottom_0()
        .w_full()
        .child(indicator)
        .with_spring(
            (id, "position"),
            policy.from(previous_left),
            |element, left| element.left(relative(left)),
        )
        .into_any_element()
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
    use gpui::{Context, InteractiveElement, Render, TestAppContext, Window, size};

    struct SelectionView {
        index: usize,
        previous: usize,
        animated: bool,
    }

    impl Render for SelectionView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .w(px(200.0))
                .h(px(40.0))
                .relative()
                .child(selection_indicator(
                    &oxideterm_theme::default_tokens(),
                    "selection",
                    div()
                        .absolute()
                        .w(px(50.0))
                        .h_full()
                        .debug_selector(|| "indicator".into()),
                    self.previous as f32 / 4.0,
                    self.index as f32 / 4.0,
                    self.animated,
                ))
        }
    }

    #[gpui::test]
    fn selection_motion_reverses_from_current_position_and_programmatic_changes_snap(
        cx: &mut TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|_, _| SelectionView {
            index: 0,
            previous: 0,
            animated: false,
        });
        cx.simulate_resize(size(px(300.0), px(100.0)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        view.update(cx, |view, cx| {
            view.previous = 0;
            view.index = 3;
            view.animated = true;
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.executor().advance_clock(Duration::from_millis(50));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let moving = cx.debug_bounds("indicator").unwrap().left();
        assert!((f32::from(moving) - 107.0).abs() <= 0.6);
        view.update(cx, |view, cx| {
            view.previous = 3;
            view.index = 1;
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(cx.debug_bounds("indicator").unwrap().left(), moving);
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.draw(cx).clear(cx);
        });
        assert_eq!(cx.debug_bounds("indicator").unwrap().left(), px(50.0));
        cx.update(|window, cx| assert_eq!(window.simulate_next_frame(cx), 0));
        view.update(cx, |view, cx| {
            view.previous = 1;
            view.index = 2;
            view.animated = false;
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(cx.debug_bounds("indicator").unwrap().left(), px(100.0));
    }
    #[test]
    fn selection_motion_follows_all_four_profiles() {
        use oxideterm_theme::{UiMotion, UiMotionProfile, default_tokens};

        for (profile, expected) in [
            (UiMotionProfile::Off, None),
            (UiMotionProfile::Reduced, Some((120, false))),
            (UiMotionProfile::Normal, Some((200, true))),
            (UiMotionProfile::Fast, Some((110, true))),
        ] {
            let mut tokens = default_tokens();
            tokens.motion = UiMotion::from_profile(profile);
            assert_eq!(
                segmented_control_motion(&tokens)
                    .map(|motion| (motion.duration.as_millis(), motion.spatial)),
                expected,
                "{profile:?}"
            );
        }
    }

    #[test]
    fn settled_transition_suppresses_stale_previous_index_motion() {
        let settled = SegmentedControlOptions::new(3, 0, 5);
        let user_transition = settled.user_transition_active(true);

        assert!(!should_animate_selection(settled, 3, 0));
        assert!(should_animate_selection(user_transition, 3, 0));
    }
}
