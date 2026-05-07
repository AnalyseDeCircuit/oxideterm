impl WorkspaceApp {
    fn render_sftp_dialog(
        &self,
        dialog: SftpDialog,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
                self.i18n.t("sftp.conflict.description"),
                self.render_sftp_conflict_body(has_background),
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
                self.render_sftp_preview_body(has_background),
                Some(self.i18n.t("sftp.preview.close")),
            ),
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
            .child(
                div()
                    .w(px(match dialog {
                        SftpDialog::Diff { .. } | SftpDialog::Preview { .. } => 960.0,
                        _ => 512.0,
                    }))
                    .max_w(relative(0.9))
                    .max_h(relative(0.9))
                    .overflow_hidden()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(sftp_panel_bg(theme.bg_elevated, has_background, 0xff))
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
                            .bg(sftp_panel_bg(theme.bg_panel, has_background, 0xff))
                            .child(
                                div()
                                    .text_size(px(SFTP_TEXT_SM))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(theme.text_heading))
                                    .child(title),
                            )
                            .child(
                                div()
                                    .mt(px(6.0))
                                    .text_size(px(SFTP_TEXT_SM))
                                    .text_color(rgb(theme.text_muted))
                                    .child(description),
                            ),
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
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let footer = div()
            .px(px(16.0))
            .py(px(12.0))
            .border_t_1()
            .border_color(rgb(theme.border))
            .bg(sftp_panel_bg(theme.bg_panel, has_background, 0xff))
            .flex()
            .flex_row()
            .flex_wrap()
            .justify_end()
            .gap(px(8.0));

        if let SftpDialog::Preview { name } = dialog.clone() {
            let path = self.sftp_view.preview_path.clone().unwrap_or_default();
            let can_compare = self.can_compare_sftp_preview(&name);
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

        if matches!(dialog, SftpDialog::Conflict) {
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
                                this.close_sftp_dialog();
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ))
                        .child(self.render_sftp_text_button(
                            self.i18n.t("sftp.conflict.skip_older"),
                            false,
                            cx.listener(|this, _event, _window, cx| {
                                this.close_sftp_dialog();
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(8.0))
                        .child(self.render_sftp_text_button(
                            self.i18n.t("sftp.conflict.keep_both"),
                            false,
                            cx.listener(|this, _event, _window, cx| {
                                this.close_sftp_dialog();
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ))
                        .child(self.render_sftp_text_button(
                            self.i18n.t("sftp.conflict.overwrite"),
                            true,
                            cx.listener(|this, _event, _window, cx| {
                                this.accept_sftp_dialog();
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

    fn render_sftp_drives_dialog_body(
        &self,
        has_background: bool,
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
                            .border_color(rgba((theme.border << 8) | 0x80))
                            .bg(sftp_panel_bg(theme.bg_panel, has_background, 0xff))
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
                                                        .bg(rgba((SFTP_YELLOW << 8) | 0x26))
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
        has_background: bool,
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
                    .bg(sftp_bg(theme.bg_sunken, has_background))
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
                    .bg(rgba((theme.bg << 8) | 0x80))
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

    fn render_sftp_conflict_body(&self, has_background: bool) -> AnyElement {
        let theme = self.tokens.ui;
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
                    .bg(sftp_panel_bg(theme.bg_panel, has_background, 0xff))
                    .text_size(px(SFTP_TEXT_SM))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child("config.toml"),
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
                                "sftp.conflict.local_file",
                                true,
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
                            "sftp.conflict.remote_file",
                            false,
                            has_background,
                        ),
                    )),
            )
            .into_any_element()
    }

    fn render_sftp_file_compare_card(
        &self,
        label_key: &'static str,
        newer: bool,
        has_background: bool,
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
                rgba(0x052e1680)
            } else {
                sftp_panel_bg(theme.bg_panel, has_background, 0xff)
            })
            .child(
                div()
                    .mb(px(8.0))
                    .text_size(px(SFTP_TEXT_XS))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t(label_key).to_uppercase()),
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
                    .child("4.2 KB"),
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
                    .child("2026-05-07 14:30"),
            )
            .into_any_element()
    }

    fn render_sftp_diff_body(
        &self,
        local_path: &str,
        local_content: &str,
        remote_path: &str,
        remote_content: &str,
        has_background: bool,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let lines = compute_sftp_diff(local_content, remote_content);
        let stats = sftp_diff_stats(&lines);
        div()
            .h(px(480.0))
            .flex()
            .flex_col()
            .bg(sftp_bg(theme.bg_sunken, has_background))
            .child(
                div()
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
                            .bg(rgba(0x7f1d1d33))
                            .text_color(rgb(0xfca5a5))
                            .child(format!(
                                "{}: {}",
                                self.i18n.t("sftp.diff.local"),
                                sftp_file_name(local_path)
                            ))
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
                            .bg(rgba(0x14532d33))
                            .text_color(rgb(0x86efac))
                            .child(format!(
                                "{}: {}",
                                self.i18n.t("sftp.diff.remote"),
                                sftp_file_name(remote_path)
                            ))
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
                    .flex_1()
                    .overflow_y_scroll()
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .text_size(px(SFTP_TEXT_XS))
                    .when(lines.is_empty(), |body| {
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
                    .children(lines.into_iter().map(|line| {
                        let removed = line.kind == SftpDiffLineKind::Removed;
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
                            .flex()
                            .border_b_1()
                            .border_color(rgba((theme.border << 8) | 0x80))
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
                    })),
            )
            .into_any_element()
    }

    fn render_sftp_preview_body(&self, has_background: bool) -> AnyElement {
        let theme = self.tokens.ui;
        let body = if self.sftp_view.preview_loading {
            self.render_sftp_preview_text(self.i18n.t("common.loading"))
        } else if let Some(error) = &self.sftp_view.preview_error {
            self.render_sftp_preview_text(error.clone())
        } else if let Some(content) = &self.sftp_view.preview_content {
            self.render_sftp_preview_content(content)
        } else {
            self.render_sftp_preview_text(String::new())
        };
        div()
            .h(px(520.0))
            .flex()
            .flex_col()
            .bg(sftp_bg(theme.bg_sunken, has_background))
            .child(
                div()
                    .id("sftp-preview-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .p(px(16.0))
                    .text_color(rgb(theme.text))
                    .child(body),
            )
            .into_any_element()
    }

    fn render_sftp_preview_text(&self, text: String) -> AnyElement {
        div()
            .font_family(settings_mono_font_family(self.settings_store.settings()))
            .text_size(px(SFTP_TEXT_XS))
            .child(text)
            .into_any_element()
    }

    fn render_sftp_preview_content(&self, content: &PreviewContent) -> AnyElement {
        match content {
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
                kind,
            } => self.render_sftp_preview_asset_placeholder(path, mime_type, kind.clone()),
            _ => self.render_sftp_preview_text(preview_content_text(content)),
        }
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

    fn render_sftp_preview_asset_placeholder(
        &self,
        path: &str,
        mime_type: &str,
        kind: AssetFileKind,
    ) -> AnyElement {
        // Non-image assets require the dedicated GPUI/WebView preview surface.
        // Keep the real temp asset path visible instead of masquerading as a media viewer.
        self.render_sftp_preview_text(format!("{kind:?} asset\n{mime_type}\n{path}"))
    }
}
