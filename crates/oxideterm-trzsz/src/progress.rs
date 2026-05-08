// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    fmt,
    sync::Arc,
    time::{Duration, Instant},
};

const SPEED_ARRAY_SIZE: usize = 30;
const PROGRESS_UPDATE_INTERVAL: Duration = Duration::from_millis(200);
const BAR_MIN_LENGTH: usize = 24;

#[derive(Clone)]
pub struct TextProgressBar {
    output: Vec<String>,
    output_writer: Option<Arc<dyn Fn(String) + Send + Sync + 'static>>,
    last_update_time: Option<Instant>,
    columns: usize,
    file_count: usize,
    file_index: usize,
    file_name: String,
    file_size: u64,
    file_step: Option<u64>,
    start_time: Option<Instant>,
    tmux_pane_columns: usize,
    first_write: bool,
    speed_count: usize,
    speed_index: usize,
    time_array: [Option<Instant>; SPEED_ARRAY_SIZE],
    step_array: [u64; SPEED_ARRAY_SIZE],
}

impl TextProgressBar {
    pub fn new(columns: usize, tmux_pane_columns: Option<usize>) -> Self {
        let tmux_pane_columns = tmux_pane_columns.unwrap_or(0);
        let columns = if tmux_pane_columns > 1 {
            tmux_pane_columns - 1
        } else {
            columns
        };

        Self {
            output: Vec::new(),
            output_writer: None,
            last_update_time: None,
            columns: columns.max(1),
            file_count: 0,
            file_index: 0,
            file_name: String::new(),
            file_size: 0,
            file_step: None,
            start_time: None,
            tmux_pane_columns,
            first_write: true,
            speed_count: 0,
            speed_index: 0,
            time_array: [None; SPEED_ARRAY_SIZE],
            step_array: [0; SPEED_ARRAY_SIZE],
        }
    }

    pub fn new_with_writer(
        columns: usize,
        tmux_pane_columns: Option<usize>,
        writer: Arc<dyn Fn(String) + Send + Sync + 'static>,
    ) -> Self {
        let mut progress = Self::new(columns, tmux_pane_columns);
        progress.output_writer = Some(writer);
        progress
    }

    pub fn set_terminal_columns(&mut self, columns: usize) {
        self.columns = columns.max(1);
        if self.tmux_pane_columns > 0 {
            self.tmux_pane_columns = 0;
        }
    }

    pub fn on_num(&mut self, num: usize) {
        self.file_count = num;
        self.file_index = 0;
    }

    pub fn on_name(&mut self, name: impl Into<String>) {
        self.file_name = name.into();
        self.file_index += 1;
        let now = Instant::now();
        self.start_time = Some(now);
        self.time_array = [None; SPEED_ARRAY_SIZE];
        self.step_array = [0; SPEED_ARRAY_SIZE];
        self.time_array[0] = Some(now);
        self.speed_count = 1;
        self.speed_index = 1;
        self.file_step = None;
    }

    pub fn on_size(&mut self, size: u64) {
        self.file_size = size;
    }

    pub fn on_step(&mut self, step: u64) {
        self.on_step_at(step, Instant::now());
    }

    pub fn on_done(&mut self) {}

    pub fn hide_cursor(&mut self) {
        self.emit("\x1b[?25l".to_string());
    }

    pub fn show_cursor(&mut self) {
        self.emit("\x1b[?25h".to_string());
    }

    pub fn take_output(&mut self) -> Vec<String> {
        std::mem::take(&mut self.output)
    }

    fn on_step_at(&mut self, step: u64, now: Instant) {
        if self.file_step.is_some_and(|previous| step <= previous) {
            return;
        }

        self.file_step = Some(step);
        self.show_progress(now);
    }

    fn emit(&mut self, output: String) {
        if let Some(writer) = &self.output_writer {
            writer(output.clone());
        }
        self.output.push(output);
    }

    fn show_progress(&mut self, now: Instant) {
        if self
            .last_update_time
            .is_some_and(|last| now.duration_since(last) < PROGRESS_UPDATE_INTERVAL)
        {
            return;
        }

        self.last_update_time = Some(now);
        let file_step = self.file_step.unwrap_or(0);
        let percentage = if self.file_size == 0 {
            "100%".to_string()
        } else {
            format!(
                "{}%",
                (file_step.saturating_mul(100) + self.file_size / 2) / self.file_size
            )
        };
        let total = convert_size_to_string(file_step);
        let speed = self.get_speed(now);
        let speed_string = if speed > 0.0 {
            format!("{}/s", convert_size_to_string(speed as u64))
        } else {
            "--- B/s".to_string()
        };
        let eta_string = if speed > 0.0 {
            let remaining = self.file_size.saturating_sub(file_step) as f64 / speed;
            format!("{} ETA", convert_time_to_string(remaining.round() as u64))
        } else {
            "--- ETA".to_string()
        };
        let progress_text = self.get_progress_text(&percentage, &total, &speed_string, &eta_string);

        if self.first_write {
            self.first_write = false;
            self.emit(progress_text);
        } else if self.tmux_pane_columns > 0 {
            self.emit(format!("\x1b[{}D{progress_text}", self.columns));
        } else {
            self.emit(format!("\r{progress_text}"));
        }
    }

    fn get_speed(&mut self, now: Instant) -> f64 {
        let file_step = self.file_step.unwrap_or(0);
        let (baseline_time, baseline_step) = if self.speed_count <= SPEED_ARRAY_SIZE {
            (self.time_array[0], self.step_array[0])
        } else {
            (
                self.time_array[self.speed_index],
                self.step_array[self.speed_index],
            )
        };

        self.time_array[self.speed_index] = Some(now);
        self.step_array[self.speed_index] = file_step;
        self.speed_count += 1;
        self.speed_index = (self.speed_index + 1) % SPEED_ARRAY_SIZE;

        let Some(baseline_time) = baseline_time else {
            return -1.0;
        };
        let millis = now.duration_since(baseline_time).as_millis() as f64;
        if millis <= 0.0 {
            return -1.0;
        }

        let bytes = file_step.saturating_sub(baseline_step) as f64;
        let speed = bytes * 1000.0 / millis;
        if speed.is_finite() { speed } else { -1.0 }
    }

    fn get_progress_text(&self, percentage: &str, total: &str, speed: &str, eta: &str) -> String {
        let mut left = if self.file_count > 1 {
            format!(
                "({}/{}) {}",
                self.file_index, self.file_count, self.file_name
            )
        } else {
            self.file_name.clone()
        };
        let mut left_length = display_length(&left);
        let mut right = format!(" {percentage} | {total} | {speed} | {eta}");

        if self.columns.saturating_sub(left_length + right.len()) < BAR_MIN_LENGTH
            && left_length > 50
        {
            (left, left_length) = ellipsis_string(&left, 50);
        }
        if self.columns.saturating_sub(left_length + right.len()) < BAR_MIN_LENGTH
            && left_length > 40
        {
            (left, left_length) = ellipsis_string(&left, 40);
        }
        if self.columns.saturating_sub(left_length + right.len()) < BAR_MIN_LENGTH {
            right = format!(" {percentage} | {speed} | {eta}");
        }
        if self.columns.saturating_sub(left_length + right.len()) < BAR_MIN_LENGTH
            && left_length > 30
        {
            (left, left_length) = ellipsis_string(&left, 30);
        }
        if self.columns.saturating_sub(left_length + right.len()) < BAR_MIN_LENGTH {
            right = format!(" {percentage} | {eta}");
        }
        if self.columns.saturating_sub(left_length + right.len()) < BAR_MIN_LENGTH {
            right = format!(" {percentage}");
        }

        let bar_length = BAR_MIN_LENGTH.max(self.columns.saturating_sub(left_length + right.len()));
        let file_step = self.file_step.unwrap_or(0);
        let completed = if self.file_size == 0 {
            bar_length
        } else {
            ((file_step as f64 * bar_length as f64) / self.file_size as f64).round() as usize
        }
        .min(bar_length);
        let between = self
            .columns
            .saturating_sub(left_length + bar_length + right.len())
            .max(1);

        format!(
            "{left}{}[{}{}{}]{right}",
            " ".repeat(between),
            "=".repeat(completed.saturating_sub(1)),
            if completed > 0 { ">" } else { "" },
            " ".repeat(bar_length.saturating_sub(completed)),
        )
    }
}

impl fmt::Debug for TextProgressBar {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TextProgressBar")
            .field("output", &self.output)
            .field("last_update_time", &self.last_update_time)
            .field("columns", &self.columns)
            .field("file_count", &self.file_count)
            .field("file_index", &self.file_index)
            .field("file_name", &self.file_name)
            .field("file_size", &self.file_size)
            .field("file_step", &self.file_step)
            .field("start_time", &self.start_time)
            .field("tmux_pane_columns", &self.tmux_pane_columns)
            .field("first_write", &self.first_write)
            .field("speed_count", &self.speed_count)
            .field("speed_index", &self.speed_index)
            .finish()
    }
}

fn display_length(value: &str) -> usize {
    value
        .chars()
        .map(|ch| {
            if ('\u{4e00}'..='\u{9fa5}').contains(&ch) {
                2
            } else {
                1
            }
        })
        .sum()
}

fn ellipsis_string(value: &str, max: usize) -> (String, usize) {
    let remaining = max.saturating_sub(3);
    let mut length = 0;
    let mut sub = String::new();
    for ch in value.chars() {
        let char_length = if ('\u{4e00}'..='\u{9fa5}').contains(&ch) {
            2
        } else {
            1
        };
        if length + char_length > remaining {
            return (format!("{sub}..."), length + 3);
        }
        length += char_length;
        sub.push(ch);
    }

    (format!("{sub}..."), length + 3)
}

fn convert_size_to_string(size: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut unit_index = 0;
    let mut next_size = size as f64;
    while next_size >= 1024.0 && unit_index < units.len() - 1 {
        next_size /= 1024.0;
        unit_index += 1;
    }

    if next_size >= 100.0 {
        format!("{next_size:.0} {}", units[unit_index])
    } else if next_size >= 10.0 {
        format!("{next_size:.1} {}", units[unit_index])
    } else {
        format!("{next_size:.2} {}", units[unit_index])
    }
}

fn convert_time_to_string(seconds: u64) -> String {
    let mut result = String::new();
    let mut remaining = seconds;
    if remaining >= 3600 {
        result.push_str(&format!("{}:", remaining / 3600));
        remaining %= 3600;
    }

    let minutes = remaining / 60;
    result.push_str(if minutes >= 10 { "" } else { "0" });
    result.push_str(&minutes.to_string());
    result.push(':');

    let seconds = remaining % 60;
    result.push_str(if seconds >= 10 { "" } else { "0" });
    result.push_str(&seconds.to_string());
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_bar_writes_cursor_vt_sequences() {
        let mut progress = TextProgressBar::new(80, None);
        progress.hide_cursor();
        progress.show_cursor();
        assert_eq!(progress.take_output(), vec!["\x1b[?25l", "\x1b[?25h"]);
    }

    #[test]
    fn progress_bar_matches_tauri_shape() {
        let mut progress = TextProgressBar::new(80, None);
        progress.on_num(2);
        progress.on_name("alpha.txt");
        progress.on_size(100);
        let start = Instant::now();
        progress.on_step_at(50, start + PROGRESS_UPDATE_INTERVAL);
        let output = progress.take_output();
        assert_eq!(output.len(), 1);
        assert!(output[0].starts_with("(1/2) alpha.txt "));
        assert!(output[0].contains("["));
        assert!(output[0].contains("50%"));
        assert!(output[0].contains("50.0 B"));
    }

    #[test]
    fn progress_bar_throttles_and_rewrites_with_carriage_return() {
        let mut progress = TextProgressBar::new(64, None);
        progress.on_name("file.bin");
        progress.on_size(100);
        let start = Instant::now();
        progress.on_step_at(10, start + PROGRESS_UPDATE_INTERVAL);
        progress.on_step_at(
            20,
            start + PROGRESS_UPDATE_INTERVAL + Duration::from_millis(50),
        );
        progress.on_step_at(30, start + PROGRESS_UPDATE_INTERVAL * 2);
        let output = progress.take_output();
        assert_eq!(output.len(), 2);
        assert!(!output[0].starts_with('\r'));
        assert!(output[1].starts_with('\r'));
    }

    #[test]
    fn progress_bar_uses_tmux_left_rewrite() {
        let mut progress = TextProgressBar::new(80, Some(40));
        progress.on_name("file.bin");
        progress.on_size(100);
        let start = Instant::now();
        progress.on_step_at(10, start + PROGRESS_UPDATE_INTERVAL);
        progress.on_step_at(20, start + PROGRESS_UPDATE_INTERVAL * 2);
        let output = progress.take_output();
        assert_eq!(output.len(), 2);
        assert!(output[1].starts_with("\x1b[39D"));
    }

    #[test]
    fn chinese_characters_count_as_tauri_double_width_range() {
        assert_eq!(display_length("ab中文"), 6);
    }
}
