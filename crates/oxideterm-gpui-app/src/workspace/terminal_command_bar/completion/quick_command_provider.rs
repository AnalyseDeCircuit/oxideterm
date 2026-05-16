impl WorkspaceApp {
    fn terminal_command_quick_command_suggestions(
        &self,
        input: &str,
    ) -> Vec<TerminalCommandSuggestion> {
        let command_bar_settings = &self.settings_store.settings().terminal.command_bar;
        if !command_bar_settings.quick_commands_enabled {
            return Vec::new();
        }
        let query = input.trim().to_lowercase();
        if query.is_empty() {
            return Vec::new();
        }
        let target_fields = self.terminal_command_target_fields();
        self.quick_commands
            .commands
            .iter()
            .filter(|command| {
                match_quick_command_host_pattern(command.host_pattern.as_deref(), &target_fields)
                    && (command.name.to_lowercase().contains(&query)
                        || command.command.to_lowercase().contains(&query)
                        || command
                            .description
                            .as_ref()
                            .is_some_and(|description| description.to_lowercase().contains(&query)))
            })
            .take(8)
            .map(|command| {
                let risk = classify_command_risk(&command.command);
                let starts_with_input = command
                    .command
                    .to_lowercase()
                    .starts_with(&input.trim_start().to_lowercase());
                TerminalCommandSuggestion {
                    kind: TerminalCommandSuggestionKind::QuickCommand,
                    label: command.command.clone(),
                    insert_text: command.command.clone(),
                    description: Some(command.name.clone()),
                    executable: true,
                    replacement: 0..input.len(),
                    group_label_key: "terminal.command_bar.group_quick_commands",
                    source_label_key: "terminal.command_bar.source_quick_command",
                    score: 860.0 - terminal_command_risk_score_penalty(risk),
                    risk,
                    inline_safe: starts_with_input && risk != Some("high"),
                }
            })
            .collect()
    }

    fn terminal_command_target_fields(&self) -> Vec<String> {
        let mut fields = Vec::new();
        if let Some(tab) = self.active_tab() {
            fields.push(self.tab_display_title(tab));
            fields.push(tab.title.clone());
        }
        if let Some(tab) = self.active_tab()
            && let Some(pane_id) = tab.active_pane_id
            && let Some(session_id) = tab
                .root_pane
                .as_ref()
                .and_then(|root| root.session_id_for_pane(pane_id))
            && let Some(node_id) = self.terminal_ssh_nodes.get(&session_id)
        {
            fields.push(node_id.0.clone());
        }
        fields.retain(|field| !field.trim().is_empty());
        fields
    }
}
