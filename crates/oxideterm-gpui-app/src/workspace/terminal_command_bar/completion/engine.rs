impl WorkspaceApp {
    pub(in crate::workspace) fn terminal_command_bar_suggestions(
        &self,
        allow_empty_history: bool,
        cx: &mut Context<Self>,
    ) -> Vec<TerminalCommandSuggestion> {
        let settings = self.settings_store.settings();
        if !settings.terminal.command_bar.smart_completion {
            return Vec::new();
        }
        let input = self.terminal_command_bar_draft.as_str();
        let cursor_index = input.len();
        let parsed = tokenize_terminal_command_line(input, cursor_index);
        let context = self.terminal_command_context(cx);
        let fig_specs = self.terminal_fig_specs();
        let active_arg_type = active_fig_arg_type(&parsed, &fig_specs);

        let mut suggestions =
            self.terminal_command_history_suggestions(input, allow_empty_history, &context, cx);
        if allow_empty_history || !parsed.reliable {
            return normalize_terminal_command_suggestions(suggestions);
        }
        if input.trim().is_empty() {
            return normalize_terminal_command_suggestions(suggestions);
        }

        suggestions.extend(self.terminal_command_quick_command_suggestions(input, &context));
        suggestions.extend(terminal_command_fig_suggestions(&parsed, &fig_specs));
        suggestions.extend(self.terminal_command_path_suggestions(
            &parsed,
            active_arg_type,
            &context,
            cx,
        ));
        normalize_terminal_command_suggestions(suggestions)
    }

    pub(in crate::workspace) fn terminal_command_bar_visible_suggestions(
        &self,
        cx: &mut Context<Self>,
    ) -> Vec<TerminalCommandSuggestion> {
        let suggestions = self.terminal_command_bar_suggestions(false, cx);
        if suggestions.is_empty() && self.terminal_command_suggestions_open {
            self.terminal_command_bar_suggestions(true, cx)
        } else {
            suggestions
        }
    }

    pub(in crate::workspace) fn accept_terminal_command_suggestion(
        &mut self,
        suggestion: &TerminalCommandSuggestion,
        cx: &mut Context<Self>,
    ) {
        let input_len = self.terminal_command_bar_draft.len();
        let start = suggestion.replacement.start.min(input_len);
        let end = suggestion.replacement.end.min(input_len).max(start);
        self.terminal_command_bar_draft
            .replace_range(start..end, &suggestion.insert_text);
        self.terminal_command_suggestions_open = false;
        self.terminal_command_suggestion_highlighted = None;
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(in crate::workspace) fn terminal_command_ghost_text(
        &self,
        suggestions: &[TerminalCommandSuggestion],
    ) -> Option<String> {
        self.terminal_command_inline_suggestion(suggestions)
            .and_then(|(suggestion, current)| {
                suggestion
                    .insert_text
                    .strip_prefix(current)
                    .filter(|suffix| !suffix.is_empty())
                    .map(ToString::to_string)
            })
    }

    pub(in crate::workspace) fn terminal_command_inline_suggestion_for_accept(
        &self,
        suggestions: &[TerminalCommandSuggestion],
    ) -> Option<TerminalCommandSuggestion> {
        self.terminal_command_inline_suggestion(suggestions)
            .map(|(suggestion, _)| suggestion.clone())
    }

    fn terminal_command_inline_suggestion<'a>(
        &'a self,
        suggestions: &'a [TerminalCommandSuggestion],
    ) -> Option<(&'a TerminalCommandSuggestion, &'a str)> {
        if self.terminal_command_bar_draft.contains('\n') {
            return None;
        }
        suggestions
            .iter()
            .find(|candidate| candidate.inline_safe)
            .and_then(|candidate| {
                let input_len = self.terminal_command_bar_draft.len();
                let start = candidate.replacement.start.min(input_len);
                let end = candidate.replacement.end.min(input_len).max(start);
                let current = &self.terminal_command_bar_draft[start..end];
                candidate
                    .insert_text
                    .starts_with(current)
                    .then_some((candidate, current))
            })
    }

    pub(in crate::workspace) fn terminal_command_active_target_label(
        &self,
        cx: &mut Context<Self>,
    ) -> String {
        // Rendered command-bar chrome uses the same inferred target label as
        // completion providers without exposing the full private context model.
        self.terminal_command_context(cx).target_label
    }

    fn terminal_command_context(&self, cx: &mut Context<Self>) -> TerminalCommandContext {
        let tab = self.active_tab();
        let pane_id = self.active_pane_id();
        let session_id = tab
            .and_then(|tab| pane_id.and_then(|pane_id| tab.root_pane.as_ref()?.session_id_for_pane(pane_id)));
        let node_id = session_id.and_then(|session_id| self.terminal_ssh_nodes.get(&session_id).cloned());
        let cwd = self.terminal_command_context_cwd(pane_id, tab.map(|tab| &tab.kind), cx);
        let cwd_host = pane_id
            .and_then(|pane_id| self.panes.get(&pane_id))
            .and_then(|pane| pane.read(cx).current_working_directory_host())
            .filter(|host| !host.trim().is_empty());
        let terminal_type = match tab.map(|tab| &tab.kind) {
            Some(TabKind::LocalTerminal) => TerminalCommandContextType::LocalTerminal,
            _ => TerminalCommandContextType::Terminal,
        };
        let target_label = self.terminal_command_target_label(tab, node_id.as_ref(), cwd.as_deref(), cwd_host.as_deref(), cx);

        TerminalCommandContext {
            pane_id,
            session_id,
            tab_id: tab.map(|tab| tab.id),
            terminal_type,
            node_id,
            cwd,
            cwd_host,
            target_label,
        }
    }

    fn terminal_command_target_label(
        &self,
        tab: Option<&Tab>,
        node_id: Option<&NodeId>,
        cwd: Option<&str>,
        cwd_host: Option<&str>,
        cx: &mut Context<Self>,
    ) -> String {
        let Some(tab) = tab else {
            return self.i18n.t("terminal.command_bar.remote_shell");
        };
        if tab.kind != TabKind::LocalTerminal {
            if let Some(node_id) = node_id
                && let Some(node) = self.ssh_nodes.get(node_id)
            {
                return format!("{}@{}", node.config.username, node.config.host);
            }
            return tab.title.clone();
        }

        if let Some(identity) = self
            .active_pane_id()
            .and_then(|pane_id| self.panes.get(&pane_id))
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
