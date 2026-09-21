use super::*;
use std::{
    cell::Cell,
    collections::BTreeMap,
    hash::{Hash, Hasher},
    rc::Rc,
};

#[derive(Default)]
pub(super) struct DisclosureMotions {
    generation: u64,
    entries: BTreeMap<String, DisclosureTransition>,
}

struct DisclosureTransition {
    generation: u64,
    expanded: bool,
    from: f32,
    opacity: Rc<Cell<f32>>,
}

impl DisclosureMotions {
    fn begin(&mut self, key: String, expanded: bool) -> u64 {
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

    fn finish(&mut self, key: &str, generation: u64) -> bool {
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

    pub(super) fn retained(&self, key: &str, expanded: bool) -> bool {
        expanded || self.entries.get(key).is_some_and(|entry| !entry.expanded)
    }

    pub(super) fn signature(&self, owner_prefix: &str) -> u64 {
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

    pub(super) fn message_signature(&self, id: &str) -> u64 {
        if self.entries.is_empty() {
            return 0;
        }
        self.signature(&format!("ai:{id}:"))
            ^ self.signature(&format!("ai:{id}-thinking-")).rotate_left(7)
            ^ self.signature(&format!("ai:{id}-tools-")).rotate_left(17)
    }

    pub(super) fn clear(&mut self) {
        self.entries.clear();
    }

    pub(super) fn render(
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
        let height = height.filter(|_| tokens.motion.spatial_enabled);
        content
            .with_animation(
                (
                    gpui::SharedString::from(key.to_owned()),
                    entry.generation as usize,
                ),
                Animation::new(oxideterm_gpui_ui::motion::duration(
                    tokens,
                    oxideterm_gpui_ui::motion::MotionDuration::Micro,
                ))
                .with_easing(oxideterm_gpui_ui::motion::ease_out_cubic),
                move |content, progress| {
                    let value = from + (target - from) * progress;
                    opacity.set(value);
                    content
                        .opacity(value)
                        .when_some(height, |content, height| content.h(px(height * value)))
                },
            )
            .into_any_element()
    }
}

impl WorkspaceApp {
    pub(super) fn begin_disclosure_motion(
        &mut self,
        key: String,
        expanded: bool,
        cx: &mut Context<Self>,
    ) {
        if !self.tokens.motion.enabled {
            self.disclosure_motions.entries.remove(&key);
            return;
        }
        let generation = self.disclosure_motions.begin(key.clone(), expanded);
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Micro,
        );
        // Only IDs and scalar motion state survive the transition, never message content.
        // The weak workspace handle prevents completion from retaining a closed window.
        cx.spawn(async move |weak, cx| {
            Timer::after(delay).await;
            let _ = weak.update(cx, |this, cx| {
                if this.disclosure_motions.finish(&key, generation) {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn toggle_ai_thinking_with_motion(
        &mut self,
        key: String,
        default_expanded: bool,
        cx: &mut Context<Self>,
    ) {
        let expanded = self.ai_entity.update(cx, |ai, _| {
            ai.toggle_thinking_expansion(key.clone(), default_expanded);
            ai.chat_ui()
                .thinking_expansion_state
                .get(&key)
                .copied()
                .unwrap_or(default_expanded)
        });
        self.begin_disclosure_motion(format!("ai:{key}:thinking"), expanded, cx);
    }

    pub(super) fn toggle_ai_tool_with_motion(&mut self, key: String, cx: &mut Context<Self>) {
        let expanded = self.ai_entity.update(cx, |ai, _| {
            ai.toggle_tool_call_expansion(key.clone());
            ai.chat_ui().tool_call_expansion_state.contains(&key)
        });
        self.begin_disclosure_motion(format!("ai:{key}:tool"), expanded, cx);
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
        let thinking = motions.begin("ai:message-thinking-1:thinking".into(), true);
        assert_ne!(motions.message_signature("message"), 0);
        motions.finish("ai:message-thinking-1:thinking", thinking);
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
        cx.executor().advance_clock(Duration::from_millis(60));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        assert_eq!(cx.debug_bounds("following-row").unwrap().origin.y, px(36.0));
        let opacity = view.read_with(cx, |view, _| view.motions.entries["test"].opacity.get());
        assert!((opacity - 0.125).abs() < 0.0001);
        cx.executor().advance_clock(Duration::from_millis(60));
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
        cx.executor().advance_clock(Duration::from_millis(60));
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
