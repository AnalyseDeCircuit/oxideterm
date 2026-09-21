use super::{MotionDuration, duration, ease_out_cubic};
use gpui::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, ParentElement, Pixels, Styled, Window, div, px,
};
use oxideterm_theme::ThemeTokens;
use std::{cell::RefCell, rc::Rc};

#[derive(Default)]
struct HeightState {
    from: f32,
    target: f32,
    current: f32,
    started_at: Option<std::time::Instant>,
}

/// Animates the occupied space of a bounded composer region. Removed content
/// is dropped immediately; element state retains only measured dimensions.
pub struct AutoHeight {
    id: ElementId,
    tokens: ThemeTokens,
    content: Option<AnyElement>,
    rendered: Option<AnyElement>,
}

pub fn auto_height(
    tokens: &ThemeTokens,
    id: impl Into<ElementId>,
    content: Option<AnyElement>,
) -> AutoHeight {
    AutoHeight {
        id: id.into(),
        tokens: *tokens,
        content,
        rendered: None,
    }
}

impl IntoElement for AutoHeight {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for AutoHeight {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let state =
            window.with_element_state(id.unwrap(), |state: Option<Rc<RefCell<HeightState>>>, _| {
                let state = state.unwrap_or_default();
                (state.clone(), state)
            });
        let animated =
            self.tokens.motion.enabled && self.tokens.motion.spatial_enabled && !cx.reduce_motion();
        let now = cx.background_executor().now();
        let mut current = state.borrow_mut();
        let running = if let Some(started_at) = current.started_at {
            let progress = (now - started_at).as_secs_f32()
                / duration(&self.tokens, MotionDuration::Micro)
                    .as_secs_f32()
                    .max(f32::EPSILON);
            current.current =
                current.from + (current.target - current.from) * ease_out_cubic(progress.min(1.0));
            if progress >= 1.0 {
                current.started_at = None;
            }
            progress < 1.0
        } else {
            false
        };
        let height = current.current;
        drop(current);
        if animated && running {
            window.request_animation_frame();
        }
        let measured = state.clone();
        let region = div()
            .w_full()
            .flex_none()
            .flex()
            .flex_col()
            .overflow_hidden()
            .children(
                self.content
                    .take()
                    .map(|content| div().w_full().flex_none().flex().flex_col().child(content)),
            )
            .on_children_prepainted(move |bounds, window, cx| {
                let height = bounds
                    .first()
                    .map_or(0.0, |bounds| f32::from(bounds.size.height));
                let mut state = measured.borrow_mut();
                if !animated {
                    state.from = height;
                    state.target = height;
                    state.current = height;
                    state.started_at = None;
                } else if (height - state.target).abs() > 0.1 {
                    state.from = state.current;
                    state.target = height;
                    state.started_at = Some(cx.background_executor().now());
                    window.request_animation_frame();
                }
            });
        // Keep the child ID path stable while height changes, preserving keyboard focus.
        let mut region = if animated {
            region.h(px(height))
        } else {
            region
        }
        .into_any_element();
        let layout = region.request_layout(window, cx);
        self.rendered = Some(region);
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
        self.rendered.as_mut().unwrap().prepaint(window, cx);
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
        self.rendered.as_mut().unwrap().paint(window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Context, InteractiveElement, Render, TestAppContext, size};
    use std::time::Duration;

    struct ComposerRegion {
        height: Option<f32>,
        tokens: ThemeTokens,
    }

    impl Render for ComposerRegion {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .flex()
                .flex_col()
                .child(auto_height(
                    &self.tokens,
                    "region",
                    self.height.map(|height| {
                        div()
                            .id("content")
                            .w_full()
                            .h(px(height))
                            .debug_selector(|| "content".into())
                            .into_any_element()
                    }),
                ))
                .child(
                    div()
                        .w_full()
                        .h(px(20.0))
                        .debug_selector(|| "editor".into()),
                )
        }
    }

    #[gpui::test]
    fn composer_height_reverses_without_retaining_removed_content(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| ComposerRegion {
            height: None,
            tokens: oxideterm_theme::default_tokens(),
        });
        cx.simulate_resize(size(px(400.0), px(400.0)));
        view.update(cx, |view, cx| {
            view.height = Some(80.0);
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(cx.debug_bounds("editor").unwrap().origin.y, px(0.0));
        cx.executor().advance_clock(Duration::from_millis(60));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(cx.debug_bounds("editor").unwrap().origin.y, px(70.0));
        view.update(cx, |view, cx| {
            view.height = None;
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(
            cx.debug_bounds("content").is_none(),
            "removed content must not be retained for exit animation"
        );
        assert_eq!(cx.debug_bounds("editor").unwrap().origin.y, px(70.0));
        cx.executor().advance_clock(Duration::from_millis(60));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!((f32::from(cx.debug_bounds("editor").unwrap().origin.y) - 8.75).abs() <= 0.5);
        cx.executor().advance_clock(Duration::from_millis(60));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(cx.debug_bounds("editor").unwrap().origin.y, px(0.0));
        cx.update(|_, cx| cx.set_reduce_motion(true));
        view.update(cx, |view, cx| {
            view.height = Some(80.0);
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(cx.debug_bounds("editor").unwrap().origin.y, px(80.0));
        cx.update(|_, cx| cx.set_reduce_motion(false));
        view.update(cx, |view, cx| {
            view.tokens.motion.enabled = false;
            view.height = Some(96.0);
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(cx.debug_bounds("editor").unwrap().origin.y, px(96.0));
    }
    #[gpui::test]
    #[ignore = "manual bounded composer layout benchmark"]
    fn composer_layout_benchmark(cx: &mut TestAppContext) {
        struct Bench {
            animated: bool,
            visible: bool,
        }
        impl Render for Bench {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                let content = self.visible.then(|| {
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .children((0..3).map(|_| div().h(px(28.0)).child("Queued message")))
                        .into_any_element()
                });
                let region = if self.animated {
                    auto_height(&oxideterm_theme::default_tokens(), "region", content)
                        .into_any_element()
                } else {
                    div().w_full().children(content).into_any_element()
                };
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(region)
                    .child(div().w_full().h(px(80.0)).child("Composer"))
            }
        }
        let (view, cx) = cx.add_window_view(|_, _| Bench {
            animated: false,
            visible: false,
        });
        cx.simulate_resize(size(px(400.0), px(600.0)));
        for animated in [false, true] {
            let mut samples = Vec::new();
            for cycle in 0..60 {
                view.update(cx, |view, cx| {
                    view.animated = animated;
                    view.visible = cycle % 2 == 0;
                    cx.notify();
                });
                for _ in 0..10 {
                    cx.executor().advance_clock(Duration::from_millis(16));
                    let start = std::time::Instant::now();
                    cx.update(|window, cx| window.draw(cx).clear(cx));
                    if cycle >= 10 {
                        samples.push(start.elapsed().as_nanos());
                    }
                }
            }
            samples.sort_unstable();
            println!(
                "composer animated={animated} median_ns={} p95_ns={}",
                samples[samples.len() / 2],
                samples[samples.len() * 95 / 100]
            );
        }
    }
}
