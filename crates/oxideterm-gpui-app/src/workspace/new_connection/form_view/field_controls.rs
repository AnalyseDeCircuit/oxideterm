use super::*;
use oxideterm_settings_model::parse_rgb24_hex;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AuthSelectorContext {
    Standard,
    EditProperties,
    Prompt,
    DrillDown,
    Jump,
}

impl WorkspaceApp {
    pub(super) fn new_connection_select_anchor_id(
        select_id: NewConnectionSelect,
    ) -> SelectAnchorId {
        match select_id {
            NewConnectionSelect::Group => SelectAnchorId::NewConnectionGroup,
            NewConnectionSelect::KeyAuthSource => SelectAnchorId::NewConnectionKeyAuthSource,
            NewConnectionSelect::ManagedKey => SelectAnchorId::NewConnectionManagedKey,
            NewConnectionSelect::JumpSavedConnection => {
                SelectAnchorId::NewConnectionJumpSavedConnection
            }
            NewConnectionSelect::JumpKeyAuthSource => {
                SelectAnchorId::NewConnectionJumpKeyAuthSource
            }
            NewConnectionSelect::JumpManagedKey => SelectAnchorId::NewConnectionJumpManagedKey,
            NewConnectionSelect::UpstreamProxyPolicy => {
                SelectAnchorId::NewConnectionUpstreamProxyPolicy
            }
            NewConnectionSelect::UpstreamProxyProtocol => {
                SelectAnchorId::NewConnectionUpstreamProxyProtocol
            }
            NewConnectionSelect::UpstreamProxyAuth => {
                SelectAnchorId::NewConnectionUpstreamProxyAuth
            }
            NewConnectionSelect::SerialPort => SelectAnchorId::NewConnectionSerialPort,
            NewConnectionSelect::SerialDataBits => SelectAnchorId::NewConnectionSerialDataBits,
            NewConnectionSelect::SerialStopBits => SelectAnchorId::NewConnectionSerialStopBits,
            NewConnectionSelect::SerialParity => SelectAnchorId::NewConnectionSerialParity,
            NewConnectionSelect::SerialFlowControl => {
                SelectAnchorId::NewConnectionSerialFlowControl
            }
            NewConnectionSelect::RawTcpLineEnding => SelectAnchorId::NewConnectionRawTcpLineEnding,
            NewConnectionSelect::RawTcpDisplayMode => {
                SelectAnchorId::NewConnectionRawTcpDisplayMode
            }
            NewConnectionSelect::RawTcpSendMode => SelectAnchorId::NewConnectionRawTcpSendMode,
            NewConnectionSelect::RawTcpTlsMode => SelectAnchorId::NewConnectionRawTcpTlsMode,
            NewConnectionSelect::RawTcpTlsVerification => {
                SelectAnchorId::NewConnectionRawTcpTlsVerification
            }
            NewConnectionSelect::RawUdpLineEnding => SelectAnchorId::NewConnectionRawUdpLineEnding,
            NewConnectionSelect::RawUdpDisplayMode => {
                SelectAnchorId::NewConnectionRawUdpDisplayMode
            }
            NewConnectionSelect::RawUdpSendMode => SelectAnchorId::NewConnectionRawUdpSendMode,
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
            browser_behavior::browser_focus_visible(
                focused,
                self.new_connection_select_focus_origin,
            ),
        )
    }

    fn open_new_connection_select_from_pointer(
        &mut self,
        select_id: NewConnectionSelect,
        _cx: &mut Context<Self>,
    ) {
        // New-connection selects share browser focus-origin semantics with
        // settings selects: pointer-opened menus should not render a keyboard
        // focus-visible ring on the trigger.
        if self.open_new_connection_select == Some(select_id) {
            self.close_new_connection_select();
            return;
        }
        self.open_new_connection_select = Some(select_id);
        self.new_connection_select_focus_origin =
            Some(browser_behavior::BrowserFocusOrigin::Pointer);
    }

    pub(in crate::workspace) fn close_new_connection_select(&mut self) {
        browser_behavior::close_browser_trigger_select(
            &mut self.open_new_connection_select,
            &mut self.new_connection_select_focus_origin,
        );
    }

    pub(super) fn clear_new_connection_select_anchor(&mut self) {
        // The group select overlay is anchored inside the new-connection scroll
        // body. Drop its cached bounds when the body scrolls so a reopened
        // overlay cannot reuse pre-scroll coordinates.
        self.select_anchors
            .remove(&SelectAnchorId::NewConnectionGroup);
        self.select_anchors
            .remove(&SelectAnchorId::NewConnectionKeyAuthSource);
        self.select_anchors
            .remove(&SelectAnchorId::NewConnectionManagedKey);
        self.select_anchors
            .remove(&SelectAnchorId::NewConnectionJumpSavedConnection);
        self.select_anchors
            .remove(&SelectAnchorId::NewConnectionJumpKeyAuthSource);
        self.select_anchors
            .remove(&SelectAnchorId::NewConnectionJumpManagedKey);
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
        self.select_anchors
            .remove(&SelectAnchorId::NewConnectionRawTcpLineEnding);
        self.select_anchors
            .remove(&SelectAnchorId::NewConnectionRawTcpDisplayMode);
        self.select_anchors
            .remove(&SelectAnchorId::NewConnectionRawTcpSendMode);
        self.select_anchors
            .remove(&SelectAnchorId::NewConnectionRawTcpTlsMode);
        self.select_anchors
            .remove(&SelectAnchorId::NewConnectionRawTcpTlsVerification);
        self.select_anchors
            .remove(&SelectAnchorId::NewConnectionRawUdpLineEnding);
        self.select_anchors
            .remove(&SelectAnchorId::NewConnectionRawUdpDisplayMode);
        self.select_anchors
            .remove(&SelectAnchorId::NewConnectionRawUdpSendMode);
    }

    pub(super) fn render_connection_hint(&self, text: String) -> AnyElement {
        self.render_connection_hint_with_color(text, self.tokens.ui.text_muted)
    }

    pub(super) fn render_connection_hint_with_color(&self, text: String, color: u32) -> AnyElement {
        div()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(color))
            .child(text)
            .into_any_element()
    }

    pub(super) fn render_agent_status(&self, available: Option<bool>) -> AnyElement {
        let (color, label) = match available {
            Some(true) => (
                self.tokens.ui.success,
                self.i18n.t("ssh.form.agent_detected"),
            ),
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

    pub(super) fn render_prompt_error_box(&self, error: String) -> AnyElement {
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

    pub(super) fn render_connection_field(
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

    pub(super) fn render_edit_saved_password_field(
        &self,
        form: &NewConnectionForm,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let value = if form.password_loaded {
            form.password.as_str()
        } else {
            ""
        };
        let icon = if form.password_visible {
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
                    if form.password_loading {
                        oxideterm_gpui_ui::button::icon_button(
                            &self.tokens,
                            self.render_loading_icon(
                                "saved-password-loading",
                                TAURI_PASSWORD_ICON_SIZE,
                                rgb(self.tokens.ui.text_muted),
                            ),
                            IconButtonOptions {
                                loading: true,
                                hover_background: Some(rgba((self.tokens.ui.bg_hover << 8) | 0x99)),
                                ..IconButtonOptions::opaque_toolbar(
                                    TAURI_PASSWORD_ICON_BUTTON_SIZE,
                                    ButtonRadius::Sm,
                                )
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            |_event, _window, cx| {
                                cx.stop_propagation();
                            },
                        )
                    } else {
                        self.workspace_icon_action_button(
                            icon,
                            TAURI_PASSWORD_ICON_SIZE,
                            rgb(self.tokens.ui.text_muted),
                            IconButtonOptions {
                                hover_background: Some(rgba((self.tokens.ui.bg_hover << 8) | 0x99)),
                                // Tauri places the reveal affordance inside the
                                // password input as an icon-only toolbar button.
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
                    }
                    .absolute()
                    .right(px(TAURI_PASSWORD_ICON_BUTTON_OFFSET))
                    .top(px(TAURI_PASSWORD_ICON_BUTTON_OFFSET)),
                ),
        )
    }

    pub(super) fn render_connection_field_with_browse(
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

    pub(super) fn render_connection_group_select(
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
                    this.open_new_connection_select_from_pointer(NewConnectionSelect::Group, cx);
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

    pub(super) fn set_new_connection_group(&mut self, group: String, cx: &mut Context<Self>) {
        if let Some(form) = self.new_connection_form.as_mut() {
            form.group = group;
            form.field_focused = false;
            form.selected_field = None;
            form.error = None;
        }
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn render_managed_key_select(
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
                    this.open_new_connection_select_from_pointer(select_id, cx);
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

    pub(super) fn render_jump_saved_connection_select(
        &self,
        selected_id: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let connections = self.connection_store.connection_infos();
        let selected_label = if selected_id.trim().is_empty() {
            self.i18n.t("ssh.form.proxy_jump_saved_connection_custom")
        } else {
            connections
                .iter()
                .find(|connection| connection.id == selected_id)
                .map(|connection| {
                    format!(
                        "{} · {}@{}:{}",
                        connection.name, connection.username, connection.host, connection.port
                    )
                })
                .unwrap_or_else(|| selected_id.to_string())
        };
        let workspace = cx.entity();
        let trigger = self
            .new_connection_select_trigger(
                NewConnectionSelect::JumpSavedConnection,
                selected_label,
                selected_id.trim().is_empty(),
                false,
            )
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    if let Some(form) = this.new_connection_form.as_mut() {
                        form.field_focused = false;
                        form.selected_field = None;
                    }
                    this.ime_marked_text = None;
                    this.open_new_connection_select_from_pointer(
                        NewConnectionSelect::JumpSavedConnection,
                        cx,
                    );
                    window.focus(&this.focus_handle);
                    cx.stop_propagation();
                    cx.notify();
                }),
            );

        form_field(
            &self.tokens,
            self.i18n.t("ssh.form.proxy_jump_saved_connection"),
            select_anchor_probe(
                SelectAnchorId::NewConnectionJumpSavedConnection,
                trigger,
                move |anchor, _window, cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        this.update_select_anchor(anchor, cx);
                    });
                },
            ),
        )
        .into_any_element()
    }

    pub(super) fn set_new_connection_managed_key(
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
                | NewConnectionSelect::KeyAuthSource
                | NewConnectionSelect::JumpSavedConnection
                | NewConnectionSelect::JumpKeyAuthSource
                | NewConnectionSelect::UpstreamProxyPolicy
                | NewConnectionSelect::UpstreamProxyProtocol
                | NewConnectionSelect::UpstreamProxyAuth
                | NewConnectionSelect::SerialPort
                | NewConnectionSelect::SerialDataBits
                | NewConnectionSelect::SerialStopBits
                | NewConnectionSelect::SerialParity
                | NewConnectionSelect::SerialFlowControl
                | NewConnectionSelect::RawTcpLineEnding
                | NewConnectionSelect::RawTcpDisplayMode
                | NewConnectionSelect::RawTcpSendMode
                | NewConnectionSelect::RawTcpTlsMode
                | NewConnectionSelect::RawTcpTlsVerification
                | NewConnectionSelect::RawUdpLineEnding
                | NewConnectionSelect::RawUdpDisplayMode
                | NewConnectionSelect::RawUdpSendMode => return,
            }
            form.field_focused = false;
            form.selected_field = None;
            form.error = None;
        }
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn clear_new_connection_jump_saved_connection(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = self.new_connection_form.as_mut() {
            if let Some(jump_form) = form.jump_server_form.as_mut() {
                jump_form.saved_connection_id.clear();
            }
            form.field_focused = false;
            form.selected_field = None;
            form.error = None;
        }
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn set_new_connection_jump_saved_connection(
        &mut self,
        connection_id: String,
        cx: &mut Context<Self>,
    ) {
        let selected_connection = self
            .connection_store
            .connection_infos()
            .into_iter()
            .find(|connection| connection.id == connection_id);
        if let (Some(form), Some(connection)) = (
            self.new_connection_form.as_mut(),
            selected_connection.as_ref(),
        ) {
            if let Some(jump_form) = form.jump_server_form.as_mut() {
                jump_form.apply_saved_connection(connection);
            }
            form.field_focused = false;
            form.selected_field = None;
            form.error = None;
        }
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn connection_form_group_options(&self, current_group: &str) -> Vec<String> {
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

    pub(super) fn connection_form_group_is_ungrouped(&self, group: &str) -> bool {
        let group = group.trim();
        group.is_empty()
            || group == "Ungrouped"
            || group == "未分组"
            || group == self.i18n.t("ssh.form.ungrouped")
            || group == self.i18n.t("sessionManager.edit_properties.ungrouped")
    }

    pub(super) fn connection_form_ungrouped_label(&self) -> String {
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
            .on_mouse_move(cx.listener(
                |this, event: &gpui::MouseMoveEvent, window, cx| {
                    this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                },
            )),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    pub(super) fn render_auth_selector(
        &self,
        active_tab: SshAuthTab,
        context: AuthSelectorContext,
        jump_form: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active_family = auth_family_from_tab(active_tab);
        let label = match context {
            AuthSelectorContext::EditProperties => {
                self.i18n.t("sessionManager.edit_properties.auth_type")
            }
            AuthSelectorContext::DrillDown => self.i18n.t("ssh.drill_down.auth_method"),
            AuthSelectorContext::Jump => self.i18n.t("ssh.form.proxy_jump_auth"),
            AuthSelectorContext::Standard | AuthSelectorContext::Prompt => {
                self.i18n.t("ssh.form.authentication")
            }
        };
        let mut row = segmented_tabs(&self.tokens);
        for (family, label_key) in Self::auth_family_choices(context) {
            row = row.child(
                segmented_tab(
                    &self.tokens,
                    self.i18n.t(label_key),
                    *family == active_family,
                )
                .min_h(px(self.tokens.metrics.ui_tabs_list_height))
                .whitespace_normal()
                .text_align(gpui::TextAlign::Center)
                .line_height(px(self.tokens.metrics.ui_text_sm + 2.0))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.set_new_connection_auth_family(*family, context, jump_form, cx);
                    }),
                ),
            );
        }

        div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.three))
            .child(form_field(&self.tokens, label, row))
            .when(
                active_family == SshAuthFamily::Key && context != AuthSelectorContext::DrillDown,
                |content| {
                    content.child(
                        self.render_key_auth_source_select(active_tab, context, jump_form, cx),
                    )
                },
            )
            .into_any_element()
    }

    fn auth_family_choices(
        context: AuthSelectorContext,
    ) -> &'static [(SshAuthFamily, &'static str)] {
        match context {
            AuthSelectorContext::DrillDown => &[
                (SshAuthFamily::Agent, "ssh.drill_down.auth_agent"),
                (SshAuthFamily::Key, "ssh.drill_down.auth_key"),
                (SshAuthFamily::Password, "ssh.drill_down.auth_password"),
            ],
            AuthSelectorContext::Jump => &[
                (SshAuthFamily::Password, "ssh.auth.password"),
                (SshAuthFamily::Key, "ssh.auth.key"),
                (SshAuthFamily::Agent, "ssh.auth.agent"),
            ],
            AuthSelectorContext::EditProperties | AuthSelectorContext::Prompt => &[
                (SshAuthFamily::Password, "ssh.auth.password"),
                (SshAuthFamily::Key, "ssh.auth.key"),
                (SshAuthFamily::Agent, "ssh.auth.agent"),
            ],
            AuthSelectorContext::Standard => &[
                (SshAuthFamily::Password, "ssh.auth.password"),
                (SshAuthFamily::Key, "ssh.auth.key"),
                (SshAuthFamily::Agent, "ssh.auth.agent"),
                (SshAuthFamily::TwoFactor, "ssh.auth.two_factor"),
            ],
        }
    }

    pub(super) fn key_auth_source_choices(
        context: AuthSelectorContext,
    ) -> &'static [SshKeyAuthSource] {
        match context {
            AuthSelectorContext::Standard | AuthSelectorContext::Jump => &[
                SshKeyAuthSource::DefaultKey,
                SshKeyAuthSource::SshKey,
                SshKeyAuthSource::ManagedKey,
                SshKeyAuthSource::Certificate,
            ],
            AuthSelectorContext::EditProperties | AuthSelectorContext::Prompt => &[
                SshKeyAuthSource::SshKey,
                SshKeyAuthSource::ManagedKey,
                SshKeyAuthSource::Certificate,
            ],
            AuthSelectorContext::DrillDown => &[SshKeyAuthSource::SshKey],
        }
    }

    pub(super) fn current_main_auth_selector_context(&self) -> AuthSelectorContext {
        let mode = new_connection_form_mode(
            self.editing_saved_connection_id.as_deref(),
            self.duplicating_saved_connection_id.as_deref(),
            self.saved_connection_prompt_action,
        );
        if self.drill_down_parent_node_id.is_some() {
            AuthSelectorContext::DrillDown
        } else if mode == NewConnectionFormMode::SavedConnectionPrompt {
            AuthSelectorContext::Prompt
        } else if mode == NewConnectionFormMode::EditProperties {
            AuthSelectorContext::EditProperties
        } else {
            AuthSelectorContext::Standard
        }
    }

    pub(super) fn key_auth_source_label(&self, source: SshKeyAuthSource) -> String {
        let key = match source {
            SshKeyAuthSource::DefaultKey => "ssh.auth.key_source_default",
            SshKeyAuthSource::SshKey => "ssh.auth.key_source_file",
            SshKeyAuthSource::ManagedKey => "ssh.auth.key_source_managed",
            SshKeyAuthSource::Certificate => "ssh.auth.key_source_certificate",
        };
        self.i18n.t(key)
    }

    pub(super) fn normalized_key_source_for_context(
        active_tab: SshAuthTab,
        context: AuthSelectorContext,
    ) -> SshKeyAuthSource {
        let choices = Self::key_auth_source_choices(context);
        let source = key_source_from_tab(active_tab).unwrap_or(SshKeyAuthSource::SshKey);
        if choices.contains(&source) {
            source
        } else {
            SshKeyAuthSource::SshKey
        }
    }

    fn render_key_auth_source_select(
        &self,
        active_tab: SshAuthTab,
        context: AuthSelectorContext,
        jump_form: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let source = Self::normalized_key_source_for_context(active_tab, context);
        let select_id = if jump_form {
            NewConnectionSelect::JumpKeyAuthSource
        } else {
            NewConnectionSelect::KeyAuthSource
        };
        let anchor_id = Self::new_connection_select_anchor_id(select_id);
        let workspace = cx.entity();
        let trigger = self
            .new_connection_select_trigger(
                select_id,
                self.key_auth_source_label(source),
                false,
                false,
            )
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    if let Some(form) = this.new_connection_form.as_mut() {
                        form.field_focused = false;
                        form.selected_field = None;
                    }
                    this.ime_marked_text = None;
                    this.open_new_connection_select_from_pointer(select_id, cx);
                    window.focus(&this.focus_handle);
                    cx.stop_propagation();
                    cx.notify();
                }),
            );

        form_field(
            &self.tokens,
            self.i18n.t("ssh.auth.key_source"),
            select_anchor_probe(anchor_id, trigger, move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_select_anchor(anchor, cx);
                });
            }),
        )
        .into_any_element()
    }

    fn set_new_connection_auth_family(
        &mut self,
        family: SshAuthFamily,
        context: AuthSelectorContext,
        jump_form: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = self.new_connection_form.as_mut() {
            let current_tab = if jump_form {
                form.jump_server_form
                    .as_ref()
                    .map(|jump_form| jump_form.auth_tab)
                    .unwrap_or(SshAuthTab::Password)
            } else {
                form.auth_tab
            };
            let next_tab = Self::auth_tab_for_family_selection(family, current_tab, context);
            if jump_form {
                if let Some(jump_form) = form.jump_server_form.as_mut() {
                    jump_form.auth_tab = next_tab;
                }
            } else {
                form.auth_tab = next_tab;
            }
            form.focused_field = Self::focus_field_for_auth_tab(next_tab, jump_form);
            form.field_focused = false;
            clear_connection_selection(form);
            form.error = None;
        }
        self.close_new_connection_select();
        self.ime_marked_text = None;
        cx.notify();
    }

    fn auth_tab_for_family_selection(
        family: SshAuthFamily,
        current_tab: SshAuthTab,
        context: AuthSelectorContext,
    ) -> SshAuthTab {
        match family {
            SshAuthFamily::Password => SshAuthTab::Password,
            SshAuthFamily::Agent => SshAuthTab::Agent,
            SshAuthFamily::TwoFactor => SshAuthTab::TwoFactor,
            SshAuthFamily::Key => {
                // A top-level switch into Key should land on the file-key form,
                // while repeated clicks preserve the selected key source.
                if auth_family_from_tab(current_tab) == SshAuthFamily::Key {
                    auth_tab_from_key_source(Self::normalized_key_source_for_context(
                        current_tab,
                        context,
                    ))
                } else {
                    default_auth_tab_for_family(family)
                }
            }
        }
    }

    pub(super) fn set_new_connection_key_auth_source(
        &mut self,
        select_id: NewConnectionSelect,
        source: SshKeyAuthSource,
        cx: &mut Context<Self>,
    ) {
        let tab = auth_tab_from_key_source(source);
        if let Some(form) = self.new_connection_form.as_mut() {
            match select_id {
                NewConnectionSelect::KeyAuthSource => form.auth_tab = tab,
                NewConnectionSelect::JumpKeyAuthSource => {
                    let Some(jump_form) = form.jump_server_form.as_mut() else {
                        return;
                    };
                    jump_form.auth_tab = tab;
                }
                _ => return,
            }
            form.focused_field = Self::focus_field_for_auth_tab(
                tab,
                select_id == NewConnectionSelect::JumpKeyAuthSource,
            );
            form.field_focused = false;
            clear_connection_selection(form);
            form.error = None;
        }
        self.ime_marked_text = None;
        cx.notify();
    }

    fn focus_field_for_auth_tab(tab: SshAuthTab, jump_form: bool) -> NewConnectionField {
        if jump_form {
            match tab {
                SshAuthTab::Password => NewConnectionField::JumpPassword,
                SshAuthTab::SshKey | SshAuthTab::Certificate => NewConnectionField::JumpKeyPath,
                SshAuthTab::ManagedKey => NewConnectionField::JumpManagedKeyId,
                SshAuthTab::DefaultKey | SshAuthTab::Agent | SshAuthTab::TwoFactor => {
                    NewConnectionField::JumpHost
                }
            }
        } else {
            match tab {
                SshAuthTab::Password => NewConnectionField::Password,
                SshAuthTab::SshKey | SshAuthTab::Certificate => NewConnectionField::KeyPath,
                SshAuthTab::ManagedKey => NewConnectionField::ManagedKeyId,
                SshAuthTab::DefaultKey => NewConnectionField::Passphrase,
                SshAuthTab::Agent | SshAuthTab::TwoFactor => NewConnectionField::Host,
            }
        }
    }

    pub(super) fn render_edit_color_field(
        &self,
        value: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let swatch = parse_rgb24_hex(value).unwrap_or(TAURI_EDIT_COLOR_FALLBACK);
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

    pub(super) fn render_edit_icon_field(
        &self,
        icon_value: &str,
        color_value: &str,
        expanded: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let preview_color = parse_rgb24_hex(color_value).unwrap_or(theme.accent);
        let active_icon = session_icon_from_id(Some(icon_value)).unwrap_or(LucideIcon::Server);
        let mut grid = div().flex().flex_wrap().gap(px(self.tokens.spacing.two));

        for choice in SESSION_ICON_CHOICES {
            let selected = icon_value.trim() == choice.id;
            let icon_id = choice.id.to_string();
            grid = grid.child(
                div()
                    .size(px(38.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(if selected {
                        rgb(theme.accent)
                    } else {
                        rgb(theme.border)
                    })
                    .bg(if selected {
                        rgba((theme.accent << 8) | 0x22)
                    } else {
                        rgb(theme.bg)
                    })
                    .cursor_pointer()
                    .child(Self::render_lucide_icon(
                        choice.icon,
                        18.0,
                        if selected {
                            rgb(theme.accent)
                        } else {
                            rgb(theme.text_muted)
                        },
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            if let Some(form) = this.new_connection_form.as_mut() {
                                form.icon = icon_id.clone();
                                clear_connection_selection(form);
                            }
                            cx.notify();
                        }),
                    ),
            );
        }

        form_field(
            &self.tokens,
            self.i18n.t("sessionManager.edit_properties.icon"),
            div()
                .flex()
                .flex_col()
                .gap(px(self.tokens.spacing.three))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(self.tokens.spacing.three))
                        .child(
                            div()
                                .size(px(self.tokens.metrics.form_input_height))
                                .rounded(px(self.tokens.radii.md))
                                .border_1()
                                .border_color(rgb(theme.border))
                                .bg(rgba((preview_color << 8) | 0x22))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(Self::render_lucide_icon(
                                    active_icon,
                                    18.0,
                                    rgb(preview_color),
                                )),
                        )
                        .child(
                            button(
                                &self.tokens,
                                if expanded {
                                    self.i18n.t("sessionManager.edit_properties.hide_icons")
                                } else {
                                    self.i18n.t("sessionManager.edit_properties.choose_icon")
                                },
                                ButtonTone::Secondary,
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    if let Some(form) = this.new_connection_form.as_mut() {
                                        form.icon_picker_expanded = !form.icon_picker_expanded;
                                        clear_connection_selection(form);
                                    }
                                    cx.notify();
                                }),
                            ),
                        )
                        .when(!icon_value.trim().is_empty(), |row| {
                            row.child(
                                button(
                                    &self.tokens,
                                    self.i18n.t("sessionManager.edit_properties.default_icon"),
                                    ButtonTone::Secondary,
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        if let Some(form) = this.new_connection_form.as_mut() {
                                            form.icon.clear();
                                            clear_connection_selection(form);
                                        }
                                        cx.notify();
                                    }),
                                ),
                            )
                        }),
                )
                .when(expanded, |content| {
                    content.child(
                        div()
                            .id("edit-connection-icon-grid")
                            .max_h(px(180.0))
                            .selectable_overflow_y_scroll(
                                &self.selectable_text_scroll_handle("edit-connection-icon-grid"),
                            )
                            // The icon grid is a nested scroll surface inside
                            // the edit dialog. Wheel input over it should not
                            // also move the outer form body.
                            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                            .child(grid),
                    )
                }),
        )
    }

    pub(super) fn render_transport_selector(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let active_transport = self
            .new_connection_form
            .as_ref()
            .map(|form| form.transport)
            .unwrap_or(NewConnectionTransport::Ssh);
        let mut choices = vec![
            (
                NewConnectionTransport::Ssh,
                self.i18n.t("modals.new_connection.transport_ssh"),
                NewConnectionField::Name,
                LucideIcon::Server,
            ),
            (
                NewConnectionTransport::Telnet,
                self.i18n.t("modals.new_connection.transport_telnet"),
                NewConnectionField::Host,
                LucideIcon::Network,
            ),
            (
                NewConnectionTransport::RawTcp,
                self.i18n.t("modals.new_connection.transport_raw_tcp"),
                NewConnectionField::Host,
                LucideIcon::Cable,
            ),
            (
                NewConnectionTransport::RawUdp,
                self.i18n.t("modals.new_connection.transport_raw_udp"),
                NewConnectionField::Host,
                LucideIcon::Radio,
            ),
            (
                NewConnectionTransport::Serial,
                self.i18n.t("modals.new_connection.transport_serial"),
                NewConnectionField::SerialPortPath,
                LucideIcon::Radio,
            ),
            (
                NewConnectionTransport::Rdp,
                self.i18n.t("modals.new_connection.transport_rdp"),
                NewConnectionField::Host,
                LucideIcon::Monitor,
            ),
            (
                NewConnectionTransport::Vnc,
                self.i18n.t("modals.new_connection.transport_vnc"),
                NewConnectionField::Host,
                LucideIcon::Monitor,
            ),
        ];
        if cfg!(target_os = "windows") {
            choices.push((
                NewConnectionTransport::WslGraphics,
                self.i18n.t("modals.new_connection.transport_wsl_graphics"),
                NewConnectionField::Name,
                LucideIcon::AppWindow,
            ));
        }
        let mut sidebar = div()
            .w(px(NEW_CONNECTION_TYPE_SIDEBAR_WIDTH))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .border_r_1()
            .border_color(rgba((theme.border << 8) | 0x80))
            .pr(px(self.tokens.spacing.three));

        for (transport, label, focus_field, icon) in choices {
            let active = active_transport == transport;
            let row_text = if active {
                theme.accent
            } else {
                theme.text_muted
            };
            let row = div()
                .w_full()
                .min_h(px(36.0))
                .flex()
                .items_center()
                .gap(px(self.tokens.spacing.two))
                .px(px(self.tokens.spacing.two))
                .py(px(6.0))
                .border_l_2()
                .border_color(if active {
                    rgba((theme.accent << 8) | 0xff)
                } else {
                    rgba((theme.border << 8) | 0x00)
                })
                .cursor_pointer()
                .text_size(px(self.tokens.metrics.ui_text_sm))
                .text_color(rgb(row_text))
                .when(active, |row| row.bg(rgba((theme.accent << 8) | 0x16)))
                .when(!active, |row| row.hover(|row| row.bg(rgb(theme.bg_hover))))
                .child(Self::render_lucide_icon(icon, 14.0, rgb(row_text)))
                .child(div().min_w(px(0.0)).truncate().child(label))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        let mut should_refresh_ports = false;
                        if let Some(form) = this.new_connection_form.as_mut() {
                            let previous_transport = form.transport;
                            apply_transport_default_port(form, previous_transport, transport);
                            apply_transport_default_username(form, previous_transport, transport);
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
                );
            sidebar = sidebar.child(row);
        }
        sidebar.into_any_element()
    }

    pub(super) fn render_wsl_graphics_form_branch(&self, _cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_panel))
            .p(px(self.tokens.spacing.three))
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.two))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(self.tokens.spacing.two))
                    .child(Self::render_lucide_icon(
                        LucideIcon::AppWindow,
                        18.0,
                        rgb(theme.accent),
                    ))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_base))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(theme.text_heading))
                            .child(self.i18n.t("modals.new_connection.transport_wsl_graphics")),
                    ),
            )
            .child(
                self.render_connection_hint(
                    self.i18n.t("modals.new_connection.wsl_graphics_detail"),
                ),
            )
            .when(!cfg!(target_os = "windows"), |panel| {
                panel.child(
                    self.render_connection_hint_with_color(
                        self.i18n
                            .t("modals.new_connection.wsl_graphics_windows_only"),
                        theme.error,
                    ),
                )
            })
            .into_any_element()
    }

    pub(super) fn render_remote_desktop_form_branch(
        &self,
        protocol: oxideterm_remote_desktop::RemoteDesktopProtocol,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(form) = self.new_connection_form.as_ref() else {
            return div().into_any_element();
        };
        let port_placeholder = match protocol {
            oxideterm_remote_desktop::RemoteDesktopProtocol::Rdp => RDP_DEFAULT_PORT_TEXT,
            oxideterm_remote_desktop::RemoteDesktopProtocol::Vnc => VNC_DEFAULT_PORT_TEXT,
        };
        let port_invalid = !form.port.trim().is_empty()
            && !form.port.trim().parse::<u16>().is_ok_and(|port| port > 0);

        div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.modal_section_gap))
            .child(self.render_connection_field(
                self.i18n.t("ssh.form.name"),
                &form.name,
                self.i18n.t("ssh.form.name_placeholder"),
                NewConnectionField::Name,
                false,
                cx,
            ))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(self.tokens.metrics.form_host_port_gap))
                    .child(div().flex_1().child(self.render_connection_field(
                        self.i18n.t("ssh.form.host"),
                        &form.host,
                        self.i18n.t("ssh.form.host_placeholder"),
                        NewConnectionField::Host,
                        false,
                        cx,
                    )))
                    .child(div().w(px(self.tokens.metrics.form_port_width)).child(
                        self.render_connection_field(
                            self.i18n.t("ssh.form.port"),
                            &form.port,
                            port_placeholder.to_string(),
                            NewConnectionField::Port,
                            false,
                            cx,
                        ),
                    )),
            )
            .when(port_invalid, |section| {
                section.child(
                    self.render_connection_hint_with_color(
                        self.i18n
                            .t("modals.new_connection.remote_desktop_invalid_port"),
                        self.tokens.ui.error,
                    ),
                )
            })
            .when(
                protocol == oxideterm_remote_desktop::RemoteDesktopProtocol::Rdp,
                |section| {
                    section.child(self.render_connection_field(
                        self.i18n.t("modals.new_connection.remote_desktop_username"),
                        &form.username,
                        "Administrator".to_string(),
                        NewConnectionField::Username,
                        false,
                        cx,
                    ))
                },
            )
            .child(self.render_connection_field(
                self.i18n.t("ssh.form.password"),
                &form.password,
                if protocol == oxideterm_remote_desktop::RemoteDesktopProtocol::Rdp {
                    self.i18n
                        .t("modals.new_connection.remote_desktop_password_placeholder")
                } else {
                    self.i18n.t("ssh.form.password")
                },
                NewConnectionField::Password,
                true,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_raw_tcp_form_branch(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(form) = self.new_connection_form.as_ref() else {
            return div().into_any_element();
        };
        let raw_tcp_port_invalid = !form.port.trim().is_empty()
            && !form.port.trim().parse::<u16>().is_ok_and(|port| port > 0);
        let tls_enabled = matches!(
            form.raw_tcp_tls_mode,
            oxideterm_connections::RawTcpTlsMode::Enabled
        );
        div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.modal_section_gap))
            .child(
                div()
                    .rounded(px(self.tokens.radii.lg))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .bg(rgba((self.tokens.ui.bg << 8) | TAURI_SERIAL_PANEL_BG_ALPHA))
                    .p(px(self.tokens.spacing.three))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t("modals.new_connection.raw_tcp_section_title")),
                    )
                    .child(
                        div()
                            .mt(px(self.tokens.spacing.one))
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("modals.new_connection.raw_tcp_connect_hint")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(self.tokens.metrics.form_host_port_gap))
                    .child(
                        div().flex_1().child(
                            self.render_connection_field(
                                self.i18n.t("modals.new_connection.raw_tcp_host"),
                                &form.host,
                                self.i18n
                                    .t("modals.new_connection.raw_tcp_host_placeholder"),
                                NewConnectionField::Host,
                                false,
                                cx,
                            ),
                        ),
                    )
                    .child(
                        div().w(px(self.tokens.metrics.form_port_width)).child(
                            self.render_connection_field(
                                self.i18n.t("modals.new_connection.raw_tcp_port"),
                                &form.port,
                                self.i18n
                                    .t("modals.new_connection.raw_tcp_port_placeholder"),
                                NewConnectionField::Port,
                                false,
                                cx,
                            ),
                        ),
                    ),
            )
            .when(raw_tcp_port_invalid, |section| {
                section.child(self.render_connection_hint_with_color(
                    self.i18n.t("modals.new_connection.raw_tcp_invalid_port"),
                    self.tokens.ui.error,
                ))
            })
            .child(
                div()
                    .grid()
                    .grid_cols(3)
                    .gap(px(TAURI_SERIAL_GRID_GAP))
                    .child(
                        self.render_raw_tcp_line_ending_select(
                            form.raw_tcp_line_ending.clone(),
                            cx,
                        ),
                    )
                    .child(
                        self.render_raw_tcp_display_mode_select(
                            form.raw_tcp_display_mode.clone(),
                            cx,
                        ),
                    )
                    .child(
                        self.render_raw_tcp_send_mode_select(form.raw_tcp_send_mode.clone(), cx),
                    ),
            )
            .child(
                div()
                    .grid()
                    .grid_cols(2)
                    .gap(px(TAURI_SERIAL_GRID_GAP))
                    .child(self.render_raw_tcp_tls_mode_select(form.raw_tcp_tls_mode.clone(), cx))
                    .child(self.render_raw_tcp_tls_verification_select(
                        form.raw_tcp_tls_verification.clone(),
                        !tls_enabled,
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(self.tokens.spacing.two))
                    .child(
                        self.render_connection_field(
                            self.i18n.t("modals.new_connection.raw_tcp_tls_server_name"),
                            &form.raw_tcp_tls_server_name,
                            self.i18n
                                .t("modals.new_connection.raw_tcp_tls_server_name_placeholder"),
                            NewConnectionField::RawTcpTlsServerName,
                            false,
                            cx,
                        ),
                    )
                    .child(self.render_connection_hint(if tls_enabled {
                        self.i18n
                            .t("modals.new_connection.raw_tcp_tls_server_name_hint")
                    } else {
                        self.i18n
                            .t("modals.new_connection.raw_tcp_tls_server_name_disabled_hint")
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .rounded(px(self.tokens.radii.lg))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .p(px(self.tokens.spacing.three))
                    .child(
                        self.render_connection_field(
                            self.i18n.t("modals.new_connection.raw_tcp_profile_name"),
                            &form.raw_tcp_profile_name,
                            self.i18n
                                .t("modals.new_connection.raw_tcp_profile_name_placeholder"),
                            NewConnectionField::RawTcpProfileName,
                            false,
                            cx,
                        ),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn render_raw_udp_form_branch(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(form) = self.new_connection_form.as_ref() else {
            return div().into_any_element();
        };
        let raw_udp_remote_port_invalid = !form.port.trim().is_empty()
            && !form.port.trim().parse::<u16>().is_ok_and(|port| port > 0);
        let raw_udp_local_port_invalid = !form.raw_udp_local_bind_port.trim().is_empty()
            && form.raw_udp_local_bind_port.trim().parse::<u16>().is_err();
        div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.modal_section_gap))
            .child(
                div()
                    .rounded(px(self.tokens.radii.lg))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .bg(rgba((self.tokens.ui.bg << 8) | TAURI_SERIAL_PANEL_BG_ALPHA))
                    .p(px(self.tokens.spacing.three))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t("modals.new_connection.raw_udp_section_title")),
                    )
                    .child(
                        div()
                            .mt(px(self.tokens.spacing.one))
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("modals.new_connection.raw_udp_connect_hint")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(self.tokens.metrics.form_host_port_gap))
                    .child(
                        div().flex_1().child(
                            self.render_connection_field(
                                self.i18n.t("modals.new_connection.raw_udp_host"),
                                &form.host,
                                self.i18n
                                    .t("modals.new_connection.raw_udp_host_placeholder"),
                                NewConnectionField::Host,
                                false,
                                cx,
                            ),
                        ),
                    )
                    .child(
                        div().w(px(self.tokens.metrics.form_port_width)).child(
                            self.render_connection_field(
                                self.i18n.t("modals.new_connection.raw_udp_port"),
                                &form.port,
                                self.i18n
                                    .t("modals.new_connection.raw_udp_port_placeholder"),
                                NewConnectionField::Port,
                                false,
                                cx,
                            ),
                        ),
                    ),
            )
            .when(raw_udp_remote_port_invalid, |section| {
                section.child(self.render_connection_hint_with_color(
                    self.i18n.t("modals.new_connection.raw_udp_invalid_port"),
                    self.tokens.ui.error,
                ))
            })
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(self.tokens.metrics.form_host_port_gap))
                    .child(
                        div().flex_1().child(
                            self.render_connection_field(
                                self.i18n.t("modals.new_connection.raw_udp_local_bind_host"),
                                &form.raw_udp_local_bind_host,
                                self.i18n
                                    .t("modals.new_connection.raw_udp_local_bind_host_placeholder"),
                                NewConnectionField::RawUdpLocalBindHost,
                                false,
                                cx,
                            ),
                        ),
                    )
                    .child(
                        div().w(px(self.tokens.metrics.form_port_width)).child(
                            self.render_connection_field(
                                self.i18n.t("modals.new_connection.raw_udp_local_bind_port"),
                                &form.raw_udp_local_bind_port,
                                self.i18n
                                    .t("modals.new_connection.raw_udp_local_bind_port_placeholder"),
                                NewConnectionField::RawUdpLocalBindPort,
                                false,
                                cx,
                            ),
                        ),
                    ),
            )
            .when(raw_udp_local_port_invalid, |section| {
                section.child(
                    self.render_connection_hint_with_color(
                        self.i18n
                            .t("modals.new_connection.raw_udp_invalid_local_bind_port"),
                        self.tokens.ui.error,
                    ),
                )
            })
            .child(
                div()
                    .grid()
                    .grid_cols(3)
                    .gap(px(TAURI_SERIAL_GRID_GAP))
                    .child(
                        self.render_raw_udp_line_ending_select(
                            form.raw_udp_line_ending.clone(),
                            cx,
                        ),
                    )
                    .child(
                        self.render_raw_udp_display_mode_select(
                            form.raw_udp_display_mode.clone(),
                            cx,
                        ),
                    )
                    .child(
                        self.render_raw_udp_send_mode_select(form.raw_udp_send_mode.clone(), cx),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .rounded(px(self.tokens.radii.lg))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .p(px(self.tokens.spacing.three))
                    .child(
                        self.render_connection_field(
                            self.i18n.t("modals.new_connection.raw_udp_profile_name"),
                            &form.raw_udp_profile_name,
                            self.i18n
                                .t("modals.new_connection.raw_udp_profile_name_placeholder"),
                            NewConnectionField::RawUdpProfileName,
                            false,
                            cx,
                        ),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn render_telnet_form_branch(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(form) = self.new_connection_form.as_ref() else {
            return div().into_any_element();
        };
        let telnet_port_invalid =
            !form.port.trim().is_empty() && form.port.trim().parse::<u16>().is_err();
        div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.modal_section_gap))
            .child(
                div()
                    .rounded(px(self.tokens.radii.lg))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .bg(rgba((self.tokens.ui.bg << 8) | TAURI_SERIAL_PANEL_BG_ALPHA))
                    .p(px(self.tokens.spacing.three))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t("modals.new_connection.telnet_section_title")),
                    )
                    .child(
                        div()
                            .mt(px(self.tokens.spacing.one))
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("modals.new_connection.telnet_connect_hint")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(self.tokens.metrics.form_host_port_gap))
                    .child(div().flex_1().child(self.render_connection_field(
                        self.i18n.t("modals.new_connection.telnet_host"),
                        &form.host,
                        self.i18n.t("modals.new_connection.telnet_host_placeholder"),
                        NewConnectionField::Host,
                        false,
                        cx,
                    )))
                    .child(div().w(px(self.tokens.metrics.form_port_width)).child(
                        self.render_connection_field(
                            self.i18n.t("modals.new_connection.telnet_port"),
                            &form.port,
                            TELNET_DEFAULT_PORT_TEXT.to_string(),
                            NewConnectionField::Port,
                            false,
                            cx,
                        ),
                    )),
            )
            .when(telnet_port_invalid, |section| {
                section.child(self.render_connection_hint_with_color(
                    self.i18n.t("modals.new_connection.telnet_invalid_port"),
                    self.tokens.ui.error,
                ))
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .rounded(px(self.tokens.radii.lg))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .p(px(self.tokens.spacing.three))
                    .child(
                        self.render_connection_field(
                            self.i18n.t("modals.new_connection.telnet_profile_name"),
                            &form.telnet_profile_name,
                            self.i18n
                                .t("modals.new_connection.telnet_profile_name_placeholder"),
                            NewConnectionField::TelnetProfileName,
                            false,
                            cx,
                        ),
                    ),
            )
            .into_any_element()
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
                        self.i18n
                            .t("modals.new_connection.serial_load_ports_failed")
                    ));
                }
            }
        }
        cx.notify();
    }

    pub(super) fn render_serial_form_branch(&self, cx: &mut Context<Self>) -> AnyElement {
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
                    .bg(rgba((self.tokens.ui.bg << 8) | TAURI_SERIAL_PANEL_BG_ALPHA))
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
                                section.child(
                                    self.render_connection_hint_with_color(
                                        self.i18n
                                            .t("modals.new_connection.serial_invalid_baud_rate"),
                                        self.tokens.ui.error,
                                    ),
                                )
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
                    .rounded(px(self.tokens.radii.lg))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .p(px(self.tokens.spacing.three))
                    .child(
                        self.render_connection_field(
                            self.i18n.t("modals.new_connection.serial_profile_name"),
                            &form.serial_profile_name,
                            self.i18n
                                .t("modals.new_connection.serial_profile_name_placeholder"),
                            NewConnectionField::SerialProfileName,
                            false,
                            cx,
                        ),
                    ),
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
                    .child(self.workspace_toolbar_action_button(
                        self.i18n.t("modals.new_connection.serial_refresh_ports"),
                        Some(if loading {
                            self.render_loading_icon(
                                "serial-ports-loading",
                                14.0,
                                rgb(self.tokens.ui.text),
                            )
                        } else {
                            Self::render_lucide_icon(
                                LucideIcon::RefreshCw,
                                14.0,
                                rgb(self.tokens.ui.text),
                            )
                        }),
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
                    )),
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
                        this.open_new_connection_select_from_pointer(select_id, cx);
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

    fn render_raw_tcp_line_ending_select(
        &self,
        selected: oxideterm_connections::RawTcpLineEnding,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        form_field(
            &self.tokens,
            self.i18n.t("modals.new_connection.raw_tcp_line_ending"),
            self.render_new_connection_select_control(
                NewConnectionSelect::RawTcpLineEnding,
                self.raw_tcp_line_ending_label(&selected),
                false,
                false,
                cx,
            ),
        )
        .into_any_element()
    }

    fn render_raw_tcp_display_mode_select(
        &self,
        selected: oxideterm_connections::RawTcpDisplayMode,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        form_field(
            &self.tokens,
            self.i18n.t("modals.new_connection.raw_tcp_display_mode"),
            self.render_new_connection_select_control(
                NewConnectionSelect::RawTcpDisplayMode,
                self.raw_tcp_display_mode_label(&selected),
                false,
                false,
                cx,
            ),
        )
        .into_any_element()
    }

    fn render_raw_tcp_send_mode_select(
        &self,
        selected: oxideterm_connections::RawTcpSendMode,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        form_field(
            &self.tokens,
            self.i18n.t("modals.new_connection.raw_tcp_send_mode"),
            self.render_new_connection_select_control(
                NewConnectionSelect::RawTcpSendMode,
                self.raw_tcp_send_mode_label(&selected),
                false,
                false,
                cx,
            ),
        )
        .into_any_element()
    }

    fn render_raw_udp_line_ending_select(
        &self,
        selected: oxideterm_connections::RawUdpLineEnding,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        form_field(
            &self.tokens,
            self.i18n.t("modals.new_connection.raw_udp_line_ending"),
            self.render_new_connection_select_control(
                NewConnectionSelect::RawUdpLineEnding,
                self.raw_udp_line_ending_label(&selected),
                false,
                false,
                cx,
            ),
        )
        .into_any_element()
    }

    fn render_raw_udp_display_mode_select(
        &self,
        selected: oxideterm_connections::RawUdpDisplayMode,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        form_field(
            &self.tokens,
            self.i18n.t("modals.new_connection.raw_udp_display_mode"),
            self.render_new_connection_select_control(
                NewConnectionSelect::RawUdpDisplayMode,
                self.raw_udp_display_mode_label(&selected),
                false,
                false,
                cx,
            ),
        )
        .into_any_element()
    }

    fn render_raw_udp_send_mode_select(
        &self,
        selected: oxideterm_connections::RawUdpSendMode,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        form_field(
            &self.tokens,
            self.i18n.t("modals.new_connection.raw_udp_send_mode"),
            self.render_new_connection_select_control(
                NewConnectionSelect::RawUdpSendMode,
                self.raw_udp_send_mode_label(&selected),
                false,
                false,
                cx,
            ),
        )
        .into_any_element()
    }

    fn render_raw_tcp_tls_mode_select(
        &self,
        selected: oxideterm_connections::RawTcpTlsMode,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        form_field(
            &self.tokens,
            self.i18n.t("modals.new_connection.raw_tcp_tls"),
            self.render_new_connection_select_control(
                NewConnectionSelect::RawTcpTlsMode,
                self.raw_tcp_tls_mode_label(&selected),
                false,
                false,
                cx,
            ),
        )
        .into_any_element()
    }

    fn render_raw_tcp_tls_verification_select(
        &self,
        selected: oxideterm_connections::RawTcpTlsVerification,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        form_field(
            &self.tokens,
            self.i18n
                .t("modals.new_connection.raw_tcp_tls_verification"),
            self.render_new_connection_select_control(
                NewConnectionSelect::RawTcpTlsVerification,
                self.raw_tcp_tls_verification_label(&selected),
                false,
                disabled,
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
            SavedUpstreamProxyProtocol::HttpConnect => {
                "settings_view.network.protocol_http_connect"
            }
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

    pub(super) fn render_upstream_proxy_policy_section(
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
            .child(self.render_connection_hint(self.i18n.t("modals.upstream_proxy.policy_hint")))
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
                                    self.upstream_proxy_protocol_label(
                                        form.upstream_proxy_protocol,
                                    ),
                                    false,
                                    false,
                                    cx,
                                ),
                            )))
                            .child(div().w(px(self.tokens.metrics.form_port_width)).child(
                                self.render_connection_field(
                                    self.i18n.t("settings_view.network.port"),
                                    &form.upstream_proxy_port,
                                    "1080".to_string(),
                                    NewConnectionField::UpstreamProxyPort,
                                    false,
                                    cx,
                                ),
                            )),
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
                    .when(
                        form.upstream_proxy_auth == NewConnectionUpstreamProxyAuth::Password,
                        |content| {
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
                        },
                    )
            })
            .into_any_element()
    }

    pub(super) fn serial_parity_label(&self, parity: oxideterm_terminal::SerialParity) -> String {
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

    pub(super) fn serial_flow_control_label(
        &self,
        flow: oxideterm_terminal::SerialFlowControl,
    ) -> String {
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

    pub(super) fn raw_tcp_line_ending_label(
        &self,
        line_ending: &oxideterm_connections::RawTcpLineEnding,
    ) -> String {
        let key = match line_ending {
            oxideterm_connections::RawTcpLineEnding::Lf => {
                "modals.new_connection.raw_tcp_line_ending_lf"
            }
            oxideterm_connections::RawTcpLineEnding::CrLf => {
                "modals.new_connection.raw_tcp_line_ending_crlf"
            }
            oxideterm_connections::RawTcpLineEnding::Cr => {
                "modals.new_connection.raw_tcp_line_ending_cr"
            }
            oxideterm_connections::RawTcpLineEnding::None => {
                "modals.new_connection.raw_tcp_line_ending_none"
            }
        };
        self.i18n.t(key)
    }

    pub(super) fn raw_tcp_display_mode_label(
        &self,
        display_mode: &oxideterm_connections::RawTcpDisplayMode,
    ) -> String {
        let key = match display_mode {
            oxideterm_connections::RawTcpDisplayMode::Text => {
                "modals.new_connection.raw_tcp_display_text"
            }
            oxideterm_connections::RawTcpDisplayMode::Hex => {
                "modals.new_connection.raw_tcp_display_hex"
            }
            oxideterm_connections::RawTcpDisplayMode::Mixed => {
                "modals.new_connection.raw_tcp_display_mixed"
            }
        };
        self.i18n.t(key)
    }

    pub(super) fn raw_tcp_send_mode_label(
        &self,
        send_mode: &oxideterm_connections::RawTcpSendMode,
    ) -> String {
        let key = match send_mode {
            oxideterm_connections::RawTcpSendMode::Text => {
                "modals.new_connection.raw_tcp_send_text"
            }
            oxideterm_connections::RawTcpSendMode::Hex => "modals.new_connection.raw_tcp_send_hex",
        };
        self.i18n.t(key)
    }

    pub(super) fn raw_udp_line_ending_label(
        &self,
        line_ending: &oxideterm_connections::RawUdpLineEnding,
    ) -> String {
        let key = match line_ending {
            oxideterm_connections::RawUdpLineEnding::Lf => {
                "modals.new_connection.raw_udp_line_ending_lf"
            }
            oxideterm_connections::RawUdpLineEnding::CrLf => {
                "modals.new_connection.raw_udp_line_ending_crlf"
            }
            oxideterm_connections::RawUdpLineEnding::Cr => {
                "modals.new_connection.raw_udp_line_ending_cr"
            }
            oxideterm_connections::RawUdpLineEnding::None => {
                "modals.new_connection.raw_udp_line_ending_none"
            }
        };
        self.i18n.t(key)
    }

    pub(super) fn raw_udp_display_mode_label(
        &self,
        display_mode: &oxideterm_connections::RawUdpDisplayMode,
    ) -> String {
        let key = match display_mode {
            oxideterm_connections::RawUdpDisplayMode::Text => {
                "modals.new_connection.raw_udp_display_text"
            }
            oxideterm_connections::RawUdpDisplayMode::Hex => {
                "modals.new_connection.raw_udp_display_hex"
            }
            oxideterm_connections::RawUdpDisplayMode::Mixed => {
                "modals.new_connection.raw_udp_display_mixed"
            }
        };
        self.i18n.t(key)
    }

    pub(super) fn raw_udp_send_mode_label(
        &self,
        send_mode: &oxideterm_connections::RawUdpSendMode,
    ) -> String {
        let key = match send_mode {
            oxideterm_connections::RawUdpSendMode::Text => {
                "modals.new_connection.raw_udp_send_text"
            }
            oxideterm_connections::RawUdpSendMode::Hex => "modals.new_connection.raw_udp_send_hex",
        };
        self.i18n.t(key)
    }

    pub(super) fn raw_tcp_tls_mode_label(
        &self,
        mode: &oxideterm_connections::RawTcpTlsMode,
    ) -> String {
        let key = match mode {
            oxideterm_connections::RawTcpTlsMode::Disabled => {
                "modals.new_connection.raw_tcp_tls_disabled"
            }
            oxideterm_connections::RawTcpTlsMode::Enabled => {
                "modals.new_connection.raw_tcp_tls_enabled"
            }
        };
        self.i18n.t(key)
    }

    pub(super) fn raw_tcp_tls_verification_label(
        &self,
        verification: &oxideterm_connections::RawTcpTlsVerification,
    ) -> String {
        let key = match verification {
            oxideterm_connections::RawTcpTlsVerification::System => {
                "modals.new_connection.raw_tcp_tls_verify_system"
            }
            oxideterm_connections::RawTcpTlsVerification::AllowInvalidCertificates => {
                "modals.new_connection.raw_tcp_tls_verify_allow_invalid"
            }
        };
        self.i18n.t(key)
    }

    pub(super) fn set_new_connection_serial_port(
        &mut self,
        port_path: String,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = self.new_connection_form.as_mut() {
            form.serial_port_path = port_path;
            form.focused_field = NewConnectionField::SerialPortPath;
            form.field_focused = false;
            clear_connection_selection(form);
            form.error = None;
        }
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn set_new_connection_serial_u8(
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
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn set_new_connection_serial_parity(
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
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn set_new_connection_serial_flow_control(
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
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn set_new_connection_raw_tcp_line_ending(
        &mut self,
        line_ending: oxideterm_connections::RawTcpLineEnding,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = self.new_connection_form.as_mut() {
            form.raw_tcp_line_ending = line_ending;
            form.field_focused = false;
            clear_connection_selection(form);
            form.error = None;
        }
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn set_new_connection_raw_tcp_display_mode(
        &mut self,
        display_mode: oxideterm_connections::RawTcpDisplayMode,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = self.new_connection_form.as_mut() {
            form.raw_tcp_display_mode = display_mode;
            form.field_focused = false;
            clear_connection_selection(form);
            form.error = None;
        }
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn set_new_connection_raw_tcp_send_mode(
        &mut self,
        send_mode: oxideterm_connections::RawTcpSendMode,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = self.new_connection_form.as_mut() {
            form.raw_tcp_send_mode = send_mode;
            form.field_focused = false;
            clear_connection_selection(form);
            form.error = None;
        }
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn set_new_connection_raw_udp_line_ending(
        &mut self,
        line_ending: oxideterm_connections::RawUdpLineEnding,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = self.new_connection_form.as_mut() {
            form.raw_udp_line_ending = line_ending;
            form.field_focused = false;
            clear_connection_selection(form);
            form.error = None;
        }
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn set_new_connection_raw_udp_display_mode(
        &mut self,
        display_mode: oxideterm_connections::RawUdpDisplayMode,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = self.new_connection_form.as_mut() {
            form.raw_udp_display_mode = display_mode;
            form.field_focused = false;
            clear_connection_selection(form);
            form.error = None;
        }
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn set_new_connection_raw_udp_send_mode(
        &mut self,
        send_mode: oxideterm_connections::RawUdpSendMode,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = self.new_connection_form.as_mut() {
            form.raw_udp_send_mode = send_mode;
            form.field_focused = false;
            clear_connection_selection(form);
            form.error = None;
        }
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn set_new_connection_raw_tcp_tls_mode(
        &mut self,
        tls_mode: oxideterm_connections::RawTcpTlsMode,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = self.new_connection_form.as_mut() {
            form.raw_tcp_tls_mode = tls_mode;
            form.field_focused = false;
            clear_connection_selection(form);
            form.error = None;
        }
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn set_new_connection_raw_tcp_tls_verification(
        &mut self,
        verification: oxideterm_connections::RawTcpTlsVerification,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = self.new_connection_form.as_mut() {
            form.raw_tcp_tls_verification = verification;
            form.field_focused = false;
            clear_connection_selection(form);
            form.error = None;
        }
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn set_new_connection_upstream_proxy_policy(
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
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn set_new_connection_upstream_proxy_protocol(
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
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn set_new_connection_upstream_proxy_auth(
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
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn render_connection_checkbox(
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

    pub(super) fn render_connection_button(
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

pub(super) fn serial_port_display_label(port: &oxideterm_terminal::SerialPortInfo) -> String {
    if port.display_name.trim().is_empty() {
        port.port_path.clone()
    } else {
        port.display_name.clone()
    }
}
