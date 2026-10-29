use super::*;

pub(in crate::workspace) const SFTP_SETTINGS_CARD_PADDING: f32 = 20.0; // Tauri p-5
// Leave room for large transfer rates and the localized unlimited placeholder.
const SFTP_SPEED_LIMIT_INPUT_WIDTH: f32 = 180.0;

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
                    self.card_title("settings_view.sftp.opening"),
                    self.setting_row(
                        "settings_view.sftp.presentation",
                        "settings_view.sftp.presentation_hint",
                        self.sftp_select_control(
                            SettingsSelect::SftpPresentation,
                            sftp_presentation_label(settings.sftp.presentation, &self.i18n),
                            cx,
                        ),
                        cx,
                    ),
                    self.card_separator(),
                    self.setting_row(
                        "settings_view.sftp.protocol",
                        "settings_view.sftp.protocol_hint",
                        self.sftp_select_control(
                            SettingsSelect::SftpProtocol,
                            file_transfer_protocol_label(
                                settings.sftp.transfer_protocol,
                                &self.i18n,
                            ),
                            cx,
                        ),
                        cx,
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
                        .child(self.setting_row(
                            "settings_view.sftp.conflict",
                            "settings_view.sftp.conflict_hint",
                            self.sftp_select_control(
                                SettingsSelect::SftpConflict,
                                conflict_label(settings.sftp.conflict_action, &self.i18n),
                                cx,
                            ),
                            cx,
                        ))
                        .into_any_element(),
                ],
                0.0,
            );
        }

        if section_index != 1 {
            return div().into_any_element();
        }

        let mut performance_rows = vec![
            self.card_title("settings_view.sftp.performance"),
            self.setting_row(
                "settings_view.sftp.concurrent",
                "settings_view.sftp.concurrent_hint",
                self.sftp_select_control(
                    SettingsSelect::SftpConcurrent,
                    sftp_transfer_count_label(&self.i18n, settings.sftp.max_concurrent_transfers),
                    cx,
                ),
                cx,
            ),
            self.card_separator(),
            self.setting_row(
                "settings_view.sftp.directory_parallelism",
                "settings_view.sftp.directory_parallelism_hint",
                self.sftp_select_control(
                    SettingsSelect::SftpDirectoryParallelism,
                    sftp_transfer_count_label(&self.i18n, settings.sftp.directory_parallelism),
                    cx,
                ),
                cx,
            ),
            self.card_separator(),
            self.setting_row(
                "settings_view.sftp.bandwidth",
                "settings_view.sftp.bandwidth_hint",
                checkbox(
                    &self.tokens,
                    "sftp-speed-limit",
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
                cx,
            ),
        ];

        if settings.sftp.speed_limit_enabled {
            performance_rows.push(
                div()
                    .pt(px(8.0))
                    .child(self.setting_row(
                        "settings_view.sftp.speed_limit",
                        "",
                        self.settings_text_input_control(
                            SettingsInput::SftpSpeedLimitKbps,
                            settings.sftp.speed_limit_kbps.to_string(),
                            self.i18n.t("settings_view.sftp.unlimited_placeholder"),
                            SFTP_SPEED_LIMIT_INPUT_WIDTH,
                            cx,
                        ),
                        cx,
                    ))
                    .into_any_element(),
            );
        }

        self.sftp_settings_card(performance_rows, 16.0)
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
            Some(self.tokens.metrics.settings_select_width),
            cx,
        )
    }
}

pub(in crate::workspace) fn sftp_presentation_label(
    preference: oxideterm_settings::SftpPresentationPreference,
    i18n: &I18n,
) -> String {
    let key = match preference {
        oxideterm_settings::SftpPresentationPreference::Ask => {
            "settings_view.sftp.presentation_ask"
        }
        oxideterm_settings::SftpPresentationPreference::Tab => {
            "settings_view.sftp.presentation_tab"
        }
        oxideterm_settings::SftpPresentationPreference::Sidebar => {
            "settings_view.sftp.presentation_sidebar"
        }
    };
    i18n.t(key)
}
