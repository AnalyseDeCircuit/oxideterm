use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FormKeyAction {
    NotHandled,
    Handled,
    CloseForm,
    CloseJumpForm,
    Submit,
    AddJumpServer,
    Paste,
}

impl ConnectionFlowEntity {
    pub fn handle_form_key(
        &mut self,
        event: &KeyDownEvent,
        caret_was_visible: bool,
        tunnel_fields: Option<&[NewConnectionField]>,
        cx: &mut Context<Self>,
    ) -> (FormKeyAction, bool) {
        if self.handle_ssh_algorithm_key(event, cx) {
            return (FormKeyAction::Handled, false);
        }
        let saved_connection_form_uses_unloaded_secret =
            self.form.saved_connection_source_id().is_some()
                && self.form.saved_connection_prompt_action.is_none();
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        let text_input = text_from_keystroke(&event.keystroke);
        if self.form.open_select.is_some()
            && matches!(key, "escape" | "enter" | "tab")
            && !modifiers.platform
        {
            self.form.close_select();
            cx.notify();
            return (FormKeyAction::Handled, false);
        }
        let uses_text_edit_modifier = if cfg!(target_os = "macos") {
            modifiers.platform
        } else {
            modifiers.platform || modifiers.control
        };
        let Some(form) = self.form.form.as_mut() else {
            return (FormKeyAction::NotHandled, false);
        };
        if !form.field_focused {
            return match key {
                "escape" if form.jump_server_form.is_some() => {
                    (FormKeyAction::CloseJumpForm, false)
                }
                "escape" => (FormKeyAction::CloseForm, false),
                "enter" if form.jump_server_form.is_some() => (FormKeyAction::AddJumpServer, false),
                "enter" => (FormKeyAction::Submit, false),
                "tab" => {
                    form.field_focused = true;
                    cx.notify();
                    (FormKeyAction::Handled, true)
                }
                _ => (FormKeyAction::Handled, false),
            };
        }

        let password_uses_saved_value = saved_connection_form_uses_unloaded_secret
            && form.focused_field == NewConnectionField::Password
            && !form.password_loaded;
        if password_uses_saved_value {
            if uses_text_edit_modifier && key == "v" {
                return (FormKeyAction::Paste, false);
            }
            if text_input.is_some() || key == "space" {
                // The protected value stays in the keychain; only new input becomes UI-owned.
                form.password_loaded = true;
            } else if !matches!(key, "escape" | "enter" | "tab") {
                return (FormKeyAction::Handled, false);
            }
        }

        let focused_field_accepts_ime = matches!(
            form.focused_field,
            NewConnectionField::Name
                | NewConnectionField::TunnelTargetHost
                | NewConnectionField::Host
                | NewConnectionField::Username
                | NewConnectionField::Group
                | NewConnectionField::Notes
                | NewConnectionField::InitialRemotePath
                | NewConnectionField::StandaloneSftpSecondaryHost
                | NewConnectionField::StandaloneSftpSecondaryUsername
                | NewConnectionField::StandaloneSftpSecondaryInitialRemotePath
                | NewConnectionField::StandaloneSftpSecondaryIdentityAgent
                | NewConnectionField::ProxyCommand
                | NewConnectionField::Color
                | NewConnectionField::IdentityAgent
                | NewConnectionField::TelnetProfileName
                | NewConnectionField::MoshServerExecutable
                | NewConnectionField::MoshUdpHost
                | NewConnectionField::MoshLocale
                | NewConnectionField::JumpHost
                | NewConnectionField::JumpUsername
                | NewConnectionField::JumpIdentityAgent
                | NewConnectionField::UpstreamProxyHost
                | NewConnectionField::UpstreamProxyNoProxy
                | NewConnectionField::UpstreamProxyUsername
        );

        if uses_text_edit_modifier {
            let mut show_caret = false;
            match key {
                "a" => {
                    select_current_connection_field(form);
                    show_caret = true;
                }
                "c" if form.selected_field == Some(form.focused_field) => {
                    cx.write_to_clipboard(ClipboardItem::new_string(
                        current_connection_field(form).to_string(),
                    ));
                }
                "x" if form.selected_field == Some(form.focused_field) => {
                    cx.write_to_clipboard(ClipboardItem::new_string(
                        current_connection_field(form).to_string(),
                    ));
                    clear_current_connection_field(form);
                    restore_saved_password_placeholder_if_empty(
                        form,
                        saved_connection_form_uses_unloaded_secret,
                    );
                    form.error = None;
                    show_caret = true;
                }
                "v" => return (FormKeyAction::Paste, false),
                _ => {}
            }
            if show_caret {
                cx.notify();
            }
            return (FormKeyAction::Handled, show_caret);
        }

        match key {
            "escape" if form.jump_server_form.is_some() => (FormKeyAction::CloseJumpForm, false),
            "escape" => (FormKeyAction::CloseForm, false),
            "enter" if form.jump_server_form.is_some() => (FormKeyAction::AddJumpServer, false),
            "enter" => (FormKeyAction::Submit, false),
            "tab" => {
                form.focused_field = if let Some(fields) = tunnel_fields {
                    let current = fields
                        .iter()
                        .position(|field| *field == form.focused_field)
                        .unwrap_or(0);
                    fields[(current + if modifiers.shift { fields.len() - 1 } else { 1 })
                        % fields.len()]
                } else if let Some(jump_form) = form.jump_server_form.as_ref() {
                    next_jump_connection_field(
                        form.focused_field,
                        jump_form.auth_tab,
                        jump_form.gssapi_enabled,
                        jump_form.empty_password,
                        !modifiers.shift,
                    )
                } else if form.transport == NewConnectionTransport::StandaloneSftp {
                    next_standalone_sftp_field(form, !modifiers.shift)
                } else {
                    next_connection_field(
                        form.focused_field,
                        form.auth_tab,
                        form.gssapi_enabled,
                        form.transport,
                        form.upstream_proxy_policy,
                        form.upstream_proxy_auth,
                        form.empty_password,
                        !modifiers.shift,
                    )
                };
                form.field_focused = true;
                clear_connection_selection(form);
                cx.notify();
                (FormKeyAction::Handled, true)
            }
            "backspace" => {
                let field_changed = backspace_current_connection_field(form);
                restore_saved_password_placeholder_if_empty(
                    form,
                    saved_connection_form_uses_unloaded_secret,
                );
                let changed = field_changed || form.error.take().is_some() || !caret_was_visible;
                if changed {
                    cx.notify();
                }
                (FormKeyAction::Handled, changed)
            }
            "space" if !focused_field_accepts_ime => {
                insert_text_into_current_connection_field(form, " ");
                form.error = None;
                cx.notify();
                (FormKeyAction::Handled, true)
            }
            _ if focused_field_accepts_ime => (FormKeyAction::Handled, false),
            _ => {
                if let Some(text) = text_input {
                    insert_text_into_current_connection_field(form, text);
                    form.error = None;
                    cx.notify();
                    (FormKeyAction::Handled, true)
                } else {
                    (FormKeyAction::Handled, false)
                }
            }
        }
    }

    pub fn paste_form_input(&mut self, cx: &mut Context<Self>) -> bool {
        let saved_connection_form_uses_unloaded_secret =
            self.form.saved_connection_source_id().is_some()
                && self.form.saved_connection_prompt_action.is_none();
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return false;
        };
        let text = zeroize::Zeroizing::new(text);
        let normalized = zeroize::Zeroizing::new(text.replace("\r\n", "\n"));
        let normalized = zeroize::Zeroizing::new(normalized.replace('\r', "\n"));
        let pasted = (|| {
            let state = &mut self.form;
            let Some(form) = state.form.as_mut() else {
                return false;
            };
            if form.focused_field == NewConnectionField::Notes {
                insert_text_into_current_connection_field(form, &normalized);
            } else {
                // All other connection form controls remain single-line inputs.
                let single_line =
                    zeroize::Zeroizing::new(normalized.lines().collect::<Vec<_>>().join(" "));
                if saved_connection_form_uses_unloaded_secret
                    && form.focused_field == NewConnectionField::Password
                    && !form.password_loaded
                {
                    if single_line.is_empty() {
                        return false;
                    }
                    // A pasted replacement is owned by the form; the saved value is never loaded.
                    form.password_loaded = true;
                }
                insert_text_into_current_connection_field(form, &single_line);
            }
            form.error = None;
            true
        })();

        if pasted {
            cx.notify();
        }
        pasted
    }
}
