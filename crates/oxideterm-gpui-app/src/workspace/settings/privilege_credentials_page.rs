use super::*;

pub(in crate::workspace) const PRIVILEGE_SCOPE_LIST_WIDTH: f32 = 280.0; // Match the current scope rail width on comfortable layouts.
pub(in crate::workspace) const PRIVILEGE_DETAIL_MIN_WIDTH: f32 = 320.0; // Wrap the detail pane before fixed controls crush its labels.
pub(in crate::workspace) const PRIVILEGE_FORM_FIELD_MIN_WIDTH: f32 = 260.0; // Keep paired fields readable before the form wraps to one column.
const PRIVILEGE_SCOPE_LIST_MAX_HEIGHT: f32 = 360.0;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) struct SettingsPrivilegeScopeRow {
    id: String,
    title: String,
    subtitle: String,
    username_placeholder: String,
    credential_count: usize,
    local: bool,
    ssh_common: bool,
}

impl SettingsWorkspaceEntity {
    pub(in crate::workspace) fn privilege_prompt_type(&self) -> PrivilegePromptType {
        self.privilege_draft.prompt_type
    }

    pub(in crate::workspace) fn privilege_response_mode(&self) -> PrivilegeResponseMode {
        self.privilege_draft.response_mode
    }

    pub(in crate::workspace) fn privilege_snapshot(&self) -> PrivilegeCredentialSnapshot {
        PrivilegeCredentialSnapshot {
            credential_id: self.privilege_draft.credential_id.clone(),
            label: self.privilege_draft.label.clone(),
            prompt_type: self.privilege_draft.prompt_type,
            response_mode: self.privilege_draft.response_mode,
            username_hint: self.privilege_draft.username_hint.clone(),
            prompt_patterns: self.privilege_draft.prompt_patterns.clone(),
            host_patterns: self.privilege_draft.host_patterns.clone(),
            second_factor_patterns: self.privilege_draft.second_factor_patterns.clone(),
            second_factor_user_hint: self.privilege_draft.second_factor_user_hint.clone(),
            totp_credential_id: self.privilege_draft.totp_credential_id.clone(),
            enabled: self.privilege_draft.enabled,
            require_click_to_send: self.privilege_draft.require_click_to_send,
            error: self.privilege_error.clone(),
        }
    }

    pub(in crate::workspace) fn privilege_layout_flags(&self) -> (Option<&str>, bool, bool, bool) {
        (
            self.privilege_scope_id.as_deref(),
            self.privilege_draft.credential_id.is_some(),
            self.privilege_editor_open,
            self.privilege_error.is_some(),
        )
    }

    pub(in crate::workspace) fn set_privilege_scope(
        &mut self,
        scope_id: String,
        cx: &mut Context<Self>,
    ) {
        self.reset_privilege_draft_state();
        self.privilege_scope_id = Some(scope_id);
        cx.notify();
    }

    pub(in crate::workspace) fn reset_privilege_draft(&mut self, cx: &mut Context<Self>) {
        self.reset_privilege_draft_state();
        cx.notify();
    }

    pub(in crate::workspace) fn begin_new_privilege_credential(&mut self, cx: &mut Context<Self>) {
        self.reset_privilege_draft_state();
        self.privilege_editor_open = true;
        self.settings_focused_input = Some(SettingsInput::LocalPrivilegeLabel);
        cx.notify();
    }

    pub(in crate::workspace) fn edit_privilege_credential(
        &mut self,
        scope_id: String,
        credential: SavedPrivilegeCredential,
        cx: &mut Context<Self>,
    ) {
        self.reset_privilege_draft_state();
        self.privilege_scope_id = Some(scope_id);
        self.privilege_draft.credential_id = Some(credential.id);
        self.privilege_draft.label = credential.label;
        self.privilege_draft.prompt_type = credential.prompt_type;
        self.privilege_draft.response_mode = credential.response_mode;
        self.privilege_draft.username_hint = credential.username_hint.unwrap_or_default();
        self.privilege_draft.prompt_patterns = credential.prompt_patterns.join("\n");
        self.privilege_draft.host_patterns = if credential.host_patterns.is_empty() {
            "*".to_string()
        } else {
            credential.host_patterns.join("\n")
        };
        self.privilege_draft.second_factor_patterns = credential.second_factor_patterns.join("\n");
        self.privilege_draft.second_factor_user_hint =
            credential.second_factor_user_hint.unwrap_or_default();
        self.privilege_draft.totp_credential_id = credential.totp_credential_id;
        self.privilege_draft.enabled = credential.enabled;
        self.privilege_draft.require_click_to_send = credential.require_click_to_send;
        self.privilege_editor_open = true;
        self.settings_focused_input = Some(SettingsInput::LocalPrivilegeLabel);
        cx.notify();
    }

    pub(in crate::workspace) fn set_privilege_prompt_type(
        &mut self,
        prompt_type: PrivilegePromptType,
        cx: &mut Context<Self>,
    ) {
        self.privilege_draft.prompt_type = prompt_type;
        self.privilege_error = None;
        cx.notify();
    }

    pub(in crate::workspace) fn set_privilege_response_mode(
        &mut self,
        response_mode: PrivilegeResponseMode,
        cx: &mut Context<Self>,
    ) {
        self.privilege_draft.response_mode = response_mode;
        if !response_mode.uses_totp() {
            self.privilege_draft.totp_credential_id = None;
        }
        if !response_mode.waits_for_totp() {
            self.privilege_draft.second_factor_patterns.clear();
            self.privilege_draft.second_factor_user_hint.clear();
        }
        self.privilege_error = None;
        cx.notify();
    }

    pub(in crate::workspace) fn toggle_privilege_enabled(&mut self, cx: &mut Context<Self>) {
        self.privilege_draft.enabled = !self.privilege_draft.enabled;
        self.privilege_error = None;
        cx.notify();
    }

    pub(in crate::workspace) fn toggle_privilege_auto_submit(&mut self, cx: &mut Context<Self>) {
        self.privilege_draft.require_click_to_send = !self.privilege_draft.require_click_to_send;
        self.privilege_error = None;
        cx.notify();
    }

    pub(in crate::workspace) fn set_privilege_totp_credential(
        &mut self,
        credential_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.privilege_draft.totp_credential_id = credential_id;
        self.privilege_error = None;
        cx.notify();
    }

    pub(in crate::workspace) fn take_privilege_save_request(
        &mut self,
        connection_id: String,
    ) -> Option<SavePrivilegeCredentialRequest> {
        let label = self.privilege_draft.label.trim().to_string();
        if label.is_empty() {
            return None;
        }
        let prompt_patterns = self
            .privilege_draft
            .prompt_patterns
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let host_patterns = self
            .privilege_draft
            .host_patterns
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let second_factor_patterns = self
            .privilege_draft
            .second_factor_patterns
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let secret = if self.privilege_draft.secret.is_empty() {
            None
        } else {
            // Move the only UI-owned secret into the store request so retries
            // cannot retain a second credential copy in the Entity.
            let secret = std::mem::replace(
                &mut self.privilege_draft.secret,
                zeroize::Zeroizing::new(String::new()),
            );
            Some(SecretString::from(secret))
        };
        self.settings_focused_input = None;
        Some(SavePrivilegeCredentialRequest {
            connection_id,
            credential_id: self.privilege_draft.credential_id.clone(),
            label,
            prompt_type: self.privilege_draft.prompt_type,
            response_mode: self.privilege_draft.response_mode,
            username_hint: optional_trimmed_privilege_value(&self.privilege_draft.username_hint),
            prompt_patterns,
            host_patterns,
            second_factor_patterns,
            second_factor_user_hint: optional_trimmed_privilege_value(
                &self.privilege_draft.second_factor_user_hint,
            ),
            totp_credential_id: self.privilege_draft.totp_credential_id.clone(),
            secret,
            enabled: self.privilege_draft.enabled,
            require_click_to_send: self.privilege_draft.require_click_to_send,
        })
    }

    pub(in crate::workspace) fn finish_privilege_save(
        &mut self,
        scope_id: String,
        error: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if let Some(error) = error {
            self.privilege_error = Some(error);
        } else {
            self.reset_privilege_draft_state();
            self.privilege_scope_id = Some(scope_id);
        }
        cx.notify();
    }

    pub(in crate::workspace) fn finish_privilege_delete(
        &mut self,
        scope_id: String,
        credential_id: &str,
        error: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if let Some(error) = error {
            self.privilege_error = Some(error);
        } else {
            if self.privilege_draft.credential_id.as_deref() == Some(credential_id) {
                self.reset_privilege_draft_state();
            }
            self.privilege_scope_id = Some(scope_id);
            self.privilege_error = None;
        }
        cx.notify();
    }

    fn reset_privilege_draft_state(&mut self) {
        self.privilege_draft = PrivilegeCredentialDraft::default();
        self.privilege_error = None;
        self.privilege_editor_open = false;
        if self.settings_focused_input.is_some_and(is_privilege_input) {
            self.settings_focused_input = None;
        }
    }
}

fn is_privilege_input(input: SettingsInput) -> bool {
    matches!(
        input,
        SettingsInput::LocalPrivilegeLabel
            | SettingsInput::LocalPrivilegeUsernameHint
            | SettingsInput::LocalPrivilegeSecret
            | SettingsInput::LocalPrivilegePromptPatterns
            | SettingsInput::LocalPrivilegeHostPatterns
            | SettingsInput::LocalPrivilegeSecondFactorPatterns
            | SettingsInput::LocalPrivilegeSecondFactorUserHint
    )
}

fn optional_trimmed_privilege_value(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

impl WorkspaceApp {
    pub(in crate::workspace) fn open_privilege_credentials_settings(
        &mut self,
        scope_id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.set_active_tab(SettingsTab::Privilege, cx);
        });
        if let Some(scope_id) = scope_id {
            self.set_settings_privilege_scope(scope_id, cx);
        }
        self.open_settings(window, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn settings_privilege_credentials_section(
        &self,
        section_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match section_index {
            0 => self.settings_privilege_credentials_card(cx),
            1 => self.settings_totp_credentials_card(cx),
            _ => div().into_any_element(),
        }
    }

    pub(in crate::workspace) fn settings_privilege_prompt_type_label(
        &self,
        prompt_type: PrivilegePromptType,
    ) -> String {
        // The unified privilege page owns these labels so local settings and
        // connection editing do not grow separate credential-specific forms.
        let key = match prompt_type {
            PrivilegePromptType::Sudo => "sessionManager.privilege_credentials.prompt_type.sudo",
            PrivilegePromptType::Su => "sessionManager.privilege_credentials.prompt_type.su",
            PrivilegePromptType::Custom => {
                "sessionManager.privilege_credentials.prompt_type.custom"
            }
        };
        self.i18n.t(key)
    }

    pub(in crate::workspace) fn settings_privilege_response_mode_label(
        &self,
        response_mode: PrivilegeResponseMode,
    ) -> String {
        let key = match response_mode {
            PrivilegeResponseMode::PasswordOnly => {
                "sessionManager.privilege_credentials.response_mode.password_only"
            }
            PrivilegeResponseMode::PasswordThenTotp => {
                "sessionManager.privilege_credentials.response_mode.password_then_totp"
            }
            PrivilegeResponseMode::PasswordWithTotp => {
                "sessionManager.privilege_credentials.response_mode.password_with_totp"
            }
        };
        self.i18n.t(key)
    }

    pub(in crate::workspace) fn settings_privilege_credentials_card(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (selected_scope_id, editor_open) = {
            let settings = self.settings_workspace.read(cx);
            let (scope_id, _editing, editor_open, _error) = settings.privilege_layout_flags();
            (scope_id.map(str::to_string), editor_open)
        };
        let scopes = self.settings_privilege_scope_rows();
        let active_scope_id = self.settings_privilege_active_scope_id(selected_scope_id.as_deref());
        let active_scope = scopes
            .iter()
            .find(|scope| scope.id == active_scope_id)
            .cloned()
            .unwrap_or_else(|| self.settings_local_privilege_scope_row());
        let credentials = self
            .connection_store
            .list_privilege_credentials(&active_scope.id)
            .unwrap_or_default();
        let scope_scroll = self.selectable_text_scroll_handle("settings-privilege-scopes-scroll");

        let mut scope_list = div().flex().flex_col().gap(px(2.0));
        for scope in scopes {
            let selected = scope.id == active_scope.id;
            let scope_id = scope.id.clone();
            scope_list = scope_list.child(
                div()
                    .w_full()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(if selected {
                        rgba((theme.accent << 8) | 0x99)
                    } else {
                        rgba(theme.border << 8)
                    })
                    .bg(if selected {
                        rgba((theme.accent << 8) | 0x14)
                    } else {
                        rgba(theme.bg_panel << 8)
                    })
                    .px(px(10.0))
                    .py(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .cursor_pointer()
                    .hover(move |row| row.bg(rgba((theme.bg_hover << 8) | 0xcc)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.select_settings_privilege_scope(scope_id.clone(), cx);
                            cx.stop_propagation();
                        }),
                    )
                    .child(Self::render_lucide_icon(
                        if scope.local {
                            LucideIcon::Terminal
                        } else {
                            LucideIcon::Server
                        },
                        16.0,
                        if selected {
                            rgb(theme.accent)
                        } else {
                            rgb(theme.text_muted)
                        },
                    ))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text))
                                    .child(scope.title.clone()),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(theme.text_muted))
                                    .child(scope.subtitle.clone()),
                            ),
                    )
                    .child(
                        div()
                            .flex_none()
                            .min_w(px(20.0))
                            .text_align(gpui::TextAlign::Right)
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(scope.credential_count.to_string()),
                    ),
            );
        }

        let editor_visible = credentials.is_empty() || editor_open;
        let privilege =
            editor_visible.then(|| self.settings_workspace.read(cx).privilege_snapshot());

        let body = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_wrap()
            .gap(px(16.0))
            .items_start()
            .child(
                div()
                    .w(px(PRIVILEGE_SCOPE_LIST_WIDTH))
                    .max_w_full()
                    .min_w(px(0.0))
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(self.i18n.t("settings_view.privilege_credentials.scopes")),
                    )
                    .child(
                        div()
                            .id("settings-privilege-scopes")
                            .relative()
                            .w_full()
                            .max_h(px(PRIVILEGE_SCOPE_LIST_MAX_HEIGHT))
                            .child(
                                div()
                                    .id("settings-privilege-scopes-scroll")
                                    .w_full()
                                    .max_h(px(PRIVILEGE_SCOPE_LIST_MAX_HEIGHT))
                                    .selectable_overflow_y_scroll(&scope_scroll)
                                    .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                                    .child(scope_list),
                            )
                            .vertical_scrollbar(&scope_scroll),
                    ),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex_basis(px(PRIVILEGE_DETAIL_MIN_WIDTH))
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .child(self.settings_privilege_active_header(
                        &active_scope,
                        !editor_visible,
                        cx,
                    ))
                    .child(self.settings_privilege_credential_list(&active_scope, &credentials, cx))
                    .when(editor_visible, |detail| {
                        detail.child(self.settings_privilege_credential_form(
                            &active_scope,
                            !credentials.is_empty(),
                            privilege.as_ref().expect("visible privilege editor"),
                            cx,
                        ))
                    }),
            );

        self.settings_card(
            "settings_view.privilege_credentials.title",
            "settings_view.privilege_credentials.description",
            vec![body.into_any_element()],
        )
    }

    pub(in crate::workspace) fn settings_privilege_active_header(
        &self,
        scope: &SettingsPrivilegeScopeRow,
        show_new_action: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_base))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text_heading))
                            .child(scope.title.clone()),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(scope.subtitle.clone()),
                    ),
            )
            .when(show_new_action, |header| {
                header.child(
                    self.workspace_toolbar_action_button(
                        self.i18n.t("sessionManager.privilege_credentials.new"),
                        Some(
                            Self::render_lucide_icon(LucideIcon::Plus, 14.0, rgb(theme.text_muted))
                                .into_any_element(),
                        ),
                        ToolbarButtonOptions {
                            button: ButtonOptions {
                                variant: ButtonVariant::Ghost,
                                size: ButtonSize::Sm,
                                ..ButtonOptions::default()
                            },
                            ..ToolbarButtonOptions::default()
                        },
                        cx.listener(|this, _event, _window, cx| {
                            this.begin_new_settings_privilege_credential(cx);
                            cx.stop_propagation();
                        }),
                    ),
                )
            })
            .into_any_element()
    }

    pub(in crate::workspace) fn settings_privilege_credential_list(
        &self,
        scope: &SettingsPrivilegeScopeRow,
        credentials: &[SavedPrivilegeCredential],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mut list = div().w_full().min_w(px(0.0)).flex().flex_col();
        if credentials.is_empty() {
            return list
                .child(
                    div()
                        .py(px(4.0))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(theme.text_muted))
                        .child(self.i18n.t("sessionManager.privilege_credentials.empty")),
                )
                .into_any_element();
        }

        for credential in credentials.iter().cloned() {
            let edit_scope = scope.id.clone();
            let edit_credential = credential.clone();
            let delete_scope = scope.id.clone();
            let delete_id = credential.id.clone();
            list = list.child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.0))
                    .border_b_1()
                    .border_color(rgba((theme.border << 8) | 0x80))
                    .px(px(8.0))
                    .py(px(8.0))
                    .child(Self::render_lucide_icon(
                        LucideIcon::KeyRound,
                        16.0,
                        rgb(theme.warning),
                    ))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .text_color(rgb(theme.text))
                                    .child(credential.label.clone()),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(theme.text_muted))
                                    .child(format!(
                                        "{} · {}",
                                        self.settings_privilege_prompt_type_label(
                                            credential.prompt_type
                                        ),
                                        self.settings_privilege_response_mode_label(
                                            credential.response_mode
                                        )
                                    )),
                            ),
                    )
                    .when(!credential.enabled, |row| {
                        row.child(
                            div()
                                .rounded_full()
                                .px(px(8.0))
                                .py(px(2.0))
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(theme.text_muted))
                                .bg(rgba((theme.bg_hover << 8) | 0x99))
                                .child(self.i18n.t("settings_view.privilege_credentials.disabled")),
                        )
                    })
                    .child(self.workspace_toolbar_action_button(
                        self.i18n.t("sessionManager.privilege_credentials.edit"),
                        None,
                        ToolbarButtonOptions {
                            button: ButtonOptions {
                                variant: ButtonVariant::Ghost,
                                size: ButtonSize::Sm,
                                ..ButtonOptions::default()
                            },
                            ..ToolbarButtonOptions::default()
                        },
                        cx.listener(move |this, _event, _window, cx| {
                            this.edit_settings_privilege_credential(
                                edit_scope.clone(),
                                edit_credential.clone(),
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ))
                    .child(self.workspace_icon_action_button(
                        LucideIcon::Trash2,
                        14.0,
                        rgb(theme.text_muted),
                        IconButtonOptions {
                            hover_background: Some(rgb(theme.bg_hover)),
                            ..IconButtonOptions::opaque_toolbar(28.0, ButtonRadius::Sm)
                        },
                        move |this, _event, _window, cx| {
                            this.delete_settings_privilege_credential(
                                delete_scope.clone(),
                                delete_id.clone(),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    )),
            );
        }
        list.into_any_element()
    }

    pub(in crate::workspace) fn settings_privilege_credential_form(
        &self,
        scope: &SettingsPrivilegeScopeRow,
        show_cancel: bool,
        privilege: &PrivilegeCredentialSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let editing = privilege.credential_id.is_some();
        let field_slot = |field: AnyElement| {
            div()
                .min_w(px(0.0))
                .flex_1()
                .flex_basis(px(PRIVILEGE_FORM_FIELD_MIN_WIDTH))
                .child(field)
        };
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .border_t_1()
            .border_color(rgba((theme.border << 8) | 0x80))
            .pt(px(14.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text_heading))
                    .child(self.i18n.t(if editing {
                        "sessionManager.privilege_credentials.edit"
                    } else {
                        "sessionManager.privilege_credentials.new"
                    })),
            )
            // The paired fields use flexible slots so wide settings surfaces do
            // not leave an oversized empty panel while narrow ones wrap cleanly.
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .gap(px(12.0))
                    .child(field_slot(
                        self.settings_privilege_text_field(
                            "sessionManager.privilege_credentials.label",
                            SettingsInput::LocalPrivilegeLabel,
                            &privilege.label,
                            self.i18n
                                .t("sessionManager.privilege_credentials.label_placeholder"),
                            false,
                            cx,
                        ),
                    ))
                    .child(field_slot(self.settings_privilege_prompt_type_field(
                        privilege.prompt_type,
                        cx,
                    )))
                    .child(field_slot(self.settings_privilege_response_mode_field(
                        privilege.response_mode,
                        cx,
                    ))),
            )
            .when(scope.ssh_common, |panel| {
                panel
                    .child(
                        self.settings_privilege_host_patterns_field(&privilege.host_patterns, cx),
                    )
                    .child(
                        self.settings_privilege_hint(
                            self.i18n
                                .t("sessionManager.privilege_credentials.host_patterns_hint"),
                        ),
                    )
            })
            .child(self.settings_privilege_prompt_patterns_field(&privilege.prompt_patterns, cx))
            .child(
                self.settings_privilege_hint(
                    self.i18n
                        .t("sessionManager.privilege_credentials.prompt_patterns_hint"),
                ),
            )
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .gap(px(12.0))
                    .child(field_slot(self.settings_privilege_text_field(
                        "sessionManager.privilege_credentials.secret",
                        SettingsInput::LocalPrivilegeSecret,
                        "",
                        if editing {
                            self.i18n
                                .t("sessionManager.privilege_credentials.secret_keep_placeholder")
                        } else {
                            self.i18n
                                .t("sessionManager.privilege_credentials.secret_placeholder")
                        },
                        true,
                        cx,
                    )))
                    .child(field_slot(self.settings_privilege_text_field(
                        "sessionManager.privilege_credentials.username_hint",
                        SettingsInput::LocalPrivilegeUsernameHint,
                        &privilege.username_hint,
                        scope.username_placeholder.clone(),
                        false,
                        cx,
                    ))),
            )
            .when(privilege.response_mode.waits_for_totp(), |panel| {
                panel
                    .child(self.settings_privilege_second_factor_patterns_field(
                        &privilege.second_factor_patterns,
                        cx,
                    ))
                    .child(self.settings_privilege_hint(
                        self.i18n.t(
                            "sessionManager.privilege_credentials.second_factor_patterns_hint",
                        ),
                    ))
            })
            .when(privilege.response_mode.uses_totp(), |panel| {
                panel
                    .child(
                        div()
                            .w_full()
                            .min_w(px(0.0))
                            .flex()
                            .flex_wrap()
                            .gap(px(12.0))
                            .child(field_slot(self.settings_privilege_totp_field(
                                privilege.totp_credential_id.as_deref(),
                                cx,
                            )))
                            .when(privilege.response_mode.waits_for_totp(), |row| {
                                row.child(field_slot(self.settings_privilege_text_field(
                                    "sessionManager.privilege_credentials.second_factor_user_hint",
                                    SettingsInput::LocalPrivilegeSecondFactorUserHint,
                                    &privilege.second_factor_user_hint,
                                    self.i18n.t(
                                        "sessionManager.privilege_credentials.second_factor_user_hint_placeholder",
                                    ),
                                    false,
                                    cx,
                                )))
                            }),
                    )
                    .child(self.settings_privilege_hint(
                        self.i18n
                            .t("sessionManager.privilege_credentials.totp_hint"),
                    ))
                    .when(
                        privilege.response_mode == PrivilegeResponseMode::PasswordWithTotp,
                        |panel| {
                            panel.child(self.settings_privilege_hint(
                                self.i18n.t(
                                    "sessionManager.privilege_credentials.two_stage_combined_hint",
                                ),
                            ))
                        },
                    )
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.settings_workspace.update(cx, |settings, cx| {
                                settings.toggle_privilege_enabled(cx);
                            });
                            cx.stop_propagation();
                        }),
                    )
                    .child(checkbox(&self.tokens, "privilege-enabled", String::new(), privilege.enabled))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(theme.text))
                            .child(self.i18n.t("sessionManager.privilege_credentials.enabled")),
                    ),
            )
            .child(self.settings_privilege_auto_submit_row(privilege.require_click_to_send, cx))
            .child(
                self.settings_privilege_hint(
                    self.i18n
                        .t("sessionManager.privilege_credentials.auto_submit_hint"),
                ),
            )
            .when_some(privilege.error.clone(), |panel, error| {
                panel.child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(theme.error))
                        .child(error),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .justify_end()
                    .gap(px(8.0))
                    .when(show_cancel, |row| {
                        row.child(
                            self.workspace_toolbar_action_button(
                                self.i18n
                                    .t("sessionManager.privilege_credentials.cancel_edit"),
                                None,
                                ToolbarButtonOptions {
                                    button: ButtonOptions {
                                        variant: ButtonVariant::Ghost,
                                        size: ButtonSize::Sm,
                                        ..ButtonOptions::default()
                                    },
                                    ..ToolbarButtonOptions::default()
                                },
                                cx.listener(|this, _event, _window, cx| {
                                    this.reset_settings_privilege_credential_draft(cx);
                                    cx.stop_propagation();
                                }),
                            ),
                        )
                    })
                    .child(
                        self.workspace_toolbar_action_button(
                            self.i18n.t("sessionManager.privilege_credentials.save"),
                            Some(
                                Self::render_lucide_icon(
                                    LucideIcon::Save,
                                    14.0,
                                    rgb(theme.text_muted),
                                )
                                .into_any_element(),
                            ),
                            ToolbarButtonOptions {
                                button: ButtonOptions {
                                    variant: ButtonVariant::Default,
                                    size: ButtonSize::Sm,
                                    disabled: privilege.label.trim().is_empty(),
                                    ..ButtonOptions::default()
                                },
                                ..ToolbarButtonOptions::default()
                            },
                            cx.listener(|this, _event, _window, cx| {
                                this.save_settings_privilege_credential(cx);
                                cx.stop_propagation();
                            }),
                        ),
                    ),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn settings_privilege_text_field(
        &self,
        label_key: &'static str,
        input: SettingsInput,
        value: impl AsRef<str>,
        placeholder: String,
        secret: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        form_field(
            &self.tokens,
            self.i18n.t(label_key),
            if secret {
                // Entity-owned secret controls borrow their live value in the shared
                // input renderer so frame snapshots never duplicate credentials.
                self.settings_secret_text_input_control_fill(input, value, placeholder, cx)
            } else {
                self.settings_text_input_control_fill(input, value, placeholder, cx)
            },
        )
    }

    pub(in crate::workspace) fn settings_privilege_prompt_type_field(
        &self,
        prompt_type: PrivilegePromptType,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        form_field(
            &self.tokens,
            self.i18n
                .t("sessionManager.privilege_credentials.prompt_type_label"),
            self.settings_select_control(
                SettingsSelect::LocalPrivilegePromptType,
                self.settings_privilege_prompt_type_label(prompt_type),
                false,
                None,
                cx,
            ),
        )
    }

    pub(in crate::workspace) fn settings_privilege_response_mode_field(
        &self,
        response_mode: PrivilegeResponseMode,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        form_field(
            &self.tokens,
            self.i18n
                .t("sessionManager.privilege_credentials.response_mode_label"),
            self.settings_select_control(
                SettingsSelect::LocalPrivilegeResponseMode,
                self.settings_privilege_response_mode_label(response_mode),
                false,
                None,
                cx,
            ),
        )
    }

    pub(in crate::workspace) fn settings_privilege_totp_field(
        &self,
        selected: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        form_field(
            &self.tokens,
            self.i18n
                .t("sessionManager.privilege_credentials.totp_credential"),
            self.settings_select_control(
                SettingsSelect::LocalPrivilegeTotpCredential,
                self.settings_privilege_totp_label(selected),
                false,
                None,
                cx,
            ),
        )
    }

    pub(in crate::workspace) fn settings_privilege_totp_label(
        &self,
        selected: Option<&str>,
    ) -> String {
        selected
            .and_then(|id| {
                self.connection_store
                    .totp_credentials()
                    .iter()
                    .find(|credential| credential.id == id)
            })
            .map(|credential| {
                if credential.enabled {
                    credential.name.clone()
                } else {
                    format!(
                        "{} ({})",
                        credential.name,
                        self.i18n.t("settings_view.privilege_credentials.disabled")
                    )
                }
            })
            .unwrap_or_else(|| {
                self.i18n
                    .t("sessionManager.privilege_credentials.totp_none")
            })
    }

    pub(in crate::workspace) fn settings_privilege_auto_submit_row(
        &self,
        require_click_to_send: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.settings_workspace.update(cx, |settings, cx| {
                        settings.toggle_privilege_auto_submit(cx);
                    });
                    cx.stop_propagation();
                }),
            )
            .child(checkbox(
                &self.tokens,
                "privilege-auto-submit",
                String::new(),
                !require_click_to_send,
            ))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(
                        self.i18n
                            .t("sessionManager.privilege_credentials.auto_submit"),
                    ),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn settings_privilege_prompt_patterns_field(
        &self,
        value: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.settings_privilege_patterns_field(
            SettingsInput::LocalPrivilegePromptPatterns,
            value,
            "sessionManager.privilege_credentials.prompt_patterns",
            "sessionManager.privilege_credentials.prompt_patterns_placeholder",
            cx,
        )
    }

    pub(in crate::workspace) fn settings_privilege_host_patterns_field(
        &self,
        value: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.settings_privilege_patterns_field(
            SettingsInput::LocalPrivilegeHostPatterns,
            value,
            "sessionManager.privilege_credentials.host_patterns",
            "sessionManager.privilege_credentials.host_patterns_placeholder",
            cx,
        )
    }

    pub(in crate::workspace) fn settings_privilege_second_factor_patterns_field(
        &self,
        value: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.settings_privilege_patterns_field(
            SettingsInput::LocalPrivilegeSecondFactorPatterns,
            value,
            "sessionManager.privilege_credentials.second_factor_patterns",
            "sessionManager.privilege_credentials.second_factor_patterns_placeholder",
            cx,
        )
    }

    fn settings_privilege_patterns_field(
        &self,
        input: SettingsInput,
        value: &str,
        label_key: &'static str,
        placeholder_key: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = self
            .settings_workspace
            .read(cx)
            .settings_entity_focused_input()
            == Some(input);
        let target = WorkspaceImeTarget::Settings(input);
        let workspace = cx.entity();
        let theme = self.tokens.ui;
        let line_height = input.textarea_line_height();
        let placeholder = value.is_empty();
        let visible_value = if placeholder {
            self.i18n.t(placeholder_key)
        } else {
            value.to_string()
        };
        let mut textarea = div()
            .w_full()
            .min_h(px(80.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(if focused {
                rgba((theme.accent << 8) | 0x99)
            } else {
                rgb(theme.border)
            })
            .bg(rgb(theme.bg))
            .px(px(12.0))
            .py(px(8.0))
            .flex()
            .flex_col()
            .items_start()
            .cursor(CursorStyle::IBeam)
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .line_height(px(line_height))
            .text_color(rgb(theme.text))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    let current = this.current_settings_input_value(input, cx);
                    this.focus_settings_input(input, current, cx);
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle, cx);
                    this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                    this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                }),
            );
        textarea = self.render_settings_multiline_textarea_lines(
            textarea,
            target,
            &visible_value,
            placeholder,
            line_height,
            cx,
        );
        if let Some(marked) = self.marked_text_for_target(target, cx) {
            textarea = textarea.child(
                div()
                    .underline()
                    .text_color(rgb(theme.text))
                    .child(marked.to_string()),
            );
        }
        let control =
            text_input_anchor_probe(target.anchor_id(), textarea, move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            });
        form_field(
            &self.tokens,
            self.i18n.t(label_key),
            control.into_any_element(),
        )
    }

    pub(in crate::workspace) fn settings_privilege_hint(&self, text: String) -> AnyElement {
        div()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(text)
            .into_any_element()
    }

    pub(in crate::workspace) fn settings_privilege_scope_rows(
        &self,
    ) -> Vec<SettingsPrivilegeScopeRow> {
        let mut rows = vec![
            self.settings_local_privilege_scope_row(),
            SettingsPrivilegeScopeRow {
                id: SSH_COMMON_PRIVILEGE_SCOPE_ID.to_string(),
                title: self
                    .i18n
                    .t("settings_view.privilege_credentials.ssh_common_scope"),
                subtitle: self
                    .i18n
                    .t("settings_view.privilege_credentials.ssh_common_scope_hint"),
                username_placeholder: String::new(),
                credential_count: self
                    .connection_store
                    .list_privilege_credentials(SSH_COMMON_PRIVILEGE_SCOPE_ID)
                    .map(|credentials| credentials.len())
                    .unwrap_or(0),
                local: false,
                ssh_common: true,
            },
        ];
        // Saved SSH connections already own privilege credential metadata in the
        // connection store. Expose those explicit scopes instead of letting the
        // terminal helper infer remote secrets from host/title/runtime matches.
        rows.extend(
            self.connection_store
                .connections()
                .iter()
                .map(|connection| self.settings_saved_connection_privilege_scope_row(connection)),
        );
        rows
    }

    pub(in crate::workspace) fn settings_local_privilege_scope_row(
        &self,
    ) -> SettingsPrivilegeScopeRow {
        SettingsPrivilegeScopeRow {
            id: LOCAL_SHELL_PRIVILEGE_CONNECTION_ID.to_string(),
            title: self
                .i18n
                .t("settings_view.privilege_credentials.local_scope"),
            subtitle: self
                .i18n
                .t("settings_view.privilege_credentials.local_scope_hint"),
            username_placeholder: self.settings_local_username_hint().unwrap_or_else(|| {
                self.i18n
                    .t("settings_view.local_terminal.privilege_username_placeholder")
            }),
            credential_count: self
                .connection_store
                .list_privilege_credentials(LOCAL_SHELL_PRIVILEGE_CONNECTION_ID)
                .map(|credentials| credentials.len())
                .unwrap_or(0),
            local: true,
            ssh_common: false,
        }
    }

    pub(in crate::workspace) fn settings_saved_connection_privilege_scope_row(
        &self,
        connection: &oxideterm_connections::SavedConnection,
    ) -> SettingsPrivilegeScopeRow {
        SettingsPrivilegeScopeRow {
            id: connection.id.clone(),
            title: connection.name.clone(),
            subtitle: format!(
                "{}@{}:{}",
                connection.username, connection.host, connection.port
            ),
            username_placeholder: connection.username.clone(),
            credential_count: connection.privilege_credentials.len(),
            local: false,
            ssh_common: false,
        }
    }

    pub(in crate::workspace) fn settings_privilege_active_scope_id(
        &self,
        selected: Option<&str>,
    ) -> String {
        if let Some(selected) = selected
            && self
                .settings_privilege_scope_rows()
                .iter()
                .any(|scope| scope.id == selected)
        {
            return selected.to_string();
        }
        LOCAL_SHELL_PRIVILEGE_CONNECTION_ID.to_string()
    }

    pub(in crate::workspace) fn set_settings_privilege_scope(
        &mut self,
        scope_id: String,
        cx: &mut Context<Self>,
    ) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.set_privilege_scope(scope_id, cx);
        });
        self.close_settings_select();
    }

    pub(in crate::workspace) fn select_settings_privilege_scope(
        &mut self,
        scope_id: String,
        cx: &mut Context<Self>,
    ) {
        let selected_scope = self
            .settings_workspace
            .read(cx)
            .privilege_layout_flags()
            .0
            .map(str::to_string);
        if self.settings_privilege_active_scope_id(selected_scope.as_deref()) != scope_id {
            self.set_settings_privilege_scope(scope_id, cx);
        }
        cx.notify();
    }

    pub(in crate::workspace) fn reset_settings_privilege_credential_draft(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.reset_privilege_draft(cx);
        });
        self.close_settings_select();
    }

    pub(in crate::workspace) fn begin_new_settings_privilege_credential(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.begin_new_privilege_credential(cx);
        });
        self.close_settings_select();
    }

    pub(in crate::workspace) fn edit_settings_privilege_credential(
        &mut self,
        scope_id: String,
        credential: SavedPrivilegeCredential,
        cx: &mut Context<Self>,
    ) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.edit_privilege_credential(scope_id, credential, cx);
        });
        self.close_settings_select();
    }

    pub(in crate::workspace) fn save_settings_privilege_credential(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let selected_scope = self
            .settings_workspace
            .read(cx)
            .privilege_layout_flags()
            .0
            .map(str::to_string);
        let scope_id = self.settings_privilege_active_scope_id(selected_scope.as_deref());
        let snapshot = self.settings_workspace.read(cx).privilege_snapshot();
        if scope_id == SSH_COMMON_PRIVILEGE_SCOPE_ID
            && !snapshot.require_click_to_send
            && snapshot.host_patterns.trim().is_empty()
        {
            let error = self
                .i18n
                .t("sessionManager.privilege_credentials.auto_requires_host_patterns");
            self.settings_workspace.update(cx, |settings, cx| {
                settings.finish_privilege_save(scope_id, Some(error), cx);
            });
            self.close_settings_select();
            return;
        }
        if snapshot.response_mode.uses_totp() && snapshot.totp_credential_id.is_none() {
            let error = self
                .i18n
                .t("sessionManager.privilege_credentials.two_stage_requires_totp");
            self.settings_workspace.update(cx, |settings, cx| {
                settings.finish_privilege_save(scope_id, Some(error), cx);
            });
            self.close_settings_select();
            return;
        }
        let Some(request) = self.settings_workspace.update(cx, |settings, _cx| {
            settings.take_privilege_save_request(scope_id.clone())
        }) else {
            return;
        };
        match self.connection_store.save_privilege_credential(request) {
            Ok(_) => {
                self.settings_workspace.update(cx, |settings, cx| {
                    settings.finish_privilege_save(scope_id, None, cx);
                });
                self.queue_cloud_sync_dirty_refresh(cx);
            }
            Err(error) => {
                self.settings_workspace.update(cx, |settings, cx| {
                    settings.finish_privilege_save(scope_id, Some(error.to_string()), cx);
                });
            }
        }
        self.close_settings_select();
    }

    pub(in crate::workspace) fn delete_settings_privilege_credential(
        &mut self,
        scope_id: String,
        credential_id: String,
        cx: &mut Context<Self>,
    ) {
        match self
            .connection_store
            .delete_privilege_credential(&scope_id, &credential_id)
        {
            Ok(_) => {
                self.settings_workspace.update(cx, |settings, cx| {
                    settings.finish_privilege_delete(scope_id, &credential_id, None, cx);
                });
                self.queue_cloud_sync_dirty_refresh(cx);
            }
            Err(error) => {
                self.settings_workspace.update(cx, |settings, cx| {
                    settings.finish_privilege_delete(
                        scope_id,
                        &credential_id,
                        Some(error.to_string()),
                        cx,
                    );
                });
            }
        }
        self.close_settings_select();
    }

    pub(in crate::workspace) fn settings_local_username_hint(&self) -> Option<String> {
        std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use gpui::{AppContext, TestAppContext};

    use super::*;

    #[gpui::test]
    fn privilege_save_request_includes_host_group_and_second_factor(cx: &mut TestAppContext) {
        let settings = cx.new(SettingsWorkspaceEntity::new);
        settings.update(cx, |settings, cx| {
            settings.begin_new_privilege_credential(cx);
            settings.privilege_draft.label = "group sudo".to_string();
            settings.privilege_draft.prompt_type = PrivilegePromptType::Sudo;
            settings.privilege_draft.response_mode = PrivilegeResponseMode::PasswordThenTotp;
            settings.privilege_draft.host_patterns = " hktkosl* \n\ni-* ".to_string();
            settings.privilege_draft.second_factor_patterns =
                " Second Factor: \nSecond factor： ".to_string();
            settings.privilege_draft.second_factor_user_hint = " deploy ".to_string();
            settings.privilege_draft.totp_credential_id = Some("totp-1".to_string());
            settings.privilege_draft.require_click_to_send = false;

            let request = settings
                .take_privilege_save_request(SSH_COMMON_PRIVILEGE_SCOPE_ID.to_string())
                .expect("privilege save request");
            assert_eq!(
                request.host_patterns,
                vec!["hktkosl*".to_string(), "i-*".to_string()]
            );
            assert_eq!(
                request.second_factor_patterns,
                vec!["Second Factor:".to_string(), "Second factor：".to_string()]
            );
            assert_eq!(request.prompt_type, PrivilegePromptType::Sudo);
            assert_eq!(
                request.response_mode,
                PrivilegeResponseMode::PasswordThenTotp
            );
            assert_eq!(request.second_factor_user_hint.as_deref(), Some("deploy"));
            assert_eq!(request.totp_credential_id.as_deref(), Some("totp-1"));
            assert!(!request.require_click_to_send);
        });
    }

    #[gpui::test]
    fn privilege_save_request_moves_secret_and_keeps_nonsecret_retry_state(
        cx: &mut TestAppContext,
    ) {
        let settings = cx.new(SettingsWorkspaceEntity::new);
        settings.update(cx, |settings, cx| {
            settings.begin_new_privilege_credential(cx);
            assert!(settings.replace_settings_entity_input(
                SettingsInput::LocalPrivilegeLabel,
                None,
                "Local sudo",
                cx,
            ));
            assert!(settings.focus_settings_entity_input(SettingsInput::LocalPrivilegeSecret, cx));
            assert!(settings.replace_settings_entity_input(
                SettingsInput::LocalPrivilegeSecret,
                None,
                "privilege-secret",
                cx,
            ));

            let request = settings
                .take_privilege_save_request(LOCAL_SHELL_PRIVILEGE_CONNECTION_ID.to_string())
                .expect("privilege save request");
            assert_eq!(
                request.secret.as_ref().map(SecretString::expose_secret),
                Some("privilege-secret")
            );
            assert!(settings.privilege_draft.secret.is_empty());
            assert_eq!(settings.privilege_draft.label, "Local sudo");

            settings.finish_privilege_save(
                LOCAL_SHELL_PRIVILEGE_CONNECTION_ID.to_string(),
                Some("retry".to_string()),
                cx,
            );
            assert_eq!(settings.privilege_error.as_deref(), Some("retry"));
            assert_eq!(settings.privilege_draft.label, "Local sudo");

            settings.finish_privilege_save(
                LOCAL_SHELL_PRIVILEGE_CONNECTION_ID.to_string(),
                None,
                cx,
            );
            assert!(settings.privilege_draft.label.is_empty());
            assert!(settings.privilege_draft.secret.is_empty());
            assert!(!settings.privilege_editor_open);
        });
    }
}
