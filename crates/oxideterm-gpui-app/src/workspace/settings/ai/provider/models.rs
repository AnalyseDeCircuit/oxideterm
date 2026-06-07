impl WorkspaceApp {
    fn ai_provider_models(
        &self,
        index: usize,
        provider: &AiProviderView,
        visible_model_count: usize,
        hidden_count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let provider_id = provider.id.clone();
        let models_expanded = self.settings_page.expanded_ai_provider_models.contains(&provider.id);
        let mut body = div().w_full().min_w(px(0.0)).flex().flex_col().gap(px(6.0)).child(
            div()
                .w_full()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .justify_between()
                .px(px(16.0))
                .child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(format!(
                            "{} ({})",
                            self.i18n.t("settings_view.ai.available_models"),
                            provider.models.len()
                        )),
                )
                .when(provider.models.len() > AI_PROVIDER_VISIBLE_MODEL_LIMIT, |row| {
                    row.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(self.tokens.ui.accent))
                            .cursor_pointer()
                            .child(if models_expanded {
                                self.i18n.t("settings_view.ai.show_fewer_models")
                            } else {
                                self.i18n_count(
                                    "settings_view.ai.show_all_models",
                                    provider.models.len(),
                                )
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    toggle_string_set(
                                        &mut this.settings_page.expanded_ai_provider_models,
                                        &provider_id,
                                    );
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ),
                    )
                }),
        );

        if visible_model_count > 0 {
            let chip_rows = ai_provider_model_chip_rows(
                provider,
                visible_model_count,
                AI_PROVIDER_MODEL_CHIPS_PER_VIRTUAL_ROW,
            );
            self.sync_ai_provider_model_chip_list_state(&provider.id, &chip_rows, hidden_count);
            let state = self
                .ai_provider_model_chip_list_states
                .borrow()
                .get(&provider.id)
                .cloned()
                .unwrap_or_else(|| {
                    ListState::new(
                        AI_PROVIDER_MODEL_CHIP_LIST_INITIAL_ROW_COUNT,
                        ListAlignment::Top,
                        self.ai_provider_model_chip_list_spec().overdraw(),
                    )
                    .measure_all()
                });
            let spec = self.ai_provider_model_chip_list_spec();
            let workspace = cx.entity();
            let provider_index = index;
            let row_count = chip_rows.len();
            let hidden_count_for_rows = hidden_count;
            let list_height = row_count as f32 * AI_PROVIDER_MODEL_CHIP_ROW_ESTIMATED_HEIGHT;
            body = body.child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .px(px(16.0))
                    .pb(px(16.0))
                    .h(px(list_height))
                    .child(tauri_virtual_list(
                        state,
                        spec,
                        move |row_index, _window, cx| {
                            workspace.update(cx, |this, cx| {
                                this.ai_provider_model_chip_row(
                                    provider_index,
                                    row_index,
                                    hidden_count_for_rows,
                                    cx,
                                )
                            })
                        },
                    )),
            );
        }

        body.into_any_element()
    }

    fn sync_ai_provider_model_chip_list_state(
        &self,
        provider_id: &str,
        rows: &[Vec<AiProviderModelChipItem>],
        hidden_count: usize,
    ) {
        let signatures = rows
            .iter()
            .enumerate()
            .map(|(row_index, row)| {
                let mut hasher = DefaultHasher::new();
                // Chip rows preserve the old flex-wrap look in bounded chunks;
                // visible labels, active state, and the final hidden counter
                // determine row measurement.
                row_index.hash(&mut hasher);
                for item in row {
                    item.model.hash(&mut hasher);
                    item.active.hash(&mut hasher);
                }
                if row_index + 1 == rows.len() {
                    hidden_count.hash(&mut hasher);
                }
                hasher.finish()
            })
            .collect::<Vec<_>>();
        let state = {
            let mut states = self.ai_provider_model_chip_list_states.borrow_mut();
            states
                .entry(provider_id.to_string())
                .or_insert_with(|| {
                    ListState::new(
                        AI_PROVIDER_MODEL_CHIP_LIST_INITIAL_ROW_COUNT,
                        ListAlignment::Top,
                        self.ai_provider_model_chip_list_spec().overdraw(),
                    )
                    .measure_all()
                })
                .clone()
        };
        {
            let mut caches = self.ai_provider_model_chip_list_caches.borrow_mut();
            let cache = caches.entry(provider_id.to_string()).or_default();
            sync_tauri_variable_list_state_by_signatures(
                &state,
                cache,
                &format!("ai-provider-model-chips:{provider_id}"),
                &signatures,
                self.ai_provider_model_chip_list_spec(),
            );
        }
    }

    fn ai_provider_model_chip_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(AI_PROVIDER_MODEL_CHIP_ROW_ESTIMATED_HEIGHT),
            AI_PROVIDER_MODEL_CHIP_ROW_OVERSCAN,
        )
    }

    fn ai_provider_model_chip_row(
        &self,
        provider_index: usize,
        row_index: usize,
        hidden_count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(provider) = ai_provider_views(self.settings_store.settings())
            .get(provider_index)
            .cloned()
        else {
            return div().into_any_element();
        };
        let models_expanded = self.settings_page.expanded_ai_provider_models.contains(&provider.id);
        let visible_model_count = if models_expanded {
            provider.models.len()
        } else {
            provider.models.len().min(AI_PROVIDER_VISIBLE_MODEL_LIMIT)
        };
        let rows = ai_provider_model_chip_rows(
            &provider,
            visible_model_count,
            AI_PROVIDER_MODEL_CHIPS_PER_VIRTUAL_ROW,
        );
        let Some(row) = rows.get(row_index) else {
            return div().into_any_element();
        };
        let is_last_row = row_index + 1 == rows.len();
        let mut chips = div().w_full().min_w(px(0.0)).flex().flex_wrap().gap(px(4.0));
        for item in row {
            chips = chips.child(self.ai_provider_model_chip(
                provider_index,
                item.model.clone(),
                item.active,
                cx,
            ));
        }
        if is_last_row && hidden_count > 0 {
            chips = chips.child(
                div()
                    .px(px(6.0))
                    .py(px(2.0))
                    .text_size(px(10.0))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(format!("+{hidden_count}")),
            );
        }
        chips.into_any_element()
    }

    fn ai_provider_model_chip(
        &self,
        index: usize,
        model: String,
        active: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let model_for_edit = model.clone();
        div()
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(if active {
                rgba((self.tokens.ui.accent << 8) | AI_PROVIDER_MODEL_ACTIVE_BORDER_ALPHA)
            } else {
                rgba((self.tokens.ui.border << 8) | AI_PROVIDER_MODEL_BORDER_ALPHA)
            })
            .bg(if active {
                rgba((self.tokens.ui.accent << 8) | AI_PROVIDER_MODEL_ACTIVE_BG_ALPHA)
            } else {
                rgb(self.tokens.ui.bg)
            })
            .px(px(6.0))
            .py(px(2.0))
            .text_size(px(10.0))
            .text_color(rgb(if active {
                self.tokens.ui.accent
            } else {
                self.tokens.ui.text_muted
            }))
            .cursor_pointer()
            .hover(|style| {
                style
                    .text_color(rgb(self.tokens.ui.text))
                    .border_color(rgb(self.tokens.ui.border))
            })
            .child(model)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.edit_settings(
                        |settings| {
                            ai_set_provider_default_model(
                                &mut settings.ai.providers,
                                index,
                                model_for_edit.clone(),
                            );
                        },
                        cx,
                    );
                }),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn ai_provider_has_key(&self, provider_id: &str) -> bool {
        self.ai_provider_has_key_cached(provider_id)
    }



}
