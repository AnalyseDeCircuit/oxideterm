use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn terminal_command_sender_history_suggestions(
        &self,
        input: &str,
        allow_empty_history: bool,
        cx: &mut Context<Self>,
    ) -> Vec<TerminalCommandSuggestion> {
        if !self
            .settings_store
            .settings()
            .terminal
            .command_bar
            .smart_completion
        {
            return Vec::new();
        }
        let context = self.terminal_command_context(cx);
        self.terminal_command_history_suggestions(input, allow_empty_history, &context, cx)
    }

    pub(in crate::workspace) fn terminal_command_sender_visible_history_suggestions(
        &self,
        input: &str,
        cx: &mut Context<Self>,
    ) -> Vec<TerminalCommandSuggestion> {
        let suggestions = self.terminal_command_sender_history_suggestions(input, false, cx);
        if suggestions.is_empty() {
            self.terminal_command_sender_history_suggestions(input, true, cx)
        } else {
            suggestions
        }
    }

    pub(in crate::workspace) fn terminal_command_active_target_label(
        &self,
        cx: &mut Context<Self>,
    ) -> String {
        // Rendered command-bar chrome uses the same inferred target label as
        // completion providers without exposing the full private context model.
        self.terminal_command_context(cx).target_label
    }

    pub(in crate::workspace) fn terminal_command_context(
        &self,
        cx: &mut Context<Self>,
    ) -> TerminalCommandContext {
        let pane_id = self.active_pane_id(cx);
        let tab_projection = self.active_tab(cx).map(|tab| {
            let session_id =
                pane_id.and_then(|pane_id| tab.root_pane.as_ref()?.session_id_for_pane(pane_id));
            (tab.id, tab.kind.clone(), tab.title.clone(), session_id)
        });
        let session_id = tab_projection
            .as_ref()
            .and_then(|(_, _, _, session_id)| *session_id);
        let node_id = session_id.and_then(|session_id| {
            self.workspace_runtime
                .read(cx)
                .ssh_terminal_node_id(session_id)
        });
        let cwd = self.terminal_command_context_cwd(
            pane_id,
            tab_projection.as_ref().map(|(_, kind, _, _)| kind),
            cx,
        );
        let cwd_host = pane_id
            .and_then(|pane_id| self.tab_host.read(cx).panes().get(&pane_id))
            .and_then(|pane| pane.read(cx).current_working_directory_host())
            .filter(|host| !host.trim().is_empty());
        let terminal_type = match tab_projection.as_ref().map(|(_, kind, _, _)| kind) {
            Some(TabKind::LocalTerminal) => TerminalCommandContextType::LocalTerminal,
            _ => TerminalCommandContextType::Terminal,
        };
        let target_label = self.terminal_command_target_label(
            tab_projection.as_ref().map(|(_, kind, _, _)| kind),
            tab_projection
                .as_ref()
                .map(|(_, _, title, _)| title.as_str()),
            node_id.as_ref(),
            cwd.as_deref(),
            cwd_host.as_deref(),
            cx,
        );

        TerminalCommandContext {
            pane_id,
            session_id,
            tab_id: tab_projection.as_ref().map(|(tab_id, _, _, _)| *tab_id),
            terminal_type,
            node_id,
            cwd,
            cwd_host,
            target_label,
        }
    }

    fn terminal_command_target_label(
        &self,
        tab_kind: Option<&TabKind>,
        tab_title: Option<&str>,
        node_id: Option<&NodeId>,
        cwd: Option<&str>,
        cwd_host: Option<&str>,
        cx: &mut Context<Self>,
    ) -> String {
        let Some(tab_kind) = tab_kind else {
            return self.i18n.t("terminal.command_bar.remote_shell");
        };
        if *tab_kind != TabKind::LocalTerminal {
            if let Some(node_id) = node_id
                && let Some(node) = self.ssh_nodes.get(node_id)
            {
                return format!("{}@{}", node.endpoint.username, node.endpoint.host);
            }
            return tab_title.unwrap_or_default().to_string();
        }

        if let Some(identity) = self
            .active_pane_id(cx)
            .and_then(|pane_id| self.tab_host.read(cx).panes().get(&pane_id))
            .map(|pane| pane.read(cx).visible_text_snapshot())
            .and_then(|text| infer_terminal_ssh_identity_from_buffer(&text))
        {
            return identity;
        }

        if let Some(cwd_host) = cwd_host
            && cwd.is_some_and(terminal_cwd_looks_remote)
        {
            return cwd_host.to_string();
        }

        self.i18n.t("terminal.command_bar.local_shell")
    }
}
