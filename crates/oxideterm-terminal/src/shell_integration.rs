use std::time::{SystemTime, UNIX_EPOCH};

use alacritty_terminal::{
    event::EventListener,
    grid::Dimensions,
    index::Line,
    term::{Term, cell::Flags},
    vte::ansi::Processor,
};

const MAX_COMMAND_TEXT_LENGTH: usize = 4096;
const MAX_MARKS: usize = 2000;
const OSC_LIMIT: usize = MAX_COMMAND_TEXT_LENGTH * 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellIntegrationSource {
    Osc133,
    Osc633,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellIntegrationLifecycleState {
    Idle,
    Prompt,
    Command,
    Output,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellIntegrationEventKind {
    PromptStart,
    CommandStart,
    OutputStart,
    CommandEnd,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellIntegrationEvent {
    pub kind: ShellIntegrationEventKind,
    pub source: ShellIntegrationSource,
    pub line: usize,
    pub col: usize,
    pub sequence: String,
    pub raw: String,
    pub command: Option<String>,
    pub exit_code: Option<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalCommandMarkDetectionSource {
    CommandBar,
    Ai,
    Broadcast,
    UserInputObserved,
    Heuristic,
    ShellIntegration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalCommandMarkClosedBy {
    NextCommand,
    ShellIntegration,
    TerminalReset,
    SessionLost,
    InterruptedMode,
    Timeout,
    Manual,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalCommandMarkConfidence {
    High,
    Medium,
    Low,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalCommandMark {
    pub command_id: String,
    pub command: Option<String>,
    pub start_line: usize,
    pub command_line: usize,
    pub end_line: Option<usize>,
    pub is_closed: bool,
    pub closed_by: Option<TerminalCommandMarkClosedBy>,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<u64>,
    pub detection_source: TerminalCommandMarkDetectionSource,
    pub submitted_by: Option<TerminalCommandMarkDetectionSource>,
    pub confidence: TerminalCommandMarkConfidence,
    pub output_confidence: TerminalCommandMarkConfidence,
    pub stale: bool,
    pub started_at: u64,
    pub finished_at: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalCommandMarkEvent {
    Created(TerminalCommandMark),
    Closed(TerminalCommandMark),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellIntegrationStatus {
    pub detected: bool,
    pub state: ShellIntegrationLifecycleState,
    pub integration_source: Option<ShellIntegrationSource>,
    pub last_seen_at: Option<u64>,
}

#[derive(Clone, Debug)]
struct CursorPosition {
    line: usize,
    at: u64,
}

#[derive(Clone, Debug)]
struct ShellIntegrationState {
    lifecycle: ShellIntegrationLifecycleState,
    integration_source: Option<ShellIntegrationSource>,
    last_seen_at: Option<u64>,
    prompt_start: Option<CursorPosition>,
    command_start: Option<CursorPosition>,
    pending_command_text: Option<String>,
    pending_command_text_from_protocol: bool,
    active_command_id: Option<String>,
    active_start_line: Option<usize>,
    started_at: Option<u64>,
}

impl Default for ShellIntegrationState {
    fn default() -> Self {
        Self {
            lifecycle: ShellIntegrationLifecycleState::Idle,
            integration_source: None,
            last_seen_at: None,
            prompt_start: None,
            command_start: None,
            pending_command_text: None,
            pending_command_text_from_protocol: false,
            active_command_id: None,
            active_start_line: None,
            started_at: None,
        }
    }
}

#[derive(Default)]
struct OscCapture {
    raw: Vec<u8>,
    payload: Vec<u8>,
}

#[derive(Default)]
pub(crate) struct TerminalShellIntegration {
    state: ShellIntegrationState,
    marks: Vec<TerminalCommandMark>,
    pending_osc: Option<OscCapture>,
    next_command_sequence: u64,
}

impl TerminalShellIntegration {
    pub(crate) fn advance<T: EventListener>(
        &mut self,
        parser: &mut Processor,
        term: &mut Term<T>,
        bytes: &[u8],
        mut emit: impl FnMut(crate::TerminalEvent),
    ) -> bool {
        let mut changed = false;
        let mut index = 0usize;
        let mut normal_start = 0usize;

        while index < bytes.len() {
            if self.pending_osc.is_some() {
                if normal_start < index {
                    parser.advance(term, &bytes[normal_start..index]);
                }
                let consumed = self.continue_osc_capture(term, parser, &bytes[index..], &mut emit);
                changed = true;
                index += consumed;
                normal_start = index;
                continue;
            }

            if bytes[index] == 0x1b && bytes.get(index + 1) == Some(&b']') {
                if normal_start < index {
                    parser.advance(term, &bytes[normal_start..index]);
                }
                self.pending_osc = Some(OscCapture {
                    raw: vec![0x1b, b']'],
                    payload: Vec::new(),
                });
                index += 2;
                normal_start = index;
                changed = true;
                continue;
            }

            index += 1;
        }

        if normal_start < bytes.len() {
            parser.advance(term, &bytes[normal_start..]);
        }

        changed
    }

    #[cfg(test)]
    pub(crate) fn status(&self) -> ShellIntegrationStatus {
        ShellIntegrationStatus {
            detected: self.state.integration_source.is_some() && self.state.last_seen_at.is_some(),
            state: self.state.lifecycle,
            integration_source: self.state.integration_source,
            last_seen_at: self.state.last_seen_at,
        }
    }

    #[cfg(test)]
    pub(crate) fn command_marks(&self) -> Vec<TerminalCommandMark> {
        self.marks.clone()
    }

    fn continue_osc_capture<T: EventListener>(
        &mut self,
        term: &mut Term<T>,
        parser: &mut Processor,
        bytes: &[u8],
        emit: &mut impl FnMut(crate::TerminalEvent),
    ) -> usize {
        let Some(capture) = self.pending_osc.as_mut() else {
            return 0;
        };

        let mut index = 0usize;
        while index < bytes.len() {
            let byte = bytes[index];
            if byte == 0x07 {
                capture.raw.push(byte);
                index += 1;
                break;
            }
            if byte == 0x1b && bytes.get(index + 1) == Some(&b'\\') {
                capture.raw.extend_from_slice(&[0x1b, b'\\']);
                index += 2;
                break;
            }

            capture.raw.push(byte);
            if capture.payload.len() < OSC_LIMIT {
                capture.payload.push(byte);
            }
            index += 1;
        }

        let complete = self.pending_osc.as_ref().is_some_and(|capture| {
            capture.raw.ends_with(&[0x07]) || capture.raw.ends_with(&[0x1b, b'\\'])
        });
        if complete {
            if let Some(capture) = self.pending_osc.take() {
                if !self.handle_osc_payload(term, &capture.payload, emit) {
                    parser.advance(term, &capture.raw);
                }
            }
        } else if self
            .pending_osc
            .as_ref()
            .is_some_and(|capture| capture.raw.len() > OSC_LIMIT + 8)
            && let Some(capture) = self.pending_osc.take()
        {
            parser.advance(term, &capture.raw);
        }

        index
    }

    fn handle_osc_payload<T: EventListener>(
        &mut self,
        term: &Term<T>,
        payload: &[u8],
        emit: &mut impl FnMut(crate::TerminalEvent),
    ) -> bool {
        let text = String::from_utf8_lossy(payload);
        let Some((code, data)) = text.split_once(';') else {
            return false;
        };
        if code == "7" {
            if let Some((cwd, host)) = parse_osc7_cwd(data) {
                emit(crate::TerminalEvent::CwdChanged { cwd, host });
                return true;
            }
            return false;
        }
        if code == "633"
            && let Some((cwd, host)) = parse_osc633_cwd(data)
        {
            emit(crate::TerminalEvent::CwdChanged { cwd, host });
            return true;
        }
        if code == "1337"
            && let Some((cwd, host)) = parse_osc1337_cwd(data)
        {
            emit(crate::TerminalEvent::CwdChanged { cwd, host });
            return true;
        }
        let source = match code {
            "133" => ShellIntegrationSource::Osc133,
            "633" => ShellIntegrationSource::Osc633,
            _ => return false,
        };
        let Some(event) = parse_shell_integration_event(source, data, cursor_position(term)) else {
            return true;
        };

        for command_event in self.handle_shell_event(term, &event) {
            emit(crate::TerminalEvent::CommandMark(command_event));
        }
        emit(crate::TerminalEvent::ShellIntegration(event));
        true
    }

    fn handle_shell_event<T: EventListener>(
        &mut self,
        term: &Term<T>,
        event: &ShellIntegrationEvent,
    ) -> Vec<TerminalCommandMarkEvent> {
        let mut command_events = Vec::new();
        let previous = self.state.lifecycle;
        self.state.lifecycle = match event.kind {
            ShellIntegrationEventKind::PromptStart => ShellIntegrationLifecycleState::Prompt,
            ShellIntegrationEventKind::CommandStart => ShellIntegrationLifecycleState::Command,
            ShellIntegrationEventKind::OutputStart => ShellIntegrationLifecycleState::Output,
            ShellIntegrationEventKind::CommandEnd => ShellIntegrationLifecycleState::Closed,
        };
        self.state.integration_source = Some(event.source);
        self.state.last_seen_at = Some(now_millis());

        match event.kind {
            ShellIntegrationEventKind::PromptStart => {
                let prompt_start = prompt_block_start_line(term, event.line);
                if let Some(closed) = self.close_active_mark_before(
                    prompt_start,
                    TerminalCommandMarkClosedBy::NextCommand,
                    None,
                ) {
                    command_events.push(TerminalCommandMarkEvent::Closed(closed));
                }
                self.state.prompt_start = Some(CursorPosition {
                    line: prompt_start,
                    at: now_millis(),
                });
                self.state.command_start = None;
                self.state.pending_command_text = None;
                self.state.pending_command_text_from_protocol = false;
                self.state.started_at = None;
            }
            ShellIntegrationEventKind::CommandStart => {
                let prompt_start = prompt_block_start_line(term, event.line);
                if let Some(closed) = self.close_active_mark_before(
                    prompt_start,
                    TerminalCommandMarkClosedBy::NextCommand,
                    None,
                ) {
                    command_events.push(TerminalCommandMarkEvent::Closed(closed));
                }
                if previous != ShellIntegrationLifecycleState::Prompt {
                    self.state.prompt_start = Some(CursorPosition {
                        line: prompt_start,
                        at: now_millis(),
                    });
                }
                self.state.command_start = Some(CursorPosition {
                    line: event.line,
                    at: now_millis(),
                });
                self.state.pending_command_text = None;
                self.state.pending_command_text_from_protocol = false;
                self.state.started_at = Some(now_millis());
            }
            ShellIntegrationEventKind::OutputStart => {
                if event.command.is_some() {
                    self.state.pending_command_text = event.command.clone();
                    self.state.pending_command_text_from_protocol = true;
                }
                if self.state.active_command_id.is_some() {
                    return command_events;
                }

                let start_line = self
                    .state
                    .prompt_start
                    .as_ref()
                    .map(|position| position.line)
                    .or_else(|| {
                        self.state
                            .command_start
                            .as_ref()
                            .map(|position| position.line)
                    })
                    .unwrap_or(event.line);
                let command_line = self
                    .state
                    .command_start
                    .as_ref()
                    .map(|position| position.line)
                    .or_else(|| {
                        self.state
                            .prompt_start
                            .as_ref()
                            .map(|position| position.line)
                    })
                    .unwrap_or(start_line);
                let command = if self.state.pending_command_text_from_protocol {
                    self.state.pending_command_text.clone()
                } else {
                    extract_command_from_visible_buffer(
                        term,
                        self.state
                            .command_start
                            .as_ref()
                            .map(|position| position.line),
                        event.line,
                    )
                };
                let mark = self.create_shell_integrated_mark(command, start_line, command_line);
                self.state.active_command_id = Some(mark.command_id.clone());
                self.state.active_start_line = Some(start_line);
                self.state.prompt_start = None;
                command_events.push(TerminalCommandMarkEvent::Created(mark));
            }
            ShellIntegrationEventKind::CommandEnd => {
                let end_boundary = prompt_block_start_line(term, event.line);
                if let Some(closed) = self.close_active_mark_before(
                    end_boundary,
                    TerminalCommandMarkClosedBy::ShellIntegration,
                    event.exit_code,
                ) {
                    command_events.push(TerminalCommandMarkEvent::Closed(closed));
                }
            }
        }

        command_events
    }

    fn create_shell_integrated_mark(
        &mut self,
        command: Option<String>,
        start_line: usize,
        command_line: usize,
    ) -> TerminalCommandMark {
        self.next_command_sequence = self.next_command_sequence.saturating_add(1);
        let now = now_millis();
        let mark = TerminalCommandMark {
            command_id: format!("term-cmd-{}-{}", now, self.next_command_sequence),
            command,
            start_line,
            command_line: command_line.max(start_line),
            end_line: None,
            is_closed: false,
            closed_by: None,
            exit_code: None,
            duration_ms: None,
            detection_source: TerminalCommandMarkDetectionSource::ShellIntegration,
            submitted_by: None,
            confidence: TerminalCommandMarkConfidence::High,
            output_confidence: TerminalCommandMarkConfidence::Unknown,
            stale: false,
            started_at: self
                .state
                .started_at
                .or_else(|| self.state.prompt_start.as_ref().map(|position| position.at))
                .unwrap_or(now),
            finished_at: None,
        };
        self.marks.push(mark.clone());
        if self.marks.len() > MAX_MARKS {
            self.marks.drain(0..self.marks.len() - MAX_MARKS);
        }
        mark
    }

    fn close_active_mark_before(
        &mut self,
        next_block_start_line: usize,
        closed_by: TerminalCommandMarkClosedBy,
        exit_code: Option<i32>,
    ) -> Option<TerminalCommandMark> {
        let command_id = self.state.active_command_id.take()?;
        let fallback_start = self
            .state
            .active_start_line
            .or_else(|| {
                self.state
                    .prompt_start
                    .as_ref()
                    .map(|position| position.line)
            })
            .unwrap_or(next_block_start_line);
        let close_line = next_block_start_line.saturating_sub(1).max(fallback_start);
        let now = now_millis();
        let mark = self
            .marks
            .iter_mut()
            .find(|mark| mark.command_id == command_id && !mark.is_closed)?;
        mark.is_closed = true;
        mark.closed_by = Some(closed_by);
        mark.output_confidence = TerminalCommandMarkConfidence::High;
        mark.end_line = Some(close_line);
        mark.exit_code = exit_code;
        mark.finished_at = Some(now);
        mark.duration_ms = Some(now.saturating_sub(mark.started_at));
        self.state.active_start_line = None;
        Some(mark.clone())
    }
}

fn parse_shell_integration_event(
    source: ShellIntegrationSource,
    data: &str,
    position: (usize, usize),
) -> Option<ShellIntegrationEvent> {
    let (sequence, args) = split_sequence(data);
    let kind = match (source, sequence.as_str()) {
        (_, "A") => ShellIntegrationEventKind::PromptStart,
        (_, "B") => ShellIntegrationEventKind::CommandStart,
        (ShellIntegrationSource::Osc133, "C") => ShellIntegrationEventKind::OutputStart,
        (ShellIntegrationSource::Osc133, "D") => ShellIntegrationEventKind::CommandEnd,
        (ShellIntegrationSource::Osc633, "C" | "E") => ShellIntegrationEventKind::OutputStart,
        (ShellIntegrationSource::Osc633, "D") => ShellIntegrationEventKind::CommandEnd,
        _ => return None,
    };
    let command = (source == ShellIntegrationSource::Osc633 && sequence == "E")
        .then(|| sanitize_shell_integration_command_text(&args.join(";")))
        .flatten();
    let exit_code = (kind == ShellIntegrationEventKind::CommandEnd)
        .then(|| parse_exit_code(&args))
        .flatten();

    Some(ShellIntegrationEvent {
        kind,
        source,
        line: position.0,
        col: position.1,
        sequence,
        raw: data.to_string(),
        command,
        exit_code,
    })
}

fn parse_osc7_cwd(data: &str) -> Option<(String, Option<String>)> {
    parse_cwd_payload(data)
}

fn parse_osc633_cwd(data: &str) -> Option<(String, Option<String>)> {
    let (sequence, args) = split_sequence(data);
    if sequence != "P" {
        return None;
    }
    args.iter()
        .find_map(|arg| arg.strip_prefix("Cwd=").and_then(parse_cwd_payload))
}

fn parse_osc1337_cwd(data: &str) -> Option<(String, Option<String>)> {
    data.strip_prefix("CurrentDir=").and_then(parse_cwd_payload)
}

fn parse_cwd_payload(data: &str) -> Option<(String, Option<String>)> {
    if let Some(rest) = data.strip_prefix("file://") {
        let (host, path) = if rest.starts_with('/') {
            (None, rest)
        } else {
            let slash = rest.find('/')?;
            let host = &rest[..slash];
            ((!host.is_empty()).then(|| host.to_string()), &rest[slash..])
        };
        let cwd = percent_decode(path)?;
        return (!cwd.is_empty()).then_some((cwd, host));
    }

    let cwd = percent_decode(data)?;
    (!cwd.is_empty()).then_some((cwd, None))
}

fn split_sequence(data: &str) -> (String, Vec<String>) {
    let mut parts = data.split(';');
    let sequence = parts.next().unwrap_or_default().to_string();
    let args = parts.map(ToOwned::to_owned).collect();
    (sequence, args)
}

fn parse_exit_code(args: &[String]) -> Option<i32> {
    args.iter().find_map(|part| part.trim().parse::<i32>().ok())
}

fn sanitize_shell_integration_command_text(raw: &str) -> Option<String> {
    if raw.is_empty() || raw.len() > MAX_COMMAND_TEXT_LENGTH * 4 {
        return None;
    }
    let mut value = percent_decode(raw)?;
    value.retain(|ch| {
        let code = ch as u32;
        !(code <= 0x08
            || code == 0x0b
            || code == 0x0c
            || (0x0e..=0x1f).contains(&code)
            || code == 0x7f)
    });
    let value = value.trim().to_string();
    (!value.is_empty() && value.len() <= MAX_COMMAND_TEXT_LENGTH).then_some(value)
}

fn percent_decode(raw: &str) -> Option<String> {
    if !raw.as_bytes().contains(&b'%') {
        return Some(raw.to_string());
    }
    let mut bytes = Vec::with_capacity(raw.len());
    let mut iter = raw.as_bytes().iter().copied();
    while let Some(byte) = iter.next() {
        if byte == b'%' {
            let hi = iter.next()?;
            let lo = iter.next()?;
            let hi = (hi as char).to_digit(16)?;
            let lo = (lo as char).to_digit(16)?;
            bytes.push(((hi << 4) | lo) as u8);
        } else {
            bytes.push(byte);
        }
    }
    String::from_utf8(bytes).ok()
}

fn cursor_position<T: EventListener>(term: &Term<T>) -> (usize, usize) {
    let content = term.renderable_content();
    let scrollback = term.total_lines().saturating_sub(term.screen_lines());
    let line = (content.cursor.point.line.0).max(0) as usize + scrollback;
    (line, content.cursor.point.column.0)
}

fn prompt_block_start_line<T: EventListener>(term: &Term<T>, command_line: usize) -> usize {
    if !is_likely_prompt_input_line(&line_text(term, command_line)) {
        return command_line;
    }

    let mut start_line = command_line;
    let min_line = command_line.saturating_sub(3);
    for line in (min_line..command_line).rev() {
        if !is_likely_prompt_preamble_line(&line_text(term, line)) {
            break;
        }
        start_line = line;
    }
    start_line
}

fn extract_command_from_visible_buffer<T: EventListener>(
    term: &Term<T>,
    command_start: Option<usize>,
    output_start: usize,
) -> Option<String> {
    let start_line = command_start.unwrap_or(output_start);
    let end_line = output_start.max(start_line);
    let mut lines = Vec::new();
    for line in start_line..=end_line {
        lines.push(line_text(term, line));
    }
    sanitize_shell_integration_command_text(strip_prompt_prefix(&lines.join("\n")))
}

pub(crate) fn line_text<T: EventListener>(term: &Term<T>, absolute_line: usize) -> String {
    let scrollback = term.total_lines().saturating_sub(term.screen_lines());
    let line = absolute_line as i32 - scrollback as i32;
    let top = -(scrollback as i32);
    let bottom = term.screen_lines() as i32;
    if line < top || line >= bottom {
        return String::new();
    }

    let grid = term.grid();
    let row = &grid[Line(line)];
    let mut text = String::new();
    for cell in row[..].iter().take(term.columns()) {
        if cell
            .flags
            .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
        {
            continue;
        }
        text.push(if cell.c == '\0' { ' ' } else { cell.c });
        for ch in cell.zerowidth().into_iter().flatten() {
            text.push(*ch);
        }
    }
    text.trim_end().to_string()
}

fn strip_prompt_prefix(text: &str) -> &str {
    text.trim_start_matches(|ch: char| {
        ch.is_whitespace()
            || matches!(
                ch,
                '❯' | '➜' | 'λ' | '>' | '$' | '#' | '%' | '❮' | '›' | '»'
            )
    })
}

fn is_likely_prompt_input_line(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.is_empty()
        || trimmed.chars().next().is_some_and(|ch| {
            matches!(
                ch,
                '❯' | '➜' | 'λ' | '>' | '$' | '#' | '%' | '❮' | '›' | '»'
            )
        })
}

fn is_likely_prompt_preamble_line(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    let has_private_use_glyph = trimmed
        .chars()
        .any(|ch| ('\u{e000}'..='\u{f8ff}').contains(&ch));
    let has_powerline_glyph = trimmed
        .chars()
        .any(|ch| matches!(ch, '' | '' | '' | ''));
    let has_ruler = trimmed.contains("......") || trimmed.contains("······");
    let has_clock = trimmed.split_whitespace().any(|part| {
        part.chars().filter(|ch| *ch == ':').count() >= 1
            && part.chars().any(|ch| ch.is_ascii_digit())
    });
    let has_prompt_context =
        trimmed.contains('@') || trimmed.contains('~') || trimmed.contains('/');
    has_powerline_glyph
        || (has_private_use_glyph && (has_clock || has_ruler || has_prompt_context))
        || (has_ruler && (has_clock || has_prompt_context))
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}
