use std::time::Duration;

use gpui::{
    Animation, AnimationExt, AnyElement, Div, ElementId, IntoElement, ParentElement,
    SpringAnimation, SpringConfig, SpringPlayback, Styled, Svg, Transformation, radians, size,
};
use oxideterm_theme::ThemeTokens;

mod auto_height;
pub use auto_height::auto_height;

mod sidebar;
pub use sidebar::SidebarMotion;

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

const FORM_ENTER_OFFSET_Y: f32 = 12.0;
// Ten time constants leave less than 0.1% displacement at the owner's deadline.
const SETTLE_RESPONSE: f32 = 10.0;

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

/// The response follows the existing duration tiers. Critical damping keeps
/// opacity and clipped geometry bounded while preserving velocity on reversal.
pub fn spring_config(tokens: &ThemeTokens, tier: MotionDuration) -> SpringConfig {
    let seconds = duration(tokens, tier).as_secs_f32().max(0.001);
    let frequency = SETTLE_RESPONSE / seconds;
    SpringConfig::new(frequency * frequency, 2.0 * frequency, 1.0)
}

pub fn spring(tokens: &ThemeTokens, tier: MotionDuration, target: f32) -> SpringAnimation<f32> {
    SpringAnimation::new(spring_config(tokens, tier))
        .to(target)
        .playback(if tokens.motion.enabled {
            SpringPlayback::Running
        } else {
            SpringPlayback::Completed
        })
}

pub fn spatial_spring(
    tokens: &ThemeTokens,
    tier: MotionDuration,
    target: f32,
) -> SpringAnimation<f32> {
    spring(tokens, tier, target).playback(
        if tokens.motion.enabled && tokens.motion.spatial_enabled {
            SpringPlayback::Running
        } else {
            SpringPlayback::Completed
        },
    )
}

/// GPUI fades individual primitives; fading the surface alone exposes its own
/// shadow through the fill. Delay shadow ink until the fill can cover it.
pub fn surface_opacity<E: Styled>(mut element: E, visibility: f32) -> E {
    let visibility = visibility.clamp(0.0, 1.0);
    if let Some(shadows) = element.style().box_shadow.as_mut() {
        for shadow in shadows {
            shadow.color.a *= visibility * visibility;
        }
    }
    element.opacity(visibility)
}

/// A bounded settle for finite handoffs that must finish before their owner
/// releases the old surface. Continuous controls use retained springs instead.
pub fn ease_settle(progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    if progress == 1.0 {
        return 1.0;
    }
    1.0 - (1.0 + SETTLE_RESPONSE * progress) * (-SETTLE_RESPONSE * progress).exp()
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
    element
        .with_spring(
            id,
            spring(tokens, tier, if visible { 1.0 } else { 0.0 }).from(if visible {
                0.0
            } else {
                1.0
            }),
            surface_opacity,
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
    let spatial = tokens.motion.spatial_enabled;
    element
        .with_spring(
            id,
            spring(tokens, tier, 1.0).from(0.0),
            move |element, progress| {
                let element = surface_opacity(element, progress);
                if spatial {
                    element.relative().top(gpui::px(offset * (1.0 - progress)))
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
    let spatial = tokens.motion.spatial_enabled;
    form.with_spring(
        id,
        spring(
            tokens,
            MotionDuration::Overlay,
            if visible { 1.0 } else { 0.0 },
        )
        .from(if visible { 0.0 } else { 1.0 }),
        move |form, progress| {
            let form = surface_opacity(form, progress);
            if spatial {
                form.relative()
                    .top(gpui::px(FORM_ENTER_OFFSET_Y * (1.0 - progress)))
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
    let spatial = tokens.motion.spatial_enabled;
    element
        .with_spring(
            id,
            spring(tokens, tier, if visible { 1.0 } else { 0.0 }).from(if visible {
                0.0
            } else {
                1.0
            }),
            move |element, progress| {
                let element = surface_opacity(element, progress);
                if spatial {
                    element.relative().left(gpui::px(offset * (1.0 - progress)))
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
    let spatial = tokens.motion.spatial_enabled;
    let target = if expanded { 1.0 } else { 0.0 };
    // Keep the child at natural width while the viewport clips its reveal.
    gpui::div()
        .h_full()
        .flex_none()
        .overflow_hidden()
        .child(element.w(gpui::px(expanded_width)).flex_none())
        .with_spring(
            id,
            spring(tokens, MotionDuration::Control, target),
            move |element, progress| {
                element.opacity(progress.clamp(0.0, 1.0)).w(gpui::px(
                    expanded_width * if spatial { progress.max(0.0) } else { target },
                ))
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
    let spatial = tokens.motion.spatial_enabled;
    let target = if expanded { 1.0 } else { 0.0 };
    element
        .overflow_hidden()
        .with_spring(
            id,
            spring(tokens, MotionDuration::Micro, target),
            move |element, progress| {
                element.opacity(progress.clamp(0.0, 1.0)).h(gpui::px(
                    expanded_height * if spatial { progress.max(0.0) } else { target },
                ))
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
    let spatial = tokens.motion.spatial_enabled;
    icon.with_spring(
        id,
        spring(
            tokens,
            MotionDuration::Micro,
            if checked { 1.0 } else { 0.0 },
        ),
        move |icon, visibility| {
            let icon = icon.opacity(visibility.clamp(0.0, 1.0));
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
    icon.with_spring(
        id,
        spatial_spring(
            tokens,
            MotionDuration::Control,
            if expanded { 1.0 } else { 0.0 },
        ),
        move |icon, progress| {
            icon.with_transformation(Transformation::rotate(radians(
                progress * std::f32::consts::FRAC_PI_2,
            )))
        },
    )
    .into_any_element()
}

pub fn animated_spinner(tokens: &ThemeTokens, id: impl Into<ElementId>, icon: Svg) -> AnyElement {
    if !tokens.motion.enabled || !tokens.motion.spatial_enabled {
        return icon.into_any_element();
    }
    icon.with_animation(
        id,
        Animation::new(scaled_duration(tokens, 900))
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
    use gpui::{Context, InteractiveElement, Render, TestAppContext, div, px, size};
    use std::{cell::Cell, rc::Rc};

    struct ProfileView {
        tokens: ThemeTokens,
        target: f32,
        opacity: Rc<Cell<f32>>,
    }

    impl Render for ProfileView {
        fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
            let opacity = self.opacity.clone();
            div().size_full().child(
                div()
                    .size(px(20.0))
                    .relative()
                    .debug_selector(|| "moving-control".into())
                    .with_spring(
                        "opacity",
                        spring(&self.tokens, MotionDuration::Control, self.target),
                        move |element, value| {
                            opacity.set(value);
                            element.opacity(value)
                        },
                    )
                    .with_spring(
                        "position",
                        spatial_spring(&self.tokens, MotionDuration::Control, self.target),
                        |element, value| {
                            element.map_element(|element| element.left(px(value * 100.0)))
                        },
                    ),
            )
        }
    }

    #[gpui::test]
    fn profiles_control_motion_and_stop_requesting_frames(cx: &mut TestAppContext) {
        use oxideterm_theme::UiMotionProfile;
        for (profile, expected_opacity, expected_x) in [
            (UiMotionProfile::Off, 1.0, 100.0),
            (UiMotionProfile::Reduced, 0.9596, 100.0),
            (UiMotionProfile::Normal, 0.8009, 80.0),
            (UiMotionProfile::Fast, 0.9724, 97.0),
        ] {
            let opacity = Rc::new(Cell::new(0.0));
            let (view, cx) = cx.add_window_view(|_, _| {
                let mut tokens = oxideterm_theme::default_tokens();
                tokens.apply_motion(profile);
                ProfileView {
                    tokens,
                    target: 0.0,
                    opacity: opacity.clone(),
                }
            });
            cx.simulate_resize(size(px(400.0), px(200.0)));
            cx.update(|window, cx| window.draw(cx).clear(cx));
            view.update(cx, |view, cx| {
                view.target = 1.0;
                cx.notify();
            });
            cx.update(|window, cx| window.draw(cx).clear(cx));
            cx.executor().advance_clock(Duration::from_millis(60));
            cx.update(|window, cx| window.draw(cx).clear(cx));
            assert!(
                (opacity.get() - expected_opacity).abs() < 0.001,
                "{profile:?}"
            );
            assert!(
                (f32::from(cx.debug_bounds("moving-control").unwrap().left()) - expected_x).abs()
                    <= 0.6,
                "{profile:?}"
            );
            // Changing the profile mid-flight must synchronize both channels.
            view.update(cx, |view, cx| {
                view.tokens.apply_motion(UiMotionProfile::Off);
                view.target = 0.5;
                cx.notify();
            });
            cx.update(|window, cx| {
                window.simulate_next_frame(cx);
                window.draw(cx).clear(cx);
            });
            assert_eq!(opacity.get(), 0.5);
            assert_eq!(cx.debug_bounds("moving-control").unwrap().left(), px(50.0));
            cx.update(|window, cx| assert_eq!(window.simulate_next_frame(cx), 0));
        }
    }

    #[test]
    fn surface_fade_keeps_shadow_behind_translucent_fill() {
        let shadow = gpui::BoxShadow {
            color: gpui::rgba(0x00000080).into(),
            offset: gpui::point(px(0.0), px(2.0)),
            blur_radius: px(8.0),
            spread_radius: px(0.0),
            inset: false,
        };
        let mut surface = surface_opacity(div().shadow(vec![shadow]), 0.5);
        assert_eq!(surface.style().opacity, Some(0.5));
        assert!((surface.style().box_shadow.as_ref().unwrap()[0].color.a - 0.12549).abs() < 0.0001);
    }

    #[test]
    fn finite_curves_preserve_endpoints_and_settle_without_overshoot() {
        for curve in [ease_out_cubic, ease_in_out_cubic, ease_settle] {
            assert_eq!(curve(0.0), 0.0);
            assert_eq!(curve(1.0), 1.0);
            let mut previous = 0.0;
            for step in 0..=100 {
                let value = curve(step as f32 / 100.0);
                assert!(value >= previous && value <= 1.0);
                previous = value;
            }
        }
        assert_eq!(lerp(4.0, 8.0, 0.0), 4.0);
        assert_eq!(lerp(4.0, 8.0, 1.0), 8.0);
    }

    struct CollapsedHorizontalReveal {
        tokens: ThemeTokens,
    }

    impl Render for CollapsedHorizontalReveal {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            _cx: &mut Context<Self>,
        ) -> impl IntoElement {
            div().child(horizontal_reveal(
                &self.tokens,
                "collapsed-horizontal-reveal-test",
                div()
                    .h(px(24.0))
                    .debug_selector(|| "reveal-content".to_string()),
                280.0,
                false,
            ))
        }
    }

    struct SidebarLayoutBench {
        expanded: bool,
        motion: SidebarMotion,
    }

    impl Render for SidebarLayoutBench {
        fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
            let content = div()
                .flex()
                .flex_col()
                .children((0..40).map(|_| div().h(px(24.0)).child("Session · connected")));
            let viewport = div()
                .h_full()
                .flex_none()
                .overflow_hidden()
                .child(content.w(px(280.0)).flex_none());
            div()
                .size_full()
                .flex()
                .child(self.motion.animate(
                    &oxideterm_theme::default_tokens(),
                    "sidebar-layout-bench",
                    viewport,
                    |element, width| element.w(px(width)),
                ))
                .child(div().flex_1().h_full())
        }
    }

    #[gpui::test]
    #[ignore = "manual headless sidebar layout benchmark"]
    fn sidebar_layout_benchmark(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| SidebarLayoutBench {
            expanded: false,
            motion: SidebarMotion::new(0.0),
        });
        cx.simulate_resize(size(px(1200.0), px(800.0)));
        let mut samples = Vec::new();
        for cycle in 0..40 {
            view.update(cx, |view, cx| {
                view.expanded = cycle % 2 == 0;
                view.motion
                    .retarget(if view.expanded { 280.0 } else { 0.0 });
                cx.notify();
            });
            for _ in 0..14 {
                cx.executor().advance_clock(Duration::from_millis(16));
                let started = std::time::Instant::now();
                cx.update(|window, cx| {
                    window.draw(cx).clear(cx);
                });
                if cycle >= 10 {
                    samples.push(started.elapsed().as_nanos());
                }
            }
        }
        samples.sort_unstable();
        println!(
            "sidebar-layout frames={} median_ns={} p95_ns={}",
            samples.len(),
            samples[samples.len() / 2],
            samples[samples.len() * 95 / 100]
        );
    }

    #[gpui::test]
    fn collapsed_horizontal_reveal_keeps_content_at_expanded_width(cx: &mut TestAppContext) {
        let (_, cx) = cx.add_window_view(|_, _| {
            let mut tokens = oxideterm_theme::default_tokens();
            tokens.apply_motion(oxideterm_theme::UiMotionProfile::Off);
            CollapsedHorizontalReveal { tokens }
        });
        cx.simulate_resize(size(px(640.0), px(480.0)));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });

        let content_bounds = cx
            .debug_bounds("reveal-content")
            .expect("reveal content bounds");
        assert_eq!(content_bounds.size.width, px(280.0));
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
