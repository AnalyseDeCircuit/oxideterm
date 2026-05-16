fn put_terminal_history_entry(
    entries: &mut HashMap<String, TerminalHistoryEntry>,
    command: String,
    source: TerminalHistorySource,
    last_used_at: i64,
    count_use: bool,
    sequence: usize,
) {
    let normalized = normalize_terminal_autosuggest_command(&command);
    if normalized.is_empty()
        || normalized.len() > 2000
        || is_likely_secret_terminal_command(&normalized)
    {
        return;
    }
    entries
        .entry(normalized.clone())
        .and_modify(|entry| {
            entry.last_used_at = entry.last_used_at.max(last_used_at);
            entry.sequence = sequence;
            if count_use {
                entry.uses = entry.uses.saturating_add(1);
            }
        })
        .or_insert(TerminalHistoryEntry {
            command: normalized,
            source,
            last_used_at,
            uses: 1,
            sequence,
        });
}

fn normalize_terminal_command_suggestions(
    suggestions: Vec<TerminalCommandSuggestion>,
) -> Vec<TerminalCommandSuggestion> {
    let mut by_key: HashMap<String, TerminalCommandSuggestion> = HashMap::new();
    for suggestion in suggestions {
        if is_likely_secret_terminal_command(&suggestion.insert_text) {
            continue;
        }
        let key = format!(
            "{}:{}:{}:{}:{}",
            suggestion.source_label_key,
            terminal_command_suggestion_kind_key(suggestion.kind),
            suggestion.insert_text,
            suggestion.replacement.start,
            suggestion.replacement.end
        );
        if by_key
            .get(&key)
            .is_none_or(|existing| suggestion.score > existing.score)
        {
            by_key.insert(key, suggestion);
        }
    }
    let mut ranked = by_key.into_values().collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.label.cmp(&right.label))
    });
    ranked.truncate(24);
    ranked
}

fn terminal_command_suggestion_kind_key(kind: TerminalCommandSuggestionKind) -> &'static str {
    match kind {
        TerminalCommandSuggestionKind::History => "history",
        TerminalCommandSuggestionKind::Command => "command",
        TerminalCommandSuggestionKind::Subcommand => "subcommand",
        TerminalCommandSuggestionKind::Option => "option",
        TerminalCommandSuggestionKind::File => "file",
        TerminalCommandSuggestionKind::Directory => "directory",
        TerminalCommandSuggestionKind::QuickCommand => "quick_command",
    }
}

fn terminal_command_risk_score_penalty(risk: Option<&'static str>) -> f64 {
    match risk {
        Some("high") => 900.0,
        Some("medium") => 250.0,
        _ => 0.0,
    }
}
