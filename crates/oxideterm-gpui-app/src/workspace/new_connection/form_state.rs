use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum SshAuthTab {
    Password,
    DefaultKey,
    SshKey,
    ManagedKey,
    Certificate,
    Agent,
    TwoFactor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum NewConnectionTransport {
    Ssh,
    Serial,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum SavedConnectionPromptAction {
    Connect,
    Test,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum NewConnectionFormMode {
    NewConnection,
    SavedConnectionPrompt,
    EditProperties,
    DuplicateTemplate,
}

impl NewConnectionFormMode {
    pub(in crate::workspace) fn submits_saved_connection_properties(self) -> bool {
        matches!(self, Self::EditProperties | Self::DuplicateTemplate)
    }

    pub(in crate::workspace) fn stores_connection_on_connect(self) -> bool {
        self == Self::NewConnection
    }
}

pub(in crate::workspace) fn new_connection_form_mode(
    editing_saved_connection_id: Option<&str>,
    duplicating_saved_connection_id: Option<&str>,
    prompt_action: Option<SavedConnectionPromptAction>,
) -> NewConnectionFormMode {
    if prompt_action.is_some() {
        NewConnectionFormMode::SavedConnectionPrompt
    } else if duplicating_saved_connection_id.is_some() {
        NewConnectionFormMode::DuplicateTemplate
    } else if editing_saved_connection_id.is_some() {
        NewConnectionFormMode::EditProperties
    } else {
        NewConnectionFormMode::NewConnection
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum NewConnectionSelect {
    Group,
    ManagedKey,
    JumpManagedKey,
    SerialPort,
    SerialDataBits,
    SerialStopBits,
    SerialParity,
    SerialFlowControl,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(in crate::workspace) enum NewConnectionField {
    Name,
    Host,
    Port,
    Username,
    Password,
    KeyPath,
    ManagedKeyId,
    CertPath,
    Passphrase,
    Group,
    PostConnectCommand,
    Color,
    JumpHost,
    JumpPort,
    JumpUsername,
    JumpPassword,
    JumpKeyPath,
    JumpManagedKeyId,
    JumpCertPath,
    JumpPassphrase,
    SerialPortPath,
    SerialBaudRate,
    SerialProfileName,
}

#[derive(Clone)]
pub(in crate::workspace) struct NewConnectionProxyHop {
    pub(in crate::workspace) host: String,
    pub(in crate::workspace) port: String,
    pub(in crate::workspace) username: String,
    pub(in crate::workspace) auth_tab: SshAuthTab,
    pub(in crate::workspace) password: String,
    pub(in crate::workspace) key_path: String,
    pub(in crate::workspace) managed_key_id: String,
    pub(in crate::workspace) cert_path: String,
    pub(in crate::workspace) passphrase: String,
    pub(in crate::workspace) agent_forwarding: bool,
}

impl fmt::Debug for NewConnectionProxyHop {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NewConnectionProxyHop")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("auth_tab", &self.auth_tab)
            .field("password", &"[redacted secret]")
            .field("key_path", &self.key_path)
            .field("managed_key_id", &self.managed_key_id)
            .field("cert_path", &self.cert_path)
            .field("passphrase", &"[redacted secret]")
            .field("agent_forwarding", &self.agent_forwarding)
            .finish()
    }
}

impl NewConnectionProxyHop {
    pub(in crate::workspace) fn new() -> Self {
        Self {
            host: String::new(),
            port: "22".to_string(),
            username: String::new(),
            auth_tab: SshAuthTab::SshKey,
            password: String::new(),
            key_path: String::new(),
            managed_key_id: String::new(),
            cert_path: String::new(),
            passphrase: String::new(),
            agent_forwarding: false,
        }
    }

    pub(in crate::workspace) fn complete(&self) -> bool {
        !self.host.trim().is_empty() && !self.username.trim().is_empty()
    }
}

#[derive(Clone)]
pub(in crate::workspace) struct NewConnectionForm {
    pub(in crate::workspace) transport: NewConnectionTransport,
    pub(in crate::workspace) name: String,
    pub(in crate::workspace) host: String,
    pub(in crate::workspace) port: String,
    pub(in crate::workspace) username: String,
    pub(in crate::workspace) auth_tab: SshAuthTab,
    pub(in crate::workspace) password: String,
    pub(in crate::workspace) saved_password_keychain_id: Option<String>,
    pub(in crate::workspace) password_loaded: bool,
    pub(in crate::workspace) password_visible: bool,
    pub(in crate::workspace) password_loading: bool,
    pub(in crate::workspace) password_error: Option<String>,
    pub(in crate::workspace) key_path: String,
    pub(in crate::workspace) managed_key_id: String,
    pub(in crate::workspace) cert_path: String,
    pub(in crate::workspace) passphrase: String,
    pub(in crate::workspace) save_password: bool,
    pub(in crate::workspace) group: String,
    pub(in crate::workspace) post_connect_command: String,
    pub(in crate::workspace) color: String,
    pub(in crate::workspace) tags: Vec<String>,
    pub(in crate::workspace) proxy_hops: Vec<NewConnectionProxyHop>,
    pub(in crate::workspace) proxy_chain_expanded: bool,
    pub(in crate::workspace) jump_server_form: Option<NewConnectionProxyHop>,
    pub(in crate::workspace) agent_forwarding: bool,
    pub(in crate::workspace) agent_available: Option<bool>,
    pub(in crate::workspace) save_connection: bool,
    pub(in crate::workspace) field_focused: bool,
    pub(in crate::workspace) focused_field: NewConnectionField,
    pub(in crate::workspace) selected_field: Option<NewConnectionField>,
    pub(in crate::workspace) error: Option<String>,
    pub(in crate::workspace) pending: bool,
    pub(in crate::workspace) serial_ports: Vec<oxideterm_terminal::SerialPortInfo>,
    pub(in crate::workspace) serial_ports_loading: bool,
    pub(in crate::workspace) serial_port_path: String,
    pub(in crate::workspace) serial_baud_rate: String,
    pub(in crate::workspace) serial_data_bits: u8,
    pub(in crate::workspace) serial_stop_bits: u8,
    pub(in crate::workspace) serial_parity: oxideterm_terminal::SerialParity,
    pub(in crate::workspace) serial_flow_control: oxideterm_terminal::SerialFlowControl,
    pub(in crate::workspace) save_serial_profile: bool,
    pub(in crate::workspace) serial_profile_name: String,
}

impl fmt::Debug for NewConnectionForm {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NewConnectionForm")
            .field("transport", &self.transport)
            .field("name", &self.name)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("auth_tab", &self.auth_tab)
            .field("password", &"[redacted secret]")
            .field(
                "saved_password_keychain_id",
                &self.saved_password_keychain_id,
            )
            .field("password_loaded", &self.password_loaded)
            .field("password_visible", &self.password_visible)
            .field("password_loading", &self.password_loading)
            .field("password_error", &self.password_error)
            .field("key_path", &self.key_path)
            .field("managed_key_id", &self.managed_key_id)
            .field("cert_path", &self.cert_path)
            .field("passphrase", &"[redacted secret]")
            .field("save_password", &self.save_password)
            .field("group", &self.group)
            .field("post_connect_command", &self.post_connect_command)
            .field("color", &self.color)
            .field("tags", &self.tags)
            .field("proxy_hops", &self.proxy_hops)
            .field("proxy_chain_expanded", &self.proxy_chain_expanded)
            .field("jump_server_form", &self.jump_server_form)
            .field("agent_forwarding", &self.agent_forwarding)
            .field("agent_available", &self.agent_available)
            .field("save_connection", &self.save_connection)
            .field("field_focused", &self.field_focused)
            .field("focused_field", &self.focused_field)
            .field("selected_field", &self.selected_field)
            .field("error", &self.error)
            .field("pending", &self.pending)
            .field("serial_ports", &self.serial_ports)
            .field("serial_ports_loading", &self.serial_ports_loading)
            .field("serial_port_path", &self.serial_port_path)
            .field("serial_baud_rate", &self.serial_baud_rate)
            .field("serial_data_bits", &self.serial_data_bits)
            .field("serial_stop_bits", &self.serial_stop_bits)
            .field("serial_parity", &self.serial_parity)
            .field("serial_flow_control", &self.serial_flow_control)
            .field("save_serial_profile", &self.save_serial_profile)
            .field("serial_profile_name", &self.serial_profile_name)
            .finish()
    }
}

impl Default for NewConnectionForm {
    fn default() -> Self {
        Self {
            transport: NewConnectionTransport::Ssh,
            name: String::new(),
            host: String::new(),
            port: "22".to_string(),
            username: "root".to_string(),
            auth_tab: SshAuthTab::Password,
            password: String::new(),
            saved_password_keychain_id: None,
            password_loaded: true,
            password_visible: false,
            password_loading: false,
            password_error: None,
            key_path: String::new(),
            managed_key_id: String::new(),
            cert_path: String::new(),
            passphrase: String::new(),
            save_password: false,
            group: String::new(),
            post_connect_command: String::new(),
            color: String::new(),
            tags: Vec::new(),
            proxy_hops: Vec::new(),
            proxy_chain_expanded: false,
            jump_server_form: None,
            agent_forwarding: false,
            agent_available: None,
            save_connection: false,
            field_focused: true,
            focused_field: NewConnectionField::Name,
            selected_field: None,
            error: None,
            pending: false,
            serial_ports: Vec::new(),
            serial_ports_loading: false,
            serial_port_path: String::new(),
            serial_baud_rate: "115200".to_string(),
            serial_data_bits: 8,
            serial_stop_bits: 1,
            serial_parity: oxideterm_terminal::SerialParity::None,
            serial_flow_control: oxideterm_terminal::SerialFlowControl::None,
            save_serial_profile: false,
            serial_profile_name: String::new(),
        }
    }
}

pub(in crate::workspace) fn next_connection_field(
    field: NewConnectionField,
    auth_tab: SshAuthTab,
    transport: NewConnectionTransport,
    forward: bool,
) -> NewConnectionField {
    if transport == NewConnectionTransport::Serial {
        let fields = [
            NewConnectionField::SerialPortPath,
            NewConnectionField::SerialBaudRate,
            NewConnectionField::SerialProfileName,
        ];
        let index = fields
            .iter()
            .position(|candidate| *candidate == field)
            .unwrap_or(0);
        let next = if forward {
            (index + 1) % fields.len()
        } else if index == 0 {
            fields.len() - 1
        } else {
            index - 1
        };
        return fields[next];
    }

    let fields: Vec<NewConnectionField> = match auth_tab {
        SshAuthTab::Password => vec![
            NewConnectionField::Name,
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::Username,
            NewConnectionField::Password,
            NewConnectionField::Group,
            NewConnectionField::PostConnectCommand,
        ],
        SshAuthTab::DefaultKey => vec![
            NewConnectionField::Name,
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::Username,
            NewConnectionField::Passphrase,
            NewConnectionField::Group,
            NewConnectionField::PostConnectCommand,
        ],
        SshAuthTab::SshKey => vec![
            NewConnectionField::Name,
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::Username,
            NewConnectionField::KeyPath,
            NewConnectionField::Passphrase,
            NewConnectionField::Group,
            NewConnectionField::PostConnectCommand,
        ],
        SshAuthTab::ManagedKey => vec![
            NewConnectionField::Name,
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::Username,
            NewConnectionField::ManagedKeyId,
            NewConnectionField::Passphrase,
            NewConnectionField::Group,
            NewConnectionField::PostConnectCommand,
        ],
        SshAuthTab::Certificate => vec![
            NewConnectionField::Name,
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::Username,
            NewConnectionField::KeyPath,
            NewConnectionField::CertPath,
            NewConnectionField::Passphrase,
            NewConnectionField::Group,
            NewConnectionField::PostConnectCommand,
        ],
        SshAuthTab::Agent | SshAuthTab::TwoFactor => vec![
            NewConnectionField::Name,
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::Username,
            NewConnectionField::Group,
            NewConnectionField::PostConnectCommand,
        ],
    };
    let index = fields
        .iter()
        .position(|candidate| *candidate == field)
        .unwrap_or(0);
    let next = if forward {
        (index + 1) % fields.len()
    } else if index == 0 {
        fields.len() - 1
    } else {
        index - 1
    };
    fields[next]
}

pub(in crate::workspace) fn next_jump_connection_field(
    field: NewConnectionField,
    auth_tab: SshAuthTab,
    forward: bool,
) -> NewConnectionField {
    let fields: Vec<NewConnectionField> = match auth_tab {
        SshAuthTab::Password => vec![
            NewConnectionField::JumpHost,
            NewConnectionField::JumpPort,
            NewConnectionField::JumpUsername,
            NewConnectionField::JumpPassword,
        ],
        SshAuthTab::DefaultKey | SshAuthTab::Agent => vec![
            NewConnectionField::JumpHost,
            NewConnectionField::JumpPort,
            NewConnectionField::JumpUsername,
        ],
        SshAuthTab::SshKey => vec![
            NewConnectionField::JumpHost,
            NewConnectionField::JumpPort,
            NewConnectionField::JumpUsername,
            NewConnectionField::JumpKeyPath,
            NewConnectionField::JumpPassphrase,
        ],
        SshAuthTab::ManagedKey => vec![
            NewConnectionField::JumpHost,
            NewConnectionField::JumpPort,
            NewConnectionField::JumpUsername,
            NewConnectionField::JumpManagedKeyId,
            NewConnectionField::JumpPassphrase,
        ],
        SshAuthTab::Certificate => vec![
            NewConnectionField::JumpHost,
            NewConnectionField::JumpPort,
            NewConnectionField::JumpUsername,
            NewConnectionField::JumpKeyPath,
            NewConnectionField::JumpCertPath,
            NewConnectionField::JumpPassphrase,
        ],
        SshAuthTab::TwoFactor => vec![
            NewConnectionField::JumpHost,
            NewConnectionField::JumpPort,
            NewConnectionField::JumpUsername,
        ],
    };
    let index = fields
        .iter()
        .position(|candidate| *candidate == field)
        .unwrap_or(0);
    let next = if forward {
        (index + 1) % fields.len()
    } else if index == 0 {
        fields.len() - 1
    } else {
        index - 1
    };
    fields[next]
}

pub(in crate::workspace) fn current_connection_field_mut(
    form: &mut NewConnectionForm,
) -> &mut String {
    match form.focused_field {
        NewConnectionField::Name => &mut form.name,
        NewConnectionField::Host => &mut form.host,
        NewConnectionField::Port => &mut form.port,
        NewConnectionField::Username => &mut form.username,
        NewConnectionField::Password => &mut form.password,
        NewConnectionField::KeyPath => &mut form.key_path,
        NewConnectionField::ManagedKeyId => &mut form.managed_key_id,
        NewConnectionField::CertPath => &mut form.cert_path,
        NewConnectionField::Passphrase => &mut form.passphrase,
        NewConnectionField::Group => &mut form.group,
        NewConnectionField::PostConnectCommand => &mut form.post_connect_command,
        NewConnectionField::Color => &mut form.color,
        NewConnectionField::JumpHost => {
            &mut form
                .jump_server_form
                .as_mut()
                .expect("jump host field without jump form")
                .host
        }
        NewConnectionField::JumpPort => {
            &mut form
                .jump_server_form
                .as_mut()
                .expect("jump port field without jump form")
                .port
        }
        NewConnectionField::JumpUsername => {
            &mut form
                .jump_server_form
                .as_mut()
                .expect("jump username field without jump form")
                .username
        }
        NewConnectionField::JumpPassword => {
            &mut form
                .jump_server_form
                .as_mut()
                .expect("jump password field without jump form")
                .password
        }
        NewConnectionField::JumpKeyPath => {
            &mut form
                .jump_server_form
                .as_mut()
                .expect("jump key path field without jump form")
                .key_path
        }
        NewConnectionField::JumpManagedKeyId => {
            &mut form
                .jump_server_form
                .as_mut()
                .expect("jump managed key field without jump form")
                .managed_key_id
        }
        NewConnectionField::JumpCertPath => {
            &mut form
                .jump_server_form
                .as_mut()
                .expect("jump cert path field without jump form")
                .cert_path
        }
        NewConnectionField::JumpPassphrase => {
            &mut form
                .jump_server_form
                .as_mut()
                .expect("jump passphrase field without jump form")
                .passphrase
        }
        NewConnectionField::SerialPortPath => &mut form.serial_port_path,
        NewConnectionField::SerialBaudRate => &mut form.serial_baud_rate,
        NewConnectionField::SerialProfileName => &mut form.serial_profile_name,
    }
}

pub(in crate::workspace) fn current_connection_field(form: &NewConnectionForm) -> &str {
    match form.focused_field {
        NewConnectionField::Name => &form.name,
        NewConnectionField::Host => &form.host,
        NewConnectionField::Port => &form.port,
        NewConnectionField::Username => &form.username,
        NewConnectionField::Password => &form.password,
        NewConnectionField::KeyPath => &form.key_path,
        NewConnectionField::ManagedKeyId => &form.managed_key_id,
        NewConnectionField::CertPath => &form.cert_path,
        NewConnectionField::Passphrase => &form.passphrase,
        NewConnectionField::Group => &form.group,
        NewConnectionField::PostConnectCommand => &form.post_connect_command,
        NewConnectionField::Color => &form.color,
        NewConnectionField::JumpHost => {
            &form
                .jump_server_form
                .as_ref()
                .expect("jump host field without jump form")
                .host
        }
        NewConnectionField::JumpPort => {
            &form
                .jump_server_form
                .as_ref()
                .expect("jump port field without jump form")
                .port
        }
        NewConnectionField::JumpUsername => {
            &form
                .jump_server_form
                .as_ref()
                .expect("jump username field without jump form")
                .username
        }
        NewConnectionField::JumpPassword => {
            &form
                .jump_server_form
                .as_ref()
                .expect("jump password field without jump form")
                .password
        }
        NewConnectionField::JumpKeyPath => {
            &form
                .jump_server_form
                .as_ref()
                .expect("jump key path field without jump form")
                .key_path
        }
        NewConnectionField::JumpManagedKeyId => {
            &form
                .jump_server_form
                .as_ref()
                .expect("jump managed key field without jump form")
                .managed_key_id
        }
        NewConnectionField::JumpCertPath => {
            &form
                .jump_server_form
                .as_ref()
                .expect("jump cert path field without jump form")
                .cert_path
        }
        NewConnectionField::JumpPassphrase => {
            &form
                .jump_server_form
                .as_ref()
                .expect("jump passphrase field without jump form")
                .passphrase
        }
        NewConnectionField::SerialPortPath => &form.serial_port_path,
        NewConnectionField::SerialBaudRate => &form.serial_baud_rate,
        NewConnectionField::SerialProfileName => &form.serial_profile_name,
    }
}

pub(in crate::workspace) fn select_current_connection_field(form: &mut NewConnectionForm) {
    if current_connection_field(form).is_empty() {
        form.selected_field = None;
    } else {
        form.selected_field = Some(form.focused_field);
    }
}

pub(in crate::workspace) fn clear_connection_selection(form: &mut NewConnectionForm) {
    form.selected_field = None;
}

pub(in crate::workspace) fn connection_field_is_selected(
    form: &NewConnectionForm,
    field: NewConnectionField,
) -> bool {
    form.selected_field == Some(field)
}

pub(in crate::workspace) fn insert_text_into_current_connection_field(
    form: &mut NewConnectionForm,
    text: &str,
) {
    let replacing_selection = form.selected_field == Some(form.focused_field);
    if replacing_selection {
        current_connection_field_mut(form).clear();
    }
    current_connection_field_mut(form).push_str(text);
    form.selected_field = None;
}

pub(in crate::workspace) fn backspace_current_connection_field(
    form: &mut NewConnectionForm,
) -> bool {
    let selection_was_visible = form.selected_field.is_some();
    if form.selected_field == Some(form.focused_field) {
        // Clearing a selected field also clears visible selection state. Track
        // text separately so empty selected fields still report a UI change.
        let field = current_connection_field_mut(form);
        let text_changed = !field.is_empty();
        field.clear();
        form.selected_field = None;
        text_changed || selection_was_visible
    } else {
        let text_changed = current_connection_field_mut(form).pop().is_some();
        form.selected_field = None;
        text_changed || selection_was_visible
    }
}

pub(in crate::workspace) fn clear_current_connection_field(form: &mut NewConnectionForm) {
    current_connection_field_mut(form).clear();
    form.selected_field = None;
}

pub(in crate::workspace) fn text_from_keystroke(keystroke: &gpui::Keystroke) -> Option<&str> {
    if keystroke.modifiers.platform || keystroke.modifiers.control {
        return None;
    }
    let text = keystroke.key_char.as_deref()?;
    if text.is_empty() || text.chars().any(char::is_control) {
        return None;
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use gpui::{Keystroke, Modifiers};

    use super::{
        NewConnectionField, NewConnectionForm, NewConnectionFormMode, SavedConnectionPromptAction,
        backspace_current_connection_field, insert_text_into_current_connection_field,
        new_connection_form_mode, select_current_connection_field, text_from_keystroke,
    };

    fn keystroke(key: &str, key_char: Option<&str>, modifiers: Modifiers) -> Keystroke {
        Keystroke {
            modifiers,
            key: key.to_string(),
            key_char: key_char.map(str::to_string),
        }
    }

    #[test]
    fn text_input_uses_platform_text_not_binding_key() {
        let shifted = keystroke(
            "1",
            Some("!"),
            Modifiers {
                shift: true,
                ..Modifiers::default()
            },
        );
        let option_char = keystroke(
            "s",
            Some("ß"),
            Modifiers {
                alt: true,
                ..Modifiers::default()
            },
        );

        assert_eq!(text_from_keystroke(&shifted), Some("!"));
        assert_eq!(text_from_keystroke(&option_char), Some("ß"));
    }

    #[test]
    fn text_input_ignores_shortcut_keystrokes() {
        let shortcut = keystroke(
            "v",
            None,
            Modifiers {
                platform: true,
                ..Modifiers::default()
            },
        );
        let control = keystroke(
            "a",
            Some("\u{1}"),
            Modifiers {
                control: true,
                ..Modifiers::default()
            },
        );

        assert_eq!(text_from_keystroke(&shortcut), None);
        assert_eq!(text_from_keystroke(&control), None);
    }

    #[test]
    fn selected_text_is_replaced_by_committed_input() {
        let mut form = NewConnectionForm {
            host: "example.test".to_string(),
            focused_field: NewConnectionField::Host,
            ..NewConnectionForm::default()
        };
        select_current_connection_field(&mut form);
        insert_text_into_current_connection_field(&mut form, "192.168.1.10");
        assert_eq!(form.host, "192.168.1.10");
        assert_eq!(form.selected_field, None);
    }

    #[test]
    fn backspace_clears_selected_field() {
        let mut form = NewConnectionForm {
            username: "root".to_string(),
            focused_field: NewConnectionField::Username,
            ..NewConnectionForm::default()
        };
        select_current_connection_field(&mut form);
        assert!(backspace_current_connection_field(&mut form));
        assert!(form.username.is_empty());
        assert_eq!(form.selected_field, None);
    }

    #[test]
    fn backspace_reports_text_changes_without_selection() {
        let mut form = NewConnectionForm {
            username: "root".to_string(),
            focused_field: NewConnectionField::Username,
            ..NewConnectionForm::default()
        };

        assert!(backspace_current_connection_field(&mut form));
        assert_eq!(form.username, "roo");
        assert_eq!(form.selected_field, None);
    }

    #[test]
    fn backspace_reports_false_for_empty_unselected_field() {
        let mut form = NewConnectionForm {
            focused_field: NewConnectionField::Name,
            ..NewConnectionForm::default()
        };

        assert!(!backspace_current_connection_field(&mut form));
        assert_eq!(form.selected_field, None);
    }

    #[test]
    fn backspace_clears_stale_selection_state() {
        let mut form = NewConnectionForm {
            focused_field: NewConnectionField::Username,
            selected_field: Some(NewConnectionField::Host),
            ..NewConnectionForm::default()
        };

        assert!(backspace_current_connection_field(&mut form));
        assert_eq!(form.selected_field, None);
    }

    #[test]
    fn form_mode_keeps_prompt_edit_and_new_submission_paths_distinct() {
        assert_eq!(
            new_connection_form_mode(None, None, None),
            NewConnectionFormMode::NewConnection
        );
        assert_eq!(
            new_connection_form_mode(Some("conn-1"), None, None),
            NewConnectionFormMode::EditProperties
        );
        assert_eq!(
            new_connection_form_mode(None, Some("conn-1"), None),
            NewConnectionFormMode::DuplicateTemplate
        );
        assert_eq!(
            new_connection_form_mode(
                Some("conn-1"),
                Some("conn-2"),
                Some(SavedConnectionPromptAction::Connect)
            ),
            NewConnectionFormMode::SavedConnectionPrompt
        );

        assert!(NewConnectionFormMode::NewConnection.stores_connection_on_connect());
        assert!(!NewConnectionFormMode::SavedConnectionPrompt.stores_connection_on_connect());
        assert!(NewConnectionFormMode::EditProperties.submits_saved_connection_properties());
        assert!(NewConnectionFormMode::DuplicateTemplate.submits_saved_connection_properties());
        assert!(
            !NewConnectionFormMode::SavedConnectionPrompt.submits_saved_connection_properties()
        );
    }
}
