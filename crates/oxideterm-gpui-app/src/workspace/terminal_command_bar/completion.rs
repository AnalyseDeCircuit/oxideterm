use super::actions::classify_command_risk;
use super::quick_commands::match_quick_command_host_pattern;
use super::*;
use oxideterm_ai::infer_ai_cwd;
use oxideterm_sftp::{FileType as RemotePathFileType, ListFilter, SortOrder};

mod common;
mod engine;
mod fig_provider;
mod fig_specs;
mod history_provider;
mod path_provider;
mod quick_command_provider;
mod render;
mod types;

pub(self) use common::{
    normalize_terminal_command_suggestions, put_terminal_history_entry,
    terminal_command_risk_score_penalty,
};
pub(self) use fig_provider::{active_fig_arg_type, terminal_command_fig_suggestions};
pub(self) use fig_specs::{
    normalize_terminal_path_token, should_run_terminal_path_provider, terminal_command_bar_now_ms,
};
pub(self) use oxideterm_terminal::{TerminalShellParseResult, TerminalShellToken};
pub(self) use oxideterm_terminal::{
    escape_terminal_path_for_shell, is_likely_secret_terminal_command,
    load_local_shell_history_commands, normalize_terminal_autosuggest_command,
    terminal_autosuggest_fuzzy_score, tokenize_terminal_command_line,
};
pub(self) use quick_command_provider::{
    infer_terminal_ssh_identity_from_buffer, terminal_cwd_looks_remote,
};
pub(self) use types::{
    TerminalCommandContext, TerminalCommandContextType, TerminalFigArgType, TerminalFigOptionSpec,
    TerminalFigSpec, TerminalFigSubcommandSpec, TerminalHistoryEntry, TerminalHistorySource,
    TerminalPathCacheEntry, TerminalPathCompletionCache, TerminalPathEntry, TerminalPathParts,
};

// Preserve the settings UI path while keeping the implementation module private.
pub(in crate::workspace) use fig_specs::{
    built_in_terminal_fig_specs, normalize_terminal_command_specs_json,
    terminal_command_specs_editor_initial_json, terminal_command_specs_example_json,
    terminal_command_specs_path, user_terminal_fig_specs_count,
};
