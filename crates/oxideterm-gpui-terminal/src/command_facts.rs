use std::time::{SystemTime, UNIX_EPOCH};

use oxideterm_terminal::{
    TerminalCommandMark, TerminalCommandMarkClosedBy, TerminalCommandMarkConfidence,
    TerminalCommandMarkDetectionSource,
};

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalAutosuggestCommandRecord {
    pub command_id: String,
    pub command: String,
    pub started_at: u64,
    pub finished_at: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalAutosuggestInputState {
    pub value: String,
    pub cursor_index: usize,
    pub is_cursor_at_end: bool,
}

#[derive(Default)]
pub(crate) struct CommandFactLedger {
    facts: Vec<TerminalCommandFact>,
    ai_records: Vec<TerminalAiCommandRecord>,
    autosuggest_records: Vec<TerminalAutosuggestCommandRecord>,
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

    pub(crate) fn record_runtime_autosuggest_command(&mut self, command: &str) {
        let command = command.trim();
        if command.is_empty() {
            return;
        }
        let normalized = command.split_whitespace().collect::<Vec<_>>().join(" ");
        if normalized.is_empty()
            || self
                .autosuggest_records
                .last()
                .is_some_and(|record| record.command == normalized)
        {
            return;
        }
        let now = now_millis();
        self.autosuggest_records
            .push(TerminalAutosuggestCommandRecord {
                command_id: format!("runtime-autosuggest-{now}"),
                command: normalized,
                started_at: now,
                finished_at: now,
            });
        const MAX_AUTOSUGGEST_RECORDS: usize = 1000;
        if self.autosuggest_records.len() > MAX_AUTOSUGGEST_RECORDS {
            let overflow = self.autosuggest_records.len() - MAX_AUTOSUGGEST_RECORDS;
            self.autosuggest_records.drain(0..overflow);
        }
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
        self.facts.push(TerminalCommandFact {
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
        });
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
        if self
            .ai_records
            .iter()
            .any(|record| record.command_id == mark.command_id)
        {
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
    fn command_fact_ledger_records_runtime_autosuggest_independently() {
        let mut ledger = CommandFactLedger::default();

        ledger.record_runtime_autosuggest_command("  git   status  ");
        ledger.record_runtime_autosuggest_command("git status");
        ledger.record_runtime_autosuggest_command(" ");

        let records = ledger.autosuggest_records();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].command, "git status");
    }

    #[test]
    fn command_fact_ledger_exposes_prefix_autosuggest_ghost_text() {
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
}
