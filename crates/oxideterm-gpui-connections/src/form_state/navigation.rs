use super::*;

pub fn next_connection_field(
    field: NewConnectionField,
    auth_tab: SshAuthTab,
    gssapi_enabled: bool,
    transport: NewConnectionTransport,
    upstream_proxy_policy: NewConnectionUpstreamProxyPolicy,
    upstream_proxy_auth: NewConnectionUpstreamProxyAuth,
    empty_password: bool,
    forward: bool,
) -> NewConnectionField {
    if transport == NewConnectionTransport::LocalTerminal {
        let fields = [
            NewConnectionField::Name,
            NewConnectionField::LocalCwd,
            NewConnectionField::Group,
        ];
        let index = fields
            .iter()
            .position(|candidate| *candidate == field)
            .unwrap_or(0);
        return fields[(index + if forward { 1 } else { fields.len() - 1 }) % fields.len()];
    }
    if transport == NewConnectionTransport::WslGraphics {
        return NewConnectionField::Name;
    }
    if transport == NewConnectionTransport::Serial {
        let fields = [
            NewConnectionField::SerialProfileName,
            NewConnectionField::SerialPortPath,
            NewConnectionField::SerialBaudRate,
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
    if matches!(
        transport,
        NewConnectionTransport::Telnet | NewConnectionTransport::Ftp
    ) {
        let mut fields = if transport == NewConnectionTransport::Ftp {
            vec![
                NewConnectionField::Name,
                NewConnectionField::Host,
                NewConnectionField::Port,
                NewConnectionField::Notes,
                NewConnectionField::Username,
                NewConnectionField::Password,
                NewConnectionField::InitialRemotePath,
                NewConnectionField::ConnectTimeoutSeconds,
            ]
        } else {
            vec![
                NewConnectionField::TelnetProfileName,
                NewConnectionField::Host,
                NewConnectionField::Port,
                NewConnectionField::Notes,
            ]
        };
        if upstream_proxy_policy == NewConnectionUpstreamProxyPolicy::Custom {
            fields.extend([
                NewConnectionField::UpstreamProxyHost,
                NewConnectionField::UpstreamProxyPort,
                NewConnectionField::UpstreamProxyNoProxy,
            ]);
            if upstream_proxy_auth == NewConnectionUpstreamProxyAuth::Password {
                fields.extend([
                    NewConnectionField::UpstreamProxyUsername,
                    NewConnectionField::UpstreamProxyPassword,
                ]);
            }
        }
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
    if matches!(
        transport,
        NewConnectionTransport::Rdp | NewConnectionTransport::Vnc
    ) {
        let mut fields = vec![
            NewConnectionField::Name,
            NewConnectionField::Group,
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::Notes,
            NewConnectionField::Username,
        ];
        if !empty_password {
            fields.push(NewConnectionField::Password);
        }
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

    let mut fields: Vec<NewConnectionField> = match auth_tab {
        SshAuthTab::Password => vec![
            NewConnectionField::Name,
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::Username,
            NewConnectionField::Group,
            NewConnectionField::Notes,
            NewConnectionField::Password,
            NewConnectionField::PreConnectCommand,
            NewConnectionField::PostConnectCommand,
        ],
        SshAuthTab::DefaultKey => vec![
            NewConnectionField::Name,
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::Username,
            NewConnectionField::Group,
            NewConnectionField::Notes,
            NewConnectionField::Passphrase,
            NewConnectionField::PreConnectCommand,
            NewConnectionField::PostConnectCommand,
        ],
        SshAuthTab::SshKey => vec![
            NewConnectionField::Name,
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::Username,
            NewConnectionField::Group,
            NewConnectionField::Notes,
            NewConnectionField::KeyPath,
            NewConnectionField::Passphrase,
            NewConnectionField::PreConnectCommand,
            NewConnectionField::PostConnectCommand,
        ],
        SshAuthTab::ManagedKey => vec![
            NewConnectionField::Name,
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::Username,
            NewConnectionField::Group,
            NewConnectionField::Notes,
            NewConnectionField::ManagedKeyId,
            NewConnectionField::Passphrase,
            NewConnectionField::PreConnectCommand,
            NewConnectionField::PostConnectCommand,
        ],
        SshAuthTab::Certificate => vec![
            NewConnectionField::Name,
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::Username,
            NewConnectionField::Group,
            NewConnectionField::Notes,
            NewConnectionField::KeyPath,
            NewConnectionField::CertPath,
            NewConnectionField::Passphrase,
            NewConnectionField::PreConnectCommand,
            NewConnectionField::PostConnectCommand,
        ],
        SshAuthTab::Agent => vec![
            NewConnectionField::Name,
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::Username,
            NewConnectionField::Group,
            NewConnectionField::Notes,
            NewConnectionField::IdentityAgent,
            NewConnectionField::PreConnectCommand,
            NewConnectionField::PostConnectCommand,
        ],
        SshAuthTab::TwoFactor => vec![
            NewConnectionField::Name,
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::Username,
            NewConnectionField::Group,
            NewConnectionField::Notes,
            NewConnectionField::PreConnectCommand,
            NewConnectionField::PostConnectCommand,
        ],
    };
    if gssapi_enabled {
        fields.insert(6, NewConnectionField::GssapiServerIdentity);
    }
    if transport == NewConnectionTransport::Mosh {
        fields.retain(|field| {
            !matches!(
                field,
                NewConnectionField::Notes | NewConnectionField::PostConnectCommand
            )
        });
        fields.extend([
            NewConnectionField::MoshServerExecutable,
            NewConnectionField::MoshUdpHost,
            NewConnectionField::MoshUdpPort,
            NewConnectionField::MoshLocale,
        ]);
    } else if transport == NewConnectionTransport::StandaloneSftp {
        // Independent SFTP uses SSH authentication and routing without terminal-only commands.
        fields.retain(|field| *field != NewConnectionField::PostConnectCommand);
        fields.push(NewConnectionField::InitialRemotePath);
    }
    fields.push(NewConnectionField::ConnectTimeoutSeconds);
    if upstream_proxy_policy == NewConnectionUpstreamProxyPolicy::Custom
        && matches!(
            transport,
            NewConnectionTransport::Ssh
                | NewConnectionTransport::StandaloneSftp
                | NewConnectionTransport::Rdp
        )
    {
        fields.extend([
            NewConnectionField::UpstreamProxyHost,
            NewConnectionField::UpstreamProxyPort,
            NewConnectionField::UpstreamProxyNoProxy,
        ]);
        if upstream_proxy_auth == NewConnectionUpstreamProxyAuth::Password {
            fields.extend([
                NewConnectionField::UpstreamProxyUsername,
                NewConnectionField::UpstreamProxyPassword,
            ]);
        }
    }
    if empty_password {
        fields.retain(|field| *field != NewConnectionField::Password);
    }
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

pub fn next_jump_connection_field(
    field: NewConnectionField,
    auth_tab: SshAuthTab,
    gssapi_enabled: bool,
    empty_password: bool,
    forward: bool,
) -> NewConnectionField {
    let mut fields: Vec<NewConnectionField> = match auth_tab {
        SshAuthTab::Password => vec![
            NewConnectionField::JumpHost,
            NewConnectionField::JumpPort,
            NewConnectionField::JumpUsername,
            NewConnectionField::JumpPassword,
        ],
        SshAuthTab::DefaultKey => vec![
            NewConnectionField::JumpHost,
            NewConnectionField::JumpPort,
            NewConnectionField::JumpUsername,
        ],
        SshAuthTab::Agent => vec![
            NewConnectionField::JumpHost,
            NewConnectionField::JumpPort,
            NewConnectionField::JumpUsername,
            NewConnectionField::JumpIdentityAgent,
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
    if gssapi_enabled {
        fields.insert(3, NewConnectionField::JumpGssapiServerIdentity);
    }
    if empty_password {
        fields.retain(|field| *field != NewConnectionField::JumpPassword);
    }
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

pub fn next_standalone_sftp_field(form: &NewConnectionForm, forward: bool) -> NewConnectionField {
    fn append_auth_fields(
        fields: &mut Vec<NewConnectionField>,
        auth_tab: SshAuthTab,
        gssapi_enabled: bool,
        secondary: bool,
    ) {
        if gssapi_enabled {
            fields.push(if secondary {
                NewConnectionField::StandaloneSftpSecondaryGssapiServerIdentity
            } else {
                NewConnectionField::GssapiServerIdentity
            });
        }
        match (auth_tab, secondary) {
            (SshAuthTab::Password, false) => fields.push(NewConnectionField::Password),
            (SshAuthTab::Password, true) => {
                fields.push(NewConnectionField::StandaloneSftpSecondaryPassword)
            }
            (SshAuthTab::DefaultKey, false) => fields.push(NewConnectionField::Passphrase),
            (SshAuthTab::DefaultKey, true) => {
                fields.push(NewConnectionField::StandaloneSftpSecondaryPassphrase)
            }
            (SshAuthTab::SshKey, false) => {
                fields.extend([NewConnectionField::KeyPath, NewConnectionField::Passphrase])
            }
            (SshAuthTab::SshKey, true) => fields.extend([
                NewConnectionField::StandaloneSftpSecondaryKeyPath,
                NewConnectionField::StandaloneSftpSecondaryPassphrase,
            ]),
            (SshAuthTab::ManagedKey, false) => fields.extend([
                NewConnectionField::ManagedKeyId,
                NewConnectionField::Passphrase,
            ]),
            (SshAuthTab::ManagedKey, true) => fields.extend([
                NewConnectionField::StandaloneSftpSecondaryManagedKeyId,
                NewConnectionField::StandaloneSftpSecondaryPassphrase,
            ]),
            (SshAuthTab::Certificate, false) => fields.extend([
                NewConnectionField::KeyPath,
                NewConnectionField::CertPath,
                NewConnectionField::Passphrase,
            ]),
            (SshAuthTab::Certificate, true) => fields.extend([
                NewConnectionField::StandaloneSftpSecondaryKeyPath,
                NewConnectionField::StandaloneSftpSecondaryCertPath,
                NewConnectionField::StandaloneSftpSecondaryPassphrase,
            ]),
            (SshAuthTab::Agent, false) => fields.push(NewConnectionField::IdentityAgent),
            (SshAuthTab::Agent, true) => {
                fields.push(NewConnectionField::StandaloneSftpSecondaryIdentityAgent)
            }
            (SshAuthTab::TwoFactor, _) => {}
        }
    }

    let mut fields = vec![
        NewConnectionField::Name,
        NewConnectionField::Group,
        NewConnectionField::Notes,
        NewConnectionField::Host,
        NewConnectionField::Port,
        NewConnectionField::Username,
    ];
    append_auth_fields(&mut fields, form.auth_tab, form.gssapi_enabled, false);
    fields.push(NewConnectionField::InitialRemotePath);
    fields.push(NewConnectionField::ConnectTimeoutSeconds);
    if form.standalone_sftp_transfer_mode == StandaloneSftpTransferMode::RemoteRemote {
        fields.extend([
            NewConnectionField::StandaloneSftpSecondaryHost,
            NewConnectionField::StandaloneSftpSecondaryPort,
            NewConnectionField::StandaloneSftpSecondaryUsername,
        ]);
        append_auth_fields(
            &mut fields,
            form.standalone_sftp_secondary.auth_tab,
            form.standalone_sftp_secondary.gssapi_enabled,
            true,
        );
        fields.push(NewConnectionField::StandaloneSftpSecondaryInitialRemotePath);
        fields.push(NewConnectionField::StandaloneSftpSecondaryConnectTimeoutSeconds);
    }
    fields.retain(|field| match field {
        NewConnectionField::Password => !form.empty_password,
        NewConnectionField::StandaloneSftpSecondaryPassword => {
            !form.standalone_sftp_secondary.empty_password
        }
        _ => true,
    });
    let index = fields
        .iter()
        .position(|candidate| *candidate == form.focused_field)
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
