#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AiStickyPrompt {
    list_index: usize,
    message_index: usize,
}

fn ai_sticky_prompt_overlay(
    mut list: AnyElement,
    state: gpui::ListState,
    prompts: Vec<AiStickyPrompt>,
    message_end: usize,
    mut render: impl FnMut(AiStickyPrompt, &mut Window, &mut App) -> Option<AnyElement> + 'static,
) -> impl IntoElement {
    gpui::canvas(
        move |bounds, window, cx| {
            list.layout_as_root(
                gpui::size(
                    gpui::AvailableSpace::Definite(bounds.size.width),
                    gpui::AvailableSpace::Definite(bounds.size.height),
                ),
                window,
                cx,
            );
            list.prepaint_at(bounds.origin, window, cx);
            // Resolve the sticky header after the list measures this frame's scroll position.
            let sticky = (|| {
                let top = state.logical_scroll_top().item_ix;
                if top >= message_end {
                    return None;
                }
                let next = prompts.partition_point(|prompt| prompt.list_index <= top);
                let prompt = *prompts.get(next.checked_sub(1)?)?;
                if prompt.list_index == top {
                    return None;
                }
                let mut content = render(prompt, window, cx)?;
                let size = content.layout_as_root(
                    gpui::size(
                        gpui::AvailableSpace::Definite(bounds.size.width),
                        gpui::AvailableSpace::MinContent,
                    ),
                    window,
                    cx,
                );
                // The next question pushes the old one out instead of being covered by it.
                let displacement = prompts
                    .get(next)
                    .and_then(|next| state.bounds_for_item(next.list_index))
                    .map_or(px(0.0), |next| {
                        (next.top() - bounds.top() - size.height).min(px(0.0))
                    });
                window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
                    content.prepaint_at(
                        bounds.origin + gpui::point(px(0.0), displacement),
                        window,
                        cx,
                    );
                });
                Some(content)
            })();
            (list, sticky)
        },
        move |bounds, (mut list, sticky), window, cx| {
            // The material samples the already-painted list. Its opaque fallback
            // covers the same region when the render profile disables blur.
            window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
                list.paint(window, cx);
            });
            if let Some(mut content) = sticky {
                window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
                    content.paint(window, cx)
                });
            }
        },
    )
    .absolute()
    .inset_0()
}

impl WorkspaceApp {
    fn render_ai_sticky_prompt(
        &self,
        conversation_id: &str,
        prompt: AiStickyPrompt,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let ai = self.ai_entity.read(cx);
        let conversation = ai.conversation_state().active_conversation()?;
        if conversation.id != conversation_id {
            return None;
        }
        let message = conversation.messages.get(prompt.message_index)?;
        if message.role != AiChatRole::User {
            return None;
        }
        // GPUI owns this bounded display copy for the current frame only. No
        // prompt body is retained in overlay state, diagnostics or async tasks.
        let mut preview: String = message.content.chars().take(1024).collect();
        if preview.len() < message.content.len() {
            preview.push('…');
        }
        let state = ai.chat_ui().message_list_state.clone();
        let wheel_state = state.clone();
        let label = self.i18n.t("ai.chat.back_to_question");
        let tokens = self.tokens;
        let body = div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(tokens.spacing.two))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(13.0))
                    .line_height(px(20.0))
                    .text_color(rgb(tokens.ui.text))
                    .line_clamp(3)
                    .child(preview),
            )
            .child(Self::render_lucide_icon(
                LucideIcon::ArrowUp,
                12.0,
                rgb(tokens.ui.text_muted),
            ));
        let conversation_id = conversation_id.to_owned();
        let button = self
            .agent_control(
                "ai-sticky-question".into(),
                label.clone(),
                body.cursor_pointer(),
                move |this, _, cx| {
                    if this
                        .ai_entity
                        .read(cx)
                        .conversation_state()
                        .active_conversation_id
                        .as_deref()
                        == Some(&conversation_id)
                    {
                        state.scroll_to(gpui::ListOffset {
                            item_ix: prompt.list_index,
                            offset_in_item: px(0.0),
                        });
                        cx.notify();
                    }
                },
                cx,
            )
            .tooltip(move |_, cx| {
                oxideterm_gpui_ui::tooltip::tooltip_view(tokens, label.clone(), None, cx)
            });
        Some(
            material_surface(
                &tokens,
                div(),
                MaterialRole::StickyHeader,
            )
            .w_full()
            .px(px(tokens.spacing.three))
            .py(px(tokens.spacing.two))
            .border_b_1()
            .border_color(self.workspace_chrome_divider())
            .occlude()
            .on_scroll_wheel(
                cx.listener(move |_, event: &gpui::ScrollWheelEvent, _, cx| {
                    wheel_state.scroll_by(-event.delta.pixel_delta(px(20.0)).y);
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .child(button)
            .into_any_element(),
        )
    }
}

#[cfg(test)]
mod sticky_prompt_tests {
    use super::*;
    use gpui::{ListAlignment, ListOffset, ListState, Render, TestAppContext, size};

    struct ChatViewport {
        state: ListState,
        shown: std::rc::Rc<std::cell::Cell<Option<usize>>>,
        prompts: Vec<AiStickyPrompt>,
        paint_top: Option<std::rc::Rc<std::cell::Cell<f32>>>,
    }

    fn test_prompts() -> Vec<AiStickyPrompt> {
        (0..120)
            .step_by(2)
            .map(|index| AiStickyPrompt {
                list_index: index,
                message_index: index,
            })
            .collect()
    }

    impl Render for ChatViewport {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.shown.set(None);
            let shown = self.shown.clone();
            let state = self.state.clone();
            let paint_top = self.paint_top.clone();
            let list = gpui::list(self.state.clone(), move |index, _, _| {
                div()
                    .w_full()
                    .h(px(if index % 2 == 0 { 80.0 } else { 400.0 }))
                    .child(format!("Message {index}"))
                    .when_some(paint_top.clone(), |row, top| {
                        row.child(
                            gpui::canvas(
                                |_, _, _| (),
                                move |_, _, window, _| {
                                    top.set(f32::from(window.content_mask().bounds.top()));
                                },
                            )
                            .absolute()
                            .inset_0(),
                        )
                    })
                    .into_any_element()
            })
            .size_full()
            .into_any_element();
            let sticky = ai_sticky_prompt_overlay(
                list,
                self.state.clone(),
                self.prompts.clone(),
                120,
                move |prompt, _, _| {
                    shown.set(Some(prompt.message_index));
                    let state = state.clone();
                    Some(
                        material_surface(
                            &oxideterm_theme::default_tokens(),
                            div(),
                            MaterialRole::StickyHeader,
                        )
                        .id("sticky-question")
                        .w_full()
                        .h(px(60.0))
                        .debug_selector(|| "sticky-question".into())
                        .child(format!("Question {}", prompt.message_index))
                        .on_click(move |_, window, _| {
                            state.scroll_to(ListOffset {
                                item_ix: prompt.list_index,
                                offset_in_item: px(0.0),
                            });
                            window.refresh();
                        })
                        .into_any_element(),
                    )
                },
            );
            div().relative().size_full().child(sticky)
        }
    }

    #[gpui::test]
    fn sticky_question_tracks_the_drawn_list_and_yields_to_the_next_question(
        cx: &mut TestAppContext,
    ) {
        let state = ListState::new(120, ListAlignment::Top, px(500.0));
        let shown = std::rc::Rc::new(std::cell::Cell::new(None));
        let paint_top = std::rc::Rc::new(std::cell::Cell::new(0.0));
        let (view, cx) = cx.add_window_view(|_, _| ChatViewport {
            state: state.clone(),
            shown: shown.clone(),
            prompts: test_prompts(),
            paint_top: Some(paint_top.clone()),
        });
        cx.simulate_resize(size(px(500.0), px(700.0)));
        for (row, offset, expected, top) in [
            (0, 0.0, None, None),
            (0, 40.0, None, None),
            (1, 80.0, Some(0), Some(0.0)),
            (1, 350.0, Some(0), Some(-10.0)),
            (2, 0.0, None, None),
            (3, 80.0, Some(2), Some(0.0)),
        ] {
            state.scroll_to(ListOffset {
                item_ix: row,
                offset_in_item: px(offset),
            });
            view.update(cx, |_, cx| cx.notify());
            cx.update(|window, cx| window.draw(cx).clear(cx));
            assert_eq!(shown.get(), expected, "row={row} offset={offset}");
            assert_eq!(
                paint_top.get(),
                0.0,
                "the material needs the list painted underneath before sampling"
            );
            assert_eq!(
                cx.debug_bounds("sticky-question")
                    .map(|bounds| f32::from(bounds.top())),
                top
            );
        }
        cx.simulate_click(gpui::point(px(10.0), px(10.0)), gpui::Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(state.logical_scroll_top().item_ix, 2);
        assert_eq!(state.logical_scroll_top().offset_in_item, px(0.0));
        assert!(cx.debug_bounds("sticky-question").is_none());
        // History controls occupy list rows but are not conversation messages.
        view.update(cx, |view, cx| {
            view.prompts = vec![AiStickyPrompt {
                list_index: 2,
                message_index: 0,
            }];
            cx.notify();
        });
        state.scroll_to(ListOffset {
            item_ix: 3,
            offset_in_item: px(80.0),
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(shown.get(), Some(0));
        view.update(cx, |view, cx| {
            view.prompts.clear();
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(cx.debug_bounds("sticky-question").is_none());
    }

    #[gpui::test]
    #[ignore = "manual chat scroll layout benchmark"]
    fn sticky_prompt_scroll_benchmark(cx: &mut TestAppContext) {
        let state = ListState::new(120, ListAlignment::Top, px(500.0));
        let (view, cx) = cx.add_window_view(|_, _| ChatViewport {
            state: state.clone(),
            shown: Default::default(),
            prompts: test_prompts(),
            paint_top: None,
        });
        cx.simulate_resize(size(px(500.0), px(700.0)));
        let mut samples = Vec::new();
        for frame in 0..360 {
            state.scroll_to(ListOffset {
                item_ix: (frame / 3) % 116,
                offset_in_item: px((frame % 3) as f32 * 20.0),
            });
            view.update(cx, |_, cx| cx.notify());
            let started = std::time::Instant::now();
            cx.update(|window, cx| window.draw(cx).clear(cx));
            if frame >= 60 {
                samples.push(started.elapsed().as_nanos());
            }
        }
        samples.sort_unstable();
        println!(
            "chat-scroll median_ns={} p95_ns={}",
            samples[samples.len() / 2],
            samples[samples.len() * 95 / 100]
        );
    }
}
