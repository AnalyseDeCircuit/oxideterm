impl WorkspaceApp {
    fn settings_local_section(&self, section_index: usize, cx: &mut Context<Self>) -> AnyElement {
        let settings = self.settings_store.settings();
        match section_index {
            0 => {
                let mut shell_rows = vec![self.local_shell_select_row(settings, cx)];
                if let Some(path_hint) = self.local_shell_path_hint(settings) {
                    shell_rows.push(path_hint);
                }
                shell_rows.push(self.card_separator());
                shell_rows.push(
                    self.setting_row(
                        "settings_view.local_terminal.git_bash_path",
                        "settings_view.local_terminal.git_bash_path_hint",
                        self.settings_text_input_control(
                            SettingsInput::LocalGitBashPath,
                            settings
                                .local_terminal
                                .git_bash_path
                                .clone()
                                .unwrap_or_default(),
                            self.i18n
                                .t("settings_view.local_terminal.git_bash_path_placeholder"),
                            300.0,
                            cx,
                        ),
                        cx,
                    ),
                );
                shell_rows.push(self.card_separator());
                shell_rows.push(
                    self.setting_row(
                        "settings_view.local_terminal.default_cwd",
                        "settings_view.local_terminal.default_cwd_hint",
                        self.settings_text_input_control(
                            SettingsInput::LocalDefaultCwd,
                            settings
                                .local_terminal
                                .default_cwd
                                .clone()
                                .unwrap_or_default(),
                            "~".to_string(),
                            self.tokens.metrics.settings_select_width,
                            cx,
                        ),
                        cx,
                    ),
                );
                self.settings_card(
                "settings_view.local_terminal.shell",
                "settings_view.local_terminal.default_shell_hint",
                shell_rows,
                )
            }
            1 => self.settings_card(
                "settings_view.local_terminal.shell_profile",
                "settings_view.local_terminal.load_shell_profile_hint",
                vec![self.checkbox_row(
                    "settings_view.local_terminal.load_shell_profile",
                    "settings_view.local_terminal.load_shell_profile_hint",
                    settings.local_terminal.load_shell_profile,
                    set_load_shell_profile,
                    cx,
                )],
            ),
            2 => self.local_privilege_credentials_card(cx),
            3 => {
                let mut oh_my_posh_rows = vec![self.checkbox_row(
                    "settings_view.local_terminal.oh_my_posh_enable",
                    "settings_view.local_terminal.oh_my_posh_enable_hint",
                    settings.local_terminal.oh_my_posh_enabled,
                    set_oh_my_posh,
                    cx,
                )];
                if settings.local_terminal.oh_my_posh_enabled {
                    oh_my_posh_rows.push(
                        div()
                            .px(px(12.0))
                            .py(px(8.0))
                            .rounded(px(self.tokens.radii.sm))
                            .border_1()
                            .border_color(rgba((self.tokens.ui.info << 8) | 0x33))
                            .bg(rgba((self.tokens.ui.info << 8) | 0x1a))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(self.tokens.ui.info))
                                    .child(format!(
                                        "💡 {}",
                                        self.i18n
                                            .t("settings_view.local_terminal.oh_my_posh_note")
                                    )),
                            )
                            .into_any_element(),
                    );
                    oh_my_posh_rows.push(self.card_separator());
                    oh_my_posh_rows.push(
                        self.setting_row(
                            "settings_view.local_terminal.oh_my_posh_theme",
                            "settings_view.local_terminal.oh_my_posh_theme_hint",
                            self.settings_text_input_control(
                                SettingsInput::LocalOhMyPoshTheme,
                                settings
                                    .local_terminal
                                    .oh_my_posh_theme
                                    .clone()
                                    .unwrap_or_default(),
                                self.i18n
                                    .t("settings_view.local_terminal.oh_my_posh_theme_placeholder"),
                                300.0,
                                cx,
                            ),
                            cx,
                        ),
                    );
                }
                self.settings_card(
                "settings_view.local_terminal.oh_my_posh",
                "settings_view.local_terminal.oh_my_posh_note",
                oh_my_posh_rows,
                )
            }
            4 => {
                let shortcut_default = if cfg!(target_os = "macos") {
                    "⌘T"
                } else {
                    "Ctrl+T"
                };
                let shortcut_launcher = if cfg!(target_os = "macos") {
                    "⌘⇧T"
                } else {
                    "Ctrl+Shift+T"
                };
                self.settings_card(
                "settings_view.local_terminal.shortcuts",
                "settings_view.local_terminal.custom_env_hint",
                vec![
                    self.local_shortcut_row(
                        "settings_view.local_terminal.new_default_shell",
                        shortcut_default,
                    ),
                    self.card_separator(),
                    self.local_shortcut_row(
                        "settings_view.local_terminal.new_shell_launcher",
                        shortcut_launcher,
                    ),
                ],
                )
            }
            5 => {
                let effective_shells = self.effective_local_shells_for_settings(settings);
                let shell_list = if effective_shells.is_empty() {
                    vec![
                        div()
                            .text_align(gpui::TextAlign::Center)
                            .py(px(32.0))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("settings_view.local_terminal.loading_shells"))
                            .into_any_element(),
                    ]
                } else {
                    effective_shells
                        .iter()
                        .map(|shell| {
                            self.available_shell_row(
                                shell,
                                settings.local_terminal.default_shell_id.as_deref(),
                            )
                        })
                        .collect()
                };
                self.settings_card(
                "settings_view.local_terminal.available_shells",
                "settings_view.local_terminal.select_shell",
                shell_list,
                )
            }
            _ => div().into_any_element(),
        }
    }

    fn reset_local_privilege_credential_draft(&mut self, cx: &mut Context<Self>) {
        // UI drafts are ordinary strings for text editing; clear them explicitly
        // when leaving an edit operation so stale secrets do not remain in the
        // settings page state.
        zeroize::Zeroize::zeroize(&mut self.settings_local_privilege_draft.secret);
        self.settings_local_privilege_draft = PrivilegeCredentialDraft::default();
        self.settings_local_privilege_error = None;
        self.focused_settings_input = None;
        self.settings_input_draft.clear();
        self.close_settings_select();
        cx.notify();
    }

    fn edit_local_privilege_credential(
        &mut self,
        credential: SavedPrivilegeCredential,
        cx: &mut Context<Self>,
    ) {
        zeroize::Zeroize::zeroize(&mut self.settings_local_privilege_draft.secret);
        self.settings_local_privilege_draft.credential_id = Some(credential.id);
        self.settings_local_privilege_draft.label = credential.label;
        self.settings_local_privilege_draft.kind = credential.kind;
        self.settings_local_privilege_draft.username_hint =
            credential.username_hint.unwrap_or_default();
        self.settings_local_privilege_draft.prompt_patterns =
            credential.prompt_patterns.join("\n");
        self.settings_local_privilege_draft.secret.clear();
        self.settings_local_privilege_draft.enabled = credential.enabled;
        self.settings_local_privilege_error = None;
        self.focused_settings_input = Some(SettingsInput::LocalPrivilegeLabel);
        self.settings_input_draft = self.settings_local_privilege_draft.label.clone();
        self.close_settings_select();
        cx.notify();
    }

    fn save_local_privilege_credential(&mut self, cx: &mut Context<Self>) {
        let draft = self.settings_local_privilege_draft.clone();
        let label = draft.label.trim().to_string();
        if label.is_empty() {
            return;
        }
        let prompt_patterns = draft
            .prompt_patterns
            .lines()
            .map(str::trim)
            .filter(|line: &&str| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        // Local shell credentials are scoped to the app/device, not an SSH
        // SavedConnection. Store only metadata in config; cleartext crosses
        // this boundary once and is then owned by the privilege keychain layer.
        let secret = (!draft.secret.is_empty())
            .then(|| SecretString::from(zeroize::Zeroizing::new(draft.secret.clone())));
        let request = SavePrivilegeCredentialRequest {
            connection_id: LOCAL_SHELL_PRIVILEGE_CONNECTION_ID.to_string(),
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
                self.focused_settings_input = None;
                self.settings_input_draft.clear();
                self.queue_cloud_sync_dirty_refresh(cx);
            }
            Err(error) => {
                self.settings_local_privilege_error = Some(error.to_string());
            }
        }
        self.close_settings_select();
        cx.notify();
    }

    fn delete_local_privilege_credential(&mut self, credential_id: String, cx: &mut Context<Self>) {
        match self.connection_store.delete_privilege_credential(
            LOCAL_SHELL_PRIVILEGE_CONNECTION_ID,
            &credential_id,
        ) {
            Ok(_) => {
                if self.settings_local_privilege_draft.credential_id.as_deref()
                    == Some(credential_id.as_str())
                {
                    zeroize::Zeroize::zeroize(&mut self.settings_local_privilege_draft.secret);
                    self.settings_local_privilege_draft = PrivilegeCredentialDraft::default();
                }
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

    fn local_privilege_credentials_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let credentials = self
            .connection_store
            .list_privilege_credentials(LOCAL_SHELL_PRIVILEGE_CONNECTION_ID)
            .unwrap_or_default();
        let mut list = div().flex().flex_col().gap(px(8.0));
        if credentials.is_empty() {
            list = list.child(
                div()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgba((theme.border << 8) | 0x80))
                    .px(px(12.0))
                    .py(px(8.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t("sessionManager.privilege_credentials.empty")),
            );
        } else {
            for credential in credentials.iter().cloned() {
                let edit_credential = credential.clone();
                let delete_id = credential.id.clone();
                list = list.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .rounded(px(self.tokens.radii.md))
                        .border_1()
                        .border_color(rgba((theme.border << 8) | 0x80))
                        .bg(rgba((theme.bg_panel << 8) | 0x4d))
                        .px(px(8.0))
                        .py(px(6.0))
                        .child(Self::render_lucide_icon(
                            LucideIcon::KeyRound,
                            16.0,
                            rgba(0xfde68aff),
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
                                this.edit_local_privilege_credential(edit_credential.clone(), cx);
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
                                this.delete_local_privilege_credential(delete_id.clone(), cx);
                                cx.stop_propagation();
                            },
                            cx,
                        )),
                );
            }
        }

        let form = div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((theme.border << 8) | 0x80))
            .bg(rgba((theme.bg << 8) | 0x80))
            .p(px(12.0))
            .child(self.local_privilege_text_field(
                "sessionManager.privilege_credentials.label",
                SettingsInput::LocalPrivilegeLabel,
                self.settings_local_privilege_draft.label.clone(),
                "sessionManager.privilege_credentials.label_placeholder",
                false,
                cx,
            ))
            .child(
                div()
                    .grid()
                    .grid_cols(2)
                    .gap(px(12.0))
                    .child(self.local_privilege_kind_field(cx))
                    .child(self.local_privilege_text_field(
                        "sessionManager.privilege_credentials.username_hint",
                        SettingsInput::LocalPrivilegeUsernameHint,
                        self.settings_local_privilege_draft.username_hint.clone(),
                        "settings_view.local_terminal.privilege_username_placeholder",
                        false,
                        cx,
                    )),
            )
            .child(self.local_privilege_text_field(
                "sessionManager.privilege_credentials.secret",
                SettingsInput::LocalPrivilegeSecret,
                self.settings_local_privilege_draft.secret.clone(),
                if self
                    .settings_local_privilege_draft
                    .credential_id
                    .is_some()
                {
                    "sessionManager.privilege_credentials.secret_keep_placeholder"
                } else {
                    "sessionManager.privilege_credentials.secret_placeholder"
                },
                true,
                cx,
            ))
            .child(self.local_privilege_prompt_patterns_field(cx))
            .child(self.local_privilege_hint(
                self.i18n
                    .t("sessionManager.privilege_credentials.prompt_patterns_hint"),
            ))
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
            .when_some(self.settings_local_privilege_error.clone(), |panel, error| {
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
                    .justify_end()
                    .gap(px(8.0))
                    .when(
                        self.settings_local_privilege_draft.credential_id.is_some(),
                        |row| {
                            row.child(self.workspace_toolbar_action_button(
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
                                    this.reset_local_privilege_credential_draft(cx);
                                    cx.stop_propagation();
                                }),
                            ))
                        },
                    )
                    .child(self.workspace_toolbar_action_button(
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
                            this.save_local_privilege_credential(cx);
                            cx.stop_propagation();
                        }),
                    )),
            );

        self.settings_card(
            "settings_view.local_terminal.privilege_credentials",
            "settings_view.local_terminal.privilege_credentials_hint",
            vec![list.into_any_element(), form.into_any_element()],
        )
    }

    fn local_privilege_kind_field(&self, cx: &mut Context<Self>) -> AnyElement {
        form_field(
            &self.tokens,
            self.i18n
                .t("sessionManager.privilege_credentials.kind_label"),
            self.settings_select_control(
                SettingsSelect::LocalPrivilegeKind,
                self.settings_privilege_kind_label(self.settings_local_privilege_draft.kind),
                false,
                Some(self.tokens.metrics.settings_select_width),
                cx,
            ),
        )
    }

    fn local_privilege_text_field(
        &self,
        label_key: &'static str,
        input: SettingsInput,
        value: String,
        placeholder_key: &'static str,
        secret: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        form_field(
            &self.tokens,
            self.i18n.t(label_key),
            if secret {
                self.settings_secret_text_input_control(
                    input,
                    value,
                    self.i18n.t(placeholder_key),
                    self.tokens.metrics.settings_select_width,
                    cx,
                )
            } else {
                self.settings_text_input_control(
                    input,
                    value,
                    self.i18n.t(placeholder_key),
                    self.tokens.metrics.settings_select_width,
                    cx,
                )
            },
        )
    }

    fn local_privilege_prompt_patterns_field(&self, cx: &mut Context<Self>) -> AnyElement {
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

    fn settings_privilege_kind_label(&self, kind: PrivilegeCredentialKind) -> String {
        // Settings owns its own label helper so the local-shell form does not
        // depend on private new-connection rendering helpers.
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

    fn local_privilege_hint(&self, text: String) -> AnyElement {
        div()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(text)
            .into_any_element()
    }

    fn settings_reconnect_section(
        &self,
        section_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.settings_store.settings();
        let reconnect_enabled = settings.reconnect.enabled;
        match section_index {
            0 => self.reconnect_enabled_row(reconnect_enabled, cx),
            1 => separator(&self.tokens, SeparatorOrientation::Horizontal).into_any_element(),
            2 => div()
                .flex()
                .flex_col()
                .gap(px(24.0))
                .opacity(if reconnect_enabled { 1.0 } else { 0.4 })
                .child(
                    div()
                        .text_size(px(18.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(self.tokens.ui.text_heading))
                        .child(self.i18n.t("settings_view.reconnect.strategy")),
                )
                .child(
                    div()
                        .w_full()
                        .max_w(px(SETTINGS_RECONNECT_MAX_WIDTH))
                        .flex()
                        .flex_row()
                        .gap(px(32.0))
                        .child(self.reconnect_select_field(
                            "settings_view.reconnect.max_attempts",
                            "settings_view.reconnect.max_attempts_hint",
                            SettingsSelect::ReconnectMaxAttempts,
                            reconnect_attempt_label(settings.reconnect.max_attempts),
                            reconnect_enabled,
                            cx,
                        ))
                        .child(self.reconnect_select_field(
                            "settings_view.reconnect.base_delay",
                            "settings_view.reconnect.base_delay_hint",
                            SettingsSelect::ReconnectBaseDelay,
                            reconnect_delay_label(settings.reconnect.base_delay_ms),
                            reconnect_enabled,
                            cx,
                        )),
                )
                .child(
                    div()
                        .w_full()
                        .max_w(px(SETTINGS_RECONNECT_MAX_WIDTH))
                        .flex()
                        .flex_row()
                        .gap(px(32.0))
                        .child(self.reconnect_select_field(
                            "settings_view.reconnect.max_delay",
                            "settings_view.reconnect.max_delay_hint",
                            SettingsSelect::ReconnectMaxDelay,
                            reconnect_delay_label(settings.reconnect.max_delay_ms),
                            reconnect_enabled,
                            cx,
                        )),
                )
                .child(
                    div()
                        .max_w(px(SETTINGS_RECONNECT_MAX_WIDTH))
                        .p(px(16.0))
                        .rounded(px(self.tokens.radii.md))
                        .border_1()
                        .border_color(rgba((self.tokens.ui.border << 8) | 0x80))
                        .bg(self.settings_panel_background(self.tokens.ui.bg_card))
                        .shadow(oxideterm_gpui_ui::tauri_card_shadow(
                            self.tokens.ui.bg_card,
                        ))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.i18n.t("settings_view.reconnect.formula_hint")),
                )
                .into_any_element(),
            _ => div().into_any_element(),
        }
    }

    fn reconnect_enabled_row(&self, checked: bool, cx: &mut Context<Self>) -> AnyElement {
        div()
            .w_full()
            .max_w(px(SETTINGS_RECONNECT_MAX_WIDTH))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .child(
                div()
                    .grid()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t("settings_view.reconnect.enabled")),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("settings_view.reconnect.enabled_hint")),
                    ),
            )
            .child(
                checkbox(&self.tokens, String::new(), checked)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.edit_settings(
                                |settings| set_reconnect_enabled(settings, !checked),
                                cx,
                            );
                        }),
                    )
                    .into_any_element(),
            )
            .into_any_element()
    }

    fn reconnect_select_field(
        &self,
        label_key: &str,
        hint_key: &str,
        select_id: SettingsSelect,
        value: String,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let control = self.settings_select_control(select_id, value, !enabled, None, cx);

        div()
            .w(px(SETTINGS_RECONNECT_FIELD_WIDTH))
            .min_w(px(0.0))
            .grid()
            .gap(px(8.0))
            .child(
                div()
                    .grid()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t(label_key)),
                    )
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .whitespace_normal()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .line_height(px(SETTINGS_RECONNECT_HINT_LINE_HEIGHT))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t(hint_key)),
                    ),
            )
            .child(control)
            .when(!enabled, |field| {
                field.on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                    cx.stop_propagation();
                })
            })
            .into_any_element()
    }

}
