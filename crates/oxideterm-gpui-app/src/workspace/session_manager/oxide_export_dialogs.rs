impl WorkspaceApp {
    pub(in crate::workspace) fn render_oxide_export_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let Some(dialog) = self.session_manager.oxide_export_dialog.as_ref() else {
            return div().into_any_element();
        };
        let connection_count = dialog.selected_ids.len();
        let connections = self.connection_store.connections();
        dismissible_dialog_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    // Tauri OxideExportModal uses Dialog onOpenChange(onClose);
                    // native backdrop clicks follow the same close path.
                    this.session_manager.oxide_export_dialog = None;
                    this.session_manager.focused_input = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(
                div()
                    .w(px(OXIDE_MODAL_WIDTH))
                    .max_h(relative(OXIDE_MODAL_MAX_HEIGHT_RATIO))
                    .flex()
                    .flex_col()
                    .rounded(px(self.tokens.radii.lg))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.bg_panel))
                    .overflow_hidden()
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation();
                    })
                    .child(
                        div()
                            .px(px(OXIDE_MODAL_HEADER_PX))
                            .py(px(OXIDE_MODAL_HEADER_PY))
                            .flex()
                            .items_center()
                            .justify_between()
                            .border_b_1()
                            .border_color(rgb(theme.border))
                            .child(
                                div()
                                    .text_size(px(20.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(theme.text_heading))
                                    .child(self.render_display_text_with_role(
                                        SelectableTextRole::PlainDocument,
                                        "oxide-export-dialog",
                                        "title",
                                        "导出配置到 .oxide 文件",
                                        theme.text_heading,
                                        cx,
                                    )),
                            )
                            .child(self.render_oxide_close_button(false, cx)),
                    )
                    .child(
                        div()
                            .id("oxide-export-dialog-scroll")
                            .flex_1()
                            .min_h(px(0.0))
                            .selectable_overflow_y_scroll(
                                &self.selectable_text_scroll_handle("oxide-export-dialog-scroll"),
                            )
                            .p(px(OXIDE_MODAL_BODY_P))
                            .flex()
                            .flex_col()
                            .gap(px(OXIDE_MODAL_SECTION_GAP))
                            .child(self.render_oxide_connection_selection(
                                &connections,
                                connection_count,
                                cx,
                            ))
                            .child(self.render_oxide_export_options(dialog, cx))
                            .child(self.render_oxide_export_preflight(
                                dialog.preflight.clone(),
                                oxide_export_connection_count(dialog) > 0
                                    || dialog.include_portable_secrets,
                                dialog.embed_keys,
                                dialog.include_passwords,
                                cx,
                            ))
                            .child(self.render_oxide_labeled_input(
                                "描述（可选）".to_string(),
                                self.render_session_text_input(
                                    SessionManagerInput::OxideExportDescription,
                                    &dialog.description,
                                    "例如：生产服务器".to_string(),
                                    cx,
                                ),
                                cx,
                            ))
                            .child(self.render_oxide_export_credential_options(dialog, cx))
                            .child(self.render_oxide_export_content_summary(dialog, cx))
                            .child(self.render_oxide_export_password_input(dialog, cx))
                            .child(self.render_oxide_labeled_input(
                                "确认密码 *".to_string(),
                                self.render_session_password_input(
                                    SessionManagerInput::OxideExportConfirmPassword,
                                    &dialog.confirm_password,
                                    "重新输入密码".to_string(),
                                    cx,
                                ),
                                cx,
                            ))
                            .child(self.render_oxide_security_notice(dialog, cx))
                            .when_some(dialog.progress_stage.clone(), |body, progress| {
                                body.child(self.render_oxide_progress(progress, Some(dialog.embed_keys), cx))
                            })
                            .when_some(dialog.result_summary.clone(), |body, result| {
                                body.child(self.render_oxide_status_line(result, false, cx))
                            })
                            .when_some(dialog.error.clone(), |body, error| {
                                body.child(self.render_oxide_error_banner(error, cx))
                            })
                            .child(self.render_oxide_export_footer(dialog, cx)),
                    )
            )
            .into_any_element()
    }

    fn render_oxide_export_credential_options(
        &self,
        dialog: &OxideExportDialogState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_oxide_card(
            Some((LucideIcon::Key, self.i18n.t("modals.export.credential_material"))),
            vec![
                self.render_oxide_option_row(
                    self.i18n.t("modals.export.include_passwords"),
                    self.i18n.t("modals.export.include_passwords_description"),
                    dialog.include_passwords,
                    cx.listener(|this, _event, _window, cx| {
                        if let Some(dialog) = this.session_manager.oxide_export_dialog.as_mut() {
                            dialog.include_passwords = !dialog.include_passwords;
                        }
                        this.refresh_oxide_export_preflight();
                        cx.notify();
                        cx.stop_propagation();
                    }),
                    cx,
                ),
                self.render_oxide_option_row(
                    self.i18n.t("modals.export.embed_keys"),
                    self.i18n.t("modals.export.embed_keys_description"),
                    dialog.embed_keys,
                    cx.listener(|this, _event, _window, cx| {
                        if let Some(dialog) = this.session_manager.oxide_export_dialog.as_mut() {
                            dialog.embed_keys = !dialog.embed_keys;
                        }
                        this.refresh_oxide_export_preflight();
                        cx.notify();
                        cx.stop_propagation();
                    }),
                    cx,
                ),
                self.render_oxide_option_row(
                    self.i18n.t("modals.export.include_key_passphrases"),
                    self.i18n
                        .t("modals.export.include_key_passphrases_description"),
                    dialog.include_key_passphrases,
                    cx.listener(|this, _event, _window, cx| {
                        if let Some(dialog) = this.session_manager.oxide_export_dialog.as_mut() {
                            dialog.include_key_passphrases = !dialog.include_key_passphrases;
                        }
                        this.refresh_oxide_export_preflight();
                        cx.notify();
                        cx.stop_propagation();
                    }),
                    cx,
                ),
                self.render_oxide_option_row(
                    self.i18n.t("modals.export.include_managed_keys"),
                    self.i18n.t("modals.export.include_managed_keys_description"),
                    dialog.include_managed_keys,
                    cx.listener(|this, _event, _window, cx| {
                        if let Some(dialog) = this.session_manager.oxide_export_dialog.as_mut() {
                            dialog.include_managed_keys = !dialog.include_managed_keys;
                            if !dialog.include_managed_keys {
                                dialog.include_managed_key_passphrases = false;
                            }
                        }
                        this.refresh_oxide_export_preflight();
                        cx.notify();
                        cx.stop_propagation();
                    }),
                    cx,
                ),
                div()
                    .opacity(if dialog.include_managed_keys { 1.0 } else { 0.45 })
                    .child(self.render_oxide_option_row(
                        self.i18n.t("modals.export.include_managed_key_passphrases"),
                        self.i18n
                            .t("modals.export.include_managed_key_passphrases_description"),
                        dialog.include_managed_key_passphrases,
                        cx.listener(|this, _event, _window, cx| {
                            if let Some(dialog) = this.session_manager.oxide_export_dialog.as_mut()
                            {
                                if dialog.include_managed_keys {
                                    dialog.include_managed_key_passphrases =
                                        !dialog.include_managed_key_passphrases;
                                }
                            }
                            this.refresh_oxide_export_preflight();
                            cx.notify();
                            cx.stop_propagation();
                        }),
                        cx,
                    ))
                    .into_any_element(),
            ],
            cx,
        )
    }


}
