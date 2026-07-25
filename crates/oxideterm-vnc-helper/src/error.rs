// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum VncErrorKind {
    Cancelled,
    Configuration,
    Network,
    Version,
    SecurityNegotiation,
    Tls,
    Certificate,
    Authentication,
    Protocol,
}

#[derive(Debug)]
pub(super) struct VncError {
    kind: VncErrorKind,
    message: String,
}

impl VncError {
    pub(super) fn new(kind: VncErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub(super) fn cancelled() -> Self {
        Self::new(VncErrorKind::Cancelled, "VNC connection canceled.")
    }

    pub(super) fn configuration(message: impl Into<String>) -> Self {
        Self::new(VncErrorKind::Configuration, message)
    }

    pub(super) fn network(message: impl Into<String>) -> Self {
        Self::new(VncErrorKind::Network, message)
    }

    pub(super) fn version(message: impl Into<String>) -> Self {
        Self::new(VncErrorKind::Version, message)
    }

    pub(super) fn security(message: impl Into<String>) -> Self {
        Self::new(VncErrorKind::SecurityNegotiation, message)
    }

    pub(super) fn tls(message: impl Into<String>) -> Self {
        Self::new(VncErrorKind::Tls, message)
    }

    pub(super) fn certificate(message: impl Into<String>) -> Self {
        Self::new(VncErrorKind::Certificate, message)
    }

    pub(super) fn authentication(message: impl Into<String>) -> Self {
        Self::new(VncErrorKind::Authentication, message)
    }

    pub(super) fn protocol(message: impl Into<String>) -> Self {
        Self::new(VncErrorKind::Protocol, message)
    }

    pub(super) fn kind(&self) -> VncErrorKind {
        self.kind
    }

    pub(super) fn category(&self) -> RemoteDesktopErrorCategory {
        match self.kind {
            VncErrorKind::Configuration => RemoteDesktopErrorCategory::Configuration,
            VncErrorKind::Network | VncErrorKind::Tls | VncErrorKind::Cancelled => {
                RemoteDesktopErrorCategory::Network
            }
            VncErrorKind::Authentication => RemoteDesktopErrorCategory::Authentication,
            VncErrorKind::SecurityNegotiation | VncErrorKind::Certificate => {
                RemoteDesktopErrorCategory::LegacySecurity
            }
            VncErrorKind::Version | VncErrorKind::Protocol => RemoteDesktopErrorCategory::Protocol,
        }
    }
}

impl std::fmt::Display for VncError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for VncError {}

pub(super) type VncResult<T> = Result<T, VncError>;
