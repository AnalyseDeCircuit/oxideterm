use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn ai_provider_key_display_state(
        &self,
        provider: &AiProviderView,
        cx: &App,
    ) -> AiProviderKeyDisplayState {
        ai_provider_key_display_state(
            &provider.provider_type,
            self.ai_provider_has_key_cached(&provider.id, cx),
        )
    }

    pub(in crate::workspace) fn ai_provider_key_input(
        &self,
        index: usize,
        provider: &AiProviderView,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match self.ai_provider_key_display_state(provider, cx) {
            AiProviderKeyDisplayState::Keyless => div().into_any_element(),
            AiProviderKeyDisplayState::Stored => {
                self.ai_provider_stored_key_input(index, provider, cx)
            }
            AiProviderKeyDisplayState::Missing => self.ai_provider_empty_key_input(index, cx),
        }
    }

    pub(in crate::workspace) fn ai_provider_empty_key_input(
        &self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input = SettingsInput::AiProviderApiKey(index);
        let (focused, save_disabled) = {
            let ai_workspace = self.ai_entity.read(cx);
            let focused = ai_workspace.focused_settings_input() == Some(input);
            let save_disabled = ai_workspace
                .settings_input_value(input)
                .is_none_or(|draft| draft.trim().is_empty());
            (focused, save_disabled)
        };
        div()
            .w_full()
            .min_w(px(0.0))
            .px(px(16.0))
            .pb(px(16.0))
            .grid()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.ai.api_key")),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .child(self.ai_provider_secret_input(
                                input,
                                "",
                                "sk-...".to_string(),
                                focused,
                                cx,
                            )),
                    )
                    .child(
                        // ProviderKeyInput.tsx uses a secondary small Button
                        // with h-8 text-xs for save. Route activation through
                        // the workspace action wrapper so disabled state cannot
                        // dispatch, matching the browser Button attribute.
                        self.workspace_toolbar_action_button(
                            self.i18n.t("settings_view.ai.save"),
                            None,
                            ToolbarButtonOptions {
                                button: ButtonOptions {
                                    variant: ButtonVariant::Secondary,
                                    size: ButtonSize::Sm,
                                    radius: ButtonRadius::Md,
                                    disabled: save_disabled,
                                },
                                height: Some(32.0),
                                font_size: Some(self.tokens.metrics.ui_text_xs),
                                ..ToolbarButtonOptions::default()
                            },
                            cx.listener(move |this, _event, _window, cx| {
                                this.save_ai_provider_api_key(index, cx);
                                cx.stop_propagation();
                            }),
                        )
                        .into_any_element(),
                    ),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn ai_provider_stored_key_input(
        &self,
        index: usize,
        provider: &AiProviderView,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let provider_id = provider.id.clone();
        div()
            .w_full()
            .min_w(px(0.0))
            .px(px(16.0))
            .pb(px(16.0))
            .grid()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.ai.api_key")),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .h(px(32.0))
                            .px(px(8.0))
                            .flex()
                            .items_center()
                            .rounded(px(self.tokens.radii.sm))
                            .border_1()
                            .border_color(rgba(
                                (self.tokens.ui.border << 8) | AI_PROVIDER_MODEL_BORDER_ALPHA,
                            ))
                            // The masked key display sits inside a provider
                            // card, so border/background is enough elevation.
                            .bg(self.settings_panel_background(self.tokens.ui.bg_card))
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .italic()
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child("••••••••••••••••"),
                    )
                    .child(
                        // Stored API key removal mirrors Tauri's ghost small
                        // danger Button. Shared activation keeps this confirm
                        // trigger on the same disabled/loading path as the
                        // rest of AI provider actions.
                        self.workspace_toolbar_action_button(
                            self.i18n.t("settings_view.ai.remove"),
                            None,
                            ToolbarButtonOptions {
                                button: ButtonOptions {
                                    variant: ButtonVariant::Ghost,
                                    size: ButtonSize::Sm,
                                    radius: ButtonRadius::Md,
                                    disabled: false,
                                },
                                height: Some(32.0),
                                font_size: Some(self.tokens.metrics.ui_text_xs),
                                text_color: Some(rgb(self.tokens.ui.error)),
                                hover_text_color: Some(rgb(self.tokens.ui.error)),
                                hover_background: Some(rgba((self.tokens.ui.error << 8) | 0x1a)),
                                ..ToolbarButtonOptions::default()
                            },
                            cx.listener(move |this, _event, _window, cx| {
                                this.ai_entity.update(cx, |ai, cx| {
                                    ai.open_provider_key_remove_confirm(
                                        index,
                                        provider_id.clone(),
                                        cx,
                                    );
                                });
                                this.reset_standard_confirm_focus();
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )
                        .into_any_element(),
                    ),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn ai_provider_secret_input(
        &self,
        input: SettingsInput,
        value: &str,
        placeholder: String,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let target = WorkspaceImeTarget::Settings(input);
        let input_control = if ai_state::AiWorkspaceEntity::owns_settings_input(input) {
            let ai_workspace = self.ai_entity.read(cx);
            text_input(
                &self.tokens,
                TextInputView {
                    value: ai_workspace.settings_input_value(input).unwrap_or_default(),
                    placeholder,
                    focused,
                    caret_visible: self.input_caret.visible(),
                    secret: true,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target, cx),
                    marked_text: self.marked_text_for_target(target, cx),
                },
            )
        } else {
            text_input(
                &self.tokens,
                TextInputView {
                    value,
                    placeholder,
                    focused,
                    caret_visible: self.input_caret.visible(),
                    secret: true,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target, cx),
                    marked_text: self.marked_text_for_target(target, cx),
                },
            )
        };
        self.text_input_with_workspace_ime(
            target,
            input_control
                .w_full()
                .h(px(32.0))
                .text_size(px(self.tokens.metrics.ui_text_xs)),
            move |this, cx| {
                this.focus_settings_input(input, String::new(), cx);
            },
            cx,
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn ai_provider_has_key_cached(
        &self,
        provider_id: &str,
        cx: &App,
    ) -> bool {
        self.ai_entity.read(cx).provider_has_key(provider_id)
    }

    pub(in crate::workspace) fn ensure_ai_provider_key_statuses(&mut self, cx: &mut Context<Self>) {
        let provider_views = ai_provider_views(self.settings_store.settings());
        self.ensure_ai_provider_key_statuses_for_views(&provider_views, cx);
    }

    pub(in crate::workspace) fn ensure_ai_provider_key_statuses_for_views(
        &mut self,
        provider_views: &[AiProviderView],
        cx: &mut Context<Self>,
    ) {
        // Rendering OxideSens already derives provider views, so reuse that
        // snapshot when available instead of parsing the same JSON again.
        let provider_jobs: Vec<_> = provider_views
            .iter()
            .filter(|provider| {
                ai_provider_key_display_state(&provider.provider_type, false).shows_key_control()
            })
            .map(|provider| provider.id.clone())
            .collect();

        self.ai_entity.update(cx, |ai, _cx| {
            ai.request_provider_key_statuses(provider_jobs);
        });
    }

    pub(in crate::workspace) fn save_ai_provider_api_key(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(provider_id) = self
            .settings_store
            .settings()
            .ai
            .providers
            .get(index)
            .and_then(ai_provider_id)
        else {
            return;
        };
        // Match Tauri ProviderKeyInput: the visible UI draft is moved into a
        // zeroizing owner before crossing into the keychain boundary, and it is
        // never written into persisted settings.
        let input = SettingsInput::AiProviderApiKey(index);
        let Some(secret) = self
            .ai_entity
            .update(cx, |ai, _cx| ai.take_provider_key_secret(input))
        else {
            cx.notify();
            return;
        };
        self.ai_entity.update(cx, |ai, cx| {
            ai.store_provider_key(index, provider_id, secret, cx);
        });
        cx.notify();
    }

    pub(in crate::workspace) fn remove_ai_provider_api_key(
        &mut self,
        _index: usize,
        provider_id: &str,
        cx: &mut Context<Self>,
    ) {
        self.ai_entity.update(cx, |ai, cx| {
            ai.remove_provider_key(provider_id.to_string(), cx);
        });
        cx.notify();
    }
}
