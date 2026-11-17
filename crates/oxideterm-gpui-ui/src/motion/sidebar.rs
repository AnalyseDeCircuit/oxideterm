use std::{cell::Cell, rc::Rc};

use gpui::{
    AnimationExt, AnyElement, App, Bounds, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, ParentElement, Pixels, Styled, Window, div, point,
    px,
};
use oxideterm_theme::ThemeTokens;

use super::{MotionDuration, MotionPhase, phase_spatial_spring, visual_offset};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DrawerEdge {
    Left,
    Right,
}

const DRAWER_PARALLAX_RATIO: f32 = 0.25;

struct DrawerViewportElement {
    content: Option<AnyElement>,
    width: f32,
    shift_x: f32,
    rendered: Option<AnyElement>,
}

impl IntoElement for DrawerViewportElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for DrawerViewportElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let content = self.content.take().expect("drawer content");
        let child = if self.shift_x != 0.0 {
            visual_offset(content, point(px(self.shift_x), px(0.0)))
        } else {
            content
        };
        let mut rendered = div()
            .h_full()
            .flex_none()
            .overflow_hidden()
            .w(px(self.width))
            .child(child)
            .into_any_element();
        let layout = rendered.request_layout(window, cx);
        self.rendered = Some(rendered);
        (layout, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.rendered
            .as_mut()
            .expect("rendered drawer")
            .prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.rendered
            .as_mut()
            .expect("rendered drawer")
            .paint(window, cx);
    }
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
        let current = self.current.clone();
        let phase = if self.target < self.from {
            MotionPhase::Exit
        } else {
            MotionPhase::Enter
        };
        element
            .with_spring(
                (id, self.generation),
                phase_spatial_spring(tokens, MotionDuration::Control, phase, self.target)
                    .from(self.from)
                    .with_epsilon(0.1),
                move |element, width| {
                    current.set(width);
                    apply_width(element, width)
                },
            )
            .into_any_element()
    }

    /// Clips the drawer to the animated width while shifting its fixed-width
    /// content with `visual_offset` so opening and closing feel like a sliding drawer
    /// rather than static edge cropping.
    pub fn animate_drawer(
        &self,
        tokens: &ThemeTokens,
        id: &'static str,
        expanded_width: f32,
        edge: DrawerEdge,
        content: AnyElement,
    ) -> AnyElement {
        let current = self.current.clone();
        let spatial = tokens.motion.enabled && tokens.motion.spatial_enabled;
        let phase = if self.target < self.from {
            MotionPhase::Exit
        } else {
            MotionPhase::Enter
        };
        DrawerViewportElement {
            content: Some(content),
            width: self.from,
            shift_x: 0.0,
            rendered: None,
        }
        .with_spring(
            (id, self.generation),
            phase_spatial_spring(tokens, MotionDuration::Control, phase, self.target)
                .from(self.from)
                .with_epsilon(0.1),
            move |mut drawer, width| {
                current.set(width);
                let hidden = (expanded_width - width).max(0.0);
                drawer.width = width;
                drawer.shift_x = if spatial && hidden > 0.0 {
                    match edge {
                        DrawerEdge::Left => -(hidden * DRAWER_PARALLAX_RATIO),
                        DrawerEdge::Right => hidden * DRAWER_PARALLAX_RATIO,
                    }
                } else {
                    0.0
                };
                drawer
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
        assert!((f32::from(closing_width) - 53.3).abs() <= 0.6);
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
}
