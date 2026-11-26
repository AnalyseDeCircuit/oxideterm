use super::*;

/// Returns the presentation-only visibility state for primary credential fields.
pub fn connection_secret_field_visible(
    form: &NewConnectionForm,
    field: NewConnectionField,
) -> Option<bool> {
    match field {
        NewConnectionField::Password => Some(form.password_visible),
        NewConnectionField::Passphrase => Some(form.passphrase_visible),
        NewConnectionField::StandaloneSftpSecondaryPassword => {
            Some(form.standalone_sftp_secondary.password_visible)
        }
        NewConnectionField::StandaloneSftpSecondaryPassphrase => {
            Some(form.standalone_sftp_secondary.passphrase_visible)
        }
        _ => None,
    }
}

/// Toggles visibility without changing or copying the underlying secret draft.
pub fn toggle_connection_secret_field_visibility(
    form: &mut NewConnectionForm,
    field: NewConnectionField,
) -> bool {
    match field {
        NewConnectionField::Password => {
            if form.password_visible && form.password_from_store {
                // Hiding a saved value releases the preview; replacement drafts stay editable.
                zeroize::Zeroize::zeroize(&mut form.password);
                form.password_from_store = false;
                form.password_loaded = false;
                if form.focused_field == NewConnectionField::Password {
                    clear_connection_selection(form);
                }
            }
            form.password_visible = !form.password_visible;
            true
        }
        NewConnectionField::Passphrase => {
            form.passphrase_visible = !form.passphrase_visible;
            true
        }
        NewConnectionField::StandaloneSftpSecondaryPassword => {
            form.standalone_sftp_secondary.password_visible =
                !form.standalone_sftp_secondary.password_visible;
            true
        }
        NewConnectionField::StandaloneSftpSecondaryPassphrase => {
            form.standalone_sftp_secondary.passphrase_visible =
                !form.standalone_sftp_secondary.passphrase_visible;
            true
        }
        _ => false,
    }
}

/// Returns the configured endpoint selector while preserving an empty draft as auto-detect.
pub fn identity_agent_selector(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

/// Converts a form endpoint into the persisted optional representation.
pub fn identity_agent_from_form(value: &str) -> Option<String> {
    identity_agent_selector(value).map(str::to_string)
}

/// Refreshes the main connection's status only after its endpoint draft changes.
#[cfg(not(target_family = "wasm"))]
pub fn refresh_identity_agent_availability(form: &mut NewConnectionForm) {
    form.agent_available =
        oxideterm_ssh::ssh_agent_available(identity_agent_selector(&form.identity_agent));
}

#[cfg(not(target_family = "wasm"))]
fn refresh_focused_identity_agent_availability(form: &mut NewConnectionForm) {
    if form.focused_field == NewConnectionField::IdentityAgent {
        refresh_identity_agent_availability(form);
    } else if form.focused_field == NewConnectionField::StandaloneSftpSecondaryIdentityAgent {
        form.standalone_sftp_secondary.agent_available = oxideterm_ssh::ssh_agent_available(
            identity_agent_selector(&form.standalone_sftp_secondary.identity_agent),
        );
    }
}

pub fn apply_transport_default_port(
    form: &mut NewConnectionForm,
    previous_transport: NewConnectionTransport,
    next_transport: NewConnectionTransport,
) {
    if let Some(replacement) =
        transport_port_replacement(&form.port, previous_transport, next_transport)
    {
        form.port = replacement.to_string();
    }
}

pub fn apply_transport_default_username(
    form: &mut NewConnectionForm,
    previous_transport: NewConnectionTransport,
    next_transport: NewConnectionTransport,
) {
    match transport_username_transition(&form.username, previous_transport, next_transport) {
        Some(TransportUsernameTransition::Set(username)) => form.username = username.to_string(),
        Some(TransportUsernameTransition::Clear) => form.username.clear(),
        None => {}
    }
}

pub fn password_draft_mut(form: &mut NewConnectionForm) -> &mut String {
    // Typing supersedes any pending read, even if the replacement is later cleared.
    form.password_loaded = true;
    form.password_load_id = None;
    form.password_load_error = None;
    form.password_from_store = false;
    &mut form.password
}

pub fn current_connection_field_mut(form: &mut NewConnectionForm) -> &mut String {
    match form.focused_field {
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
    }
}

pub fn current_connection_field(form: &NewConnectionForm) -> &str {
    match form.focused_field {
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
        NewConnectionField::JumpGssapiServerIdentity => {
            &form
                .jump_server_form
                .as_ref()
                .expect("jump Kerberos server field without jump form")
                .gssapi_server_identity
        }
        NewConnectionField::JumpIdentityAgent => {
            &form
                .jump_server_form
                .as_ref()
                .expect("jump identity agent field without jump form")
                .identity_agent
        }
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
    }
}

pub fn select_current_connection_field(form: &mut NewConnectionForm) {
    if current_connection_field(form).is_empty() {
        form.selected_field = None;
    } else {
        form.selected_field = Some(form.focused_field);
    }
}

pub fn clear_connection_selection(form: &mut NewConnectionForm) {
    form.selected_field = None;
}

pub fn connection_field_is_selected(form: &NewConnectionForm, field: NewConnectionField) -> bool {
    form.selected_field == Some(field)
}

pub fn insert_text_into_current_connection_field(form: &mut NewConnectionForm, text: &str) {
    let focused_field = form.focused_field;
    let replacing_selection = form.selected_field == Some(form.focused_field);
    if replacing_selection {
        current_connection_field_mut(form).clear();
    }
    current_connection_field_mut(form).push_str(text);
    form.selected_field = None;
    refresh_connection_timeout_seconds(form, focused_field);
    #[cfg(not(target_family = "wasm"))]
    refresh_focused_identity_agent_availability(form);
}

pub fn backspace_current_connection_field(form: &mut NewConnectionForm) -> bool {
    let focused_field = form.focused_field;
    let selection_was_visible = form.selected_field.is_some();
    if form.selected_field == Some(form.focused_field) {
        // Clearing a selected field also clears visible selection state. Track
        // text separately so empty selected fields still report a UI change.
        let field = current_connection_field_mut(form);
        let text_changed = !field.is_empty();
        field.clear();
        form.selected_field = None;
        if text_changed {
            refresh_connection_timeout_seconds(form, focused_field);
            #[cfg(not(target_family = "wasm"))]
            refresh_focused_identity_agent_availability(form);
        }
        text_changed || selection_was_visible
    } else {
        let text_changed = current_connection_field_mut(form).pop().is_some();
        form.selected_field = None;
        if text_changed {
            refresh_connection_timeout_seconds(form, focused_field);
            #[cfg(not(target_family = "wasm"))]
            refresh_focused_identity_agent_availability(form);
        }
        text_changed || selection_was_visible
    }
}

pub fn clear_current_connection_field(form: &mut NewConnectionForm) {
    let focused_field = form.focused_field;
    current_connection_field_mut(form).clear();
    form.selected_field = None;
    refresh_connection_timeout_seconds(form, focused_field);
    #[cfg(not(target_family = "wasm"))]
    refresh_focused_identity_agent_availability(form);
}

pub fn refresh_connection_timeout_seconds(form: &mut NewConnectionForm, field: NewConnectionField) {
    // Invalid drafts map to zero so keyboard submission cannot reuse a stale valid value.
    match field {
        NewConnectionField::ConnectTimeoutSeconds => {
            form.connect_timeout_seconds = form
                .connect_timeout_seconds_text
                .trim()
                .parse::<u64>()
                .ok()
                .filter(|seconds| *seconds > 0)
                .unwrap_or(0);
        }
        NewConnectionField::KeepAliveIntervalSeconds => {
            form.keep_alive_interval_seconds = form
                .keep_alive_interval_seconds_text
                .trim()
                .parse::<u32>()
                .ok()
                .filter(|seconds| *seconds > 0)
                .unwrap_or(0);
        }
        NewConnectionField::KeepAliveCountMax => {
            form.keep_alive_count_max = form
                .keep_alive_count_max_text
                .trim()
                .parse::<u32>()
                .ok()
                .filter(|count| *count > 0)
                .unwrap_or(0);
        }
        NewConnectionField::StandaloneSftpSecondaryConnectTimeoutSeconds => {
            form.standalone_sftp_secondary.connect_timeout_seconds = form
                .standalone_sftp_secondary
                .connect_timeout_seconds_text
                .trim()
                .parse::<u64>()
                .ok()
                .filter(|seconds| *seconds > 0)
                .unwrap_or(0);
        }
        _ => {}
    }
}

pub fn connection_keep_alive_interval_valid(form: &NewConnectionForm) -> bool {
    form.keep_alive_interval_seconds_text
        .trim()
        .parse::<u32>()
        .is_ok_and(|seconds| seconds > 0)
}

pub fn connection_keep_alive_count_max_valid(form: &NewConnectionForm) -> bool {
    form.keep_alive_count_max_text
        .trim()
        .parse::<u32>()
        .is_ok_and(|count| count > 0)
}

pub fn connection_timeout_drafts_valid(form: &NewConnectionForm) -> bool {
    let primary_valid = form
        .connect_timeout_seconds_text
        .trim()
        .parse::<u64>()
        .is_ok_and(|seconds| seconds > 0);
    let secondary_valid = form
        .standalone_sftp_secondary
        .connect_timeout_seconds_text
        .trim()
        .parse::<u64>()
        .is_ok_and(|seconds| seconds > 0);
    primary_valid
        && (form.standalone_sftp_transfer_mode != StandaloneSftpTransferMode::RemoteRemote
            || secondary_valid)
}

pub fn text_from_keystroke(keystroke: &gpui::Keystroke) -> Option<&str> {
    if keystroke.modifiers.platform || keystroke.modifiers.control {
        return None;
    }
    let text = keystroke.key_char.as_deref()?;
    if text.is_empty() || text.chars().any(char::is_control) {
        return None;
    }
    Some(text)
}

pub fn restore_saved_password_placeholder_if_empty(
    form: &mut NewConnectionForm,
    editing_saved_connection: bool,
) {
    if editing_saved_connection
        && form.focused_field == NewConnectionField::Password
        && form.saved_password_keychain_id.is_some()
        && form.password.is_empty()
    {
        form.password_loaded = false;
        form.password_visible = false;
    }
}

pub fn connection_group_is_ungrouped(group: &str, i18n: &oxideterm_i18n::I18n) -> bool {
    let group = group.trim();
    group.is_empty()
        || group == "Ungrouped"
        || group == "未分组"
        || group == i18n.t("ssh.form.ungrouped")
        || group == i18n.t("sessionManager.edit_properties.ungrouped")
}
