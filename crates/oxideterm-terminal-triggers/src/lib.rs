// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Versioned trigger rules and bounded scanning for decoded terminal output.

mod compiler;
mod error;
mod model;
mod scanner;
mod store;
mod template;

pub use compiler::{CompiledTriggerSet, compile_active, validate_snapshot};
pub use error::TerminalTriggerError;
pub use model::{
    LocalProcessSpec, SavedConnectionKind, SavedConnectionRef, TERMINAL_TRIGGERS_SCHEMA_VERSION,
    TerminalTrigger, TerminalTriggerAction, TerminalTriggerDispatch, TerminalTriggerMatch,
    TerminalTriggerMatchMode, TerminalTriggerScope, TerminalTriggerTiming,
    TerminalTriggersSnapshot,
};
pub use scanner::{TerminalTriggerStream, TriggerMatched};
pub use store::{
    default_snapshot, load_snapshot, new_trigger_id, now_ms, save_snapshot, terminal_triggers_path,
};
pub use template::{ExpandedLocalProcessSpec, ExpandedTriggerAction, expand_template};
