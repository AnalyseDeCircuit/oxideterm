use super::*;

impl WorkspaceApp {
    pub(super) fn terminal_command_history_suggestions(
        &self,
        input: &str,
        allow_empty_history: bool,
        context: &TerminalCommandContext,
        cx: &mut Context<Self>,
    ) -> Vec<TerminalCommandSuggestion> {
        let query = input.trim_start();
        if query.is_empty() && !allow_empty_history {
            return Vec::new();
        }

        let mut entries: HashMap<String, TerminalHistoryEntry> = HashMap::new();
        let mut origins = HashMap::new();
        let mut seen_ids = HashSet::new();
        if self.terminal_history_all_connections {
            for (_, label, history) in self.terminal_history_store.histories() {
                for (sequence, record) in history.records().into_iter().enumerate() {
                    seen_ids.insert(record.command_id.clone());
                    let origin = origins
                        .entry(record.command.clone())
                        .or_insert_with(|| (record.finished_at, label.clone()));
                    if record.finished_at > origin.0 {
                        *origin = (record.finished_at, label.clone());
                    }
                    put_terminal_history_entry(
                        &mut entries,
                        record.command,
                        TerminalHistorySource::Runtime,
                        record.finished_at as i64,
                        true,
                        sequence,
                    );
                }
            }
        }
        let tab_host = self.tab_host.read(cx);
        if let Some(pane) = context
            .pane_id
            .and_then(|pane_id| tab_host.panes().get(&pane_id))
        {
            for (sequence, record) in pane
                .read(cx)
                .history_command_records()
                .into_iter()
                .enumerate()
            {
                if !seen_ids.insert(record.command_id.clone()) {
                    continue;
                }
                origins.insert(
                    record.command.clone(),
                    (record.finished_at, context.target_label.clone()),
                );
                put_terminal_history_entry(
                    &mut entries,
                    record.command,
                    TerminalHistorySource::Runtime,
                    record.finished_at as i64,
                    true,
                    sequence,
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
                    + 1000.0;
                Some(TerminalCommandSuggestion {
                    kind: TerminalCommandSuggestionKind::History,
                    label: entry.command.clone(),
                    insert_text: entry.command.clone(),
                    description: self
                        .terminal_history_all_connections
                        .then(|| origins.get(&entry.command).map(|(_, label)| label.clone()))
                        .flatten(),
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
