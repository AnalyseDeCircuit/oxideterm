// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum X11ForwardTrust {
    Trusted,
    Untrusted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum X11AuthFallbackMode {
    RequireRealAuth,
    GenerateSyntheticCookie,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct X11ForwardPolicy {
    pub trust: X11ForwardTrust,
    pub timeout_millis: Option<u64>,
    pub fallback: X11AuthFallbackMode,
}

impl X11ForwardPolicy {
    pub fn trusted() -> Self {
        Self {
            trust: X11ForwardTrust::Trusted,
            timeout_millis: None,
            fallback: X11AuthFallbackMode::RequireRealAuth,
        }
    }

    pub fn untrusted() -> Self {
        Self::default()
    }

    pub fn with_timeout_millis(mut self, timeout_millis: u64) -> Self {
        self.timeout_millis = Some(timeout_millis);
        self
    }

    pub fn without_timeout(mut self) -> Self {
        self.timeout_millis = None;
        self
    }

    pub fn with_fallback(mut self, fallback: X11AuthFallbackMode) -> Self {
        self.fallback = fallback;
        self
    }

    pub fn is_trusted(self) -> bool {
        self.trust == X11ForwardTrust::Trusted
    }
}

impl Default for X11ForwardPolicy {
    fn default() -> Self {
        Self {
            trust: X11ForwardTrust::Untrusted,
            timeout_millis: None,
            fallback: X11AuthFallbackMode::RequireRealAuth,
        }
    }
}
