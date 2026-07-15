use super::*;

pub(in crate::workspace) const PRIVILEGE_SCOPE_LIST_WIDTH: f32 = 280.0; // Match the current scope rail width on comfortable layouts.
pub(in crate::workspace) const PRIVILEGE_DETAIL_MIN_WIDTH: f32 = 320.0; // Wrap the detail pane before fixed controls crush its labels.
pub(in crate::workspace) const PRIVILEGE_FORM_FIELD_MIN_WIDTH: f32 = 260.0; // Keep paired fields readable before the form wraps to one column.

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) struct SettingsPrivilegeScopeRow {
    id: String,
    title: String,
    subtitle: String,
    username_placeholder: String,
    credential_count: usize,
    local: bool,
}

impl WorkspaceApp {
    pub(in crate::workspace) fn open_privilege_credentials_settings(
        &mut self,
        scope_id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings_page.set_active_tab(SettingsTab::Privilege);
        if let Some(scope_id) = scope_id {
            self.set_settings_privilege_scope(scope_id);
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
            _ => div().into_any_element(),
        }
    }

    pub(in crate::workspace) fn settings_privilege_kind_label(
        &self,
        kind: PrivilegeCredentialKind,
    ) -> String {
        // The unified privilege page owns these labels so local settings and
        // connection editing do not grow separate credential-specific forms.
        let key = match kind {
            PrivilegeCredentialKind::SudoPassword => {
                "sessionManager.privilege_credentials.kind.sudo_password"
            }
            PrivilegeCredentialKind::SuPassword => {
                "sessionManager.privilege_credentials.kind.su_password"
            }
            PrivilegeCredentialKind::CustomPrompt => {
                "sessionManager.privilege_credentials.kind.custom_prompt"
            }
        };
        self.i18n.t(key)
    }

    pub(in crate::workspace) fn settings_privilege_credentials_card(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let scopes = self.settings_privilege_scope_rows();
        let active_scope_id = self.settings_privilege_active_scope_id();
        let active_scope = scopes
            .iter()
            .find(|scope| scope.id == active_scope_id)
            .cloned()
            .unwrap_or_else(|| self.settings_local_privilege_scope_row());
        let credentials = self
            .connection_store
            .list_privilege_credentials(&active_scope.id)
            .unwrap_or_default();

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
                        rgba((theme.border << 8) | 0x00)
                    })
                    .bg(if selected {
                        rgba((theme.accent << 8) | 0x14)
                    } else {
                        rgba((theme.bg_panel << 8) | 0x00)
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

        let editor_visible = credentials.is_empty() || self.settings_privilege_editor_open;

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
                    .child(scope_list),
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
                                    .child(self.settings_privilege_kind_label(credential.kind)),
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
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let editing = self.settings_local_privilege_draft.credential_id.is_some();
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
                            self.settings_local_privilege_draft.label.clone(),
                            self.i18n
                                .t("sessionManager.privilege_credentials.label_placeholder"),
                            false,
                            cx,
                        ),
                    ))
                    .child(field_slot(self.settings_privilege_kind_field(cx)))
                    .child(field_slot(self.settings_privilege_text_field(
                        "sessionManager.privilege_credentials.username_hint",
                        SettingsInput::LocalPrivilegeUsernameHint,
                        self.settings_local_privilege_draft.username_hint.clone(),
                        scope.username_placeholder.clone(),
                        false,
                        cx,
                    )))
                    .child(field_slot(self.settings_privilege_text_field(
                        "sessionManager.privilege_credentials.secret",
                        SettingsInput::LocalPrivilegeSecret,
                        self.settings_local_privilege_draft.secret.clone(),
                        if editing {
                            self.i18n
                                .t("sessionManager.privilege_credentials.secret_keep_placeholder")
                        } else {
                            self.i18n
                                .t("sessionManager.privilege_credentials.secret_placeholder")
                        },
                        true,
                        cx,
                    ))),
            )
            .child(self.settings_privilege_prompt_patterns_field(cx))
            .child(
                self.settings_privilege_hint(
                    self.i18n
                        .t("sessionManager.privilege_credentials.prompt_patterns_hint"),
                ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.settings_local_privilege_draft.enabled =
                                !this.settings_local_privilege_draft.enabled;
                            this.settings_local_privilege_error = None;
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .child(checkbox(
                        &self.tokens,
                        String::new(),
                        self.settings_local_privilege_draft.enabled,
                    ))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(theme.text))
                            .child(self.i18n.t("sessionManager.privilege_credentials.enabled")),
                    ),
            )
            .when_some(
                self.settings_local_privilege_error.clone(),
                |panel, error| {
                    panel.child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.error))
                            .child(error),
                    )
                },
            )
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
                                    disabled: self
                                        .settings_local_privilege_draft
                                        .label
                                        .trim()
                                        .is_empty(),
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
        value: String,
        placeholder: String,
        secret: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        form_field(
            &self.tokens,
            self.i18n.t(label_key),
            if secret {
                self.settings_secret_text_input_control_fill(input, value, placeholder, cx)
            } else {
                self.settings_text_input_control_fill(input, value, placeholder, cx)
            },
        )
    }

    pub(in crate::workspace) fn settings_privilege_kind_field(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        form_field(
            &self.tokens,
            self.i18n
                .t("sessionManager.privilege_credentials.kind_label"),
            self.settings_select_control(
                SettingsSelect::LocalPrivilegeKind,
                self.settings_privilege_kind_label(self.settings_local_privilege_draft.kind),
                false,
                None,
                cx,
            ),
        )
    }

    pub(in crate::workspace) fn settings_privilege_prompt_patterns_field(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input = SettingsInput::LocalPrivilegePromptPatterns;
        let focused = self.focused_settings_input == Some(input);
        let value = if focused {
            self.settings_input_draft.clone()
        } else {
            self.settings_local_privilege_draft.prompt_patterns.clone()
        };
        let target = WorkspaceImeTarget::Settings(input);
        let workspace = cx.entity();
        let theme = self.tokens.ui;
        let line_height = input.textarea_line_height();
        let placeholder = value.is_empty();
        let visible_value = if placeholder {
            self.i18n
                .t("sessionManager.privilege_credentials.prompt_patterns_placeholder")
        } else {
            value
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
                    let current = this.current_settings_input_value(input);
                    this.focus_settings_input(input, current, cx);
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle);
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
        );
        if let Some(marked) = self.marked_text_for_target(target) {
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
            self.i18n
                .t("sessionManager.privilege_credentials.prompt_patterns"),
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
        let mut rows = vec![self.settings_local_privilege_scope_row()];
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
        }
    }

    pub(in crate::workspace) fn settings_privilege_active_scope_id(&self) -> String {
        let selected = self.settings_page.privilege_scope_id.as_deref();
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

    pub(in crate::workspace) fn set_settings_privilege_scope(&mut self, scope_id: String) {
        zeroize::Zeroize::zeroize(&mut self.settings_local_privilege_draft.secret);
        self.settings_local_privilege_draft = PrivilegeCredentialDraft::default();
        self.settings_local_privilege_error = None;
        self.focused_settings_input = None;
        self.settings_input_draft.clear();
        self.close_settings_select();
        self.settings_page.privilege_scope_id = Some(scope_id);
        self.settings_privilege_editor_open = false;
    }

    pub(in crate::workspace) fn select_settings_privilege_scope(
        &mut self,
        scope_id: String,
        cx: &mut Context<Self>,
    ) {
        if self.settings_privilege_active_scope_id() != scope_id {
            self.set_settings_privilege_scope(scope_id);
        }
        cx.notify();
    }

    pub(in crate::workspace) fn reset_settings_privilege_credential_draft(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        zeroize::Zeroize::zeroize(&mut self.settings_local_privilege_draft.secret);
        self.settings_local_privilege_draft = PrivilegeCredentialDraft::default();
        self.settings_local_privilege_error = None;
        self.focused_settings_input = None;
        self.settings_input_draft.clear();
        self.close_settings_select();
        self.settings_privilege_editor_open = false;
        cx.notify();
    }

    pub(in crate::workspace) fn begin_new_settings_privilege_credential(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        // Starting a fresh editor must clear every secret-bearing draft value
        // before the new form receives focus.
        zeroize::Zeroize::zeroize(&mut self.settings_local_privilege_draft.secret);
        self.settings_local_privilege_draft = PrivilegeCredentialDraft::default();
        self.settings_local_privilege_error = None;
        self.settings_privilege_editor_open = true;
        self.focused_settings_input = Some(SettingsInput::LocalPrivilegeLabel);
        self.settings_input_draft.clear();
        self.close_settings_select();
        cx.notify();
    }

    pub(in crate::workspace) fn edit_settings_privilege_credential(
        &mut self,
        scope_id: String,
        credential: SavedPrivilegeCredential,
        cx: &mut Context<Self>,
    ) {
        self.set_settings_privilege_scope(scope_id);
        self.settings_local_privilege_draft.credential_id = Some(credential.id);
        self.settings_local_privilege_draft.label = credential.label;
        self.settings_local_privilege_draft.kind = credential.kind;
        self.settings_local_privilege_draft.username_hint =
            credential.username_hint.unwrap_or_default();
        self.settings_local_privilege_draft.prompt_patterns = credential.prompt_patterns.join("\n");
        self.settings_local_privilege_draft.secret.clear();
        self.settings_local_privilege_draft.enabled = credential.enabled;
        self.settings_local_privilege_error = None;
        self.settings_privilege_editor_open = true;
        self.focused_settings_input = Some(SettingsInput::LocalPrivilegeLabel);
        self.settings_input_draft = self.settings_local_privilege_draft.label.clone();
        self.close_settings_select();
        cx.notify();
    }

    pub(in crate::workspace) fn save_settings_privilege_credential(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let scope_id = self.settings_privilege_active_scope_id();
        let draft = &self.settings_local_privilege_draft;
        let label = draft.label.trim().to_string();
        if label.is_empty() {
            return;
        }
        let prompt_patterns = draft
            .prompt_patterns
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        // The settings draft must stay editable for validation failure. Only
        // the one temporary password copy that crosses into the connection
        // store is wrapped for zeroization and then persisted in the dedicated
        // privilege keychain scope.
        let secret = (!draft.secret.is_empty())
            .then(|| SecretString::from(zeroize::Zeroizing::new(draft.secret.clone())));
        let request = SavePrivilegeCredentialRequest {
            connection_id: scope_id.clone(),
            credential_id: draft.credential_id.clone(),
            label,
            kind: draft.kind,
            username_hint: draft
                .username_hint
                .trim()
                .is_empty()
                .then_some(None)
                .unwrap_or_else(|| Some(draft.username_hint.trim().to_string())),
            prompt_patterns,
            secret,
            enabled: draft.enabled,
            require_click_to_send: true,
        };
        match self.connection_store.save_privilege_credential(request) {
            Ok(_) => {
                zeroize::Zeroize::zeroize(&mut self.settings_local_privilege_draft.secret);
                self.settings_local_privilege_draft = PrivilegeCredentialDraft::default();
                self.settings_local_privilege_error = None;
                self.settings_privilege_editor_open = false;
                self.focused_settings_input = None;
                self.settings_input_draft.clear();
                self.settings_page.privilege_scope_id = Some(scope_id);
                self.queue_cloud_sync_dirty_refresh(cx);
            }
            Err(error) => {
                self.settings_local_privilege_error = Some(error.to_string());
            }
        }
        self.close_settings_select();
        cx.notify();
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
                if self.settings_local_privilege_draft.credential_id.as_deref()
                    == Some(credential_id.as_str())
                {
                    zeroize::Zeroize::zeroize(&mut self.settings_local_privilege_draft.secret);
                    self.settings_local_privilege_draft = PrivilegeCredentialDraft::default();
                    self.settings_privilege_editor_open = false;
                }
                self.settings_page.privilege_scope_id = Some(scope_id);
                self.settings_local_privilege_error = None;
                self.queue_cloud_sync_dirty_refresh(cx);
            }
            Err(error) => {
                self.settings_local_privilege_error = Some(error.to_string());
            }
        }
        self.close_settings_select();
        cx.notify();
    }

    pub(in crate::workspace) fn settings_local_username_hint(&self) -> Option<String> {
        std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }
}
