use super::*;
use crate::workspace::standalone_connections::StandaloneConnectionSurface;

/// Runtime identity, never a hostname or a saved profile shared by separate connections.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum TabAffinity {
    Global,
    LocalTerminal,
    Ssh(NodeId),
    Standalone(String),
    FileTransfer(String),
    Plugin(String),
}

/// Terminal pages retain session IDs when merging removes their original tab descriptors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum TabContent {
    Page(TabId),
    Terminal(TerminalSessionId),
}

impl WorkspaceApp {
    pub(in crate::workspace) fn tab_affinities(&self, tab_id: TabId, cx: &App) -> Vec<TabAffinity> {
        let mut affinities = Vec::new();
        for content in self.tab_host.read(cx).tab_contents(tab_id) {
            for affinity in self.content_affinities(content, cx) {
                if !affinities.contains(&affinity) {
                    affinities.push(affinity);
                }
            }
        }
        affinities
    }

    fn content_affinities(&self, content: TabContent, cx: &App) -> Vec<TabAffinity> {
        match content {
            TabContent::Page(tab) => self.page_affinities(tab, cx),
            TabContent::Terminal(session) => self.terminal_affinities(session, cx),
        }
    }

    fn page_affinities(&self, tab_id: TabId, cx: &App) -> Vec<TabAffinity> {
        let Some(tab) = self.tab_by_id(tab_id, cx) else {
            return Vec::new();
        };
        if let Some(instance) = self
            .plugin_entity
            .read(cx)
            .connections
            .instance_for_page(tab_id)
        {
            return vec![TabAffinity::Plugin(instance.to_owned())];
        }
        if let Some(node) = self.sftp_tab_nodes.get(&tab_id) {
            return vec![TabAffinity::Ssh(node.clone())];
        }
        if let Some(binding) = self.standalone_sftp_tabs.get(&tab_id) {
            return std::iter::once(&binding.primary_endpoint_id)
                .chain(binding.secondary_endpoint_id.as_ref())
                .map(|id| TabAffinity::FileTransfer(id.clone()))
                .collect();
        }
        if let Some(node) = self.ide_workspace.read(cx).node_for_tab(tab_id) {
            return vec![TabAffinity::Ssh(node.clone())];
        }
        if let Some(node) = self.forwarding.read(cx).node_for_tab(tab_id) {
            return vec![TabAffinity::Ssh(node)];
        }
        if let Some(connection) = self
            .standalone_connections
            .connection_id_for_surface(StandaloneConnectionSurface::RemoteDesktop(tab_id))
        {
            return vec![TabAffinity::Standalone(connection)];
        }
        match tab.kind {
            // Missing connection bindings are not global pages. Pending/disconnected pages
            // remain in All until their runtime owner supplies the binding.
            TabKind::Sftp
            | TabKind::Forwards
            | TabKind::RemoteDesktop
            | TabKind::SshTerminal
            | TabKind::MoshTerminal
            | TabKind::LocalTerminal
            | TabKind::Workspace => Vec::new(),
            _ => vec![TabAffinity::Global],
        }
    }

    fn terminal_affinities(&self, session_id: TerminalSessionId, cx: &App) -> Vec<TabAffinity> {
        let host = self.tab_host.read(cx);
        let terminal = host
            .terminal_location(session_id)
            .and_then(|location| host.panes().get(&location.pane_id))
            .map(|pane| pane.read(cx).shared_session());
        let mut affinities = Vec::new();
        if let Some(instance) = terminal.as_ref().and_then(|terminal| {
            self.plugin_entity
                .read(cx)
                .connections
                .instance_for_terminal(terminal)
        }) {
            affinities.push(TabAffinity::Plugin(instance.to_owned()));
        }
        if let Some(node) = self
            .workspace_runtime
            .read(cx)
            .ssh_terminal_node_id(session_id)
        {
            affinities.push(TabAffinity::Ssh(node));
        } else if let Some(connection) = self
            .standalone_connections
            .connection_id_for_surface(StandaloneConnectionSurface::Terminal(session_id))
        {
            affinities.push(TabAffinity::Standalone(connection));
        } else if affinities.is_empty() && host.local_sessions.contains_key(&session_id) {
            // A plugin-owned process uses a local PTY but belongs to its plugin connection.
            affinities.push(TabAffinity::LocalTerminal);
        }
        affinities
    }
}
