use std::collections::{HashMap, HashSet};

// Suggestion values can contain credentials embedded in commands, so diagnostics must not format them.
#[derive(Clone)]
pub struct TerminalCommandSuggestion {
    pub kind: TerminalCommandSuggestionKind,
    pub label: String,
    pub insert_text: String,
    pub description: Option<String>,
    pub executable: bool,
    pub replacement: std::ops::Range<usize>,
    pub group_label_key: &'static str,
    pub source_label_key: &'static str,
    pub score: f64,
    pub risk: Option<&'static str>,
    pub inline_safe: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalCommandSuggestionKind {
    History,
    Command,
    Subcommand,
    Option,
    File,
    Directory,
}

#[derive(Clone)]
pub struct TerminalHistoryEntry {
    pub command: String,
    pub source: TerminalHistorySource,
    pub last_used_at: i64,
    pub uses: usize,
    pub sequence: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalHistorySource {
    Runtime,
    LocalHistory,
}

impl TerminalHistorySource {
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Runtime => "terminal.command_bar.source_runtime",
            // Tauri's command-bar history provider preserves these underlying
            // autosuggest sources internally, but renders completion rows with
            // the generic history source badge.
            Self::LocalHistory => "terminal.command_bar.source_history",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum TerminalFigArgType {
    #[serde(alias = "none")]
    #[default]
    None,
    Path,
    File,
    Directory,
    Value,
    Command,
}

#[derive(Clone)]
pub struct TerminalFigOptionSpec {
    pub name: String,
    pub description: Option<String>,
    pub args: TerminalFigArgType,
}

#[derive(Clone)]
pub struct TerminalFigSubcommandSpec {
    pub name: String,
    pub description: Option<String>,
    pub options: Vec<TerminalFigOptionSpec>,
    pub args: TerminalFigArgType,
}

#[derive(Clone)]
pub struct TerminalFigSpec {
    pub name: String,
    pub description: String,
    pub subcommands: Vec<TerminalFigSubcommandSpec>,
    pub options: Vec<TerminalFigOptionSpec>,
    pub args: TerminalFigArgType,
}

pub struct TerminalPathParts {
    pub directory: String,
    pub query: String,
    pub display_prefix: String,
}

#[derive(Clone)]
pub struct TerminalPathEntry {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
}

pub struct TerminalPathCacheEntry {
    pub created_at: std::time::Instant,
    pub entries: Vec<TerminalPathEntry>,
}

#[derive(Default)]
pub struct TerminalPathCompletionCache {
    pub entries: HashMap<String, TerminalPathCacheEntry>,
    pub pending: HashSet<String>,
}
