// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::{HashSet, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};

use zeroize::{Zeroize, Zeroizing};

use crate::{CompiledTriggerSet, TerminalTriggerDispatch};

const MAX_LOGICAL_LINE_BYTES: usize = 16 * 1024;
const RETAINED_LOGICAL_LINE_BYTES: usize = 8 * 1024;
const MAX_RETAINED_MATCH_RANGES: usize = 512;
const MAX_PENDING_MATCHES: usize = 128;

#[derive(Clone, Copy, Default, Eq, PartialEq)]
enum ControlSequenceState {
    #[default]
    Ground,
    Escape,
    Csi,
    Osc,
    OscEscape,
    StringControl,
    StringControlEscape,
}

#[derive(Clone)]
struct TriggerCapture {
    name: String,
    value: String,
}

impl Zeroize for TriggerCapture {
    fn zeroize(&mut self) {
        self.name.zeroize();
        self.value.zeroize();
    }
}

/// A compact semantic match event. Output-derived values are redacted and zeroized.
#[derive(Clone)]
pub struct TriggerMatched {
    trigger_id: String,
    generation: u64,
    delay_ms: u64,
    full_match: Zeroizing<String>,
    captures: Zeroizing<Vec<TriggerCapture>>,
}

impl TriggerMatched {
    pub fn trigger_id(&self) -> &str {
        &self.trigger_id
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn delay_ms(&self) -> u64 {
        self.delay_ms
    }

    pub fn capture(&self, name: &str) -> Option<&str> {
        if name == "match" {
            return Some(self.full_match.as_str());
        }
        self.captures
            .iter()
            .find(|capture| capture.name == name)
            .map(|capture| capture.value.as_str())
    }
}

impl std::fmt::Debug for TriggerMatched {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TriggerMatched")
            .field("trigger_id", &self.trigger_id)
            .field("generation", &self.generation)
            .field("delay_ms", &self.delay_ms)
            .field("capture_count", &self.captures.len())
            .field("content", &"<redacted>")
            .finish()
    }
}

struct MatchData {
    trigger_index: usize,
    start: usize,
    end: usize,
    full_match: String,
    captures: Vec<TriggerCapture>,
}

/// Per-session mutable state for bounded scanning of already-decoded output.
pub struct TerminalTriggerStream {
    rules: Arc<CompiledTriggerSet>,
    current_line: Zeroizing<String>,
    utf8_pending: Zeroizing<Vec<u8>>,
    retained_line_start: usize,
    scanned_line_bytes: usize,
    control_state: ControlSequenceState,
    pending_carriage_return: bool,
    emitted_ranges: HashSet<(usize, usize, usize)>,
    emitted_range_order: VecDeque<(usize, usize, usize)>,
    last_triggered_at: Vec<Option<Instant>>,
    pending_matches: VecDeque<TriggerMatched>,
}

impl TerminalTriggerStream {
    pub fn new(rules: Arc<CompiledTriggerSet>) -> Self {
        let trigger_count = rules.len();
        Self {
            rules,
            current_line: Zeroizing::new(String::new()),
            utf8_pending: Zeroizing::new(Vec::with_capacity(4)),
            retained_line_start: 0,
            scanned_line_bytes: 0,
            control_state: ControlSequenceState::Ground,
            pending_carriage_return: false,
            emitted_ranges: HashSet::new(),
            emitted_range_order: VecDeque::new(),
            last_triggered_at: vec![None; trigger_count],
            pending_matches: VecDeque::new(),
        }
    }

    /// Replaces immutable rules and drops all prior stream and capture state.
    pub fn replace_rules(&mut self, rules: Arc<CompiledTriggerSet>) {
        *self = Self::new(rules);
    }

    pub fn generation(&self) -> u64 {
        self.rules.generation()
    }

    /// Observes decoded terminal text and calls the sink only for semantic matches.
    pub fn observe(&mut self, output: &str, sink: impl FnMut(TriggerMatched)) {
        self.observe_at(output, Instant::now(), sink);
    }

    /// Observes UTF-8 bytes while retaining an incomplete multibyte suffix across chunks.
    pub fn observe_bytes(&mut self, output: &[u8], sink: impl FnMut(TriggerMatched)) {
        self.observe_bytes_at(output, Instant::now(), sink);
    }

    #[doc(hidden)]
    pub fn observe_bytes_at(
        &mut self,
        output: &[u8],
        now: Instant,
        mut sink: impl FnMut(TriggerMatched),
    ) {
        if self.utf8_pending.is_empty() {
            if let Ok(decoded) = std::str::from_utf8(output) {
                self.observe_at(decoded, now, sink);
                return;
            }
            self.observe_utf8_slice(output, now, &mut sink);
            return;
        }

        // Split code points are rare; only that path joins the at-most-three-byte suffix.
        let mut joined = Zeroizing::new(Vec::with_capacity(self.utf8_pending.len() + output.len()));
        joined.extend_from_slice(&self.utf8_pending);
        joined.extend_from_slice(output);
        self.utf8_pending.clear();
        self.observe_utf8_slice(&joined, now, &mut sink);
    }

    #[doc(hidden)]
    pub fn observe_at(&mut self, output: &str, now: Instant, mut sink: impl FnMut(TriggerMatched)) {
        for character in output.chars() {
            self.observe_character(character, now, &mut sink);
        }
        self.scan_current_line(now, false, &mut sink);
    }

    fn observe_utf8_slice(
        &mut self,
        mut bytes: &[u8],
        now: Instant,
        sink: &mut impl FnMut(TriggerMatched),
    ) {
        while !bytes.is_empty() {
            match std::str::from_utf8(bytes) {
                Ok(decoded) => {
                    self.observe_at(decoded, now, sink);
                    return;
                }
                Err(error) => {
                    let valid_up_to = error.valid_up_to();
                    if valid_up_to > 0 {
                        // SAFETY: `valid_up_to` is guaranteed by `Utf8Error`.
                        let valid = unsafe { std::str::from_utf8_unchecked(&bytes[..valid_up_to]) };
                        self.observe_at(valid, now, &mut *sink);
                    }
                    match error.error_len() {
                        Some(invalid_len) => {
                            // Keep invalid input from joining two otherwise separate matches.
                            self.observe_at("\u{fffd}", now, &mut *sink);
                            bytes = &bytes[valid_up_to + invalid_len..];
                        }
                        None => {
                            self.utf8_pending.extend_from_slice(&bytes[valid_up_to..]);
                            return;
                        }
                    }
                }
            }
        }
    }

    fn observe_character(
        &mut self,
        character: char,
        now: Instant,
        sink: &mut impl FnMut(TriggerMatched),
    ) {
        match self.control_state {
            ControlSequenceState::Escape => {
                self.control_state = match character {
                    '[' => ControlSequenceState::Csi,
                    ']' => ControlSequenceState::Osc,
                    'P' | '_' | '^' => ControlSequenceState::StringControl,
                    _ => ControlSequenceState::Ground,
                };
                return;
            }
            ControlSequenceState::Csi => {
                if ('@'..='~').contains(&character) {
                    self.control_state = ControlSequenceState::Ground;
                }
                return;
            }
            ControlSequenceState::Osc => {
                if matches!(character, '\x07' | '\u{9c}') {
                    self.control_state = ControlSequenceState::Ground;
                } else if character == '\x1b' {
                    self.control_state = ControlSequenceState::OscEscape;
                }
                return;
            }
            ControlSequenceState::OscEscape => {
                self.control_state = if character == '\\' {
                    ControlSequenceState::Ground
                } else {
                    ControlSequenceState::Osc
                };
                return;
            }
            ControlSequenceState::StringControl => {
                if character == '\u{9c}' {
                    self.control_state = ControlSequenceState::Ground;
                } else if character == '\x1b' {
                    self.control_state = ControlSequenceState::StringControlEscape;
                }
                return;
            }
            ControlSequenceState::StringControlEscape => {
                self.control_state = if character == '\\' {
                    ControlSequenceState::Ground
                } else {
                    ControlSequenceState::StringControl
                };
                return;
            }
            ControlSequenceState::Ground => {}
        }

        if character == '\x1b' {
            self.control_state = ControlSequenceState::Escape;
            return;
        }
        if character == '\u{9b}' {
            self.control_state = ControlSequenceState::Csi;
            return;
        }
        if character == '\u{9d}' {
            self.control_state = ControlSequenceState::Osc;
            return;
        }
        if matches!(character, '\u{90}' | '\u{98}' | '\u{9e}' | '\u{9f}') {
            self.control_state = ControlSequenceState::StringControl;
            return;
        }
        if character == '\r' {
            self.scan_current_line(now, true, sink);
            self.pending_carriage_return = true;
            return;
        }
        if character == '\n' {
            self.scan_current_line(now, true, sink);
            self.release_pending_matches(sink);
            self.reset_line();
            self.pending_carriage_return = false;
            return;
        }
        if self.pending_carriage_return {
            // A bare carriage return makes following text overwrite the logical line.
            self.reset_line();
            self.pending_carriage_return = false;
        }
        if character == '\u{8}' {
            self.current_line.pop();
            self.scanned_line_bytes = 0;
            return;
        }
        if character.is_control() {
            return;
        }

        self.current_line.push(character);
        if self.current_line.len() > MAX_LOGICAL_LINE_BYTES {
            self.scan_current_line(now, false, sink);
            self.bound_current_line();
        }
    }

    fn scan_current_line(
        &mut self,
        now: Instant,
        line_terminated: bool,
        sink: &mut impl FnMut(TriggerMatched),
    ) {
        if self.current_line.is_empty() || self.rules.is_empty() {
            self.scanned_line_bytes = self.current_line.len();
            return;
        }

        let matches = self.collect_new_matches(line_terminated);
        self.scanned_line_bytes = self.current_line.len();
        for matched in matches {
            let range = (
                matched.trigger_index,
                self.retained_line_start + matched.start,
                self.retained_line_start + matched.end,
            );
            if !self.emitted_ranges.insert(range) {
                continue;
            }
            self.emitted_range_order.push_back(range);
            self.bound_emitted_ranges();

            let rule = &self.rules.triggers[matched.trigger_index];
            let cooldown = Duration::from_millis(rule.cooldown_ms);
            if self.last_triggered_at[matched.trigger_index]
                .and_then(|last| now.checked_duration_since(last))
                .is_some_and(|elapsed| elapsed < cooldown)
            {
                continue;
            }
            self.last_triggered_at[matched.trigger_index] = Some(now);
            let event = TriggerMatched {
                trigger_id: rule.id.clone(),
                generation: self.rules.generation,
                delay_ms: rule.delay_ms,
                full_match: Zeroizing::new(matched.full_match),
                captures: Zeroizing::new(matched.captures),
            };
            match rule.dispatch {
                TerminalTriggerDispatch::Immediate => sink(event),
                TerminalTriggerDispatch::AfterNextLineBreak => {
                    if self.pending_matches.len() < MAX_PENDING_MATCHES
                        && !self
                            .pending_matches
                            .iter()
                            .any(|pending| pending.trigger_id == event.trigger_id)
                    {
                        self.pending_matches.push_back(event);
                    }
                }
            }
        }
    }

    fn collect_new_matches(&self, line_terminated: bool) -> Vec<MatchData> {
        let candidate_matches = self.rules.candidates.matches(&self.current_line);
        let mut matches = Vec::new();
        for trigger_index in candidate_matches.iter() {
            let rule = &self.rules.triggers[trigger_index];
            for captures in rule.matcher.captures_iter(&self.current_line) {
                let Some(full_match) = captures.get(0) else {
                    continue;
                };
                let range = (
                    trigger_index,
                    self.retained_line_start + full_match.start(),
                    self.retained_line_start + full_match.end(),
                );
                let deferred_whole_word =
                    rule.whole_word && full_match.end() == self.scanned_line_bytes;
                if (full_match.end() <= self.scanned_line_bytes && !deferred_whole_word)
                    || self.emitted_ranges.contains(&range)
                    || !whole_word_matches(
                        &self.current_line,
                        full_match.start(),
                        full_match.end(),
                        line_terminated,
                        rule.whole_word,
                    )
                {
                    continue;
                }
                let capture_values = rule
                    .capture_names
                    .iter()
                    .map(|name| TriggerCapture {
                        name: name.clone(),
                        value: captures
                            .name(name)
                            .map_or_else(String::new, |capture| capture.as_str().to_owned()),
                    })
                    .collect();
                matches.push(MatchData {
                    trigger_index,
                    start: full_match.start(),
                    end: full_match.end(),
                    full_match: full_match.as_str().to_owned(),
                    captures: capture_values,
                });
            }
        }
        matches
    }

    fn release_pending_matches(&mut self, sink: &mut impl FnMut(TriggerMatched)) {
        while let Some(event) = self.pending_matches.pop_front() {
            sink(event);
        }
    }

    fn bound_current_line(&mut self) {
        let mut retained_start = self.current_line.len() - RETAINED_LOGICAL_LINE_BYTES;
        while !self.current_line.is_char_boundary(retained_start) {
            retained_start += 1;
        }
        self.current_line.drain(..retained_start);
        self.retained_line_start += retained_start;
        self.scanned_line_bytes = self.scanned_line_bytes.saturating_sub(retained_start);
        let retained_line_start = self.retained_line_start;
        self.emitted_ranges
            .retain(|(_, _, end)| *end > retained_line_start);
        self.emitted_range_order
            .retain(|range| self.emitted_ranges.contains(range));
    }

    fn bound_emitted_ranges(&mut self) {
        while self.emitted_range_order.len() > MAX_RETAINED_MATCH_RANGES {
            if let Some(range) = self.emitted_range_order.pop_front() {
                self.emitted_ranges.remove(&range);
            }
        }
    }

    fn reset_line(&mut self) {
        self.current_line.clear();
        self.retained_line_start = 0;
        self.scanned_line_bytes = 0;
        self.emitted_ranges.clear();
        self.emitted_range_order.clear();
    }
}

fn whole_word_matches(
    line: &str,
    start: usize,
    end: usize,
    line_terminated: bool,
    whole_word: bool,
) -> bool {
    if !whole_word {
        return true;
    }
    let starts_at_boundary = start == 0
        || line[..start]
            .chars()
            .next_back()
            .is_none_or(|character| !is_word_character(character));
    let ends_at_boundary = if end < line.len() {
        line[end..]
            .chars()
            .next()
            .is_none_or(|character| !is_word_character(character))
    } else {
        line_terminated
    };
    starts_at_boundary && ends_at_boundary
}

fn is_word_character(character: char) -> bool {
    character == '_' || character.is_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ExpandedLocalProcessSpec, ExpandedTriggerAction, LocalProcessSpec,
        TERMINAL_TRIGGERS_SCHEMA_VERSION, TerminalTrigger, TerminalTriggerAction,
        TerminalTriggerMatch, TerminalTriggerMatchMode, TerminalTriggerScope,
        TerminalTriggerTiming, TerminalTriggersSnapshot, compile_active,
    };

    fn rule(id: &str, pattern: &str, mode: TerminalTriggerMatchMode) -> TerminalTrigger {
        TerminalTrigger {
            id: id.to_string(),
            name: id.to_string(),
            description: None,
            enabled: true,
            matcher: TerminalTriggerMatch {
                pattern: pattern.to_string(),
                mode,
                case_sensitive: true,
                whole_word: false,
            },
            action: TerminalTriggerAction::SendText {
                text: "${match}".to_string(),
                append_enter: false,
            },
            timing: TerminalTriggerTiming {
                dispatch: TerminalTriggerDispatch::Immediate,
                delay_ms: 0,
                cooldown_ms: 100,
            },
            scope: TerminalTriggerScope::AllTerminals,
            created_at: 1,
            updated_at: 1,
        }
    }

    fn make_stream(rules: Vec<TerminalTrigger>) -> TerminalTriggerStream {
        let snapshot = TerminalTriggersSnapshot {
            version: TERMINAL_TRIGGERS_SCHEMA_VERSION,
            triggers: rules,
            updated_at: 1,
        };
        TerminalTriggerStream::new(compile_active(&snapshot, 11).unwrap().unwrap())
    }

    fn observe_at(
        stream: &mut TerminalTriggerStream,
        input: &str,
        now: Instant,
    ) -> Vec<TriggerMatched> {
        let mut events = Vec::new();
        stream.observe_at(input, now, |event| events.push(event));
        events
    }

    #[test]
    fn matches_literal_across_every_chunk_split_exactly_once() {
        let input = "prefix READY suffix";
        for split in 0..=input.len() {
            let mut stream = make_stream(vec![rule(
                "literal",
                "READY",
                TerminalTriggerMatchMode::Literal,
            )]);
            let now = Instant::now();
            let mut events = observe_at(&mut stream, &input[..split], now);
            events.extend(observe_at(&mut stream, &input[split..], now));
            events.extend(observe_at(&mut stream, "", now));

            assert_eq!(events.len(), 1, "split {split}");
            assert_eq!(events[0].capture("match"), Some("READY"));
        }
    }

    #[test]
    fn handles_case_insensitive_and_whole_word_matching() {
        let mut case_rule = rule("case", "error", TerminalTriggerMatchMode::Literal);
        case_rule.matcher.case_sensitive = false;
        case_rule.matcher.whole_word = true;
        let mut stream = make_stream(vec![case_rule]);
        let now = Instant::now();

        assert!(observe_at(&mut stream, "ERROR", now).is_empty());
        let events = observe_at(&mut stream, " ", now);
        assert_eq!(events.len(), 1);

        let mut stream = make_stream(vec![{
            let mut rule = rule("word", "error", TerminalTriggerMatchMode::Literal);
            rule.matcher.whole_word = true;
            rule
        }]);
        assert!(observe_at(&mut stream, "errors ", now).is_empty());
    }

    #[test]
    fn preserves_named_captures_and_split_unicode() {
        let mut capture_rule = rule(
            "capture",
            r"用户=(?P<name>\p{Han}{2})",
            TerminalTriggerMatchMode::Regex,
        );
        capture_rule.action = TerminalTriggerAction::SendText {
            text: "hello ${name}".to_string(),
            append_enter: false,
        };
        let mut stream = make_stream(vec![capture_rule]);
        let now = Instant::now();

        assert!(observe_at(&mut stream, "用户=张", now).is_empty());
        let events = observe_at(&mut stream, "三 ", now);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].capture("name"), Some("张三"));
        assert_eq!(events[0].capture("match"), Some("用户=张三"));
    }

    #[test]
    fn retains_utf8_code_points_split_at_every_byte_boundary() {
        let input = "prefix 密码 suffix";
        for split in 0..=input.len() {
            let mut stream = make_stream(vec![rule(
                "utf8",
                "密码",
                TerminalTriggerMatchMode::Literal,
            )]);
            let now = Instant::now();
            let mut events = Vec::new();
            stream.observe_bytes_at(&input.as_bytes()[..split], now, |event| events.push(event));
            stream.observe_bytes_at(&input.as_bytes()[split..], now, |event| events.push(event));

            assert_eq!(events.len(), 1, "byte split {split}");
            assert_eq!(events[0].capture("match"), Some("密码"));
        }
    }

    #[test]
    fn strips_split_ansi_osc_and_string_controls() {
        let mut stream = make_stream(vec![rule(
            "control",
            "ERROR ready",
            TerminalTriggerMatchMode::Literal,
        )]);
        let input = "\x1b[31mER\x1b[1mROR\x1b[0m\x1b]0;ignored\x07 \x1bPignored\x1b\\ready";
        let now = Instant::now();
        let mut events = Vec::new();
        for character in input.chars() {
            let chunk = character.to_string();
            events.extend(observe_at(&mut stream, &chunk, now));
        }

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].capture("match"), Some("ERROR ready"));
    }

    #[test]
    fn applies_carriage_return_backspace_and_line_break_semantics() {
        let mut stream = make_stream(vec![rule(
            "editing",
            "ERROR",
            TerminalTriggerMatchMode::Literal,
        )]);
        let now = Instant::now();

        let events = observe_at(&mut stream, "ERX\u{8}ROR\n", now);
        assert_eq!(events.len(), 1);

        let events = observe_at(
            &mut stream,
            "stale\roverwrite ERROR\n",
            now + Duration::from_millis(100),
        );
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn suppresses_duplicate_ranges_and_enforces_cooldown() {
        let mut stream = make_stream(vec![rule(
            "cooldown",
            "hit",
            TerminalTriggerMatchMode::Literal,
        )]);
        let now = Instant::now();

        assert_eq!(observe_at(&mut stream, "hit", now).len(), 1);
        assert!(observe_at(&mut stream, "", now).is_empty());
        assert!(observe_at(&mut stream, "\nhit\n", now).is_empty());
        assert_eq!(
            observe_at(&mut stream, "hit\n", now + Duration::from_millis(100)).len(),
            1
        );
    }

    #[test]
    fn releases_after_next_line_break_with_configured_delay() {
        let mut delayed = rule("delayed", "ready", TerminalTriggerMatchMode::Literal);
        delayed.timing.dispatch = TerminalTriggerDispatch::AfterNextLineBreak;
        delayed.timing.delay_ms = 250;
        let mut stream = make_stream(vec![delayed]);
        let now = Instant::now();

        assert!(observe_at(&mut stream, "ready", now).is_empty());
        let events = observe_at(&mut stream, "\n", now);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].delay_ms(), 250);
        assert_eq!(events[0].generation(), 11);
    }

    #[test]
    fn keeps_long_lines_bounded_and_matches_the_retained_tail() {
        let mut stream = make_stream(vec![rule(
            "bounded",
            "TAIL-MATCH",
            TerminalTriggerMatchMode::Literal,
        )]);
        let mut input = "x".repeat(MAX_LOGICAL_LINE_BYTES * 3);
        input.push_str("TAIL-MATCH");

        let events = observe_at(&mut stream, &input, Instant::now());
        assert_eq!(events.len(), 1);
        assert!(stream.current_line.len() <= MAX_LOGICAL_LINE_BYTES);
    }

    #[test]
    fn matches_multiple_candidate_rules() {
        let mut stream = make_stream(vec![
            rule("literal", "alpha", TerminalTriggerMatchMode::Literal),
            rule("regex", r"beta-\d+", TerminalTriggerMatchMode::Regex),
        ]);

        let events = observe_at(&mut stream, "alpha beta-42", Instant::now());
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].trigger_id(), "literal");
        assert_eq!(events[1].trigger_id(), "regex");
    }

    #[test]
    fn debug_output_redacts_terminal_content() {
        let mut stream = make_stream(vec![rule(
            "redacted",
            "token-value",
            TerminalTriggerMatchMode::Literal,
        )]);
        let events = observe_at(&mut stream, "token-value", Instant::now());
        let debug = format!("{:?}", events[0]);

        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("token-value"));
    }

    #[test]
    fn expands_capture_into_one_direct_process_argument() {
        let mut process_rule = rule(
            "process",
            r"value=(?P<value>[^\r\n]+)",
            TerminalTriggerMatchMode::Regex,
        );
        process_rule.action = TerminalTriggerAction::LaunchLocalProcess {
            process: LocalProcessSpec::DirectProgram {
                executable: "/usr/bin/printf".to_string(),
                arguments: vec!["%s".to_string(), "${value}".to_string()],
                working_directory: None,
            },
        };
        let action = process_rule.action.clone();
        let mut stream = make_stream(vec![process_rule]);
        let events = observe_at(&mut stream, "value=hello; rm -rf ignored\n", Instant::now());

        let ExpandedTriggerAction::LaunchLocalProcess { process } =
            action.expand(&events[0]).unwrap()
        else {
            unreachable!();
        };
        let ExpandedLocalProcessSpec::DirectProgram { arguments, .. } = process else {
            unreachable!();
        };
        assert_eq!(arguments.len(), 2);
        assert_eq!(arguments[1].as_str(), "hello; rm -rf ignored");
    }
}
