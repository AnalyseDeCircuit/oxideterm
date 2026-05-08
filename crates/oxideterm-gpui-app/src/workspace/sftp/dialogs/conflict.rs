impl WorkspaceApp {
    fn sftp_conflict_description(&self) -> String {
        self.i18n.t("sftp.conflict.description")
    }

    fn sftp_conflict_remaining_count(&self) -> usize {
        self.sftp_view
            .conflict_state
            .as_ref()
            .map(|state| state.conflicts.len().saturating_sub(state.current_index + 1))
            .unwrap_or(0)
    }

    fn render_sftp_conflict_body(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let Some(state) = self.sftp_view.conflict_state.as_ref() else {
            return div().into_any_element();
        };
        let Some(conflict) = state.conflicts.get(state.current_index) else {
            return div().into_any_element();
        };
        let source_newer = match (conflict.source_modified, conflict.target_modified) {
            (Some(source), Some(target)) => Some(source > target),
            _ => None,
        };
        let source_label_key = match conflict.direction {
            SftpTransferDirection::Upload => "sftp.conflict.local_file",
            SftpTransferDirection::Download => "sftp.conflict.remote_file",
        };
        let target_label_key = match conflict.direction {
            SftpTransferDirection::Upload => "sftp.conflict.remote_file",
            SftpTransferDirection::Download => "sftp.conflict.local_file",
        };
        let show_apply_all = state.conflicts.len() > 1;
        let apply_all = state.apply_to_all;
        div()
            .p(px(16.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .px(px(12.0))
                    .py(px(8.0))
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.bg_panel))
                    .text_size(px(SFTP_TEXT_SM))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(Self::render_lucide_icon(
                                LucideIcon::File,
                                16.0,
                                rgb(theme.text_muted),
                            ))
                            .child(conflict.file_name.clone()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .child(self.render_sftp_file_compare_card(
                                source_label_key,
                                source_newer == Some(true),
                                conflict.source_size,
                                conflict.source_modified,
                                has_background,
                            )),
                    )
                    .child(
                        div()
                            .w(px(32.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(Self::render_lucide_icon(
                                LucideIcon::ArrowRight,
                                20.0,
                                rgb(theme.text_muted),
                            )),
                    )
                    .child(div().flex_1().min_w(px(0.0)).child(
                        self.render_sftp_file_compare_card(
                            target_label_key,
                            source_newer == Some(false),
                            conflict.target_size,
                            conflict.target_modified,
                            has_background,
                        ),
                    )),
            )
            .when(show_apply_all, |body| {
                body.child(
                    div()
                        .pt(px(8.0))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            oxideterm_gpui_ui::checkbox(&self.tokens, String::new(), apply_all)
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.toggle_sftp_conflict_apply_all();
                                        cx.stop_propagation();
                                        cx.notify();
                                    }),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(SFTP_TEXT_SM))
                                .text_color(rgb(theme.text_muted))
                                .cursor_pointer()
                                .child(
                                    self.i18n.t("sftp.conflict.apply_all").replace(
                                        "{{count}}",
                                        &state.conflicts.len().to_string(),
                                    ),
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.toggle_sftp_conflict_apply_all();
                                        cx.stop_propagation();
                                        cx.notify();
                                    }),
                                ),
                        ),
                )
            })
            .into_any_element()
    }

    fn render_sftp_file_compare_card(
        &self,
        label_key: &'static str,
        newer: bool,
        size: u64,
        modified: Option<i64>,
        _has_background: bool,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .p(px(12.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(if newer {
                rgb(0x16a34a)
            } else {
                rgb(theme.border)
            })
            .bg(if newer {
                rgba((0x052e16 << 8) | SFTP_CONFLICT_NEWER_BG_ALPHA)
            } else {
                rgb(theme.bg_panel)
            })
            .child(
                div()
                    .mb(px(8.0))
                    .flex()
                    .items_center()
                    .text_size(px(SFTP_TEXT_XS))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t(label_key).to_uppercase())
                    .when(newer, |label| {
                        label.child(
                            div()
                                .ml(px(8.0))
                                .text_color(rgb(SFTP_GREEN))
                                .child(self.i18n.t("sftp.conflict.newer")),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .text_size(px(SFTP_TEXT_SM))
                    .text_color(rgb(theme.text))
                    .child(Self::render_lucide_icon(
                        LucideIcon::HardDrive,
                        SFTP_ICON_MD,
                        rgb(theme.text_muted),
                    ))
                    .child(format_file_size(size)),
            )
            .child(
                div()
                    .mt(px(6.0))
                    .flex()
                    .gap(px(8.0))
                    .text_size(px(SFTP_TEXT_SM))
                    .text_color(rgb(theme.text))
                    .child(Self::render_lucide_icon(
                        LucideIcon::Clock,
                        SFTP_ICON_MD,
                        rgb(theme.text_muted),
                    ))
                    .child(format_conflict_modified(modified)),
            )
            .into_any_element()
    }
}
