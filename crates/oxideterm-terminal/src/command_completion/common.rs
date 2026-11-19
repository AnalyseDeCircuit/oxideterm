use std::{collections::HashMap, time::SystemTime};

use super::{
    TerminalCommandSuggestion, TerminalCommandSuggestionKind, TerminalHistoryEntry,
    TerminalHistorySource,
};
use crate::terminal_autosuggest_fuzzy_score;

#[cfg(test)]
mod history_tests {
    use super::*;

    #[test]
    fn history_candidates_preserve_quoted_spacing_and_multiline_commands() {
        let mut entries = HashMap::new();
        for command in [
            "printf 'a  b'",
            "printf 'a b'",
            "printf first\nprintf second",
        ] {
            put_terminal_history_entry(
                &mut entries,
                command.to_owned(),
                TerminalHistorySource::Runtime,
                1,
                true,
                0,
            );
        }
        let mut commands = entries
            .into_values()
            .map(|entry| entry.command)
            .collect::<Vec<_>>();
        commands.sort();
        assert_eq!(
            commands,
            [
                "printf 'a  b'",
                "printf 'a b'",
                "printf first\nprintf second"
            ]
        );
    }
}

pub fn put_terminal_history_entry(
    entries: &mut HashMap<String, TerminalHistoryEntry>,
    command: String,
    source: TerminalHistorySource,
    last_used_at: i64,
    count_use: bool,
    sequence: usize,
) {
    if command.trim().is_empty() {
        return;
    }
    entries
        .entry(command.clone())
        .and_modify(|entry| {
            entry.last_used_at = entry.last_used_at.max(last_used_at);
            entry.sequence = sequence;
            if count_use {
                entry.uses = entry.uses.saturating_add(1);
            }
        })
        .or_insert(TerminalHistoryEntry {
            command,
            source,
            last_used_at,
            uses: 1,
            sequence,
        });
}

pub fn normalize_terminal_command_suggestions(
    suggestions: Vec<TerminalCommandSuggestion>,
) -> Vec<TerminalCommandSuggestion> {
    let mut by_key: HashMap<String, TerminalCommandSuggestion> = HashMap::new();
    for suggestion in suggestions {
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
    }
}

pub fn terminal_command_risk_score_penalty(risk: Option<&'static str>) -> f64 {
    match risk {
        Some("high") => 900.0,
        Some("medium") => 250.0,
        _ => 0.0,
    }
}
pub fn terminal_command_bar_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

pub fn terminal_history_suggestions(
    input: &str,
    entries: HashMap<String, TerminalHistoryEntry>,
    origins: Option<&HashMap<String, (u64, String)>>,
    classify_command_risk: impl Fn(&str) -> Option<&'static str>,
) -> Vec<TerminalCommandSuggestion> {
    let query = input.trim_start();
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
                description: origins.and_then(|origins| {
                    origins.get(&entry.command).map(|(_, label)| label.clone())
                }),
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

pub(super) fn completion_provider_scope(
    node: Option<&str>,
    session: Option<u64>,
    pane: Option<u64>,
    tab: Option<u64>,
) -> String {
    node.map(str::to_owned)
        .or_else(|| session.or(pane).or(tab).map(|id| id.to_string()))
        .unwrap_or_default()
}
