use gpui::{
    Animation, AnimationExt, AnyElement, App, Bounds, IntoElement, Pixels, Styled, Window,
    prelude::*, px,
};
use oxideterm_theme::ThemeTokens;
use std::{
    cell::Cell,
    collections::BTreeMap,
    hash::{Hash, Hasher},
    rc::Rc,
};

#[derive(Default)]
pub struct DisclosureMotions {
    generation: u64,
    entries: BTreeMap<String, DisclosureTransition>,
}

struct DisclosureTransition {
    generation: u64,
    expanded: bool,
    from: f32,
    opacity: Rc<Cell<f32>>,
}

struct VisualDisclosureElement {
    content: Option<gpui::Div>,
    opacity: f32,
    height: Option<f32>,
    offset_y: f32,
    rendered: Option<AnyElement>,
}

impl IntoElement for VisualDisclosureElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl gpui::Element for VisualDisclosureElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<gpui::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (gpui::LayoutId, ()) {
        let div = self
            .content
            .take()
            .expect("disclosure content")
            .opacity(self.opacity)
            .when_some(self.height, |content, height| content.h(px(height)));
        let mut rendered = div.into_any_element();
        let layout = rendered.request_layout(window, cx);
        self.rendered = Some(rendered);
        (layout, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_element_offset(gpui::point(px(0.0), px(self.offset_y)), |window| {
            self.rendered
                .as_mut()
                .expect("rendered disclosure")
                .prepaint(window, cx);
        });
    }

    fn paint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.rendered
            .as_mut()
            .expect("rendered disclosure")
            .paint(window, cx);
    }
}

impl DisclosureMotions {
    pub fn remove(&mut self, key: &str) {
        self.entries.remove(key);
    }

    pub fn begin(&mut self, key: String, expanded: bool) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        let from = self
            .entries
            .get(&key)
            .map_or(if expanded { 0.0 } else { 1.0 }, |entry| {
                entry.opacity.get()
            });
        self.entries.insert(
            key,
            DisclosureTransition {
                generation: self.generation,
                expanded,
                from,
                opacity: Rc::new(Cell::new(from)),
            },
        );
        self.generation
    }

    pub fn finish(&mut self, key: &str, generation: u64) -> bool {
        if self
            .entries
            .get(key)
            .is_some_and(|entry| entry.generation == generation)
        {
            self.entries.remove(key);
            true
        } else {
            false
        }
    }

    pub fn retained(&self, key: &str, expanded: bool) -> bool {
        expanded || self.entries.get(key).is_some_and(|entry| !entry.expanded)
    }

    pub fn signature(&self, owner_prefix: &str) -> u64 {
        if self.entries.is_empty() {
            return 0;
        }
        let mut matched = false;
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        for (key, entry) in self.entries.range(owner_prefix.to_owned()..) {
            if !key.starts_with(owner_prefix) {
                break;
            }
            matched = true;
            key.hash(&mut hash);
            entry.generation.hash(&mut hash);
        }
        if matched { hash.finish() } else { 0 }
    }

    pub fn message_signature(&self, id: &str) -> u64 {
        if self.entries.is_empty() {
            return 0;
        }
        self.signature(&format!("ai:{id}:"))
            ^ self.signature(&format!("ai:{id}-thinking-")).rotate_left(7)
            ^ self.signature(&format!("ai:{id}-tools-")).rotate_left(17)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn render(
        &self,
        key: &str,
        tokens: &ThemeTokens,
        content: gpui::Div,
        height: Option<f32>,
    ) -> AnyElement {
        let Some(entry) = self.entries.get(key) else {
            return content.into_any_element();
        };
        let from = entry.from;
        let target = if entry.expanded { 1.0 } else { 0.0 };
        let opacity = entry.opacity.clone();
        let spatial = tokens.motion.spatial_enabled;
        let height = height.filter(|_| spatial);
        VisualDisclosureElement {
            content: Some(content),
            opacity: from,
            height: height.map(|target_height| target_height * from),
            offset_y: 0.0,
            rendered: None,
        }
        .with_animation(
            (
                gpui::SharedString::from(key.to_owned()),
                entry.generation as usize,
            ),
            Animation::new(crate::motion::duration(
                tokens,
                crate::motion::MotionDuration::Micro,
            ))
            .with_easing(crate::motion::ease_out_cubic),
            move |mut element, progress| {
                let value = from + (target - from) * progress;
                opacity.set(value);
                element.opacity = value;
                element.height = height.map(|target_height| target_height * value);
                element.offset_y = if spatial && height.is_some() {
                    -6.0 * (1.0 - value)
                } else {
                    0.0
                };
                element
            },
        )
        .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    #[test]
    fn retained_disclosures_ignore_stale_completion_and_invalidate_only_their_owner() {
        let mut motions = DisclosureMotions::default();
        let close = motions.begin("ai:message-tools-0:call:tool".into(), false);
        assert!(motions.retained("ai:message-tools-0:call:tool", false));
        assert_ne!(motions.message_signature("message"), 0);
        assert_eq!(motions.message_signature("other-message"), 0);
        let reopen = motions.begin("ai:message-tools-0:call:tool".into(), true);
        motions.finish("ai:message-tools-0:call:tool", close);
        assert_ne!(motions.message_signature("message"), 0);
        motions.finish("ai:message-tools-0:call:tool", reopen);
        assert_eq!(motions.message_signature("message"), 0);
        assert!(!motions.retained("ai:message-tools-0:call:tool", false));
        let thinking = motions.begin("ai:message-thinking-activity-1:thinking".into(), true);
        assert_ne!(motions.message_signature("message"), 0);
        motions.finish("ai:message-thinking-activity-1:thinking", thinking);
        assert_eq!(motions.message_signature("message"), 0);
    }

    struct DisclosureTestRoot {
        motions: DisclosureMotions,
        expanded: bool,
        height: Option<f32>,
        tokens: ThemeTokens,
    }

    impl Render for DisclosureTestRoot {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .flex()
                .flex_col()
                .when(self.motions.retained("test", self.expanded), |root| {
                    let content = div()
                        .flex_none()
                        .overflow_hidden()
                        .debug_selector(|| "disclosure-body".into())
                        .children((0..3).map(|_| div().h(px(12.0)).flex_none().child("Content")));
                    root.child(
                        self.motions
                            .render("test", &self.tokens, content, self.height),
                    )
                })
                .child(
                    div()
                        .h(px(10.0))
                        .flex_none()
                        .debug_selector(|| "following-row".into()),
                )
        }
    }

    #[gpui::test]
    fn content_fades_without_moving_adjacent_rows_until_exit_finishes(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| DisclosureTestRoot {
            motions: DisclosureMotions::default(),
            expanded: true,
            height: None,
            tokens: oxideterm_theme::default_tokens(),
        });
        cx.simulate_resize(gpui::size(px(640.0), px(480.0)));
        let close = view.update(cx, |view, cx| {
            view.expanded = false;
            let generation = view.motions.begin("test".into(), false);
            cx.notify();
            generation
        });
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        cx.executor().advance_clock(Duration::from_millis(45));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        assert_eq!(cx.debug_bounds("following-row").unwrap().origin.y, px(36.0));
        let opacity = view.read_with(cx, |view, _| view.motions.entries["test"].opacity.get());
        assert!((opacity - 0.125).abs() < 0.0001);
        cx.executor().advance_clock(Duration::from_millis(45));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        assert_eq!(cx.debug_bounds("following-row").unwrap().origin.y, px(36.0));
        view.update(cx, |view, cx| {
            view.motions.finish("test", close);
            cx.notify();
        });
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        assert_eq!(cx.debug_bounds("following-row").unwrap().origin.y, px(0.0));
    }

    #[gpui::test]
    fn fixed_search_row_height_reverses_from_its_current_position(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| DisclosureTestRoot {
            motions: DisclosureMotions::default(),
            expanded: true,
            height: Some(36.0),
            tokens: oxideterm_theme::default_tokens(),
        });
        cx.simulate_resize(gpui::size(px(640.0), px(480.0)));
        view.update(cx, |view, cx| {
            view.motions.begin("test".into(), true);
            cx.notify();
        });
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        cx.executor().advance_clock(Duration::from_millis(45));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        let height = cx.debug_bounds("disclosure-body").unwrap().size.height;
        assert_eq!(height, px(31.5));
        view.update(cx, |view, cx| {
            view.expanded = false;
            view.motions.begin("test".into(), false);
            cx.notify();
        });
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        assert_eq!(
            cx.debug_bounds("disclosure-body").unwrap().size.height,
            height
        );
        cx.executor().advance_clock(Duration::from_millis(120));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        assert_eq!(
            cx.debug_bounds("disclosure-body").unwrap().size.height,
            px(0.0)
        );
    }

    #[gpui::test]
    #[ignore = "manual headless disclosure layout comparison"]
    fn disclosure_layout_benchmark(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| DisclosureTestRoot {
            motions: DisclosureMotions::default(),
            expanded: false,
            height: None,
            tokens: oxideterm_theme::default_tokens(),
        });
        cx.simulate_resize(gpui::size(px(640.0), px(480.0)));
        for animate in [false, true] {
            let mut samples = Vec::new();
            for cycle in 0..40 {
                let generation = view.update(cx, |view, cx| {
                    view.expanded = cycle % 2 == 0;
                    let generation = if animate {
                        view.motions.begin("test".into(), view.expanded)
                    } else {
                        0
                    };
                    cx.notify();
                    generation
                });
                for frame in 0..10 {
                    cx.executor().advance_clock(Duration::from_millis(16));
                    if animate && frame == 8 {
                        view.update(cx, |view, cx| {
                            view.motions.finish("test", generation);
                            cx.notify();
                        });
                    }
                    let start = std::time::Instant::now();
                    cx.update(|window, cx| {
                        window.draw(cx).clear(cx);
                    });
                    if cycle >= 10 {
                        samples.push(start.elapsed().as_nanos());
                    }
                }
            }
            samples.sort_unstable();
            println!(
                "disclosure animate={animate} frames={} median_ns={} p95_ns={}",
                samples.len(),
                samples[samples.len() / 2],
                samples[samples.len() * 95 / 100]
            );
        }
    }
}
