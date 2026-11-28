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

#[cfg(not(target_family = "wasm"))]
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
        self.form
            .handle_key(event, caret_was_visible, tunnel_fields, cx)
    }
    pub fn paste_form_input(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(text) = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .map(zeroize::Zeroizing::new)
        else {
            return false;
        };
        let pasted = self.form.paste_text(&text);
        if pasted {
            cx.notify();
        }
        pasted
    }
}

impl ConnectionFormState {
    pub(crate) fn handle_key<T: 'static>(
        &mut self,
        event: &KeyDownEvent,
        caret_was_visible: bool,
        tunnel_fields: Option<&[NewConnectionField]>,
        cx: &mut Context<T>,
    ) -> (FormKeyAction, bool) {
        let saved_connection_form_uses_unloaded_secret =
            self.saved_connection_source_id().is_some()
                && self.saved_connection_prompt_action.is_none();
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        let text_input = text_from_keystroke(&event.keystroke);
        if self.open_select.is_some()
            && matches!(key, "escape" | "enter" | "tab")
            && !modifiers.platform
        {
            self.close_select();
            cx.notify();
            return (FormKeyAction::Handled, false);
        }
        let uses_text_edit_modifier = if cfg!(target_os = "macos") {
            modifiers.platform
        } else {
            modifiers.platform || modifiers.control
        };
        let is_drill_down = self.drill_down_parent_node_id.is_some();
        let Some(form) = self.form.as_mut() else {
            return (FormKeyAction::NotHandled, false);
        };
        if !form.field_focused {
            return match key {
                "escape" if form.transport_picker_open || form.appearance_popover_open => {
                    form.transport_picker_open = false;
                    form.appearance_popover_open = false;
                    cx.notify();
                    (FormKeyAction::Handled, false)
                }
                "escape" if form.jump_server_form.is_some() => {
                    (FormKeyAction::CloseJumpForm, false)
                }
                "escape" if !is_drill_down && step_back_ssh_subpage(form) => {
                    cx.notify();
                    (FormKeyAction::Handled, false)
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
                | NewConnectionField::ContainerShell
                | NewConnectionField::ContainerCommand
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
            "escape" if form.transport_picker_open || form.appearance_popover_open => {
                form.transport_picker_open = false;
                form.appearance_popover_open = false;
                cx.notify();
                (FormKeyAction::Handled, false)
            }
            "escape" if form.jump_server_form.is_some() => (FormKeyAction::CloseJumpForm, false),
            "escape" if !is_drill_down && step_back_ssh_subpage(form) => {
                cx.notify();
                (FormKeyAction::Handled, false)
            }
            "escape" => (FormKeyAction::CloseForm, false),
            "enter" if form.jump_server_form.is_some() => (FormKeyAction::AddJumpServer, false),
            "enter" => (FormKeyAction::Submit, false),
            "tab" if tunnel_fields.is_some_and(<[_]>::is_empty) => (FormKeyAction::Handled, false),
            "tab" => {
                form.focused_field = if let Some(fields) = tunnel_fields {
                    let index = fields
                        .iter()
                        .position(|field| form.field_focused && *field == form.focused_field);
                    let step = if modifiers.shift { fields.len() - 1 } else { 1 };
                    fields[index.map_or(if modifiers.shift { fields.len() - 1 } else { 0 }, |i| {
                        (i + step) % fields.len()
                    })]
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

    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn paste_text(&mut self, text: &str) -> bool {
        let saved_connection_form_uses_unloaded_secret =
            self.saved_connection_source_id().is_some()
                && self.saved_connection_prompt_action.is_none();
        let normalized = zeroize::Zeroizing::new(text.replace("\r\n", "\n"));
        let normalized = zeroize::Zeroizing::new(normalized.replace('\r', "\n"));
        (|| {
            let state = self;
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
                if !form.apply_address_paste(&single_line) {
                    insert_text_into_current_connection_field(form, &single_line);
                }
            }
            form.error = None;
            true
        })()
    }
}

impl NewConnectionForm {
    pub(crate) fn apply_address_paste(&mut self, text: &str) -> bool {
        let form = self;
        if form.focused_field == NewConnectionField::Host
            && matches!(
                form.transport,
                NewConnectionTransport::Ssh
                    | NewConnectionTransport::Mosh
                    | NewConnectionTransport::StandaloneSftp
            )
            && (form.host.trim().is_empty()
                || form.selected_field == Some(NewConnectionField::Host))
            && let Some(parsed) = parse_ssh_address_paste(text)
        {
            form.host = parsed.host;
            if let Some(username) = parsed.username {
                form.username = username;
            }
            if let Some(port) = parsed.port {
                form.port = port;
            }
            clear_connection_selection(form);
            true
        } else {
            false
        }
    }
}

fn step_back_ssh_subpage(form: &mut NewConnectionForm) -> bool {
    if form.transport == NewConnectionTransport::StandaloneSftp {
        return false;
    }
    let Some(drawer) = form.active_drawer else {
        return false;
    };
    form.active_drawer = match drawer {
        crate::form_state::ConnectionFormDrawer::SshAlgorithms
            if form.transport == NewConnectionTransport::Ssh =>
        {
            Some(crate::form_state::ConnectionFormDrawer::SshSecurity)
        }
        _ => None,
    };
    form.ssh_algorithm_menu = None;
    form.field_focused = false;
    clear_connection_selection(form);
    true
}

#[derive(Debug, Eq, PartialEq)]
struct ParsedSshAddress {
    username: Option<String>,
    host: String,
    port: Option<String>,
}

/// Parses pasted SSH commands, `ssh://` URIs, or `user@host:port` targets into
/// separate fields. Returns `None` for plain hostnames so regular paste runs unchanged.
fn parse_ssh_address_paste(raw: &str) -> Option<ParsedSshAddress> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(rest) = trimmed
        .strip_prefix("ssh://")
        .or_else(|| trimmed.strip_prefix("SSH://"))
    {
        let authority = rest.split(['/', '?', '#']).next().unwrap_or("").trim();
        return parse_ssh_authority(authority, true);
    }

    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    if tokens
        .first()
        .is_some_and(|t| t.eq_ignore_ascii_case("ssh"))
        && tokens.len() >= 2
    {
        let mut username: Option<String> = None;
        let mut port: Option<String> = None;
        let mut target: Option<&str> = None;
        let mut index = 1;
        while index < tokens.len() {
            let token = tokens[index];
            if token == "--" {
                if let Some(&next) = tokens.get(index + 1) {
                    target = Some(next);
                }
                break;
            }
            if token == "-p" {
                if let Some(&value) = tokens.get(index + 1)
                    && value.parse::<u16>().is_ok_and(|p| p > 0)
                {
                    port = Some(value.to_string());
                    index += 2;
                    continue;
                }
                return None;
            }
            if let Some(value) = token.strip_prefix("-p")
                && !value.is_empty()
                && value.parse::<u16>().is_ok_and(|p| p > 0)
            {
                port = Some(value.to_string());
                index += 1;
                continue;
            }
            if token == "-l" {
                if let Some(&value) = tokens.get(index + 1)
                    && !value.is_empty()
                {
                    username = Some(value.to_string());
                    index += 2;
                    continue;
                }
                return None;
            }
            if let Some(value) = token.strip_prefix("-l")
                && !value.is_empty()
            {
                username = Some(value.to_string());
                index += 1;
                continue;
            }
            if matches!(
                token,
                "-b" | "-B"
                    | "-c"
                    | "-D"
                    | "-E"
                    | "-e"
                    | "-F"
                    | "-I"
                    | "-i"
                    | "-J"
                    | "-L"
                    | "-m"
                    | "-O"
                    | "-o"
                    | "-P"
                    | "-Q"
                    | "-R"
                    | "-S"
                    | "-W"
                    | "-w"
            ) {
                index += 2;
                continue;
            }
            if token.starts_with('-') {
                index += 1;
                continue;
            }
            if target.is_none() {
                target = Some(token);
                index += 1;
                continue;
            }
            break;
        }
        let target = target?;
        let mut parsed = parse_ssh_authority(target, true)?;
        if parsed.username.is_none() {
            parsed.username = username;
        }
        if parsed.port.is_none() {
            parsed.port = port;
        }
        return Some(parsed);
    }

    if tokens.len() == 1 {
        return parse_ssh_authority(trimmed, false);
    }
    None
}

fn parse_ssh_authority(authority: &str, allow_bare_host: bool) -> Option<ParsedSshAddress> {
    let authority = authority.trim();
    if authority.is_empty() || authority.contains(char::is_whitespace) {
        return None;
    }
    let (username, host_port) = if let Some((userinfo, rest)) = authority.rsplit_once('@') {
        // Never copy an inline URI password into visible username/host fields.
        let user = userinfo.split(':').next().unwrap_or("").trim();
        let username = (!user.is_empty()).then(|| user.to_string());
        (username, rest)
    } else {
        (None, authority)
    };

    let (host, port) = if let Some(after_open) = host_port.strip_prefix('[') {
        let (ipv6, remainder) = after_open.split_once(']')?;
        let port = if let Some(port_str) = remainder.strip_prefix(':') {
            port_str.parse::<u16>().ok().filter(|&p| p > 0)?;
            Some(port_str.to_string())
        } else if remainder.is_empty() {
            None
        } else {
            return None;
        };
        (ipv6.trim().to_string(), port)
    } else if host_port.matches(':').count() == 1 {
        let (host_part, port_part) = host_port.split_once(':')?;
        if port_part.parse::<u16>().is_ok_and(|p| p > 0) {
            (host_part.trim().to_string(), Some(port_part.to_string()))
        } else {
            return None;
        }
    } else {
        (host_port.trim().to_string(), None)
    };

    if host.is_empty() {
        return None;
    }
    if !allow_bare_host && username.is_none() && port.is_none() {
        return None;
    }
    Some(ParsedSshAddress {
        username,
        host,
        port,
    })
}

#[cfg(test)]
mod tests {
    use super::{ParsedSshAddress, parse_ssh_address_paste};

    #[test]
    fn parse_ssh_address_paste_extracts_target_fields_and_ignores_plain_hosts() {
        let cases = [
            (
                "ssh -p 2222 deploy@prod.example.com",
                Some(ParsedSshAddress {
                    username: Some("deploy".into()),
                    host: "prod.example.com".into(),
                    port: Some("2222".into()),
                }),
            ),
            (
                "ssh -i ~/.ssh/id_ed25519 -l ops 10.0.0.8 -p2200",
                Some(ParsedSshAddress {
                    username: Some("ops".into()),
                    host: "10.0.0.8".into(),
                    port: Some("2200".into()),
                }),
            ),
            (
                "ssh://admin:s3cr3t@bastion.internal:2222/ignored",
                Some(ParsedSshAddress {
                    username: Some("admin".into()),
                    host: "bastion.internal".into(),
                    port: Some("2222".into()),
                }),
            ),
            (
                "root@[2001:db8::1]:2222",
                Some(ParsedSshAddress {
                    username: Some("root".into()),
                    host: "2001:db8::1".into(),
                    port: Some("2222".into()),
                }),
            ),
            (
                "ubuntu@192.168.1.50",
                Some(ParsedSshAddress {
                    username: Some("ubuntu".into()),
                    host: "192.168.1.50".into(),
                    port: None,
                }),
            ),
            (
                "db.internal:2202",
                Some(ParsedSshAddress {
                    username: None,
                    host: "db.internal".into(),
                    port: Some("2202".into()),
                }),
            ),
            ("192.168.1.100", None),
            ("2001:db8::1", None),
        ];

        for (input, expected) in cases {
            assert_eq!(parse_ssh_address_paste(input), expected, "input: {input}");
        }
    }
}
