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
pub(crate) struct TerminalRecordingOptions {
    pub(crate) title: Option<String>,
    pub(crate) capture_input: bool,
    pub(crate) theme: Option<TerminalRecordingTheme>,
}

#[derive(Clone, Debug)]
pub(crate) struct TerminalRecordingTheme {
    pub(crate) fg: String,
    pub(crate) bg: String,
}

#[derive(Clone, Debug)]
enum RecordingEventKind {
    Output,
    Input,
    Resize,
}

#[derive(Clone, Debug)]
struct RecordingEvent {
    at: Duration,
    kind: RecordingEventKind,
    data: String,
}

pub(crate) struct TerminalRecorder {
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
    pub(crate) fn start(cols: usize, rows: usize, options: TerminalRecordingOptions) -> Self {
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

    pub(crate) fn status(&self) -> TerminalRecordingStatus {
        TerminalRecordingStatus {
            state: self.state,
            elapsed: self.elapsed(),
            event_count: self.events.len(),
        }
    }

    pub(crate) fn pause(&mut self) {
        if self.state != TerminalRecordingState::Recording {
            return;
        }
        self.paused_at = Some(Instant::now());
        self.state = TerminalRecordingState::Paused;
    }

    pub(crate) fn resume(&mut self) {
        if self.state != TerminalRecordingState::Paused {
            return;
        }
        if let Some(paused_at) = self.paused_at.take() {
            self.paused_duration += paused_at.elapsed();
        }
        self.state = TerminalRecordingState::Recording;
    }

    pub(crate) fn record_output(&mut self, bytes: &[u8]) {
        if self.state != TerminalRecordingState::Recording || bytes.is_empty() {
            return;
        }
        self.events.push(RecordingEvent {
            at: self.elapsed(),
            kind: RecordingEventKind::Output,
            data: String::from_utf8_lossy(bytes).into_owned(),
        });
    }

    pub(crate) fn record_input(&mut self, data: &str) {
        if self.state != TerminalRecordingState::Recording
            || !self.options.capture_input
            || data.is_empty()
        {
            return;
        }
        self.events.push(RecordingEvent {
            at: self.elapsed(),
            kind: RecordingEventKind::Input,
            data: data.to_string(),
        });
    }

    pub(crate) fn record_resize(&mut self, cols: usize, rows: usize) {
        self.cols = cols;
        self.rows = rows;
        if self.state != TerminalRecordingState::Recording {
            return;
        }
        self.events.push(RecordingEvent {
            at: self.elapsed(),
            kind: RecordingEventKind::Resize,
            data: format!("{cols}x{rows}"),
        });
    }

    pub(crate) fn stop(mut self) -> String {
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
            cast.push_str(match event.kind {
                RecordingEventKind::Output => "\"o\"",
                RecordingEventKind::Input => "\"i\"",
                RecordingEventKind::Resize => "\"r\"",
            });
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

fn merge_output_events(
    events: Vec<RecordingEvent>,
    merge_threshold: Duration,
) -> Vec<RecordingEvent> {
    let mut merged: Vec<RecordingEvent> = Vec::with_capacity(events.len());
    for event in events {
        if let Some(last) = merged.last_mut()
            && matches!(last.kind, RecordingEventKind::Output)
            && matches!(event.kind, RecordingEventKind::Output)
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
}
