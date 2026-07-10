// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Connection transport identity and default-field transition rules.

pub const SSH_DEFAULT_PORT_TEXT: &str = "22";
pub const TELNET_DEFAULT_PORT_TEXT: &str = "23";
pub const RAW_TCP_DEFAULT_PORT_TEXT: &str = "";
pub const RAW_UDP_DEFAULT_PORT_TEXT: &str = "";
pub const RDP_DEFAULT_PORT_TEXT: &str = "3389";
pub const VNC_DEFAULT_PORT_TEXT: &str = "5900";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionTransport {
    Ssh,
    Telnet,
    RawTcp,
    RawUdp,
    Serial,
    Rdp,
    Vnc,
    WslGraphics,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportUsernameTransition {
    Set(&'static str),
    Clear,
}

pub fn transport_default_port(transport: ConnectionTransport) -> Option<&'static str> {
    match transport {
        ConnectionTransport::Ssh => Some(SSH_DEFAULT_PORT_TEXT),
        ConnectionTransport::Telnet => Some(TELNET_DEFAULT_PORT_TEXT),
        ConnectionTransport::RawTcp => Some(RAW_TCP_DEFAULT_PORT_TEXT),
        ConnectionTransport::RawUdp => Some(RAW_UDP_DEFAULT_PORT_TEXT),
        ConnectionTransport::Rdp => Some(RDP_DEFAULT_PORT_TEXT),
        ConnectionTransport::Vnc => Some(VNC_DEFAULT_PORT_TEXT),
        ConnectionTransport::Serial | ConnectionTransport::WslGraphics => None,
    }
}

pub fn transport_port_replacement(
    current_port: &str,
    previous_transport: ConnectionTransport,
    next_transport: ConnectionTransport,
) -> Option<&'static str> {
    let current_port = current_port.trim();
    let next_default_port = transport_default_port(next_transport)?;
    let current_matches_previous_default = transport_default_port(previous_transport).map_or_else(
        || is_known_transport_default_port(current_port),
        |previous_default| current_port == previous_default,
    );

    (current_port.is_empty() || current_matches_previous_default).then_some(next_default_port)
}

pub fn transport_username_transition(
    current_username: &str,
    previous_transport: ConnectionTransport,
    next_transport: ConnectionTransport,
) -> Option<TransportUsernameTransition> {
    let username = current_username.trim();
    match next_transport {
        ConnectionTransport::Rdp
            if username.is_empty()
                || (previous_transport == ConnectionTransport::Ssh && username == "root") =>
        {
            Some(TransportUsernameTransition::Set("Administrator"))
        }
        ConnectionTransport::Vnc
            if matches!(
                previous_transport,
                ConnectionTransport::Ssh | ConnectionTransport::Rdp
            ) && matches!(username, "root" | "Administrator") =>
        {
            Some(TransportUsernameTransition::Clear)
        }
        ConnectionTransport::RawTcp | ConnectionTransport::RawUdp
            if matches!(
                previous_transport,
                ConnectionTransport::Ssh | ConnectionTransport::Rdp
            ) && matches!(username, "root" | "Administrator") =>
        {
            Some(TransportUsernameTransition::Clear)
        }
        ConnectionTransport::Ssh
            if matches!(
                previous_transport,
                ConnectionTransport::Rdp
                    | ConnectionTransport::RawTcp
                    | ConnectionTransport::RawUdp
            ) && (username == "Administrator" || username.is_empty()) =>
        {
            Some(TransportUsernameTransition::Set("root"))
        }
        ConnectionTransport::Ssh
            if previous_transport == ConnectionTransport::Vnc && username.is_empty() =>
        {
            Some(TransportUsernameTransition::Set("root"))
        }
        _ => None,
    }
}

fn is_known_transport_default_port(port: &str) -> bool {
    [
        SSH_DEFAULT_PORT_TEXT,
        TELNET_DEFAULT_PORT_TEXT,
        RDP_DEFAULT_PORT_TEXT,
        VNC_DEFAULT_PORT_TEXT,
    ]
    .contains(&port)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_transition_replaces_only_empty_or_known_defaults() {
        assert_eq!(
            transport_port_replacement("22", ConnectionTransport::Ssh, ConnectionTransport::Rdp),
            Some("3389")
        );
        assert_eq!(
            transport_port_replacement("", ConnectionTransport::Serial, ConnectionTransport::Vnc),
            Some("5900")
        );
        assert_eq!(
            transport_port_replacement("2200", ConnectionTransport::Ssh, ConnectionTransport::Rdp),
            None
        );
    }

    #[test]
    fn username_transition_preserves_custom_values() {
        assert_eq!(
            transport_username_transition(
                "root",
                ConnectionTransport::Ssh,
                ConnectionTransport::Rdp
            ),
            Some(TransportUsernameTransition::Set("Administrator"))
        );
        assert_eq!(
            transport_username_transition(
                "custom",
                ConnectionTransport::Ssh,
                ConnectionTransport::Rdp
            ),
            None
        );
        assert_eq!(
            transport_username_transition(
                "Administrator",
                ConnectionTransport::Rdp,
                ConnectionTransport::Vnc
            ),
            Some(TransportUsernameTransition::Clear)
        );
    }
}
