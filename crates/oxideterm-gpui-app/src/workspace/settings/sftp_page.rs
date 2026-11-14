use super::*;

// Leave room for large transfer rates and the localized unlimited placeholder.
const SFTP_SPEED_LIMIT_INPUT_WIDTH: f32 = 180.0;

impl WorkspaceApp {
    pub(in crate::workspace) fn settings_sftp_section(
        &self,
        section: SettingsSectionId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.settings_store.settings();
        if section == SettingsSectionId::SftpOpening {
            return self.settings_card(
                "settings_view.sftp.opening",
                "",
                self.separated_settings_rows(vec![
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
                ]),
            );
        }

        if section == SettingsSectionId::SftpConflict {
            return self.settings_card(
                "settings_view.sftp.conflict",
                "",
                vec![self.setting_row(
                    "settings_view.sftp.conflict",
                    "settings_view.sftp.conflict_hint",
                    self.sftp_select_control(
                        SettingsSelect::SftpConflict,
                        conflict_label(settings.sftp.conflict_action, &self.i18n),
                        cx,
                    ),
                    cx,
                )],
            );
        }

        if section != SettingsSectionId::SftpPerformance {
            return div().into_any_element();
        }

        let mut performance_rows = vec![
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
            self.bool_row(
                "settings_view.sftp.bandwidth",
                "settings_view.sftp.bandwidth_hint",
                settings.sftp.speed_limit_enabled,
                |settings, enabled| settings.sftp.speed_limit_enabled = enabled,
                cx,
            ),
        ];

        if settings.sftp.speed_limit_enabled {
            performance_rows.push(self.setting_row(
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
            ));
        }

        self.settings_card(
            "settings_view.sftp.performance",
            "",
            self.separated_settings_rows(performance_rows),
        )
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
