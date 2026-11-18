// Compact Rich Input consumes history here; the remaining providers still back settings data.
#![allow(dead_code, unused_imports)]

use super::actions::classify_command_risk;
use super::*;
use oxideterm_ai::infer_ai_cwd;
use oxideterm_sftp::{FileType as RemotePathFileType, ListFilter, SortOrder};
use oxideterm_terminal::TerminalShellParseResult;
use oxideterm_terminal::command_completion::{
    completion_provider_scope, infer_terminal_ssh_identity_from_buffer, terminal_cwd_looks_remote,
    clear_terminal_path_request_pending, get_cached_terminal_path_entries, mark_terminal_path_request_pending,
    normalize_terminal_path_token, put_cached_terminal_path_entries, should_run_terminal_path_provider,
    terminal_path_cache_key, terminal_path_entries_to_suggestions, TerminalFigArgType, TerminalPathEntry,
    put_terminal_history_entry, terminal_history_suggestions, TerminalHistoryEntry, TerminalHistorySource,
};

mod engine;
mod history_provider;
mod path_provider;
mod render;
mod types;

use types::{TerminalCommandContext, TerminalCommandContextType};

// Settings reads the same registry as completion.
pub(in crate::workspace) use oxideterm_terminal::command_completion::{
    built_in_terminal_fig_specs, normalize_terminal_command_specs_json,
    terminal_command_specs_editor_initial_json, terminal_command_specs_example_json,
    terminal_command_specs_path, user_terminal_fig_specs_count,
};
