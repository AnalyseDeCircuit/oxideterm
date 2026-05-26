// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Quick Commands snapshot storage and import semantics.
//!
//! The GPUI view owns interaction state; this crate owns the portable snapshot
//! format used by `.oxide` import/export and the CLI.

pub mod model;
pub mod store;

pub use model::{
    QuickCommand, QuickCommandCategory, QuickCommandIcon, QuickCommandImportResult,
    QuickCommandImportStrategy, QuickCommandsSnapshot,
};
pub use store::{
    apply_snapshot_json, default_quick_command_categories, default_quick_commands,
    export_snapshot_json, quick_commands_path,
};
