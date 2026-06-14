impl WorkspaceApp {
    fn new_connection_select_anchor_id(select_id: NewConnectionSelect) -> SelectAnchorId {
        match select_id {
            NewConnectionSelect::Group => SelectAnchorId::NewConnectionGroup,
            NewConnectionSelect::ManagedKey => SelectAnchorId::NewConnectionManagedKey,
            NewConnectionSelect::JumpManagedKey => SelectAnchorId::NewConnectionJumpManagedKey,
            NewConnectionSelect::PrivilegeKind => SelectAnchorId::NewConnectionPrivilegeKind,
            NewConnectionSelect::UpstreamProxyPolicy => {
                SelectAnchorId::NewConnectionUpstreamProxyPolicy
            }
            NewConnectionSelect::UpstreamProxyProtocol => {
                SelectAnchorId::NewConnectionUpstreamProxyProtocol
            }
            NewConnectionSelect::UpstreamProxyAuth => SelectAnchorId::NewConnectionUpstreamProxyAuth,
            NewConnectionSelect::SerialPort => SelectAnchorId::NewConnectionSerialPort,
            NewConnectionSelect::SerialDataBits => SelectAnchorId::NewConnectionSerialDataBits,
            NewConnectionSelect::SerialStopBits => SelectAnchorId::NewConnectionSerialStopBits,
            NewConnectionSelect::SerialParity => SelectAnchorId::NewConnectionSerialParity,
            NewConnectionSelect::SerialFlowControl => SelectAnchorId::NewConnectionSerialFlowControl,
        }
    }

    fn new_connection_select_trigger(
        &self,
        select_id: NewConnectionSelect,
        value: String,
        placeholder: bool,
        disabled: bool,
    ) -> Div {
        let focused = self.open_new_connection_select == Some(select_id);
        // New-connection selects live inside modal forms; keep their keyboard
        // focus ring tied to the same browser focus-origin rule as settings
        // and Cloud Sync selects.
        select_trigger_with_focus_visible(
            &self.tokens,
            value,
            placeholder,
            disabled,
            browser_behavior::browser_focus_visible(focused, self.new_connection_select_focus_origin),
        )
    }

    fn open_new_connection_select_from_pointer(&mut self, select_id: NewConnectionSelect) {
        // New-connection selects share browser focus-origin semantics with
        // settings selects: pointer-opened menus should not render a keyboard
        // focus-visible ring on the trigger.
        browser_behavior::toggle_browser_trigger_select_from_pointer(
            &mut self.open_new_connection_select,
            &mut self.new_connection_select_focus_origin,
            select_id,
        );
    }

    pub(in crate::workspace) fn close_new_connection_select(&mut self) {
        browser_behavior::close_browser_trigger_select(
            &mut self.open_new_connection_select,
            &mut self.new_connection_select_focus_origin,
        );
    }

    fn clear_new_connection_select_anchor(&mut self) {
        // The group select overlay is anchored inside the new-connection scroll
        // body. Drop its cached bounds when the body scrolls so a reopened
        // overlay cannot reuse pre-scroll coordinates.
        self.select_anchors.remove(&SelectAnchorId::NewConnectionGroup);
        self.select_anchors
            .remove(&SelectAnchorId::NewConnectionManagedKey);
        self.select_anchors
            .remove(&SelectAnchorId::NewConnectionJumpManagedKey);
        self.select_anchors
            .remove(&SelectAnchorId::NewConnectionPrivilegeKind);
        self.select_anchors
            .remove(&SelectAnchorId::NewConnectionUpstreamProxyPolicy);
        self.select_anchors
            .remove(&SelectAnchorId::NewConnectionUpstreamProxyProtocol);
        self.select_anchors
            .remove(&SelectAnchorId::NewConnectionUpstreamProxyAuth);
        self.select_anchors
            .remove(&SelectAnchorId::NewConnectionSerialPort);
        self.select_anchors
            .remove(&SelectAnchorId::NewConnectionSerialDataBits);
        self.select_anchors
            .remove(&SelectAnchorId::NewConnectionSerialStopBits);
        self.select_anchors
            .remove(&SelectAnchorId::NewConnectionSerialParity);
        self.select_anchors
            .remove(&SelectAnchorId::NewConnectionSerialFlowControl);
    }

    fn render_connection_hint(&self, text: String) -> AnyElement {
        self.render_connection_hint_with_color(text, self.tokens.ui.text_muted)
    }

    fn render_connection_hint_with_color(&self, text: String, color: u32) -> AnyElement {
        div()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(color))
            .child(text)
            .into_any_element()
    }

    fn render_agent_status(&self, available: Option<bool>) -> AnyElement {
        let (color, label) = match available {
            Some(true) => (self.tokens.ui.success, self.i18n.t("ssh.form.agent_detected")),
            Some(false) => (
                self.tokens.ui.error,
                self.i18n.t("ssh.form.agent_not_detected"),
            ),
            None => (self.tokens.ui.text_muted, "...".to_string()),
        };
        div()
            .flex()
            .items_center()
            .gap_2()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .child(div().size(px(8.0)).rounded_full().bg(rgb(color)))
            .child(div().text_color(rgb(color)).child(label))
            .into_any_element()
    }

    fn render_prompt_error_box(&self, error: String) -> AnyElement {
        let error_color = self.tokens.ui.error;
        div()
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgba((error_color << 8) | TAURI_PROMPT_ERROR_BORDER_ALPHA))
            .bg(rgba((error_color << 8) | TAURI_PROMPT_ERROR_ALPHA))
            .px(px(self.tokens.spacing.three))
            .py(px(self.tokens.spacing.two))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(error_color))
            .child(error)
            .into_any_element()
    }

    fn render_connection_field(
        &self,
        label: String,
        value: &str,
        placeholder: String,
        field: NewConnectionField,
        secret: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        form_field(
            &self.tokens,
            label,
            self.render_connection_input(value, placeholder, field, secret, cx),
        )
    }

    fn render_connection_textarea(
        &self,
        label: String,
        value: &str,
        placeholder: String,
        field: NewConnectionField,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        form_field(
            &self.tokens,
            label,
            self.render_connection_textarea_input(value, placeholder, field, cx),
        )
    }

    fn render_edit_saved_password_field(
        &self,
        form: &NewConnectionForm,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let value = if form.password_loaded {
            form.password.as_str()
        } else {
            ""
        };
        let icon = if form.password_loading {
            LucideIcon::LoaderCircle
        } else if form.password_visible {
            LucideIcon::EyeOff
        } else {
            LucideIcon::Eye
        };
        let secret = form.password_loaded && !form.password_visible;
        form_field(
            &self.tokens,
            self.i18n.t("sessionManager.edit_properties.saved_password"),
            div()
                .relative()
                .child(
                    self.render_connection_input(
                        value,
                        self.i18n
                            .t("sessionManager.edit_properties.password_placeholder"),
                        NewConnectionField::Password,
                        secret,
                        cx,
                    ),
                )
                .child(
                    self.workspace_icon_action_button(
                        icon,
                        TAURI_PASSWORD_ICON_SIZE,
                        rgb(self.tokens.ui.text_muted),
                        IconButtonOptions {
                            loading: form.password_loading,
                            hover_background: Some(rgba((self.tokens.ui.bg_hover << 8) | 0x99)),
                            // Tauri places the reveal affordance inside the
                            // password input as an icon-only toolbar button.
                            // Keep size/radius/loading in the shared primitive
                            // so password-like controls do not hand-roll div
                            // opacity and cursor semantics.
                            ..IconButtonOptions::opaque_toolbar(
                                TAURI_PASSWORD_ICON_BUTTON_SIZE,
                                ButtonRadius::Sm,
                            )
                        },
                        |this, _event, _window, cx| {
                            this.toggle_edit_saved_password_visibility(cx);
                            cx.stop_propagation();
                        },
                        cx,
                    )
                    .absolute()
                    .right(px(TAURI_PASSWORD_ICON_BUTTON_OFFSET))
                    .top(px(TAURI_PASSWORD_ICON_BUTTON_OFFSET)),
                ),
        )
    }

    fn render_connection_field_with_browse(
        &self,
        label: String,
        value: &str,
        placeholder: String,
        field: NewConnectionField,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        form_field(
            &self.tokens,
            label,
            div()
                .flex()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .child(self.render_connection_input(value, placeholder, field, false, cx)),
                )
                .child(
                    // Tauri browse controls are outline Buttons beside the
                    // path input. Keep this modal-form action on the shared
                    // toolbar primitive so disabled/focus behavior can be
                    // centralized with other form buttons.
                    self.workspace_toolbar_action_button(
                        self.i18n.t("sessionManager.edit_properties.browse"),
                        None,
                        ToolbarButtonOptions {
                            button: ButtonOptions {
                                variant: ButtonVariant::Outline,
                                size: ButtonSize::Sm,
                                ..ButtonOptions::default()
                            },
                            ..ToolbarButtonOptions::default()
                        },
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_new_connection_select();
                            this.pick_new_connection_path(field, cx);
                            cx.stop_propagation();
                        }),
                    ),
                ),
        )
    }

    fn render_connection_group_select(
        &self,
        label: String,
        value: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected_label = if self.connection_form_group_is_ungrouped(value) {
            self.connection_form_ungrouped_label()
        } else {
            value.trim().to_string()
        };
        let anchor_id = SelectAnchorId::NewConnectionGroup;
        let workspace = cx.entity();
        let trigger = self
            .new_connection_select_trigger(NewConnectionSelect::Group, selected_label, false, false)
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    if let Some(form) = this.new_connection_form.as_mut() {
                        form.field_focused = false;
                        form.selected_field = None;
                    }
                    this.ime_marked_text = None;
                    this.open_new_connection_select_from_pointer(NewConnectionSelect::Group);
                    window.focus(&this.focus_handle);
                    cx.stop_propagation();
                    cx.notify();
                }),
            );

        form_field(
            &self.tokens,
            label,
            select_anchor_probe(anchor_id, trigger, move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_select_anchor(anchor, cx);
                });
            }),
        )
    }

    fn privilege_kind_label(&self, kind: PrivilegeCredentialKind) -> String {
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

    fn render_privilege_kind_select(
        &self,
        kind: PrivilegeCredentialKind,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        form_field(
            &self.tokens,
            self.i18n
                .t("sessionManager.privilege_credentials.kind_label"),
            self.render_new_connection_select_control(
                NewConnectionSelect::PrivilegeKind,
                self.privilege_kind_label(kind),
                false,
                false,
                cx,
            ),
        )
    }

    fn reset_privilege_credential_draft(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = self.new_connection_form.as_mut() {
            form.privilege_draft = Default::default();
            form.privilege_error = None;
            form.focused_field = NewConnectionField::PrivilegeLabel;
            form.field_focused = true;
            form.selected_field = None;
        }
        self.close_new_connection_select();
        cx.notify();
    }

    fn edit_privilege_credential(&mut self, credential: SavedPrivilegeCredential, cx: &mut Context<Self>) {
        if let Some(form) = self.new_connection_form.as_mut() {
            form.privilege_draft.credential_id = Some(credential.id);
            form.privilege_draft.label = credential.label;
            form.privilege_draft.kind = credential.kind;
            form.privilege_draft.username_hint = credential.username_hint.unwrap_or_default();
            form.privilege_draft.prompt_patterns = credential.prompt_patterns.join("\n");
            form.privilege_draft.secret.clear();
            form.privilege_draft.enabled = credential.enabled;
            form.privilege_error = None;
            form.focused_field = NewConnectionField::PrivilegeLabel;
            form.field_focused = true;
            form.selected_field = None;
        }
        self.close_new_connection_select();
        cx.notify();
    }

    fn set_privilege_credential_kind(
        &mut self,
        kind: PrivilegeCredentialKind,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = self.new_connection_form.as_mut() {
            form.privilege_draft.kind = kind;
            form.privilege_error = None;
        }
        self.close_new_connection_select();
        cx.notify();
    }

    fn save_privilege_credential_from_form(&mut self, cx: &mut Context<Self>) {
        let Some(connection_id) = self.editing_saved_connection_id.clone() else {
            return;
        };
        if self.duplicating_saved_connection_id.is_some() {
            return;
        }
        let Some(draft) = self.new_connection_form.as_ref().map(|form| form.privilege_draft.clone()) else {
            return;
        };
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
        // UI drafts necessarily live as String for GPUI text editing. The
        // save action is the store boundary where the cleartext moves into
        // SecretString's Zeroizing owner before keychain persistence.
        let secret = (!draft.secret.is_empty()).then(|| {
            SecretString::from(zeroize::Zeroizing::new(draft.secret.clone()))
        });
        let request = SavePrivilegeCredentialRequest {
            connection_id: connection_id.clone(),
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
            Ok(saved) => {
                if let Some(form) = self.new_connection_form.as_mut() {
                    if let Some(index) = form
                        .privilege_credentials
                        .iter()
                        .position(|credential| credential.id == saved.id)
                    {
                        form.privilege_credentials[index] = saved;
                    } else {
                        form.privilege_credentials.push(saved);
                    }
                    form.privilege_draft = Default::default();
                    form.privilege_error = None;
                }
                self.queue_cloud_sync_dirty_refresh(cx);
            }
            Err(error) => {
                if let Some(form) = self.new_connection_form.as_mut() {
                    form.privilege_error = Some(error.to_string());
                }
            }
        }
        self.close_new_connection_select();
        cx.notify();
    }

    fn delete_privilege_credential_from_form(&mut self, credential_id: String, cx: &mut Context<Self>) {
        let Some(connection_id) = self.editing_saved_connection_id.clone() else {
            return;
        };
        match self
            .connection_store
            .delete_privilege_credential(&connection_id, &credential_id)
        {
            Ok(_) => {
                if let Some(form) = self.new_connection_form.as_mut() {
                    form.privilege_credentials
                        .retain(|credential| credential.id != credential_id);
                    if form.privilege_draft.credential_id.as_deref() == Some(&credential_id) {
                        form.privilege_draft = Default::default();
                    }
                    form.privilege_error = None;
                }
                self.queue_cloud_sync_dirty_refresh(cx);
            }
            Err(error) => {
                if let Some(form) = self.new_connection_form.as_mut() {
                    form.privilege_error = Some(error.to_string());
                }
            }
        }
        self.close_new_connection_select();
        cx.notify();
    }

    fn render_privilege_credentials_section(
        &self,
        form: &NewConnectionForm,
        duplicate_mode: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        // Tauri EditConnectionPropertiesModal uses rounded-lg,
        // border-theme-border/60, bg-theme-bg-panel/45, p-3.
        let mut list = div().flex().flex_col().gap(px(8.0));
        if form.privilege_credentials.is_empty() {
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
            for credential in form.privilege_credentials.iter().cloned() {
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
                        .bg(rgba((theme.bg << 8) | 0x73))
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
                                        .child(self.privilege_kind_label(credential.kind)),
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
                                this.edit_privilege_credential(edit_credential.clone(), cx);
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
                                this.delete_privilege_credential_from_form(delete_id.clone(), cx);
                                cx.stop_propagation();
                            },
                            cx,
                        )),
                );
            }
        }

        let description = if duplicate_mode {
            self.i18n
                .t("sessionManager.privilege_credentials.duplicate_hint")
        } else {
            self.i18n
                .t("sessionManager.privilege_credentials.description")
        };
        let mut section = div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgba((theme.border << 8) | 0x99))
            .bg(rgba((theme.bg_panel << 8) | 0x73))
            .p(px(12.0))
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text))
                                    .child(Self::render_lucide_icon(
                                        LucideIcon::KeyRound,
                                        16.0,
                                        rgb(theme.text_muted),
                                    ))
                                    .child(
                                        self.i18n
                                            .t("sessionManager.privilege_credentials.title"),
                                    ),
                            )
                            .child(
                                div()
                                    .mt(px(4.0))
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(theme.text_muted))
                                    .child(description),
                            ),
                    )
                    .when(!duplicate_mode, |header| {
                        header.child(self.workspace_toolbar_action_button(
                            self.i18n.t("sessionManager.privilege_credentials.new"),
                            Some(
                                Self::render_lucide_icon(
                                    LucideIcon::Plus,
                                    14.0,
                                    rgb(theme.text_muted),
                                )
                                .into_any_element(),
                            ),
                            ToolbarButtonOptions {
                                button: ButtonOptions {
                                    variant: ButtonVariant::Outline,
                                    size: ButtonSize::Sm,
                                    ..ButtonOptions::default()
                                },
                                ..ToolbarButtonOptions::default()
                            },
                            cx.listener(|this, _event, _window, cx| {
                                this.reset_privilege_credential_draft(cx);
                                cx.stop_propagation();
                            }),
                        ))
                    }),
            );
        if !duplicate_mode {
            section = section.child(list).child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgba((theme.border << 8) | 0x80))
                    .bg(rgba((theme.bg << 8) | 0x80))
                    .p(px(12.0))
                    .child(self.render_connection_field(
                        self.i18n.t("sessionManager.privilege_credentials.label"),
                        &form.privilege_draft.label,
                        self.i18n
                            .t("sessionManager.privilege_credentials.label_placeholder"),
                        NewConnectionField::PrivilegeLabel,
                        false,
                        cx,
                    ))
                    .child(
                        div()
                            .grid()
                            .grid_cols(2)
                            .gap(px(12.0))
                            .child(self.render_privilege_kind_select(
                                form.privilege_draft.kind,
                                cx,
                            ))
                            .child(self.render_connection_field(
                                self.i18n
                                    .t("sessionManager.privilege_credentials.username_hint"),
                                &form.privilege_draft.username_hint,
                                form.username.clone(),
                                NewConnectionField::PrivilegeUsernameHint,
                                false,
                                cx,
                            )),
                    )
                    .child(self.render_connection_field(
                        self.i18n.t("sessionManager.privilege_credentials.secret"),
                        &form.privilege_draft.secret,
                        if form.privilege_draft.credential_id.is_some() {
                            self.i18n
                                .t("sessionManager.privilege_credentials.secret_keep_placeholder")
                        } else {
                            self.i18n
                                .t("sessionManager.privilege_credentials.secret_placeholder")
                        },
                        NewConnectionField::PrivilegeSecret,
                        true,
                        cx,
                    ))
                    .child(self.render_connection_textarea(
                        self.i18n
                            .t("sessionManager.privilege_credentials.prompt_patterns"),
                        &form.privilege_draft.prompt_patterns,
                        self.i18n
                            .t("sessionManager.privilege_credentials.prompt_patterns_placeholder"),
                        NewConnectionField::PrivilegePromptPatterns,
                        cx,
                    ))
                    .child(self.render_connection_hint(
                        self.i18n
                            .t("sessionManager.privilege_credentials.prompt_patterns_hint"),
                    ))
                    .child(self.render_connection_checkbox(
                        self.i18n.t("sessionManager.privilege_credentials.enabled"),
                        form.privilege_draft.enabled,
                        |form| form.privilege_draft.enabled = !form.privilege_draft.enabled,
                        cx,
                    ))
                    .when_some(form.privilege_error.clone(), |panel, error| {
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
                            .when(form.privilege_draft.credential_id.is_some(), |row| {
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
                                        this.reset_privilege_credential_draft(cx);
                                        cx.stop_propagation();
                                    }),
                                ))
                            })
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
                                        disabled: form.privilege_draft.label.trim().is_empty(),
                                        ..ButtonOptions::default()
                                    },
                                    ..ToolbarButtonOptions::default()
                                },
                                cx.listener(|this, _event, _window, cx| {
                                    this.save_privilege_credential_from_form(cx);
                                    cx.stop_propagation();
                                }),
                            )),
                    ),
            );
        }
        section.into_any_element()
    }

    fn set_new_connection_group(&mut self, group: String, cx: &mut Context<Self>) {
        if let Some(form) = self.new_connection_form.as_mut() {
            form.group = group;
            form.field_focused = false;
            form.selected_field = None;
            form.error = None;
        }
        self.close_new_connection_select();
        self.ime_marked_text = None;
        cx.notify();
    }

    fn render_managed_key_select(
        &self,
        label: String,
        selected_id: &str,
        jump_form: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let keys = self.connection_store.managed_ssh_keys();
        let selected_label = if selected_id.trim().is_empty() {
            self.i18n.t("ssh.form.managed_key_placeholder")
        } else {
            keys.iter()
                .find(|key| key.id == selected_id)
                .map(|key| key.name.clone())
                .unwrap_or_else(|| selected_id.to_string())
        };
        let select_id = if jump_form {
            NewConnectionSelect::JumpManagedKey
        } else {
            NewConnectionSelect::ManagedKey
        };
        let anchor_id = if jump_form {
            SelectAnchorId::NewConnectionJumpManagedKey
        } else {
            SelectAnchorId::NewConnectionManagedKey
        };
        let workspace = cx.entity();
        let trigger = self
            .new_connection_select_trigger(
                select_id,
                selected_label,
                selected_id.trim().is_empty(),
                keys.is_empty(),
            )
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    if this.connection_store.managed_ssh_keys().is_empty() {
                        cx.stop_propagation();
                        return;
                    }
                    if let Some(form) = this.new_connection_form.as_mut() {
                        form.field_focused = false;
                        form.selected_field = None;
                    }
                    this.ime_marked_text = None;
                    this.open_new_connection_select_from_pointer(select_id);
                    window.focus(&this.focus_handle);
                    cx.stop_propagation();
                    cx.notify();
                }),
            );

        form_field(
            &self.tokens,
            label,
            select_anchor_probe(anchor_id, trigger, move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_select_anchor(anchor, cx);
                });
            }),
        )
        .into_any_element()
    }

    fn set_new_connection_managed_key(
        &mut self,
        select_id: NewConnectionSelect,
        key_id: String,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = self.new_connection_form.as_mut() {
            match select_id {
                NewConnectionSelect::ManagedKey => {
                    form.managed_key_id = key_id;
                    form.focused_field = NewConnectionField::ManagedKeyId;
                }
                NewConnectionSelect::JumpManagedKey => {
                    let Some(jump_form) = form.jump_server_form.as_mut() else {
                        return;
                    };
                    jump_form.managed_key_id = key_id;
                    form.focused_field = NewConnectionField::JumpManagedKeyId;
                }
                NewConnectionSelect::Group
                | NewConnectionSelect::PrivilegeKind
                | NewConnectionSelect::UpstreamProxyPolicy
                | NewConnectionSelect::UpstreamProxyProtocol
                | NewConnectionSelect::UpstreamProxyAuth
                | NewConnectionSelect::SerialPort
                | NewConnectionSelect::SerialDataBits
                | NewConnectionSelect::SerialStopBits
                | NewConnectionSelect::SerialParity
                | NewConnectionSelect::SerialFlowControl => return,
            }
            form.field_focused = false;
            form.selected_field = None;
            form.error = None;
        }
        self.close_new_connection_select();
        self.ime_marked_text = None;
        cx.notify();
    }

    fn connection_form_group_options(&self, current_group: &str) -> Vec<String> {
        let mut groups = self.connection_store.groups().to_vec();
        let current = current_group.trim();
        if !current.is_empty()
            && !self.connection_form_group_is_ungrouped(current)
            && !groups.iter().any(|group| group == current)
        {
            groups.push(current.to_string());
        }
        groups.sort();
        groups.dedup();
        groups
    }

    fn connection_form_group_is_ungrouped(&self, group: &str) -> bool {
        let group = group.trim();
        group.is_empty()
            || group == "Ungrouped"
            || group == "未分组"
            || group == self.i18n.t("ssh.form.ungrouped")
            || group == self.i18n.t("sessionManager.edit_properties.ungrouped")
    }

    fn connection_form_ungrouped_label(&self) -> String {
        self.i18n.t("ssh.form.ungrouped")
    }

    fn pick_new_connection_path(&mut self, field: NewConnectionField, cx: &mut Context<Self>) {
        if !matches!(
            field,
            NewConnectionField::KeyPath
                | NewConnectionField::CertPath
                | NewConnectionField::JumpKeyPath
                | NewConnectionField::JumpCertPath
        ) {
            return;
        }
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(SharedString::from(
                self.i18n.t("sessionManager.edit_properties.browse"),
            )),
        });
        cx.spawn(async move |weak, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let path = path.to_string_lossy().to_string();
            let _ = weak.update(cx, |this, cx| {
                if let Some(form) = this.new_connection_form.as_mut() {
                    match field {
                        NewConnectionField::KeyPath => form.key_path = path,
                        NewConnectionField::CertPath => form.cert_path = path,
                        NewConnectionField::JumpKeyPath => {
                            let Some(jump_form) = form.jump_server_form.as_mut() else {
                                return;
                            };
                            jump_form.key_path = path;
                        }
                        NewConnectionField::JumpCertPath => {
                            let Some(jump_form) = form.jump_server_form.as_mut() else {
                                return;
                            };
                            jump_form.cert_path = path;
                        }
                        _ => return,
                    }
                    form.focused_field = field;
                    form.field_focused = true;
                    form.error = None;
                    clear_connection_selection(form);
                }
                this.new_connection_caret_visible = true;
                cx.notify();
            });
        })
        .detach();
    }

    fn toggle_edit_saved_password_visibility(&mut self, cx: &mut Context<Self>) {
        let source_connection_id = self
            .saved_connection_form_source_id()
            .map(|connection_id| connection_id.to_string());
        let Some(form) = self.new_connection_form.as_mut() else {
            return;
        };
        if form.password_loading {
            return;
        }
        if form.password_loaded {
            form.password_visible = !form.password_visible;
            form.password_error = None;
            cx.notify();
            return;
        }

        let Some(connection_id) = source_connection_id else {
            return;
        };
        form.password_loading = true;
        form.password_error = None;
        cx.notify();

        let store = self.connection_store.clone();
        cx.spawn(async move |weak, cx| {
            let result = store.get_connection_password(&connection_id);
            let _ = weak.update(cx, |this, cx| {
                if let Some(form) = this.new_connection_form.as_mut() {
                    form.password_loading = false;
                    match result {
                        Ok(password) => {
                            // Replacing an editable password draft should wipe
                            // the previous buffer before the newly loaded value
                            // is exposed for user editing.
                            zeroize::Zeroize::zeroize(&mut form.password);
                            form.password = password.expose_secret().to_string();
                            form.password_loaded = true;
                            form.password_visible = true;
                            form.password_error = None;
                            form.focused_field = NewConnectionField::Password;
                            form.field_focused = true;
                            clear_connection_selection(form);
                            this.new_connection_caret_visible = true;
                        }
                        Err(error) => {
                            form.password_error = Some(error.to_string());
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn render_connection_input(
        &self,
        value: &str,
        placeholder: String,
        field: NewConnectionField,
        secret: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = self
            .new_connection_form
            .as_ref()
            .is_some_and(|form| form.field_focused && form.focused_field == field);
        let selected_all = self
            .new_connection_form
            .as_ref()
            .is_some_and(|form| connection_field_is_selected(form, field));
        let target = WorkspaceImeTarget::NewConnection(field);
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value,
                    placeholder,
                    focused,
                    caret_visible: self.new_connection_caret_visible,
                    secret,
                    selected_all,
                    selected_range: self.ime_selected_range_for_target(target),
                    marked_text: self.marked_text_for_target(target),
                },
            )
            .id(("connection-field", field as u32))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    if let Some(form) = this.new_connection_form.as_mut() {
                        form.field_focused = true;
                        form.focused_field = field;
                        clear_connection_selection(form);
                    }
                    this.close_new_connection_select();
                    this.ime_marked_text = None;
                    this.new_connection_caret_visible = true;
                    window.focus(&this.focus_handle);
                    this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                    this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                }),
            ),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    fn render_connection_textarea_input(
        &self,
        value: &str,
        placeholder: String,
        field: NewConnectionField,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = self
            .new_connection_form
            .as_ref()
            .is_some_and(|form| form.field_focused && form.focused_field == field);
        let selected_all = self
            .new_connection_form
            .as_ref()
            .is_some_and(|form| connection_field_is_selected(form, field));
        let target = WorkspaceImeTarget::NewConnection(field);
        let workspace = cx.entity();
        let theme = self.tokens.ui;
        let visually_empty = value.is_empty();
        let mut lines = div().flex().flex_col().gap(px(2.0));

        if visually_empty {
            lines = lines.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .when(focused, |row| {
                        row.child(text_caret(&self.tokens, self.new_connection_caret_visible))
                    })
                    .child(placeholder),
            );
        } else {
            let split_lines: Vec<&str> = value.split('\n').collect();
            let last_index = split_lines.len().saturating_sub(1);
            for (index, line) in split_lines.into_iter().enumerate() {
                let line_selected_range =
                    selected_all.then_some(0..line.encode_utf16().count());
                let is_last = index == last_index;
                let row = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .min_h(px(self.tokens.metrics.form_caret_height))
                    .child(text_input_value_segments(
                        &self.tokens,
                        line,
                        false,
                        line_selected_range,
                        None,
                        self.new_connection_caret_visible,
                    ))
                    .when(focused && is_last && !selected_all, |row| {
                        row.child(text_caret(&self.tokens, self.new_connection_caret_visible))
                    });
                lines = lines.child(row);
            }
        }

        text_input_anchor_probe(
            target.anchor_id(),
            div()
                // Tauri uses `<textarea className="min-h-20 resize-y ...">`.
                // Native keeps the same minimum height and multiline editing
                // semantics, while leaving all other connection fields single-line.
                .min_h(px(80.0))
                .px(px(self.tokens.metrics.ui_control_padding_x))
                .py(px(self.tokens.spacing.two))
                .rounded(px(self.tokens.radii.md))
                .bg(rgba((theme.bg << 8) | 0x80))
                .border_1()
                .border_color(if focused {
                    rgb(theme.accent)
                } else {
                    rgb(theme.border)
                })
                .text_size(px(self.tokens.metrics.ui_text_sm))
                .text_color(if visually_empty {
                    rgb(theme.text_muted)
                } else {
                    rgb(theme.text)
                })
                .cursor(CursorStyle::IBeam)
                .overflow_hidden()
                .child(lines)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                        if let Some(form) = this.new_connection_form.as_mut() {
                            form.field_focused = true;
                            form.focused_field = field;
                            clear_connection_selection(form);
                        }
                        this.close_new_connection_select();
                        this.ime_marked_text = None;
                        this.new_connection_caret_visible = true;
                        window.focus(&this.focus_handle);
                        this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_move(
                    cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                        this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                    }),
                ),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    fn render_prompt_auth_radios(
        &self,
        active_tab: SshAuthTab,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let choices = [
            (SshAuthTab::Password, "ssh.auth.password"),
            (SshAuthTab::SshKey, "ssh.auth.ssh_key"),
            (SshAuthTab::ManagedKey, "ssh.auth.managed_key"),
            (SshAuthTab::Agent, "ssh.auth.agent"),
            (SshAuthTab::Certificate, "ssh.auth.certificate"),
        ];
        let mut group = radio_group(&self.tokens).flex().flex_row().gap_4();
        for (tab, key) in choices {
            let selected = tab == active_tab
                || (tab == SshAuthTab::SshKey && active_tab == SshAuthTab::DefaultKey);
            group = group.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .cursor_pointer()
                    .child(radio_group_item(&self.tokens, selected, false))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t(key)),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            if let Some(form) = this.new_connection_form.as_mut() {
                                form.auth_tab = tab;
                                clear_connection_selection(form);
                            }
                            this.close_new_connection_select();
                            cx.notify();
                        }),
                    ),
            );
        }
        form_field(
            &self.tokens,
            self.i18n.t("sessionManager.edit_properties.auth_type"),
            group,
        )
    }

    fn render_auth_tabs(
        &self,
        active_tab: SshAuthTab,
        edit_properties_mode: bool,
        kbi_disabled_for_proxy_chain: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tabs: Vec<(SshAuthTab, &str)> = if edit_properties_mode {
            vec![
                (
                    SshAuthTab::Password,
                    "sessionManager.edit_properties.auth_password",
                ),
                (
                    SshAuthTab::SshKey,
                    "sessionManager.edit_properties.auth_key",
                ),
                (SshAuthTab::ManagedKey, "ssh.auth.managed_key"),
                (SshAuthTab::Certificate, "ssh.auth.certificate"),
                (
                    SshAuthTab::Agent,
                    "sessionManager.edit_properties.auth_agent",
                ),
            ]
        } else {
            vec![
                (SshAuthTab::Password, "ssh.auth.password"),
                (SshAuthTab::DefaultKey, "ssh.auth.default_key"),
                (SshAuthTab::SshKey, "ssh.auth.ssh_key"),
                (SshAuthTab::ManagedKey, "ssh.auth.managed_key"),
                (SshAuthTab::Certificate, "ssh.auth.certificate"),
                (SshAuthTab::Agent, "ssh.auth.agent"),
                (SshAuthTab::TwoFactor, "ssh.auth.two_factor"),
            ]
        };
        let build_tab = |this: &Self,
                         tab: SshAuthTab,
                         key: &str,
                         active_tab: SshAuthTab,
                         disabled: bool,
                         cx: &mut Context<Self>| {
            let selected = tab == active_tab
                || (edit_properties_mode
                    && tab == SshAuthTab::SshKey
                    && active_tab == SshAuthTab::DefaultKey);
            let item = segmented_tab(&this.tokens, this.i18n.t(key), selected)
                .min_h(px(this.tokens.metrics.ui_tabs_list_height))
                .whitespace_normal()
                .text_align(gpui::TextAlign::Center)
                .line_height(px(this.tokens.metrics.ui_text_sm + 2.0))
                .opacity(if disabled { 0.45 } else { 1.0 });
            if disabled {
                item
            } else {
                item.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        if let Some(form) = this.new_connection_form.as_mut() {
                            form.auth_tab = tab;
                            clear_connection_selection(form);
                        }
                        this.close_new_connection_select();
                        cx.notify();
                    }),
                )
            }
        };

        let row = if edit_properties_mode {
            let mut row = segmented_tabs(&self.tokens);
            for (tab, key) in tabs {
                let disabled = tab == SshAuthTab::TwoFactor && kbi_disabled_for_proxy_chain;
                row = row.child(build_tab(self, tab, key, active_tab, disabled, cx));
            }
            row.into_any_element()
        } else {
            let mut first_row = self.render_auth_tab_row();
            let mut second_row = self.render_auth_tab_row();
            for (index, (tab, key)) in tabs.into_iter().enumerate() {
                let disabled = tab == SshAuthTab::TwoFactor && kbi_disabled_for_proxy_chain;
                let item = build_tab(self, tab, key, active_tab, disabled, cx);
                if index < 3 {
                    first_row = first_row.child(item);
                } else {
                    second_row = second_row.child(item);
                }
            }
            // Mirrors Tauri's 3+4 auth-tab wrap while keeping one shared auth state.
            div()
                .flex()
                .flex_col()
                .gap(px(self.tokens.spacing.one))
                .child(first_row)
                .child(second_row)
                .into_any_element()
        };
        let field = form_field(&self.tokens, self.i18n.t("ssh.form.authentication"), row);
        if kbi_disabled_for_proxy_chain {
            div()
                .flex()
                .flex_col()
                .gap(px(self.tokens.spacing.two))
                .child(field)
                .child(self.render_connection_hint_with_color(
                    self.i18n.t("sessionManager.toast.proxy_hop_kbi_unsupported"),
                    self.tokens.ui.warning,
                ))
                .into_any_element()
        } else {
            field
        }
    }

    fn render_auth_tab_row(&self) -> Div {
        div()
            .min_h(px(self.tokens.metrics.ui_tabs_list_height))
            .flex()
            .flex_row()
            .items_center()
            .justify_center()
            .p(px(self.tokens.metrics.ui_tabs_list_padding))
            .rounded(px(self.tokens.radii.xs))
            .bg(rgb(self.tokens.ui.bg_panel))
            .text_color(rgb(self.tokens.ui.text_muted))
    }

    fn render_drill_auth_tabs(&self, active_tab: SshAuthTab, cx: &mut Context<Self>) -> AnyElement {
        let tabs = [
            (SshAuthTab::Agent, "ssh.drill_down.auth_agent"),
            (SshAuthTab::SshKey, "ssh.drill_down.auth_key"),
            (SshAuthTab::Password, "ssh.drill_down.auth_password"),
        ];
        let mut row = segmented_tabs(&self.tokens);
        for (tab, key) in tabs {
            row = row.child(
                segmented_tab(&self.tokens, self.i18n.t(key), tab == active_tab).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        if let Some(form) = this.new_connection_form.as_mut() {
                            form.auth_tab = tab;
                            clear_connection_selection(form);
                        }
                        this.close_new_connection_select();
                        cx.notify();
                    }),
                ),
            );
        }
        form_field(&self.tokens, self.i18n.t("ssh.drill_down.auth_method"), row).into_any_element()
    }

    fn render_edit_color_field(&self, value: &str, cx: &mut Context<Self>) -> AnyElement {
        let swatch = parse_form_hex_color(value).unwrap_or(TAURI_EDIT_COLOR_FALLBACK);
        form_field(
            &self.tokens,
            self.i18n.t("sessionManager.edit_properties.color"),
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(
                    div()
                        .size(px(self.tokens.metrics.form_input_height))
                        .rounded(px(self.tokens.radii.md))
                        .border_1()
                        .border_color(rgb(self.tokens.ui.border))
                        .bg(rgb(swatch)),
                )
                .child(div().flex_1().child(self.render_connection_input(
                    value,
                    TAURI_EDIT_COLOR_FALLBACK_TEXT.to_string(),
                    NewConnectionField::Color,
                    false,
                    cx,
                )))
                .when(!value.is_empty(), |row| {
                    row.child(
                        button(
                            &self.tokens,
                            self.i18n.t("sessionManager.edit_properties.clear_color"),
                            ButtonTone::Secondary,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                if let Some(form) = this.new_connection_form.as_mut() {
                                    form.color.clear();
                                    clear_connection_selection(form);
                                }
                                cx.notify();
                            }),
                        ),
                    )
                }),
        )
    }

    fn render_transport_selector(&self, cx: &mut Context<Self>) -> AnyElement {
        let active_transport = self
            .new_connection_form
            .as_ref()
            .map(|form| form.transport)
            .unwrap_or(NewConnectionTransport::Ssh);
        let choices = [
            (
                NewConnectionTransport::Ssh,
                self.i18n.t("modals.new_connection.transport_ssh"),
                NewConnectionField::Name,
            ),
            (
                NewConnectionTransport::Serial,
                self.i18n.t("modals.new_connection.transport_serial"),
                NewConnectionField::SerialPortPath,
            ),
        ];
        let mut row = segmented_tabs(&self.tokens);
        for (transport, label, focus_field) in choices {
            row = row.child(
                segmented_tab(&self.tokens, label, active_transport == transport).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        let mut should_refresh_ports = false;
                        if let Some(form) = this.new_connection_form.as_mut() {
                            form.transport = transport;
                            form.focused_field = focus_field;
                            form.field_focused = false;
                            form.error = None;
                            clear_connection_selection(form);
                            should_refresh_ports = transport == NewConnectionTransport::Serial
                                && form.serial_ports.is_empty()
                                && !form.serial_ports_loading;
                        }
                        this.close_new_connection_select();
                        if should_refresh_ports {
                            this.refresh_serial_ports(cx);
                        }
                        cx.notify();
                    }),
                ),
            );
        }
        row.into_any_element()
    }

    pub(in crate::workspace) fn refresh_serial_ports(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = self.new_connection_form.as_mut() {
            form.serial_ports_loading = true;
            form.error = None;
        }
        cx.notify();

        let result = oxideterm_terminal::serial_list_ports();
        if let Some(form) = self.new_connection_form.as_mut() {
            form.serial_ports_loading = false;
            match result {
                Ok(ports) => {
                    if form.serial_port_path.trim().is_empty()
                        && let Some(first_port) = ports.first()
                    {
                        form.serial_port_path = first_port.port_path.clone();
                    }
                    form.serial_ports = ports;
                }
                Err(error) => {
                    form.error = Some(format!(
                        "{}: {error}",
                        self.i18n.t("modals.new_connection.serial_load_ports_failed")
                    ));
                }
            }
        }
        cx.notify();
    }

    fn render_serial_form_branch(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(form) = self.new_connection_form.as_ref() else {
            return div().into_any_element();
        };
        let ports = form.serial_ports.clone();
        let serial_baud_rate_invalid = !form.serial_baud_rate.trim().is_empty()
            && !form
                .serial_baud_rate
                .trim()
                .parse::<u32>()
                .is_ok_and(|baud| baud > 0);
        div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.modal_section_gap))
            .child(
                div()
                    .rounded(px(self.tokens.radii.lg))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .bg(rgba(
                        (self.tokens.ui.bg << 8) | TAURI_SERIAL_PANEL_BG_ALPHA,
                    ))
                    .p(px(self.tokens.spacing.three))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t("modals.new_connection.serial_section_title")),
                    )
                    .child(
                        div()
                            .mt(px(self.tokens.spacing.one))
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("modals.new_connection.serial_connect_hint")),
                    ),
            )
            .child(self.render_serial_port_field(&ports, cx))
            .child(
                div()
                    .grid()
                    .grid_cols(2)
                    .gap(px(TAURI_SERIAL_GRID_GAP))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(self.tokens.spacing.two))
                            .child(self.render_connection_field(
                                self.i18n.t("modals.new_connection.serial_baud_rate"),
                                &form.serial_baud_rate,
                                "115200".to_string(),
                                NewConnectionField::SerialBaudRate,
                                false,
                                cx,
                            ))
                            .when(serial_baud_rate_invalid, |section| {
                                section.child(self.render_connection_hint_with_color(
                                    self.i18n
                                        .t("modals.new_connection.serial_invalid_baud_rate"),
                                    self.tokens.ui.error,
                                ))
                            }),
                    )
                    .child(self.render_serial_u8_select(
                        self.i18n.t("modals.new_connection.serial_data_bits"),
                        NewConnectionSelect::SerialDataBits,
                        &[(5, "5"), (6, "6"), (7, "7"), (8, "8")],
                        form.serial_data_bits,
                        cx,
                    )),
            )
            .child(
                div()
                    .grid()
                    .grid_cols(3)
                    .gap(px(TAURI_SERIAL_GRID_GAP))
                    .child(self.render_serial_u8_select(
                        self.i18n.t("modals.new_connection.serial_stop_bits"),
                        NewConnectionSelect::SerialStopBits,
                        &[(1, "1"), (2, "2")],
                        form.serial_stop_bits,
                        cx,
                    ))
                    .child(self.render_serial_parity_select(form.serial_parity, cx))
                    .child(self.render_serial_flow_select(form.serial_flow_control, cx)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(self.tokens.spacing.three))
                    .rounded(px(self.tokens.radii.lg))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .p(px(self.tokens.spacing.three))
                    .child(self.render_connection_checkbox(
                        self.i18n.t("modals.new_connection.save_serial_profile"),
                        form.save_serial_profile,
                        |form| form.save_serial_profile = !form.save_serial_profile,
                        cx,
                    ))
                    .when(form.save_serial_profile, |section| {
                        section.child(
                            div()
                                .pl(px(TAURI_SERIAL_PROFILE_NAME_INDENT))
                                .child(self.render_connection_field(
                                    self.i18n.t("modals.new_connection.serial_profile_name"),
                                    &form.serial_profile_name,
                                    self.i18n
                                        .t("modals.new_connection.serial_profile_name_placeholder"),
                                    NewConnectionField::SerialProfileName,
                                    false,
                                    cx,
                                )),
                        )
                    }),
            )
            .into_any_element()
    }

    fn render_serial_port_field(
        &self,
        ports: &[oxideterm_terminal::SerialPortInfo],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(form) = self.new_connection_form.as_ref() else {
            return div().into_any_element();
        };
        let loading = form.serial_ports_loading;
        let selected_port = form.serial_port_path.clone();
        let port_selector = if ports.is_empty() {
            self.render_connection_hint(if loading {
                self.i18n.t("modals.new_connection.serial_loading_ports")
            } else {
                self.i18n.t("modals.new_connection.serial_no_ports")
            })
        } else {
            let selected_label = ports
                .iter()
                .find(|port| port.port_path == selected_port)
                .map(serial_port_display_label)
                .unwrap_or_else(|| {
                    if selected_port.trim().is_empty() {
                        self.i18n
                            .t("modals.new_connection.serial_select_detected_port")
                    } else {
                        selected_port.clone()
                    }
                });
            // Tauri renders detected serial ports as a Radix Select below the
            // editable path input; keep manual entry and detected-choice paths separate.
            self.render_new_connection_select_control(
                NewConnectionSelect::SerialPort,
                selected_label,
                selected_port.trim().is_empty(),
                false,
                cx,
            )
        };

        div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.two))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(self.tokens.spacing.three))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(format!(
                                "{} *",
                                self.i18n.t("modals.new_connection.serial_port")
                            )),
                    )
                    .child(
                        self.workspace_toolbar_action_button(
                            self.i18n.t("modals.new_connection.serial_refresh_ports"),
                            Some(Self::render_lucide_icon(
                                if loading {
                                    LucideIcon::LoaderCircle
                                } else {
                                    LucideIcon::RefreshCw
                                },
                                14.0,
                                rgb(self.tokens.ui.text),
                            )),
                            ToolbarButtonOptions {
                                button: ButtonOptions {
                                    variant: ButtonVariant::Outline,
                                    size: ButtonSize::Sm,
                                    disabled: loading,
                                    ..ButtonOptions::default()
                                },
                                ..ToolbarButtonOptions::default()
                            },
                            cx.listener(|this, _event, _window, cx| {
                                this.refresh_serial_ports(cx);
                                cx.stop_propagation();
                            }),
                        ),
                    ),
            )
            .child(self.render_connection_input(
                &selected_port,
                self.i18n.t("modals.new_connection.serial_port_placeholder"),
                NewConnectionField::SerialPortPath,
                false,
                cx,
            ))
            .child(port_selector)
            .into_any_element()
    }

    fn render_new_connection_select_control(
        &self,
        select_id: NewConnectionSelect,
        value: String,
        placeholder: bool,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let anchor_id = Self::new_connection_select_anchor_id(select_id);
        let workspace = cx.entity();
        let trigger = self
            .new_connection_select_trigger(select_id, value, placeholder, disabled)
            .when(!disabled, |trigger| {
                trigger.cursor_pointer().on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, window, cx| {
                        if let Some(form) = this.new_connection_form.as_mut() {
                            form.field_focused = false;
                            form.selected_field = None;
                        }
                        this.ime_marked_text = None;
                        this.open_new_connection_select_from_pointer(select_id);
                        window.focus(&this.focus_handle);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
            });

        select_anchor_probe(anchor_id, trigger, move |anchor, _window, cx| {
            let _ = workspace.update(cx, |this, cx| {
                this.update_select_anchor(anchor, cx);
            });
        })
        .into_any_element()
    }

    fn render_serial_u8_select(
        &self,
        label: String,
        select_id: NewConnectionSelect,
        choices: &[(u8, &'static str)],
        selected: u8,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected_label = choices
            .iter()
            .find(|(value, _)| *value == selected)
            .map(|(_, option_label)| (*option_label).to_string())
            .unwrap_or_else(|| selected.to_string());
        // Tauri serial numeric choices are Select controls, not segmented tabs.
        form_field(
            &self.tokens,
            label,
            self.render_new_connection_select_control(select_id, selected_label, false, false, cx),
        )
        .into_any_element()
    }

    fn render_serial_parity_select(
        &self,
        selected: oxideterm_terminal::SerialParity,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        form_field(
            &self.tokens,
            self.i18n.t("modals.new_connection.serial_parity"),
            self.render_new_connection_select_control(
                NewConnectionSelect::SerialParity,
                self.serial_parity_label(selected),
                false,
                false,
                cx,
            ),
        )
        .into_any_element()
    }

    fn render_serial_flow_select(
        &self,
        selected: oxideterm_terminal::SerialFlowControl,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        form_field(
            &self.tokens,
            self.i18n.t("modals.new_connection.serial_flow_control"),
            self.render_new_connection_select_control(
                NewConnectionSelect::SerialFlowControl,
                self.serial_flow_control_label(selected),
                false,
                false,
                cx,
            ),
        )
        .into_any_element()
    }

    fn upstream_proxy_policy_label(&self, policy: NewConnectionUpstreamProxyPolicy) -> String {
        let key = match policy {
            NewConnectionUpstreamProxyPolicy::UseGlobal => "modals.upstream_proxy.use_global",
            NewConnectionUpstreamProxyPolicy::Direct => "modals.upstream_proxy.direct",
            NewConnectionUpstreamProxyPolicy::Custom => "modals.upstream_proxy.custom",
        };
        self.i18n.t(key)
    }

    fn upstream_proxy_protocol_label(&self, protocol: SavedUpstreamProxyProtocol) -> String {
        let key = match protocol {
            SavedUpstreamProxyProtocol::Socks5 => "settings_view.network.protocol_socks5",
            SavedUpstreamProxyProtocol::HttpConnect => "settings_view.network.protocol_http_connect",
        };
        self.i18n.t(key)
    }

    fn upstream_proxy_auth_label(&self, auth: NewConnectionUpstreamProxyAuth) -> String {
        let key = match auth {
            NewConnectionUpstreamProxyAuth::None => "settings_view.network.auth_none",
            NewConnectionUpstreamProxyAuth::Password => "settings_view.network.auth_password",
        };
        self.i18n.t(key)
    }

    fn render_upstream_proxy_policy_section(
        &self,
        form: &NewConnectionForm,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let custom = form.upstream_proxy_policy == NewConnectionUpstreamProxyPolicy::Custom;
        div()
            .flex()
            .flex_col()
            .gap_4()
            .border_t_1()
            .border_color(rgb(self.tokens.ui.border))
            .pt_4()
            .child(form_field(
                &self.tokens,
                self.i18n.t("modals.upstream_proxy.policy"),
                self.render_new_connection_select_control(
                    NewConnectionSelect::UpstreamProxyPolicy,
                    self.upstream_proxy_policy_label(form.upstream_proxy_policy),
                    false,
                    false,
                    cx,
                ),
            ))
            .child(self.render_connection_hint(
                self.i18n.t("modals.upstream_proxy.policy_hint"),
            ))
            .when(custom, |content| {
                content
                    .child(
                        div()
                            .flex()
                            .gap_4()
                            .child(div().flex_1().child(form_field(
                                &self.tokens,
                                self.i18n.t("settings_view.network.protocol"),
                                self.render_new_connection_select_control(
                                    NewConnectionSelect::UpstreamProxyProtocol,
                                    self.upstream_proxy_protocol_label(form.upstream_proxy_protocol),
                                    false,
                                    false,
                                    cx,
                                ),
                            )))
                            .child(
                                div()
                                    .w(px(self.tokens.metrics.form_port_width))
                                    .child(self.render_connection_field(
                                        self.i18n.t("settings_view.network.port"),
                                        &form.upstream_proxy_port,
                                        "1080".to_string(),
                                        NewConnectionField::UpstreamProxyPort,
                                        false,
                                        cx,
                                    )),
                            ),
                    )
                    .child(self.render_connection_field(
                        self.i18n.t("settings_view.network.host"),
                        &form.upstream_proxy_host,
                        "127.0.0.1".to_string(),
                        NewConnectionField::UpstreamProxyHost,
                        false,
                        cx,
                    ))
                    .child(self.render_connection_field(
                        self.i18n.t("settings_view.network.no_proxy"),
                        &form.upstream_proxy_no_proxy,
                        "localhost,127.0.0.1,*.internal".to_string(),
                        NewConnectionField::UpstreamProxyNoProxy,
                        false,
                        cx,
                    ))
                    .child(self.render_connection_checkbox(
                        self.i18n.t("settings_view.network.remote_dns"),
                        form.upstream_proxy_remote_dns,
                        |form| form.upstream_proxy_remote_dns = !form.upstream_proxy_remote_dns,
                        cx,
                    ))
                    .child(form_field(
                        &self.tokens,
                        self.i18n.t("settings_view.network.auth"),
                        self.render_new_connection_select_control(
                            NewConnectionSelect::UpstreamProxyAuth,
                            self.upstream_proxy_auth_label(form.upstream_proxy_auth),
                            false,
                            false,
                            cx,
                        ),
                    ))
                    .when(form.upstream_proxy_auth == NewConnectionUpstreamProxyAuth::Password, |content| {
                        content
                            .child(self.render_connection_field(
                                self.i18n.t("settings_view.network.username"),
                                &form.upstream_proxy_username,
                                String::new(),
                                NewConnectionField::UpstreamProxyUsername,
                                false,
                                cx,
                            ))
                            .child(self.render_connection_field(
                                self.i18n.t("settings_view.network.password"),
                                &form.upstream_proxy_password,
                                String::new(),
                                NewConnectionField::UpstreamProxyPassword,
                                true,
                                cx,
                            ))
                            .child(self.render_connection_hint(
                                self.i18n.t("settings_view.network.password_hint"),
                            ))
                    })
            })
            .into_any_element()
    }

    fn serial_parity_label(&self, parity: oxideterm_terminal::SerialParity) -> String {
        match parity {
            oxideterm_terminal::SerialParity::None => {
                self.i18n.t("modals.new_connection.serial_parity_none")
            }
            oxideterm_terminal::SerialParity::Odd => {
                self.i18n.t("modals.new_connection.serial_parity_odd")
            }
            oxideterm_terminal::SerialParity::Even => {
                self.i18n.t("modals.new_connection.serial_parity_even")
            }
        }
    }

    fn serial_flow_control_label(&self, flow: oxideterm_terminal::SerialFlowControl) -> String {
        match flow {
            oxideterm_terminal::SerialFlowControl::None => {
                self.i18n.t("modals.new_connection.serial_flow_none")
            }
            oxideterm_terminal::SerialFlowControl::Software => {
                self.i18n.t("modals.new_connection.serial_flow_software")
            }
            oxideterm_terminal::SerialFlowControl::Hardware => {
                self.i18n.t("modals.new_connection.serial_flow_hardware")
            }
        }
    }

    fn set_new_connection_serial_port(&mut self, port_path: String, cx: &mut Context<Self>) {
        if let Some(form) = self.new_connection_form.as_mut() {
            form.serial_port_path = port_path;
            form.focused_field = NewConnectionField::SerialPortPath;
            form.field_focused = false;
            clear_connection_selection(form);
            form.error = None;
        }
        self.close_new_connection_select();
        self.ime_marked_text = None;
        cx.notify();
    }

    fn set_new_connection_serial_u8(
        &mut self,
        select_id: NewConnectionSelect,
        value: u8,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = self.new_connection_form.as_mut() {
            match select_id {
                NewConnectionSelect::SerialDataBits => form.serial_data_bits = value,
                NewConnectionSelect::SerialStopBits => form.serial_stop_bits = value,
                _ => return,
            }
            form.field_focused = false;
            clear_connection_selection(form);
            form.error = None;
        }
        self.close_new_connection_select();
        self.ime_marked_text = None;
        cx.notify();
    }

    fn set_new_connection_serial_parity(
        &mut self,
        parity: oxideterm_terminal::SerialParity,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = self.new_connection_form.as_mut() {
            form.serial_parity = parity;
            form.field_focused = false;
            clear_connection_selection(form);
            form.error = None;
        }
        self.close_new_connection_select();
        self.ime_marked_text = None;
        cx.notify();
    }

    fn set_new_connection_serial_flow_control(
        &mut self,
        flow: oxideterm_terminal::SerialFlowControl,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = self.new_connection_form.as_mut() {
            form.serial_flow_control = flow;
            form.field_focused = false;
            clear_connection_selection(form);
            form.error = None;
        }
        self.close_new_connection_select();
        self.ime_marked_text = None;
        cx.notify();
    }

    fn set_new_connection_upstream_proxy_policy(
        &mut self,
        policy: NewConnectionUpstreamProxyPolicy,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = self.new_connection_form.as_mut() {
            form.upstream_proxy_policy = policy;
            form.field_focused = false;
            clear_connection_selection(form);
            form.error = None;
        }
        self.close_new_connection_select();
        self.ime_marked_text = None;
        cx.notify();
    }

    fn set_new_connection_upstream_proxy_protocol(
        &mut self,
        protocol: SavedUpstreamProxyProtocol,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = self.new_connection_form.as_mut() {
            form.upstream_proxy_protocol = protocol;
            form.field_focused = false;
            clear_connection_selection(form);
            form.error = None;
        }
        self.close_new_connection_select();
        self.ime_marked_text = None;
        cx.notify();
    }

    fn set_new_connection_upstream_proxy_auth(
        &mut self,
        auth: NewConnectionUpstreamProxyAuth,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = self.new_connection_form.as_mut() {
            if auth == NewConnectionUpstreamProxyAuth::None {
                // Hidden password fields should not retain a draft secret after
                // switching the custom proxy back to unauthenticated mode.
                form.upstream_proxy_password.clear();
                form.upstream_proxy_password_keychain_id = None;
            }
            form.upstream_proxy_auth = auth;
            form.field_focused = false;
            clear_connection_selection(form);
            form.error = None;
        }
        self.close_new_connection_select();
        self.ime_marked_text = None;
        cx.notify();
    }

    fn render_connection_checkbox(
        &self,
        label: String,
        checked: bool,
        toggle: fn(&mut NewConnectionForm),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        checkbox(&self.tokens, label, checked)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if let Some(form) = this.new_connection_form.as_mut() {
                        toggle(form);
                    }
                    this.close_new_connection_select();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn render_connection_button(
        &self,
        label: String,
        primary: bool,
        action: ConnectionButtonAction,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // NewConnectionModal footer uses shadcn Button variants. Route native
        // footer buttons through the shared toolbar primitive while keeping the
        // existing form action dispatch unchanged.
        self.workspace_toolbar_action_button(
            label,
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: if primary {
                        ButtonVariant::Default
                    } else {
                        ButtonVariant::Secondary
                    },
                    disabled,
                    ..ButtonOptions::default()
                },
                ..ToolbarButtonOptions::default()
            },
            cx.listener(move |this, _event, window, cx| match action {
                ConnectionButtonAction::Cancel => {
                    this.close_new_connection_form(window, cx);
                }
                ConnectionButtonAction::Test => {
                    this.start_new_connection_flow(SshConnectionIntent::Test, window, cx);
                }
                ConnectionButtonAction::Connect => {
                    this.submit_new_connection_form_with_action(
                        NewConnectionSubmitAction::Connect,
                        window,
                        cx,
                    );
                }
                ConnectionButtonAction::Save => {
                    this.submit_new_connection_form_with_action(
                        NewConnectionSubmitAction::Save,
                        window,
                        cx,
                    );
                }
                ConnectionButtonAction::SaveAndConnect => {
                    this.submit_new_connection_form_with_action(
                        NewConnectionSubmitAction::SaveAndConnect,
                        window,
                        cx,
                    );
                }
            }),
        )
            .into_any_element()
    }
}

fn serial_port_display_label(port: &oxideterm_terminal::SerialPortInfo) -> String {
    if port.display_name.trim().is_empty() {
        port.port_path.clone()
    } else {
        port.display_name.clone()
    }
}

fn parse_form_hex_color(value: &str) -> Option<u32> {
    let trimmed = value.trim().trim_start_matches('#');
    if trimmed.len() != 6 {
        return None;
    }
    u32::from_str_radix(trimmed, 16).ok()
}
