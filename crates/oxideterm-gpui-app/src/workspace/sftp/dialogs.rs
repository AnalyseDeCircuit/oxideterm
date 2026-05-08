impl WorkspaceApp {
    fn render_sftp_dialog(
        &self,
        dialog: SftpDialog,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let SftpDialog::EditorCloseConfirm { name } = dialog.clone() {
            return self.render_sftp_editor_close_confirm_dialog(name, cx);
        }

        let theme = self.tokens.ui;
        let (title, description, body, primary) = match dialog.clone() {
            SftpDialog::Drives => (
                self.i18n.t("sftp.dialogs.select_drive"),
                self.i18n.t("sftp.dialogs.select_drive_desc"),
                self.render_sftp_drives_dialog_body(has_background, cx),
                None,
            ),
            SftpDialog::Rename { .. } => (
                self.i18n.t("sftp.dialogs.rename"),
                self.i18n.t("sftp.dialogs.rename_desc"),
                self.render_sftp_dialog_input("sftp.dialogs.rename_desc", cx),
                Some(self.i18n.t("sftp.dialogs.rename")),
            ),
            SftpDialog::NewFolder { .. } => (
                self.i18n.t("sftp.dialogs.new_folder"),
                self.i18n.t("sftp.dialogs.new_folder_desc"),
                self.render_sftp_dialog_input("sftp.dialogs.new_folder_placeholder", cx),
                Some(self.i18n.t("sftp.dialogs.create")),
            ),
            SftpDialog::Delete { files, .. } => (
                self.i18n.t("sftp.dialogs.delete"),
                self.i18n
                    .t("sftp.dialogs.delete_confirm")
                    .replace("{{count}}", &files.len().to_string()),
                self.render_sftp_delete_dialog_body(files, has_background),
                Some(self.i18n.t("sftp.dialogs.delete")),
            ),
            SftpDialog::Conflict => (
                self.i18n.t("sftp.conflict.title"),
                self.sftp_conflict_description(),
                self.render_sftp_conflict_body(has_background, cx),
                Some(self.i18n.t("sftp.conflict.overwrite")),
            ),
            SftpDialog::Diff {
                local_path,
                local_content,
                remote_path,
                remote_content,
            } => (
                self.i18n.t("sftp.diff.title"),
                self.i18n.t("sftp.diff.description"),
                self.render_sftp_diff_body(
                    &local_path,
                    &local_content,
                    &remote_path,
                    &remote_content,
                    has_background,
                ),
                Some(self.i18n.t("sftp.diff.close")),
            ),
            SftpDialog::Preview { name } => (
                name,
                self.i18n.t("sftp.preview.description"),
                self.render_sftp_preview_body(has_background, cx),
                Some(self.i18n.t("sftp.preview.close")),
            ),
            SftpDialog::Editor { name } => (
                name,
                self.i18n.t("sftp.preview.editor_description"),
                self.render_sftp_editor_body(has_background, cx),
                None,
            ),
            SftpDialog::EditorCloseConfirm { .. } => unreachable!(),
        };
        let width = match dialog {
            SftpDialog::Drives => SFTP_DIALOG_WIDTH_XS,
            SftpDialog::Rename { .. } | SftpDialog::NewFolder { .. } | SftpDialog::Delete { .. } => {
                SFTP_DIALOG_WIDTH_SM
            }
            SftpDialog::Conflict => SFTP_DIALOG_WIDTH_LG,
            SftpDialog::Diff { .. } => SFTP_DIALOG_WIDTH_5XL,
            SftpDialog::Preview { .. } => SFTP_DIALOG_WIDTH_4XL,
            SftpDialog::Editor { .. } => SFTP_EDITOR_DIALOG_WIDTH_6XL,
            SftpDialog::EditorCloseConfirm { .. } => unreachable!(),
        };

        div()
            .absolute()
            .top_0()
            .right_0()
            .bottom_0()
            .left_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(SFTP_DIALOG_OVERLAY_ALPHA))
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .w(px(width))
                    .max_w(relative(0.9))
                    .max_h(relative(0.9))
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(theme.border))
                    // Tauri DialogContent stays opaque; only the overlay is translucent.
                    .bg(rgb(theme.bg_elevated))
                    .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                    .shadow(vec![gpui::BoxShadow {
                        color: gpui::Hsla::from(rgba(SFTP_DIALOG_SHADOW_ALPHA)),
                        offset: gpui::point(px(0.0), px(16.0)),
                        blur_radius: px(32.0),
                        spread_radius: px(0.0),
                    }])
                    .child(
                        div()
                            .px(px(16.0))
                            .py(px(12.0))
                            .border_b_1()
                            .border_color(rgb(theme.border))
                            // Mirrors DialogHeader bg-theme-bg-panel, not the tab background alpha path.
                            .bg(rgb(theme.bg_panel))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .text_size(px(SFTP_TEXT_SM))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(theme.text_heading))
                                    .when(matches!(dialog, SftpDialog::Conflict), |row| {
                                        row.child(Self::render_lucide_icon(
                                            LucideIcon::AlertTriangle,
                                            20.0,
                                            rgb(SFTP_YELLOW),
                                        ))
                                    })
                                    .when(matches!(dialog, SftpDialog::Diff { .. }), |row| {
                                        row.child(Self::render_lucide_icon(
                                            LucideIcon::ArrowLeftRight,
                                            16.0,
                                            rgb(theme.accent),
                                        ))
                                    })
                                    .child(title),
                            )
                            .when(!description.is_empty(), |header| {
                                header.child(
                                div()
                                    .mt(px(6.0))
                                    .text_size(px(SFTP_TEXT_SM))
                                    .text_color(rgb(theme.text_muted))
                                    .child(description),
                                )
                            }),
                    )
                    .child(body)
                    .child(self.render_sftp_dialog_footer(
                        dialog.clone(),
                        primary,
                        has_background,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn render_sftp_dialog_footer(
        &self,
        dialog: SftpDialog,
        primary: Option<String>,
        _has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let footer = div()
            .px(px(16.0))
            .py(px(12.0))
            .border_t_1()
            .border_color(rgb(theme.border))
            // Mirrors DialogFooter bg-theme-bg-panel, not the tab background alpha path.
            .bg(rgb(theme.bg_panel))
            .flex()
            .flex_row()
            .flex_wrap()
            .justify_end()
            .gap(px(8.0));

        if let SftpDialog::Preview { name } = dialog.clone() {
            let path = self.sftp_view.preview_path.clone().unwrap_or_default();
            let can_compare = self.can_compare_sftp_preview(&name);
            let can_edit = self.can_edit_sftp_preview();
            let is_markdown = self.sftp_preview_is_markdown_content();
            let can_download = self.sftp_view.preview_pane == Some(SftpPane::Remote)
                && self.sftp_view.preview_path.is_some();
            return footer
                .justify_between()
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .px(px(8.0))
                        .truncate()
                        .text_size(px(SFTP_TEXT_XS))
                        .text_color(rgb(theme.text_muted))
                        .child(path),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(8.0))
                        .when(is_markdown, |actions| {
                            let label = if self.sftp_view.preview_markdown_source_mode {
                                self.i18n.t("sftp.preview.rendered")
                            } else {
                                self.i18n.t("sftp.preview.source")
                            };
                            actions.child(self.render_sftp_text_button(
                                label,
                                false,
                                cx.listener(|this, _event, _window, cx| {
                                    this.sftp_view.preview_markdown_source_mode =
                                        !this.sftp_view.preview_markdown_source_mode;
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ))
                        })
                        .when(can_edit, |actions| {
                            let name = name.clone();
                            actions.child(self.render_sftp_text_button(
                                self.i18n.t("sftp.preview.edit"),
                                true,
                                cx.listener(move |this, _event, window, cx| {
                                    this.open_sftp_preview_editor(&name, window, cx);
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ))
                        })
                        .when(can_compare, |actions| {
                            let name = name.clone();
                            actions.child(self.render_sftp_text_button(
                                self.i18n.t("sftp.preview.compare"),
                                false,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.open_sftp_preview_compare(&name);
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ))
                        })
                        .when(can_download, |actions| {
                            let name = name.clone();
                            actions.child(self.render_sftp_text_button(
                                self.i18n.t("sftp.preview.download"),
                                false,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.download_sftp_preview(&name);
                                    this.close_sftp_dialog();
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ))
                        })
                        .child(self.render_sftp_text_button(
                            self.i18n.t("sftp.preview.close"),
                            false,
                            cx.listener(|this, _event, _window, cx| {
                                this.close_sftp_dialog();
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )),
                )
                .into_any_element();
        }

        if let SftpDialog::Editor { .. } = dialog.clone() {
            let path = self.sftp_view.preview_path.clone().unwrap_or_default();
            let saving = self.sftp_view.preview_editor_saving;
            let dirty = self.sftp_view.preview_editor_dirty;
            let save_label = if saving {
                self.i18n.t("sftp.preview.saving")
            } else {
                self.i18n.t("sftp.preview.save")
            };
            return footer
                .justify_between()
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .px(px(8.0))
                        .truncate()
                        .text_size(px(SFTP_TEXT_XS))
                        .text_color(rgb(theme.text_muted))
                        .child(path),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(8.0))
                        .child(self.render_sftp_text_button(
                            save_label,
                            true,
                            cx.listener(move |this, _event, _window, cx| {
                                if !saving && dirty {
                                    this.save_sftp_preview_editor(cx);
                                }
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ))
                        .child(self.render_sftp_text_button(
                            self.i18n.t("sftp.preview.close"),
                            false,
                            cx.listener(|this, _event, _window, cx| {
                                this.request_close_sftp_editor();
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )),
                )
                .into_any_element();
        }

        if let SftpDialog::EditorCloseConfirm { name } = dialog.clone() {
            return footer
                .child(self.render_sftp_text_button(
                    self.i18n.t("sftp.dialogs.cancel"),
                    false,
                    cx.listener(move |this, _event, _window, cx| {
                        this.cancel_sftp_editor_close_confirm(name.clone());
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ))
                .child(self.render_sftp_text_button(
                    self.i18n.t("sftp.preview.discard"),
                    true,
                    cx.listener(|this, _event, _window, cx| {
                        this.discard_sftp_editor_changes();
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ))
                .into_any_element();
        }

        if let SftpDialog::Diff {
            local_content,
            remote_content,
            ..
        } = dialog.clone()
        {
            let stats = sftp_diff_stats(&compute_sftp_diff(&local_content, &remote_content));
            return footer
                .justify_between()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .flex_1()
                        .min_w(px(0.0))
                        .text_size(px(SFTP_TEXT_XS))
                        .text_color(rgb(theme.text_muted))
                        .child(
                            self.i18n
                                .t("sftp.diff.unchanged")
                                .replace("{{count}}", &stats.unchanged.to_string()),
                        )
                        .child(", ")
                        .child(
                            div().text_color(rgb(SFTP_GREEN)).child(
                                self.i18n
                                    .t("sftp.diff.added")
                                    .replace("{{count}}", &stats.added.to_string()),
                            ),
                        )
                        .child(", ")
                        .child(
                            div().text_color(rgb(SFTP_RED)).child(
                                self.i18n
                                    .t("sftp.diff.removed")
                                    .replace("{{count}}", &stats.removed.to_string()),
                            ),
                        ),
                )
                .child(self.render_sftp_text_button(
                    self.i18n.t("sftp.diff.close"),
                    false,
                    cx.listener(|this, _event, _window, cx| {
                        this.close_sftp_dialog();
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ))
                .into_any_element();
        }

        if matches!(dialog, SftpDialog::Conflict) {
            let source_newer = self
                .sftp_view
                .conflict_state
                .as_ref()
                .and_then(|state| state.conflicts.get(state.current_index))
                .and_then(|conflict| {
                    Some(conflict.source_modified? > conflict.target_modified?)
                });
            return footer
                .justify_between()
                .child(
                    div()
                        .flex()
                        .gap(px(8.0))
                        .child(self.render_sftp_text_button(
                            self.i18n.t("sftp.conflict.skip"),
                            false,
                            cx.listener(|this, _event, _window, cx| {
                                this.resolve_sftp_transfer_conflict(SftpConflictResolution::Skip);
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ))
                        .when(source_newer.is_some(), |actions| {
                            actions.child(self.render_sftp_text_button(
                                self.i18n.t("sftp.conflict.skip_older"),
                                false,
                                cx.listener(|this, _event, _window, cx| {
                                    this.resolve_sftp_transfer_conflict(
                                        SftpConflictResolution::SkipOlder,
                                    );
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ))
                        }),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(8.0))
                        .child(self.render_sftp_text_button(
                            self.i18n.t("sftp.conflict.keep_both"),
                            false,
                            cx.listener(|this, _event, _window, cx| {
                                this.resolve_sftp_transfer_conflict(SftpConflictResolution::Rename);
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ))
                        .child(self.render_sftp_text_button(
                            self.i18n.t("sftp.conflict.overwrite"),
                            true,
                            cx.listener(|this, _event, _window, cx| {
                                this.resolve_sftp_transfer_conflict(
                                    SftpConflictResolution::Overwrite,
                                );
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )),
                )
                .into_any_element();
        }

        footer
            .child(self.render_sftp_text_button(
                self.i18n.t("sftp.dialogs.cancel"),
                false,
                cx.listener(|this, _event, _window, cx| {
                    this.close_sftp_dialog();
                    cx.stop_propagation();
                    cx.notify();
                }),
            ))
            .when_some(primary, |footer, label| {
                footer.child(self.render_sftp_text_button(
                    label,
                    true,
                    cx.listener(|this, _event, _window, cx| {
                        this.accept_sftp_dialog();
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ))
            })
            .into_any_element()
    }

    fn render_sftp_editor_close_confirm_dialog(
        &self,
        name: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .absolute()
            .top_0()
            .right_0()
            .bottom_0()
            .left_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(SFTP_DIALOG_OVERLAY_ALPHA))
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .w(px(SFTP_DIALOG_WIDTH_SM))
                    .max_w(relative(0.9))
                    .overflow_hidden()
                    .rounded(px(self.tokens.radii.lg))
                    .border_1()
                    .border_color(rgba((theme.border << 8) | SFTP_DIALOG_BORDER_SUBTLE_ALPHA))
                    .bg(rgb(theme.bg_elevated))
                    .shadow(vec![gpui::BoxShadow {
                        color: gpui::Hsla::from(rgba(SFTP_DIALOG_SHADOW_ALPHA)),
                        offset: gpui::point(px(0.0), px(16.0)),
                        blur_radius: px(32.0),
                        spread_radius: px(0.0),
                    }])
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
                                    .w(px(48.0))
                                    .h(px(48.0))
                                    .rounded_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .border_1()
                                    .border_color(rgba((theme.accent << 8) | SFTP_CONFIRM_ICON_RING_ALPHA))
                                    .bg(rgba((theme.accent << 8) | SFTP_CONFIRM_ICON_BG_ALPHA))
                                    .child(Self::render_lucide_icon(
                                        LucideIcon::HelpCircle,
                                        24.0,
                                        rgb(theme.accent),
                                    )),
                            )
                            .child(
                                div()
                                    .text_size(px(SFTP_TEXT_SM))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(theme.text))
                                    .text_center()
                                    .line_height(px(20.0))
                                    .child(self.i18n.t("sftp.preview.unsaved_changes_confirm")),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .border_t_1()
                            .border_color(rgba((theme.border << 8) | SFTP_DIALOG_DIVIDER_ALPHA))
                            .child(
                                div()
                                    .flex_1()
                                    .py(px(10.0))
                                    .border_r_1()
                                    .border_color(rgba((theme.border << 8) | SFTP_DIALOG_DIVIDER_ALPHA))
                                    .text_center()
                                    .text_size(px(SFTP_TEXT_SM))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text_muted))
                                    .hover(move |button| {
                                        button
                                            .bg(rgb(theme.bg_hover))
                                            .text_color(rgb(theme.text))
                                    })
                                    .cursor_pointer()
                                    .child(self.i18n.t("sftp.dialogs.cancel"))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |this, _event, _window, cx| {
                                            this.cancel_sftp_editor_close_confirm(name.clone());
                                            cx.stop_propagation();
                                            cx.notify();
                                        }),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .py(px(10.0))
                                    .text_center()
                                    .text_size(px(SFTP_TEXT_SM))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(theme.accent))
                                    .hover(move |button| {
                                        button.bg(rgba(
                                            (theme.accent << 8) | SFTP_CONFIRM_ACTION_HOVER_ALPHA,
                                        ))
                                    })
                                    .cursor_pointer()
                                    .child(self.i18n.t("sftp.preview.confirm"))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _event, _window, cx| {
                                            this.discard_sftp_editor_changes();
                                            cx.stop_propagation();
                                            cx.notify();
                                        }),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_sftp_drives_dialog_body(
        &self,
        _has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .px(px(16.0))
            .py(px(12.0))
            .child(
                div()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .overflow_hidden()
                    .children(mock_drives().into_iter().map(|drive| {
                        let path = drive.path.clone();
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .gap(px(12.0))
                            .px(px(12.0))
                            .py(px(10.0))
                            .border_b_1()
                            .border_color(rgba((theme.border << 8) | SFTP_DIALOG_BORDER_HALF_ALPHA))
                            .bg(rgb(theme.bg_panel))
                            .hover(move |row| row.bg(rgb(theme.bg_hover)))
                            .cursor_pointer()
                            .child(Self::render_lucide_icon(
                                if drive.drive_type == "network" {
                                    LucideIcon::Network
                                } else {
                                    LucideIcon::HardDrive
                                },
                                16.0,
                                rgb(theme.text_muted),
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(6.0))
                                            .text_size(px(SFTP_TEXT_SM))
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(rgb(theme.text))
                                            .child(drive.name)
                                            .when(drive.read_only, |row| {
                                                row.child(
                                                    div()
                                                        .rounded(px(self.tokens.radii.xs))
                                                        .px(px(4.0))
                                                        .py(px(2.0))
                                                        .text_size(px(SFTP_TEXT_10))
                                                        .bg(rgba((SFTP_YELLOW << 8) | SFTP_READONLY_BADGE_BG_ALPHA))
                                                        .text_color(rgb(SFTP_YELLOW))
                                                        .child(
                                                            self.i18n.t("sftp.dialogs.readOnly"),
                                                        ),
                                                )
                                            }),
                                    )
                                    .child(
                                        div()
                                            .mt(px(2.0))
                                            .text_size(px(SFTP_TEXT_XS))
                                            .text_color(rgb(theme.text_muted))
                                            .child(path.clone()),
                                    )
                                    .child(
                                        div()
                                            .mt(px(2.0))
                                            .text_size(px(SFTP_TEXT_10))
                                            .text_color(rgb(theme.text_muted))
                                            .child(format!(
                                                "{} {} / {}",
                                                format_file_size(drive.available_space),
                                                self.i18n.t("sftp.dialogs.available"),
                                                format_file_size(drive.total_space),
                                            )),
                                    ),
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.sftp_view.local_path = path.clone();
                                    this.sftp_view.local_path_input = path.clone();
                                    this.close_sftp_dialog();
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            )
                    })),
            )
            .into_any_element()
    }

    fn render_sftp_delete_dialog_body(
        &self,
        files: Vec<String>,
        _has_background: bool,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .px(px(16.0))
            .py(px(12.0))
            .child(
                div()
                    .id("sftp-drives-scroll")
                    .max_h(px(128.0))
                    .overflow_y_scroll()
                    .rounded(px(self.tokens.radii.sm))
                    .bg(rgb(theme.bg_sunken))
                    .p(px(8.0))
                    .text_size(px(SFTP_TEXT_XS))
                    .text_color(rgb(theme.text_muted))
                    .children(files.into_iter().map(|file| div().child(file))),
            )
            .into_any_element()
    }

    fn render_sftp_dialog_input(
        &self,
        placeholder_key: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let focused = self.sftp_view.focused_input == Some(SftpInput::DialogValue);
        div()
            .px(px(16.0))
            .py(px(12.0))
            .child(
                div()
                    .h(px(36.0))
                    .w_full()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(if focused {
                        rgb(theme.accent)
                    } else {
                        rgb(theme.border)
                    })
                    .bg(rgb(theme.bg))
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .child(self.render_sftp_inline_text(
                        SftpInput::DialogValue,
                        &self.sftp_view.dialog_value,
                        placeholder_key,
                        focused,
                        cx,
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.sftp_view.focused_input = Some(SftpInput::DialogValue);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            )
            .into_any_element()
    }

    fn sftp_conflict_description(&self) -> String {
        let mut description = self.i18n.t("sftp.conflict.description");
        if let Some(state) = self.sftp_view.conflict_state.as_ref() {
            let remaining = state
                .conflicts
                .len()
                .saturating_sub(state.current_index + 1);
            if remaining > 0 {
                description.push(' ');
                description.push_str(
                    &self
                        .i18n
                        .t("sftp.conflict.remaining")
                        .replace("{{count}}", &remaining.to_string()),
                );
            }
        }
        description
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

    fn render_sftp_diff_body(
        &self,
        local_path: &str,
        local_content: &str,
        remote_path: &str,
        remote_content: &str,
        _has_background: bool,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let lines = compute_sftp_diff(local_content, remote_content);
        let stats = sftp_diff_stats(&lines);
        let line_count = lines.len();
        let diff_lines = std::sync::Arc::new(lines);
        let diff_scroll = self.sftp_view.diff_scroll.clone();
        div()
            .w_full()
            .h(px(480.0))
            .flex()
            .flex_col()
            .bg(rgb(theme.bg_sunken))
            .child(
                div()
                    .w_full()
                    .flex()
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .text_size(px(SFTP_TEXT_XS))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .px(px(12.0))
                            .py(px(8.0))
                            .bg(rgba((0x7f1d1d << 8) | SFTP_DIFF_HEADER_BG_ALPHA))
                            .child(Self::render_lucide_icon(
                                LucideIcon::File,
                                SFTP_ICON_SM,
                                rgb(SFTP_RED),
                            ))
                            .child(
                                div()
                                    .text_color(rgb(0xfca5a5))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .child(format!("{}:", self.i18n.t("sftp.diff.local"))),
                            )
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .text_color(rgb(theme.text_muted))
                                    .child(sftp_file_name(local_path)),
                            )
                            .child(
                                div()
                                    .ml_auto()
                                    .text_color(rgb(SFTP_RED))
                                    .child(format!("-{}", stats.removed)),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .px(px(12.0))
                            .py(px(8.0))
                            .bg(rgba((0x14532d << 8) | SFTP_DIFF_HEADER_BG_ALPHA))
                            .child(Self::render_lucide_icon(
                                LucideIcon::File,
                                SFTP_ICON_SM,
                                rgb(SFTP_GREEN),
                            ))
                            .child(
                                div()
                                    .text_color(rgb(0x86efac))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .child(format!("{}:", self.i18n.t("sftp.diff.remote"))),
                            )
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .text_color(rgb(theme.text_muted))
                                    .child(sftp_file_name(remote_path)),
                            )
                            .child(
                                div()
                                    .ml_auto()
                                    .text_color(rgb(SFTP_GREEN))
                                    .child(format!("+{}", stats.added)),
                            ),
                    ),
            )
            .child(
                div()
                    .id("sftp-diff-scroll")
                    .w_full()
                    .flex_1()
                    .overflow_y_scroll()
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .text_size(px(SFTP_TEXT_XS))
                    .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                    .when(line_count == 0, |body| {
                        body.child(
                            div()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_color(rgb(theme.text_muted))
                                .child(self.i18n.t("sftp.diff.identical")),
                        )
                    })
                    .when(line_count > 0, |body| {
                        let diff_lines = diff_lines.clone();
                        body.child(
                            uniform_list(
                                "sftp-diff-virtual-list",
                                line_count,
                                move |range, _window, _cx| {
                                    range
                                        .map(|index| {
                                            let line = diff_lines[index].clone();
                                            let removed =
                                                line.kind == SftpDiffLineKind::Removed;
                                            let added = line.kind == SftpDiffLineKind::Added;
                                            let left_num = line
                                                .left_line_num
                                                .map(|number| number.to_string())
                                                .unwrap_or_default();
                                            let right_num = line
                                                .right_line_num
                                                .map(|number| number.to_string())
                                                .unwrap_or_default();
                                            let left_content = if added {
                                                String::new()
                                            } else if removed {
                                                format!("- {}", line.content)
                                            } else {
                                                line.content.clone()
                                            };
                                            let right_content = if removed {
                                                String::new()
                                            } else if added {
                                                format!("+ {}", line.content)
                                            } else {
                                                line.content
                                            };
                                            div()
                                                .w_full()
                                                .h(px(SFTP_DIFF_ROW_HEIGHT))
                                                .flex()
                                                .border_b_1()
                                                .border_color(rgba((theme.border << 8) | SFTP_DIALOG_BORDER_HALF_ALPHA))
                                                .child(diff_cell(
                                                    &left_num,
                                                    &left_content,
                                                    removed,
                                                    theme.border,
                                                    true,
                                                ))
                                                .child(diff_cell(
                                                    &right_num,
                                                    &right_content,
                                                    added,
                                                    theme.border,
                                                    false,
                                                ))
                                                .into_any_element()
                                        })
                                        .collect::<Vec<_>>()
                                },
                            )
                            .track_scroll(diff_scroll)
                            .size_full()
                            .on_scroll_wheel(|_, _, cx| cx.stop_propagation()),
                        )
                    }),
            )
            .into_any_element()
    }

    fn render_sftp_preview_body(&self, _has_background: bool, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let body = if self.sftp_view.preview_loading {
            self.render_sftp_preview_text(self.i18n.t("sftp.preview.loading"))
        } else if let Some(error) = &self.sftp_view.preview_error {
            self.render_sftp_preview_text(error.clone())
        } else if let Some(content) = &self.sftp_view.preview_content {
            self.render_sftp_preview_content(content, cx)
        } else {
            self.render_sftp_preview_text(String::new())
        };
        let uses_virtual_text = self.sftp_preview_uses_virtual_text();
        div()
            .h(px(520.0))
            .flex()
            .flex_col()
            .bg(rgb(theme.bg_sunken))
            .child(
                div()
                    .id("sftp-preview-scroll")
                    .flex_1()
                    .when(!uses_virtual_text, |scroll| {
                        scroll.overflow_y_scroll().p(px(16.0))
                    })
                    .text_color(rgb(theme.text))
                    .child(body),
            )
            .into_any_element()
    }

    fn render_sftp_editor_body(&self, _has_background: bool, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let language = self
            .sftp_view
            .preview_editor_language
            .clone()
            .unwrap_or_else(|| "text".to_string());
        let encoding = self.sftp_view.preview_editor_encoding.clone();
        let (line, column) = self
            .sftp_view
            .preview_editor_input
            .as_ref()
            .map(|input| {
                let pos = input.read(cx).cursor_position();
                (pos.line + 1, pos.character + 1)
            })
            .unwrap_or((1, 1));
        let status = if self.sftp_view.preview_editor_saving {
            Some((self.i18n.t("sftp.preview.saving"), rgb(theme.text_muted)))
        } else if self.sftp_view.preview_editor_dirty {
            Some((self.i18n.t("sftp.preview.modified"), rgb(SFTP_YELLOW)))
        } else if let Some(atomic) = self.sftp_view.preview_editor_last_atomic_write {
            let key = if atomic {
                "sftp.preview.saved_atomic"
            } else {
                "sftp.preview.saved_direct"
            };
            Some((self.i18n.t(key), rgb(SFTP_GREEN)))
        } else {
            None
        };

        div()
            .h(px(600.0))
            .flex()
            .flex_col()
            .bg(rgb(theme.bg_sunken))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_hidden()
                    .when_some(self.sftp_view.preview_editor_input.clone(), |body, input| {
                        body.child(
                            CodeEditorInput::new(&input)
                                .appearance(false)
                                .h_full()
                                .font_family(settings_mono_font_family(
                                    self.settings_store.settings(),
                                ))
                                .text_size(px(SFTP_TEXT_SM))
                                .text_color(rgb(theme.text))
                                .bg(rgb(theme.bg_sunken)),
                        )
                    })
                    .when(self.sftp_view.preview_editor_input.is_none(), |body| {
                        body.child(self.render_sftp_preview_text(String::new()))
                    }),
            )
            .child(
                div()
                    .h(px(32.0))
                    .flex_none()
                    .px(px(16.0))
                    .border_t_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.bg_panel))
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(px(SFTP_TEXT_XS))
                    .text_color(rgb(theme.text_muted))
                    .child(
                        div()
                            .flex()
                            .gap(px(16.0))
                            .child(format!(
                                "{} {}, {} {}",
                                self.i18n.t("sftp.preview.line"),
                                line,
                                self.i18n.t("sftp.preview.column"),
                                column
                            ))
                            .child(language)
                            .child(format!(
                                "{} {}",
                                self.i18n.t("sftp.preview.encoding"),
                                encoding
                            )),
                    )
                .child(self.render_sftp_editor_status(status, cx)),
            )
            .into_any_element()
    }

    fn render_sftp_editor_status(
        &self,
        status: Option<(String, gpui::Rgba)>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(error) = &self.sftp_view.preview_editor_save_error {
            let message = error.clone();
            if self.sftp_view.preview_editor_network_error {
                let retry_count = self.sftp_view.preview_editor_retry_count;
                let label = if retry_count > 0 {
                    format!("{} ({retry_count})", self.i18n.t("sftp.preview.retry"))
                } else {
                    self.i18n.t("sftp.preview.retry")
                };
                return div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .text_color(rgb(SFTP_ORANGE))
                            .child(Self::render_lucide_icon(
                                LucideIcon::WifiOff,
                                SFTP_ICON_MD,
                                rgb(SFTP_ORANGE),
                            ))
                            .child(div().max_w(px(320.0)).truncate().child(message)),
                    )
                    .child(
                        div()
                            .h(px(20.0))
                            .px(px(8.0))
                            .rounded(px(self.tokens.radii.sm))
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .text_size(px(SFTP_TEXT_XS))
                            .text_color(rgb(SFTP_ORANGE))
                            .hover(|style| {
                                style.bg(rgba(
                                    (SFTP_ORANGE << 8) | SFTP_EDITOR_RETRY_HOVER_ALPHA,
                                ))
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.retry_sftp_preview_editor_save(cx);
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            )
                            .child(Self::render_lucide_icon(
                                LucideIcon::RefreshCcw,
                                SFTP_ICON_SM,
                                rgb(SFTP_ORANGE),
                            ))
                            .child(label),
                    )
                    .into_any_element();
            }
            return div()
                .max_w(px(360.0))
                .truncate()
                .text_color(rgb(SFTP_RED))
                .child(message)
                .into_any_element();
        }

        if let Some((message, color)) = status {
            div()
                .max_w(px(360.0))
                .truncate()
                .text_color(color)
                .child(message)
                .into_any_element()
        } else {
            div().into_any_element()
        }
    }

    fn render_sftp_preview_text(&self, text: String) -> AnyElement {
        div()
            .font_family(settings_mono_font_family(self.settings_store.settings()))
            .text_size(px(SFTP_TEXT_XS))
            .child(text)
            .into_any_element()
    }

    fn render_sftp_preview_content(
        &self,
        content: &PreviewContent,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match content {
            PreviewContent::Text {
                data,
                mime_type,
                language,
                ..
            } if sftp_preview_is_markdown(language.as_deref(), mime_type.as_deref()) => {
                if self.sftp_view.preview_markdown_source_mode {
                    self.render_sftp_preview_code(data, Some("markdown"))
                } else {
                    self.render_sftp_preview_markdown(data, cx)
                }
            }
            PreviewContent::Text { data, language, .. } => {
                self.render_sftp_preview_code(data, language.as_deref())
            }
            PreviewContent::Image { mime_type, data } => {
                let source = format!("data:{mime_type};base64,{data}");
                self.render_sftp_preview_image(source, mime_type.clone())
            }
            PreviewContent::AssetFile {
                path,
                mime_type,
                kind: AssetFileKind::Image,
            } => self.render_sftp_preview_image(std::path::PathBuf::from(path), mime_type.clone()),
            PreviewContent::AssetFile {
                path,
                mime_type,
                kind: AssetFileKind::Pdf,
            } => self.render_sftp_preview_pdf(path, mime_type),
            PreviewContent::AssetFile {
                path,
                mime_type,
                kind: AssetFileKind::Audio,
            } => self.render_sftp_preview_audio(path, mime_type, cx),
            PreviewContent::AssetFile {
                path,
                mime_type,
                kind: AssetFileKind::Video,
            } => self.render_sftp_preview_video(path, mime_type, cx),
            PreviewContent::AssetFile {
                path,
                mime_type,
                kind: AssetFileKind::Office,
            } => self.render_sftp_preview_office(path, mime_type, cx),
            PreviewContent::AssetFile {
                path,
                mime_type,
                kind: AssetFileKind::Font,
            } => self.render_sftp_preview_font(path, mime_type, cx),
            PreviewContent::Hex {
                data,
                total_size,
                offset,
                chunk_size,
                has_more,
            } => self.render_sftp_preview_hex(
                data,
                *total_size,
                *offset,
                *chunk_size,
                *has_more,
                cx,
            ),
            PreviewContent::TooLarge { .. } | PreviewContent::Unsupported { .. } => {
                self.render_sftp_preview_text(preview_content_text(content))
            }
        }
    }

    fn render_sftp_preview_hex(
        &self,
        data: &str,
        total_size: u64,
        offset: u64,
        chunk_size: u64,
        has_more: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let showing = offset.saturating_add(chunk_size).min(total_size);
        let loading_more = self.sftp_view.preview_hex_loading_more;
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .mb(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(SFTP_TEXT_XS))
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t("sftp.preview.hex_view"))
                    .child("•")
                    .child(
                        self.i18n
                            .t("sftp.preview.showing_first")
                            .replace("{{size}}", &format_file_size(showing)),
                    )
                    .when(total_size > 0, |header| {
                        header.child("•").child(
                            self.i18n
                                .t("sftp.preview.total_size")
                                .replace("{{size}}", &format_file_size(total_size)),
                        )
                    }),
            )
            .child(
                div()
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .text_size(px(SFTP_TEXT_XS))
                    .line_height(px(20.0))
                    .text_color(rgb(theme.text))
                    .child(data.to_string()),
            )
            .when(has_more, |body| {
                let label = if loading_more {
                    self.i18n.t("sftp.preview.loading")
                } else {
                    self.i18n.t("sftp.preview.load_more")
                };
                body.child(
                    div().mt(px(16.0)).flex().justify_center().child(
                        self.render_sftp_text_button(
                            label,
                            false,
                            cx.listener(move |this, _event, _window, cx| {
                                if !loading_more {
                                    this.load_more_sftp_preview_hex();
                                }
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ),
                    ),
                )
            })
            .into_any_element()
    }

    fn render_sftp_preview_markdown(&self, source: &str, cx: &mut Context<Self>) -> AnyElement {
        let opts = MarkdownOptions::from_theme(&self.tokens);
        div()
            .size_full()
            .p(px(16.0))
            .child(markdown_virtual_with_options(
                cx.entity(),
                "sftp-preview-markdown-virtual",
                &self.tokens,
                source,
                &opts,
                &self.sftp_view.preview_markdown_scroll,
            ))
            .into_any_element()
    }

    fn render_sftp_preview_font(
        &self,
        path: &str,
        mime_type: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        if let Some(error) = self.sftp_view.preview_font_error.as_ref() {
            return self
                .render_sftp_native_asset_status("Font", path, mime_type, error)
                .into_any_element();
        }
        let Some(font_family) = self.sftp_view.preview_font_family.clone() else {
            return self.render_sftp_preview_text(self.i18n.t("sftp.preview.loading"));
        };
        let font_size = self.sftp_view.preview_font_size;
        let sample_font = SharedString::from(font_family.clone());
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(16.0))
                    .py(px(12.0))
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .bg(rgba((theme.bg_panel << 8) | SFTP_PANEL_80_ALPHA))
                    .child(self.render_sftp_font_size_button(
                        "-",
                        false,
                        cx.listener(|this, _event, _window, cx| {
                            this.sftp_view.preview_font_size =
                                (this.sftp_view.preview_font_size - 4.0).max(8.0);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ))
                    .child(
                        div()
                            .w(px(52.0))
                            .text_center()
                            .text_size(px(SFTP_TEXT_XS))
                            .text_color(rgb(theme.text_muted))
                            .child(format!("{font_size:.0}px")),
                    )
                    .child(self.render_sftp_font_size_button(
                        "+",
                        false,
                        cx.listener(|this, _event, _window, cx| {
                            this.sftp_view.preview_font_size =
                                (this.sftp_view.preview_font_size + 4.0).min(120.0);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ))
                    .children([16.0, 24.0, 32.0, 48.0, 72.0].into_iter().map(|size| {
                        self.render_sftp_font_size_button(
                            format!("{size:.0}"),
                            (font_size - size).abs() < f32::EPSILON,
                            cx.listener(move |this, _event, _window, cx| {
                                this.sftp_view.preview_font_size = size;
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )
                    }))
                    .child(
                        div()
                            .ml(px(8.0))
                            .min_w(px(0.0))
                            .truncate()
                            .text_size(px(SFTP_TEXT_XS))
                            .text_color(rgb(theme.text_muted))
                            .child(font_family.clone()),
                    ),
            )
            .child(
                div()
                    .id("sftp-font-preview-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .p(px(24.0))
                    .bg(rgb(theme.bg_sunken))
                    .font_family(sample_font.clone())
                    .text_color(rgb(theme.text))
                    .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(32.0))
                            .child(self.render_sftp_font_sample_section(
                                self.i18n.t("sftp.preview.font_alphabet"),
                                "ABCDEFGHIJKLMNOPQRSTUVWXYZ\nabcdefghijklmnopqrstuvwxyz",
                                sample_font.clone(),
                                font_size,
                                1.4,
                            ))
                            .child(self.render_sftp_font_sample_section(
                                self.i18n.t("sftp.preview.font_numbers"),
                                "0123456789\n!@#$%^&*()_+-=[]{}|;:'\",.<>?/\\~`",
                                sample_font.clone(),
                                font_size,
                                1.4,
                            ))
                            .child(self.render_sftp_font_sample_section(
                                self.i18n.t("sftp.preview.font_pangram"),
                                "The quick brown fox jumps over the lazy dog.",
                                sample_font.clone(),
                                font_size,
                                1.4,
                            ))
                            .child(self.render_sftp_font_sample_section(
                                self.i18n.t("sftp.preview.font_cjk"),
                                "天地玄黄，宇宙洪荒。日月盈昃，辰宿列张。\nいろはにほへとちりぬるを\n키스의 고유조건은 입술끼리 만나는 것이다",
                                sample_font.clone(),
                                font_size,
                                1.6,
                            ))
                            .child(self.render_sftp_font_sample_section(
                                self.i18n.t("sftp.preview.font_nerd_icons"),
                                "       󰊤  󰇘  󱁤           ",
                                sample_font.clone(),
                                font_size,
                                1.4,
                            ))
                            .child(self.render_sftp_font_sample_section(
                                self.i18n.t("sftp.preview.font_code"),
                                "fn main() {\n    println!(\"Hello, 世界!\");\n    let x = 42;\n}",
                                sample_font.clone(),
                                (font_size * 0.75).max(12.0),
                                1.6,
                            ))
                            .child(self.render_sftp_font_sample_section(
                                self.i18n.t("sftp.preview.font_ligatures"),
                                "-> => == != <= >= && || :: ++ -- ** // /* */ <!-- -->",
                                sample_font,
                                font_size,
                                1.4,
                            )),
                    ),
            )
            .into_any_element()
    }

    fn render_sftp_font_size_button(
        &self,
        label: impl Into<String>,
        active: bool,
        on_click: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .h(px(28.0))
            .min_w(px(28.0))
            .px(px(8.0))
            .rounded(px(self.tokens.radii.sm))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(SFTP_TEXT_XS))
            .text_color(if active {
                rgb(theme.text)
            } else {
                rgb(theme.text_muted)
            })
            .bg(if active {
                rgb(theme.bg_hover)
            } else {
                rgb(theme.bg_panel)
            })
            .hover(move |button| button.bg(rgb(theme.bg_hover)).text_color(rgb(theme.text)))
            .cursor_pointer()
            .child(label.into())
            .on_mouse_down(MouseButton::Left, on_click)
            .into_any_element()
    }

    fn render_sftp_font_sample_section(
        &self,
        title: String,
        sample: &'static str,
        font_family: SharedString,
        font_size: f32,
        line_height: f32,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .font_family(settings_ui_font_family(
                        self.settings_store.settings().appearance.ui_font_family.as_str(),
                    ))
                    .text_size(px(SFTP_TEXT_XS))
                    .text_color(rgb(theme.text_muted))
                    .child(title),
            )
            .child(
                div()
                    .font_family(font_family)
                    .text_size(px(font_size))
                    .line_height(px(font_size * line_height))
                    .text_color(rgb(theme.text))
                    .child(sample),
            )
            .into_any_element()
    }

    fn sftp_preview_uses_virtual_text(&self) -> bool {
        matches!(
            self.sftp_view.preview_content.as_ref(),
            Some(PreviewContent::Text { .. })
        )
    }

    fn render_sftp_preview_code(&self, source: &str, language: Option<&str>) -> AnyElement {
        let theme = self.tokens.ui;
        let opts = MarkdownOptions::from_theme(&self.tokens);
        let language = language
            .filter(|language| !language.trim().is_empty())
            .unwrap_or("text")
            .to_ascii_lowercase();
        let lines = std::sync::Arc::new(
            source
                .split('\n')
                .map(|line| line.to_string())
                .collect::<Vec<_>>(),
        );
        let row_count = lines.len();
        let list_lines = lines.clone();
        let font_family = settings_mono_font_family(self.settings_store.settings());
        let scroll = self.sftp_view.preview_code_scroll.clone();
        div()
            .size_full()
            .bg(rgb(theme.bg_sunken))
            .child(
                uniform_list("sftp-preview-code-virtual", row_count, move |range, _window, _cx| {
                    let opts = opts.clone();
                    let language = language.clone();
                    let font_family = font_family.clone();
                    range
                        .map(|index| {
                            let line = &list_lines[index];
                            let content: AnyElement =
                                if language != "text"
                                    && let Some(runs) =
                                        highlight::highlight_code(&language, line, &opts)
                                {
                                    let (text, text_runs) =
                                        highlight::highlighted_runs_to_text_runs(&runs);
                                    StyledText::new(text)
                                        .with_runs(text_runs)
                                        .into_any_element()
                                } else {
                                    SharedString::from(line.clone()).into_any_element()
                                };
                            div()
                                .h(px(SFTP_PREVIEW_CODE_LINE_HEIGHT))
                                .w_full()
                                .flex()
                                .flex_row()
                                .items_center()
                                .font_family(font_family.clone())
                                .text_size(px(SFTP_TEXT_XS))
                                .line_height(px(SFTP_PREVIEW_CODE_LINE_HEIGHT))
                                .text_color(rgb(theme.text))
                                .child(
                                    div()
                                        .w(px(SFTP_DIFF_LINE_NUMBER_COL))
                                        .flex_none()
                                        .pr(px(12.0))
                                        .text_align(gpui::TextAlign::Right)
                                        .text_color(rgba(
                                            (theme.text_muted << 8)
                                                | SFTP_PREVIEW_CODE_GUTTER_ALPHA,
                                        ))
                                        .child((index + 1).to_string()),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .overflow_hidden()
                                        .child(content),
                                )
                                .into_any_element()
                        })
                        .collect::<Vec<_>>()
                })
                .track_scroll(scroll)
                .size_full()
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation()),
            )
            .into_any_element()
    }

    fn render_sftp_preview_pdf(&self, path: &str, mime_type: &str) -> AnyElement {
        let backend = PdfiumPreviewBackend;
        let path_buf = std::path::PathBuf::from(path);
        match backend.render_page(&path_buf, 0, 900) {
            Ok(bitmap) => {
                if let Some(image) = bitmap.into_render_image() {
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(
                            gpui::img(image)
                                .w_full()
                                .h(px(456.0))
                                .object_fit(ObjectFit::Contain),
                        )
                        .child(
                            div()
                                .text_size(px(SFTP_TEXT_XS))
                                .text_color(rgb(self.tokens.ui.text_muted))
                                .child(format!("PDF · {mime_type} · page 1")),
                        )
                        .into_any_element()
                } else {
                    self.render_sftp_native_asset_status(
                        "PDF",
                        path,
                        mime_type,
                        "PDFium rendered a page but GPUI could not build a bitmap.",
                    )
                    .into_any_element()
                }
            }
            Err(error) => self.render_sftp_native_asset_status(
                "PDF",
                path,
                mime_type,
                &format!("{error}"),
            )
            .into_any_element(),
        }
    }

    fn render_sftp_preview_audio(
        &self,
        path: &str,
        mime_type: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let snapshot = self.sftp_view.preview_audio.snapshot();
        let name = std::path::Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(path)
            .to_string();
        let duration = snapshot.duration.unwrap_or_default();
        let position = snapshot.position.min(duration);
        let progress = if duration.is_zero() {
            0.0
        } else {
            (position.as_secs_f32() / duration.as_secs_f32()).clamp(0.0, 1.0)
        };
        let play_icon = if snapshot.state == AudioPreviewState::Playing {
            LucideIcon::Pause
        } else {
            LucideIcon::Play
        };
        let can_seek = snapshot.duration.is_some() && snapshot.state != AudioPreviewState::Error;

        div()
            .w_full()
            .min_h(px(456.0))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .p_4()
            .gap(px(16.0))
            .child(
                div()
                    .text_size(px(56.0))
                    .line_height(px(64.0))
                    .text_color(rgb(theme.text_muted))
                    .child("♪"),
            )
            .child(
                div()
                    .max_w(px(448.0))
                    .truncate()
                    .text_size(px(SFTP_TEXT_SM))
                    .text_color(rgb(theme.text_muted))
                    .child(name),
            )
            .child(
                div()
                    .w_full()
                    .max_w(px(448.0))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.bg_panel))
                    .px_3()
                    .py_2()
                    .child(
                        div()
                            .w(px(32.0))
                            .h(px(32.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(self.tokens.radii.md))
                            .border_1()
                            .border_color(rgb(theme.border))
                            .bg(rgb(theme.bg))
                            .text_color(rgb(theme.text))
                            .when(snapshot.state != AudioPreviewState::Error, |button| {
                                button.cursor_pointer().hover(move |button| {
                                    button.bg(rgb(theme.bg_hover))
                                })
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.toggle_sftp_preview_audio(cx);
                                    cx.notify();
                                }),
                            )
                            .child(Self::render_lucide_icon(play_icon, 14.0, rgb(theme.text))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .h(px(6.0))
                            .rounded(px(self.tokens.radii.sm))
                            .overflow_hidden()
                            .bg(rgb(theme.bg_sunken))
                            .child(
                                div()
                                    .h_full()
                                    .w(relative(progress))
                                    .rounded(px(self.tokens.radii.sm))
                                    .bg(rgb(theme.accent)),
                            ),
                    )
                    .child(
                        div()
                            .min_w(px(92.0))
                            .text_size(px(SFTP_TEXT_XS))
                            .text_color(rgb(theme.text_muted))
                            .child(format!(
                                "{} / {}",
                                format_sftp_media_time(position),
                                format_sftp_media_time(duration)
                            )),
                    )
                    .when(can_seek, |row| {
                        row.child(
                            div()
                                .px_2()
                                .py_1()
                                .rounded(px(self.tokens.radii.sm))
                                .text_size(px(SFTP_TEXT_XS))
                                .text_color(rgb(theme.text_muted))
                                .cursor_pointer()
                                .hover(move |button| button.bg(rgb(theme.bg_hover)))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _event, _window, cx| {
                                        let now = this.sftp_view.preview_audio.snapshot().position;
                                        let next = now.saturating_sub(std::time::Duration::from_secs(15));
                                        this.seek_sftp_preview_audio(next, cx);
                                        cx.notify();
                                    }),
                                )
                                .child("-15s"),
                        )
                        .child(
                            div()
                                .px_2()
                                .py_1()
                                .rounded(px(self.tokens.radii.sm))
                                .text_size(px(SFTP_TEXT_XS))
                                .text_color(rgb(theme.text_muted))
                                .cursor_pointer()
                                .hover(move |button| button.bg(rgb(theme.bg_hover)))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _event, _window, cx| {
                                        let snapshot = this.sftp_view.preview_audio.snapshot();
                                        let Some(duration) = snapshot.duration else {
                                            return;
                                        };
                                        let next = (snapshot.position
                                            + std::time::Duration::from_secs(15))
                                        .min(duration);
                                        this.seek_sftp_preview_audio(next, cx);
                                        cx.notify();
                                    }),
                                )
                                .child("+15s"),
                        )
                    })
                    .when_some(snapshot.error, |row, error| {
                        row.child(
                            div()
                                .text_size(px(SFTP_TEXT_XS))
                                .text_color(rgb(SFTP_RED))
                                .child(error),
                        )
                    }),
            )
            .child(
                div()
                    .text_size(px(SFTP_TEXT_XS))
                    .text_color(rgb(theme.text_muted))
                    .child(mime_type.to_string()),
            )
            .into_any_element()
    }

    fn render_sftp_preview_video(
        &self,
        path: &str,
        mime_type: &str,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        {
            let snapshot = self.sftp_view.preview_video_surface.snapshot();
            let detail = snapshot.error.unwrap_or_else(|| {
                "Native video playback is initializing.".to_string()
            });
            let fallback = self.render_sftp_native_asset_status_with_external(
                "Video",
                path,
                mime_type,
                &detail,
                _cx,
            );
            sftp_native_video_element(
                path.to_string(),
                self.sftp_view.preview_video_surface.clone(),
                fallback,
            )
            .into_any_element()
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            let snapshot = self.sftp_view.preview_video_surface.snapshot();
            let detail = snapshot.error.unwrap_or_else(|| {
                format!("{} backend is unavailable", snapshot.backend)
            });
            self.render_sftp_native_asset_status_with_external(
                "Video", path, mime_type, &detail, _cx,
            )
                .into_any_element()
        }
    }

    fn render_sftp_preview_office(
        &self,
        path: &str,
        mime_type: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_sftp_native_asset_status_with_external(
            "Office",
            path,
            mime_type,
            "Office preview requires the later Office -> PDF/image conversion pipeline.",
            cx,
        )
        .into_any_element()
    }

    fn render_sftp_native_asset_status_with_external(
        &self,
        title: &str,
        path: &str,
        mime_type: &str,
        detail: &str,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        self.render_sftp_native_asset_status(title, path, mime_type, detail)
            .child(self.render_sftp_external_open_button(path.to_string(), cx))
    }

    fn render_sftp_native_asset_status(
        &self,
        title: &str,
        path: &str,
        mime_type: &str,
        detail: &str,
    ) -> gpui::Div {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .min_h(px(456.0))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_panel))
            .text_size(px(SFTP_TEXT_XS))
            .text_color(rgb(theme.text_muted))
            .child(
                div()
                    .text_size(px(SFTP_TEXT_SM))
                    .text_color(rgb(theme.text))
                    .child(title.to_string()),
            )
            .child(mime_type.to_string())
            .child(div().max_w(px(680.0)).child(detail.to_string()))
            .child(
                div()
                    .max_w(px(680.0))
                    .truncate()
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .child(path.to_string()),
            )
    }

    fn render_sftp_external_open_button(
        &self,
        path: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .mt_2()
            .h(px(32.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg))
            .px_3()
            .text_size(px(SFTP_TEXT_XS))
            .text_color(rgb(theme.text))
            .cursor_pointer()
            .hover(move |button| button.bg(rgb(theme.bg_hover)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.open_sftp_preview_external(&path);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(Self::render_lucide_icon(
                LucideIcon::ExternalLink,
                SFTP_ICON_MD,
                rgb(theme.text),
            ))
            .child(self.i18n.t("sftp.preview.open_external"))
            .into_any_element()
    }

    fn render_sftp_preview_image(
        &self,
        source: impl Into<gpui::ImageSource>,
        fallback_label: String,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        gpui::img(source)
            .w_full()
            .h(px(456.0))
            .object_fit(ObjectFit::Contain)
            .with_fallback(move || {
                div()
                    .w_full()
                    .h(px(456.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(SFTP_TEXT_SM))
                    .text_color(rgb(theme.text_muted))
                    .child(fallback_label.clone())
                    .into_any_element()
            })
            .into_any_element()
    }

}
