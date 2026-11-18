use super::*;

pub fn form_from_remote_desktop_profile(
    profile: &RemoteDesktopProfile,
    ungrouped_label: String,
) -> NewConnectionForm {
    // Editing carries only the keychain reference; the credential value is never loaded.
    let mut form = NewConnectionForm::default();
    form.transport = match profile.protocol {
        oxideterm_remote_desktop::RemoteDesktopProtocol::Rdp => NewConnectionTransport::Rdp,
        oxideterm_remote_desktop::RemoteDesktopProtocol::Vnc => NewConnectionTransport::Vnc,
    };
    form.name = profile.name.clone();
    form.host = profile.host.clone();
    form.port = profile.port.to_string();
    form.username = profile.username.clone().unwrap_or_default();
    form.remote_desktop_session_options = profile.session_options;
    form.remote_desktop_profile_id = Some(profile.id.clone());
    form.remote_desktop_ssh_gateway_connection_id = profile.ssh_gateway_connection_id.clone();
    apply_saved_upstream_proxy_to_form(&mut form, &profile.upstream_proxy);
    form.saved_password_keychain_id = profile.credential_ref.clone();
    form.save_password = profile.credential_ref.is_some();
    form.group = profile.group.clone().unwrap_or(ungrouped_label);
    form.notes = profile.notes.clone().unwrap_or_default();
    form.icon = profile.icon.clone().unwrap_or_default();
    form.color = profile.color.clone().unwrap_or_default();
    form.icon_background_color = profile.icon_background_color.clone().unwrap_or_default();
    form.focused_field = NewConnectionField::Name;
    form
}

pub fn form_from_mosh_profile(profile: &MoshProfile, ungrouped_label: String) -> NewConnectionForm {
    // Editing retains only protected-store references and never loads the secret value.
    let mut form = NewConnectionForm::default();
    form.transport = NewConnectionTransport::Mosh;
    form.mosh_profile_id = Some(profile.id.clone());
    form.name = profile.name.clone();
    form.host = profile.host.clone();
    form.port = profile.ssh_port.to_string();
    form.username = profile.username.clone();
    form.auth_tab = ssh_auth_tab_from_saved_auth(&profile.auth);
    form.saved_password_keychain_id = match profile.auth.conventional_fallback() {
        SavedAuth::Password { keychain_id, .. } => keychain_id.clone(),
        _ => None,
    };
    // Password-profile editors default to persisting a replacement credential.
    form.save_password = matches!(
        profile.auth.conventional_fallback(),
        SavedAuth::Password { .. }
    );
    form.password_loaded = true;
    form.key_path = profile.auth.key_path().unwrap_or_default().to_string();
    form.managed_key_id = profile
        .auth
        .managed_key_id()
        .unwrap_or_default()
        .to_string();
    form.cert_path = profile.auth.cert_path().unwrap_or_default().to_string();
    form.gssapi_enabled = profile.auth.gssapi_options().is_some();
    form.gssapi_server_identity = profile
        .auth
        .gssapi_options()
        .and_then(|(identity, _)| identity.map(ToOwned::to_owned))
        .unwrap_or_default();
    form.gssapi_delegate_credentials = profile
        .auth
        .gssapi_options()
        .is_some_and(|(_, delegate)| delegate);
    form.group = profile.group.clone().unwrap_or(ungrouped_label);
    form.notes = profile.notes.clone().unwrap_or_default();
    form.icon = profile.icon.clone().unwrap_or_default();
    form.color = profile.color.clone().unwrap_or_default();
    form.icon_background_color = profile.icon_background_color.clone().unwrap_or_default();
    form.identity_agent = profile.identity_agent.clone().unwrap_or_default();
    form.agent_available =
        oxideterm_ssh::ssh_agent_available(identity_agent_selector(&form.identity_agent));
    form.legacy_ssh_compatibility = profile.legacy_ssh_compatibility;
    form.ssh_algorithms = profile.ssh_algorithms.clone();
    form.proxy_hops = profile
        .proxy_chain
        .iter()
        .enumerate()
        .map(|(index, hop)| NewConnectionProxyHop::from_saved(index, hop))
        .collect();
    form.proxy_chain_expanded = !form.proxy_hops.is_empty();
    form.mosh_server_executable = profile.server_executable.clone();
    form.mosh_udp_host = profile.udp_host_override.clone().unwrap_or_default();
    form.mosh_udp_port = match profile.udp_port {
        MoshUdpPortSelection::Automatic => String::new(),
        MoshUdpPortSelection::Fixed { port } => port.to_string(),
        MoshUdpPortSelection::Range { start, end } => format!("{start}:{end}"),
    };
    form.mosh_locale = profile.locale.clone().unwrap_or_default();
    form.mosh_ip_family = profile.ip_family;
    form.mosh_prediction = profile.prediction;
    form.terminal = profile.terminal.clone();
    form.focused_field = NewConnectionField::Name;
    form
}

pub fn terminal_serial_parity_from_profile(
    parity: &oxideterm_connections::SerialParity,
) -> oxideterm_terminal::SerialParity {
    match parity {
        oxideterm_connections::SerialParity::None => oxideterm_terminal::SerialParity::None,
        oxideterm_connections::SerialParity::Odd => oxideterm_terminal::SerialParity::Odd,
        oxideterm_connections::SerialParity::Even => oxideterm_terminal::SerialParity::Even,
    }
}

pub fn terminal_serial_flow_from_profile(
    flow: &oxideterm_connections::SerialFlowControl,
) -> oxideterm_terminal::SerialFlowControl {
    match flow {
        oxideterm_connections::SerialFlowControl::None => {
            oxideterm_terminal::SerialFlowControl::None
        }
        oxideterm_connections::SerialFlowControl::Software => {
            oxideterm_terminal::SerialFlowControl::Software
        }
        oxideterm_connections::SerialFlowControl::Hardware => {
            oxideterm_terminal::SerialFlowControl::Hardware
        }
    }
}

pub fn terminal_serial_runtime_options_from_profile(
    profile: &SerialProfile,
) -> oxideterm_terminal::SerialRuntimeOptions {
    oxideterm_terminal::SerialRuntimeOptions {
        line_ending: terminal_serial_line_ending_from_profile(profile.input_line_ending),
        output_line_ending: terminal_serial_line_ending_from_profile(profile.output_line_ending),
        ..Default::default()
    }
}

pub fn terminal_serial_line_ending_from_profile(
    line_ending: oxideterm_connections::SerialLineEnding,
) -> oxideterm_terminal::SerialLineEnding {
    match line_ending {
        oxideterm_connections::SerialLineEnding::Lf => oxideterm_terminal::SerialLineEnding::Lf,
        oxideterm_connections::SerialLineEnding::CrLf => oxideterm_terminal::SerialLineEnding::CrLf,
        oxideterm_connections::SerialLineEnding::Cr => oxideterm_terminal::SerialLineEnding::Cr,
        oxideterm_connections::SerialLineEnding::None => oxideterm_terminal::SerialLineEnding::None,
    }
}

fn terminal_serial_send_mode_from_profile(
    mode: oxideterm_connections::SerialSendMode,
) -> oxideterm_terminal::SerialSendMode {
    match mode {
        oxideterm_connections::SerialSendMode::Text => oxideterm_terminal::SerialSendMode::Text,
        oxideterm_connections::SerialSendMode::Hex => oxideterm_terminal::SerialSendMode::Hex,
    }
}

fn terminal_serial_display_mode_from_profile(
    mode: oxideterm_connections::SerialDisplayMode,
) -> oxideterm_terminal::SerialDisplayMode {
    match mode {
        oxideterm_connections::SerialDisplayMode::Text => {
            oxideterm_terminal::SerialDisplayMode::Text
        }
        oxideterm_connections::SerialDisplayMode::Hex => oxideterm_terminal::SerialDisplayMode::Hex,
        oxideterm_connections::SerialDisplayMode::Mixed => {
            oxideterm_terminal::SerialDisplayMode::Mixed
        }
    }
}

pub fn terminal_serial_runtime_options_from_ble_profile(
    profile: &oxideterm_connections::BleProfile,
) -> oxideterm_terminal::SerialRuntimeOptions {
    oxideterm_terminal::SerialRuntimeOptions {
        line_ending: terminal_serial_line_ending_from_profile(profile.input_line_ending),
        output_line_ending: terminal_serial_line_ending_from_profile(profile.output_line_ending),
        send_mode: terminal_serial_send_mode_from_profile(profile.send_mode),
        display_mode: terminal_serial_display_mode_from_profile(profile.display_mode),
        local_echo: profile.local_echo,
    }
}

pub fn terminal_serial_runtime_options_from_ble_form(
    form: &NewConnectionForm,
) -> oxideterm_terminal::SerialRuntimeOptions {
    oxideterm_terminal::SerialRuntimeOptions {
        line_ending: terminal_serial_line_ending_from_profile(form.ble.input_line_ending),
        output_line_ending: terminal_serial_line_ending_from_profile(form.ble.output_line_ending),
        send_mode: terminal_serial_send_mode_from_profile(form.ble.send_mode),
        display_mode: terminal_serial_display_mode_from_profile(form.ble.display_mode),
        local_echo: form.ble.local_echo,
    }
}

pub fn form_from_serial_profile(
    profile: &SerialProfile,
    ungrouped_label: String,
) -> NewConnectionForm {
    // The edit form owns only persisted settings; live serial sessions keep their current port.
    let mut form = NewConnectionForm::default();
    form.transport = NewConnectionTransport::Serial;
    form.serial_profile_id = Some(profile.id.clone());
    form.serial_profile_name = profile.name.clone();
    form.group = profile.group.clone().unwrap_or(ungrouped_label);
    form.notes = profile.notes.clone().unwrap_or_default();
    form.icon = profile.icon.clone().unwrap_or_default();
    form.color = profile.color.clone().unwrap_or_default();
    form.icon_background_color = profile.icon_background_color.clone().unwrap_or_default();
    form.serial_port_path = profile.port_path.clone();
    form.serial_baud_rate = profile.baud_rate.to_string();
    form.serial_data_bits = profile.data_bits;
    form.serial_stop_bits = profile.stop_bits;
    form.serial_parity = terminal_serial_parity_from_profile(&profile.parity);
    form.serial_flow_control = terminal_serial_flow_from_profile(&profile.flow_control);
    form.terminal = profile.terminal.clone();
    form.focused_field = NewConnectionField::SerialProfileName;
    form
}

pub fn apply_saved_upstream_proxy_to_form(
    form: &mut NewConnectionForm,
    policy: &oxideterm_connections::SavedUpstreamProxyPolicy,
) {
    match policy {
        oxideterm_connections::SavedUpstreamProxyPolicy::Direct => {
            form.upstream_proxy_policy = NewConnectionUpstreamProxyPolicy::Direct
        }
        oxideterm_connections::SavedUpstreamProxyPolicy::UseGlobal => {
            form.upstream_proxy_policy = NewConnectionUpstreamProxyPolicy::UseGlobal
        }
        oxideterm_connections::SavedUpstreamProxyPolicy::Custom { proxy } => {
            form.upstream_proxy_policy = NewConnectionUpstreamProxyPolicy::Custom;
            form.upstream_proxy_protocol = proxy.protocol;
            form.upstream_proxy_host = proxy.host.clone();
            form.upstream_proxy_port = proxy.port.to_string();
            form.upstream_proxy_remote_dns = proxy.remote_dns;
            form.upstream_proxy_no_proxy = proxy.no_proxy.clone();
            match &proxy.auth {
                oxideterm_connections::SavedUpstreamProxyAuth::None => {
                    form.upstream_proxy_auth = NewConnectionUpstreamProxyAuth::None
                }
                oxideterm_connections::SavedUpstreamProxyAuth::Password {
                    username,
                    keychain_id,
                    ..
                } => {
                    form.upstream_proxy_auth = NewConnectionUpstreamProxyAuth::Password;
                    form.upstream_proxy_username = username.clone();
                    form.upstream_proxy_password_keychain_id = keychain_id.clone();
                }
            }
        }
    }
}

pub fn form_from_telnet_profile(
    profile: &TelnetProfile,
    ungrouped_label: String,
) -> NewConnectionForm {
    // The edit form owns only persisted settings; live Telnet sessions keep their current socket.
    let mut form = NewConnectionForm::default();
    form.transport = NewConnectionTransport::Telnet;
    form.telnet_profile_id = Some(profile.id.clone());
    form.telnet_input_line_ending = profile.input_line_ending;
    form.telnet_output_line_ending = profile.output_line_ending;
    apply_saved_upstream_proxy_to_form(&mut form, &profile.upstream_proxy);
    form.telnet_profile_name = profile.name.clone();
    form.group = profile.group.clone().unwrap_or(ungrouped_label);
    form.notes = profile.notes.clone().unwrap_or_default();
    form.icon = profile.icon.clone().unwrap_or_default();
    form.color = profile.color.clone().unwrap_or_default();
    form.icon_background_color = profile.icon_background_color.clone().unwrap_or_default();
    form.host = profile.host.clone();
    form.port = profile.port.to_string();
    form.terminal = profile.terminal.clone();
    form.focused_field = NewConnectionField::TelnetProfileName;
    form
}

pub fn terminal_telnet_line_endings_from_profile(
    profile: &TelnetProfile,
) -> oxideterm_terminal::TelnetLineEndings {
    oxideterm_terminal::TelnetLineEndings {
        input: terminal_serial_line_ending_from_profile(profile.input_line_ending),
        output: terminal_serial_line_ending_from_profile(profile.output_line_ending),
    }
}
