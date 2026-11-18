use super::*;

impl CredentialPage<'_> {
    fn settings_totp_connection_list(
        &self,
        credential_id: Option<&str>,
        connection_ids: &HashSet<String>,
        _cx: &App,
    ) -> AnyElement {
        let tokens = self.tokens;
        let scroll = (self.scroll)("settings-totp-connections-scroll");
        let mut list = div().w_full().flex().flex_col().gap_1();
        if self.store.connections().is_empty() {
            list = list.child(
                self.settings_privilege_hint(self.i18n.t("sessionManager.table.no_connections")),
            );
        }
        for connection in self.store.connections() {
            let id = connection.id.clone();
            let selected = connection_ids.contains(&id);
            let other = connection
                .options
                .totp_credential_id
                .as_deref()
                .filter(|id| Some(*id) != credential_id)
                .map(|id| {
                    self.store
                        .totp_credentials()
                        .iter()
                        .find(|credential| credential.id == id)
                        .map(|credential| credential.name.clone())
                        .unwrap_or_else(|| self.i18n.t("settings_view.totp.unavailable"))
                });
            list = list.child(
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .p_2()
                    .rounded(px(tokens.radii.sm))
                    .cursor_pointer()
                    .hover(move |row| row.bg(rgb(tokens.ui.bg_hover)))
                    .on_mouse_down(
                        MouseButton::Left,
                        self.select(move |settings, cx| {
                            if let Some(draft) = &mut settings.totp_draft {
                                if !draft.connection_ids.remove(&id) {
                                    draft.connection_ids.insert(id.clone());
                                }
                            }
                            cx.notify();
                        }),
                    )
                    .child(checkbox(
                        &tokens,
                        format!("totp-connection-{}", connection.id),
                        String::new(),
                        selected,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(div().truncate().child(connection.name.clone()))
                            .child(self.settings_privilege_hint(format!(
                                "{}@{}:{}",
                                connection.username, connection.host, connection.port
                            )))
                            .when_some(other, |row, name| {
                                row.child(
                                    self.settings_privilege_hint(
                                        self.i18n
                                            .t("settings_view.totp.replace_binding")
                                            .replace("{{name}}", &name),
                                    ),
                                )
                            }),
                    ),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(form_field(
                &tokens,
                self.i18n.t("settings_view.totp.connections"),
                div()
                    .id("settings-totp-connections")
                    .relative()
                    .w_full()
                    .max_h(px(288.0))
                    .child(
                        div()
                            .id("settings-totp-connections-scroll")
                            .w_full()
                            .max_h(px(288.0))
                            .overflow_y_scroll()
                            .track_scroll(&scroll)
                            .on_scroll(|_, _, cx| cx.stop_propagation())
                            .child(list),
                    )
                    .vertical_scrollbar(&scroll),
            ))
            .child(self.settings_privilege_hint(self.i18n.t("settings_view.totp.connections_hint")))
            .into_any_element()
    }

    pub fn totp(&self, cx: &App) -> AnyElement {
        let tokens = self.tokens;
        let entries = self.store.totp_credentials().to_vec();
        let mut body = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(self.workspace_toolbar_action_button(
                self.i18n.t("settings_view.totp.add"),
                None,
                ToolbarButtonOptions::default(),
                self.press(CredentialRequest::EditTotp(None)),
            ));
        for credential in entries {
            let edit = credential.id.clone();
            let id = credential.id.clone();
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .py_2()
                    .border_b_1()
                    .border_color(rgba((tokens.ui.border << 8) | 0x66))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(credential.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(tokens.metrics.ui_text_xs))
                            .text_color(rgb(tokens.ui.text_muted))
                            .child(self.i18n.t(if credential.enabled {
                                "settings_view.totp.enabled"
                            } else {
                                "settings_view.totp.disabled"
                            })),
                    )
                    .child(self.workspace_toolbar_action_button(
                        self.i18n.t("sessionManager.actions.edit"),
                        None,
                        ToolbarButtonOptions::default(),
                        self.press(CredentialRequest::EditTotp(Some(edit))),
                    ))
                    .child(self.workspace_toolbar_action_button(
                        self.i18n.t("common.actions.delete"),
                        None,
                        ToolbarButtonOptions::default(),
                        self.press(CredentialRequest::DeleteTotp(id)),
                    )),
            );
        }
        if let Some(draft) = self.state.read(cx).totp_draft.as_ref() {
            let id = draft.id.clone();
            let name = draft.name.clone();
            let pattern = draft.pattern.clone();
            let period = draft.period.clone();
            let algorithm = draft.algorithm;
            let digits = draft.digits;
            let enabled = draft.enabled;
            let connection_ids = draft.connection_ids.clone();
            let mut editor = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(self.settings_privilege_text_field(
                    "settings_view.totp.name",
                    SettingsInput::TotpName,
                    name,
                    String::new(),
                    false,
                    cx,
                ))
                .child(self.settings_privilege_text_field(
                    "settings_view.totp.secret",
                    SettingsInput::TotpSecret,
                    "",
                    self.i18n.t(if id.is_some() {
                        "settings_view.totp.keep_secret"
                    } else {
                        "settings_view.totp.secret_hint"
                    }),
                    true,
                    cx,
                ))
                .child(self.settings_privilege_hint(self.i18n.t("settings_view.totp.uri_hint")))
                .child(self.settings_privilege_text_field(
                    "settings_view.totp.pattern",
                    SettingsInput::TotpPattern,
                    pattern,
                    String::new(),
                    false,
                    cx,
                ));
            let mut algorithms = div()
                .flex()
                .items_center()
                .flex_wrap()
                .gap_2()
                .child(self.i18n.t("settings_view.totp.algorithm"));
            for (value, label) in [
                (TotpAlgorithm::Sha1, "SHA-1"),
                (TotpAlgorithm::Sha256, "SHA-256"),
                (TotpAlgorithm::Sha512, "SHA-512"),
            ] {
                algorithms = algorithms.child(self.workspace_toolbar_action_button(
                    label.into(),
                    None,
                    ToolbarButtonOptions {
                        button: ButtonOptions {
                            variant: if value == algorithm {
                                ButtonVariant::Default
                            } else {
                                ButtonVariant::Ghost
                            },
                            size: ButtonSize::Sm,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    self.select(move |settings, cx| {
                        if let Some(draft) = &mut settings.totp_draft {
                            draft.algorithm = value;
                        }
                        cx.notify();
                    }),
                ));
            }
            let mut digit_buttons = div()
                .flex()
                .items_center()
                .gap_2()
                .child(self.i18n.t("settings_view.totp.digits"));
            for value in [6, 8] {
                digit_buttons = digit_buttons.child(self.workspace_toolbar_action_button(
                    value.to_string(),
                    None,
                    ToolbarButtonOptions {
                        button: ButtonOptions {
                            variant: if value == digits {
                                ButtonVariant::Default
                            } else {
                                ButtonVariant::Ghost
                            },
                            size: ButtonSize::Sm,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    self.select(move |settings, cx| {
                        if let Some(draft) = &mut settings.totp_draft {
                            draft.digits = value;
                        }
                        cx.notify();
                    }),
                ));
            }
            editor = editor
                .child(algorithms)
                .child(digit_buttons)
                .child(self.settings_privilege_text_field(
                    "settings_view.totp.period",
                    SettingsInput::TotpPeriod,
                    period,
                    "30".into(),
                    false,
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .cursor_pointer()
                        .on_mouse_down(
                            MouseButton::Left,
                            self.select(|settings, cx| {
                                if let Some(draft) = &mut settings.totp_draft {
                                    draft.enabled = !draft.enabled;
                                }
                                cx.notify();
                            }),
                        )
                        .child(checkbox(&tokens, "totp-enabled", String::new(), enabled))
                        .child(self.i18n.t("settings_view.totp.enabled")),
                )
                .child(self.settings_totp_connection_list(id.as_deref(), &connection_ids, cx))
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(self.workspace_toolbar_action_button(
                            self.i18n.t("common.actions.cancel"),
                            None,
                            ToolbarButtonOptions::default(),
                            self.press(CredentialRequest::CancelTotp),
                        ))
                        .child(self.workspace_toolbar_action_button(
                            self.i18n.t("sessionManager.privilege_credentials.save"),
                            None,
                            ToolbarButtonOptions {
                                button: ButtonOptions {
                                    variant: ButtonVariant::Default,
                                    size: ButtonSize::Sm,
                                    ..Default::default()
                                },
                                ..Default::default()
                            },
                            self.press(CredentialRequest::SaveTotp),
                        )),
                );
            body = body.child(editor);
        }
        if let Some(error) = self.state.read(cx).totp_error {
            let key = match error {
                TotpError::InvalidSecret => "settings_view.totp.invalid_secret",
                TotpError::InvalidParameters => "settings_view.totp.invalid_parameters",
                TotpError::InvalidPattern => "settings_view.totp.invalid_pattern",
                TotpError::InUse => "settings_view.totp.in_use",
                TotpError::Unavailable => "settings_view.totp.unavailable",
                TotpError::SaveFailed => "settings_view.totp.save_failed",
            };
            body = body.child(
                div()
                    .text_color(rgb(tokens.ui.error))
                    .child(self.i18n.t(key)),
            );
        }
        self.settings_card(
            "settings_view.totp.title",
            "settings_view.totp.description",
            vec![body.into_any_element()],
        )
    }
}
