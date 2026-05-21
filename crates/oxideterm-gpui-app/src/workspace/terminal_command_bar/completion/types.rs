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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerminalCommandContextType {
    Terminal,
    LocalTerminal,
}

#[derive(Clone, Debug)]
struct TerminalCommandContext {
    pane_id: Option<PaneId>,
    session_id: Option<TerminalSessionId>,
    tab_id: Option<TabId>,
    terminal_type: TerminalCommandContextType,
    node_id: Option<NodeId>,
    cwd: Option<String>,
    cwd_host: Option<String>,
    target_label: String,
}

impl TerminalCommandContext {
    fn is_local_terminal(&self) -> bool {
        self.terminal_type == TerminalCommandContextType::LocalTerminal
    }

    fn is_remote_terminal(&self) -> bool {
        self.terminal_type == TerminalCommandContextType::Terminal
    }

    fn provider_scope_id(&self) -> String {
        self.node_id
            .as_ref()
            .map(|node_id| node_id.0.clone())
            .or_else(|| self.session_id.map(|session_id| session_id.0.to_string()))
            .or_else(|| self.pane_id.map(|pane_id| pane_id.0.to_string()))
            .or_else(|| self.tab_id.map(|tab_id| tab_id.0.to_string()))
            .unwrap_or_default()
    }

    fn target_fields(&self) -> Vec<String> {
        let mut fields = vec![self.target_label.clone()];
        if let Some(cwd_host) = &self.cwd_host {
            fields.push(cwd_host.clone());
        }
        if let Some(node_id) = &self.node_id {
            fields.push(node_id.0.clone());
        }
        fields.retain(|field| !field.trim().is_empty());
        fields.dedup();
        fields
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

#[cfg(test)]
mod terminal_command_context_tests {
    use super::*;

    fn context() -> TerminalCommandContext {
        TerminalCommandContext {
            pane_id: Some(PaneId(7)),
            session_id: Some(TerminalSessionId(42)),
            tab_id: Some(TabId(9)),
            terminal_type: TerminalCommandContextType::Terminal,
            node_id: Some(NodeId::new("node-prod")),
            cwd: Some("/srv/app".to_string()),
            cwd_host: Some("prod.example.com".to_string()),
            target_label: "deploy@prod.example.com".to_string(),
        }
    }

    #[test]
    fn command_context_target_fields_match_tauri_provider_order() {
        assert_eq!(
            context().target_fields(),
            vec![
                "deploy@prod.example.com".to_string(),
                "prod.example.com".to_string(),
                "node-prod".to_string(),
            ]
        );
    }

    #[test]
    fn command_context_provider_scope_prefers_node_then_session_identity() {
        let mut context = context();
        assert_eq!(context.provider_scope_id(), "node-prod");

        context.node_id = None;
        assert_eq!(context.provider_scope_id(), "42");

        context.session_id = None;
        assert_eq!(context.provider_scope_id(), "7");

        context.pane_id = None;
        assert_eq!(context.provider_scope_id(), "9");
    }
}
