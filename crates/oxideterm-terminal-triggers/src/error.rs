// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use thiserror::Error;

/// A configuration, persistence, or template-expansion failure.
#[derive(Debug, Error)]
pub enum TerminalTriggerError {
    #[error("terminal trigger field {field} cannot be empty")]
    EmptyField { field: &'static str },
    #[error("terminal trigger field {field} exceeds limit {limit}")]
    FieldTooLong { field: &'static str, limit: usize },
    #[error("terminal trigger collection {field} exceeds limit {limit}")]
    CollectionTooLarge { field: &'static str, limit: usize },
    #[error("duplicate terminal trigger id")]
    DuplicateId,
    #[error("unsupported terminal trigger schema version {0}")]
    UnsupportedSchema(u32),
    #[error("terminal trigger regular expression is invalid")]
    InvalidRegex,
    #[error("terminal trigger patterns cannot match empty text")]
    EmptyRegexMatch,
    #[error("terminal trigger template is invalid")]
    InvalidTemplate,
    #[error("terminal trigger template references unknown capture {0}")]
    UnknownCapture(String),
    #[error("terminal trigger delay exceeds limit")]
    DelayTooLong,
    #[error("terminal trigger cooldown is outside the supported range")]
    InvalidCooldown,
    #[error("terminal trigger file exceeds size limit")]
    FileTooLarge,
    #[error("failed to access terminal triggers: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse terminal triggers: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("expanded terminal trigger action exceeds size limit")]
    ExpandedActionTooLarge,
}
