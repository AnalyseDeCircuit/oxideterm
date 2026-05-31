impl WorkspaceApp {
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
                NewConnectionSelect::Group => return,
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
                    cx.notify();
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
        let mut row = segmented_tabs(&self.tokens);
        for (tab, key) in tabs {
            let selected = tab == active_tab
                || (edit_properties_mode
                    && tab == SshAuthTab::SshKey
                    && active_tab == SshAuthTab::DefaultKey);
            let disabled = tab == SshAuthTab::TwoFactor && kbi_disabled_for_proxy_chain;
            let item = segmented_tab(&self.tokens, self.i18n.t(key), selected)
                .opacity(if disabled { 0.45 } else { 1.0 });
            row = row.child(if disabled {
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
            });
        }
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
                    .child(self.render_serial_u8_tabs(
                        self.i18n.t("modals.new_connection.serial_data_bits"),
                        &[(5, "5"), (6, "6"), (7, "7"), (8, "8")],
                        form.serial_data_bits,
                        |form, value| form.serial_data_bits = value,
                        cx,
                    )),
            )
            .child(
                div()
                    .grid()
                    .grid_cols(3)
                    .gap(px(TAURI_SERIAL_GRID_GAP))
                    .child(self.render_serial_u8_tabs(
                        self.i18n.t("modals.new_connection.serial_stop_bits"),
                        &[(1, "1"), (2, "2")],
                        form.serial_stop_bits,
                        |form, value| form.serial_stop_bits = value,
                        cx,
                    ))
                    .child(self.render_serial_parity_tabs(form.serial_parity, cx))
                    .child(self.render_serial_flow_tabs(form.serial_flow_control, cx)),
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
        let mut port_list = div().flex().flex_col().gap(px(self.tokens.spacing.two));
        if ports.is_empty() {
            port_list = port_list.child(self.render_connection_hint(if loading {
                self.i18n.t("modals.new_connection.serial_loading_ports")
            } else {
                self.i18n.t("modals.new_connection.serial_no_ports")
            }));
        } else {
            for port in ports.iter().cloned() {
                let active = port.port_path == selected_port;
                let meta = [port.manufacturer.clone(), port.product.clone()]
                    .into_iter()
                    .flatten()
                    .filter(|part| !part.trim().is_empty())
                    .collect::<Vec<_>>()
                    .join(" · ");
                let port_path = port.port_path.clone();
                port_list = port_list.child(
                    div()
                        .rounded(px(self.tokens.radii.md))
                        .border_1()
                        .border_color(if active {
                            rgb(self.tokens.ui.accent)
                        } else {
                            rgb(self.tokens.ui.border)
                        })
                        .bg(if active {
                            rgba(
                                (self.tokens.ui.accent << 8) | TAURI_SERIAL_SELECTED_BG_ALPHA,
                            )
                        } else {
                            rgba(
                                (self.tokens.ui.bg_secondary << 8) | TAURI_SERIAL_PANEL_BG_ALPHA,
                            )
                        })
                        .px(px(self.tokens.spacing.three))
                        .py(px(self.tokens.spacing.two))
                        .cursor_pointer()
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                .text_color(rgb(self.tokens.ui.text))
                                .child(port.display_name.clone()),
                        )
                        .when(!meta.is_empty(), |row| {
                            row.child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .child(meta),
                            )
                        })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                if let Some(form) = this.new_connection_form.as_mut() {
                                    form.serial_port_path = port_path.clone();
                                    form.focused_field = NewConnectionField::SerialPortPath;
                                    form.field_focused = false;
                                    clear_connection_selection(form);
                                }
                                this.close_new_connection_select();
                                cx.notify();
                            }),
                        ),
                );
            }
        }

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
            .child(port_list)
            .into_any_element()
    }

    fn render_serial_u8_tabs(
        &self,
        label: String,
        choices: &[(u8, &'static str)],
        selected: u8,
        update: fn(&mut NewConnectionForm, u8),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut row = segmented_tabs(&self.tokens);
        for (value, option_label) in choices.iter().copied() {
            row = row.child(
                segmented_tab(&self.tokens, option_label.to_string(), value == selected)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            if let Some(form) = this.new_connection_form.as_mut() {
                                update(form, value);
                                clear_connection_selection(form);
                            }
                            this.close_new_connection_select();
                            cx.notify();
                        }),
                    ),
            );
        }
        form_field(&self.tokens, label, row).into_any_element()
    }

    fn render_serial_parity_tabs(
        &self,
        selected: oxideterm_terminal::SerialParity,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let choices = [
            (
                oxideterm_terminal::SerialParity::None,
                self.i18n.t("modals.new_connection.serial_parity_none"),
            ),
            (
                oxideterm_terminal::SerialParity::Odd,
                self.i18n.t("modals.new_connection.serial_parity_odd"),
            ),
            (
                oxideterm_terminal::SerialParity::Even,
                self.i18n.t("modals.new_connection.serial_parity_even"),
            ),
        ];
        let mut row = segmented_tabs(&self.tokens);
        for (value, label) in choices {
            row = row.child(
                segmented_tab(&self.tokens, label, value == selected).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        if let Some(form) = this.new_connection_form.as_mut() {
                            form.serial_parity = value;
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
            self.i18n.t("modals.new_connection.serial_parity"),
            row,
        )
        .into_any_element()
    }

    fn render_serial_flow_tabs(
        &self,
        selected: oxideterm_terminal::SerialFlowControl,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let choices = [
            (
                oxideterm_terminal::SerialFlowControl::None,
                self.i18n.t("modals.new_connection.serial_flow_none"),
            ),
            (
                oxideterm_terminal::SerialFlowControl::Software,
                self.i18n.t("modals.new_connection.serial_flow_software"),
            ),
            (
                oxideterm_terminal::SerialFlowControl::Hardware,
                self.i18n.t("modals.new_connection.serial_flow_hardware"),
            ),
        ];
        let mut row = segmented_tabs(&self.tokens);
        for (value, label) in choices {
            row = row.child(
                segmented_tab(&self.tokens, label, value == selected).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        if let Some(form) = this.new_connection_form.as_mut() {
                            form.serial_flow_control = value;
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
            self.i18n.t("modals.new_connection.serial_flow_control"),
            row,
        )
        .into_any_element()
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
                    this.submit_new_connection_form(window, cx);
                }
                ConnectionButtonAction::Save => {
                    this.submit_new_connection_form(window, cx);
                }
            }),
        )
            .into_any_element()
    }
}

fn parse_form_hex_color(value: &str) -> Option<u32> {
    let trimmed = value.trim().trim_start_matches('#');
    if trimmed.len() != 6 {
        return None;
    }
    u32::from_str_radix(trimmed, 16).ok()
}
