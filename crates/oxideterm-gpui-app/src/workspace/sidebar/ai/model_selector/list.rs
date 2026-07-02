impl WorkspaceApp {
    fn render_ai_model_selector_list(
        &self,
        providers: &[AiProviderView],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let groups =
            model_selector_visible_provider_groups(providers, &self.ai_model_selector_search_query);
        let mut list = ai_model_selector_list("ai-model-selector-list");
        if groups.is_empty() {
            return list
                .child(ai_model_selector_empty_search(
                    &self.tokens,
                    self.i18n.t("ai.model_selector.no_search_results"),
                ))
                .into_any_element();
        }

        for (index, group) in groups.into_iter().enumerate() {
            let provider = group.provider;
            let has_key = self.ai_model_selector_has_key(&provider);
            let online = self.ai_model_selector_provider_is_online(&provider);
            let expanded = !self.ai_model_selector_search_query.trim().is_empty()
                || self
                    .ai_model_selector_expanded_providers
                    .contains(&provider.id);
            let active_provider = self
                .ai_active_model_selector_provider_id()
                .as_deref()
                == Some(provider.id.as_str());
            let active_provider_model = (active_provider
                && Self::ai_acp_agent_id_from_provider_id(&provider.id).is_none())
                .then(|| {
                    self.settings_store
                        .settings()
                        .ai
                        .active_model
                        .as_deref()
                        .and_then(|model| model.rsplit('/').next())
                        .map(str::to_string)
                })
                .flatten();
            let status = self.render_ai_model_selector_provider_status(&provider, has_key, online);
            let refresh = (has_key
                && online
                && Self::ai_acp_agent_id_from_provider_id(&provider.id).is_none())
                .then(|| {
                let provider_for_refresh = provider.clone();
                ai_model_selector_refresh_button(
                    &self.tokens,
                    Self::render_lucide_icon(
                        LucideIcon::RefreshCw,
                        10.0,
                        rgb(self.tokens.ui.text_muted),
                    ),
                )
                .opacity(if self.ai_model_refreshing.contains(&provider.id) {
                    0.45
                } else {
                    1.0
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.refresh_ai_provider_from_selector(provider_for_refresh.clone(), cx);
                        cx.stop_propagation();
                    }),
                )
                .into_any_element()
            });

            let provider_id = provider.id.clone();
            let header = ai_model_selector_provider_header(
                &self.tokens,
                provider.name.clone(),
                Self::render_lucide_icon(
                    if expanded {
                        LucideIcon::ChevronDown
                    } else {
                        LucideIcon::ChevronRight
                    },
                    12.0,
                    rgb(self.tokens.ui.accent),
                ),
                active_provider_model,
                status,
                refresh,
                has_key,
                index == 0,
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if !this
                        .ai_model_selector_expanded_providers
                        .remove(&provider_id)
                    {
                        this.ai_model_selector_expanded_providers
                            .insert(provider_id.clone());
                    }
                    // Collapsing/expanding changes which model rows are
                    // focusable, so clear the active item like Radix does when
                    // menu content is restructured.
                    this.ai_model_selector_highlighted_model = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            );

            let mut section = div().child(header);
            if expanded {
                section = section.child(self.render_ai_model_selector_models(
                    provider,
                    group.visible_models,
                    has_key,
                    online,
                    cx,
                ));
            }
            list = list.child(section);
        }
        list.into_any_element()
    }

    fn render_ai_model_selector_provider_status(
        &self,
        provider: &AiProviderView,
        has_key: bool,
        online: bool,
    ) -> AnyElement {
        match resolve_model_selector_provider_probe(provider) {
            ModelSelectorProviderProbe::ImplicitKey { .. } => ai_model_selector_local_status(
                &self.tokens,
                online,
                if online {
                    self.i18n.t("ai.model_selector.ok")
                } else {
                    self.i18n.t("ai.model_selector.offline")
                },
            )
            .into_any_element(),
            _ => ai_model_selector_key_status(
                &self.tokens,
                has_key,
                Self::render_lucide_icon(
                    LucideIcon::Key,
                    10.0,
                    rgb(if has_key { 0x34d399 } else { 0xfbbf24 }),
                ),
                if has_key {
                    self.i18n.t("ai.model_selector.ok")
                } else {
                    self.i18n.t("ai.model_selector.no_key")
                },
            )
            .into_any_element(),
        }
    }


}
