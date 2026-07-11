use std::time::Duration;

use gpui::{
    Animation, AnimationExt, AnyElement, Div, ElementId, IntoElement, Styled, Svg, Transformation,
    radians, size,
};
use oxideterm_theme::ThemeTokens;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MotionDuration {
    Micro,
    Control,
    Overlay,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExitPhase {
    Visible,
    Exiting,
}

const FORM_ENTER_OFFSET_Y: f32 = 18.0;
const FORM_ENTER_SPRING_OVERSHOOT: f32 = 0.55;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExitPresence {
    phase: ExitPhase,
    generation: u64,
}

impl ExitPresence {
    pub fn visible() -> Self {
        Self {
            phase: ExitPhase::Visible,
            generation: 0,
        }
    }

    pub fn phase(self) -> ExitPhase {
        self.phase
    }

    pub fn begin_exit(&mut self) -> Option<u64> {
        if self.phase == ExitPhase::Exiting {
            return None;
        }
        self.generation = self.generation.wrapping_add(1);
        self.phase = ExitPhase::Exiting;
        Some(self.generation)
    }

    pub fn finish_exit(self, generation: u64) -> bool {
        self.phase == ExitPhase::Exiting && self.generation == generation
    }

    pub fn reopen(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.phase = ExitPhase::Visible;
    }
}

pub fn duration(tokens: &ThemeTokens, tier: MotionDuration) -> Duration {
    let millis = match tier {
        MotionDuration::Micro => tokens.motion.short_duration_ms,
        MotionDuration::Control => tokens.motion.normal_duration_ms,
        MotionDuration::Overlay => tokens.motion.long_duration_ms,
    };
    Duration::from_millis(millis)
}

pub fn scaled_duration(tokens: &ThemeTokens, normal_baseline_ms: u64) -> Duration {
    Duration::from_millis(tokens.motion.scaled_duration_ms(normal_baseline_ms))
}

pub fn ease_out_cubic(progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    1.0 - (1.0 - progress).powi(3)
}

pub fn ease_in_out_cubic(progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    if progress < 0.5 {
        4.0 * progress.powi(3)
    } else {
        1.0 - (-2.0 * progress + 2.0).powi(3) / 2.0
    }
}

pub fn spring_out(progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    if progress == 0.0 || progress == 1.0 {
        return progress;
    }
    let shifted = progress - 1.0;
    let cubic = FORM_ENTER_SPRING_OVERSHOOT + 1.0;
    1.0 + cubic * shifted.powi(3) + FORM_ENTER_SPRING_OVERSHOOT * shifted.powi(2)
}

pub fn lerp(from: f32, to: f32, progress: f32) -> f32 {
    from + (to - from) * progress.clamp(0.0, 1.0)
}

pub fn fade_in<E>(
    tokens: &ThemeTokens,
    id: impl Into<ElementId>,
    element: E,
    tier: MotionDuration,
) -> AnyElement
where
    E: IntoElement + Styled + 'static,
{
    fade(tokens, id, element, tier, true)
}

pub fn fade<E>(
    tokens: &ThemeTokens,
    id: impl Into<ElementId>,
    element: E,
    tier: MotionDuration,
    visible: bool,
) -> AnyElement
where
    E: IntoElement + Styled + 'static,
{
    let id = (id.into(), if visible { "visible" } else { "exiting" });
    if !tokens.motion.enabled {
        return element
            .opacity(if visible { 1.0 } else { 0.0 })
            .into_any_element();
    }
    element
        .with_animation(
            id,
            Animation::new(duration(tokens, tier)).with_easing(ease_out_cubic),
            move |element, progress| {
                element.opacity(if visible { progress } else { 1.0 - progress })
            },
        )
        .into_any_element()
}

pub fn slide_fade_in_y(
    tokens: &ThemeTokens,
    id: impl Into<ElementId>,
    element: Div,
    offset: f32,
    tier: MotionDuration,
) -> AnyElement {
    if !tokens.motion.enabled {
        return element.opacity(1.0).into_any_element();
    }
    let spatial = tokens.motion.spatial_enabled;
    element
        .with_animation(
            id,
            Animation::new(duration(tokens, tier)).with_easing(ease_out_cubic),
            move |element, progress| {
                let element = element.opacity(progress);
                if spatial {
                    element
                        .relative()
                        .top(gpui::px(lerp(offset, 0.0, progress)))
                } else {
                    element
                }
            },
        )
        .into_any_element()
}

pub fn form_enter(tokens: &ThemeTokens, id: impl Into<ElementId>, form: Div) -> AnyElement {
    form_transition(tokens, id, form, true)
}

pub fn form_transition(
    tokens: &ThemeTokens,
    id: impl Into<ElementId>,
    form: Div,
    visible: bool,
) -> AnyElement {
    let id = (id.into(), if visible { "visible" } else { "exiting" });
    if !tokens.motion.enabled {
        return form
            .opacity(if visible { 1.0 } else { 0.0 })
            .into_any_element();
    }
    let spatial = tokens.motion.spatial_enabled;
    form.with_animation(
        id,
        Animation::new(duration(tokens, MotionDuration::Overlay)),
        move |form, progress| {
            let visibility = if visible {
                ease_out_cubic(progress)
            } else {
                1.0 - ease_in_out_cubic(progress)
            };
            let form = form.opacity(visibility);
            if spatial {
                let spatial_progress = if visible {
                    spring_out(progress)
                } else {
                    1.0 - ease_in_out_cubic(progress)
                };
                form.relative()
                    .top(gpui::px(FORM_ENTER_OFFSET_Y * (1.0 - spatial_progress)))
            } else {
                form
            }
        },
    )
    .into_any_element()
}

pub fn slide_fade_in_x(
    tokens: &ThemeTokens,
    id: impl Into<ElementId>,
    element: Div,
    offset: f32,
    tier: MotionDuration,
) -> AnyElement {
    slide_fade_x(tokens, id, element, offset, tier, true)
}

pub fn slide_fade_x(
    tokens: &ThemeTokens,
    id: impl Into<ElementId>,
    element: Div,
    offset: f32,
    tier: MotionDuration,
    visible: bool,
) -> AnyElement {
    let id = (id.into(), if visible { "visible" } else { "exiting" });
    if !tokens.motion.enabled {
        return element
            .opacity(if visible { 1.0 } else { 0.0 })
            .into_any_element();
    }
    let spatial = tokens.motion.spatial_enabled;
    element
        .with_animation(
            id,
            Animation::new(duration(tokens, tier)).with_easing(ease_out_cubic),
            move |element, progress| {
                let visibility = if visible { progress } else { 1.0 - progress };
                let element = element.opacity(visibility);
                if spatial {
                    element
                        .relative()
                        .left(gpui::px(lerp(offset, 0.0, visibility)))
                } else {
                    element
                }
            },
        )
        .into_any_element()
}

pub fn horizontal_reveal(
    tokens: &ThemeTokens,
    id: impl Into<ElementId>,
    element: Div,
    expanded_width: f32,
    expanded: bool,
) -> AnyElement {
    // GPUI retains one-shot animation state by element ID, so the target state
    // must participate in the ID while remaining stable across ordinary redraws.
    let id = (id.into(), if expanded { "expanded" } else { "collapsed" });
    let final_width = if expanded { expanded_width } else { 0.0 };
    let final_opacity = if expanded { 1.0 } else { 0.0 };
    if !tokens.motion.enabled {
        return element
            .w(gpui::px(final_width))
            .opacity(final_opacity)
            .into_any_element();
    }
    let spatial = tokens.motion.spatial_enabled;
    element
        .overflow_hidden()
        .with_animation(
            id,
            Animation::new(duration(tokens, MotionDuration::Control))
                .with_easing(ease_in_out_cubic),
            move |element, progress| {
                let visibility = if expanded { progress } else { 1.0 - progress };
                let element = element.opacity(visibility);
                if spatial {
                    element.w(gpui::px(lerp(0.0, expanded_width, visibility)))
                } else {
                    // Reduced motion preserves layout and limits the transition to opacity.
                    element.w(gpui::px(expanded_width))
                }
            },
        )
        .into_any_element()
}

pub fn vertical_reveal(
    tokens: &ThemeTokens,
    id: impl Into<ElementId>,
    element: Div,
    expanded_height: f32,
    expanded: bool,
) -> AnyElement {
    // Keep the content mounted so the same stable element can animate in both directions.
    let id = (id.into(), if expanded { "expanded" } else { "collapsed" });
    let final_height = if expanded { expanded_height } else { 0.0 };
    let final_opacity = if expanded { 1.0 } else { 0.0 };
    if !tokens.motion.enabled {
        return element
            .h(gpui::px(final_height))
            .opacity(final_opacity)
            .into_any_element();
    }
    let spatial = tokens.motion.spatial_enabled;
    element
        .overflow_hidden()
        .with_animation(
            id,
            Animation::new(duration(tokens, MotionDuration::Micro)).with_easing(ease_in_out_cubic),
            move |element, progress| {
                let visibility = if expanded { progress } else { 1.0 - progress };
                let element = element.opacity(visibility);
                if spatial {
                    element.h(gpui::px(lerp(0.0, expanded_height, visibility)))
                } else {
                    // Reduced motion changes layout immediately and limits the visual transition.
                    element.h(gpui::px(final_height))
                }
            },
        )
        .into_any_element()
}

pub fn animated_checkmark(
    tokens: &ThemeTokens,
    id: impl Into<ElementId>,
    icon: Svg,
    checked: bool,
) -> AnyElement {
    // Restart only when the semantic state changes, not on every repaint.
    let id = (id.into(), if checked { "checked" } else { "unchecked" });
    if !tokens.motion.enabled {
        return icon
            .opacity(if checked { 1.0 } else { 0.0 })
            .into_any_element();
    }
    let spatial = tokens.motion.spatial_enabled;
    icon.with_animation(
        id,
        Animation::new(duration(tokens, MotionDuration::Micro)).with_easing(ease_out_cubic),
        move |icon, progress| {
            let visibility = if checked { progress } else { 1.0 - progress };
            let icon = icon.opacity(visibility);
            if spatial {
                let scale = lerp(0.92, 1.0, visibility);
                icon.with_transformation(Transformation::scale(size(scale, scale)))
            } else {
                icon
            }
        },
    )
    .into_any_element()
}

pub fn animated_chevron(
    tokens: &ThemeTokens,
    id: impl Into<ElementId>,
    icon: Svg,
    expanded: bool,
) -> AnyElement {
    // A completed expansion timeline cannot also drive the reverse transition.
    let id = (id.into(), if expanded { "expanded" } else { "collapsed" });
    let final_turn = if expanded { 0.25 } else { 0.0 };
    if !tokens.motion.enabled || !tokens.motion.spatial_enabled {
        return icon
            .with_transformation(Transformation::rotate(radians(
                final_turn * std::f32::consts::TAU,
            )))
            .into_any_element();
    }
    icon.with_animation(
        id,
        Animation::new(duration(tokens, MotionDuration::Control)).with_easing(ease_in_out_cubic),
        move |icon, progress| {
            let turn = if expanded {
                progress * 0.25
            } else {
                (1.0 - progress) * 0.25
            };
            icon.with_transformation(Transformation::rotate(radians(
                turn * std::f32::consts::TAU,
            )))
        },
    )
    .into_any_element()
}

pub fn animated_spinner(tokens: &ThemeTokens, id: impl Into<ElementId>, icon: Svg) -> AnyElement {
    if !tokens.motion.enabled {
        return icon.into_any_element();
    }
    icon.with_animation(
        id,
        Animation::new(duration(tokens, MotionDuration::Overlay))
            .repeat()
            .with_easing(|progress| progress),
        |icon, progress| {
            icon.with_transformation(Transformation::rotate(radians(
                progress * std::f32::consts::TAU,
            )))
        },
    )
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easing_and_interpolation_keep_exact_endpoints() {
        assert_eq!(ease_out_cubic(0.0), 0.0);
        assert_eq!(ease_out_cubic(1.0), 1.0);
        assert_eq!(ease_in_out_cubic(0.0), 0.0);
        assert_eq!(ease_in_out_cubic(1.0), 1.0);
        assert_eq!(spring_out(0.0), 0.0);
        assert_eq!(spring_out(1.0), 1.0);
        assert_eq!(lerp(4.0, 8.0, 0.0), 4.0);
        assert_eq!(lerp(4.0, 8.0, 1.0), 8.0);
    }

    #[test]
    fn spring_out_enters_quickly_and_settles_after_a_small_overshoot() {
        assert!(spring_out(0.5) > 0.9);
        assert!(spring_out(0.75) > 1.0);
        assert!(spring_out(0.75) < 1.03);
    }

    #[test]
    fn duration_tiers_follow_the_active_motion_profile() {
        let mut tokens = oxideterm_theme::default_tokens();
        assert_eq!(
            duration(&tokens, MotionDuration::Micro),
            Duration::from_millis(120)
        );
        assert_eq!(
            duration(&tokens, MotionDuration::Control),
            Duration::from_millis(200)
        );
        assert_eq!(
            duration(&tokens, MotionDuration::Overlay),
            Duration::from_millis(300)
        );

        tokens.apply_motion(oxideterm_theme::UiMotionProfile::Off);
        assert_eq!(duration(&tokens, MotionDuration::Overlay), Duration::ZERO);
    }

    #[test]
    fn horizontal_reveal_targets_match_visibility() {
        assert_eq!(lerp(0.0, 280.0, 0.0), 0.0);
        assert_eq!(lerp(0.0, 280.0, 1.0), 280.0);
    }

    #[test]
    fn vertical_reveal_targets_match_visibility() {
        assert_eq!(lerp(0.0, 28.0, 0.0), 0.0);
        assert_eq!(lerp(0.0, 28.0, 1.0), 28.0);
    }

    #[test]
    fn form_enter_has_a_visible_but_bounded_spatial_range() {
        assert!((12.0..=24.0).contains(&FORM_ENTER_OFFSET_Y));
    }

    #[test]
    fn exit_presence_rejects_stale_completion_after_reopen() {
        let mut presence = ExitPresence::visible();
        let generation = presence.begin_exit().expect("visible state can exit");
        assert_eq!(presence.phase(), ExitPhase::Exiting);
        presence.reopen();
        assert_eq!(presence.phase(), ExitPhase::Visible);
        assert!(!presence.finish_exit(generation));
    }
}
