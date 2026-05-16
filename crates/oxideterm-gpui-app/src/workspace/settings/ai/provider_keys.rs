impl WorkspaceApp {
    fn ai_provider_key_display_state(
        &self,
        provider: &AiProviderView,
    ) -> AiProviderKeyDisplayState {
        ai_provider_key_display_state(
            &provider.provider_type,
            self.ai_provider_has_key(&provider.id),
        )
    }

    fn ai_provider_key_input(
        &self,
        index: usize,
        provider: &AiProviderView,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match self.ai_provider_key_display_state(provider) {
            AiProviderKeyDisplayState::Keyless => div().into_any_element(),
            AiProviderKeyDisplayState::Stored => self.ai_provider_stored_key_input(index, provider, cx),
            AiProviderKeyDisplayState::Missing => self.ai_provider_empty_key_input(index, cx),
        }
    }

    fn ai_provider_empty_key_input(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let input = SettingsInput::AiProviderApiKey(index);
        let focused = self.focused_settings_input == Some(input);
        let draft = if focused {
            self.settings_input_draft.as_str()
        } else {
            ""
        };
        let save_disabled = draft.trim().is_empty();
        div()
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
                    .child(div().flex_1().min_w(px(0.0)).child(
                        self.ai_provider_secret_input(
                            input,
                            draft,
                            "sk-...".to_string(),
                            focused,
                            cx,
                        ),
                    ))
                    .child(
                        button_with(
                            &self.tokens,
                            self.i18n.t("settings_view.ai.save"),
                            ButtonOptions {
                                variant: ButtonVariant::Secondary,
                                size: ButtonSize::Sm,
                                radius: ButtonRadius::Md,
                                disabled: save_disabled,
                            },
                        )
                        .h(px(32.0))
                        .on_mouse_down(
                            MouseButton::Left,
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

    fn ai_provider_stored_key_input(
        &self,
        index: usize,
        provider: &AiProviderView,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let provider_id = provider.id.clone();
        div()
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
                            .h(px(32.0))
                            .px(px(8.0))
                            .flex()
                            .items_center()
                            .rounded(px(self.tokens.radii.sm))
                            .border_1()
                            .border_color(rgba(
                                (self.tokens.ui.border << 8) | AI_PROVIDER_MODEL_BORDER_ALPHA,
                            ))
                            .bg(self.settings_panel_background(self.tokens.ui.bg_card))
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .italic()
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child("••••••••••••••••"),
                    )
                    .child(
                        button_with(
                            &self.tokens,
                            self.i18n.t("settings_view.ai.remove"),
                            ButtonOptions {
                                variant: ButtonVariant::Ghost,
                                size: ButtonSize::Sm,
                                radius: ButtonRadius::Md,
                                disabled: false,
                            },
                        )
                        .h(px(32.0))
                        .text_color(rgb(self.tokens.ui.error))
                        .hover(|style| style.bg(rgba((self.tokens.ui.error << 8) | 0x1a)))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.ai_provider_key_remove_confirm =
                                    Some((index, provider_id.clone()));
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )
                        .into_any_element(),
                    ),
            )
            .into_any_element()
    }

    fn ai_provider_secret_input(
        &self,
        input: SettingsInput,
        value: &str,
        placeholder: String,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let target = WorkspaceImeTarget::Settings(input);
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value,
                    placeholder,
                    focused,
                    caret_visible: self.new_connection_caret_visible,
                    secret: true,
                    selected_all: false,
                    marked_text: self.marked_text_for_target(target),
                },
            )
            .w_full()
            .h(px(32.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    if this.focused_settings_input != Some(input) {
                        this.focus_settings_input(input, String::new(), cx);
                    }
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle);
                    cx.stop_propagation();
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

    fn save_ai_provider_api_key(&mut self, index: usize, cx: &mut Context<Self>) {
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
        if self.focused_settings_input != Some(SettingsInput::AiProviderApiKey(index)) {
            cx.notify();
            return;
        }

        // Match Tauri ProviderKeyInput: the visible UI draft is moved into a
        // zeroizing owner before crossing into the keychain boundary, and it is
        // never written into persisted settings.
        let Some(secret) = ai_take_provider_key_secret(&mut self.settings_input_draft) else {
            cx.notify();
            return;
        };
        match self.ai_key_store.store_provider_key(&provider_id, secret) {
            Ok(()) => {
                self.ai_provider_key_status.insert(provider_id.clone(), true);
                self.focused_settings_input = None;
                if let Some(provider) = self
                    .settings_store
                    .settings()
                    .ai
                    .providers
                    .get(index)
                    .and_then(ai_provider_view)
                {
                    self.refresh_ai_provider_models(index, provider, cx);
                }
            }
            Err(error) => {
                self.push_ai_settings_toast(
                    self.ai_i18n_error("settings_view.ai.save_failed", &error.to_string()),
                    TerminalNoticeVariant::Error,
                );
            }
        }
        cx.notify();
    }

    fn remove_ai_provider_api_key(
        &mut self,
        _index: usize,
        provider_id: &str,
        cx: &mut Context<Self>,
    ) {
        match self.ai_key_store.delete_provider_key(provider_id) {
            Ok(()) => {
                self.ai_provider_key_status
                    .insert(provider_id.to_string(), false);
            }
            Err(error) => {
                self.push_ai_settings_toast(
                    self.ai_i18n_error("settings_view.ai.remove_failed", &error.to_string()),
                    TerminalNoticeVariant::Error,
                );
            }
        }
        cx.notify();
    }

}
