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

        terminal_history_suggestions(
            input, entries, self.terminal_history_all_connections.then_some(&origins), classify_command_risk,
        )
    }
}
