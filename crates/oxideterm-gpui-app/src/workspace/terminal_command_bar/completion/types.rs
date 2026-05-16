#[derive(Clone)]
struct TerminalHistoryEntry {
    command: String,
    source: TerminalHistorySource,
    last_used_at: i64,
    uses: usize,
    sequence: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerminalHistorySource {
    Runtime,
    LocalHistory,
    AiLedger,
}

impl TerminalHistorySource {
    fn label_key(self) -> &'static str {
        match self {
            Self::Runtime => "terminal.command_bar.source_runtime",
            // Tauri's command-bar history provider preserves these underlying
            // autosuggest sources internally, but renders completion rows with
            // the generic history source badge.
            Self::LocalHistory | Self::AiLedger => "terminal.command_bar.source_history",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum TerminalFigArgType {
    #[serde(alias = "none")]
    None,
    Path,
    File,
    Directory,
    Value,
    Command,
}

impl Default for TerminalFigArgType {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Clone)]
struct TerminalFigOptionSpec {
    name: String,
    description: Option<String>,
    args: TerminalFigArgType,
}

#[derive(Clone)]
struct TerminalFigSubcommandSpec {
    name: String,
    description: Option<String>,
    options: Vec<TerminalFigOptionSpec>,
    args: TerminalFigArgType,
}

#[derive(Clone)]
pub(in crate::workspace) struct TerminalFigSpec {
    name: String,
    description: String,
    subcommands: Vec<TerminalFigSubcommandSpec>,
    options: Vec<TerminalFigOptionSpec>,
    args: TerminalFigArgType,
}

#[derive(Clone, Debug)]
struct TerminalShellToken {
    value: String,
    start: usize,
    end: usize,
    quote: Option<char>,
}

#[derive(Clone, Debug)]
struct TerminalShellParseResult {
    reliable: bool,
    tokens: Vec<TerminalShellToken>,
    current_token: TerminalShellToken,
    current_token_index: isize,
    command_name: Option<String>,
}

struct TerminalPathParts {
    directory: String,
    query: String,
    display_prefix: String,
}

#[derive(Clone)]
struct TerminalPathEntry {
    name: String,
    path: String,
    is_directory: bool,
}

struct TerminalPathCacheEntry {
    created_at: std::time::Instant,
    entries: Vec<TerminalPathEntry>,
}

#[derive(Default)]
struct TerminalPathCompletionCache {
    entries: HashMap<String, TerminalPathCacheEntry>,
    pending: HashSet<String>,
}
