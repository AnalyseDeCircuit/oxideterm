use super::*;
use oxideterm_connections::totp::{
    DEFAULT_TOTP_PROMPT, TotpAlgorithm, TotpCredential, TotpError, TotpParameters,
};
use zeroize::Zeroizing;

pub(super) struct TotpDraft {
    pub id: Option<String>,
    pub name: String,
    pub secret: Zeroizing<String>,
    pub pattern: String,
    pub period: String,
    pub algorithm: TotpAlgorithm,
    pub digits: u8,
    pub enabled: bool,
}

impl TotpDraft {
    fn new(credential: Option<&TotpCredential>) -> Self {
        Self {
            id: credential.map(|entry| entry.id.clone()),
            name: credential
                .map(|entry| entry.name.clone())
                .unwrap_or_default(),
            secret: Zeroizing::new(String::new()),
            pattern: credential
                .map(|entry| entry.prompt_pattern.clone())
                .unwrap_or_else(|| DEFAULT_TOTP_PROMPT.into()),
            period: credential
                .map(|entry| entry.parameters.period)
                .unwrap_or(30)
                .to_string(),
            algorithm: credential
                .map(|entry| entry.parameters.algorithm)
                .unwrap_or_default(),
            digits: credential.map(|entry| entry.parameters.digits).unwrap_or(6),
            enabled: credential.is_none_or(|entry| entry.enabled),
        }
    }
}

impl WorkspaceApp {
    fn edit_totp_credential(&mut self, credential: Option<TotpCredential>, cx: &mut Context<Self>) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.totp_draft = Some(TotpDraft::new(credential.as_ref()));
            settings.totp_error = None;
            settings.settings_focused_input = None;
            cx.notify();
        });
        self.clear_ime_selection();
        cx.notify();
    }

    fn save_totp_draft(&mut self, cx: &mut Context<Self>) {
        let Some(mut draft) = self
            .settings_workspace
            .update(cx, |settings, _| settings.totp_draft.take())
        else {
            return;
        };
        let secret = SecretString::from(std::mem::take(&mut *draft.secret));
        let result = draft
            .period
            .parse()
            .map_err(|_| TotpError::InvalidParameters)
            .and_then(|period| {
                self.connection_store.save_totp_credential(
                    draft.id.as_deref(),
                    &draft.name,
                    (!secret.is_empty()).then_some(&secret),
                    TotpParameters {
                        algorithm: draft.algorithm,
                        digits: draft.digits,
                        period,
                    },
                    &draft.pattern,
                    draft.enabled,
                )
            });
        if result.is_ok() {
            self.queue_cloud_sync_dirty_refresh(cx);
        }
        self.settings_workspace.update(cx, |settings, cx| {
            settings.settings_focused_input = None;
            settings.totp_error = result.err();
            if settings.totp_error.is_some() {
                draft.secret = secret.into_zeroizing();
                settings.totp_draft = Some(draft);
            }
            cx.notify();
        });
        self.clear_ime_selection();
        cx.notify();
    }

    pub(in crate::workspace) fn settings_totp_credentials_card(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tokens = self.tokens;
        let entries = self.connection_store.totp_credentials().to_vec();
        let mut body = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(self.workspace_toolbar_action_button(
                self.i18n.t("settings_view.totp.add"),
                None,
                ToolbarButtonOptions::default(),
                cx.listener(|this, _, _, cx| this.edit_totp_credential(None, cx)),
            ));
        for credential in entries {
            let edit = credential.clone();
            let id = credential.id.clone();
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(tokens.ui.border))
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
                        cx.listener(move |this, _, _, cx| {
                            this.edit_totp_credential(Some(edit.clone()), cx)
                        }),
                    ))
                    .child(self.workspace_toolbar_action_button(
                        self.i18n.t("common.actions.delete"),
                        None,
                        ToolbarButtonOptions::default(),
                        cx.listener(move |this, _, _, cx| {
                            let result = this.connection_store.delete_totp_credential(&id);
                            if result.is_ok() {
                                this.queue_cloud_sync_dirty_refresh(cx);
                            }
                            this.settings_workspace.update(cx, |settings, cx| {
                                settings.totp_error = result.err();
                                cx.notify();
                            });
                            cx.notify();
                        }),
                    )),
            );
        }
        if let Some(draft) = self.settings_workspace.read(cx).totp_draft.as_ref() {
            let id = draft.id.clone();
            let name = draft.name.clone();
            let pattern = draft.pattern.clone();
            let period = draft.period.clone();
            let algorithm = draft.algorithm;
            let digits = draft.digits;
            let enabled = draft.enabled;
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
                    cx.listener(move |this, _, _, cx| {
                        this.settings_workspace.update(cx, |settings, cx| {
                            if let Some(draft) = &mut settings.totp_draft {
                                draft.algorithm = value;
                            }
                            cx.notify();
                        });
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
                    cx.listener(move |this, _, _, cx| {
                        this.settings_workspace.update(cx, |settings, cx| {
                            if let Some(draft) = &mut settings.totp_draft {
                                draft.digits = value;
                            }
                            cx.notify();
                        });
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
                            cx.listener(|this, _, _, cx| {
                                this.settings_workspace.update(cx, |settings, cx| {
                                    if let Some(draft) = &mut settings.totp_draft {
                                        draft.enabled = !draft.enabled;
                                    }
                                    cx.notify();
                                });
                                cx.stop_propagation();
                            }),
                        )
                        .child(checkbox(&tokens, String::new(), enabled))
                        .child(self.i18n.t("settings_view.totp.enabled")),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(self.workspace_toolbar_action_button(
                            self.i18n.t("common.actions.cancel"),
                            None,
                            ToolbarButtonOptions::default(),
                            cx.listener(|this, _, _, cx| {
                                this.settings_workspace.update(cx, |settings, cx| {
                                    settings.totp_draft = None;
                                    settings.totp_error = None;
                                    settings.settings_focused_input = None;
                                    cx.notify();
                                });
                                this.clear_ime_selection();
                                cx.notify();
                            }),
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
                            cx.listener(|this, _, _, cx| this.save_totp_draft(cx)),
                        )),
                );
            body = body.child(editor);
        }
        if let Some(error) = self.settings_workspace.read(cx).totp_error {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn totp_editor_accepts_shared_input_and_clears_seed_on_page_change(
        cx: &mut gpui::TestAppContext,
    ) {
        let entity = cx.new(SettingsWorkspaceEntity::new);
        entity.update(cx, |settings, cx| {
            settings.set_active_tab(SettingsTab::Privilege, cx);
            settings.totp_draft = Some(TotpDraft::new(None));
            assert!(settings.focus_settings_entity_input(SettingsInput::TotpName, cx));
            assert!(settings.replace_settings_entity_input(
                SettingsInput::TotpName,
                None,
                "跳板验证码",
                cx
            ));
            assert_eq!(
                settings.settings_entity_input_value(SettingsInput::TotpName),
                Some("跳板验证码")
            );
            assert!(settings.focus_settings_entity_input(SettingsInput::TotpSecret, cx));
            assert!(settings.replace_settings_entity_input(
                SettingsInput::TotpSecret,
                None,
                "JBSWY3DPEHPK3PXP",
                cx
            ));
            assert_eq!(
                settings.settings_entity_input_value(SettingsInput::TotpSecret),
                Some("JBSWY3DPEHPK3PXP")
            );
            assert!(SettingsInput::TotpSecret.is_secret());
            settings.set_active_tab(SettingsTab::Terminal, cx);
            assert!(
                settings
                    .settings_entity_input_value(SettingsInput::TotpSecret)
                    .is_none()
            );
        });
    }
}
