// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::fmt;

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

#[derive(Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RemoteDesktopSecret(Zeroizing<String>);

impl RemoteDesktopSecret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(Zeroizing::new(value.into()))
    }

    pub fn expose_secret(&self) -> &str {
        self.0.as_str()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn duplicate_for_reauthentication(&self) -> Self {
        // Both copies remain zeroizing and the session retains the original
        // only so a reconnect can cross a fresh certificate boundary.
        Self(Zeroizing::new(self.0.to_string()))
    }
}

impl From<String> for RemoteDesktopSecret {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for RemoteDesktopSecret {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<Zeroizing<String>> for RemoteDesktopSecret {
    fn from(value: Zeroizing<String>) -> Self {
        Self(value)
    }
}

impl fmt::Debug for RemoteDesktopSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[redacted remote desktop secret]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_secret_value() {
        let secret = RemoteDesktopSecret::from("rdp-password");

        let debug = format!("{secret:?}");

        assert!(debug.contains("redacted"));
        assert!(!debug.contains("rdp-password"));
    }

    #[test]
    fn zeroizing_secret_handoff_reuses_the_original_buffer() {
        let source = Zeroizing::new("rdp-password".to_string());
        let source_buffer = source.as_ptr();
        let secret = RemoteDesktopSecret::from(source);

        assert_eq!(source_buffer, secret.0.as_ptr());
    }
}
