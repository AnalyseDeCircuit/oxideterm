use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::json;

const DEFAULT_MERGE_THRESHOLD: Duration = Duration::from_millis(16);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalRecordingState {
    Idle,
    Recording,
    Paused,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalRecordingStatus {
    pub state: TerminalRecordingState,
    pub elapsed: Duration,
    pub event_count: usize,
}

impl Default for TerminalRecordingStatus {
    fn default() -> Self {
        Self {
            state: TerminalRecordingState::Idle,
            elapsed: Duration::ZERO,
            event_count: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TerminalRecordingOptions {
    pub title: Option<String>,
    pub capture_input: bool,
    pub theme: Option<TerminalRecordingTheme>,
}

#[derive(Clone, Debug)]
pub struct TerminalRecordingTheme {
    pub fg: String,
    pub bg: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AsciicastEventKind {
    Output,
    Input,
    Resize,
}

impl AsciicastEventKind {
    fn from_char(kind: char) -> Self {
        match kind {
            'i' => Self::Input,
            'r' => Self::Resize,
            _ => Self::Output,
        }
    }

    pub fn as_char(&self) -> char {
        match self {
            Self::Output => 'o',
            Self::Input => 'i',
            Self::Resize => 'r',
        }
    }
}

#[derive(Clone, Debug)]
pub struct AsciicastEvent {
    pub at: f64,
    pub kind: AsciicastEventKind,
    pub data: String,
}

#[derive(Clone, Debug)]
pub struct AsciicastRecording {
    pub file_name: String,
    pub width: usize,
    pub height: usize,
    pub duration: f64,
    pub events: Vec<AsciicastEvent>,
}

impl AsciicastRecording {
    pub fn parse(file_name: impl Into<String>, content: &str) -> Result<Self, String> {
        let file_name = file_name.into();
        let mut lines = content.lines();
        let header_line = lines.next().ok_or_else(|| "empty cast file".to_string())?;
        let header: serde_json::Value =
            serde_json::from_str(header_line).map_err(|error| error.to_string())?;
        let version = header
            .get("version")
            .and_then(|value| value.as_u64())
            .unwrap_or(1);
        let width = header
            .get("width")
            .and_then(|value| value.as_u64())
            .unwrap_or(80) as usize;
        let height = header
            .get("height")
            .and_then(|value| value.as_u64())
            .unwrap_or(24) as usize;
        let mut events = Vec::new();
        if version == 2 {
            for line in lines {
                let value: serde_json::Value =
                    serde_json::from_str(line).map_err(|error| error.to_string())?;
                let Some(array) = value.as_array() else {
                    continue;
                };
                let Some(at) = array.first().and_then(|value| value.as_f64()) else {
                    continue;
                };
                let kind = array
                    .get(1)
                    .and_then(|value| value.as_str())
                    .and_then(|value| value.chars().next())
                    .map(AsciicastEventKind::from_char)
                    .unwrap_or(AsciicastEventKind::Output);
                let data = array
                    .get(2)
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
                    .to_string();
                events.push(AsciicastEvent { at, kind, data });
            }
        } else if let Some(stdout) = header.get("stdout").and_then(|value| value.as_array()) {
            let mut at = 0.0;
            for item in stdout {
                let Some(array) = item.as_array() else {
                    continue;
                };
                at += array
                    .first()
                    .and_then(|value| value.as_f64())
                    .unwrap_or(0.0);
                let data = array
                    .get(1)
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
                    .to_string();
                events.push(AsciicastEvent {
                    at,
                    kind: AsciicastEventKind::Output,
                    data,
                });
            }
        }
        let duration = header
            .get("duration")
            .and_then(|value| value.as_f64())
            .unwrap_or_else(|| events.last().map(|event| event.at).unwrap_or(0.0));

        Ok(Self {
            file_name,
            width,
            height,
            duration,
            events,
        })
    }
}

#[derive(Clone, Debug)]
pub struct TerminalRecordingSearchResult {
    pub at: f64,
    pub snippet: String,
}

#[derive(Clone, Debug)]
pub struct TerminalRecordingPlayback {
    recording: AsciicastRecording,
    position: f64,
    speed: f64,
    playing: bool,
    last_tick: Option<Instant>,
    replayed_event_index: usize,
}

impl TerminalRecordingPlayback {
    pub fn new(recording: AsciicastRecording) -> Self {
        Self {
            recording,
            position: 0.0,
            speed: 1.0,
            playing: false,
            last_tick: None,
            replayed_event_index: 0,
        }
    }

    pub fn recording(&self) -> &AsciicastRecording {
        &self.recording
    }

    pub fn position(&self) -> f64 {
        self.position
    }

    pub fn speed(&self) -> f64 {
        self.speed
    }

    pub fn playing(&self) -> bool {
        self.playing
    }

    pub fn toggle_playing(&mut self) {
        self.playing = !self.playing;
        self.last_tick = self.playing.then(Instant::now);
    }

    pub fn set_speed(&mut self, speed: f64) {
        self.speed = speed;
    }

    pub fn advance_to_now(&mut self) {
        if !self.playing {
            return;
        }
        let now = Instant::now();
        let Some(last_tick) = self.last_tick.replace(now) else {
            return;
        };
        self.position = (self.position + last_tick.elapsed().as_secs_f64() * self.speed)
            .min(self.recording.duration.max(0.0));
        if self.position >= self.recording.duration {
            self.playing = false;
            self.last_tick = None;
        }
    }

    pub fn seek_ratio(&mut self, ratio: f64) {
        self.position = (self.recording.duration * ratio.clamp(0.0, 1.0)).max(0.0);
        self.replayed_event_index = 0;
    }

    pub fn reset_replay(&mut self) {
        self.replayed_event_index = 0;
    }

    pub fn take_due_events(&mut self) -> Vec<AsciicastEvent> {
        let start = self.replayed_event_index;
        let mut end = start;
        while end < self.recording.events.len() && self.recording.events[end].at <= self.position {
            end += 1;
        }
        self.replayed_event_index = end;
        self.recording.events[start..end].to_vec()
    }

    pub fn search(&self, query: &str) -> Vec<TerminalRecordingSearchResult> {
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            return Vec::new();
        }
        self.recording
            .events
            .iter()
            .filter(|event| matches!(event.kind, AsciicastEventKind::Output))
            .filter_map(|event| {
                let snippet = terminal_recording_search_snippet(&event.data, &needle)?;
                Some(TerminalRecordingSearchResult {
                    at: event.at,
                    snippet,
                })
            })
            .take(50)
            .collect()
    }
}

#[derive(Clone, Debug)]
struct RecordingEvent {
    at: Duration,
    kind: AsciicastEventKind,
    data: String,
}

pub struct TerminalRecorder {
    state: TerminalRecordingState,
    cols: usize,
    rows: usize,
    started_at: SystemTime,
    started_at_instant: Instant,
    paused_at: Option<Instant>,
    paused_duration: Duration,
    events: Vec<RecordingEvent>,
    options: TerminalRecordingOptions,
}

impl TerminalRecorder {
    pub fn start(cols: usize, rows: usize, options: TerminalRecordingOptions) -> Self {
        Self {
            state: TerminalRecordingState::Recording,
            cols,
            rows,
            started_at: SystemTime::now(),
            started_at_instant: Instant::now(),
            paused_at: None,
            paused_duration: Duration::ZERO,
            events: Vec::new(),
            options,
        }
    }

    pub fn status(&self) -> TerminalRecordingStatus {
        TerminalRecordingStatus {
            state: self.state,
            elapsed: self.elapsed(),
            event_count: self.events.len(),
        }
    }

    pub fn pause(&mut self) {
        if self.state != TerminalRecordingState::Recording {
            return;
        }
        self.paused_at = Some(Instant::now());
        self.state = TerminalRecordingState::Paused;
    }

    pub fn resume(&mut self) {
        if self.state != TerminalRecordingState::Paused {
            return;
        }
        if let Some(paused_at) = self.paused_at.take() {
            self.paused_duration += paused_at.elapsed();
        }
        self.state = TerminalRecordingState::Recording;
    }

    pub fn record_output(&mut self, bytes: &[u8]) {
        if self.state != TerminalRecordingState::Recording || bytes.is_empty() {
            return;
        }
        self.events.push(RecordingEvent {
            at: self.elapsed(),
            kind: AsciicastEventKind::Output,
            data: String::from_utf8_lossy(bytes).into_owned(),
        });
    }

    pub fn record_input(&mut self, data: &str) {
        if self.state != TerminalRecordingState::Recording
            || !self.options.capture_input
            || data.is_empty()
        {
            return;
        }
        self.events.push(RecordingEvent {
            at: self.elapsed(),
            kind: AsciicastEventKind::Input,
            data: data.to_string(),
        });
    }

    pub fn record_resize(&mut self, cols: usize, rows: usize) {
        self.cols = cols;
        self.rows = rows;
        if self.state != TerminalRecordingState::Recording {
            return;
        }
        self.events.push(RecordingEvent {
            at: self.elapsed(),
            kind: AsciicastEventKind::Resize,
            data: format!("{cols}x{rows}"),
        });
    }

    pub fn stop(mut self) -> String {
        if self.state == TerminalRecordingState::Paused {
            self.resume();
        }
        let duration = self.elapsed();
        let events = merge_output_events(self.events, DEFAULT_MERGE_THRESHOLD);
        let timestamp = self
            .started_at
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or_default();
        let mut header = json!({
            "version": 2,
            "width": self.cols,
            "height": self.rows,
            "timestamp": timestamp,
            "duration": seconds(duration),
            "env": { "TERM": "xterm-256color" },
        });
        if let Some(title) = self.options.title.filter(|title| !title.is_empty()) {
            header["title"] = json!(title);
        }
        if let Some(theme) = self.options.theme {
            header["theme"] = json!({
                "fg": theme.fg,
                "bg": theme.bg,
            });
        }

        let mut cast = String::new();
        cast.push_str(&header.to_string());
        cast.push('\n');
        for event in events {
            cast.push('[');
            cast.push_str(&format!("{:.6}", seconds(event.at)));
            cast.push(',');
            cast.push('"');
            cast.push(event.kind.as_char());
            cast.push('"');
            cast.push(',');
            cast.push_str(&serde_json::to_string(&event.data).unwrap_or_else(|_| "\"\"".into()));
            cast.push_str("]\n");
        }
        cast
    }

    fn elapsed(&self) -> Duration {
        let mut elapsed = self.started_at_instant.elapsed();
        if let Some(paused_at) = self.paused_at {
            elapsed = elapsed.saturating_sub(paused_at.elapsed());
        }
        elapsed.saturating_sub(self.paused_duration)
    }
}

fn seconds(duration: Duration) -> f64 {
    duration.as_secs_f64()
}

pub fn format_recording_elapsed(duration: Duration) -> String {
    let total = duration.as_secs();
    format!("{:02}:{:02}", total / 60, total % 60)
}

pub fn format_cast_time(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    format!("{:02}:{:02}", total / 60, total % 60)
}

pub fn parse_cast_resize(value: &str) -> Option<(usize, usize)> {
    let (cols, rows) = value.split_once('x')?;
    Some((cols.parse().ok()?, rows.parse().ok()?))
}

fn terminal_recording_search_snippet(data: &str, needle: &str) -> Option<String> {
    let plain = strip_cast_control_sequences(data)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let lower = plain.to_lowercase();
    let found = lower.find(needle)?;
    let start = plain[..found]
        .char_indices()
        .rev()
        .nth(24)
        .map_or(0, |(i, _)| i);
    let end = plain[found..]
        .char_indices()
        .nth(96)
        .map_or(plain.len(), |(i, _)| found + i);
    let mut snippet = plain[start..end].trim().to_string();
    if start > 0 {
        snippet.insert_str(0, "...");
    }
    if end < plain.len() {
        snippet.push_str("...");
    }
    Some(snippet)
}

fn strip_cast_control_sequences(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            if matches!(chars.peek(), Some('[' | ']' | 'P' | '_' | '^')) {
                while let Some(next) = chars.next() {
                    if next.is_ascii_alphabetic() || matches!(next, '\u{7}' | '\\') {
                        break;
                    }
                }
            }
            continue;
        }
        if ch.is_control() && ch != '\n' && ch != '\t' {
            continue;
        }
        output.push(ch);
    }
    output
}

fn merge_output_events(
    events: Vec<RecordingEvent>,
    merge_threshold: Duration,
) -> Vec<RecordingEvent> {
    let mut merged: Vec<RecordingEvent> = Vec::with_capacity(events.len());
    for event in events {
        if let Some(last) = merged.last_mut()
            && matches!(last.kind, AsciicastEventKind::Output)
            && matches!(event.kind, AsciicastEventKind::Output)
            && event.at.saturating_sub(last.at) < merge_threshold
        {
            last.data.push_str(&event.data);
            continue;
        }
        merged.push(event);
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_asciicast_v2_output() {
        let mut recorder = TerminalRecorder::start(
            80,
            24,
            TerminalRecordingOptions {
                title: Some("demo".into()),
                capture_input: false,
                theme: None,
            },
        );

        recorder.record_output(b"he");
        recorder.record_output(b"llo");
        recorder.record_input("secret");
        recorder.record_resize(100, 30);
        let cast = recorder.stop();

        assert!(cast.lines().next().unwrap().contains("\"version\":2"));
        assert!(cast.contains("\"hello\""));
        assert!(!cast.contains("secret"));
        assert!(cast.contains("\"r\",\"100x30\""));
    }

    #[test]
    fn parses_and_searches_asciicast_playback() {
        let cast = concat!(
            "{\"version\":2,\"width\":80,\"height\":24,\"duration\":2.0}\n",
            "[0.5,\"o\",\"hello world\"]\n",
            "[1.0,\"r\",\"100x30\"]\n"
        );
        let recording = AsciicastRecording::parse("demo.cast", cast).unwrap();
        assert_eq!(recording.file_name, "demo.cast");
        assert_eq!(recording.width, 80);
        assert_eq!(recording.height, 24);

        let mut playback = TerminalRecordingPlayback::new(recording);
        assert_eq!(playback.search("world")[0].at, 0.5);
        playback.seek_ratio(0.5);
        let events = playback.take_due_events();
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0].kind, AsciicastEventKind::Output));
        assert_eq!(parse_cast_resize(&events[1].data), Some((100, 30)));
    }
}
