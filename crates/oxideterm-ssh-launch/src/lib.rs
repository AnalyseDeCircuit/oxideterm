// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Temporary SSH launch requests shared by the native CLI and GPUI app.
//!
//! This crate intentionally stays small: it owns only the safe, explicit
//! `oxideterm ssh user@host` launch surface, not a partial OpenSSH parser.

use std::fmt;

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
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
}
