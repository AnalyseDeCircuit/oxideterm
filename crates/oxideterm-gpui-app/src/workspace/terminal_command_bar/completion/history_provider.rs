impl WorkspaceApp {
    fn terminal_command_history_suggestions(
        &self,
        input: &str,
        allow_empty_history: bool,
        cx: &mut Context<Self>,
    ) -> Vec<TerminalCommandSuggestion> {
        let query = input.trim_start();
        if query.is_empty() && !allow_empty_history {
            return Vec::new();
        }

        let mut entries: HashMap<String, TerminalHistoryEntry> = HashMap::new();
        for pane in self.panes.values() {
            for record in pane.read(cx).autosuggest_command_records() {
                put_terminal_history_entry(
                    &mut entries,
                    record.command,
                    TerminalHistorySource::Runtime,
                    record.finished_at as i64,
                    false,
                    record.started_at as usize,
                );
            }
        }
        for (sequence, record) in self.ai_runtime_command_records(cx).into_iter().enumerate() {
            put_terminal_history_entry(
                &mut entries,
                record.command,
                TerminalHistorySource::AiLedger,
                record.finished_at.unwrap_or(record.started_at),
                false,
                sequence,
            );
        }
        if self
            .settings_store
            .settings()
            .terminal
            .autosuggest
            .local_shell_history
            && self
                .active_tab()
                .is_some_and(|tab| tab.kind == TabKind::LocalTerminal)
        {
            for (index, command) in load_local_shell_history_commands().into_iter().enumerate() {
                put_terminal_history_entry(
                    &mut entries,
                    command,
                    TerminalHistorySource::LocalHistory,
                    terminal_command_bar_now_ms().saturating_sub(index as i64),
                    false,
                    index,
                );
            }
        }

        let now = terminal_command_bar_now_ms();
        let mut suggestions = entries
            .into_values()
            .filter_map(|entry| {
                let fuzzy = terminal_autosuggest_fuzzy_score(&entry.command, query);
                if !query.is_empty() && (fuzzy <= 0.0 || entry.command == query) {
                    return None;
                }
                let recency = (200
                    - now
                        .saturating_sub(entry.last_used_at)
                        .saturating_div(60_000))
                .max(0) as f64;
                let risk = classify_command_risk(&entry.command);
                let score = if query.is_empty() {
                    recency + entry.uses as f64 * 5.0
                } else {
                    fuzzy + recency + entry.uses as f64 * 5.0
                } + entry.sequence as f64 / 1_000_000.0
                    + 1000.0
                    - terminal_command_risk_score_penalty(risk);
                Some(TerminalCommandSuggestion {
                    kind: TerminalCommandSuggestionKind::History,
                    label: entry.command.clone(),
                    insert_text: entry.command.clone(),
                    description: None,
                    executable: true,
                    replacement: 0..input.len(),
                    group_label_key: "terminal.command_bar.group_history",
                    source_label_key: entry.source.label_key(),
                    score,
                    risk,
                    inline_safe: entry.command.starts_with(query) && risk != Some("high"),
                })
            })
            .collect::<Vec<_>>();
        suggestions.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| right.label.cmp(&left.label))
        });
        suggestions.truncate(8);
        suggestions
    }
}
