use super::*;

const AI_PROVIDER_ACTION_BUTTON_H: f32 = 28.0; // Tauri provider card action h-7.
const AI_PROVIDER_ACTION_BUTTON_PX: f32 = 8.0; // Tauri provider card action px-2.

impl AiProviderSettingsView {
    pub(super) fn ai_provider_type_badge(&self, provider_type: String) -> AnyElement {
        div()
            .rounded(px(self.tokens.radii.sm))
            .bg(rgb(self.tokens.ui.bg_panel))
            .px(px(6.0))
            .py(px(2.0))
            .text_size(px(self.tokens.metrics.ui_text_2xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(provider_type.to_uppercase())
            .into_any_element()
    }

    pub(super) fn ai_provider_badge(&self, label: String, color: u32, bg_alpha: u32) -> AnyElement {
        div()
            .rounded(px(self.tokens.radii.sm))
            .bg(rgba((color << 8) | bg_alpha))
            .px(px(6.0))
            .py(px(2.0))
            .text_size(px(self.tokens.metrics.ui_text_2xs))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(color))
            .child(label)
            .into_any_element()
    }

    pub(super) fn ai_provider_active_button(
        &self,
        provider: &AiProviderView,
        _active_provider: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let provider = provider.clone();
        self.workspace_toolbar_action_button(
            self.i18n.t("settings_view.ai.set_active"),
            None,
            ToolbarButtonOptions::compact_text(
                ButtonVariant::Outline,
                ButtonRadius::Md,
                AI_PROVIDER_ACTION_BUTTON_H,
                AI_PROVIDER_ACTION_BUTTON_PX,
                self.tokens.metrics.ui_text_xs,
            ),
            cx.listener(move |this, _event, _window, cx| {
                let provider = provider.clone();
                this.edit_settings(
                    move |settings| {
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

    pub(super) fn ai_provider_enabled_toggle(
        &self,
        index: usize,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        checkbox(
            &self.tokens,
            format!(
                "ai-provider-enabled-{}",
                self.provider(index)
                    .map_or("", |provider| provider.id.as_str())
            ),
            String::new(),
            enabled,
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, _window, cx| {
                this.edit_settings(
                    move |settings| {
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

    pub(super) fn ai_provider_remove_button(
        &self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Tauri renders this custom-provider action as a ghost small Button
        // with danger text. Route activation through the workspace wrapper so
        // disabled/loading/focus-visible behavior stays centralized.
        self.workspace_toolbar_action_button(
            self.i18n.t("settings_view.ai.remove"),
            None,
            ToolbarButtonOptions {
                text_color: Some(rgb(self.tokens.ui.error)),
                hover_text_color: Some(rgb(self.tokens.ui.error)),
                hover_background: Some(rgba((self.tokens.ui.error << 8) | 0x1a)),
                ..ToolbarButtonOptions::compact_text(
                    ButtonVariant::Ghost,
                    ButtonRadius::Md,
                    AI_PROVIDER_ACTION_BUTTON_H,
                    AI_PROVIDER_ACTION_BUTTON_PX,
                    self.tokens.metrics.ui_text_xs,
                )
            },
            cx.listener(move |this, _event, _window, cx| {
                if let Some(provider) = this.provider(index) {
                    this.open_ai_settings_confirm(
                        AiSettingsConfirmIntent::RemoveProvider {
                            provider_id: provider.id.clone(),
                        },
                        Some(provider.name.clone()),
                        cx,
                    );
                }
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    pub(super) fn ai_provider_expanded_toolbar(
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
            .into_any_element()
    }

    pub(super) fn ai_provider_refresh_button(
        &self,
        _index: usize,
        provider: &AiProviderView,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let refreshing = (self.bindings.status)(&provider.id, cx).1;
        let provider_id = provider.id.clone();
        let mut options = ToolbarButtonOptions::compact_text(
            ButtonVariant::Ghost,
            ButtonRadius::Md,
            AI_PROVIDER_ACTION_BUTTON_H,
            AI_PROVIDER_ACTION_BUTTON_PX,
            self.tokens.metrics.ui_text_xs,
        );
        options.button.disabled = refreshing;
        options.icon_gap = Some(4.0);
        options.loading = refreshing;
        let refresh_id = provider_id.clone();
        // Tauri refresh is a compact ghost button with a leading RefreshCw
        // icon. Shared action guard keeps the loading state non-submitting.
        self.workspace_toolbar_action_button(
            self.i18n.t("settings_view.ai.refresh_models"),
            Some(Self::render_lucide_icon(
                LucideIcon::RefreshCw,
                12.0,
                rgb(self.tokens.ui.text_muted),
            )),
            options,
            cx.listener(move |this, _event, _window, cx| {
                (this.bindings.request)(AiProviderRequest::RefreshModels(provider_id.clone()), cx);
                cx.stop_propagation();
            }),
        )
        .debug_selector(move || format!("refresh-models-{refresh_id}"))
        .into_any_element()
    }

    pub(super) fn ai_provider_fields(
        &self,
        index: usize,
        provider: &AiProviderView,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .min_w(px(0.0))
            .px(px(16.0))
            .pb(px(12.0))
            .flex()
            .flex_col()
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
            .into_any_element()
    }

    pub(super) fn ai_provider_protocol_field(
        &self,
        index: usize,
        provider: &AiProviderView,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .px(px(16.0))
            .pb(px(16.0))
            .child(self.ai_provider_field(
                "settings_view.ai.api_protocol",
                self.ai_provider_protocol_control(index, provider, cx),
            ))
            .into_any_element()
    }

    fn ai_provider_protocol_control(
        &self,
        index: usize,
        provider: &AiProviderView,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut row = div().flex().flex_wrap().gap(px(4.0));
        for (protocol, key) in [
            (
                oxideterm_ai::AiApiProtocol::ChatCompletions,
                "settings_view.ai.protocol_chat_completions",
            ),
            (
                oxideterm_ai::AiApiProtocol::Responses,
                "settings_view.ai.protocol_responses",
            ),
        ] {
            row = row.child(
                oxideterm_gpui_ui::segmented_control_item(
                    &self.tokens,
                    self.i18n.t(key),
                    provider.api_protocol == protocol,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.edit_settings(
                            move |settings| {
                                ai_update_provider(settings, index, |provider| {
                                    provider
                                        .insert("apiProtocol".into(), serde_json::json!(protocol));
                                });
                            },
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                ),
            );
        }
        row.into_any_element()
    }

    pub(super) fn ai_provider_text_input_control(
        &self,
        input: SettingsInput,
        value: String,
        placeholder: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.field(input, &value, placeholder, false, cx)
    }

    pub(super) fn ai_provider_field(&self, label_key: &str, control: AnyElement) -> AnyElement {
        crate::settings_row(
            &self.tokens,
            div().child(self.i18n.t(label_key)).into_any_element(),
            None,
            div()
                .min_w(px(AI_PROVIDER_SELECT_W))
                .max_w_full()
                .flex_basis(px(320.0))
                .flex_grow(2.0)
                .child(control)
                .into_any_element(),
        )
    }
}
