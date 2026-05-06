// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

#[derive(Debug, thiserror::Error)]
pub enum ForwardingError {
    #[error("forward rule not found: {0}")]
    NotFound(String),
    #[error("forward rule already exists: {0}")]
    AlreadyExists(String),
    #[error("forward rule is active and cannot be edited: {0}")]
    ActiveRuleCannotBeEdited(String),
    #[error("forward type is not implemented in native yet: {0}")]
    UnsupportedForwardType(&'static str),
    #[error("invalid forward rule: {0}")]
    InvalidRule(String),
    #[error("SSH forwarding failed: {0}")]
    Ssh(String),
    #[error("I/O forwarding failed: {0}")]
    Io(#[from] std::io::Error),
}

impl From<oxideterm_ssh::SshTransportError> for ForwardingError {
    fn from(error: oxideterm_ssh::SshTransportError) -> Self {
        Self::Ssh(error.to_string())
    }
}
