impl WorkspaceApp {
    pub(in crate::workspace) fn render_ai_model_selector(
        &self,
        scope: AiModelSelectorScope,
        anchor_id: SelectAnchorId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let providers = self.ai_model_selector_providers();
        let enabled_providers = providers
            .iter()
            .filter(|provider| provider.enabled)
            .cloned()
            .collect::<Vec<_>>();
        if enabled_providers.is_empty() {
            return ai_model_selector_no_provider_button(
                &self.tokens,
                Self::render_lucide_icon(
                    LucideIcon::Settings,
                    12.0,
                    rgb(self.tokens.ui.text_muted),
                ),
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

        let active_provider_id = self.ai_active_model_selector_provider_id();
        let active_provider = active_provider_view(&providers, active_provider_id.as_deref());
        let settings = self.settings_store.settings();
        let active_model = if settings.ai.active_backend == AiActiveBackend::Acp {
            None
        } else {
            settings.ai.active_model.as_deref()
        };
        let display = if settings.ai.active_backend == AiActiveBackend::Acp {
            settings
                .ai
                .active_acp_agent_id
                .as_deref()
                .and_then(|agent_id| self.active_ai_acp_session_state(agent_id))
                .and_then(|state| {
                    let option = oxideterm_ai::acp_model_config_option(&state.config_options)?;
                    oxideterm_ai::acp_selected_config_choice(
                        option,
                        state.model_selection.as_ref(),
                    )
                        .map(|choice| choice.label.clone())
                })
                .unwrap_or_else(|| {
                    settings
                        .ai
                        .active_acp_agent_id
                        .as_deref()
                        .map(|agent_id| self.ai_acp_agent_model_fallback_label(agent_id))
                        .unwrap_or_else(|| {
                            self.i18n.t("ai.model_selector.agent_model_unavailable")
                        })
                })
        } else {
            model_selector_display_name(active_provider, active_model)
        };
        let selector_open =
            self.ai.models.selector_open && self.ai.models.selector_scope == Some(scope);
        let ready = active_provider
            .map(|provider| {
                self.ai_model_selector_has_key(provider)
                    && self.ai_model_selector_provider_is_online(provider)
            })
            .unwrap_or(false);
        let trigger = ai_model_selector_trigger_compact(
            &self.tokens,
            model_selector_truncated_label(&display),
            ready,
            selector_open,
            browser_behavior::browser_focus_visible(
                selector_open,
                self.ai.models.selector_focus_origin,
            ),
            self.render_animated_chevron(
                (
                    "ai-model-selector-trigger-chevron",
                    selector_open as usize,
                ),
                selector_open,
                12.0,
                rgb(self.tokens.ui.text_muted),
            ),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, window, cx| {
                this.ai.models.selector_focus_origin =
                    Some(browser_behavior::BrowserFocusOrigin::Pointer);
                this.toggle_ai_model_selector(scope, window, cx);
                cx.stop_propagation();
            }),
        );

        let workspace = cx.entity();
        ai_model_selector_root()
            .child(select_anchor_probe(
                anchor_id,
                trigger,
                Self::deferred_ai_select_anchor_update(workspace),
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn render_ai_model_selector_dropdown(
        &self,
        providers: &[AiProviderView],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let dropdown = ai_model_selector_dropdown(&self.tokens, AiModelSelectorPlacement::Up)
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
                        this.close_ai_model_selector();
                        this.open_ai_settings(window, cx);
                        cx.stop_propagation();
                    }),
                ),
            );
        dropdown.into_any_element()
    }

    pub(in crate::workspace) fn render_ai_model_selector_search(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let target = WorkspaceImeTarget::AiModelSelectorSearch;
        let focused = self.ai.models.selector_search_focused;
        let marked_text = self.marked_text_for_target(target).unwrap_or_default();
        let showing_placeholder =
            self.ai.models.selector_search_query.is_empty() && marked_text.is_empty();
        let display_text = if showing_placeholder {
            self.i18n.t("ai.model_selector.search_placeholder")
        } else {
            self.ai.models.selector_search_query.clone()
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
                    this.ai.models.selector_search_focused = true;
                    this.ime_marked_text = None;
window.focus(&this.focus_handle, cx);
                    this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                    this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                }),
            );
        let input = text_input_anchor_probe(
            target.anchor_id(),
            input,
            Self::deferred_ai_text_input_anchor_update(cx.entity()),
        );
        let clear = (!self.ai.models.selector_search_query.is_empty()).then(|| {
            div()
                .size(px(14.0))
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.ai.models.selector_search_query.clear();
                        this.ai.models.selector_highlighted_model = None;
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
