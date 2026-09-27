use std::{
    collections::HashMap,
    fmt,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use oxideterm_terminal::{
    TerminalCommandMark, TerminalCommandMarkClosedBy, TerminalCommandMarkConfidence,
    TerminalCommandMarkDetectionSource, terminal_autosuggest_fuzzy_score,
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
    audit: Option<oxideterm_audit::AuditContext>,
    audit_commands: HashMap<String, oxideterm_audit::AuditOperation>,
    facts: Vec<TerminalCommandFact>,
    ai_records: Vec<TerminalAiCommandRecord>,
    autosuggest_records: Vec<TerminalAutosuggestCommandRecord>,
    transient_command_highlight: Option<TransientCommandHighlight>,
}

impl CommandFactLedger {
    pub(crate) fn set_audit_context(&mut self, context: Option<oxideterm_audit::AuditContext>) {
        // In-flight guards retain their original connection snapshot.
        self.audit = context;
    }

    pub(crate) fn with_audit(context: Option<oxideterm_audit::AuditContext>) -> Self {
        Self {
            audit: context,
            ..Self::default()
        }
    }

    pub(crate) fn audit_enabled(&self) -> bool {
        self.audit.is_some()
    }

    pub(crate) fn record_dispatch(
        &self,
        command_id: Option<&str>,
        parent_id: Option<&str>,
        source: oxideterm_audit::AuditSource,
        sent: bool,
    ) {
        let Some(mut context) = self.audit.clone() else {
            return;
        };
        let request = oxideterm_audit::AuditContext::current_request();
        if let Some(request) = &request {
            context.agent_id = request.agent_id.clone();
            if context.parent_id.is_none() {
                context.parent_id = request.parent_id.clone();
            }
        }
        let request_source = request.map(|request| request.source);
        context.source = request_source
            .filter(|source| {
                matches!(
                    source,
                    oxideterm_audit::AuditSource::Ai
                        | oxideterm_audit::AuditSource::Mcp
                        | oxideterm_audit::AuditSource::Plugin
                        | oxideterm_audit::AuditSource::Cli
                )
            })
            .unwrap_or(source);
        let inherited_parent = context.parent_id.clone();
        context.parent_id = command_id
            .and_then(|id| self.audit_commands.get(id))
            .and_then(oxideterm_audit::AuditOperation::id)
            .map(str::to_string)
            .or_else(|| parent_id.map(str::to_string))
            .or(inherited_parent);
        context
            .operation(
                oxideterm_audit::AuditCategory::Automation,
                "command_dispatch",
                None,
            )
            .finish(
                if sent {
                    oxideterm_audit::AuditOutcome::Sent
                } else {
                    oxideterm_audit::AuditOutcome::Failed
                },
                oxideterm_audit::AuditEvidence::Dispatch,
                None,
                None,
            );
    }

    pub(crate) fn disable_audit(&mut self) {
        self.audit = None;
        self.audit_commands.clear();
    }

    pub(crate) fn interrupt_audit_commands(&mut self) {
        self.audit_commands.clear();
    }

    pub(crate) fn facts(&self) -> Vec<TerminalCommandFact> {
        self.facts.clone()
    }

    pub(crate) fn ai_records(&self) -> Vec<TerminalAiCommandRecord> {
        self.ai_records.clone()
    }

    pub(crate) fn ai_command_status(&self, id: &str) -> Option<TerminalCommandFactStatus> {
        self.ai_records
            .iter()
            .rev()
            .find(|record| record.command_id == id)
            .map(|record| record.status)
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
        if let Some(context) = &self.audit {
            let mut context = context.clone();
            let request = oxideterm_audit::AuditContext::current_request();
            if let Some(request) = &request {
                context.agent_id = request.agent_id.clone();
                if context.parent_id.is_none() {
                    context.parent_id = request.parent_id.clone();
                }
            }
            let request_source = request.map(|request| request.source);
            context.source = request_source
                .filter(|source| {
                    matches!(
                        source,
                        oxideterm_audit::AuditSource::Ai
                            | oxideterm_audit::AuditSource::Mcp
                            | oxideterm_audit::AuditSource::Plugin
                            | oxideterm_audit::AuditSource::Cli
                    )
                })
                .unwrap_or(match mark.submitted_by.unwrap_or(mark.detection_source) {
                    TerminalCommandMarkDetectionSource::Ai => oxideterm_audit::AuditSource::Ai,
                    TerminalCommandMarkDetectionSource::Broadcast => {
                        oxideterm_audit::AuditSource::Broadcast
                    }
                    TerminalCommandMarkDetectionSource::CommandBar => {
                        oxideterm_audit::AuditSource::CommandBar
                    }
                    TerminalCommandMarkDetectionSource::QuickCommand => {
                        oxideterm_audit::AuditSource::QuickCommand
                    }
                    _ => oxideterm_audit::AuditSource::User,
                });
            self.audit_commands.insert(
                mark.command_id.clone(),
                context.operation(
                    oxideterm_audit::AuditCategory::Command,
                    "command_execute",
                    mark.command.as_deref(),
                ),
            );
        }
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
                output_start_global_line: mark.output_start_line(),
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
            output_start_global_line: mark.output_start_line(),
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

    pub(crate) fn trim_history(&mut self, lines: usize) {
        // Closed facts are historical metadata, not live grid annotations. Only
        // the latest open fact needs rebasing before its closing boundary arrives.
        if let Some(fact) = self.facts.last_mut()
            && fact.status == TerminalCommandFactStatus::Open
        {
            fact.start_global_line = fact.start_global_line.saturating_sub(lines);
            fact.command_global_line = fact.command_global_line.saturating_sub(lines);
            fact.output_start_global_line = fact.output_start_global_line.saturating_sub(lines);
            fact.end_global_line = fact.end_global_line.map(|end| end.saturating_sub(lines));
        }
        if let Some(highlight) = &mut self.transient_command_highlight {
            if highlight
                .output_end_global_line
                .is_some_and(|end| end < lines)
            {
                self.transient_command_highlight = None;
            } else {
                highlight.output_start_global_line =
                    highlight.output_start_global_line.saturating_sub(lines);
                highlight.output_end_global_line = highlight
                    .output_end_global_line
                    .map(|end| end.saturating_sub(lines));
            }
        }
    }

    pub(crate) fn close_from_mark(&mut self, mark: &TerminalCommandMark) {
        if let Some(operation) = self.audit_commands.remove(&mark.command_id) {
            use oxideterm_audit::{AuditEvidence, AuditOutcome};
            let evidence = if mark.closed_by == Some(TerminalCommandMarkClosedBy::ShellIntegration)
            {
                AuditEvidence::ShellIntegration
            } else {
                AuditEvidence::InputInference
            };
            let outcome = if evidence == AuditEvidence::ShellIntegration {
                match mark.exit_code {
                    Some(0) => AuditOutcome::Succeeded,
                    Some(_) => AuditOutcome::Failed,
                    None => AuditOutcome::Unknown,
                }
            } else {
                AuditOutcome::Unknown
            };
            operation.finish(outcome, evidence, mark.exit_code, None);
        }
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
        for (_, operation) in self.audit_commands.drain() {
            operation.finish(
                oxideterm_audit::AuditOutcome::Unknown,
                oxideterm_audit::AuditEvidence::InputInference,
                None,
                None,
            );
        }
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

    let mut candidates_by_command = HashMap::<&str, (f64, TerminalAutosuggestCandidate)>::new();
    for record in records {
        if record.command == query {
            continue;
        }
        let candidate = match candidates_by_command.entry(&record.command) {
            std::collections::hash_map::Entry::Occupied(entry) => &mut entry.into_mut().1,
            std::collections::hash_map::Entry::Vacant(entry) => {
                let score = terminal_autosuggest_fuzzy_score(&record.command, query);
                if score <= 0.0 {
                    continue;
                }
                &mut entry
                    .insert((
                        score,
                        TerminalAutosuggestCandidate {
                            command: record.command.clone(),
                            use_count: 0,
                            last_used_at: record.finished_at,
                        },
                    ))
                    .1
            }
        };
        candidate.use_count = candidate.use_count.saturating_add(1);
        candidate.last_used_at = candidate.last_used_at.max(record.finished_at);
    }

    let mut candidates = candidates_by_command.into_values().collect::<Vec<_>>();
    candidates.sort_by(|(left_score, left), (right_score, right)| {
        right_score
            .total_cmp(left_score)
            .then_with(|| right.use_count.cmp(&left.use_count))
            .then_with(|| right.last_used_at.cmp(&left.last_used_at))
            .then_with(|| left.command.cmp(&right.command))
    });
    candidates.truncate(limit);
    candidates
        .into_iter()
        .map(|(_, candidate)| candidate)
        .collect()
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

    struct Keys;
    impl oxideterm_audit::AuditKeyProvider for Keys {
        fn load(
            &self,
            _: &str,
        ) -> Result<zeroize::Zeroizing<Vec<u8>>, oxideterm_audit::AuditError> {
            Ok(zeroize::Zeroizing::new(vec![7; 32]))
        }
        fn create(
            &self,
            id: &str,
        ) -> Result<zeroize::Zeroizing<Vec<u8>>, oxideterm_audit::AuditError> {
            self.load(id)
        }
    }

    fn mark(command_id: &str, command: Option<&str>, closed: bool) -> TerminalCommandMark {
        TerminalCommandMark {
            command_id: command_id.to_string(),
            command: command.map(str::to_string),
            start_line: 10,
            command_line: 10,
            command_line_clipped: false,
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
    fn quick_command_dispatch_and_shell_result_share_one_command_identity() {
        use oxideterm_audit::*;

        let directory = tempfile::tempdir().unwrap();
        oxideterm_audit::AuditStore::open(&directory.path().join("audit.db"), &Keys)
            .unwrap()
            .set_policy(oxideterm_audit::AuditPolicy {
                enabled: true,
                ..Default::default()
            })
            .unwrap();
        let service =
            AuditService::with_key_provider(directory.path().join("audit.db"), Keys).unwrap();
        let mut context = AuditContext::new(service.client(), AuditSource::User)
            .session("ssh", "operator@example.invalid:22")
            .consumer();
        context.parent_id = Some("quick-batch-1".to_string());
        let mut ledger = CommandFactLedger::with_audit(Some(context));
        let mut started = mark("quick-target-1", Some("printf done"), false);
        started.detection_source = TerminalCommandMarkDetectionSource::QuickCommand;
        ledger.create_from_mark(&started);
        ledger.record_dispatch(
            Some(&started.command_id),
            Some("quick-batch-1"),
            AuditSource::QuickCommand,
            true,
        );
        let mut closed = mark("quick-target-1", Some("printf done"), true);
        closed.submitted_by = Some(TerminalCommandMarkDetectionSource::QuickCommand);
        ledger.close_from_mark(&closed);

        let page = futures::executor::block_on(service.client().query(AuditQuery {
            limit: 20,
            ..Default::default()
        }))
        .unwrap();
        let command_records = page
            .records
            .iter()
            .filter(|record| record.category == AuditCategory::Command)
            .map(|record| record.details.operation.as_ref().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(command_records.len(), 2);
        assert_eq!(command_records[0].id, command_records[1].id);
        assert!(
            command_records
                .iter()
                .all(|op| op.parent_id.as_deref() == Some("quick-batch-1")
                    && op.source == AuditSource::QuickCommand)
        );
        assert!(
            command_records
                .iter()
                .any(|op| op.outcome == AuditOutcome::Succeeded
                    && op.evidence == AuditEvidence::ShellIntegration)
        );
        let dispatch = page
            .records
            .iter()
            .filter(|record| record.category == AuditCategory::Automation)
            .find_map(|record| {
                record
                    .details
                    .operation
                    .as_ref()
                    .filter(|op| op.action == "command_dispatch")
            })
            .unwrap();
        assert_eq!(
            dispatch.parent_id.as_deref(),
            Some(command_records[0].id.as_str())
        );
        assert_eq!(dispatch.outcome, AuditOutcome::Sent);
    }

    #[test]
    fn audit_keeps_started_command_owner_when_the_terminal_context_changes() {
        use oxideterm_audit::*;
        let directory = tempfile::tempdir().unwrap();
        oxideterm_audit::AuditStore::open(&directory.path().join("audit.db"), &Keys)
            .unwrap()
            .set_policy(oxideterm_audit::AuditPolicy {
                enabled: true,
                ..Default::default()
            })
            .unwrap();
        let service =
            AuditService::with_key_provider(directory.path().join("audit.db"), Keys).unwrap();
        let base = AuditContext::new(service.client(), AuditSource::User);
        let old = base.session("ssh", "alice@old.example:22").consumer();
        let new = base.session("ssh", "bob@new.example:2222").consumer();
        let mut ledger = CommandFactLedger::with_audit(Some(old.clone()));
        ledger.create_from_mark(&mark("old", Some("pwd"), false));
        ledger.set_audit_context(Some(new.clone()));
        ledger.close_from_mark(&mark("old", Some("pwd"), true));
        ledger.create_from_mark(&mark("new", Some("date"), false));
        ledger.close_from_mark(&mark("new", Some("date"), true));
        let page = futures::executor::block_on(service.client().query(AuditQuery {
            limit: 20,
            ..Default::default()
        }))
        .unwrap();
        let actual = page
            .records
            .iter()
            .map(|record| {
                let op = record.details.operation.as_ref().unwrap();
                (
                    record.details.target.as_ref().unwrap().as_str(),
                    op.session_id.as_deref(),
                    op.consumer_id.as_deref(),
                    op.outcome,
                    op.exit_code,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            [
                (
                    "bob@new.example:2222",
                    new.session_id.as_deref(),
                    new.consumer_id.as_deref(),
                    AuditOutcome::Succeeded,
                    Some(0)
                ),
                (
                    "bob@new.example:2222",
                    new.session_id.as_deref(),
                    new.consumer_id.as_deref(),
                    AuditOutcome::Started,
                    None
                ),
                (
                    "alice@old.example:22",
                    old.session_id.as_deref(),
                    old.consumer_id.as_deref(),
                    AuditOutcome::Succeeded,
                    Some(0)
                ),
                (
                    "alice@old.example:22",
                    old.session_id.as_deref(),
                    old.consumer_id.as_deref(),
                    AuditOutcome::Started,
                    None
                ),
            ]
        );
    }

    #[test]
    fn audit_reuses_command_identity_and_does_not_infer_success_from_next_prompt() {
        use oxideterm_audit::*;
        let directory = tempfile::tempdir().unwrap();
        oxideterm_audit::AuditStore::open(&directory.path().join("audit.db"), &Keys)
            .unwrap()
            .set_policy(oxideterm_audit::AuditPolicy {
                enabled: true,
                ..Default::default()
            })
            .unwrap();
        let service =
            AuditService::with_key_provider(directory.path().join("audit.db"), Keys).unwrap();
        let mut context = AuditContext::new(service.client(), AuditSource::User);
        context.session_id = Some("terminal-session".into());
        let mut ledger = CommandFactLedger::with_audit(Some(context));
        ledger.create_from_mark(&mark("cmd-unknown", None, false));
        ledger.close_from_mark(&mark("cmd-unknown", None, true));
        let first = mark("cmd-1", Some("pwd"), false);
        ledger.create_from_mark(&first);
        ledger.create_from_mark(&first);
        let mut closed = mark("cmd-1", Some("pwd"), true);
        closed.exit_code = Some(2);
        ledger.close_from_mark(&closed);
        ledger.close_from_mark(&closed);
        ledger.create_from_mark(&mark("cmd-2", Some("ls"), false));
        ledger.create_from_mark(&mark("cmd-3", Some("date"), false));
        drop(ledger);
        let page = futures::executor::block_on(service.client().query(AuditQuery {
            limit: 20,
            ..Default::default()
        }))
        .unwrap();
        let observations = page
            .records
            .iter()
            .map(|record| {
                let operation = record.details.operation.as_ref().unwrap();
                (
                    record.details.detail.as_ref().map(|value| value.as_str()),
                    operation.outcome,
                    operation.exit_code,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            observations,
            [
                (Some("date"), AuditOutcome::Unknown, None),
                (Some("date"), AuditOutcome::Started, None),
                (Some("ls"), AuditOutcome::Unknown, None),
                (Some("ls"), AuditOutcome::Started, None),
                (Some("pwd"), AuditOutcome::Failed, Some(2)),
                (Some("pwd"), AuditOutcome::Started, None),
                (None, AuditOutcome::Succeeded, Some(0)),
                (None, AuditOutcome::Started, None),
            ]
        );
        assert_eq!(
            page.records[4].details.operation.as_ref().unwrap().id,
            page.records[5].details.operation.as_ref().unwrap().id
        );
    }

    #[test]
    fn closed_facts_keep_boundaries_but_only_nonblank_commands_enter_ai_history() {
        for (command, expected) in [("ls", vec!["ls"]), ("  ", vec![])] {
            let mut ledger = CommandFactLedger::default();
            ledger.create_from_mark(&mark("cmd-1", Some(command), false));
            ledger.close_from_mark(&mark("cmd-1", Some(command), true));
            let facts = ledger.facts();
            assert_eq!(facts.len(), 1);
            assert_eq!(facts[0].status, TerminalCommandFactStatus::Closed);
            assert_eq!(facts[0].end_global_line, Some(12));
            let records = ledger.ai_records();
            assert_eq!(
                records
                    .iter()
                    .map(|record| record.command.as_str())
                    .collect::<Vec<_>>(),
                expected
            );
            for record in records {
                assert_eq!(record.status, TerminalCommandFactStatus::Closed);
            }
        }
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
    fn suggestions_rank_match_quality_before_activity_and_deduplicate_commands() {
        let history = SharedTerminalCommandHistory::from_commands(vec![
            "git status".into(),
            "git status".into(),
            "echo gts".into(),
            "GTS-cache".into(),
            "gts-tool".into(),
            "docker ps".into(),
        ]);
        let state = TerminalAutosuggestInputState {
            value: "gts".into(),
            cursor_index: 3,
            is_cursor_at_end: true,
        };
        let candidates = history.candidates(&state, 8);
        assert_eq!(
            candidates
                .iter()
                .map(|item| item.command.as_str())
                .collect::<Vec<_>>(),
            ["gts-tool", "GTS-cache", "echo gts", "git status"]
        );
        assert_eq!(candidates[3].use_count, 2);
        assert_eq!(
            history
                .candidates(
                    &TerminalAutosuggestInputState {
                        value: "git status".into(),
                        cursor_index: 10,
                        is_cursor_at_end: true,
                    },
                    8
                )
                .iter()
                .map(|item| item.command.as_str())
                .collect::<Vec<_>>(),
            Vec::<&str>::new()
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
    fn history_eviction_keeps_live_query_on_retained_output_only() {
        let mut ledger = CommandFactLedger::default();
        let mut command = mark("grep", Some("ps -ef | grep dbx"), false);
        ledger.create_from_mark(&command);
        ledger.trim_history(11);
        assert!(command.trim_history(11));
        assert_eq!(
            ledger
                .transient_command_highlight()
                .unwrap()
                .output_start_global_line,
            0
        );
        command.is_closed = true;
        command.end_line = Some(1);
        ledger.close_from_mark(&command);
        assert_eq!(
            ledger
                .transient_command_highlight()
                .unwrap()
                .output_end_global_line,
            Some(1)
        );
        ledger.trim_history(2);
        assert!(ledger.transient_command_highlight().is_none());
        assert_eq!(
            ledger.facts()[0].command.as_deref(),
            Some("ps -ef | grep dbx")
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
