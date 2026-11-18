impl AiModelPickerView {
    fn render_ai_model_selector(
        &self,
        scope: AiModelSelectorScope,
        anchor_id: SelectAnchorId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let providers = &self.display().providers;
        let enabled_providers = providers
            .iter()
            .filter(|provider| provider.enabled)
            .cloned()
            .collect::<Vec<_>>();
        if enabled_providers.is_empty() {
            return ai_model_selector_no_provider_button(
                &self.display().tokens,
                render_lucide_icon(
                    LucideIcon::Settings,
                    12.0,
                    rgb(self.display().tokens.ui.text_muted),
                ),
                self.display().i18n.t("ai.model_selector.no_provider"),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| {
                    (this.bindings.request)(
                        AiModelPickerRequest::Settings { close: false },
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }),
            )
            .into_any_element();
        }

        let active_provider_id = self.ai_active_model_selector_provider_id();
        let active_provider = active_provider_view(&providers, active_provider_id.as_deref());
        let display = if self.display().acp_active
            && let Some(agent_id) = self.display().active_acp_agent_id.as_deref()
        {
            self.active_ai_acp_session_state(agent_id, cx)
                .and_then(|state| {
                    let option = oxideterm_ai::acp_model_config_option(&state.config_options)?;
                    oxideterm_ai::acp_selected_config_choice(option, state.model_selection.as_ref())
                        .map(|choice| choice.label.clone())
                })
                .unwrap_or_else(|| self.ai_acp_agent_model_fallback_label(agent_id, cx))
        } else if self.display().acp_active {
            self.display()
                .i18n
                .t("ai.model_selector.agent_model_unavailable")
        } else {
            model_selector_display_name(self.display().active_model.as_deref())
                .unwrap_or_else(|| self.display().i18n.t("ai.model_selector.select_model"))
        };
        let selector_open = self.state.read(cx).model_selector_is_open(scope);
        let ready = active_provider
            .map(|provider| {
                self.ai_model_selector_has_key(provider, cx)
                    && self.ai_model_selector_provider_is_online(provider, cx)
            })
            .unwrap_or(false);
        let trigger = ai_model_selector_trigger_compact(
            &self.display().tokens,
            model_selector_truncated_label(&display),
            ready,
            selector_open,
            browser_focus_visible(
                selector_open,
                self.state.read(cx).model_selector_focus_origin(),
            ),
            self.render_animated_chevron(
                ("ai-model-selector-trigger-chevron", selector_open as usize),
                selector_open,
                12.0,
                rgb(self.display().tokens.ui.text_muted),
            ),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, window, cx| {
                this.state.update(cx, |ai, _cx| {
                    ai.set_model_selector_focus_origin(Some(BrowserFocusOrigin::Pointer));
                });
                (this.bindings.request)(AiModelPickerRequest::Toggle(scope), window, cx);
                cx.stop_propagation();
            }),
        );

        let set_anchor = self.bindings.anchor.clone();
        ai_model_selector_root()
            .child(select_anchor_probe(
                anchor_id,
                trigger,
                move |anchor, window, cx| set_anchor(anchor, window, cx),
            ))
            .into_any_element()
    }

    fn render_ai_model_selector_dropdown(
        &self,
        providers: &[AiProviderView],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        ai_model_selector_dropdown(&self.display().tokens, AiModelSelectorPlacement::Up)
            .child(self.render_ai_model_selector_search(cx))
            .child(self.render_ai_model_selector_list(providers, cx))
            .child(
                ai_model_selector_footer(
                    &self.display().tokens,
                    render_lucide_icon(
                        LucideIcon::Puzzle,
                        12.0,
                        rgb(self.display().tokens.ui.text_muted),
                    ),
                    self.display().i18n.t("ai.model_selector.manage_acp_agents"),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, window, cx| {
                        (this.bindings.request)(AiModelPickerRequest::ManageAcp(false), window, cx);
                        cx.stop_propagation();
                    }),
                ),
            )
            .child(
                ai_model_selector_footer(
                    &self.display().tokens,
                    render_lucide_icon(
                        LucideIcon::Plus,
                        12.0,
                        rgb(self.display().tokens.ui.text_muted),
                    ),
                    self.display()
                        .i18n
                        .t("ai.model_selector.add_custom_acp_agent"),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, window, cx| {
                        (this.bindings.request)(AiModelPickerRequest::ManageAcp(true), window, cx);
                        cx.stop_propagation();
                    }),
                ),
            )
            .child(
                ai_model_selector_footer(
                    &self.display().tokens,
                    render_lucide_icon(
                        LucideIcon::Settings,
                        12.0,
                        rgb(self.display().tokens.ui.text_muted),
                    ),
                    self.display().i18n.t("ai.model_selector.manage_providers"),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, window, cx| {
                        (this.bindings.request)(
                            AiModelPickerRequest::Settings { close: true },
                            window,
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                ),
            )
            .into_any_element()
    }

    fn render_ai_model_selector_search(&self, cx: &mut Context<Self>) -> AnyElement {
        let frame = self.display();
        let focused = self.state.read(cx).model_selector_search_focused();
        let marked_text = frame.marked.as_deref().unwrap_or_default();
        let showing_placeholder =
            self.state.read(cx).model_selector_search_query().is_empty() && marked_text.is_empty();
        let display_text = if showing_placeholder {
            self.display()
                .i18n
                .t("ai.model_selector.search_placeholder")
        } else {
            self.state.read(cx).model_selector_search_query().to_owned()
        };
        let input = div()
            .min_w_0()
            .flex_1()
            .h(px(18.0))
            .flex()
            .items_center()
            .overflow_hidden()
            .text_size(px(12.0))
            .text_color(if showing_placeholder {
                rgba((self.display().tokens.ui.text_muted << 8) | 0x99)
            } else {
                rgb(self.display().tokens.ui.text)
            })
            .cursor(CursorStyle::IBeam)
            .when(focused && showing_placeholder, |input| {
                input.child(text_caret(&self.display().tokens, frame.caret_visible))
            })
            .child(div().truncate().child(display_text))
            .when(focused && !marked_text.is_empty(), |input| {
                input.child(
                    div()
                        .underline()
                        .text_color(rgb(self.display().tokens.ui.text))
                        .child(marked_text.to_string()),
                )
            })
            .when(focused && !showing_placeholder, |input| {
                input.child(text_caret(&self.display().tokens, frame.caret_visible))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    this.state.update(cx, |ai, _cx| {
                        ai.set_model_selector_search_focused(true);
                    });
                    (this.bindings.request)(
                        AiModelPickerRequest::SearchPointer(event.clone()),
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                    (this.bindings.request)(
                        AiModelPickerRequest::SearchDrag(event.clone()),
                        window,
                        cx,
                    );
                }),
            );
        let set_anchor = self.bindings.text_anchor.clone();
        let input =
            text_input_anchor_probe(frame.search_anchor, input, move |anchor, window, cx| {
                set_anchor(anchor, window, cx)
            });
        let clear = (!self.state.read(cx).model_selector_search_query().is_empty()).then(|| {
            div()
                .size(px(14.0))
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.state.update(cx, |ai, _cx| {
                            ai.clear_model_selector_search();
                        });
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(render_lucide_icon(
                    LucideIcon::X,
                    12.0,
                    rgb(self.display().tokens.ui.text_muted),
                ))
                .into_any_element()
        });
        ai_model_selector_search_bar(&self.display().tokens, focused, input, clear)
            .into_any_element()
    }
}
