impl WorkspaceApp {
    fn render_knowledge_create_collection_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.settings_page.knowledge_create_dialog_open {
            return None;
        }
        let can_create = !self.settings_page.knowledge_new_collection_name.trim().is_empty();
        Some(
            dismissible_dialog_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        // Tauri DocumentManager uses Dialog onOpenChange for
                        // create collection; outside close matches Cancel.
                        this.settings_page.close_knowledge_create_dialog();
                        this.clear_standard_confirm_focus();
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(
                    dialog_content(&self.tokens)
                        .w(px(KNOWLEDGE_DIALOG_WIDTH))
                        .max_w(relative(0.92))
                        .shadow_lg()
                        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                            cx.stop_propagation();
                        })
                        .child(
                            dialog_header(&self.tokens)
                                .child(dialog_title(
                                    &self.tokens,
                                    self.i18n.t("settings_view.knowledge.create_collection"),
                                ))
                                .child(dialog_description(
                                    &self.tokens,
                                    self.i18n.t("settings_view.knowledge.create_description"),
                                )),
                        )
                        .child(
                            div()
                                .px(px(24.0))
                                .py(px(18.0))
                                .flex()
                                .flex_col()
                                .gap(px(12.0))
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(8.0))
                                        .child(
                                            div()
                                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                                .font_weight(gpui::FontWeight::MEDIUM)
                                                .text_color(rgb(self.tokens.ui.text))
                                                .child(self.i18n.t("settings_view.knowledge.collection_name")),
                                        )
                                        .child(self.settings_text_input_control(
                                            SettingsInput::KnowledgeCollectionName,
                                            self.settings_page.knowledge_new_collection_name.clone(),
                                            self.i18n
                                                .t("settings_view.knowledge.collection_name_placeholder"),
                                            420.0,
                                            cx,
                                        )),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(8.0))
                                        .child(
                                            div()
                                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                                .font_weight(gpui::FontWeight::MEDIUM)
                                                .text_color(rgb(self.tokens.ui.text))
                                                .child(self.i18n.t("settings_view.knowledge.scope")),
                                        )
                                        .child(self.ai_settings_select_control(
                                            SettingsSelect::KnowledgeCollectionScope,
                                            self.i18n.t("settings_view.knowledge.scope_global"),
                                            420.0,
                                            cx,
                                        )),
                                ),
                        )
                        .child(
                            dialog_footer(&self.tokens)
                                .child(
                                    self.standard_footer_action_button(
                                        self.i18n.t("common.actions.cancel"),
                                        ButtonVariant::Outline,
                                        ConfirmDialogAction::Cancel,
                                        false,
                                        |this, _event, _window, cx| {
                                            this.settings_page.close_knowledge_create_dialog();
                                            cx.notify();
                                        },
                                        cx,
                                    ),
                                )
                                .child(
                                    self.standard_footer_action_button(
                                        self.i18n.t("settings_view.knowledge.create_collection"),
                                        ButtonVariant::Default,
                                        ConfirmDialogAction::Confirm,
                                        !can_create,
                                        |this, _event, _window, cx| {
                                            this.knowledge_create_collection(cx);
                                            this.settings_page.hide_knowledge_create_dialog();
                                        },
                                        cx,
                                    ),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }

    fn render_knowledge_new_document_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.settings_page.knowledge_new_document_dialog_open {
            return None;
        }
        let can_create = !self.settings_page.knowledge_new_document_title.trim().is_empty();
        Some(
            dismissible_dialog_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        // Tauri new-document Dialog closes through
                        // setNewDocDialogOpen(false) on backdrop click.
                        this.settings_page.close_knowledge_new_document_dialog();
                        this.clear_standard_confirm_focus();
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(
                    dialog_content(&self.tokens)
                        .w(px(KNOWLEDGE_DIALOG_WIDTH))
                        .max_w(relative(0.92))
                        .shadow_lg()
                        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                            cx.stop_propagation();
                        })
                        .child(
                            dialog_header(&self.tokens)
                                .child(dialog_title(
                                    &self.tokens,
                                    self.i18n.t("settings_view.knowledge.new_document"),
                                ))
                                .child(dialog_description(
                                    &self.tokens,
                                    self.i18n.t("settings_view.knowledge.new_document_description"),
                                )),
                        )
                        .child(
                            div()
                                .px(px(24.0))
                                .py(px(18.0))
                                .flex()
                                .flex_col()
                                .gap(px(12.0))
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(8.0))
                                        .child(
                                            div()
                                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                                .font_weight(gpui::FontWeight::MEDIUM)
                                                .text_color(rgb(self.tokens.ui.text))
                                                .child(self.i18n.t("settings_view.knowledge.new_document_title")),
                                        )
                                        .child(self.settings_text_input_control(
                                            SettingsInput::KnowledgeDocumentTitle,
                                            self.settings_page.knowledge_new_document_title.clone(),
                                            self.i18n
                                                .t("settings_view.knowledge.new_document_title_placeholder"),
                                            420.0,
                                            cx,
                                        )),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(8.0))
                                        .child(
                                            div()
                                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                                .font_weight(gpui::FontWeight::MEDIUM)
                                                .text_color(rgb(self.tokens.ui.text))
                                                .child(self.i18n.t("settings_view.knowledge.format")),
                                        )
                                        .child(self.ai_settings_select_control(
                                            SettingsSelect::KnowledgeDocumentFormat,
                                            self.knowledge_document_format_label(),
                                            420.0,
                                            cx,
                                        )),
                                ),
                        )
                        .child(
                            dialog_footer(&self.tokens)
                                .child(
                                    self.standard_footer_action_button(
                                        self.i18n.t("common.actions.cancel"),
                                        ButtonVariant::Outline,
                                        ConfirmDialogAction::Cancel,
                                        false,
                                        |this, _event, _window, cx| {
                                            this.settings_page.close_knowledge_new_document_dialog();
                                            cx.notify();
                                        },
                                        cx,
                                    ),
                                )
                                .child(
                                    self.standard_footer_action_button(
                                        self.i18n.t("settings_view.knowledge.new_document"),
                                        ButtonVariant::Default,
                                        ConfirmDialogAction::Confirm,
                                        !can_create,
                                        |this, _event, _window, cx| {
                                            this.knowledge_create_blank_document(cx);
                                            this.settings_page.hide_knowledge_new_document_dialog();
                                        },
                                        cx,
                                    ),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }

    fn render_knowledge_delete_confirm_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let confirm = self.settings_page.knowledge_delete_confirm.as_ref()?;
        let message_key = match confirm.target {
            KnowledgeDeleteTarget::Collection => "settings_view.knowledge.delete_collection_confirm",
            KnowledgeDeleteTarget::Document => "settings_view.knowledge.delete_document_confirm",
        };
        let message = self
            .i18n
            .t(message_key)
            .replace("{{name}}", &confirm.name);
        Some(
            dismissible_dialog_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        // Tauri delete confirm uses onOpenChange(false) to
                        // clear the pending delete target.
                        this.settings_page.clear_knowledge_delete_confirm();
                        this.clear_standard_confirm_focus();
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(
                    dialog_content(&self.tokens)
                        .w(px(KNOWLEDGE_DIALOG_WIDTH))
                        .max_w(relative(0.92))
                        .shadow_lg()
                        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                            cx.stop_propagation();
                        })
                        .child(
                            dialog_header(&self.tokens)
                                .child(dialog_title(
                                    &self.tokens,
                                    self.i18n.t("settings_view.knowledge.delete_confirm_title"),
                                ))
                                .child(dialog_description(&self.tokens, message)),
                        )
                        .child(
                            dialog_footer(&self.tokens)
                                .child(
                                    self.standard_footer_action_button(
                                        self.i18n.t("common.actions.cancel"),
                                        ButtonVariant::Outline,
                                        ConfirmDialogAction::Cancel,
                                        false,
                                        |this, _event, _window, cx| {
                                            this.settings_page.clear_knowledge_delete_confirm();
                                            cx.notify();
                                        },
                                        cx,
                                    ),
                                )
                                .child(
                                    self.standard_footer_action_button(
                                        self.i18n.t("common.delete"),
                                        ButtonVariant::Destructive,
                                        ConfirmDialogAction::Confirm,
                                        false,
                                        |this, _event, _window, cx| {
                                            this.knowledge_confirm_delete(cx);
                                        },
                                        cx,
                                    ),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }

}
