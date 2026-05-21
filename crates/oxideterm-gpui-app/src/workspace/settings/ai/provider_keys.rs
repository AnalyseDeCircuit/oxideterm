impl WorkspaceApp {
    fn ai_provider_key_display_state(
        &self,
        provider: &AiProviderView,
    ) -> AiProviderKeyDisplayState {
        ai_provider_key_display_state(
            &provider.provider_type,
            self.ai_provider_has_key_cached(&provider.id),
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
                    selected_range: self.ime_selected_range_for_target(target),
                    marked_text: self.marked_text_for_target(target),
                },
            )
            .w_full()
            .h(px(32.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    if this.focused_settings_input != Some(input) {
                        this.focus_settings_input(input, String::new(), cx);
                    }
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

    fn ai_provider_has_key_cached(&self, provider_id: &str) -> bool {
        self.ai_provider_key_status
            .get(provider_id)
            .copied()
            .unwrap_or(false)
    }

    pub(in crate::workspace) fn ensure_ai_provider_key_statuses(&mut self, cx: &mut Context<Self>) {
        let provider_jobs: Vec<_> = ai_provider_views(self.settings_store.settings())
            .into_iter()
            .filter(|provider| {
                ai_provider_key_display_state(&provider.provider_type, false).shows_key_control()
            })
            .map(|provider| provider.id)
            .collect();

        for provider_id in provider_jobs {
            if self.ai_provider_key_status.contains_key(&provider_id)
                || self.ai_provider_key_status_pending.contains(&provider_id)
            {
                continue;
            }
            self.ai_provider_key_status_pending
                .insert(provider_id.clone());
            if self.ai_provider_key_status_tx.is_none() {
                let (tx, rx) = std::sync::mpsc::channel();
                self.ai_provider_key_status_tx = Some(tx);
                self.ai_provider_key_status_rx = Some(rx);
            }
            let Some(ui_tx) = self.ai_provider_key_status_tx.as_ref().cloned() else {
                continue;
            };
            let key_store = self.ai_key_store.clone();
            self.forwarding_runtime.spawn(async move {
                let provider_id_for_check = provider_id.clone();
                let has_key = tokio::task::spawn_blocking(move || {
                    key_store.has_provider_key(&provider_id_for_check)
                })
                .await
                .unwrap_or(false);
                let _ = ui_tx.send(AiProviderKeyStatusDelivery {
                    provider_id,
                    has_key,
                });
            });
        }

        if !self.ai_provider_key_status_pending.is_empty() {
            self.schedule_ai_provider_key_status_poll(cx);
        }
    }

    fn poll_ai_provider_key_statuses(&mut self, cx: &mut Context<Self>) {
        let Some(rx) = self.ai_provider_key_status_rx.take() else {
            return;
        };
        let mut changed = false;
        loop {
            match rx.try_recv() {
                Ok(delivery) => {
                    self.ai_provider_key_status_pending
                        .remove(&delivery.provider_id);
                    let previous = self
                        .ai_provider_key_status
                        .insert(delivery.provider_id, delivery.has_key);
                    changed |= previous != Some(delivery.has_key);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.ai_provider_key_status_pending.clear();
                    self.ai_provider_key_status_tx = None;
                    break;
                }
            }
        }
        if changed {
            cx.notify();
        }
        if self.ai_provider_key_status_pending.is_empty() {
            self.ai_provider_key_status_tx = None;
        } else {
            self.ai_provider_key_status_rx = Some(rx);
        }
    }

    fn schedule_ai_provider_key_status_poll(&mut self, cx: &mut Context<Self>) {
        if self.ai_provider_key_status_polling {
            return;
        }
        self.ai_provider_key_status_polling = true;
        cx.spawn(async move |weak, cx| {
            Timer::after(Duration::from_millis(50)).await;
            let _ = weak.update(cx, |this, cx| {
                this.ai_provider_key_status_polling = false;
                this.poll_ai_provider_key_statuses(cx);
                if !this.ai_provider_key_status_pending.is_empty() {
                    this.schedule_ai_provider_key_status_poll(cx);
                }
            });
        })
        .detach();
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
        let key_store = self.ai_key_store.clone();
        let runtime = self.forwarding_runtime.clone();
        cx.spawn(async move |weak, cx| {
            let provider_id_for_store = provider_id.clone();
            let result = runtime
                .spawn_blocking(move || key_store.store_provider_key(&provider_id_for_store, secret))
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));
            let _ = weak.update(cx, |this, cx| {
                match result {
                    Ok(()) => {
                        this.ai_provider_key_status.insert(provider_id.clone(), true);
                        this.focused_settings_input = None;
                        if let Some(provider) = this
                            .settings_store
                            .settings()
                            .ai
                            .providers
                            .get(index)
                            .and_then(ai_provider_view)
                        {
                            this.refresh_ai_provider_models(index, provider, cx);
                        }
                    }
                    Err(error) => {
                        this.push_ai_settings_toast(
                            this.ai_i18n_error("settings_view.ai.save_failed", &error),
                            TerminalNoticeVariant::Error,
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn remove_ai_provider_api_key(
        &mut self,
        _index: usize,
        provider_id: &str,
        cx: &mut Context<Self>,
    ) {
        let provider_id = provider_id.to_string();
        let key_store = self.ai_key_store.clone();
        let runtime = self.forwarding_runtime.clone();
        cx.spawn(async move |weak, cx| {
            let provider_id_for_delete = provider_id.clone();
            let result = runtime
                .spawn_blocking(move || key_store.delete_provider_key(&provider_id_for_delete))
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));
            let _ = weak.update(cx, |this, cx| {
                match result {
                    Ok(()) => {
                        this.ai_provider_key_status.insert(provider_id.clone(), false);
                    }
                    Err(error) => {
                        this.push_ai_settings_toast(
                            this.ai_i18n_error("settings_view.ai.remove_failed", &error),
                            TerminalNoticeVariant::Error,
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

}
