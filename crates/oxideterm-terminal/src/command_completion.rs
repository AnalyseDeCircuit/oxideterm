//! Shell completion candidates, Fig registry and provider helpers. The caller owns live terminal and SFTP access.

mod common;
mod context;
mod fig_provider;
mod fig_specs;
mod paths;
mod prompt_identity;
mod types;

pub use common::{
    normalize_terminal_command_suggestions, put_terminal_history_entry,
    terminal_command_risk_score_penalty, terminal_history_suggestions,
};
pub use context::{TerminalCommandContext, TerminalCommandContextType};
pub use fig_provider::{active_fig_arg_type, terminal_command_fig_suggestions};
pub use fig_specs::{
    built_in_terminal_fig_specs, normalize_terminal_command_specs_json,
    terminal_command_specs_editor_initial_json, terminal_command_specs_example_json,
    terminal_command_specs_path, terminal_fig_specs, user_terminal_fig_specs_count,
};
pub use paths::{
    clear_terminal_path_request_pending, get_cached_terminal_path_entries,
    mark_terminal_path_request_pending, normalize_terminal_path_token,
    put_cached_terminal_path_entries, read_local_terminal_path_entries,
    should_run_terminal_path_provider, terminal_path_cache_key,
    terminal_path_entries_to_suggestions,
};
pub use prompt_identity::{infer_terminal_ssh_identity_from_buffer, terminal_cwd_looks_remote};
pub use types::{
    TerminalCommandSuggestion, TerminalCommandSuggestionKind, TerminalFigArgType,
    TerminalFigOptionSpec, TerminalFigSpec, TerminalFigSubcommandSpec, TerminalHistoryEntry,
    TerminalHistorySource, TerminalPathEntry, TerminalPathParts,
};
