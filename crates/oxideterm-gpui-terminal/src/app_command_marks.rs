impl TerminalPane {
    pub fn begin_command_mark(
        &mut self,
        command: &str,
        source: TerminalCommandMarkDetectionSource,
        cx: &mut Context<Self>,
    ) {
        let command = command.trim();
        if command.is_empty() || !self.settings.command_marks_enabled {
            return;
        }
        let (mode, snapshot) = {
            let terminal = self.terminal.lock();
            (terminal.mode(), terminal.snapshot())
        };
        if mode.contains(TermMode::ALT_SCREEN) || mode.intersects(TermMode::MOUSE_MODE) {
            return;
        }

        self.snapshot = self.stamp_snapshot(snapshot);
        let now = now_millis();
        let command_line = self.absolute_cursor_line();
        let start_line = self.prompt_block_start_line(command_line);
        self.close_open_command_marks_before(
            start_line,
            TerminalCommandMarkClosedBy::NextCommand,
            TerminalCommandMarkConfidence::High,
        );

        let mark = TerminalCommandMark {
            command_id: next_command_mark_id(),
            command: Some(command.to_string()),
            start_line,
            command_line,
            end_line: None,
            is_closed: false,
            closed_by: None,
            exit_code: None,
            duration_ms: None,
            detection_source: source,
            submitted_by: None,
            confidence: command_mark_confidence(source),
            output_confidence: TerminalCommandMarkConfidence::Unknown,
            stale: false,
            started_at: now,
            finished_at: None,
        };
        self.command_fact_ledger.create_from_mark(&mark);
        self.command_marks.push(mark);
        self.trim_command_marks();
        cx.notify();
    }

}
impl TerminalPane {
    fn absolute_cursor_line(&self) -> usize {
        self.snapshot
            .scrollback_lines
            .saturating_add(self.snapshot.cursor_row)
            .saturating_sub(self.snapshot.display_offset)
    }

    fn prompt_block_start_line(&self, command_line: usize) -> usize {
        if !self
            .line_text(command_line)
            .is_some_and(is_likely_prompt_input_line)
        {
            return command_line;
        }

        let mut start_line = command_line;
        let min_line = command_line.saturating_sub(3);
        for line in (min_line..command_line).rev() {
            if !self
                .line_text(line)
                .is_some_and(is_likely_prompt_preamble_line)
            {
                break;
            }
            start_line = line;
        }
        start_line
    }

    pub(crate) fn selectable_command_mark_end_line(&self, mark: &TerminalCommandMark) -> usize {
        if let Some(end_line) = mark.end_line {
            return end_line.max(mark.start_line);
        }
        self.prompt_block_start_line(self.absolute_cursor_line())
            .saturating_sub(1)
            .max(mark.start_line)
    }

    pub(crate) fn absolute_line_for_position(&self, position: Point<Pixels>) -> usize {
        let point = self.terminal_point_for_position(position);
        self.snapshot
            .scrollback_lines
            .saturating_add(point.row)
            .saturating_sub(self.snapshot.display_offset)
    }

    pub(crate) fn command_mark_id_at_absolute_line(&self, absolute_line: usize) -> Option<String> {
        if !self.settings.command_marks_enabled {
            return None;
        }
        self.command_marks
            .iter()
            .rev()
            .find(|mark| {
                let end_line = self.selectable_command_mark_end_line(mark);
                absolute_line >= mark.start_line && absolute_line <= end_line
            })
            .map(|mark| mark.command_id.clone())
    }

    pub(crate) fn command_mark_start_line(&self, command_mark_id: &str) -> Option<usize> {
        self.command_marks
            .iter()
            .find(|mark| mark.command_id == command_mark_id)
            .map(|mark| mark.start_line)
    }

    pub(crate) fn command_mark_has_command_text(&self, command_mark_id: Option<&str>) -> bool {
        command_mark_id
            .and_then(|id| {
                self.command_marks
                    .iter()
                    .find(|mark| mark.command_id == id)
                    .and_then(|mark| mark.command.as_deref())
            })
            .is_some_and(|command| !command.trim().is_empty())
    }

    pub(crate) fn previous_command_mark_id_before_line(
        &self,
        absolute_line: usize,
    ) -> Option<String> {
        if !self.settings.command_marks_enabled {
            return None;
        }
        self.command_marks
            .iter()
            .rev()
            .find(|mark| mark.start_line < absolute_line)
            .map(|mark| mark.command_id.clone())
    }

    pub(crate) fn next_command_mark_id_after_line(
        &self,
        absolute_line: usize,
    ) -> Option<String> {
        if !self.settings.command_marks_enabled {
            return None;
        }
        self.command_marks
            .iter()
            .find(|mark| mark.start_line > absolute_line)
            .map(|mark| mark.command_id.clone())
    }

    pub(crate) fn copy_command_mark_command_to_clipboard(
        &mut self,
        command_mark_id: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let Some(command) = command_mark_id.and_then(|id| {
            self.command_marks
                .iter()
                .find(|mark| mark.command_id == id)
                .and_then(|mark| mark.command.as_deref())
                .map(str::to_string)
        }) else {
            return;
        };
        if command.trim().is_empty() {
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(command));
    }

    pub(crate) fn select_command_mark_by_id(
        &mut self,
        command_mark_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let selected = command_mark_id.filter(|id| {
            self.command_marks
                .iter()
                .any(|mark| mark.command_id == id.as_str())
        });
        if self.selected_command_mark_id == selected {
            return;
        }
        self.selected_command_mark_id = selected;
        cx.notify();
    }

    pub(crate) fn jump_to_command_mark_from_context_menu(
        &mut self,
        reference_line: usize,
        direction: TerminalCommandNavigationDirection,
        cx: &mut Context<Self>,
    ) {
        let target_id = match direction {
            TerminalCommandNavigationDirection::Previous => {
                self.previous_command_mark_id_before_line(reference_line)
            }
            TerminalCommandNavigationDirection::Next => {
                self.next_command_mark_id_after_line(reference_line)
            }
        };
        let Some(target_id) = target_id else {
            return;
        };
        let Some(target_line) = self.command_mark_start_line(&target_id) else {
            return;
        };
        self.selected_command_mark_id = Some(target_id);
        self.scroll_to_absolute_line(target_line, cx);
    }

    pub(crate) fn clear_screen_from_context_menu(&mut self, cx: &mut Context<Self>) {
        // Ctrl-L asks the running shell/TUI to redraw without deleting scrollback
        // or invalidating command facts the way a terminal reset would.
        self.send_user_protocol_bytes(&[0x0c], cx);
    }

    fn scroll_to_absolute_line(&mut self, absolute_line: usize, cx: &mut Context<Self>) {
        let desired_row = (self.snapshot.rows / 3).max(1);
        let target_offset = self
            .snapshot
            .scrollback_lines
            .saturating_add(desired_row)
            .saturating_sub(absolute_line)
            .min(self.snapshot.scrollback_lines);
        let snapshot = {
            let mut terminal = self.terminal.lock();
            terminal.scroll_to_display_offset(target_offset);
            terminal.snapshot()
        };
        self.clear_smooth_scroll_remainder();
        self.snapshot = self.stamp_snapshot(snapshot);
        cx.notify();
    }

    fn line_text(&self, absolute_line: usize) -> Option<String> {
        let viewport_start = self
            .snapshot
            .scrollback_lines
            .saturating_sub(self.snapshot.display_offset);
        let row = absolute_line.checked_sub(viewport_start)?;
        self.snapshot.lines.get(row).map(|line| line.text())
    }

    fn close_open_command_marks_before(
        &mut self,
        next_start_line: usize,
        closed_by: TerminalCommandMarkClosedBy,
        output_confidence: TerminalCommandMarkConfidence,
    ) {
        let now = now_millis();
        for mark in &mut self.command_marks {
            if mark.is_closed {
                continue;
            }
            mark.is_closed = true;
            mark.closed_by = Some(closed_by);
            mark.output_confidence = output_confidence;
            mark.end_line = Some(mark.start_line.max(next_start_line.saturating_sub(1)));
            mark.finished_at = Some(now);
            mark.duration_ms = Some(now.saturating_sub(mark.started_at));
            self.command_fact_ledger.close_from_mark(mark);
        }
    }

    pub(crate) fn mark_open_command_marks_stale_for_terminal_reset(&mut self) {
        let now = now_millis();
        let fallback_end_line = self.absolute_cursor_line();
        for mark in &mut self.command_marks {
            if mark.is_closed {
                continue;
            }
            // Tauri marks only open command facts stale on clear_buffer. Native
            // mirrors that by closing open visual command marks as terminal
            // resets while preserving already closed command history.
            mark.is_closed = true;
            mark.closed_by = Some(TerminalCommandMarkClosedBy::TerminalReset);
            mark.output_confidence = TerminalCommandMarkConfidence::Unknown;
            mark.end_line = Some(fallback_end_line.max(mark.start_line));
            mark.finished_at = Some(now);
            mark.duration_ms = Some(now.saturating_sub(mark.started_at));
            mark.stale = true;
            self.command_fact_ledger.close_from_mark(mark);
        }
    }

    fn shell_integration_dedup_candidate(
        &self,
        mark: &TerminalCommandMark,
    ) -> Option<(usize, TerminalCommandMarkDetectionSource)> {
        let command = normalized_command(mark.command.as_deref()?)?;
        let now = now_millis();
        self.command_marks
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, candidate)| {
                if candidate.detection_source
                    == TerminalCommandMarkDetectionSource::ShellIntegration
                {
                    return None;
                }
                if !matches!(
                    candidate.detection_source,
                    TerminalCommandMarkDetectionSource::CommandBar
                        | TerminalCommandMarkDetectionSource::Ai
                        | TerminalCommandMarkDetectionSource::Broadcast
                ) {
                    return None;
                }
                if normalized_command(candidate.command.as_deref()?)? != command {
                    return None;
                }
                if mark.start_line.abs_diff(candidate.start_line) > COMMAND_MARK_DEDUP_LINE_DISTANCE
                {
                    return None;
                }
                if now.saturating_sub(candidate.started_at) > COMMAND_MARK_DEDUP_WINDOW_MS {
                    return None;
                }
                Some((index, candidate.detection_source))
            })
    }

    fn trim_command_marks(&mut self) {
        if self.command_marks.len() <= MAX_COMMAND_MARKS_PER_PANE {
            return;
        }
        let remove_count = self.command_marks.len() - MAX_COMMAND_MARKS_PER_PANE;
        let removed_selected = self
            .selected_command_mark_id
            .as_ref()
            .is_some_and(|selected| {
                self.command_marks
                    .iter()
                    .take(remove_count)
                    .any(|mark| &mark.command_id == selected)
            });
        self.command_marks.drain(..remove_count);
        if removed_selected {
            self.selected_command_mark_id = None;
        }
    }

}
fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

fn next_command_mark_id() -> String {
    format!(
        "term-cmd-{}-{}",
        now_millis(),
        NEXT_COMMAND_MARK_ID.fetch_add(1, Ordering::Relaxed)
    )
}

fn command_mark_confidence(
    source: TerminalCommandMarkDetectionSource,
) -> TerminalCommandMarkConfidence {
    match source {
        TerminalCommandMarkDetectionSource::Heuristic
        | TerminalCommandMarkDetectionSource::UserInputObserved => {
            TerminalCommandMarkConfidence::Low
        }
        TerminalCommandMarkDetectionSource::CommandBar
        | TerminalCommandMarkDetectionSource::Ai
        | TerminalCommandMarkDetectionSource::Broadcast
        | TerminalCommandMarkDetectionSource::ShellIntegration => {
            TerminalCommandMarkConfidence::High
        }
    }
}

fn normalized_command(command: &str) -> Option<String> {
    let command = command.trim();
    (!command.is_empty()).then(|| command.to_string())
}

fn is_likely_prompt_input_line(text: String) -> bool {
    let trimmed = text.trim();
    trimmed.is_empty()
        || trimmed.chars().next().is_some_and(|ch| {
            matches!(
                ch,
                '❯' | '➜' | 'λ' | '>' | '$' | '#' | '%' | '❮' | '›' | '»'
            )
        })
}

fn is_likely_prompt_preamble_line(text: String) -> bool {
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
    let has_ruler = has_repeated_ruler(trimmed);
    let has_clock = has_clock_like_text(trimmed);
    let has_prompt_context = trimmed.contains('@')
        || trimmed.contains('~')
        || trimmed.contains('/')
        || trimmed.contains('$');

    has_powerline_glyph
        || (has_private_use_glyph && (has_clock || has_ruler || has_prompt_context))
        || (has_ruler && (has_clock || has_prompt_context))
}

fn has_repeated_ruler(text: &str) -> bool {
    let mut count = 0;
    for ch in text.chars() {
        if matches!(ch, '·' | '•' | '∙' | '.') {
            count += 1;
            if count >= 6 {
                return true;
            }
        } else {
            count = 0;
        }
    }
    false
}

fn has_clock_like_text(text: &str) -> bool {
    text.split(|ch: char| !ch.is_ascii_digit() && ch != ':')
        .any(|part| {
            let pieces = part.split(':').collect::<Vec<_>>();
            match pieces.as_slice() {
                [hour, minute] | [hour, minute, ..] => {
                    (1..=2).contains(&hour.len()) && minute.len() == 2
                }
                _ => false,
            }
        })
}

#[derive(Default)]
struct TerminalInputTracker {
    value: String,
    cursor_index: usize,
}

impl TerminalInputTracker {
    fn state(&self) -> TerminalAutosuggestInputState {
        TerminalAutosuggestInputState {
            value: self.value.clone(),
            cursor_index: self.cursor_index,
            is_cursor_at_end: self.cursor_index == self.value.len(),
        }
    }

    fn apply_bytes(&mut self, bytes: &[u8]) -> Option<String> {
        let data = String::from_utf8_lossy(bytes);
        if data.contains("\u{1b}[200~") || data.contains("\u{1b}[201~") {
            self.reset();
            return None;
        }

        let mut completed_command = None;
        let chars = data.chars().collect::<Vec<_>>();
        let mut index = 0;
        while index < chars.len() {
            let ch = chars[index];
            match ch {
                '\r' | '\n' => {
                    let command = self.value.trim().to_string();
                    if !command.is_empty() {
                        completed_command = Some(command);
                    }
                    self.reset();
                    index += 1;
                }
                '\u{3}' => {
                    self.reset();
                    index += 1;
                }
                '\u{15}' => {
                    self.value = self.value[self.cursor_index..].to_string();
                    self.cursor_index = 0;
                    index += 1;
                }
                '\u{b}' => {
                    self.value.truncate(self.cursor_index);
                    index += 1;
                }
                '\u{1}' => {
                    self.cursor_index = 0;
                    index += 1;
                }
                '\u{5}' => {
                    self.cursor_index = self.value.len();
                    index += 1;
                }
                '\u{7f}' | '\u{8}' => {
                    self.backspace();
                    index += 1;
                }
                '\u{1b}' => {
                    let consumed = self.apply_escape_sequence(&chars[index..]);
                    if consumed > 0 {
                        index += consumed;
                    } else {
                        self.reset();
                        index += 1;
                    }
                }
                _ if is_printable_input(ch) => {
                    self.insert(ch);
                    index += 1;
                }
                _ => index += 1,
            }
        }
        completed_command
    }

    fn reset(&mut self) {
        self.value.clear();
        self.cursor_index = 0;
    }

    fn insert(&mut self, ch: char) {
        self.value.insert(self.cursor_index, ch);
        self.cursor_index += ch.len_utf8();
    }

    fn backspace(&mut self) {
        if self.cursor_index == 0 {
            return;
        }
        let Some((previous_index, _)) = self.value[..self.cursor_index].char_indices().last()
        else {
            return;
        };
        self.value.drain(previous_index..self.cursor_index);
        self.cursor_index = previous_index;
    }

    fn apply_escape_sequence(&mut self, chars: &[char]) -> usize {
        if starts_with_chars(chars, &['\u{1b}', '[', 'D'])
            || starts_with_chars(chars, &['\u{1b}', 'O', 'D'])
        {
            self.cursor_index = previous_char_boundary(&self.value, self.cursor_index);
            return 3;
        }
        if starts_with_chars(chars, &['\u{1b}', '[', 'C'])
            || starts_with_chars(chars, &['\u{1b}', 'O', 'C'])
        {
            self.cursor_index = next_char_boundary(&self.value, self.cursor_index);
            return 3;
        }
        if starts_with_chars(chars, &['\u{1b}', '[', 'H'])
            || starts_with_chars(chars, &['\u{1b}', 'O', 'H'])
        {
            self.cursor_index = 0;
            return 3;
        }
        if starts_with_chars(chars, &['\u{1b}', '[', '1', '~'])
            || starts_with_chars(chars, &['\u{1b}', '[', '7', '~'])
        {
            self.cursor_index = 0;
            return 4;
        }
        if starts_with_chars(chars, &['\u{1b}', '[', 'F'])
            || starts_with_chars(chars, &['\u{1b}', 'O', 'F'])
        {
            self.cursor_index = self.value.len();
            return 3;
        }
        if starts_with_chars(chars, &['\u{1b}', '[', '4', '~'])
            || starts_with_chars(chars, &['\u{1b}', '[', '8', '~'])
        {
            self.cursor_index = self.value.len();
            return 4;
        }
        if starts_with_chars(chars, &['\u{1b}', '[', '3', '~']) {
            if self.cursor_index < self.value.len() {
                let next = next_char_boundary(&self.value, self.cursor_index);
                self.value.drain(self.cursor_index..next);
            }
            return 4;
        }
        0
    }
}

fn starts_with_chars(chars: &[char], prefix: &[char]) -> bool {
    chars.len() >= prefix.len() && chars.iter().zip(prefix).all(|(left, right)| left == right)
}

fn previous_char_boundary(value: &str, cursor_index: usize) -> usize {
    value[..cursor_index]
        .char_indices()
        .last()
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn next_char_boundary(value: &str, cursor_index: usize) -> usize {
    if cursor_index >= value.len() {
        return value.len();
    }
    value[cursor_index..]
        .char_indices()
        .nth(1)
        .map(|(offset, _)| cursor_index + offset)
        .unwrap_or(value.len())
}

fn is_printable_input(ch: char) -> bool {
    let code = ch as u32;
    code >= 0x20 && code != 0x7f
}

#[cfg(test)]
mod input_tracker_tests {
    use super::TerminalInputTracker;

    #[test]
    fn input_tracker_completes_plain_command_on_enter() {
        let mut tracker = TerminalInputTracker::default();

        assert_eq!(tracker.apply_bytes(b"pwd"), None);
        assert_eq!(tracker.apply_bytes(b"\r"), Some("pwd".to_string()));
        assert_eq!(tracker.apply_bytes(b"\r"), None);
    }

    #[test]
    fn input_tracker_tracks_basic_line_editing_sequences() {
        let mut tracker = TerminalInputTracker::default();

        assert_eq!(tracker.apply_bytes(b"abc"), None);
        assert_eq!(tracker.apply_bytes(b"\x1b[D"), None);
        assert_eq!(tracker.apply_bytes(b"\x7f"), None);
        assert_eq!(tracker.apply_bytes(b"Z\r"), Some("aZc".to_string()));
    }

    #[test]
    fn input_tracker_exposes_tauri_autosuggest_state_shape() {
        let mut tracker = TerminalInputTracker::default();

        assert_eq!(tracker.apply_bytes(b"abc"), None);
        assert_eq!(tracker.apply_bytes(b"\x1b[D"), None);
        assert_eq!(tracker.apply_bytes(b"Z"), None);

        let state = tracker.state();
        assert_eq!(state.value, "abZc");
        assert_eq!(state.cursor_index, 3);
        assert!(!state.is_cursor_at_end);
    }

    #[test]
    fn input_tracker_resets_on_interrupt_and_bracketed_paste() {
        let mut tracker = TerminalInputTracker::default();

        assert_eq!(tracker.apply_bytes(b"rm -rf"), None);
        assert_eq!(tracker.apply_bytes(b"\x03"), None);
        assert_eq!(tracker.apply_bytes(b"\r"), None);

        assert_eq!(tracker.apply_bytes(b"echo before"), None);
        assert_eq!(tracker.apply_bytes(b"\x1b[200~pasted\x1b[201~"), None);
        assert_eq!(tracker.apply_bytes(b"\r"), None);
    }
}
