impl WorkspaceApp {
    pub(in crate::workspace) fn active_ai_reasoning_level(
        &self,
        provider: &AiProviderView,
        model: &str,
        cx: &App,
    ) -> AiReasoningLevel {
        if let Some(value) = self
            .ai.state
            .read(cx)
            .conversation_state()
            .active_conversation()
            .and_then(|conversation| {
                ai_conversation_reasoning_effort(conversation, &provider.id, model)
            })
        {
            return oxideterm_ai::normalize_reasoning_level_for_model(
                &provider.provider_type,
                model,
                value,
            );
        }
        let settings = self.settings_store.settings();
        let value = settings
            .ai
            .reasoning_model_overrides
            .get(&provider.id)
            .and_then(|models| models.get(model))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("auto");
        oxideterm_ai::normalize_reasoning_level_for_model(&provider.provider_type, model, value)
    }

    pub(in crate::workspace) fn render_ai_reasoning_indicator(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let settings = self.settings_store.settings();
        if settings.ai.active_backend == AiActiveBackend::Acp {
            return None;
        }
        let providers = ai_provider_views(&settings.ai.providers);
        let provider = active_provider_view(&providers, settings.ai.active_provider_id.as_deref())?;
        let model = active_model_selection(settings.ai.active_model.as_deref())?;
        let capability = model_reasoning_capability(&provider.provider_type, &model);
        if capability.levels.is_empty() {
            return None;
        }
        let selected = self.active_ai_reasoning_level(provider, &model, cx);
        let open = self.ai.state.read(cx).chat_ui().reasoning_menu_open;
        let trigger = div()
            .flex()
            .flex_none()
            .items_center()
            .rounded(px(self.tokens.radii.md))
            .px(px(self.tokens.spacing.one))
            .py(px(self.tokens.spacing.one / 2.0))
            .text_size(px(10.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(if open {
                rgb(self.tokens.ui.text)
            } else {
                rgb(self.tokens.ui.text_muted)
            })
            .bg(if open {
                rgba((self.tokens.ui.accent << 8) | 0x1a)
            } else {
                rgba(0x00000000)
            })
            .cursor_pointer()
            .hover(|style| {
                style
                    .bg(rgba((self.tokens.ui.accent << 8) | 0x1a))
                    .text_color(rgb(self.tokens.ui.text))
            })
            // The compact value stays English because it is a protocol level.
            .child(selected.display_name())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    let next_open = !this.ai.state.read(cx).chat_ui().reasoning_menu_open;
                    this.close_ai_sidebar_popovers(cx);
                    this.ai.state.update(cx, |ai, _cx| {
                        ai.set_chat_popover_open(AiChatPopover::Reasoning, next_open);
                    });
                    cx.stop_propagation();
                    cx.notify();
                }),
            );
        Some(
            select_anchor_probe(
                SelectAnchorId::AiReasoningMenu,
                trigger,
                Self::deferred_ai_select_anchor_update(cx.entity()),
            )
            .into_any_element(),
        )
    }

    pub(in crate::workspace) fn render_ai_reasoning_menu(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let settings = self.settings_store.settings();
        if settings.ai.active_backend == AiActiveBackend::Acp {
            return None;
        }
        let providers = ai_provider_views(&settings.ai.providers);
        let provider = active_provider_view(&providers, settings.ai.active_provider_id.as_deref())?;
        let model = active_model_selection(settings.ai.active_model.as_deref())?;
        let capability = model_reasoning_capability(&provider.provider_type, &model);
        if capability.levels.is_empty() {
            return None;
        }
        let selected = self.active_ai_reasoning_level(provider, &model, cx);
        let mut levels = vec![AiReasoningLevel::Auto];
        levels.extend(capability.levels);
        let mut menu = material_surface(&self.tokens, div(), MaterialRole::Popover)
            .w(px(AI_REASONING_MENU_WIDTH))
            .overflow_hidden()
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .shadow_lg()
            .on_scroll(|_, _, cx| cx.stop_propagation())
            .py(px(self.tokens.spacing.one))
            .child(
                div()
                    .px(px(self.tokens.spacing.three))
                    .py(px(self.tokens.spacing.one))
                    .text_size(px(12.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("ai.reasoning.title")),
            );
        for level in levels {
            let provider_id = provider.id.clone();
            let provider_type = provider.provider_type.clone();
            let model_for_click = model.clone();
            let label = self.ai_reasoning_level_display(level);
            let is_selected = selected == level;
            menu = menu.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(self.tokens.spacing.two))
                    .mx(px(self.tokens.spacing.one))
                    .rounded(px(self.tokens.radii.md))
                    .px(px(self.tokens.spacing.two))
                    .py(px(self.tokens.spacing.one + self.tokens.spacing.one / 2.0))
                    .text_size(px(12.0))
                    .text_color(rgb(self.tokens.ui.text))
                    .bg(if is_selected {
                        rgba((self.tokens.ui.accent << 8) | 0x1a)
                    } else {
                        rgba(0x00000000)
                    })
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
                    .child(div().w(px(14.0)).flex_none().when(is_selected, |check| {
                        check.child(Self::render_lucide_icon(
                            LucideIcon::Check,
                            14.0,
                            rgb(self.tokens.ui.accent),
                        ))
                    }))
                    .child(label)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.select_ai_reasoning_level(
                                provider_id.clone(),
                                provider_type.clone(),
                                model_for_click.clone(),
                                level,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ),
            );
        }
        if !capability.known_model {
            menu = menu.child(
                div()
                    .mt(px(self.tokens.spacing.one))
                    .border_t_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | 0x4d))
                    .px(px(self.tokens.spacing.three))
                    .pt(px(self.tokens.spacing.two))
                    .pb(px(self.tokens.spacing.one))
                    .text_size(px(9.0))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("ai.reasoning.custom_model_hint")),
            )
        }
        Some(menu.into_any_element())
    }

    fn ai_reasoning_level_display(&self, level: AiReasoningLevel) -> String {
        self.i18n
            .t(&format!("ai.reasoning.level_{}", level.as_str()))
    }
}
