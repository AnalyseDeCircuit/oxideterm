impl WorkspaceApp {
    fn render_ai_model_selector(&self, cx: &mut Context<Self>) -> AnyElement {
        let providers = ai_provider_views(&self.settings_store.settings().ai.providers);
        let enabled_providers = providers
            .iter()
            .filter(|provider| provider.enabled)
            .cloned()
            .collect::<Vec<_>>();
        if enabled_providers.is_empty() {
            return ai_model_selector_no_provider_button(
                &self.tokens,
                Self::render_lucide_icon(LucideIcon::Settings, 12.0, rgb(self.tokens.ui.accent)),
                self.i18n.t("ai.model_selector.no_provider"),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| {
                    this.open_ai_settings(window, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element();
        }

        let active_provider = active_provider_view(
            &providers,
            self.settings_store
                .settings()
                .ai
                .active_provider_id
                .as_deref(),
        );
        let active_model = self.settings_store.settings().ai.active_model.as_deref();
        let display = model_selector_display_name(active_provider, active_model);
        let ready = active_provider
            .map(|provider| {
                self.ai_model_selector_has_key(provider)
                    && self.ai_model_selector_provider_is_online(provider)
            })
            .unwrap_or(false);
        let chevron = if self.ai_model_selector_open {
            LucideIcon::ChevronDown
        } else {
            LucideIcon::ChevronRight
        };
        let trigger = ai_model_selector_trigger_compact(
            &self.tokens,
            model_selector_truncated_label(&display),
            ready,
            self.ai_model_selector_open,
            Self::render_lucide_icon(chevron, 12.0, rgb(self.tokens.ui.text_muted)),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, window, cx| {
                this.toggle_ai_model_selector(window, cx);
                cx.stop_propagation();
            }),
        );

        let workspace = cx.entity();
        ai_model_selector_root()
            .child(select_anchor_probe(
                SelectAnchorId::AiModelSelector,
                trigger,
                move |anchor, _window, cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        this.update_select_anchor(anchor, cx);
                    });
                },
            ))
            .into_any_element()
    }

    fn render_ai_model_selector_dropdown(
        &self,
        providers: &[AiProviderView],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        ai_model_selector_dropdown(&self.tokens, AiModelSelectorPlacement::Up)
            .child(self.render_ai_model_selector_search(cx))
            .child(self.render_ai_model_selector_list(providers, cx))
            .child(
                ai_model_selector_footer(
                    &self.tokens,
                    Self::render_lucide_icon(
                        LucideIcon::Settings,
                        12.0,
                        rgb(self.tokens.ui.text_muted),
                    ),
                    self.i18n.t("ai.model_selector.manage_providers"),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, window, cx| {
                        this.ai_model_selector_open = false;
                        this.ai_model_selector_search_focused = false;
                        this.ai_model_selector_search_query.clear();
                        this.ai_model_selector_highlighted_model = None;
                        this.open_ai_settings(window, cx);
                        cx.stop_propagation();
                    }),
                ),
            )
            .into_any_element()
    }

    fn render_ai_model_selector_search(&self, cx: &mut Context<Self>) -> AnyElement {
        let target = WorkspaceImeTarget::AiModelSelectorSearch;
        let focused = self.ai_model_selector_search_focused;
        let marked_text = self.marked_text_for_target(target).unwrap_or_default();
        let showing_placeholder =
            self.ai_model_selector_search_query.is_empty() && marked_text.is_empty();
        let display_text = if showing_placeholder {
            self.i18n.t("ai.model_selector.search_placeholder")
        } else {
            self.ai_model_selector_search_query.clone()
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
                rgba((self.tokens.ui.text_muted << 8) | 0x99)
            } else {
                rgb(self.tokens.ui.text)
            })
            .cursor(CursorStyle::IBeam)
            .when(focused && showing_placeholder, |input| {
                input.child(text_caret(&self.tokens, self.new_connection_caret_visible))
            })
            .child(div().truncate().child(display_text))
            .when(focused && !marked_text.is_empty(), |input| {
                input.child(
                    div()
                        .underline()
                        .text_color(rgb(self.tokens.ui.text))
                        .child(marked_text.to_string()),
                )
            })
            .when(focused && !showing_placeholder, |input| {
                input.child(text_caret(&self.tokens, self.new_connection_caret_visible))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    this.ai_model_selector_search_focused = true;
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle);
                    this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                    this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                }),
            );
        let workspace = cx.entity();
        let input = text_input_anchor_probe(
            target.anchor_id(),
            input,
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        );
        let clear = (!self.ai_model_selector_search_query.is_empty()).then(|| {
            div()
                .size(px(14.0))
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.ai_model_selector_search_query.clear();
                        this.ai_model_selector_highlighted_model = None;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(Self::render_lucide_icon(
                    LucideIcon::X,
                    12.0,
                    rgb(self.tokens.ui.text_muted),
                ))
                .into_any_element()
        });
        ai_model_selector_search_bar(
            &self.tokens,
            Self::render_lucide_icon(LucideIcon::Search, 12.0, rgb(self.tokens.ui.text_muted)),
            input,
            clear,
        )
        .into_any_element()
    }

}
