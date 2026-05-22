impl WorkspaceApp {
    fn ai_provider_type_badge(&self, provider_type: String) -> AnyElement {
        div()
            .rounded(px(self.tokens.radii.sm))
            .bg(rgb(self.tokens.ui.bg_panel))
            .px(px(6.0))
            .py(px(2.0))
            .text_size(px(10.0))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(provider_type.to_uppercase())
            .into_any_element()
    }

    fn ai_provider_badge(&self, label: String, color: u32, bg_alpha: u32) -> AnyElement {
        div()
            .rounded(px(self.tokens.radii.sm))
            .bg(rgba((color << 8) | bg_alpha))
            .px(px(6.0))
            .py(px(2.0))
            .text_size(px(10.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(color))
            .child(label)
            .into_any_element()
    }

    fn ai_provider_active_button(
        &self,
        provider: &AiProviderView,
        active_provider: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let provider = provider.clone();
        let label = if active_provider {
            self.i18n.t("settings_view.ai.active")
        } else {
            self.i18n.t("settings_view.ai.set_active")
        };
        button_with(
            &self.tokens,
            label,
            ButtonOptions {
                variant: if active_provider {
                    ButtonVariant::Default
                } else {
                    ButtonVariant::Outline
                },
                size: ButtonSize::Sm,
                radius: ButtonRadius::Md,
                disabled: false,
            },
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, _window, cx| {
                this.edit_settings(
                    |settings| {
                        ai_set_active_provider_selection(
                            &mut settings.ai.active_provider_id,
                            &mut settings.ai.active_model,
                            &provider,
                        );
                    },
                    cx,
                );
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    fn ai_provider_enabled_toggle(
        &self,
        index: usize,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        checkbox(&self.tokens, String::new(), enabled)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.edit_settings(
                        |settings| {
                            ai_update_provider(settings, index, |provider| {
                                provider.insert("enabled".to_string(), serde_json::json!(!enabled));
                            });
                        },
                        cx,
                    );
                }),
            )
            .into_any_element()
    }

    fn ai_provider_remove_button(
        &self,
        index: usize,
        _name: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Tauri renders this custom-provider action as a ghost small Button
        // with danger text. Use the toolbar primitive so loading/disabled and
        // future focus-visible behavior stay shared with other action buttons.
        toolbar_button(
            &self.tokens,
            self.i18n.t("settings_view.ai.remove"),
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                height: Some(28.0),
                padding_x: Some(8.0),
                font_size: Some(self.tokens.metrics.ui_text_xs),
                text_color: Some(rgb(self.tokens.ui.error)),
                hover_text_color: Some(rgb(self.tokens.ui.error)),
                hover_background: Some(rgba((self.tokens.ui.error << 8) | 0x1a)),
                ..ToolbarButtonOptions::default()
            },
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, _window, cx| {
                if let Some(provider_id) = this
                    .settings_store
                    .settings()
                    .ai
                    .providers
                    .get(index)
                    .and_then(ai_provider_id)
                {
                    let provider_name = this
                        .settings_store
                        .settings()
                        .ai
                        .providers
                        .get(index)
                        .and_then(|provider| ai_provider_string(provider, "name"))
                        .unwrap_or_else(|| _name.clone());
                    this.ai_provider_remove_confirm = Some((provider_id, provider_name));
                    this.reset_standard_confirm_focus();
                }
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .into_any_element()
    }

    fn ai_provider_expanded_toolbar(
        &self,
        index: usize,
        provider: &AiProviderView,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .border_t_1()
            .border_color(rgba((self.tokens.ui.border << 8) | 0x4d))
            .px(px(16.0))
            .pt(px(12.0))
            .pb(px(12.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .cursor_pointer()
                    .child(self.ai_provider_enabled_toggle(index, provider.enabled, cx))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("settings_view.ai.provider_enabled")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(self.ai_provider_refresh_button(index, provider.clone(), cx))
                    .when(provider.custom, |row| {
                        row.child(self.ai_provider_remove_button(index, provider.name.clone(), cx))
                    }),
            )
            .into_any_element()
    }

    fn ai_provider_refresh_button(
        &self,
        index: usize,
        provider: AiProviderView,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let refreshing = self.ai_model_refreshing.contains(&provider.id);
        // Tauri refresh is a compact ghost button with a leading RefreshCw
        // icon. Keep disabled/loading chrome in the shared toolbar button.
        toolbar_button(
            &self.tokens,
            self.i18n.t("settings_view.ai.refresh_models"),
            Some(Self::render_lucide_icon(
                LucideIcon::RefreshCw,
                12.0,
                rgb(self.tokens.ui.text_muted),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: refreshing,
                },
                height: Some(28.0),
                padding_x: Some(8.0),
                font_size: Some(10.0),
                icon_gap: Some(4.0),
                loading: refreshing,
                ..ToolbarButtonOptions::default()
            },
        )
        .when(!refreshing, |button| {
            button.on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.refresh_ai_provider_models(index, provider.clone(), cx);
                    cx.stop_propagation();
                }),
            )
        })
        .into_any_element()
    }

    fn ai_provider_fields(
        &self,
        index: usize,
        provider: &AiProviderView,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .px(px(16.0))
            .pb(px(12.0))
            .grid()
            .grid_cols(2)
            .gap(px(12.0))
            .child(self.ai_provider_field(
                "settings_view.ai.provider_name",
                self.ai_provider_text_input_control(
                    SettingsInput::AiProviderName(index),
                    provider.name.clone(),
                    self.i18n.t("settings_view.ai.provider_name"),
                    cx,
                ),
            ))
            .child(self.ai_provider_field(
                "settings_view.ai.base_url",
                self.ai_provider_text_input_control(
                    SettingsInput::AiProviderBaseUrl(index),
                    provider.base_url.clone(),
                    if provider.provider_type == "openai_compatible" {
                        "http://localhost:1234/v1".to_string()
                    } else {
                        String::new()
                    },
                    cx,
                ),
            ))
            .child(self.ai_provider_field(
                "settings_view.ai.default_model",
                self.ai_provider_text_input_control(
                    SettingsInput::AiProviderDefaultModel(index),
                    provider.default_model.clone(),
                    self.i18n.t("settings_view.ai.default_model"),
                    cx,
                ),
            ))
            .child(self.ai_provider_field(
                "settings_view.ai.reasoning_provider_default",
                self.ai_provider_reasoning_select(index, provider, cx),
            ))
            .into_any_element()
    }

    fn ai_provider_text_input_control(
        &self,
        input: SettingsInput,
        value: String,
        placeholder: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = self.focused_settings_input == Some(input);
        let display_value = if focused {
            self.settings_input_draft.as_str()
        } else {
            value.as_str()
        };
        let target = WorkspaceImeTarget::Settings(input);
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value: display_value,
                    placeholder,
                    focused,
                    caret_visible: self.new_connection_caret_visible,
                    secret: false,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target),
                    marked_text: self.marked_text_for_target(target),
                },
            )
            .w_full()
            .h(px(32.0))
            .min_w(px(0.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    let current = this.current_settings_input_value(input);
                    this.focus_settings_input(input, current, cx);
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle);
                    this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                    this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                }),
            ),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    fn ai_provider_reasoning_select(
        &self,
        index: usize,
        provider: &AiProviderView,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let current = self
            .settings_store
            .settings()
            .ai
            .reasoning_provider_overrides
            .get(&provider.id)
            .and_then(serde_json::Value::as_str)
            .unwrap_or("__inherit__")
            .to_string();
        let label = if current == "__inherit__" {
            let global = self.ai_reasoning_display(ai_reasoning_profile_value(
                self.settings_store.settings().ai.reasoning_effort,
            ));
            self.i18n
                .t("settings_view.ai.reasoning_inherit_global")
                .replace("{{value}}", &global)
        } else {
            self.ai_reasoning_display(&current)
        };
        let select_id = SettingsSelect::AiProviderReasoning(index);
        let workspace = cx.entity();
        select_anchor_probe(
            select_id.anchor_id(),
            self.settings_select_trigger(select_id, label, false, false)
                .w_full()
                .h(px(32.0))
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.open_settings_select_from_pointer(select_id);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_select_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    fn ai_provider_field(&self, label_key: &str, control: AnyElement) -> AnyElement {
        div()
            .min_w(px(0.0))
            .grid()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(label_key)),
            )
            .child(control)
            .into_any_element()
    }


}
