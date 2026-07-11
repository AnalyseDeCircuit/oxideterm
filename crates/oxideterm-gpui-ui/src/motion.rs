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
    if !tokens.motion.enabled {
        return element.opacity(1.0).into_any_element();
    }
    element
        .with_animation(
            id,
            Animation::new(duration(tokens, tier)).with_easing(ease_out_cubic),
            |element, progress| element.opacity(progress),
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

pub fn slide_fade_in_x(
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
                        .left(gpui::px(lerp(offset, 0.0, progress)))
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
        assert_eq!(lerp(4.0, 8.0, 0.0), 4.0);
        assert_eq!(lerp(4.0, 8.0, 1.0), 8.0);
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
}
