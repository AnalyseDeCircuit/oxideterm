// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! SSH launch requests shared by the native CLI and GPUI app.
//!
//! This crate intentionally stays small: it owns only the safe, explicit
//! native launch boundary, not a partial OpenSSH parser or session runtime.

use std::fmt;

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// Default port used by temporary SSH launch targets.
pub const DEFAULT_SSH_PORT: u16 = 22;

/// A native application launch request carried by the owner-only handoff file.
#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NativeSshLaunch {
    SavedConnection(SavedConnectionLaunch),
    Temporary(TemporarySshLaunch),
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
    fn parses_user_at_host() {
        let (username, host) = parse_user_host_target("alice@example.com", None).unwrap();
        assert_eq!(username, "alice");
        assert_eq!(host, "example.com");
    }

    #[test]
    fn parses_host_with_default_username() {
        let (username, host) = parse_user_host_target("example.com", Some("alice")).unwrap();
        assert_eq!(username, "alice");
        assert_eq!(host, "example.com");
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
    fn native_launch_wire_accepts_legacy_temporary_and_saved_requests() {
        let temporary: NativeSshLaunch = serde_json::from_value(serde_json::json!({
            "username": "alice",
            "host": "example.com",
            "port": 22
        }))
        .unwrap();
        assert!(matches!(temporary, NativeSshLaunch::Temporary(_)));

        let saved: NativeSshLaunch = serde_json::from_value(serde_json::json!({
            "saved_connection_id": "connection-1"
        }))
        .unwrap();
        assert_eq!(
            saved,
            NativeSshLaunch::SavedConnection(SavedConnectionLaunch {
                saved_connection_id: "connection-1".to_string(),
            })
        );
    }
}
