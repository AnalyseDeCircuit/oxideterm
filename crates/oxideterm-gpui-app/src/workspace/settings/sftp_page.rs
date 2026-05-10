const SFTP_SETTINGS_CARD_PADDING: f32 = 20.0; // Tauri p-5
const SFTP_SETTINGS_SELECT_WIDTH: f32 = 180.0; // Tauri w-[180px]

impl WorkspaceApp {
    fn settings_sftp(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        let mut speed_rows = vec![self.sftp_settings_row(
            "settings_view.sftp.bandwidth",
            Some("settings_view.sftp.bandwidth_hint"),
            checkbox(&self.tokens, String::new(), settings.sftp.speed_limit_enabled)
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
        )];

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

        vec![
            self.sftp_settings_card(
                vec![
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
                            sftp_transfer_count_label(&self.i18n, settings.sftp.directory_parallelism),
                            cx,
                        ),
                    ),
                ],
                20.0,
            ),
            self.sftp_settings_card(speed_rows, 16.0),
            self.sftp_settings_card(
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
            ),
        ]
    }

    fn sftp_settings_card(&self, rows: Vec<AnyElement>, gap: f32) -> AnyElement {
        div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(self.settings_panel_background(self.tokens.ui.bg_card))
            .p(px(SFTP_SETTINGS_CARD_PADDING))
            .flex()
            .flex_col()
            .gap(px(gap))
            .children(rows)
            .into_any_element()
    }

    fn sftp_settings_row(
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

    fn sftp_select_control(
        &self,
        select_id: SettingsSelect,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let anchor_id = select_id.anchor_id();
        let workspace = cx.entity();
        let trigger = select_trigger(&self.tokens, value, false, false)
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.focused_settings_input = None;
                    this.open_settings_select = if this.open_settings_select == Some(select_id) {
                        None
                    } else {
                        Some(select_id)
                    };
                    cx.stop_propagation();
                    cx.notify();
                }),
            );

        div()
            .relative()
            .w(px(SFTP_SETTINGS_SELECT_WIDTH))
            .child(select_anchor_probe(
                anchor_id,
                trigger,
                move |anchor, _window, cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        this.update_select_anchor(anchor, cx);
                    });
                },
            ))
            .into_any_element()
    }

}
