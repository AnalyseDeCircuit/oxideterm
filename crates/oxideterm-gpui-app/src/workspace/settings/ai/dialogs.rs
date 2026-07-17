use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn render_ai_text_editor_modal(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let dialog = self
            .ai_text_editor_dialog
            .expect("AI text editor is rendered only while a dialog is open");
        let editor = self
            .ai_text_editor
            .as_ref()
            .expect("AI text editor entity exists while its dialog is open")
            .clone();
        let (title_key, description_key) = match dialog {
            AiTextEditorDialog::SystemPrompt => (
                "settings_view.ai.system_prompt_title",
                "settings_view.ai.system_prompt_hint",
            ),
            AiTextEditorDialog::Memory => (
                "settings_view.ai.memory_title",
                "settings_view.ai.memory_hint",
            ),
        };
        let modal = oxideterm_gpui_ui::modal_container(&self.tokens)
            .w(px(AI_TEXT_EDITOR_MODAL_WIDTH))
            .max_w_full()
            .h(px(AI_TEXT_EDITOR_MODAL_HEIGHT))
            .max_h_full()
            .shadow(oxideterm_gpui_ui::theme_overlay_shadow(&self.tokens))
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
                if event.keystroke.key.as_str() == "escape" {
                    this.close_ai_text_editor(false, cx);
                    cx.stop_propagation();
                }
            }))
            .child(
                dialog_header(&self.tokens)
                    .child(dialog_title(&self.tokens, self.i18n.t(title_key)))
                    .child(dialog_description(
                        &self.tokens,
                        self.i18n.t(description_key),
                    )),
            )
            .child(
                // TextEditorView owns the sole document scroll viewport. The
                // modal body only constrains its available layout rectangle.
                oxideterm_gpui_ui::modal::modal_body(&self.tokens)
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_hidden()
                    .child(editor),
            )
            .child(
                dialog_footer(&self.tokens)
                    .justify_between()
                    .child(if dialog == AiTextEditorDialog::Memory {
                        self.workspace_toolbar_action_button(
                            self.i18n.t("settings_view.ai.memory_clear"),
                            None,
                            ToolbarButtonOptions {
                                button: ButtonOptions {
                                    variant: ButtonVariant::Ghost,
                                    size: ButtonSize::Sm,
                                    radius: ButtonRadius::Md,
                                    disabled: false,
                                },
                                ..ToolbarButtonOptions::default()
                            },
                            cx.listener(|this, _event, _window, cx| {
                                if let Some(editor) = this.ai_text_editor.clone() {
                                    editor.update(cx, |editor, cx| {
                                        editor.replace_text_external(String::new(), cx);
                                    });
                                }
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )
                        .into_any_element()
                    } else {
                        div().into_any_element()
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(self.tokens.metrics.modal_field_gap))
                            .child(self.workspace_toolbar_action_button(
                                self.i18n.t("settings_view.ai.editor_cancel"),
                                None,
                                ToolbarButtonOptions {
                                    button: ButtonOptions {
                                        variant: ButtonVariant::Outline,
                                        size: ButtonSize::Sm,
                                        radius: ButtonRadius::Md,
                                        disabled: false,
                                    },
                                    ..ToolbarButtonOptions::default()
                                },
                                cx.listener(|this, _event, _window, cx| {
                                    this.close_ai_text_editor(false, cx);
                                    cx.stop_propagation();
                                }),
                            ))
                            .child(self.workspace_toolbar_action_button(
                                self.i18n.t("settings_view.ai.editor_save"),
                                None,
                                ToolbarButtonOptions {
                                    button: ButtonOptions {
                                        variant: ButtonVariant::Default,
                                        size: ButtonSize::Sm,
                                        radius: ButtonRadius::Md,
                                        disabled: false,
                                    },
                                    ..ToolbarButtonOptions::default()
                                },
                                cx.listener(|this, _event, _window, cx| {
                                    this.close_ai_text_editor(true, cx);
                                    cx.stop_propagation();
                                }),
                            )),
                    ),
            );

        dismissible_dialog_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.close_ai_text_editor(false, cx);
                    cx.stop_propagation();
                }),
            )
            .child(overlay_content_boundary(modal))
            .into_any_element()
    }

    pub(in crate::workspace) fn open_ai_text_editor(
        &mut self,
        dialog: AiTextEditorDialog,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let value = match dialog {
            AiTextEditorDialog::SystemPrompt => self
                .settings_store
                .settings()
                .ai
                .custom_system_prompt
                .clone(),
            AiTextEditorDialog::Memory => self.settings_store.settings().ai.memory.content.clone(),
        };
        self.prepare_modal_interaction_boundary();
        let tokens = self.tokens;
        let runtime_settings = self.ide_runtime_settings();
        let placeholder = self.i18n.t(match dialog {
            AiTextEditorDialog::SystemPrompt => "settings_view.ai.system_prompt_placeholder",
            AiTextEditorDialog::Memory => "settings_view.ai.memory_placeholder",
        });
        let context_menu_labels = oxideterm_gpui_editor::EditorContextMenuLabels {
            copy: self.i18n.t("menu.copy"),
            cut: self.i18n.t("fileManager.cut"),
            paste: self.i18n.t("menu.paste"),
            select_all: self.i18n.t("fileManager.selectAll"),
        };
        let workspace = cx.entity();
        let editor = cx.new(|cx| {
            let mut editor = oxideterm_gpui_editor::TextEditorView::new(value, &tokens, cx);
            let mut editor_settings = oxideterm_gpui_editor::EditorSettings::default();
            editor_settings.soft_wrap = true;
            editor_settings.indentation_markers = false;
            editor_settings.highlight_special_chars = false;
            editor_settings.placeholder = Some(placeholder);
            editor.set_settings(editor_settings, cx);
            editor.set_context_menu_labels(context_menu_labels);
            editor.apply_ide_runtime_settings(
                &tokens,
                runtime_settings.editor_font_size,
                runtime_settings.editor_line_height,
                true,
                runtime_settings.background_active,
                cx,
            );
            editor.set_on_save(Box::new(move |text, _window, cx| {
                let text = text.to_string();
                let _ = workspace.update(cx, |this, cx| {
                    this.persist_ai_text_editor(dialog, text, cx);
                });
                Ok(())
            }));
            editor
        });
        let focus_handle = editor.read(cx).focus_handle(cx);
        self.ai_text_editor_dialog = Some(dialog);
        self.ai_text_editor = Some(editor);
        window.focus(&focus_handle, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn close_ai_text_editor(
        &mut self,
        save: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = self.ai_text_editor_dialog.take() else {
            return;
        };
        let editor = self.ai_text_editor.take();
        if save && let Some(editor) = editor {
            let text = editor.read(cx).buffer().text();
            self.persist_ai_text_editor(dialog, text, cx);
        }
        cx.notify();
    }

    fn persist_ai_text_editor(
        &mut self,
        dialog: AiTextEditorDialog,
        text: String,
        cx: &mut Context<Self>,
    ) {
        // The editor owns transient text; persistence remains scoped to the
        // selected AI document and never mutates the other modal draft.
        self.edit_settings(
            move |settings| match dialog {
                AiTextEditorDialog::SystemPrompt => settings.ai.custom_system_prompt = text,
                AiTextEditorDialog::Memory => settings.ai.memory.content = text,
            },
            cx,
        );
    }

    pub(in crate::workspace) fn handle_ai_settings_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.settings_page.show_ai_enable_confirm {
            match self.handle_standard_confirm_key(event, cx) {
                Some(ConfirmKeyboardAction::Cancel) => {
                    self.close_ai_settings_dialog(false, cx);
                    true
                }
                Some(ConfirmKeyboardAction::Confirm) => {
                    self.close_ai_settings_dialog(true, cx);
                    true
                }
                Some(ConfirmKeyboardAction::Handled) => true,
                None => false,
            }
        } else if self.settings_page.ai_provider_key_remove_confirm.is_some() {
            match self.handle_standard_confirm_key(event, cx) {
                Some(ConfirmKeyboardAction::Cancel) => {
                    self.close_ai_settings_dialog(false, cx);
                    true
                }
                Some(ConfirmKeyboardAction::Confirm) => {
                    self.close_ai_settings_dialog(true, cx);
                    true
                }
                Some(ConfirmKeyboardAction::Handled) => true,
                None => false,
            }
        } else if self.settings_page.ai_provider_remove_confirm.is_some() {
            match self.handle_standard_confirm_key(event, cx) {
                Some(ConfirmKeyboardAction::Cancel) => {
                    self.close_ai_settings_dialog(false, cx);
                    true
                }
                Some(ConfirmKeyboardAction::Confirm) => {
                    self.close_ai_settings_dialog(true, cx);
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
        let is_visible = self.ai_settings_dialog_presence.phase()
            == oxideterm_gpui_ui::motion::ExitPhase::Visible;
        dismissible_dialog_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    // Tauri SettingsView AI confirm is a Radix Dialog bound to
                    // setShowAiConfirm, so outside click is the Cancel path.
                    this.close_ai_settings_dialog(false, cx);
                    this.clear_standard_confirm_focus();
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(oxideterm_gpui_ui::motion::form_transition(
                &self.tokens,
                "ai-enable-confirm-form",
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
                                    .child(
                                        self.ai_confirm_bullet(
                                            "settings_view.ai_confirm.point_local",
                                        ),
                                    )
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
                            .child(self.standard_footer_action_button(
                                self.i18n.t("settings_view.ai_confirm.cancel"),
                                ButtonVariant::Ghost,
                                ConfirmDialogAction::Cancel,
                                false,
                                |this, _event, _window, cx| {
                                    this.close_ai_settings_dialog(false, cx);
                                },
                                cx,
                            ))
                            .child(self.standard_footer_action_button(
                                self.i18n.t("settings_view.ai_confirm.enable"),
                                ButtonVariant::Default,
                                ConfirmDialogAction::Confirm,
                                false,
                                |this, _event, _window, cx| {
                                    this.close_ai_settings_dialog(true, cx);
                                },
                                cx,
                            )),
                    ),
                is_visible,
            ))
            .when(!is_visible, settings_dialog_inert_overlay)
            .into_any_element()
    }

    pub(in crate::workspace) fn ai_confirm_bullet(&self, label_key: &str) -> AnyElement {
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
        let is_visible = self.ai_settings_dialog_presence.phase()
            == oxideterm_gpui_ui::motion::ExitPhase::Visible;
        dismissible_dialog_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    // Tauri provider-key removal uses the shared confirm
                    // dialog; outside close cancels the pending removal.
                    this.close_ai_settings_dialog(false, cx);
                    this.clear_standard_confirm_focus();
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(oxideterm_gpui_ui::motion::form_transition(
                &self.tokens,
                "ai-provider-key-remove-form",
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
                            .child(self.split_confirm_footer_action_button(
                                self.i18n.t("common.actions.cancel"),
                                ConfirmDialogAction::Cancel,
                                false,
                                true,
                                |this, _event, _window, cx| {
                                    this.close_ai_settings_dialog(false, cx);
                                },
                                cx,
                            ))
                            .child(self.split_confirm_footer_action_button(
                                self.i18n.t("settings_view.ai.remove"),
                                ConfirmDialogAction::Confirm,
                                true,
                                false,
                                |this, _event, _window, cx| {
                                    this.close_ai_settings_dialog(true, cx);
                                },
                                cx,
                            )),
                    ),
                is_visible,
            ))
            .when(!is_visible, settings_dialog_inert_overlay)
            .into_any_element()
    }

    pub(in crate::workspace) fn render_ai_provider_remove_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let provider_name = self
            .settings_page
            .ai_provider_remove_confirm
            .as_ref()
            .map(|(_, name)| name.as_str())
            .unwrap_or_default();
        let title = self
            .i18n
            .t("settings_view.ai.remove_provider_confirm")
            .replace("{{name}}", provider_name);
        let is_visible = self.ai_settings_dialog_presence.phase()
            == oxideterm_gpui_ui::motion::ExitPhase::Visible;
        dismissible_dialog_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    // Tauri remove-provider confirm is cancellable via
                    // Dialog onOpenChange(false).
                    this.close_ai_settings_dialog(false, cx);
                    this.clear_standard_confirm_focus();
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(oxideterm_gpui_ui::motion::form_transition(
                &self.tokens,
                "ai-provider-remove-form",
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
                            .child(self.split_confirm_footer_action_button(
                                self.i18n.t("common.actions.cancel"),
                                ConfirmDialogAction::Cancel,
                                false,
                                true,
                                |this, _event, _window, cx| {
                                    this.close_ai_settings_dialog(false, cx);
                                },
                                cx,
                            ))
                            .child(self.split_confirm_footer_action_button(
                                self.i18n.t("settings_view.ai.remove"),
                                ConfirmDialogAction::Confirm,
                                true,
                                false,
                                |this, _event, _window, cx| {
                                    this.close_ai_settings_dialog(true, cx);
                                },
                                cx,
                            )),
                    ),
                is_visible,
            ))
            .when(!is_visible, settings_dialog_inert_overlay)
            .into_any_element()
    }

    pub(in crate::workspace) fn remove_ai_provider(
        &mut self,
        provider_id: &str,
        cx: &mut Context<Self>,
    ) {
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
        self.ai.models.provider_key_status.remove(&provider_id);
        self.ai
            .models
            .provider_key_status_pending
            .remove(&provider_id);
        self.settings_page
            .remove_ai_provider_page_state(&provider_id);
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

        let key_store = self.ai.models.key_store.clone();
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

    /// Defers payload removal until the matching exit generation completes.
    pub(in crate::workspace) fn close_ai_settings_dialog(
        &mut self,
        confirm: bool,
        cx: &mut Context<Self>,
    ) {
        self.clear_standard_confirm_focus();
        let Some(generation) = self.ai_settings_dialog_presence.begin_exit() else {
            return;
        };
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        if delay.is_zero() {
            self.finish_ai_settings_dialog_exit(generation, confirm, cx);
            cx.notify();
            return;
        }
        cx.spawn(async move |weak, cx| {
            gpui::Timer::after(delay).await;
            let _ = weak.update(cx, |this, cx| {
                if this.finish_ai_settings_dialog_exit(generation, confirm, cx) {
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn finish_ai_settings_dialog_exit(
        &mut self,
        generation: u64,
        confirm: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.ai_settings_dialog_presence.finish_exit(generation) {
            return false;
        }
        if self.settings_page.show_ai_enable_confirm {
            self.settings_page.set_ai_enable_confirm_open(false);
            if confirm {
                self.edit_settings(
                    |settings| {
                        settings.ai.enabled = true;
                        settings.ai.enabled_confirmed = true;
                    },
                    cx,
                );
            }
        } else if self.settings_page.ai_provider_key_remove_confirm.is_some() {
            let target = self.settings_page.take_ai_provider_key_remove();
            if confirm && let Some((index, provider_id)) = target {
                self.remove_ai_provider_api_key(index, &provider_id, cx);
            }
        } else if self.settings_page.ai_provider_remove_confirm.is_some() {
            let target = self.settings_page.take_ai_provider_remove();
            if confirm && let Some((provider_id, _name)) = target {
                self.remove_ai_provider(&provider_id, cx);
            }
        }
        self.ai_settings_dialog_presence.reopen();
        true
    }
}
