use std::{cell::Cell, rc::Rc};

use gpui::{Animation, AnimationExt, AnyElement, IntoElement};
use oxideterm_theme::ThemeTokens;

use super::{MotionDuration, duration};

// CSS cubic-bezier(0.25, 1, 0.5, 1). Its x derivative is at least 0.75,
// so bounded Newton iterations converge without a general curve solver.
fn sidebar_ease_out(progress: f32) -> f32 {
    let x = progress.clamp(0.0, 1.0);
    let mut t = x;
    for _ in 0..5 {
        t -= (0.75 * t + 0.25 * t * t * t - x) / (0.75 + 0.75 * t * t);
    }
    1.0 - (1.0 - t).powi(3)
}

/// Window-owned sidebar position, shared by the clipping viewport and resize edge.
/// Retargeting starts at the last drawn width, not at the previous endpoint.
pub struct SidebarMotion {
    from: f32,
    target: f32,
    current: Rc<Cell<f32>>,
    generation: usize,
}

impl SidebarMotion {
    pub fn new(width: f32) -> Self {
        Self {
            from: width,
            target: width,
            current: Rc::new(Cell::new(width)),
            generation: 0,
        }
    }

    pub fn retarget(&mut self, width: f32) {
        if self.target == width {
            return;
        }
        self.from = self.current.get();
        self.target = width;
        self.generation = self.generation.wrapping_add(1);
    }

    /// Manual resizing owns the width immediately and invalidates the old timeline.
    pub fn settle(&mut self, width: f32) {
        self.from = width;
        self.target = width;
        self.current.set(width);
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn animate<E: IntoElement + 'static>(
        &self,
        tokens: &ThemeTokens,
        id: &'static str,
        element: E,
        apply_width: impl Fn(E, f32) -> E + 'static,
    ) -> AnyElement {
        if !tokens.motion.enabled || !tokens.motion.spatial_enabled || self.from == self.target {
            self.current.set(self.target);
            return apply_width(element, self.target).into_any_element();
        }
        let from = self.from;
        let target = self.target;
        let current = self.current.clone();
        element
            .with_animation(
                (id, self.generation),
                Animation::new(duration(tokens, MotionDuration::Control))
                    .with_easing(sidebar_ease_out),
                move |element, progress| {
                    let width = from + (target - from) * progress;
                    current.set(width);
                    apply_width(element, width)
                },
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use gpui::{
        Context, InteractiveElement, ParentElement, Render, Styled, TestAppContext, div, px, size,
    };
    use std::time::Duration;

    struct SidebarTestView {
        motion: SidebarMotion,
        tokens: ThemeTokens,
    }

    impl Render for SidebarTestView {
        fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .relative()
                .child(
                    self.motion.animate(
                        &self.tokens,
                        "sidebar",
                        div()
                            .h_full()
                            .overflow_hidden()
                            .debug_selector(|| "sidebar-viewport".into())
                            .child(
                                div()
                                    .w(px(280.0))
                                    .h_full()
                                    .debug_selector(|| "sidebar-content".into()),
                            ),
                        |element, width| element.w(px(width)),
                    ),
                )
                .child(
                    self.motion.animate(
                        &self.tokens,
                        "edge",
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .w(px(9.0))
                            .debug_selector(|| "sidebar-edge".into()),
                        |element, width| element.left(px(width)),
                    ),
                )
        }
    }

    #[gpui::test]
    fn sidebar_reverses_at_drawn_width_and_resize_stops_both_tracks(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| SidebarTestView {
            motion: SidebarMotion::new(280.0),
            tokens: oxideterm_theme::default_tokens(),
        });
        cx.simulate_resize(size(px(800.0), px(600.0)));
        view.update(cx, |view, cx| {
            view.motion.retarget(0.0);
            cx.notify();
        });
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        cx.executor().advance_clock(Duration::from_millis(50));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        let closing_width = cx.debug_bounds("sidebar-viewport").unwrap().size.width;
        assert_eq!(closing_width, px(87.0));
        assert_eq!(
            cx.debug_bounds("sidebar-edge").unwrap().origin.x,
            closing_width
        );
        view.update(cx, |view, cx| {
            view.motion.retarget(280.0);
            cx.notify();
        });
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        assert_eq!(
            cx.debug_bounds("sidebar-viewport").unwrap().size.width,
            closing_width
        );
        cx.executor().advance_clock(Duration::from_millis(50));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        let opening_width = cx.debug_bounds("sidebar-viewport").unwrap().size.width;
        assert!(opening_width > closing_width && opening_width < px(280.0));
        view.update(cx, |view, cx| {
            view.motion.retarget(0.0);
            cx.notify();
        });
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        assert_eq!(
            cx.debug_bounds("sidebar-viewport").unwrap().size.width,
            opening_width
        );
        view.update(cx, |view, cx| {
            view.motion.settle(184.0);
            cx.notify();
        });
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        cx.executor().advance_clock(Duration::from_millis(500));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        assert_eq!(
            cx.debug_bounds("sidebar-viewport").unwrap().size.width,
            px(184.0)
        );
        assert_eq!(cx.debug_bounds("sidebar-edge").unwrap().origin.x, px(184.0));
        assert_eq!(
            cx.debug_bounds("sidebar-content").unwrap().size.width,
            px(280.0)
        );
    }

    #[gpui::test]
    fn reduced_motion_applies_sidebar_width_immediately(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| {
            let mut tokens = oxideterm_theme::default_tokens();
            tokens.apply_motion(oxideterm_theme::UiMotionProfile::Reduced);
            SidebarTestView {
                motion: SidebarMotion::new(280.0),
                tokens,
            }
        });
        cx.simulate_resize(size(px(800.0), px(600.0)));
        for width in [0.0, 280.0] {
            view.update(cx, |view, cx| {
                view.motion.retarget(width);
                cx.notify();
            });
            cx.update(|window, cx| {
                window.draw(cx).clear(cx);
            });
            assert_eq!(
                cx.debug_bounds("sidebar-viewport").unwrap().size.width,
                px(width)
            );
            assert_eq!(cx.debug_bounds("sidebar-edge").unwrap().origin.x, px(width));
        }
    }

    #[test]
    fn sidebar_curve_matches_css_reference_without_overshoot() {
        for (time, expected) in [
            (0.0, 0.0),
            (0.25, 0.6885898),
            (0.5, 0.9340958),
            (0.75, 0.9939447),
            (1.0, 1.0),
        ] {
            assert!(
                (sidebar_ease_out(time) - expected).abs() < 0.00001,
                "time={time}"
            );
        }
        let mut previous = 0.0;
        for step in 0..=200 {
            let value = sidebar_ease_out(step as f32 / 200.0);
            assert!(value >= previous && value <= 1.0);
            previous = value;
        }
    }
}
