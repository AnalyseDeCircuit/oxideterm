use std::{
    collections::HashMap,
    fmt,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use oxideterm_terminal::{
    TerminalCommandMark, TerminalCommandMarkClosedBy, TerminalCommandMarkConfidence,
    TerminalCommandMarkDetectionSource,
};
use parking_lot::Mutex;

use crate::terminal_ui::MAX_HIGHLIGHT_PATTERN_LENGTH;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalCommandFactStatus {
    Open,
    Closed,
    Stale,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalCommandFact {
    pub fact_id: String,
    pub client_mark_id: String,
    pub source: TerminalCommandMarkDetectionSource,
    pub submitted_by: Option<TerminalCommandMarkDetectionSource>,
    pub command: Option<String>,
    pub start_global_line: usize,
    pub command_global_line: usize,
    pub output_start_global_line: usize,
    pub end_global_line: Option<usize>,
    pub status: TerminalCommandFactStatus,
    pub confidence: TerminalCommandMarkConfidence,
    pub closed_by: Option<TerminalCommandMarkClosedBy>,
    pub exit_code: Option<i32>,
    pub created_at: u64,
    pub closed_at: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalAiCommandRecord {
    pub command_id: String,
    pub command: String,
    pub source: TerminalCommandMarkDetectionSource,
    pub status: TerminalCommandFactStatus,
    pub started_at: u64,
    pub finished_at: Option<u64>,
    pub exit_code: Option<i32>,
    pub start_line: usize,
    pub end_line: Option<usize>,
}

#[derive(Clone, Eq, PartialEq)]
pub struct TerminalAutosuggestCommandRecord {
    pub command_id: String,
    pub command: String,
    pub started_at: u64,
    pub finished_at: u64,
}

impl fmt::Debug for TerminalAutosuggestCommandRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TerminalAutosuggestCommandRecord")
            .field("command_id", &self.command_id)
            // Command lines can contain credentials and must not enter diagnostic output.
            .field("command", &"<redacted>")
            .field("started_at", &self.started_at)
            .field("finished_at", &self.finished_at)
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct TerminalAutosuggestCandidate {
    pub(crate) command: String,
    pub(crate) use_count: usize,
    pub(crate) last_used_at: u64,
}

const MAX_AUTOSUGGEST_RECORDS: usize = 10_000;

#[derive(Clone, Default)]
pub struct SharedTerminalCommandHistory {
    state: Arc<Mutex<TerminalCommandHistoryState>>,
}

#[derive(Default)]
struct TerminalCommandHistoryState {
    records: Vec<TerminalAutosuggestCommandRecord>,
}

impl SharedTerminalCommandHistory {
    pub fn from_commands(commands: Vec<String>) -> Self {
        let now = now_millis();
        let command_count = commands.len();
        let records = commands
            .into_iter()
            .enumerate()
            .filter(|(_, command)| !command.trim().is_empty())
            .map(|(index, command)| {
                // Shell history is oldest-first, so retain that ordering when timestamps are absent.
                let used_at = now.saturating_sub(command_count.saturating_sub(index) as u64);
                TerminalAutosuggestCommandRecord {
                    command_id: format!("shell-history-{used_at}-{index}"),
                    command,
                    started_at: used_at,
                    finished_at: used_at,
                }
            })
            .collect::<Vec<_>>();
        let mut state = TerminalCommandHistoryState { records };
        trim_autosuggest_records(&mut state.records);
        Self {
            state: Arc::new(Mutex::new(state)),
        }
    }

    pub(crate) fn records(&self) -> Vec<TerminalAutosuggestCommandRecord> {
        self.state.lock().records.clone()
    }

    pub(crate) fn candidates(
        &self,
        state: &TerminalAutosuggestInputState,
        limit: usize,
    ) -> Vec<TerminalAutosuggestCandidate> {
        autosuggest_candidates_for_records(&self.state.lock().records, state, limit)
    }

    pub(crate) fn ghost_text(&self, state: &TerminalAutosuggestInputState) -> Option<String> {
        let query = state.value.as_str();
        self.candidates(state, 1)
            .into_iter()
            .next()
            .and_then(|candidate| candidate.command.strip_prefix(query).map(str::to_string))
            .filter(|suffix| !suffix.is_empty())
    }

    pub(crate) fn record(&self, command: &str) -> bool {
        if command.trim().is_empty() {
            return false;
        }
        let now = now_millis();
        let mut state = self.state.lock();
        state.records.push(TerminalAutosuggestCommandRecord {
            command_id: format!("runtime-autosuggest-{now}"),
            command: command.to_string(),
            started_at: now,
            finished_at: now,
        });
        trim_autosuggest_records(&mut state.records);
        true
    }

    pub(crate) fn remove(&self, command: &str) -> bool {
        let mut state = self.state.lock();
        let previous_len = state.records.len();
        state.records.retain(|record| record.command != command);
        if state.records.len() == previous_len {
            return false;
        }
        true
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalAutosuggestInputState {
    pub value: String,
    pub cursor_index: usize,
    pub is_cursor_at_end: bool,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct TransientCommandHighlight {
    pub(crate) command_id: Arc<str>,
    pub(crate) query: Arc<str>,
    pub(crate) case_sensitive: bool,
    pub(crate) output_start_global_line: usize,
    pub(crate) output_end_global_line: Option<usize>,
}

#[derive(Debug, Eq, PartialEq)]
struct TransientLiteralQuery {
    query: String,
    case_sensitive: bool,
}

#[derive(Default)]
pub(crate) struct CommandFactLedger {
    facts: Vec<TerminalCommandFact>,
    ai_records: Vec<TerminalAiCommandRecord>,
    autosuggest_records: Vec<TerminalAutosuggestCommandRecord>,
    transient_command_highlight: Option<TransientCommandHighlight>,
}

impl CommandFactLedger {
    pub(crate) fn facts(&self) -> Vec<TerminalCommandFact> {
        self.facts.clone()
    }

    pub(crate) fn ai_records(&self) -> Vec<TerminalAiCommandRecord> {
        self.ai_records.clone()
    }

    pub(crate) fn autosuggest_records(&self) -> Vec<TerminalAutosuggestCommandRecord> {
        self.autosuggest_records.clone()
    }

    pub(crate) fn transient_command_highlight(&self) -> Option<TransientCommandHighlight> {
        self.transient_command_highlight.clone()
    }

    pub(crate) fn autosuggest_ghost_text(
        &self,
        state: &TerminalAutosuggestInputState,
    ) -> Option<String> {
        let query = state.value.trim_start();
        if query.is_empty() || !state.is_cursor_at_end {
            return None;
        }
        self.autosuggest_records
            .iter()
            .rev()
            .find_map(|record| {
                (record.command.starts_with(query) && record.command != query)
                    .then(|| record.command[query.len()..].to_string())
            })
            .filter(|suffix| !suffix.is_empty())
    }

    #[cfg(test)]
    pub(crate) fn autosuggest_candidates(
        &self,
        state: &TerminalAutosuggestInputState,
        limit: usize,
    ) -> Vec<TerminalAutosuggestCandidate> {
        autosuggest_candidates_for_records(&self.autosuggest_records, state, limit)
    }

    pub(crate) fn remove_autosuggest_command(&mut self, command: &str) -> bool {
        let previous_len = self.autosuggest_records.len();
        self.autosuggest_records
            .retain(|record| record.command != command);
        self.autosuggest_records.len() != previous_len
    }

    pub(crate) fn record_runtime_autosuggest_command(&mut self, command: &str) {
        if command.trim().is_empty() {
            return;
        }
        let now = now_millis();
        self.autosuggest_records
            .push(TerminalAutosuggestCommandRecord {
                command_id: format!("runtime-autosuggest-{now}"),
                command: command.to_string(),
                started_at: now,
                finished_at: now,
            });
        trim_autosuggest_records(&mut self.autosuggest_records);
    }

    pub(crate) fn create_from_mark(&mut self, mark: &TerminalCommandMark) {
        if self
            .facts
            .iter()
            .any(|fact| fact.client_mark_id == mark.command_id)
        {
            return;
        }

        self.close_previous_open(mark.start_line);
        // The pane owns one derived query for only the latest command fact. Replacing
        // it here prevents a prior grep query from leaking into later output.
        self.transient_command_highlight = mark
            .command
            .as_deref()
            .and_then(transient_literal_query)
            .map(|query| TransientCommandHighlight {
                command_id: Arc::from(mark.command_id.as_str()),
                query: Arc::from(query.query),
                case_sensitive: query.case_sensitive,
                output_start_global_line: mark.command_line.saturating_add(1),
                output_end_global_line: None,
            });
        let fact = TerminalCommandFact {
            fact_id: format!("native-command-fact-{}", mark.command_id),
            client_mark_id: mark.command_id.clone(),
            source: mark.detection_source,
            submitted_by: mark.submitted_by,
            command: mark
                .command
                .clone()
                .filter(|command| !command.trim().is_empty()),
            start_global_line: mark.start_line,
            command_global_line: mark.command_line,
            output_start_global_line: mark.command_line.saturating_add(1),
            end_global_line: None,
            status: TerminalCommandFactStatus::Open,
            confidence: mark.confidence,
            closed_by: None,
            exit_code: None,
            created_at: now_millis(),
            closed_at: None,
        };
        self.record_ai_command_if_eligible(mark, &fact);
        self.facts.push(fact);
    }

    pub(crate) fn close_from_mark(&mut self, mark: &TerminalCommandMark) {
        let mut closed_fact = None;
        if let Some(fact) = self
            .facts
            .iter_mut()
            .find(|fact| fact.client_mark_id == mark.command_id)
        {
            fact.end_global_line = Some(
                mark.end_line
                    .unwrap_or(mark.start_line)
                    .max(mark.start_line),
            );
            fact.status = if mark.stale {
                TerminalCommandFactStatus::Stale
            } else {
                TerminalCommandFactStatus::Closed
            };
            fact.closed_by = mark.closed_by;
            fact.exit_code = mark.exit_code;
            fact.closed_at = Some(mark.finished_at.unwrap_or_else(now_millis));
            closed_fact = Some(fact.clone());
        }

        if self
            .transient_command_highlight
            .as_ref()
            .is_some_and(|highlight| highlight.command_id.as_ref() == mark.command_id)
        {
            if mark.stale {
                self.transient_command_highlight = None;
            } else if let Some(highlight) = self.transient_command_highlight.as_mut() {
                highlight.output_end_global_line =
                    closed_fact.as_ref().and_then(|fact| fact.end_global_line);
            }
        }

        if let Some(fact) = closed_fact {
            self.record_ai_command_if_eligible(mark, &fact);
        }
    }

    fn close_previous_open(&mut self, next_start_line: usize) {
        let now = now_millis();
        for fact in &mut self.facts {
            if fact.status != TerminalCommandFactStatus::Open {
                continue;
            }
            fact.status = TerminalCommandFactStatus::Closed;
            fact.end_global_line = Some(
                next_start_line
                    .saturating_sub(1)
                    .max(fact.start_global_line),
            );
            fact.closed_by = Some(TerminalCommandMarkClosedBy::NextCommand);
            fact.closed_at = Some(now);
        }
    }

    fn record_ai_command_if_eligible(
        &mut self,
        mark: &TerminalCommandMark,
        fact: &TerminalCommandFact,
    ) {
        let Some(command) = mark
            .command
            .as_deref()
            .map(str::trim)
            .filter(|command| !command.is_empty())
        else {
            return;
        };
        if fact.confidence != TerminalCommandMarkConfidence::High {
            return;
        }
        if !matches!(
            fact.source,
            TerminalCommandMarkDetectionSource::CommandBar
                | TerminalCommandMarkDetectionSource::Ai
                | TerminalCommandMarkDetectionSource::Broadcast
                | TerminalCommandMarkDetectionSource::ShellIntegration
        ) {
            return;
        }
        if let Some(record) = self
            .ai_records
            .iter_mut()
            .find(|record| record.command_id == mark.command_id)
        {
            // The opening record gives AI a stable identifier immediately;
            // closing the same fact fills in its authoritative status/exit code.
            record.status = fact.status;
            record.finished_at = mark.finished_at;
            record.exit_code = mark.exit_code;
            record.end_line = fact.end_global_line;
            return;
        }

        self.ai_records.push(TerminalAiCommandRecord {
            command_id: mark.command_id.clone(),
            command: command.to_string(),
            source: fact.source,
            status: fact.status,
            started_at: mark.started_at,
            finished_at: mark.finished_at,
            exit_code: mark.exit_code,
            start_line: mark.start_line,
            end_line: fact.end_global_line,
        });
        const MAX_AI_RECORDS: usize = 200;
        if self.ai_records.len() > MAX_AI_RECORDS {
            let overflow = self.ai_records.len() - MAX_AI_RECORDS;
            self.ai_records.drain(0..overflow);
        }
    }
}

fn transient_literal_query(command: &str) -> Option<TransientLiteralQuery> {
    // Keep the first stage intentionally conservative: unsupported shell syntax
    // or grep/rg options produce no transient highlight instead of a wrong one.
    let tokens = command.split_whitespace().collect::<Vec<_>>();
    let mut command_position = true;
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        if is_shell_separator(token) {
            command_position = true;
            index += 1;
            continue;
        }
        if command_position {
            command_position = false;
            let executable = token.rsplit(['/', '\\']).next().unwrap_or(token);
            let executable = executable.strip_suffix(".exe").unwrap_or(executable);
            if executable.eq_ignore_ascii_case("grep") || executable.eq_ignore_ascii_case("rg") {
                return literal_query_after_command(&tokens[index + 1..]);
            }
        }
        index += 1;
    }
    None
}

fn autosuggest_candidates_for_records(
    records: &[TerminalAutosuggestCommandRecord],
    state: &TerminalAutosuggestInputState,
    limit: usize,
) -> Vec<TerminalAutosuggestCandidate> {
    let query = state.value.as_str();
    if query.trim().is_empty() || !state.is_cursor_at_end || limit == 0 {
        return Vec::new();
    }

    let mut candidates_by_command = HashMap::<&str, TerminalAutosuggestCandidate>::new();
    for record in records {
        if !record.command.starts_with(query) || record.command == query {
            continue;
        }
        let candidate = candidates_by_command
            .entry(&record.command)
            .or_insert_with(|| TerminalAutosuggestCandidate {
                command: record.command.clone(),
                use_count: 0,
                last_used_at: record.finished_at,
            });
        candidate.use_count = candidate.use_count.saturating_add(1);
        candidate.last_used_at = candidate.last_used_at.max(record.finished_at);
    }

    let mut candidates = candidates_by_command.into_values().collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .use_count
            .cmp(&left.use_count)
            .then_with(|| right.last_used_at.cmp(&left.last_used_at))
            .then_with(|| left.command.cmp(&right.command))
    });
    candidates.truncate(limit);
    candidates
}

fn trim_autosuggest_records(records: &mut Vec<TerminalAutosuggestCommandRecord>) {
    if records.len() > MAX_AUTOSUGGEST_RECORDS {
        let overflow = records.len() - MAX_AUTOSUGGEST_RECORDS;
        records.drain(0..overflow);
    }
}

fn literal_query_after_command(tokens: &[&str]) -> Option<TransientLiteralQuery> {
    let mut case_sensitive = true;
    let mut fixed_strings = false;
    let mut options_ended = false;
    for token in tokens {
        if is_shell_separator(token) {
            return None;
        }
        if !options_ended && *token == "--" {
            options_ended = true;
            continue;
        }
        if !options_ended && token.starts_with("--") {
            match *token {
                "--ignore-case" => case_sensitive = false,
                "--case-sensitive" => case_sensitive = true,
                "--fixed-strings" => fixed_strings = true,
                "--line-number" | "--with-filename" | "--no-filename" | "--only-matching"
                | "--no-color" | "--text" | "--hidden" => {}
                "--invert-match" | "--regexp" | "--smart-case" => return None,
                option if option.starts_with("--color=") => {}
                _ => return None,
            }
            continue;
        }
        if !options_ended && token.starts_with('-') && token.len() > 1 {
            for flag in token[1..].chars() {
                match flag {
                    'i' => case_sensitive = false,
                    's' => case_sensitive = true,
                    'F' => fixed_strings = true,
                    'n' | 'H' | 'h' | 'o' | 'u' | 'a' => {}
                    'v' | 'e' | 'S' => return None,
                    _ => return None,
                }
            }
            continue;
        }

        let query = simple_query_token(token)?;
        if query.chars().count() > MAX_HIGHLIGHT_PATTERN_LENGTH
            || (!fixed_strings && query.chars().any(is_regex_meta_character))
        {
            return None;
        }
        let query = if case_sensitive {
            query.to_string()
        } else {
            query.to_lowercase()
        };
        return Some(TransientLiteralQuery {
            query,
            case_sensitive,
        });
    }
    None
}

fn simple_query_token(token: &str) -> Option<&str> {
    if token.is_empty()
        || token
            .chars()
            .any(|ch| matches!(ch, '$' | '`' | '<' | '>' | '&' | ';'))
    {
        return None;
    }
    let bytes = token.as_bytes();
    if matches!(bytes.first(), Some(b'\'') | Some(b'"')) {
        let quote = *bytes.first()?;
        if bytes.len() < 2 || bytes.last().copied() != Some(quote) {
            return None;
        }
        let inner = &token[1..token.len() - 1];
        return (!inner.is_empty() && !inner.as_bytes().contains(&quote)).then_some(inner);
    }
    (!token.contains('\'') && !token.contains('"')).then_some(token)
}

fn is_shell_separator(token: &str) -> bool {
    matches!(token, "|" | "||" | "&&" | ";")
}

fn is_regex_meta_character(ch: char) -> bool {
    matches!(
        ch,
        '.' | '^' | '$' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '\\'
    )
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mark(command_id: &str, command: Option<&str>, closed: bool) -> TerminalCommandMark {
        TerminalCommandMark {
            command_id: command_id.to_string(),
            command: command.map(str::to_string),
            start_line: 10,
            command_line: 10,
            end_line: closed.then_some(12),
            is_closed: closed,
            closed_by: closed.then_some(TerminalCommandMarkClosedBy::ShellIntegration),
            exit_code: closed.then_some(0),
            duration_ms: closed.then_some(20),
            detection_source: TerminalCommandMarkDetectionSource::ShellIntegration,
            submitted_by: None,
            confidence: TerminalCommandMarkConfidence::High,
            output_confidence: TerminalCommandMarkConfidence::High,
            stale: false,
            started_at: 100,
            finished_at: closed.then_some(120),
        }
    }

    #[test]
    fn command_fact_ledger_closes_fact_and_records_ai_command() {
        let mut ledger = CommandFactLedger::default();
        ledger.create_from_mark(&mark("cmd-1", Some("ls"), false));
        ledger.close_from_mark(&mark("cmd-1", Some("ls"), true));

        let facts = ledger.facts();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].status, TerminalCommandFactStatus::Closed);
        assert_eq!(facts[0].end_global_line, Some(12));

        let records = ledger.ai_records();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].command, "ls");
        assert_eq!(records[0].status, TerminalCommandFactStatus::Closed);
    }

    #[test]
    fn command_fact_ledger_skips_empty_commands_for_ai_records() {
        let mut ledger = CommandFactLedger::default();
        ledger.create_from_mark(&mark("cmd-1", Some("  "), false));
        ledger.close_from_mark(&mark("cmd-1", Some("  "), true));

        assert_eq!(ledger.facts().len(), 1);
        assert!(ledger.ai_records().is_empty());
    }

    #[test]
    fn command_fact_ledger_records_and_exposes_runtime_autosuggest() {
        let mut ledger = CommandFactLedger::default();

        ledger.record_runtime_autosuggest_command("  git   status  ");
        ledger.record_runtime_autosuggest_command("  git   status  ");
        ledger.record_runtime_autosuggest_command("git status");
        ledger.record_runtime_autosuggest_command(" ");

        let records = ledger.autosuggest_records();
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].command, "  git   status  ");
        assert_eq!(records[1].command, "  git   status  ");
        assert_eq!(records[2].command, "git status");

        let mut ledger = CommandFactLedger::default();
        ledger.record_runtime_autosuggest_command("git status");
        ledger.record_runtime_autosuggest_command("git stash list");

        assert_eq!(
            ledger.autosuggest_ghost_text(&TerminalAutosuggestInputState {
                value: "git sta".to_string(),
                cursor_index: 7,
                is_cursor_at_end: true,
            }),
            Some("sh list".to_string())
        );
        assert_eq!(
            ledger.autosuggest_ghost_text(&TerminalAutosuggestInputState {
                value: "git status".to_string(),
                cursor_index: 10,
                is_cursor_at_end: true,
            }),
            None
        );
        assert_eq!(
            ledger.autosuggest_ghost_text(&TerminalAutosuggestInputState {
                value: "git sta".to_string(),
                cursor_index: 3,
                is_cursor_at_end: false,
            }),
            None
        );
    }

    #[test]
    fn runtime_autosuggest_candidates_rank_activity_without_changing_history() {
        let mut ledger = CommandFactLedger::default();
        ledger.record_runtime_autosuggest_command("docker ps");
        ledger.record_runtime_autosuggest_command("docker images");
        ledger.record_runtime_autosuggest_command("docker ps");
        ledger.record_runtime_autosuggest_command("docker compose up");

        let state = TerminalAutosuggestInputState {
            value: "dock".to_string(),
            cursor_index: 4,
            is_cursor_at_end: true,
        };
        let candidates = ledger.autosuggest_candidates(&state, 3);

        assert_eq!(candidates.len(), 3);
        assert_eq!(candidates[0].command, "docker ps");
        assert_eq!(candidates[0].use_count, 2);
        assert_eq!(ledger.autosuggest_records().len(), 4);

        assert!(ledger.remove_autosuggest_command("docker ps"));
        assert!(
            ledger
                .autosuggest_records()
                .iter()
                .all(|record| record.command != "docker ps")
        );
    }

    #[test]
    fn shared_command_history_keeps_commands_without_content_filtering() {
        let history = SharedTerminalCommandHistory::default();
        let command = "curl -H 'Authorization: Bearer example-token' https://example.test";

        assert!(history.record(command));
        assert_eq!(history.records().len(), 1);
        assert_eq!(history.records()[0].command, command);
        assert!(!format!("{:?}", history.records()[0]).contains(command));
    }

    #[test]
    fn shell_history_seed_preserves_recency_order() {
        let history = SharedTerminalCommandHistory::from_commands(vec![
            "docker ps".to_string(),
            "docker images".to_string(),
        ]);
        let candidates = history.candidates(
            &TerminalAutosuggestInputState {
                value: "docker ".to_string(),
                cursor_index: 7,
                is_cursor_at_end: true,
            },
            2,
        );

        assert_eq!(
            candidates
                .into_iter()
                .map(|candidate| candidate.command)
                .collect::<Vec<_>>(),
            ["docker images", "docker ps"]
        );
    }

    #[test]
    fn shared_command_history_projects_the_top_match_as_a_suffix() {
        let history = SharedTerminalCommandHistory::from_commands(vec!["ls -la".to_string()]);

        assert_eq!(
            history.ghost_text(&TerminalAutosuggestInputState {
                value: "ls".to_string(),
                cursor_index: 2,
                is_cursor_at_end: true,
            }),
            Some(" -la".to_string())
        );
    }

    #[test]
    fn command_fact_ledger_limits_literal_query_to_latest_command() {
        let mut ledger = CommandFactLedger::default();
        ledger.create_from_mark(&mark("cmd-1", Some("ps -ef | grep -i dbx"), false));

        let highlight = ledger
            .transient_command_highlight()
            .expect("grep query highlight");
        assert_eq!(highlight.query.as_ref(), "dbx");
        assert!(!highlight.case_sensitive);
        assert_eq!(highlight.output_start_global_line, 11);

        let closed = mark("cmd-1", Some("ps -ef | grep -i dbx"), true);
        ledger.close_from_mark(&closed);
        assert_eq!(
            ledger
                .transient_command_highlight()
                .and_then(|highlight| highlight.output_end_global_line),
            Some(12)
        );

        let mut next = mark("cmd-2", Some("pwd"), false);
        next.start_line = 20;
        next.command_line = 20;
        ledger.create_from_mark(&next);

        assert!(ledger.transient_command_highlight().is_none());
        assert_eq!(
            transient_literal_query("rg needle"),
            Some(TransientLiteralQuery {
                query: "needle".to_string(),
                case_sensitive: true,
            })
        );
        assert!(transient_literal_query("grep 'db.*'").is_none());
    }
}
