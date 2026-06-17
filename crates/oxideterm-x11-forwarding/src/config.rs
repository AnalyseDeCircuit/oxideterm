// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::fmt;

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::{X11AuthMaterial, X11AuthProtocol, X11Display, X11ForwardPolicy};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct X11ForwardConfig {
    pub local_display: X11Display,
    pub remote_display: u16,
    pub single_connection: bool,
    pub policy: X11ForwardPolicy,
}

impl X11ForwardConfig {
    pub fn new(local_display: X11Display) -> Self {
        Self {
            local_display,
            remote_display: 10,
            single_connection: false,
            policy: X11ForwardPolicy::default(),
        }
    }

    pub fn remote_display_value(&self) -> String {
        self.local_display.remote_display_value(self.remote_display)
    }

    pub fn with_policy(mut self, policy: X11ForwardPolicy) -> Self {
        self.policy = policy;
        self
    }

    pub fn ssh_request(&self, auth: &X11AuthMaterial) -> X11SshRequest {
        X11SshRequest {
            single_connection: self.single_connection,
            auth_protocol: auth.protocol,
            auth_cookie_hex: auth.ssh_auth_cookie(),
            screen_number: self.local_display.screen as u32,
        }
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct X11SshRequest {
    pub single_connection: bool,
    pub auth_protocol: X11AuthProtocol,
    pub auth_cookie_hex: String,
    pub screen_number: u32,
}

impl X11SshRequest {
    pub fn auth_protocol_name(&self) -> &'static str {
        self.auth_protocol.ssh_name()
    }
}

impl Drop for X11SshRequest {
    fn drop(&mut self) {
        self.auth_cookie_hex.zeroize();
    }
}

impl fmt::Debug for X11SshRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("X11SshRequest")
            .field("single_connection", &self.single_connection)
            .field("auth_protocol", &self.auth_protocol)
            .field("auth_cookie_hex", &"<redacted>")
            .field("screen_number", &self.screen_number)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use crate::{X11AuthCookie, X11AuthMaterial, X11Display, X11ForwardPolicy, X11ForwardTrust};

    use super::*;

    #[test]
    fn config_builds_russh_request_shape_without_runtime_wiring() {
        let display = X11Display::parse(":0.1").unwrap();
        let mut config = X11ForwardConfig::new(display);
        config.single_connection = true;
        let auth = X11AuthMaterial::with_fake_cookie(
            X11AuthCookie::from_hex("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap(),
            X11AuthCookie::from_hex("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").unwrap(),
        );

        let request = config.ssh_request(&auth);

        assert!(request.single_connection);
        assert_eq!(request.auth_protocol_name(), "MIT-MAGIC-COOKIE-1");
        assert_eq!(request.auth_cookie_hex, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        assert_eq!(request.screen_number, 1);
        assert_eq!(config.remote_display_value(), "localhost:10.1");
        assert!(!format!("{request:?}").contains("aaaaaaaa"));
    }

    #[test]
    fn config_carries_explicit_forwarding_policy() {
        let display = X11Display::parse(":0").unwrap();
        let config = X11ForwardConfig::new(display)
            .with_policy(X11ForwardPolicy::trusted().with_timeout_millis(30_000));

        assert_eq!(config.policy.trust, X11ForwardTrust::Trusted);
        assert_eq!(config.policy.timeout_millis, Some(30_000));
    }
}
