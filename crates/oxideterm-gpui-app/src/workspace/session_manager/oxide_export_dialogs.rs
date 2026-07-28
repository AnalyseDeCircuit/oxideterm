use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn render_oxide_export_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let Some((
            connection_count,
            dialog_visible,
            has_export_content,
            embed_keys,
            include_passwords,
            description,
            progress_stage,
            result_summary,
            error,
        )) = ({
            self.session_manager
                .read(cx)
                .oxide_export_dialog
                .as_ref()
                .map(|dialog| {
                    (
                        dialog.selected_ids.len(),
                        dialog.presence.phase() == oxideterm_gpui_ui::motion::ExitPhase::Visible,
                        oxide_export_connection_count(dialog) > 0
                            || dialog.include_portable_secrets,
                        dialog.embed_keys,
                        dialog.include_passwords,
                        dialog.description.clone(),
                        dialog.progress_stage.clone(),
                        dialog.result_summary.clone(),
                        dialog.error.clone(),
                    )
                })
        })
        else {
            return div().into_any_element();
        };
        let connections = self.connection_store.connections();
        dismissible_dialog_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    // Tauri OxideExportModal uses Dialog onOpenChange(onClose);
                    // native backdrop clicks follow the same close path.
                    this.begin_oxide_export_dialog_exit(cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(oxideterm_gpui_ui::motion::form_transition(
                &self.tokens,
                "oxide-export-dialog-transition",
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
                            .child(self.render_oxide_export_options(cx))
                            .child(self.render_oxide_export_preflight(
                                has_export_content,
                                embed_keys,
                                include_passwords,
                                cx,
                            ))
                            .child(self.render_oxide_labeled_input(
                                "描述（可选）".to_string(),
                                self.render_session_text_input(
                                    SessionManagerInput::OxideExportDescription,
                                    &description,
                                    "例如：生产服务器".to_string(),
                                    cx,
                                ),
                                cx,
                            ))
                            .child(self.render_oxide_export_credential_options(cx))
                            .child(self.render_oxide_export_content_summary(cx))
                            .child(self.render_oxide_export_password_input(cx))
                            .child(self.render_oxide_labeled_input(
                                "确认密码 *".to_string(),
                                self.render_session_password_input(
                                    SessionManagerInput::OxideExportConfirmPassword,
                                    "重新输入密码".to_string(),
                                    cx,
                                ),
                                cx,
                            ))
                            .child(self.render_oxide_security_notice(cx))
                            .when_some(progress_stage, |body, progress| {
                                body.child(self.render_oxide_progress(
                                    progress,
                                    Some(embed_keys),
                                    cx,
                                ))
                            })
                            .when_some(result_summary, |body, result| {
                                body.child(self.render_oxide_status_line(result, false, cx))
                            })
                            .when_some(error, |body, error| {
                                body.child(self.render_oxide_error_banner(error, cx))
                            })
                            .child(self.render_oxide_export_footer(cx)),
                    ),
                dialog_visible,
            ))
            .into_any_element()
    }

    pub(super) fn render_oxide_export_credential_options(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (
            include_passwords,
            embed_keys,
            include_key_passphrases,
            include_managed_keys,
            include_managed_key_passphrases,
        ) = self
            .session_manager
            .read(cx)
            .oxide_export_dialog
            .as_ref()
            .map(|dialog| {
                (
                    dialog.include_passwords,
                    dialog.embed_keys,
                    dialog.include_key_passphrases,
                    dialog.include_managed_keys,
                    dialog.include_managed_key_passphrases,
                )
            })
            .unwrap_or_default();
        self.render_oxide_card(
            Some((LucideIcon::Key, self.i18n.t("export.credential_material"))),
            vec![
                self.render_oxide_option_row(
                    self.i18n.t("export.include_passwords"),
                    self.i18n.t("export.include_passwords_description"),
                    include_passwords,
                    cx.listener(|this, _event, _window, cx| {
                        this.session_manager.update(cx, |manager, cx| {
                            if let Some(dialog) = manager.oxide_export_dialog.as_mut() {
                                dialog.include_passwords = !dialog.include_passwords;
                                cx.notify();
                            }
                        });
                        this.refresh_oxide_export_preflight(cx);
                        cx.stop_propagation();
                    }),
                    cx,
                ),
                self.render_oxide_option_row(
                    self.i18n.t("export.embed_keys"),
                    self.i18n.t("export.embed_keys_description"),
                    embed_keys,
                    cx.listener(|this, _event, _window, cx| {
                        this.session_manager.update(cx, |manager, cx| {
                            if let Some(dialog) = manager.oxide_export_dialog.as_mut() {
                                dialog.embed_keys = !dialog.embed_keys;
                                cx.notify();
                            }
                        });
                        this.refresh_oxide_export_preflight(cx);
                        cx.stop_propagation();
                    }),
                    cx,
                ),
                self.render_oxide_option_row(
                    self.i18n.t("export.include_key_passphrases"),
                    self.i18n.t("export.include_key_passphrases_description"),
                    include_key_passphrases,
                    cx.listener(|this, _event, _window, cx| {
                        this.session_manager.update(cx, |manager, cx| {
                            if let Some(dialog) = manager.oxide_export_dialog.as_mut() {
                                dialog.include_key_passphrases = !dialog.include_key_passphrases;
                                cx.notify();
                            }
                        });
                        this.refresh_oxide_export_preflight(cx);
                        cx.stop_propagation();
                    }),
                    cx,
                ),
                self.render_oxide_option_row(
                    self.i18n.t("export.include_managed_keys"),
                    self.i18n.t("export.include_managed_keys_description"),
                    include_managed_keys,
                    cx.listener(|this, _event, _window, cx| {
                        this.session_manager.update(cx, |manager, cx| {
                            if let Some(dialog) = manager.oxide_export_dialog.as_mut() {
                                dialog.include_managed_keys = !dialog.include_managed_keys;
                                if !dialog.include_managed_keys {
                                    dialog.include_managed_key_passphrases = false;
                                }
                                cx.notify();
                            }
                        });
                        this.refresh_oxide_export_preflight(cx);
                        cx.stop_propagation();
                    }),
                    cx,
                ),
                div()
                    .opacity(if include_managed_keys { 1.0 } else { 0.45 })
                    .child(
                        self.render_oxide_option_row(
                            self.i18n.t("export.include_managed_key_passphrases"),
                            self.i18n
                                .t("export.include_managed_key_passphrases_description"),
                            include_managed_key_passphrases,
                            cx.listener(|this, _event, _window, cx| {
                                this.session_manager.update(cx, |manager, cx| {
                                    if let Some(dialog) = manager.oxide_export_dialog.as_mut()
                                        && dialog.include_managed_keys
                                    {
                                        dialog.include_managed_key_passphrases =
                                            !dialog.include_managed_key_passphrases;
                                        cx.notify();
                                    }
                                });
                                this.refresh_oxide_export_preflight(cx);
                                cx.stop_propagation();
                            }),
                            cx,
                        ),
                    )
                    .into_any_element(),
            ],
            cx,
        )
    }
}
