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
        let fig_specs = self.terminal_fig_specs();
        let active_arg_type = active_fig_arg_type(&parsed, &fig_specs);

        let mut suggestions =
            self.terminal_command_history_suggestions(input, allow_empty_history, cx);
        if allow_empty_history || !parsed.reliable {
            return normalize_terminal_command_suggestions(suggestions);
        }
        if input.trim().is_empty() {
            return normalize_terminal_command_suggestions(suggestions);
        }

        suggestions.extend(self.terminal_command_quick_command_suggestions(input));
        suggestions.extend(terminal_command_fig_suggestions(&parsed, &fig_specs));
        suggestions.extend(self.terminal_command_path_suggestions(&parsed, active_arg_type, cx));
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
}
