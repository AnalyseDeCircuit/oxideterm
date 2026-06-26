impl WorkspaceApp {
    fn render_sftp_drives_dialog_body(
        &self,
        _has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let drives = local_drives();
        let drive_count = drives.len();
        div()
            .px(px(16.0))
            .py(px(12.0))
            .child(
                div()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .overflow_hidden()
                    .children(drives.into_iter().enumerate().map(|(index, drive)| {
                        let path = drive.path.clone();
                        let is_first = index == 0;
                        let is_last = index + 1 == drive_count;
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .gap(px(12.0))
                            .px(px(12.0))
                            .py(px(10.0))
                            .border_b_1()
                            .border_color(rgba((theme.border << 8) | SFTP_DIALOG_BORDER_HALF_ALPHA))
                            // Browser overflow clips the first and last painted
                            // rows into the rounded drive list; GPUI needs the
                            // edge rows to own those corners explicitly.
                            .when(is_first, |row| {
                                row.rounded_t(px(rounded_shell_child_radius(self.tokens.radii.md)))
                            })
                            .when(is_last, |row| {
                                row.rounded_b(px(rounded_shell_child_radius(self.tokens.radii.md)))
                            })
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
                    .selectable_overflow_y_scroll(
                        &self.selectable_text_scroll_handle("sftp-drives-scroll"),
                    )
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
                        None,
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
}
