// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::fmt;

use zeroize::Zeroizing;

use crate::X11AuthMaterial;

pub struct X11RemoteXauthUpdate {
    pub display_value: String,
    pub auth: X11AuthMaterial,
}

impl X11RemoteXauthUpdate {
    pub fn new(display_value: impl Into<String>, auth: X11AuthMaterial) -> Self {
        Self {
            display_value: display_value.into(),
            auth,
        }
    }

    pub fn command(&self) -> Zeroizing<String> {
        let display = shell_quote(&self.display_value);
        let protocol = shell_quote(self.auth.protocol.ssh_name());
        let cookie = shell_quote(&self.auth.fake_cookie.to_hex());
        // The command string carries the fake X11 cookie. It must be treated as
        // secret-bearing by callers and should be passed directly to SSH exec.
        Zeroizing::new(format!(
            "set -eu; command -v xauth >/dev/null 2>&1; \
authfile=${{XAUTHORITY:-$HOME/.Xauthority}}; \
xauth -f \"$authfile\" remove {display} >/dev/null 2>&1 || true; \
xauth -f \"$authfile\" add {display} {protocol} {cookie}"
        ))
    }
}

impl fmt::Debug for X11RemoteXauthUpdate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("X11RemoteXauthUpdate")
            .field("display_value", &self.display_value)
            .field("auth", &"<redacted>")
            .finish()
    }
}

fn shell_quote(value: &str) -> String {
    let mut output = String::with_capacity(value.len() + 2);
    output.push('\'');
    for ch in value.chars() {
        if ch == '\'' {
            output.push_str("'\\''");
        } else {
            output.push(ch);
        }
    }
    output.push('\'');
    output
}

#[cfg(test)]
mod tests {
    use crate::{X11AuthCookie, X11AuthMaterial};

    use super::*;

    #[test]
    fn remote_xauth_command_quotes_display_and_redacts_debug() {
        let auth = X11AuthMaterial::with_fake_cookie(
            X11AuthCookie::from_hex("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap(),
            X11AuthCookie::from_hex("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").unwrap(),
        );
        let update = X11RemoteXauthUpdate::new("localhost:10.0", auth);

        let command = update.command();
        assert!(command.contains("xauth -f \"$authfile\" add 'localhost:10.0'"));
        assert!(command.contains("'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'"));
        assert!(!format!("{update:?}").contains("aaaaaaaa"));
    }
}
