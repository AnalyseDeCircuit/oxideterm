use oxideterm_editor_core::utf16::{byte_index_for_utf16, replace_utf16};
use zeroize::Zeroize;

use crate::{
    ConnectionFormState, NewConnectionField, NewConnectionForm,
    form_state::restore_saved_password_placeholder_if_empty,
    form_state::{password_draft_mut, refresh_connection_timeout_seconds},
};

pub(crate) fn protected(f: NewConnectionField) -> bool {
    use NewConnectionField::*;
    matches!(
        f,
        Password
            | Passphrase
            | JumpPassword
            | JumpPassphrase
            | UpstreamProxyPassword
            | ProxyCommand
            | StandaloneSftpSecondaryPassword
            | StandaloneSftpSecondaryPassphrase
            | StandaloneSftpSecondaryUpstreamProxyPassword
            | StandaloneSftpSecondaryProxyCommand
    )
}
pub(crate) fn replace_text(
    state: &mut ConnectionFormState,
    field: NewConnectionField,
    replacement_range: Option<std::ops::Range<usize>>,
    text: &str,
) -> bool {
    let editing_saved = state.saved_connection_source_id().is_some()
        && state.saved_connection_prompt_action.is_none();
    let Some(form) = state.form.as_mut() else {
        return false;
    };
    if form.selected_field == Some(field) && replacement_range.is_none() {
        // A revealed password may own the old allocation; scrub before replacement.
        connection_field_value_mut(form, field).zeroize();
    }
    let value = connection_field_value_mut(form, field);
    if protected(field) {
        // In-place growth can return an allocation containing a password to the allocator.
        let (start, end) = replacement_range
            .map(|r| {
                (
                    byte_index_for_utf16(value, r.start),
                    byte_index_for_utf16(value, r.end),
                )
            })
            .unwrap_or((value.len(), value.len()));
        let mut next = zeroize::Zeroizing::new(String::with_capacity(
            value.len() - (end - start) + text.len(),
        ));
        next.push_str(&value[..start]);
        next.push_str(text);
        next.push_str(&value[end..]);
        std::mem::swap(value, &mut *next);
    } else {
        replace_utf16(value, replacement_range, text);
    }

    if field == NewConnectionField::Password {
        restore_saved_password_placeholder_if_empty(form, editing_saved);
    }
    form.selected_field = None;
    form.error = None;
    refresh_connection_timeout_seconds(form, field);

    true
}

pub(crate) fn field_value(form: &NewConnectionForm, field: NewConnectionField) -> Option<&str> {
    Some(match field {
        NewConnectionField::Name => &form.name,
        NewConnectionField::LocalCwd => &form.local_cwd,
        NewConnectionField::Host => &form.host,
        NewConnectionField::TunnelTargetHost => &form.tunnel_target_host,
        NewConnectionField::TunnelTargetPort => &form.tunnel_target_port,
        NewConnectionField::Port => &form.port,
        NewConnectionField::Username => &form.username,
        NewConnectionField::Password => &form.password,
        NewConnectionField::KeyPath => &form.key_path,
        NewConnectionField::ManagedKeyId => &form.managed_key_id,
        NewConnectionField::CertPath => &form.cert_path,
        NewConnectionField::Passphrase => &form.passphrase,
        NewConnectionField::GssapiServerIdentity => &form.gssapi_server_identity,
        NewConnectionField::IdentityAgent => &form.identity_agent,
        NewConnectionField::Group => &form.group,
        NewConnectionField::Notes => &form.notes,
        NewConnectionField::PostConnectCommand => &form.post_connect_command,
        NewConnectionField::PreConnectCommand => &form.pre_connect_command,
        NewConnectionField::ProxyCommand => &form.proxy_command,
        NewConnectionField::UpstreamProxyHost => &form.upstream_proxy_host,
        NewConnectionField::UpstreamProxyPort => &form.upstream_proxy_port,
        NewConnectionField::UpstreamProxyNoProxy => &form.upstream_proxy_no_proxy,
        NewConnectionField::UpstreamProxyUsername => &form.upstream_proxy_username,
        NewConnectionField::UpstreamProxyPassword => &form.upstream_proxy_password,
        NewConnectionField::Color => &form.color,
        NewConnectionField::IconBackgroundColor => &form.icon_background_color,
        NewConnectionField::SerialPortPath => &form.serial_port_path,
        NewConnectionField::SerialBaudRate => &form.serial_baud_rate,
        NewConnectionField::SerialProfileName => &form.serial_profile_name,
        NewConnectionField::TelnetProfileName => &form.telnet_profile_name,
        NewConnectionField::BleProfileName => &form.ble.profile_name,
        NewConnectionField::BleMtu => &form.ble.mtu,
        NewConnectionField::MoshServerExecutable => &form.mosh_server_executable,
        NewConnectionField::MoshUdpHost => &form.mosh_udp_host,
        NewConnectionField::MoshUdpPort => &form.mosh_udp_port,
        NewConnectionField::MoshLocale => &form.mosh_locale,
        NewConnectionField::InitialRemotePath => &form.sftp_initial_remote_path,
        NewConnectionField::ConnectTimeoutSeconds => &form.connect_timeout_seconds_text,
        NewConnectionField::KeepAliveIntervalSeconds => &form.keep_alive_interval_seconds_text,
        NewConnectionField::KeepAliveCountMax => &form.keep_alive_count_max_text,
        NewConnectionField::StandaloneSftpSecondaryHost => &form.standalone_sftp_secondary.host,
        NewConnectionField::StandaloneSftpSecondaryPort => &form.standalone_sftp_secondary.port,
        NewConnectionField::StandaloneSftpSecondaryUsername => {
            &form.standalone_sftp_secondary.username
        }
        NewConnectionField::StandaloneSftpSecondaryPassword => {
            &form.standalone_sftp_secondary.password
        }
        NewConnectionField::StandaloneSftpSecondaryKeyPath => {
            &form.standalone_sftp_secondary.key_path
        }
        NewConnectionField::StandaloneSftpSecondaryManagedKeyId => {
            &form.standalone_sftp_secondary.managed_key_id
        }
        NewConnectionField::StandaloneSftpSecondaryCertPath => {
            &form.standalone_sftp_secondary.cert_path
        }
        NewConnectionField::StandaloneSftpSecondaryPassphrase => {
            &form.standalone_sftp_secondary.passphrase
        }
        NewConnectionField::StandaloneSftpSecondaryGssapiServerIdentity => {
            &form.standalone_sftp_secondary.gssapi_server_identity
        }
        NewConnectionField::StandaloneSftpSecondaryIdentityAgent => {
            &form.standalone_sftp_secondary.identity_agent
        }
        NewConnectionField::StandaloneSftpSecondaryInitialRemotePath => {
            &form.standalone_sftp_secondary.initial_remote_path
        }
        NewConnectionField::StandaloneSftpSecondaryConnectTimeoutSeconds => {
            &form.standalone_sftp_secondary.connect_timeout_seconds_text
        }
        NewConnectionField::StandaloneSftpSecondaryProxyCommand => {
            &form.standalone_sftp_secondary.proxy_command
        }
        NewConnectionField::StandaloneSftpSecondaryUpstreamProxyHost => {
            &form.standalone_sftp_secondary.upstream_proxy_host
        }
        NewConnectionField::StandaloneSftpSecondaryUpstreamProxyPort => {
            &form.standalone_sftp_secondary.upstream_proxy_port
        }
        NewConnectionField::StandaloneSftpSecondaryUpstreamProxyNoProxy => {
            &form.standalone_sftp_secondary.upstream_proxy_no_proxy
        }
        NewConnectionField::StandaloneSftpSecondaryUpstreamProxyUsername => {
            &form.standalone_sftp_secondary.upstream_proxy_username
        }
        NewConnectionField::StandaloneSftpSecondaryUpstreamProxyPassword => {
            &form.standalone_sftp_secondary.upstream_proxy_password
        }
        NewConnectionField::JumpHost => &form.jump_server_form.as_ref()?.host,
        NewConnectionField::JumpPort => &form.jump_server_form.as_ref()?.port,
        NewConnectionField::JumpUsername => &form.jump_server_form.as_ref()?.username,
        NewConnectionField::JumpPassword => &form.jump_server_form.as_ref()?.password,
        NewConnectionField::JumpKeyPath => &form.jump_server_form.as_ref()?.key_path,
        NewConnectionField::JumpManagedKeyId => &form.jump_server_form.as_ref()?.managed_key_id,
        NewConnectionField::JumpCertPath => &form.jump_server_form.as_ref()?.cert_path,
        NewConnectionField::JumpPassphrase => &form.jump_server_form.as_ref()?.passphrase,
        NewConnectionField::JumpGssapiServerIdentity => {
            &form.jump_server_form.as_ref()?.gssapi_server_identity
        }
        NewConnectionField::JumpIdentityAgent => &form.jump_server_form.as_ref()?.identity_agent,
    })
}

fn connection_field_value_mut(
    form: &mut NewConnectionForm,
    field: NewConnectionField,
) -> &mut String {
    match field {
        NewConnectionField::Name => &mut form.name,
        NewConnectionField::LocalCwd => &mut form.local_cwd,
        NewConnectionField::Host => &mut form.host,
        NewConnectionField::TunnelTargetHost => &mut form.tunnel_target_host,
        NewConnectionField::TunnelTargetPort => &mut form.tunnel_target_port,
        NewConnectionField::Port => &mut form.port,
        NewConnectionField::Username => &mut form.username,
        NewConnectionField::Password => password_draft_mut(form),
        NewConnectionField::KeyPath => &mut form.key_path,
        NewConnectionField::ManagedKeyId => &mut form.managed_key_id,
        NewConnectionField::CertPath => &mut form.cert_path,
        NewConnectionField::Passphrase => &mut form.passphrase,
        NewConnectionField::GssapiServerIdentity => &mut form.gssapi_server_identity,
        NewConnectionField::IdentityAgent => &mut form.identity_agent,
        NewConnectionField::Group => &mut form.group,
        NewConnectionField::Notes => &mut form.notes,
        NewConnectionField::PostConnectCommand => &mut form.post_connect_command,
        NewConnectionField::PreConnectCommand => &mut form.pre_connect_command,
        NewConnectionField::ProxyCommand => &mut form.proxy_command,
        NewConnectionField::UpstreamProxyHost => &mut form.upstream_proxy_host,
        NewConnectionField::UpstreamProxyPort => &mut form.upstream_proxy_port,
        NewConnectionField::UpstreamProxyNoProxy => &mut form.upstream_proxy_no_proxy,
        NewConnectionField::UpstreamProxyUsername => &mut form.upstream_proxy_username,
        NewConnectionField::UpstreamProxyPassword => &mut form.upstream_proxy_password,
        NewConnectionField::Color => &mut form.color,
        NewConnectionField::IconBackgroundColor => &mut form.icon_background_color,
        NewConnectionField::SerialPortPath => &mut form.serial_port_path,
        NewConnectionField::SerialBaudRate => &mut form.serial_baud_rate,
        NewConnectionField::SerialProfileName => &mut form.serial_profile_name,
        NewConnectionField::TelnetProfileName => &mut form.telnet_profile_name,
        NewConnectionField::BleProfileName => &mut form.ble.profile_name,
        NewConnectionField::BleMtu => &mut form.ble.mtu,
        NewConnectionField::MoshServerExecutable => &mut form.mosh_server_executable,
        NewConnectionField::MoshUdpHost => &mut form.mosh_udp_host,
        NewConnectionField::MoshUdpPort => &mut form.mosh_udp_port,
        NewConnectionField::MoshLocale => &mut form.mosh_locale,
        NewConnectionField::InitialRemotePath => &mut form.sftp_initial_remote_path,
        NewConnectionField::ConnectTimeoutSeconds => &mut form.connect_timeout_seconds_text,
        NewConnectionField::KeepAliveIntervalSeconds => &mut form.keep_alive_interval_seconds_text,
        NewConnectionField::KeepAliveCountMax => &mut form.keep_alive_count_max_text,
        NewConnectionField::StandaloneSftpSecondaryHost => &mut form.standalone_sftp_secondary.host,
        NewConnectionField::StandaloneSftpSecondaryPort => &mut form.standalone_sftp_secondary.port,
        NewConnectionField::StandaloneSftpSecondaryUsername => {
            &mut form.standalone_sftp_secondary.username
        }
        NewConnectionField::StandaloneSftpSecondaryPassword => {
            &mut form.standalone_sftp_secondary.password
        }
        NewConnectionField::StandaloneSftpSecondaryKeyPath => {
            &mut form.standalone_sftp_secondary.key_path
        }
        NewConnectionField::StandaloneSftpSecondaryManagedKeyId => {
            &mut form.standalone_sftp_secondary.managed_key_id
        }
        NewConnectionField::StandaloneSftpSecondaryCertPath => {
            &mut form.standalone_sftp_secondary.cert_path
        }
        NewConnectionField::StandaloneSftpSecondaryPassphrase => {
            &mut form.standalone_sftp_secondary.passphrase
        }
        NewConnectionField::StandaloneSftpSecondaryGssapiServerIdentity => {
            &mut form.standalone_sftp_secondary.gssapi_server_identity
        }
        NewConnectionField::StandaloneSftpSecondaryIdentityAgent => {
            &mut form.standalone_sftp_secondary.identity_agent
        }
        NewConnectionField::StandaloneSftpSecondaryInitialRemotePath => {
            &mut form.standalone_sftp_secondary.initial_remote_path
        }
        NewConnectionField::StandaloneSftpSecondaryConnectTimeoutSeconds => {
            &mut form.standalone_sftp_secondary.connect_timeout_seconds_text
        }
        NewConnectionField::StandaloneSftpSecondaryProxyCommand => {
            &mut form.standalone_sftp_secondary.proxy_command
        }
        NewConnectionField::StandaloneSftpSecondaryUpstreamProxyHost => {
            &mut form.standalone_sftp_secondary.upstream_proxy_host
        }
        NewConnectionField::StandaloneSftpSecondaryUpstreamProxyPort => {
            &mut form.standalone_sftp_secondary.upstream_proxy_port
        }
        NewConnectionField::StandaloneSftpSecondaryUpstreamProxyNoProxy => {
            &mut form.standalone_sftp_secondary.upstream_proxy_no_proxy
        }
        NewConnectionField::StandaloneSftpSecondaryUpstreamProxyUsername => {
            &mut form.standalone_sftp_secondary.upstream_proxy_username
        }
        NewConnectionField::StandaloneSftpSecondaryUpstreamProxyPassword => {
            &mut form.standalone_sftp_secondary.upstream_proxy_password
        }
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
        NewConnectionField::JumpGssapiServerIdentity => {
            &mut form
                .jump_server_form
                .as_mut()
                .expect("jump Kerberos server field without jump form")
                .gssapi_server_identity
        }
        NewConnectionField::JumpIdentityAgent => {
            &mut form
                .jump_server_form
                .as_mut()
                .expect("jump identity agent field without jump form")
                .identity_agent
        }
    }
}
