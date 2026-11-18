impl AiModelPickerView {
    fn render_ai_model_selector_list(
        &self,
        providers: &[AiProviderView],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let search_query = self.state.read(cx).model_selector_search_query().to_owned();
        let groups = model_selector_visible_provider_groups(providers, &search_query);
        let mut list = ai_model_selector_list("ai-model-selector-list");
        if groups.is_empty() {
            return list
                .child(ai_model_selector_empty_search(
                    &self.display().tokens,
                    self.display().i18n.t("ai.model_selector.no_search_results"),
                ))
                .into_any_element();
        }

        for (index, group) in groups.into_iter().enumerate() {
            let provider = group.provider;
            let has_key = self.ai_model_selector_has_key(&provider, cx);
            let online = self.ai_model_selector_provider_is_online(&provider, cx);
            let expanded = !search_query.trim().is_empty()
                || self
                    .state
                    .read(cx)
                    .model_selector_provider_expanded(&provider.id);
            let active_provider = self.ai_active_model_selector_provider_id().as_deref()
                == Some(provider.id.as_str());
            let active_provider_model = (active_provider
                && ai_acp_agent_id_from_provider_id(&provider.id).is_none())
            .then(|| {
                self.display()
                    .active_model
                    .as_deref()
                    .and_then(|model| model.rsplit('/').next())
                    .map(str::to_string)
            })
            .flatten();
            let status = self.render_ai_model_selector_provider_status(&provider, has_key, online);
            let refresh = (has_key
                && online
                && ai_acp_agent_id_from_provider_id(&provider.id).is_none())
            .then(|| {
                let provider_for_refresh = provider.clone();
                ai_model_selector_refresh_button(
                    &self.display().tokens,
                    render_lucide_icon(
                        LucideIcon::RefreshCw,
                        10.0,
                        rgb(self.display().tokens.ui.text_muted),
                    ),
                )
                .opacity(if self.state.read(cx).model_is_refreshing(&provider.id) {
                    0.45
                } else {
                    1.0
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, window, cx| {
                        (this.bindings.request)(
                            AiModelPickerRequest::Refresh(provider_for_refresh.clone()),
                            window,
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                )
                .into_any_element()
            });

            let provider_id = provider.id.clone();
            let header = ai_model_selector_provider_header(
                &self.display().tokens,
                provider.name.clone(),
                self.render_animated_chevron(
                    (
                        gpui::SharedString::from(format!("model-provider-chevron-{}", provider.id)),
                        expanded as usize,
                    ),
                    expanded,
                    12.0,
                    rgb(self.display().tokens.ui.accent),
                ),
                active_provider_model,
                status,
                refresh,
                has_key,
                index == 0,
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    let expanded = this.state.update(cx, |ai, _cx| {
                        ai.toggle_model_selector_provider(provider_id.clone())
                    });
                    if expanded
                        && let Some(agent_id) = ai_acp_agent_id_from_provider_id(&provider_id)
                    {
                        (this.bindings.request)(
                            AiModelPickerRequest::Discover(agent_id.to_string()),
                            window,
                            cx,
                        );
                    }
                    // Collapsing/expanding changes which model rows are
                    // focusable, so clear the active item like Radix does when
                    // menu content is restructured.
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
                &self.display().tokens,
                online,
                if online {
                    self.display().i18n.t("ai.model_selector.ok")
                } else {
                    self.display().i18n.t("ai.model_selector.offline")
                },
            )
            .into_any_element(),
            _ if !ai_provider_chat_requires_key(&provider.provider_type) => {
                ai_model_selector_local_status(
                    &self.display().tokens,
                    online,
                    if online {
                        self.display().i18n.t("ai.model_selector.ok")
                    } else {
                        self.display().i18n.t("ai.model_selector.offline")
                    },
                )
                .into_any_element()
            }
            _ => ai_model_selector_key_status(
                &self.display().tokens,
                has_key,
                render_lucide_icon(
                    LucideIcon::Key,
                    10.0,
                    rgb(if has_key {
                        self.display().tokens.ui.success
                    } else {
                        self.display().tokens.ui.warning
                    }),
                ),
                if has_key {
                    self.display().i18n.t("ai.model_selector.ok")
                } else {
                    self.display().i18n.t("ai.model_selector.no_key")
                },
            )
            .into_any_element(),
        }
    }
}
