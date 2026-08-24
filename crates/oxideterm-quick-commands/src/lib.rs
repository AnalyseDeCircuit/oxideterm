// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Quick Command storage, editing, filtering, and risk classification.
//!
//! The GPUI view owns interaction and presentation state; this crate owns the
//! portable domain behavior shared by `.oxide` import/export, the CLI, and UI.

mod editing;
pub mod model;
mod risk;
mod snapshot;
pub mod store;
mod template;
mod v1;

pub use editing::{
    QuickCommandCategoryDraft, QuickCommandDraft, delete_quick_command,
    delete_quick_command_category, ensure_active_quick_command_category,
    match_quick_command_host_pattern, match_quick_command_host_patterns,
    quick_command_available_for_target, quick_command_category_draft_can_save,
    quick_command_draft_can_save, upsert_quick_command, upsert_quick_command_category,
    visible_quick_commands, visible_quick_commands_for_management,
};
pub use model::{
    QUICK_COMMANDS_SCHEMA_VERSION, QuickCommand, QuickCommandAvailability, QuickCommandCategory,
    QuickCommandConfirmationPolicy, QuickCommandIcon, QuickCommandImportResult,
    QuickCommandImportStrategy, QuickCommandParameter, QuickCommandParameterKind,
    QuickCommandTargetProtocol, QuickCommandsSnapshot,
};
pub use risk::{QuickCommandRisk, classify_command_risk};
pub use snapshot::{decode_snapshot_json, encode_snapshot_json};
pub use store::{
    MAX_CATEGORIES, MAX_QUICK_COMMANDS_FILE_BYTES, QuickCommandsCheckpoint, apply_snapshot_json,
    capture_checkpoint, default_quick_command_categories, default_quick_commands,
    export_snapshot_json, is_builtin_category_id, load_snapshot, new_quick_category_id,
    new_quick_command_id, now_ms, quick_commands_path, restore_checkpoint, save_snapshot,
};
pub use template::{
    MAX_QUICK_COMMAND_ARGUMENT_NAME_BYTES, MAX_QUICK_COMMAND_ARGUMENT_VALUE_BYTES,
    MAX_QUICK_COMMAND_ARGUMENTS, MAX_QUICK_COMMAND_EXPANDED_BYTES, PreparedQuickCommand,
    PreparedQuickCommandTarget, QuickCommandContextValues, QuickCommandTargetContext,
    QuickCommandTemplateError, prepare_quick_command, quick_command_can_run_non_interactively,
    quick_command_has_runtime_substitutions, quick_command_target_match_fields,
    validate_quick_command_template,
};
