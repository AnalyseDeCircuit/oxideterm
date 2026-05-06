#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum SshAuthTab {
    Password,
    DefaultKey,
    SshKey,
    Certificate,
    Agent,
    TwoFactor,
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
}

impl NewConnectionFormMode {
    pub(in crate::workspace) fn submits_saved_connection_properties(self) -> bool {
        self == Self::EditProperties
    }

    pub(in crate::workspace) fn stores_connection_on_connect(self) -> bool {
        self == Self::NewConnection
    }
}

pub(in crate::workspace) fn new_connection_form_mode(
    editing_saved_connection_id: Option<&str>,
    prompt_action: Option<SavedConnectionPromptAction>,
) -> NewConnectionFormMode {
    if prompt_action.is_some() {
        NewConnectionFormMode::SavedConnectionPrompt
    } else if editing_saved_connection_id.is_some() {
        NewConnectionFormMode::EditProperties
    } else {
        NewConnectionFormMode::NewConnection
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum NewConnectionSelect {
    Group,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(in crate::workspace) enum NewConnectionField {
    Name,
    Host,
    Port,
    Username,
    Password,
    KeyPath,
    CertPath,
    Passphrase,
    Group,
    Color,
    JumpHost,
    JumpPort,
    JumpUsername,
    JumpPassword,
    JumpKeyPath,
    JumpCertPath,
    JumpPassphrase,
}

#[derive(Clone, Debug)]
pub(in crate::workspace) struct NewConnectionProxyHop {
    pub(in crate::workspace) host: String,
    pub(in crate::workspace) port: String,
    pub(in crate::workspace) username: String,
    pub(in crate::workspace) auth_tab: SshAuthTab,
    pub(in crate::workspace) password: String,
    pub(in crate::workspace) key_path: String,
    pub(in crate::workspace) cert_path: String,
    pub(in crate::workspace) passphrase: String,
    pub(in crate::workspace) agent_forwarding: bool,
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
            cert_path: String::new(),
            passphrase: String::new(),
            agent_forwarding: false,
        }
    }

    pub(in crate::workspace) fn complete(&self) -> bool {
        !self.host.trim().is_empty() && !self.username.trim().is_empty()
    }
}

#[derive(Clone, Debug)]
pub(in crate::workspace) struct NewConnectionForm {
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
    pub(in crate::workspace) cert_path: String,
    pub(in crate::workspace) passphrase: String,
    pub(in crate::workspace) save_password: bool,
    pub(in crate::workspace) group: String,
    pub(in crate::workspace) color: String,
    pub(in crate::workspace) tags: Vec<String>,
    pub(in crate::workspace) proxy_hops: Vec<NewConnectionProxyHop>,
    pub(in crate::workspace) proxy_chain_expanded: bool,
    pub(in crate::workspace) jump_server_form: Option<NewConnectionProxyHop>,
    pub(in crate::workspace) agent_forwarding: bool,
    pub(in crate::workspace) save_connection: bool,
    pub(in crate::workspace) field_focused: bool,
    pub(in crate::workspace) focused_field: NewConnectionField,
    pub(in crate::workspace) selected_field: Option<NewConnectionField>,
    pub(in crate::workspace) error: Option<String>,
    pub(in crate::workspace) pending: bool,
}

impl Default for NewConnectionForm {
    fn default() -> Self {
        Self {
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
            cert_path: String::new(),
            passphrase: String::new(),
            save_password: false,
            group: String::new(),
            color: String::new(),
            tags: Vec::new(),
            proxy_hops: Vec::new(),
            proxy_chain_expanded: false,
            jump_server_form: None,
            agent_forwarding: false,
            save_connection: true,
            field_focused: true,
            focused_field: NewConnectionField::Name,
            selected_field: None,
            error: None,
            pending: false,
        }
    }
}

pub(in crate::workspace) fn next_connection_field(
    field: NewConnectionField,
    auth_tab: SshAuthTab,
    forward: bool,
) -> NewConnectionField {
    let fields: Vec<NewConnectionField> = match auth_tab {
        SshAuthTab::Password => vec![
            NewConnectionField::Name,
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::Username,
            NewConnectionField::Password,
            NewConnectionField::Group,
        ],
        SshAuthTab::DefaultKey => vec![
            NewConnectionField::Name,
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::Username,
            NewConnectionField::Passphrase,
            NewConnectionField::Group,
        ],
        SshAuthTab::SshKey => vec![
            NewConnectionField::Name,
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::Username,
            NewConnectionField::KeyPath,
            NewConnectionField::Passphrase,
            NewConnectionField::Group,
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
        ],
        SshAuthTab::Agent | SshAuthTab::TwoFactor => vec![
            NewConnectionField::Name,
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::Username,
            NewConnectionField::Group,
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
        NewConnectionField::CertPath => &mut form.cert_path,
        NewConnectionField::Passphrase => &mut form.passphrase,
        NewConnectionField::Group => &mut form.group,
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
        NewConnectionField::CertPath => &form.cert_path,
        NewConnectionField::Passphrase => &form.passphrase,
        NewConnectionField::Group => &form.group,
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

pub(in crate::workspace) fn backspace_current_connection_field(form: &mut NewConnectionForm) {
    if form.selected_field == Some(form.focused_field) {
        current_connection_field_mut(form).clear();
    } else {
        current_connection_field_mut(form).pop();
    }
    form.selected_field = None;
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
        backspace_current_connection_field(&mut form);
        assert!(form.username.is_empty());
        assert_eq!(form.selected_field, None);
    }

    #[test]
    fn form_mode_keeps_prompt_edit_and_new_submission_paths_distinct() {
        assert_eq!(
            new_connection_form_mode(None, None),
            NewConnectionFormMode::NewConnection
        );
        assert_eq!(
            new_connection_form_mode(Some("conn-1"), None),
            NewConnectionFormMode::EditProperties
        );
        assert_eq!(
            new_connection_form_mode(Some("conn-1"), Some(SavedConnectionPromptAction::Connect)),
            NewConnectionFormMode::SavedConnectionPrompt
        );

        assert!(NewConnectionFormMode::NewConnection.stores_connection_on_connect());
        assert!(!NewConnectionFormMode::SavedConnectionPrompt.stores_connection_on_connect());
        assert!(NewConnectionFormMode::EditProperties.submits_saved_connection_properties());
        assert!(
            !NewConnectionFormMode::SavedConnectionPrompt.submits_saved_connection_properties()
        );
    }
}
