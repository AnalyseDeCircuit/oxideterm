// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Connection launch requests shared by the native CLI and GPUI app.
//!
//! This crate intentionally stays small: it owns only the safe, explicit
//! native launch boundary, not a transport or session runtime.

use std::fmt;

use percent_encoding::percent_decode_str;
use serde::{Deserialize, Serialize};
use url::Host;
use zeroize::Zeroizing;

/// Default port used by temporary SSH launch targets.
pub const DEFAULT_SSH_PORT: u16 = 22;
pub const DEFAULT_TELNET_PORT: u16 = 23;
pub const DEFAULT_MOSH_SSH_PORT: u16 = 22;
pub const DEFAULT_RDP_PORT: u16 = 3389;
pub const DEFAULT_VNC_PORT: u16 = 5900;

/// URI schemes accepted by native startup and operating-system deep links.
pub const SUPPORTED_CONNECTION_URI_SCHEMES: [&str; 5] = ["ssh", "telnet", "mosh", "rdp", "vnc"];

/// A native application launch request carried by an owner-only handoff.
#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NativeConnectionLaunch {
    SavedConnection(SavedConnectionLaunch),
    Ssh(TemporarySshLaunch),
    Telnet(TemporaryTelnetLaunch),
    Mosh(TemporaryMoshLaunch),
    RemoteDesktop(TemporaryRemoteDesktopLaunch),
}

/// Selects a saved SSH profile without copying its connection properties or secrets.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SavedConnectionLaunch {
    pub saved_connection_id: String,
}

#[derive(Eq, PartialEq, Serialize, Deserialize)]
pub struct TemporarySshLaunch {
    pub username: String,
    pub host: String,
    pub port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<Zeroizing<String>>,
}

impl TemporarySshLaunch {
    pub fn title(&self) -> String {
        format!("{}@{}", self.username, self.host)
    }
}

impl fmt::Debug for TemporarySshLaunch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TemporarySshLaunch")
            .field("username", &self.username)
            .field("host", &self.host)
            .field("port", &self.port)
            .field(
                "password",
                &self.password.as_ref().map(|_| "[redacted secret]"),
            )
            .finish()
    }
}

#[derive(Eq, PartialEq, Serialize, Deserialize)]
pub struct TemporaryTelnetLaunch {
    pub host: String,
    pub port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<Zeroizing<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<Zeroizing<String>>,
}

impl TemporaryTelnetLaunch {
    pub fn title(&self) -> String {
        format!("Telnet {}:{}", self.host, self.port)
    }
}

impl fmt::Debug for TemporaryTelnetLaunch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TemporaryTelnetLaunch")
            .field("host", &self.host)
            .field("port", &self.port)
            .field(
                "username",
                &self.username.as_ref().map(|_| "[redacted userinfo]"),
            )
            .field(
                "password",
                &self.password.as_ref().map(|_| "[redacted secret]"),
            )
            .finish()
    }
}

#[derive(Eq, PartialEq, Serialize, Deserialize)]
pub struct TemporaryMoshLaunch {
    pub username: String,
    pub host: String,
    pub ssh_port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<Zeroizing<String>>,
}

impl TemporaryMoshLaunch {
    pub fn title(&self) -> String {
        format!("{}@{}", self.username, self.host)
    }
}

impl fmt::Debug for TemporaryMoshLaunch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TemporaryMoshLaunch")
            .field("username", &self.username)
            .field("host", &self.host)
            .field("ssh_port", &self.ssh_port)
            .field(
                "password",
                &self.password.as_ref().map(|_| "[redacted secret]"),
            )
            .finish()
    }
}

/// Selects the existing native remote-desktop runtime without coupling this
/// process-boundary crate to its GPUI or protocol implementation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteDesktopLaunchProtocol {
    Rdp,
    Vnc,
}

impl RemoteDesktopLaunchProtocol {
    pub const fn scheme(self) -> &'static str {
        match self {
            Self::Rdp => "rdp",
            Self::Vnc => "vnc",
        }
    }

    pub const fn default_port(self) -> u16 {
        match self {
            Self::Rdp => DEFAULT_RDP_PORT,
            Self::Vnc => DEFAULT_VNC_PORT,
        }
    }
}

#[derive(Eq, PartialEq, Serialize, Deserialize)]
pub struct TemporaryRemoteDesktopLaunch {
    pub protocol: RemoteDesktopLaunchProtocol,
    pub host: String,
    pub port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<Zeroizing<String>>,
}

impl fmt::Debug for TemporaryRemoteDesktopLaunch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TemporaryRemoteDesktopLaunch")
            .field("protocol", &self.protocol)
            .field("host", &self.host)
            .field("port", &self.port)
            .field(
                "username",
                &self.username.as_ref().map(|_| "[redacted userinfo]"),
            )
            .field(
                "domain",
                &self.domain.as_ref().map(|_| "[redacted userinfo]"),
            )
            .field(
                "password",
                &self.password.as_ref().map(|_| "[redacted secret]"),
            )
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseConnectionUriError {
    Empty,
    Invalid,
    UnsupportedScheme,
    MissingHost,
    MissingUsername,
    UnsupportedComponents,
    InvalidUserInfo,
}

impl fmt::Display for ParseConnectionUriError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("connection URI is empty"),
            Self::Invalid => formatter.write_str("connection URI is invalid"),
            Self::UnsupportedScheme => formatter.write_str("connection URI scheme is unsupported"),
            Self::MissingHost => formatter.write_str("connection URI is missing a host"),
            Self::MissingUsername => formatter.write_str("connection URI is missing a username"),
            Self::UnsupportedComponents => formatter
                .write_str("connection URI contains unsupported path, query, or fragment data"),
            Self::InvalidUserInfo => {
                formatter.write_str("connection URI user information is invalid")
            }
        }
    }
}

impl std::error::Error for ParseConnectionUriError {}

pub fn parse_connection_uri(
    raw_uri: &str,
    default_username: Option<&str>,
) -> Result<NativeConnectionLaunch, ParseConnectionUriError> {
    let raw_uri = raw_uri.trim();
    if raw_uri.is_empty() {
        return Err(ParseConnectionUriError::Empty);
    }
    if raw_uri
        .chars()
        .any(|character| character.is_whitespace() || character.is_control())
    {
        return Err(ParseConnectionUriError::Invalid);
    }
    let (scheme, remainder) = raw_uri
        .split_once("://")
        .ok_or(ParseConnectionUriError::Invalid)?;
    let authority = remainder.strip_suffix('/').unwrap_or(remainder);
    if authority.is_empty()
        || authority
            .chars()
            .any(|character| matches!(character, '/' | '?' | '#'))
    {
        return Err(ParseConnectionUriError::UnsupportedComponents);
    }
    let (raw_user_info, host_authority) = match authority.rsplit_once('@') {
        Some((user_info, host_authority)) if !user_info.contains('@') => {
            (Some(user_info), host_authority)
        }
        Some(_) => return Err(ParseConnectionUriError::InvalidUserInfo),
        None => (None, authority),
    };
    let (raw_username, raw_password) = raw_user_info
        .map(|user_info| {
            user_info
                .split_once(':')
                .map_or((user_info, None), |(username, password)| {
                    (username, Some(password))
                })
        })
        .unwrap_or(("", None));
    let username = decode_user_info(raw_username)?;
    let password = raw_password.map(decode_secret_user_info).transpose()?;
    let (host, explicit_port) = parse_uri_host_authority(host_authority)?;

    match scheme.to_ascii_lowercase().as_str() {
        "ssh" => {
            let username = connection_username(username, default_username)?;
            Ok(NativeConnectionLaunch::Ssh(TemporarySshLaunch {
                username,
                host,
                port: explicit_port.unwrap_or(DEFAULT_SSH_PORT),
                password,
            }))
        }
        "telnet" => {
            if password.is_some() && username.is_empty() {
                return Err(ParseConnectionUriError::MissingUsername);
            }
            Ok(NativeConnectionLaunch::Telnet(TemporaryTelnetLaunch {
                host,
                port: explicit_port.unwrap_or(DEFAULT_TELNET_PORT),
                username: (!username.is_empty()).then(|| Zeroizing::new(username)),
                password,
            }))
        }
        "mosh" => {
            let username = connection_username(username, default_username)?;
            Ok(NativeConnectionLaunch::Mosh(TemporaryMoshLaunch {
                username,
                host,
                ssh_port: explicit_port.unwrap_or(DEFAULT_MOSH_SSH_PORT),
                password,
            }))
        }
        "rdp" => parse_remote_desktop_launch(
            RemoteDesktopLaunchProtocol::Rdp,
            username,
            password,
            host,
            explicit_port,
        ),
        "vnc" => parse_remote_desktop_launch(
            RemoteDesktopLaunchProtocol::Vnc,
            username,
            password,
            host,
            explicit_port,
        ),
        _ => Err(ParseConnectionUriError::UnsupportedScheme),
    }
}

fn parse_remote_desktop_launch(
    protocol: RemoteDesktopLaunchProtocol,
    username: String,
    password: Option<Zeroizing<String>>,
    host: String,
    explicit_port: Option<u16>,
) -> Result<NativeConnectionLaunch, ParseConnectionUriError> {
    if protocol == RemoteDesktopLaunchProtocol::Rdp && password.is_some() && username.is_empty() {
        return Err(ParseConnectionUriError::MissingUsername);
    }
    let (username, domain) = split_remote_desktop_identity(protocol, username);
    Ok(NativeConnectionLaunch::RemoteDesktop(
        TemporaryRemoteDesktopLaunch {
            protocol,
            host,
            port: explicit_port.unwrap_or_else(|| protocol.default_port()),
            username,
            domain,
            password,
        },
    ))
}

fn split_remote_desktop_identity(
    protocol: RemoteDesktopLaunchProtocol,
    username: String,
) -> (Option<String>, Option<String>) {
    if username.is_empty() {
        return (None, None);
    }
    if protocol == RemoteDesktopLaunchProtocol::Rdp
        && let Some((domain, username)) = username.split_once('\\')
        && !domain.is_empty()
        && !username.is_empty()
    {
        return (Some(username.to_string()), Some(domain.to_string()));
    }
    (Some(username), None)
}

fn parse_uri_host_authority(
    authority: &str,
) -> Result<(String, Option<u16>), ParseConnectionUriError> {
    if authority.is_empty() {
        return Err(ParseConnectionUriError::MissingHost);
    }
    if let Some(bracketed) = authority.strip_prefix('[') {
        let bracket_end = bracketed
            .find(']')
            .ok_or(ParseConnectionUriError::Invalid)?;
        let host = &bracketed[..bracket_end];
        let suffix = &bracketed[bracket_end + 1..];
        let address = host
            .parse::<std::net::Ipv6Addr>()
            .map_err(|_| ParseConnectionUriError::Invalid)?;
        let port = parse_uri_port_suffix(suffix)?;
        return Ok((address.to_string(), port));
    }
    if authority.contains('[') || authority.contains(']') || authority.matches(':').count() > 1 {
        return Err(ParseConnectionUriError::Invalid);
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, Some(parse_uri_port(port)?)),
        None => (authority, None),
    };
    let host = match Host::parse(host).map_err(|_| ParseConnectionUriError::Invalid)? {
        Host::Domain(host) if !host.is_empty() => host,
        Host::Ipv4(host) => host.to_string(),
        Host::Ipv6(_) | Host::Domain(_) => return Err(ParseConnectionUriError::MissingHost),
    };
    Ok((host, port))
}

fn parse_uri_port_suffix(suffix: &str) -> Result<Option<u16>, ParseConnectionUriError> {
    if suffix.is_empty() {
        return Ok(None);
    }
    let port = suffix
        .strip_prefix(':')
        .ok_or(ParseConnectionUriError::Invalid)?;
    parse_uri_port(port).map(Some)
}

fn parse_uri_port(port: &str) -> Result<u16, ParseConnectionUriError> {
    let port = port
        .parse::<u16>()
        .ok()
        .filter(|port| *port > 0)
        .ok_or(ParseConnectionUriError::Invalid)?;
    Ok(port)
}

fn decode_user_info(value: &str) -> Result<String, ParseConnectionUriError> {
    if !has_valid_percent_encoding(value) {
        return Err(ParseConnectionUriError::InvalidUserInfo);
    }
    let decoded = percent_decode_str(value)
        .decode_utf8()
        .map_err(|_| ParseConnectionUriError::InvalidUserInfo)?;
    if decoded.chars().any(char::is_control) {
        return Err(ParseConnectionUriError::InvalidUserInfo);
    }
    Ok(decoded.into_owned())
}

fn decode_secret_user_info(value: &str) -> Result<Zeroizing<String>, ParseConnectionUriError> {
    if !has_valid_percent_encoding(value) {
        return Err(ParseConnectionUriError::InvalidUserInfo);
    }
    let decoded_bytes = Zeroizing::new(percent_decode_str(value).collect::<Vec<_>>());
    let decoded = std::str::from_utf8(&decoded_bytes)
        .map_err(|_| ParseConnectionUriError::InvalidUserInfo)?;
    let decoded = Zeroizing::new(decoded.to_string());
    if decoded.chars().any(char::is_control) {
        return Err(ParseConnectionUriError::InvalidUserInfo);
    }
    Ok(decoded)
}

fn has_valid_percent_encoding(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            index += 1;
            continue;
        }
        if index + 2 >= bytes.len()
            || !bytes[index + 1].is_ascii_hexdigit()
            || !bytes[index + 2].is_ascii_hexdigit()
        {
            return false;
        }
        index += 3;
    }
    true
}

fn connection_username(
    parsed_username: String,
    default_username: Option<&str>,
) -> Result<String, ParseConnectionUriError> {
    let username = if parsed_username.is_empty() {
        default_username.unwrap_or_default().trim().to_string()
    } else {
        parsed_username
    };
    if username.is_empty() {
        return Err(ParseConnectionUriError::MissingUsername);
    }
    Ok(username)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParseSshTargetError {
    Empty,
    MissingHost,
    MissingUsername,
    UnsupportedUri,
}

impl fmt::Display for ParseSshTargetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("SSH target is empty"),
            Self::MissingHost => formatter.write_str("SSH target is missing a host"),
            Self::MissingUsername => formatter.write_str("SSH target is missing a username"),
            Self::UnsupportedUri => formatter.write_str("SSH target must be user@host, not a URI"),
        }
    }
}

impl std::error::Error for ParseSshTargetError {}

pub fn parse_user_host_target(
    target: &str,
    default_username: Option<&str>,
) -> Result<(String, String), ParseSshTargetError> {
    let target = target.trim();
    if target.is_empty() {
        return Err(ParseSshTargetError::Empty);
    }
    if target.contains("://") {
        return Err(ParseSshTargetError::UnsupportedUri);
    }

    let (username, host) = if let Some((username, host)) = target.rsplit_once('@') {
        if username.trim().is_empty() {
            return Err(ParseSshTargetError::MissingUsername);
        }
        (username.trim(), host.trim())
    } else {
        (default_username.unwrap_or("").trim(), target)
    };

    if username.is_empty() {
        return Err(ParseSshTargetError::MissingUsername);
    }
    if host.is_empty() {
        return Err(ParseSshTargetError::MissingHost);
    }

    Ok((username.to_string(), host.to_string()))
}

/// Parses a strict `user@host[:port]` target for quick-connect surfaces.
pub fn parse_explicit_user_host_port_target(target: &str) -> Option<(String, String, u16)> {
    if target.is_empty()
        || target.contains("://")
        || target
            .chars()
            .any(|ch| ch.is_whitespace() || ch.is_control())
    {
        return None;
    }
    let (username, authority) = target.split_once('@')?;
    if username.is_empty() || authority.is_empty() || authority.contains('@') {
        return None;
    }

    let (host, port) = parse_host_port_authority(authority)?;
    Some((username.to_string(), host, port))
}

/// Formats a parsed target while preserving an unambiguous IPv6 authority.
pub fn format_user_host_port_target(username: &str, host: &str, port: u16) -> String {
    let host = if host.contains(':') && !host.starts_with('[') {
        // Brackets keep IPv6 hosts distinct from the explicit SSH port.
        format!("[{host}]")
    } else {
        host.to_string()
    };
    format!("{username}@{host}:{port}")
}

fn parse_host_port_authority(authority: &str) -> Option<(String, u16)> {
    if authority.chars().any(|ch| matches!(ch, '/' | '?' | '#')) {
        return None;
    }

    let (host, port) = if let Some(rest) = authority.strip_prefix('[') {
        let end = rest.find(']')?;
        let host = &rest[..end];
        let suffix = &rest[end + 1..];
        let port = if suffix.is_empty() {
            DEFAULT_SSH_PORT
        } else {
            suffix.strip_prefix(':')?.parse::<u16>().ok()?
        };
        (host, port)
    } else if authority.matches(':').count() > 1 {
        // Unbracketed IPv6 is accepted only with the default port.
        (authority, DEFAULT_SSH_PORT)
    } else if let Some((host, port)) = authority.rsplit_once(':') {
        (host, port.parse::<u16>().ok()?)
    } else {
        (authority, DEFAULT_SSH_PORT)
    };
    if host.is_empty() || port == 0 {
        return None;
    }
    Some((host.to_string(), port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_explicit_and_default_user_targets() {
        // Explicit and inherited usernames must resolve to the same launch target.
        let cases = [("alice@example.com", None), ("example.com", Some("alice"))];

        for (target, default_username) in cases {
            let (username, host) = parse_user_host_target(target, default_username).unwrap();
            assert_eq!(username, "alice");
            assert_eq!(host, "example.com");
        }
    }

    #[test]
    fn rejects_uri_targets() {
        assert_eq!(
            parse_user_host_target("ssh://alice@example.com", None).unwrap_err(),
            ParseSshTargetError::UnsupportedUri
        );
    }

    #[test]
    fn parses_explicit_user_host_and_optional_port() {
        assert_eq!(
            parse_explicit_user_host_port_target("root@example.com"),
            Some(("root".to_string(), "example.com".to_string(), 22))
        );
        assert_eq!(
            parse_explicit_user_host_port_target("root@example.com:2200"),
            Some(("root".to_string(), "example.com".to_string(), 2200))
        );
    }

    #[test]
    fn parses_and_formats_ipv6_targets() {
        let parsed = parse_explicit_user_host_port_target("root@[::1]:2200").unwrap();

        assert_eq!(parsed, ("root".to_string(), "::1".to_string(), 2200));
        assert_eq!(
            format_user_host_port_target(&parsed.0, &parsed.1, parsed.2),
            "root@[::1]:2200"
        );
    }

    #[test]
    fn rejects_unsafe_or_invalid_explicit_targets() {
        for target in [
            "example.com",
            "root@",
            "@example.com",
            "root@example.com:0",
            "root@example.com:invalid",
            "root@example .com",
            "root@example.com/path",
            "ssh://root@example.com",
        ] {
            assert!(parse_explicit_user_host_port_target(target).is_none());
        }
    }

    #[test]
    fn connection_uri_parses_protocol_defaults_credentials_and_ipv6() {
        let ssh = parse_connection_uri("ssh://alice:p%40ss@[2001:db8::10]:2200", None).unwrap();
        assert!(matches!(
            ssh,
            NativeConnectionLaunch::Ssh(TemporarySshLaunch {
                username,
                host,
                port: 2200,
                password: Some(password),
            }) if username == "alice" && host == "2001:db8::10" && password.as_str() == "p@ss"
        ));

        let telnet = parse_connection_uri("telnet://router.example.com", None).unwrap();
        assert!(matches!(
            telnet,
            NativeConnectionLaunch::Telnet(TemporaryTelnetLaunch {
                port: DEFAULT_TELNET_PORT,
                username: None,
                password: None,
                ..
            })
        ));

        let mosh = parse_connection_uri("mosh://server.example.com", Some("local-user")).unwrap();
        assert!(matches!(
            mosh,
            NativeConnectionLaunch::Mosh(TemporaryMoshLaunch {
                username,
                ssh_port: DEFAULT_MOSH_SSH_PORT,
                ..
            }) if username == "local-user"
        ));

        let rdp =
            parse_connection_uri("rdp://CORP%5Calice:p%40ss@desktop.example.com", None).unwrap();
        assert!(matches!(
            rdp,
            NativeConnectionLaunch::RemoteDesktop(TemporaryRemoteDesktopLaunch {
                protocol: RemoteDesktopLaunchProtocol::Rdp,
                host,
                port: DEFAULT_RDP_PORT,
                username: Some(username),
                domain: Some(domain),
                password: Some(password),
            }) if host == "desktop.example.com"
                && username == "alice"
                && domain == "CORP"
                && password.as_str() == "p@ss"
        ));

        let vnc = parse_connection_uri("vnc://:screen-secret@[::1]:5901", None).unwrap();
        assert!(matches!(
            vnc,
            NativeConnectionLaunch::RemoteDesktop(TemporaryRemoteDesktopLaunch {
                protocol: RemoteDesktopLaunchProtocol::Vnc,
                host,
                port: 5901,
                username: None,
                domain: None,
                password: Some(password),
            }) if host == "::1" && password.as_str() == "screen-secret"
        ));
    }

    #[test]
    fn connection_uri_rejects_unhandled_or_ambiguous_components() {
        for uri in [
            "https://example.com",
            "ssh://",
            "ssh://example.com/path",
            "ssh://example.com?command=id",
            "ssh://example.com:0",
            "ssh://user:bad%ZZpassword@example.com",
            "telnet://:password@example.com",
            "rdp://:password@example.com",
        ] {
            assert!(parse_connection_uri(uri, None).is_err(), "accepted {uri}");
        }
    }

    #[test]
    fn native_launch_wire_redacts_credentials_and_round_trips() {
        let temporary: NativeConnectionLaunch = serde_json::from_value(serde_json::json!({
            "kind": "ssh",
            "username": "alice",
            "host": "example.com",
            "port": 22,
            "password": "wire-secret"
        }))
        .unwrap();
        assert!(!format!("{temporary:?}").contains("wire-secret"));
        assert!(matches!(temporary, NativeConnectionLaunch::Ssh(_)));

        let remote_desktop: NativeConnectionLaunch = serde_json::from_value(serde_json::json!({
            "kind": "remote_desktop",
            "protocol": "rdp",
            "host": "desktop.example.com",
            "port": 3389,
            "username": "alice",
            "domain": "CORP",
            "password": "remote-secret"
        }))
        .unwrap();
        assert!(!format!("{remote_desktop:?}").contains("remote-secret"));

        let saved: NativeConnectionLaunch = serde_json::from_value(serde_json::json!({
            "kind": "saved_connection",
            "saved_connection_id": "connection-1"
        }))
        .unwrap();
        assert_eq!(
            saved,
            NativeConnectionLaunch::SavedConnection(SavedConnectionLaunch {
                saved_connection_id: "connection-1".to_string(),
            })
        );
    }
}
