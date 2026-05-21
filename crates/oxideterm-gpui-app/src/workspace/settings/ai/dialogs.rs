impl WorkspaceApp {
    pub(in crate::workspace) fn handle_ai_settings_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.show_ai_enable_confirm {
            match self.handle_standard_confirm_key(event, cx) {
                Some(ConfirmKeyboardAction::Cancel) => {
                    self.show_ai_enable_confirm = false;
                    cx.notify();
                    true
                }
                Some(ConfirmKeyboardAction::Confirm) => {
                    self.edit_settings(
                        |settings| {
                            settings.ai.enabled = true;
                            settings.ai.enabled_confirmed = true;
                        },
                        cx,
                    );
                    self.show_ai_enable_confirm = false;
                    true
                }
                Some(ConfirmKeyboardAction::Handled) => true,
                None => false,
            }
        } else if self.ai_provider_key_remove_confirm.is_some() {
            match self.handle_standard_confirm_key(event, cx) {
                Some(ConfirmKeyboardAction::Cancel) => {
                    self.ai_provider_key_remove_confirm = None;
                    cx.notify();
                    true
                }
                Some(ConfirmKeyboardAction::Confirm) => {
                    if let Some((index, provider_id)) = self.ai_provider_key_remove_confirm.take()
                    {
                        self.remove_ai_provider_api_key(index, &provider_id, cx);
                    }
                    true
                }
                Some(ConfirmKeyboardAction::Handled) => true,
                None => false,
            }
        } else if self.ai_provider_remove_confirm.is_some() {
            match self.handle_standard_confirm_key(event, cx) {
                Some(ConfirmKeyboardAction::Cancel) => {
                    self.ai_provider_remove_confirm = None;
                    cx.notify();
                    true
                }
                Some(ConfirmKeyboardAction::Confirm) => {
                    if let Some((provider_id, _name)) = self.ai_provider_remove_confirm.take() {
                        self.remove_ai_provider(&provider_id, cx);
                    }
                    true
                }
                Some(ConfirmKeyboardAction::Handled) => true,
                None => false,
            }
        } else {
            false
        }
    }

    pub(in crate::workspace) fn render_ai_enable_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        dismissible_dialog_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    // Tauri SettingsView AI confirm is a Radix Dialog bound to
                    // setShowAiConfirm, so outside click is the Cancel path.
                    this.show_ai_enable_confirm = false;
                    this.clear_standard_confirm_focus();
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(
                dialog_content(&self.tokens)
                    .w(px(AI_CONFIRM_DIALOG_WIDTH))
                    .max_w(relative(0.92))
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation();
                    })
                    .child(
                        dialog_header(&self.tokens)
                            .child(dialog_title(
                                &self.tokens,
                                self.i18n.t("settings_view.ai_confirm.title"),
                            ))
                            .child(dialog_description(
                                &self.tokens,
                                self.i18n.t("settings_view.ai_confirm.description"),
                            )),
                    )
                    .child(
                        div()
                            .p(px(16.0))
                            .flex()
                            .flex_col()
                            .gap(px(16.0))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(self.i18n.t("settings_view.ai_confirm.intro")),
                            )
                            .child(
                                div()
                                    .rounded(px(self.tokens.radii.sm))
                                    .border_1()
                                    .border_color(rgba((self.tokens.ui.border << 8) | 0x80))
                                    .bg(rgba((self.tokens.ui.bg_panel << 8) | 0x4d))
                                    .p(px(12.0))
                                    .flex()
                                    .flex_col()
                                    .gap(px(8.0))
                                    .child(self.ai_confirm_bullet(
                                        "settings_view.ai_confirm.point_local",
                                    ))
                                    .child(self.ai_confirm_bullet(
                                        "settings_view.ai_confirm.point_no_server",
                                    ))
                                    .child(self.ai_confirm_bullet(
                                        "settings_view.ai_confirm.point_context",
                                    )),
                            ),
                    )
                    .child(
                        dialog_footer(&self.tokens)
                            .child(
                                button_focus_visible(
                                    &self.tokens,
                                    button_with(
                                        &self.tokens,
                                        self.i18n.t("settings_view.ai_confirm.cancel"),
                                        ButtonOptions {
                                            variant: ButtonVariant::Ghost,
                                            size: ButtonSize::Sm,
                                            radius: ButtonRadius::Md,
                                            disabled: false,
                                        },
                                    ),
                                    self.standard_confirm_focus()
                                        == Some(ConfirmDialogAction::Cancel),
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.show_ai_enable_confirm = false;
                                        this.clear_standard_confirm_focus();
                                        cx.stop_propagation();
                                        cx.notify();
                                    }),
                                ),
                            )
                            .child(
                                button_focus_visible(
                                    &self.tokens,
                                    button_with(
                                        &self.tokens,
                                        self.i18n.t("settings_view.ai_confirm.enable"),
                                        ButtonOptions {
                                            variant: ButtonVariant::Default,
                                            size: ButtonSize::Sm,
                                            radius: ButtonRadius::Md,
                                            disabled: false,
                                        },
                                    ),
                                    self.standard_confirm_focus()
                                        == Some(ConfirmDialogAction::Confirm),
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.edit_settings(
                                            |settings| {
                                                settings.ai.enabled = true;
                                                settings.ai.enabled_confirmed = true;
                                            },
                                            cx,
                                        );
                                        this.show_ai_enable_confirm = false;
                                        this.clear_standard_confirm_focus();
                                        cx.stop_propagation();
                                    }),
                                ),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn ai_confirm_bullet(&self, label_key: &str) -> AnyElement {
        div()
            .flex()
            .items_start()
            .gap(px(8.0))
            .child(
                div()
                    .mt(px(6.0))
                    .size(px(AI_CONFIRM_BULLET_SIZE))
                    .rounded(px(AI_CONFIRM_BULLET_SIZE / 2.0))
                    .bg(rgb(self.tokens.ui.text_muted)),
            )
            .child(
                div()
                    .flex_1()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(label_key)),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_ai_provider_key_remove_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        dismissible_dialog_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    // Tauri provider-key removal uses the shared confirm
                    // dialog; outside close cancels the pending removal.
                    this.ai_provider_key_remove_confirm = None;
                    this.clear_standard_confirm_focus();
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(
                dialog_content(&self.tokens)
                    .w(px(AI_KEY_REMOVE_DIALOG_WIDTH))
                    .max_w(relative(0.92))
                    .shadow_lg()
                    .rounded(px(self.tokens.radii.lg))
                    .border_color(rgba((self.tokens.ui.border << 8) | 0x99))
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation();
                    })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(12.0))
                            .px(px(24.0))
                            .pt(px(24.0))
                            .pb(px(16.0))
                            .child(
                                div()
                                    .size(px(AI_CONFIRM_ICON_WRAP))
                                    .rounded(px(AI_CONFIRM_ICON_WRAP / 2.0))
                                    .border_1()
                                    .border_color(rgba((self.tokens.ui.error << 8) | 0x33))
                                    .bg(rgba((self.tokens.ui.error << 8) | 0x1a))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(Self::render_lucide_icon(
                                        LucideIcon::AlertTriangle,
                                        AI_CONFIRM_ICON,
                                        rgb(self.tokens.ui.error),
                                    )),
                            )
                            .child(
                                div()
                                    .text_align(gpui::TextAlign::Center)
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .line_height(px(20.0))
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(self.i18n.t("settings_view.ai.remove_confirm")),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .border_t_1()
                            .border_color(rgba((self.tokens.ui.border << 8) | 0x66))
                            .child(
                                button_focus_visible(
                                    &self.tokens,
                                    div()
                                    .flex_1()
                                    .py(px(10.0))
                                    .border_r_1()
                                    .border_color(rgba((self.tokens.ui.border << 8) | 0x66))
                                    .text_align(gpui::TextAlign::Center)
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .cursor_pointer()
                                    .hover(|style| {
                                        style
                                            .bg(rgba((self.tokens.ui.bg_hover << 8) | 0x80))
                                            .text_color(rgb(self.tokens.ui.text))
                                    })
                                    .child(self.i18n.t("common.actions.cancel")),
                                    self.standard_confirm_focus()
                                        == Some(ConfirmDialogAction::Cancel),
                                )
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _event, _window, cx| {
                                            this.ai_provider_key_remove_confirm = None;
                                            this.clear_standard_confirm_focus();
                                            cx.stop_propagation();
                                            cx.notify();
                                        }),
                                    ),
                            )
                            .child(
                                button_focus_visible(
                                    &self.tokens,
                                    div()
                                    .flex_1()
                                    .py(px(10.0))
                                    .text_align(gpui::TextAlign::Center)
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(self.tokens.ui.error))
                                    .cursor_pointer()
                                    .hover(|style| {
                                        style
                                            .bg(rgba((self.tokens.ui.error << 8) | 0x1a))
                                            .text_color(rgb(self.tokens.ui.error))
                                    })
                                    .child(self.i18n.t("settings_view.ai.remove")),
                                    self.standard_confirm_focus()
                                        == Some(ConfirmDialogAction::Confirm),
                                )
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _event, _window, cx| {
                                            this.clear_standard_confirm_focus();
                                            if let Some((index, provider_id)) =
                                                this.ai_provider_key_remove_confirm.take()
                                            {
                                                this.remove_ai_provider_api_key(
                                                    index,
                                                    &provider_id,
                                                    cx,
                                                );
                                            }
                                            cx.stop_propagation();
                                        }),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_ai_provider_remove_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let provider_name = self
            .ai_provider_remove_confirm
            .as_ref()
            .map(|(_, name)| name.as_str())
            .unwrap_or_default();
        let title = self
            .i18n
            .t("settings_view.ai.remove_provider_confirm")
            .replace("{{name}}", provider_name);
        dismissible_dialog_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    // Tauri remove-provider confirm is cancellable via
                    // Dialog onOpenChange(false).
                    this.ai_provider_remove_confirm = None;
                    this.clear_standard_confirm_focus();
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(
                dialog_content(&self.tokens)
                    .w(px(AI_KEY_REMOVE_DIALOG_WIDTH))
                    .max_w(relative(0.92))
                    .shadow_lg()
                    .rounded(px(self.tokens.radii.lg))
                    .border_color(rgba((self.tokens.ui.border << 8) | 0x99))
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation();
                    })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(12.0))
                            .px(px(24.0))
                            .pt(px(24.0))
                            .pb(px(16.0))
                            .child(
                                div()
                                    .size(px(AI_CONFIRM_ICON_WRAP))
                                    .rounded(px(AI_CONFIRM_ICON_WRAP / 2.0))
                                    .border_1()
                                    .border_color(rgba((self.tokens.ui.error << 8) | 0x33))
                                    .bg(rgba((self.tokens.ui.error << 8) | 0x1a))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(Self::render_lucide_icon(
                                        LucideIcon::AlertTriangle,
                                        AI_CONFIRM_ICON,
                                        rgb(self.tokens.ui.error),
                                    )),
                            )
                            .child(
                                div()
                                    .text_align(gpui::TextAlign::Center)
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .line_height(px(20.0))
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(title),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .border_t_1()
                            .border_color(rgba((self.tokens.ui.border << 8) | 0x66))
                            .child(
                                button_focus_visible(
                                    &self.tokens,
                                    div()
                                    .flex_1()
                                    .py(px(10.0))
                                    .border_r_1()
                                    .border_color(rgba((self.tokens.ui.border << 8) | 0x66))
                                    .text_align(gpui::TextAlign::Center)
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .cursor_pointer()
                                    .hover(|style| {
                                        style
                                            .bg(rgba((self.tokens.ui.bg_hover << 8) | 0x80))
                                            .text_color(rgb(self.tokens.ui.text))
                                    })
                                    .child(self.i18n.t("common.actions.cancel")),
                                    self.standard_confirm_focus()
                                        == Some(ConfirmDialogAction::Cancel),
                                )
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _event, _window, cx| {
                                            this.ai_provider_remove_confirm = None;
                                            this.clear_standard_confirm_focus();
                                            cx.stop_propagation();
                                            cx.notify();
                                        }),
                                    ),
                            )
                            .child(
                                button_focus_visible(
                                    &self.tokens,
                                    div()
                                    .flex_1()
                                    .py(px(10.0))
                                    .text_align(gpui::TextAlign::Center)
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(self.tokens.ui.error))
                                    .cursor_pointer()
                                    .hover(|style| {
                                        style
                                            .bg(rgba((self.tokens.ui.error << 8) | 0x1a))
                                            .text_color(rgb(self.tokens.ui.error))
                                    })
                                    .child(self.i18n.t("settings_view.ai.remove")),
                                    self.standard_confirm_focus()
                                        == Some(ConfirmDialogAction::Confirm),
                                )
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _event, _window, cx| {
                                            this.clear_standard_confirm_focus();
                                            if let Some((provider_id, _name)) =
                                                this.ai_provider_remove_confirm.take()
                                            {
                                                this.remove_ai_provider(&provider_id, cx);
                                            }
                                            cx.stop_propagation();
                                        }),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn remove_ai_provider(&mut self, provider_id: &str, cx: &mut Context<Self>) {
        let Some(index) = self
            .settings_store
            .settings()
            .ai
            .providers
            .iter()
            .position(|provider| ai_provider_id(provider).as_deref() == Some(provider_id))
        else {
            cx.notify();
            return;
        };
        let provider_id = provider_id.to_string();
        self.ai_provider_key_status.remove(&provider_id);
        self.ai_provider_key_status_pending.remove(&provider_id);
        self.expanded_ai_providers.remove(&provider_id);
        self.expanded_ai_provider_models.remove(&provider_id);
        self.expanded_ai_context_providers.remove(&provider_id);
        self.expanded_ai_model_reasoning_providers
            .remove(&provider_id);
        self.edit_settings(
            |settings| {
                ai_remove_provider_at_with_scoped_settings(
                    &mut settings.ai.providers,
                    &mut settings.ai.active_provider_id,
                    &mut settings.ai.active_model,
                    &mut settings.ai.reasoning_provider_overrides,
                    &mut settings.ai.reasoning_model_overrides,
                    &mut settings.ai.user_context_windows,
                    &mut settings.ai.model_max_response_tokens,
                    index,
                );
            },
            cx,
        );

        let key_store = self.ai_key_store.clone();
        let runtime = self.forwarding_runtime.clone();
        cx.spawn(async move |weak, cx| {
            let provider_id_for_delete = provider_id.clone();
            let result = runtime
                .spawn_blocking(move || key_store.delete_provider_key(&provider_id_for_delete))
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));
            if let Err(error) = result {
                let _ = weak.update(cx, |this, cx| {
                    this.push_ai_settings_toast(
                        this.ai_i18n_error("settings_view.ai.remove_failed", &error),
                        TerminalNoticeVariant::Error,
                    );
                    cx.notify();
                });
            }
        })
        .detach();
    }

}
