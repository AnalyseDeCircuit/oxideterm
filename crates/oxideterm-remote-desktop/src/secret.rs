// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{fmt, sync::Arc};

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

#[derive(Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RemoteDesktopSecret(Arc<Zeroizing<String>>);

impl RemoteDesktopSecret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(Arc::new(Zeroizing::new(value.into())))
    }

    pub fn expose_secret(&self) -> &str {
        self.0.as_str()
    }

    /// Shares one zeroizing allocation across reconnect and authentication owners.
    pub fn share(&self) -> Self {
        Self(Arc::clone(&self.0))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
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
        Self(Arc::new(value))
    }
}

impl Clone for RemoteDesktopSecret {
    fn clone(&self) -> Self {
        self.share()
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
    fn shared_secret_reuses_one_zeroizing_allocation() {
        let secret = RemoteDesktopSecret::from("rdp-password");
        let shared = secret.share();

        assert!(Arc::ptr_eq(&secret.0, &shared.0));
    }
}
