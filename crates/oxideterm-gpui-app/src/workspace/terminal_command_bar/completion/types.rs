use super::*;

#[derive(Clone)]
pub(super) struct TerminalHistoryEntry {
    pub(super) command: String,
    pub(super) source: TerminalHistorySource,
    pub(super) last_used_at: i64,
    pub(super) uses: usize,
    pub(super) sequence: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TerminalHistorySource {
    Runtime,
    LocalHistory,
    AiLedger,
}

impl TerminalHistorySource {
    pub(super) fn label_key(self) -> &'static str {
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
pub(super) enum TerminalCommandContextType {
    Terminal,
    LocalTerminal,
}

#[derive(Clone, Debug)]
pub(super) struct TerminalCommandContext {
    pub(super) pane_id: Option<PaneId>,
    pub(super) session_id: Option<TerminalSessionId>,
    pub(super) tab_id: Option<TabId>,
    pub(super) terminal_type: TerminalCommandContextType,
    pub(super) node_id: Option<NodeId>,
    pub(super) cwd: Option<String>,
    pub(super) cwd_host: Option<String>,
    pub(super) target_label: String,
}

impl TerminalCommandContext {
    pub(super) fn is_local_terminal(&self) -> bool {
        self.terminal_type == TerminalCommandContextType::LocalTerminal
    }

    pub(super) fn is_remote_terminal(&self) -> bool {
        self.terminal_type == TerminalCommandContextType::Terminal
    }

    pub(super) fn provider_scope_id(&self) -> String {
        self.node_id
            .as_ref()
            .map(|node_id| node_id.0.clone())
            .or_else(|| self.session_id.map(|session_id| session_id.0.to_string()))
            .or_else(|| self.pane_id.map(|pane_id| pane_id.0.to_string()))
            .or_else(|| self.tab_id.map(|tab_id| tab_id.0.to_string()))
            .unwrap_or_default()
    }

    pub(super) fn target_fields(&self) -> Vec<String> {
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
pub(super) enum TerminalFigArgType {
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
pub(super) struct TerminalFigOptionSpec {
    pub(super) name: String,
    pub(super) description: Option<String>,
    pub(super) args: TerminalFigArgType,
}

#[derive(Clone)]
pub(super) struct TerminalFigSubcommandSpec {
    pub(super) name: String,
    pub(super) description: Option<String>,
    pub(super) options: Vec<TerminalFigOptionSpec>,
    pub(super) args: TerminalFigArgType,
}

#[derive(Clone)]
pub(in crate::workspace) struct TerminalFigSpec {
    pub(super) name: String,
    pub(super) description: String,
    pub(super) subcommands: Vec<TerminalFigSubcommandSpec>,
    pub(super) options: Vec<TerminalFigOptionSpec>,
    pub(super) args: TerminalFigArgType,
}

pub(super) struct TerminalPathParts {
    pub(super) directory: String,
    pub(super) query: String,
    pub(super) display_prefix: String,
}

#[derive(Clone)]
pub(super) struct TerminalPathEntry {
    pub(super) name: String,
    pub(super) path: String,
    pub(super) is_directory: bool,
}

pub(super) struct TerminalPathCacheEntry {
    pub(super) created_at: std::time::Instant,
    pub(super) entries: Vec<TerminalPathEntry>,
}

#[derive(Default)]
pub(super) struct TerminalPathCompletionCache {
    pub(super) entries: HashMap<String, TerminalPathCacheEntry>,
    pub(super) pending: HashSet<String>,
}

#[cfg(test)]
mod terminal_command_context_tests {
    use super::{TerminalCommandContext, TerminalCommandContextType};
    use crate::workspace::{NodeId, PaneId, TabId, TerminalSessionId};

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
