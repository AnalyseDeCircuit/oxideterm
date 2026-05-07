impl WorkspaceApp {
    fn render_sftp_transfer_queue(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active_count = self
            .sftp_view
            .transfers
            .iter()
            .filter(|item| {
                matches!(
                    item.state,
                    SftpTransferState::Active | SftpTransferState::Pending
                )
            })
            .count();
        let has_completed = self.sftp_view.transfers.iter().any(|item| {
            matches!(
                item.state,
                SftpTransferState::Completed | SftpTransferState::Cancelled
            )
        });

        div()
            .h(px(SFTP_QUEUE_HEIGHT))
            .flex_none()
            .flex()
            .flex_col()
            .bg(sftp_bg(theme.bg, has_background))
            .border_t_1()
            .border_color(sftp_border(theme.border, has_background))
            .child(
                div()
                    .h(px(29.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .px(px(8.0))
                    .py(px(4.0))
                    .bg(sftp_panel_bg(theme.bg_panel, has_background, 0xff))
                    .border_b_1()
                    .border_color(sftp_border(theme.border, has_background))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(SFTP_TEXT_XS))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(theme.text_muted))
                            .child(self.queue_title(active_count))
                            .when(true, |row| {
                                row.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(4.0))
                                        .text_color(rgb(theme.accent))
                                        .cursor_pointer()
                                        .child(Self::render_lucide_icon(
                                            LucideIcon::Clock,
                                            SFTP_ICON_SM,
                                            rgb(theme.accent),
                                        ))
                                        .child(
                                            self.i18n
                                                .t("sftp.queue.incomplete_count")
                                                .replace("{{count}}", "1"),
                                        )
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|this, _event, _window, cx| {
                                                this.sftp_view.show_incomplete =
                                                    !this.sftp_view.show_incomplete;
                                                cx.stop_propagation();
                                                cx.notify();
                                            }),
                                        ),
                                )
                            }),
                    )
                    .when(has_completed, |header| {
                        header.child(
                            div()
                                .h(px(24.0))
                                .px(px(8.0))
                                .flex()
                                .items_center()
                                .rounded(px(self.tokens.radii.sm))
                                .text_size(px(SFTP_TEXT_XS))
                                .text_color(rgb(theme.text))
                                .hover(move |button| button.bg(rgb(theme.bg_hover)))
                                .cursor_pointer()
                                .child(self.i18n.t("sftp.queue.clear_done"))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.sftp_view.transfers.retain(|item| {
                                            !matches!(
                                                item.state,
                                                SftpTransferState::Completed
                                                    | SftpTransferState::Cancelled
                                            )
                                        });
                                        cx.stop_propagation();
                                        cx.notify();
                                    }),
                                ),
                        )
                    }),
            )
            .when(self.sftp_view.show_incomplete, |queue| {
                queue.child(self.render_sftp_incomplete_section(has_background, cx))
            })
            .child(
                div()
                    .id("sftp-transfer-queue-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .p(px(8.0))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .when(self.sftp_view.transfers.is_empty(), |body| {
                        body.child(
                            div()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(SFTP_TEXT_SM))
                                .text_color(rgb(theme.text_muted))
                                .child(self.i18n.t("sftp.queue.empty")),
                        )
                    })
                    .children(self.sftp_view.transfers.iter().cloned().map(|transfer| {
                        self.render_sftp_transfer_row(transfer, has_background, cx)
                    })),
            )
            .into_any_element()
    }

    fn render_sftp_incomplete_section(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .border_b_1()
            .border_color(sftp_border(theme.border, has_background))
            .bg(sftp_panel_bg(theme.bg_card, has_background, 0xff))
            .child(
                div()
                    .px(px(8.0))
                    .py(px(4.0))
                    .text_size(px(SFTP_TEXT_10))
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t("sftp.queue.incomplete_title").to_uppercase()),
            )
            .child(
                div()
                    .id("sftp-incomplete-transfer-scroll")
                    .max_h(px(128.0))
                    .overflow_y_scroll()
                    .p(px(8.0))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(8.0))
                            .p(px(8.0))
                            .rounded(px(self.tokens.radii.sm))
                            .border_1()
                            .border_color(rgba((SFTP_YELLOW << 8) | 0x4d))
                            .bg(sftp_panel_bg(
                                theme.bg_panel,
                                has_background,
                                SFTP_PANEL_80_ALPHA,
                            ))
                            .text_size(px(SFTP_TEXT_XS))
                            .child(
                                div()
                                    .w(px(16.0))
                                    .text_center()
                                    .text_color(rgb(SFTP_YELLOW))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child("↓"),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .child(
                                        div()
                                            .truncate()
                                            .text_color(rgb(theme.text))
                                            .child("archive.tar"),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .gap(px(8.0))
                                            .text_size(px(SFTP_TEXT_10))
                                            .text_color(rgb(theme.text_muted))
                                            .child("Download")
                                            .child("•")
                                            .child("42%")
                                            .child("•")
                                            .child("18.0 MB / 42.0 MB"),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(SFTP_TEXT_10))
                                    .text_color(rgb(theme.text_muted))
                                    .child(self.i18n.t("sftp.queue.status_paused")),
                            )
                            .child(self.render_sftp_icon_button(
                                LucideIcon::Play,
                                self.i18n.t("sftp.queue.resume_tooltip"),
                                cx.listener(|this, _event, _window, cx| {
                                    this.sftp_view.show_incomplete = false;
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            )),
                    ),
            )
            .into_any_element()
    }

    fn render_sftp_transfer_row(
        &self,
        transfer: SftpTransferItem,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let progress = if transfer.size == 0 {
            0.0
        } else {
            (transfer.transferred as f32 / transfer.size as f32).clamp(0.0, 1.0)
        };
        let status_color = match transfer.state {
            SftpTransferState::Error => SFTP_RED,
            SftpTransferState::Cancelled => SFTP_YELLOW,
            _ => theme.text_muted,
        };
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(12.0))
            .p(px(8.0))
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(match transfer.state {
                SftpTransferState::Error => rgba((SFTP_RED << 8) | 0x80),
                SftpTransferState::Cancelled => rgba((SFTP_YELLOW << 8) | 0x4d),
                _ => rgba(theme.border << 8),
            })
            .bg(sftp_panel_bg(
                theme.bg_panel,
                has_background,
                SFTP_PANEL_80_ALPHA,
            ))
            .hover(move |row| row.border_color(rgb(theme.border)))
            .text_size(px(SFTP_TEXT_SM))
            .child(
                div()
                    .w(px(16.0))
                    .text_center()
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(theme.text_muted))
                    .child(match transfer.direction {
                        SftpTransferDirection::Upload => "↑",
                        SftpTransferDirection::Download => "↓",
                    }),
            )
            .child(
                div()
                    .w(px(192.0))
                    .truncate()
                    .text_color(rgb(theme.text))
                    .child(transfer.name.clone()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .h(px(6.0))
                            .w_full()
                            .overflow_hidden()
                            .rounded_full()
                            .border_1()
                            .border_color(rgb(theme.border))
                            .bg(rgb(theme.bg_panel))
                            .child(div().h_full().w(relative(progress)).bg(rgb(theme.accent))),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .text_size(px(SFTP_TEXT_10))
                            .text_color(rgb(theme.text_muted))
                            .child(format!(
                                "{} / {}",
                                format_file_size(transfer.transferred),
                                format_file_size(transfer.size)
                            ))
                            .child(format!("{}%", (progress * 100.0).round() as u32)),
                    ),
            )
            .child(
                div()
                    .w(px(96.0))
                    .text_align(gpui::TextAlign::Right)
                    .text_size(px(SFTP_TEXT_XS))
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .text_color(rgb(status_color))
                    .child(self.transfer_status_text(&transfer)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .child(match transfer.state {
                        SftpTransferState::Completed => {
                            Self::render_lucide_icon(LucideIcon::Check, 16.0, rgb(SFTP_GREEN))
                        }
                        SftpTransferState::Cancelled | SftpTransferState::Error => {
                            Self::render_lucide_icon(
                                LucideIcon::ShieldQuestion,
                                16.0,
                                rgb(status_color),
                            )
                        }
                        _ => div().w(px(0.0)).into_any_element(),
                    })
                    .when(
                        matches!(
                            transfer.state,
                            SftpTransferState::Active | SftpTransferState::Pending
                        ),
                        |actions| {
                            actions.child(self.render_sftp_icon_button(
                                LucideIcon::Pause,
                                self.i18n.t("sftp.queue.pause_tooltip"),
                                cx.listener({
                                    let id = transfer.id;
                                    move |this, _event, _window, cx| {
                                        this.set_sftp_transfer_state(id, SftpTransferState::Paused);
                                        cx.stop_propagation();
                                        cx.notify();
                                    }
                                }),
                            ))
                        },
                    )
                    .when(transfer.state == SftpTransferState::Paused, |actions| {
                        actions.child(self.render_sftp_icon_button(
                            LucideIcon::Play,
                            self.i18n.t("sftp.queue.resume_tooltip"),
                            cx.listener({
                                let id = transfer.id;
                                move |this, _event, _window, cx| {
                                    this.set_sftp_transfer_state(id, SftpTransferState::Pending);
                                    cx.stop_propagation();
                                    cx.notify();
                                }
                            }),
                        ))
                    })
                    .child(self.render_sftp_icon_button(
                        LucideIcon::X,
                        self.i18n.t(
                            if matches!(
                                transfer.state,
                                SftpTransferState::Active
                                    | SftpTransferState::Pending
                                    | SftpTransferState::Paused
                            ) {
                                "sftp.queue.cancel_tooltip"
                            } else {
                                "sftp.queue.remove_tooltip"
                            },
                        ),
                        cx.listener({
                            let id = transfer.id;
                            move |this, _event, _window, cx| {
                                this.cancel_or_remove_sftp_transfer(id);
                                cx.stop_propagation();
                                cx.notify();
                            }
                        }),
                    )),
            )
            .into_any_element()
    }
}
