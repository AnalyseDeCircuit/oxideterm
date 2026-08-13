use super::*;

// Keep the chip viewport at its full row height before adding spacing below it.
const AI_PROVIDER_MODEL_CHIP_LIST_BOTTOM_PADDING: f32 = 16.0;

fn ai_provider_model_chip_list_height(row_count: usize) -> f32 {
    row_count as f32 * AI_PROVIDER_MODEL_CHIP_ROW_ESTIMATED_HEIGHT
        + AI_PROVIDER_MODEL_CHIP_LIST_BOTTOM_PADDING
}

impl WorkspaceApp {
    pub(in crate::workspace) fn ai_provider_models(
        &self,
        index: usize,
        provider: &AiProviderView,
        visible_model_count: usize,
        hidden_count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let provider_id = provider.id.clone();
        let models_expanded = self
            .ai_entity
            .read(cx)
            .settings_provider_models_expanded(&provider.id);
        let mut body = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
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
                    .when(
                        provider.models.len() > AI_PROVIDER_VISIBLE_MODEL_LIMIT,
                        |row| {
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
                                            this.ai_entity.update(cx, |ai, cx| {
                                                ai.toggle_settings_provider_models(
                                                    &provider_id,
                                                    cx,
                                                );
                                            });
                                            cx.stop_propagation();
                                            // WorkspaceApp owns the surrounding settings render.
                                            cx.notify();
                                        }),
                                    ),
                            )
                        },
                    ),
            );

        if visible_model_count > 0 {
            let chip_rows = ai_provider_model_chip_rows(
                provider,
                visible_model_count,
                AI_PROVIDER_MODEL_CHIPS_PER_VIRTUAL_ROW,
            );
            self.sync_ai_provider_model_chip_list_state(&provider.id, &chip_rows, hidden_count, cx);
            let state = self
                .ai_entity
                .read(cx)
                .model_ui()
                .provider_model_chip_list_states
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
            let list_height = ai_provider_model_chip_list_height(row_count);
            body = body.child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .px(px(16.0))
                    .pb(px(AI_PROVIDER_MODEL_CHIP_LIST_BOTTOM_PADDING))
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

        body = body.child(self.ai_provider_model_input(index, provider, cx));

        body.into_any_element()
    }

    fn ai_provider_model_input(
        &self,
        index: usize,
        provider: &AiProviderView,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input = SettingsInput::AiProviderNewModel(index);
        let can_add = self.focused_settings_input == Some(input)
            && !self.settings_input_draft.trim().is_empty();
        let provider_id = provider.id.clone();
        let mut options = ToolbarButtonOptions::compact_text(
            ButtonVariant::Outline,
            ButtonRadius::Md,
            32.0,
            10.0,
            self.tokens.metrics.ui_text_xs,
        );
        options.button.disabled = !can_add;

        div()
            .w_full()
            .min_w(px(0.0))
            .px(px(16.0))
            .pb(px(16.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .child(self.ai_provider_text_input_control(
                        input,
                        String::new(),
                        self.i18n.t("settings_view.ai.model_name_placeholder"),
                        cx,
                    )),
            )
            .child(self.workspace_toolbar_action_button(
                self.i18n.t("settings_view.ai.add_model"),
                None,
                options,
                cx.listener(move |this, _event, _window, cx| {
                    this.add_ai_provider_model(index, &provider_id, cx);
                    cx.stop_propagation();
                }),
            ))
            .into_any_element()
    }

    fn add_ai_provider_model(&mut self, index: usize, provider_id: &str, cx: &mut Context<Self>) {
        let input = SettingsInput::AiProviderNewModel(index);
        if self.focused_settings_input != Some(input) {
            return;
        }
        let model = self.settings_input_draft.trim().to_string();
        if model.is_empty() {
            return;
        }
        let provider_id = provider_id.to_string();
        let already_present = self
            .settings_store
            .settings()
            .ai
            .providers
            .get(index)
            .filter(|provider| ai_provider_id(provider).as_deref() == Some(provider_id.as_str()))
            .and_then(|provider| provider.get("models"))
            .and_then(serde_json::Value::as_array)
            .is_some_and(|models| {
                models
                    .iter()
                    .any(|existing| existing.as_str() == Some(model.as_str()))
            });
        if already_present {
            return;
        }
        self.edit_settings(
            move |settings| {
                ai_add_provider_model(&mut settings.ai.providers, index, &provider_id, &model);
            },
            cx,
        );
        self.settings_input_draft.clear();
        self.clear_ime_selection();
        cx.notify();
    }

    pub(in crate::workspace) fn sync_ai_provider_model_chip_list_state(
        &self,
        provider_id: &str,
        rows: &[Vec<AiProviderModelChipItem>],
        hidden_count: usize,
        cx: &App,
    ) {
        let signatures = rows
            .iter()
            .enumerate()
            .map(|(row_index, row)| {
                let mut hasher = DefaultHasher::new();
                // Chip rows preserve the old flex-wrap look in bounded chunks;
                // visible labels and the final hidden counter determine row
                // measurement.
                row_index.hash(&mut hasher);
                for item in row {
                    item.model.hash(&mut hasher);
                }
                if row_index + 1 == rows.len() {
                    hidden_count.hash(&mut hasher);
                }
                hasher.finish()
            })
            .collect::<Vec<_>>();
        let state = {
            let ai = self.ai_entity.read(cx);
            let mut states = ai.model_ui().provider_model_chip_list_states.borrow_mut();
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
            let ai = self.ai_entity.read(cx);
            let mut caches = ai.model_ui().provider_model_chip_list_caches.borrow_mut();
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

    pub(in crate::workspace) fn ai_provider_model_chip_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(AI_PROVIDER_MODEL_CHIP_ROW_ESTIMATED_HEIGHT),
            AI_PROVIDER_MODEL_CHIP_ROW_OVERSCAN,
        )
    }

    pub(in crate::workspace) fn ai_provider_model_chip_row(
        &self,
        provider_index: usize,
        row_index: usize,
        hidden_count: usize,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(provider) = ai_provider_views(self.settings_store.settings())
            .get(provider_index)
            .cloned()
        else {
            return div().into_any_element();
        };
        let models_expanded = self
            .ai_entity
            .read(_cx)
            .settings_provider_models_expanded(&provider.id);
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
        let mut chips = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_wrap()
            .gap(px(4.0));
        for item in row {
            chips = chips.child(self.ai_provider_model_chip(item.model.clone()));
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

    pub(in crate::workspace) fn ai_provider_model_chip(&self, model: String) -> AnyElement {
        div()
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgba(
                (self.tokens.ui.border << 8) | AI_PROVIDER_MODEL_BORDER_ALPHA,
            ))
            .bg(rgb(self.tokens.ui.bg))
            .px(px(6.0))
            .py(px(2.0))
            .text_size(px(10.0))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(model)
            .into_any_element()
    }

    pub(in crate::workspace) fn ai_provider_has_key(&self, provider_id: &str, cx: &App) -> bool {
        self.ai_provider_has_key_cached(provider_id, cx)
    }
}
