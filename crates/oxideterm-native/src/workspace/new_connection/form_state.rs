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
pub(in crate::workspace) enum NewConnectionField {
    Name,
    Host,
    Port,
    Username,
    Password,
    Group,
}

#[derive(Clone, Debug)]
pub(in crate::workspace) struct NewConnectionForm {
    pub(in crate::workspace) name: String,
    pub(in crate::workspace) host: String,
    pub(in crate::workspace) port: String,
    pub(in crate::workspace) username: String,
    pub(in crate::workspace) auth_tab: SshAuthTab,
    pub(in crate::workspace) password: String,
    pub(in crate::workspace) save_password: bool,
    pub(in crate::workspace) group: String,
    pub(in crate::workspace) agent_forwarding: bool,
    pub(in crate::workspace) save_connection: bool,
    pub(in crate::workspace) focused_field: NewConnectionField,
    pub(in crate::workspace) error: Option<String>,
    pub(in crate::workspace) pending: bool,
}

impl Default for NewConnectionForm {
    fn default() -> Self {
        Self {
            name: String::new(),
            host: String::new(),
            port: "22".to_string(),
            username: String::new(),
            auth_tab: SshAuthTab::Password,
            password: String::new(),
            save_password: false,
            group: String::new(),
            agent_forwarding: false,
            save_connection: true,
            focused_field: NewConnectionField::Name,
            error: None,
            pending: false,
        }
    }
}

pub(in crate::workspace) fn next_connection_field(
    field: NewConnectionField,
    forward: bool,
) -> NewConnectionField {
    let fields = [
        NewConnectionField::Name,
        NewConnectionField::Host,
        NewConnectionField::Port,
        NewConnectionField::Username,
        NewConnectionField::Password,
        NewConnectionField::Group,
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
        NewConnectionField::Group => &mut form.group,
    }
}
