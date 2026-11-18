use oxideterm_ssh::NodeId;
use oxideterm_workspace::{PaneId, TabId, TerminalSessionId};

use super::common::completion_provider_scope;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalCommandContextType {
    Terminal,
    LocalTerminal,
}

#[derive(Clone)]
pub struct TerminalCommandContext {
    pub pane_id: Option<PaneId>,
    pub session_id: Option<TerminalSessionId>,
    pub tab_id: Option<TabId>,
    pub terminal_type: TerminalCommandContextType,
    pub node_id: Option<NodeId>,
    pub cwd: Option<String>,
    pub cwd_host: Option<String>,
    pub target_label: String,
}

impl TerminalCommandContext {
    pub fn is_local_terminal(&self) -> bool {
        self.terminal_type == TerminalCommandContextType::LocalTerminal
    }

    pub fn is_remote_terminal(&self) -> bool {
        self.terminal_type == TerminalCommandContextType::Terminal
    }

    pub fn provider_scope_id(&self) -> String {
        completion_provider_scope(
            self.node_id.as_ref().map(|node| node.0.as_str()),
            self.session_id.map(|id| id.0),
            self.pane_id.map(|id| id.0),
            self.tab_id.map(|id| id.0),
        )
    }
}

#[cfg(test)]
mod terminal_command_context_tests {
    use super::{TerminalCommandContext, TerminalCommandContextType};
    use oxideterm_ssh::NodeId;
    use oxideterm_workspace::{PaneId, TabId, TerminalSessionId};

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
