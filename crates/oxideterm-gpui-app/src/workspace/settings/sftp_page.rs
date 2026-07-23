use super::*;

pub(in crate::workspace) const SFTP_SETTINGS_CARD_PADDING: f32 = 20.0; // Tauri p-5
pub(in crate::workspace) const SFTP_SETTINGS_SELECT_WIDTH: f32 = 180.0; // Tauri w-[180px]

impl WorkspaceApp {
    pub(in crate::workspace) fn settings_sftp_section(
        &self,
        section_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.settings_store.settings();
        if section_index == 0 {
            return self.sftp_settings_card(
                vec![
                    self.sftp_settings_row(
                        "settings_view.sftp.protocol",
                        Some("settings_view.sftp.protocol_hint"),
                        self.sftp_select_control(
                            SettingsSelect::SftpProtocol,
                            file_transfer_protocol_label(
                                settings.sftp.transfer_protocol,
                                &self.i18n,
                            ),
                            cx,
                        ),
                    ),
                    self.card_separator(),
                    self.sftp_settings_row(
                        "settings_view.sftp.concurrent",
                        Some("settings_view.sftp.concurrent_hint"),
                        self.sftp_select_control(
                            SettingsSelect::SftpConcurrent,
                            sftp_transfer_count_label(
                                &self.i18n,
                                settings.sftp.max_concurrent_transfers,
                            ),
                            cx,
                        ),
                    ),
                    self.card_separator(),
                    self.sftp_settings_row(
                        "settings_view.sftp.directory_parallelism",
                        Some("settings_view.sftp.directory_parallelism_hint"),
                        self.sftp_select_control(
                            SettingsSelect::SftpDirectoryParallelism,
                            sftp_transfer_count_label(
                                &self.i18n,
                                settings.sftp.directory_parallelism,
                            ),
                            cx,
                        ),
                    ),
                ],
                20.0,
            );
        }

        if section_index == 2 {
            return self.sftp_settings_card(
                vec![
                    div()
                        .mb(px(8.0))
                        .child(self.sftp_settings_row(
                            "settings_view.sftp.conflict",
                            Some("settings_view.sftp.conflict_hint"),
                            self.sftp_select_control(
                                SettingsSelect::SftpConflict,
                                conflict_label(settings.sftp.conflict_action, &self.i18n),
                                cx,
                            ),
                        ))
                        .into_any_element(),
                ],
                0.0,
            );
        }

        if section_index != 1 {
            return div().into_any_element();
        }

        let mut speed_rows = vec![
            self.sftp_settings_row(
                "settings_view.sftp.bandwidth",
                Some("settings_view.sftp.bandwidth_hint"),
                checkbox(
                    &self.tokens,
                    String::new(),
                    settings.sftp.speed_limit_enabled,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.edit_settings(
                            |settings| {
                                settings.sftp.speed_limit_enabled =
                                    !settings.sftp.speed_limit_enabled
                            },
                            cx,
                        );
                    }),
                )
                .into_any_element(),
            ),
        ];

        if settings.sftp.speed_limit_enabled {
            speed_rows.push(
                div()
                    .pt(px(8.0))
                    .child(self.sftp_settings_row(
                        "settings_view.sftp.speed_limit",
                        None,
                        self.settings_text_input_control(
                            SettingsInput::SftpSpeedLimitKbps,
                            settings.sftp.speed_limit_kbps.to_string(),
                            "0 = unlimited".to_string(),
                            SFTP_SETTINGS_SELECT_WIDTH,
                            cx,
                        ),
                    ))
                    .into_any_element(),
            );
        }

        self.sftp_settings_card(speed_rows, 16.0)
    }

    pub(in crate::workspace) fn sftp_settings_card(
        &self,
        rows: Vec<AnyElement>,
        gap: f32,
    ) -> AnyElement {
        let card = div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .p(px(SFTP_SETTINGS_CARD_PADDING))
            .flex()
            .flex_col()
            .gap(px(gap))
            .children(rows);
        self.settings_card_surface(card, self.tokens.ui.bg_card)
            .into_any_element()
    }

    pub(in crate::workspace) fn sftp_settings_row(
        &self,
        label_key: &str,
        hint_key: Option<&str>,
        control: AnyElement,
    ) -> AnyElement {
        let mut label = div()
            .min_w(px(0.0))
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t(label_key)),
            );
        if let Some(hint_key) = hint_key {
            label = label.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(hint_key)),
            );
        }

        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .child(label)
            .child(control)
            .into_any_element()
    }

    pub(in crate::workspace) fn sftp_select_control(
        &self,
        select_id: SettingsSelect,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.settings_select_control(
            select_id,
            value,
            false,
            Some(SFTP_SETTINGS_SELECT_WIDTH),
            cx,
        )
    }
}
