impl TerminalPane {
    fn command_folding_surface_available(&self) -> bool {
        let terminal = self.terminal.lock();
        command_mark_ui_available(self.settings.command_marks_enabled, terminal.mode())
            && terminal.tmux_state().is_none()
    }

    fn rebuild_command_fold_projection(&mut self) {
        self.command_fold_projection = CommandFoldProjection::new(
            &self.command_marks,
            self.collapsed_command_ids.as_ref(),
            self.snapshot
                .scrollback_lines
                .saturating_add(self.snapshot.rows),
            self.snapshot.rows,
        );
    }

    fn refresh_command_fold_projection(&mut self, cx: &mut Context<Self>) {
        let snapshot = {
            let mut terminal = self.terminal.lock();
            if self.command_fold_projection.is_active() || self.fold_bottom_padding_rows > 0 {
                // Folded scrolling owns its projected offset, so the backend snapshot stays on
                // the live screen and captures every mutable row for incremental projection.
                terminal.scroll_to_display_offset(0);
            } else {
                terminal.scroll_to_display_offset(self.fold_visual_display_offset);
            }
            terminal.snapshot()
        };
        self.clear_smooth_scroll_remainder();
        self.snapshot = self.stamp_snapshot(snapshot);
        cx.notify();
    }

    pub(super) fn command_folding_active(&self) -> bool {
        self.fold_bottom_padding_rows > 0 || self.command_fold_projection.is_active()
    }

    pub(super) fn folded_visual_history_rows(&self) -> usize {
        self.command_fold_projection.visual_history_rows()
    }

    pub(super) fn scroll_folded_to_display_offset(
        &mut self,
        display_offset: usize,
        cx: &mut Context<Self>,
    ) {
        let display_offset = self
            .command_fold_projection
            .clamp_display_offset(display_offset);
        if display_offset == self.fold_visual_display_offset && self.fold_bottom_padding_rows == 0 {
            return;
        }
        self.fold_visual_display_offset = display_offset;
        self.fold_bottom_padding_rows = 0;
        self.fold_anchor_bottom_padding_rows = 0;
        self.refresh_command_fold_projection(cx);
    }

    pub(super) fn scroll_folded_rows(&mut self, rows: i32, cx: &mut Context<Self>) {
        let previous_position = (
            self.fold_visual_display_offset,
            self.fold_bottom_padding_rows,
        );
        if rows >= 0 {
            let rows = rows as usize;
            let consumed_padding = rows.min(self.fold_bottom_padding_rows);
            self.fold_bottom_padding_rows = self
                .fold_bottom_padding_rows
                .saturating_sub(consumed_padding);
            self.fold_visual_display_offset = self
                .fold_visual_display_offset
                .saturating_add(rows.saturating_sub(consumed_padding));
        } else {
            let rows = rows.unsigned_abs() as usize;
            let consumed_offset = rows.min(self.fold_visual_display_offset);
            self.fold_visual_display_offset = self
                .fold_visual_display_offset
                .saturating_sub(consumed_offset);
            self.fold_bottom_padding_rows = self
                .fold_bottom_padding_rows
                .saturating_add(rows.saturating_sub(consumed_offset))
                .min(self.fold_anchor_bottom_padding_rows);
        }
        self.fold_visual_display_offset =
            self.command_fold_projection
                .clamp_display_offset(self.fold_visual_display_offset);
        if previous_position
            == (
                self.fold_visual_display_offset,
                self.fold_bottom_padding_rows,
            )
        {
            return;
        }
        self.refresh_command_fold_projection(cx);
    }

    pub(super) fn expand_command_fold_containing_physical_line(
        &mut self,
        physical_line: usize,
    ) -> bool {
        let command_id = self
            .command_marks
            .iter()
            .find(|mark| {
                self.collapsed_command_ids.contains(&mark.command_id)
                    && crate::command_folding::foldable_output_range(
                        mark,
                        self.snapshot
                            .scrollback_lines
                            .saturating_add(self.snapshot.rows),
                    )
                    .is_some_and(|range| range.contains(&physical_line))
            })
            .map(|mark| mark.command_id.clone());
        let expanded = command_id.is_some_and(|command_id| {
            Arc::make_mut(&mut self.collapsed_command_ids).remove(&command_id)
        });
        if expanded {
            self.rebuild_command_fold_projection();
        }
        expanded
    }

    pub(crate) fn command_mark_is_foldable(&self, command_mark_id: &str) -> bool {
        if !self.command_folding_surface_available() {
            return false;
        }
        let total_physical_lines = self
            .snapshot
            .scrollback_lines
            .saturating_add(self.snapshot.rows);
        self.command_marks
            .iter()
            .find(|mark| mark.command_id == command_mark_id)
            .and_then(|mark| {
                crate::command_folding::foldable_output_range(mark, total_physical_lines)
            })
            .is_some()
    }

    pub(crate) fn command_mark_is_collapsed(&self, command_mark_id: &str) -> bool {
        self.collapsed_command_ids.contains(command_mark_id)
    }

    pub(crate) fn toggle_command_fold(
        &mut self,
        command_mark_id: &str,
        viewport_row: usize,
        cx: &mut Context<Self>,
    ) {
        if !self.command_mark_is_foldable(command_mark_id) {
            return;
        }
        let Some(command_line) = self
            .command_marks
            .iter()
            .find(|mark| mark.command_id == command_mark_id)
            .map(|mark| mark.command_line)
        else {
            return;
        };
        if !Arc::make_mut(&mut self.collapsed_command_ids).remove(command_mark_id) {
            Arc::make_mut(&mut self.collapsed_command_ids)
                .insert(command_mark_id.to_string());
        }
        self.rebuild_command_fold_projection();
        let position = self
            .command_fold_projection
            .viewport_position_for_anchor(command_line, viewport_row);
        self.fold_visual_display_offset = position.display_offset;
        self.fold_bottom_padding_rows = position.bottom_padding_rows;
        self.fold_anchor_bottom_padding_rows = position.bottom_padding_rows;
        self.refresh_command_fold_projection(cx);
    }

    pub(crate) fn set_all_command_folds(
        &mut self,
        collapsed: bool,
        anchor_command_mark_id: &str,
        viewport_row: usize,
        cx: &mut Context<Self>,
    ) {
        let total_physical_lines = self
            .snapshot
            .scrollback_lines
            .saturating_add(self.snapshot.rows);
        let foldable_ids = self
            .command_marks
            .iter()
            .filter(|mark| {
                crate::command_folding::foldable_output_range(mark, total_physical_lines).is_some()
            })
            .map(|mark| mark.command_id.clone())
            .collect::<Vec<_>>();
        if collapsed {
            Arc::make_mut(&mut self.collapsed_command_ids).extend(foldable_ids);
        } else {
            Arc::make_mut(&mut self.collapsed_command_ids).clear();
        }
        self.rebuild_command_fold_projection();
        let anchor_line = self
            .command_marks
            .iter()
            .find(|mark| mark.command_id == anchor_command_mark_id)
            .map(|mark| mark.command_line)
            .unwrap_or_default();
        let position = self
            .command_fold_projection
            .viewport_position_for_anchor(anchor_line, viewport_row);
        self.fold_visual_display_offset = position.display_offset;
        self.fold_bottom_padding_rows = position.bottom_padding_rows;
        self.fold_anchor_bottom_padding_rows = position.bottom_padding_rows;
        self.refresh_command_fold_projection(cx);
    }

    fn observe_terminal_cwd_action_from_closed_command_mark(
        &mut self,
        mark: &TerminalCommandMark,
        cx: &mut Context<Self>,
    ) {
        if self.apply_pending_cwd_from_closed_command_mark(mark, cx) {
            return;
        }
        if !command_mark_allows_cwd_fallback(mark.exit_code) {
            return;
        }
        if !self.settings.current_directory_awareness_enabled {
            return;
        }
        // OSC 7 owns cwd updates once it has actually reported a cwd. Some
        // shells emit command marks without cwd metadata; keep simple `cd`
        // actions usable for those partially integrated sessions.
        if self.cwd_is_shell_integrated() {
            return;
        }
        let Some(command) = mark.command.as_deref() else {
            return;
        };
        let Some(cwd) = cwd_after_simple_cd_command(command, self.cwd.as_deref()) else {
            return;
        };
        self.set_current_working_directory_from_terminal_action(cwd, cx);
    }

    fn apply_pending_cwd_from_closed_command_mark(
        &mut self,
        mark: &TerminalCommandMark,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(pending) = self.pending_cwd.as_ref() else {
            return false;
        };
        let Some(command) = mark.command.as_deref().map(str::trim) else {
            return false;
        };
        if command != pending.command {
            return false;
        }

        if command_mark_allows_cwd_fallback(mark.exit_code) {
            self.cwd = Some(pending.path.clone());
            self.cwd_source = Some(TerminalWorkingDirectorySource::VisibleCommand);
        }
        self.pending_cwd = None;
        cx.notify();
        true
    }

    pub fn begin_command_mark(
        &mut self,
        command: &str,
        source: TerminalCommandMarkDetectionSource,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let command = command.trim();
        if command.is_empty() || !self.settings.command_marks_enabled {
            return None;
        }
        let (mode, snapshot) = {
            let terminal = self.terminal.lock();
            (terminal.mode(), terminal.snapshot())
        };
        if mode.contains(TermMode::ALT_SCREEN) || mode.intersects(TermMode::MOUSE_MODE) {
            return None;
        }

        self.snapshot = self.stamp_snapshot(snapshot);
        let now = now_millis();
        let command_line = self.absolute_cursor_line();
        let start_line = self.prompt_block_start_line(command_line);
        self.close_open_command_marks_before(
            start_line,
            TerminalCommandMarkClosedBy::NextCommand,
            TerminalCommandMarkConfidence::High,
            cx,
        );

        let mark = TerminalCommandMark {
            command_id: next_command_mark_id(),
            command: Some(command.to_string()),
            start_line,
            command_line,
            output_start_line: None,
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
        let command_id = mark.command_id.clone();
        self.command_fact_ledger.create_from_mark(&mark);
        self.command_marks.push(mark);
        self.command_marks_render_cache_dirty = true;
        self.trim_command_marks();
        cx.notify();
        Some(command_id)
    }

}
impl TerminalPane {
    fn absolute_cursor_line(&self) -> usize {
        self.snapshot
            .lines
            .get(self.snapshot.cursor_row)
            .and_then(|_| {
                usize::try_from(
                    self.snapshot.scrollback_lines as i64
                        + i64::from(snapshot_grid_line_for_row(
                            &self.snapshot,
                            self.snapshot.cursor_row,
                        )?),
                )
                .ok()
            })
            .unwrap_or(self.snapshot.scrollback_lines)
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
            .lines
            .get(point.row)
            .and_then(|_| {
                usize::try_from(
                    self.snapshot.scrollback_lines as i64
                        + i64::from(snapshot_grid_line_for_row(&self.snapshot, point.row)?),
                )
                .ok()
            })
            .unwrap_or(self.snapshot.scrollback_lines)
    }

    pub(crate) fn command_fold_target_at_position(
        &self,
        position: Point<Pixels>,
    ) -> Option<(String, usize)> {
        if !self.command_folding_surface_available() {
            return None;
        }
        let origin = self.content_origin();
        let gutter_start = f32::from(origin.x)
            + TERMINAL_CONTENT_PADDING
            + self.timestamp_gutter_width();
        let gutter_end = gutter_start + self.command_mark_gutter_width();
        let pointer_x = f32::from(position.x);
        if pointer_x < gutter_start || pointer_x >= gutter_end {
            return None;
        }
        let viewport_row = self.terminal_point_for_position(position).row;
        let absolute_line = self.absolute_line_for_position(position);
        let total_physical_lines = self
            .snapshot
            .scrollback_lines
            .saturating_add(self.snapshot.rows);
        self.command_marks
            .iter()
            .find(|mark| {
                mark.command_line == absolute_line
                    && crate::command_folding::foldable_output_range(mark, total_physical_lines)
                        .is_some()
            })
            .map(|mark| (mark.command_id.clone(), viewport_row))
    }

    pub(crate) fn select_command_fold_range(
        &mut self,
        command_mark_id: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(mark) = self
            .command_marks
            .iter()
            .find(|mark| mark.command_id == command_mark_id)
        else {
            return;
        };
        let Some(end_line) = mark.end_line else {
            return;
        };
        let scrollback_lines = self.snapshot.scrollback_lines as i64;
        let start_line = (mark.start_line as i64 - scrollback_lines)
            .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        let end_line = (end_line as i64 - scrollback_lines)
            .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        self.selection = Some(TerminalSelection {
            anchor: TerminalGridPoint {
                line: start_line,
                col: 0,
            },
            head: TerminalGridPoint {
                line: end_line,
                col: self.snapshot.cols.saturating_sub(1),
            },
            mode: TerminalSelectionMode::Lines,
        });
        self.selecting = false;
        self.selected_command_mark_id = Some(command_mark_id.to_string());
        cx.notify();
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

    fn scroll_to_absolute_line(&mut self, absolute_line: usize, cx: &mut Context<Self>) {
        let desired_row = (self.snapshot.rows / 3).max(1);
        self.expand_command_fold_containing_physical_line(absolute_line);
        if self.command_folding_active() {
            let position = self
                .command_fold_projection
                .viewport_position_for_anchor(absolute_line, desired_row);
            self.fold_visual_display_offset = position.display_offset;
            self.fold_bottom_padding_rows = position.bottom_padding_rows;
            self.fold_anchor_bottom_padding_rows = position.bottom_padding_rows;
            self.refresh_command_fold_projection(cx);
            return;
        }
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
        self.snapshot
            .lines
            .iter()
            .enumerate()
            .find(|(row, _)| {
                usize::try_from(
                    self.snapshot.scrollback_lines as i64
                        + i64::from(
                            snapshot_grid_line_for_row(&self.snapshot, *row).unwrap_or_default(),
                        ),
                )
                .ok()
                    == Some(absolute_line)
            })
            .map(|(_, line)| line.text())
    }

    fn close_open_command_marks_before(
        &mut self,
        next_start_line: usize,
        closed_by: TerminalCommandMarkClosedBy,
        output_confidence: TerminalCommandMarkConfidence,
        cx: &mut Context<Self>,
    ) {
        let now = now_millis();
        let mut closed_marks = Vec::new();
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
            closed_marks.push(mark.clone());
        }
        for mark in closed_marks {
            // User-input command marks do not carry shell exit status, but a
            // later command boundary still proves the previous command returned
            // to the shell. The cwd parser remains restricted to simple `cd`.
            self.observe_terminal_cwd_action_from_closed_command_mark(&mark, cx);
        }
        self.command_marks_render_cache_dirty = true;
    }

    pub(crate) fn reset_command_marks_for_terminal_reset(&mut self) {
        self.close_open_command_marks_for_terminal_reset();
        self.clear_visual_command_marks();
    }

    pub(crate) fn reset_command_marks_while_awaiting_backend_reset(&mut self) {
        self.close_open_command_marks_for_terminal_reset();
        // Preserve shell-to-frontend aliases until the backend's ordered
        // Closed -> Reset events finish updating the durable fact ledger.
        self.clear_visual_command_mark_ranges();
    }

    fn close_open_command_marks_for_terminal_reset(&mut self) {
        let now = now_millis();
        let fallback_end_line = self.absolute_cursor_line();
        for mark in &mut self.command_marks {
            if mark.is_closed {
                continue;
            }
            // Preserve the durable command fact while closing any command that
            // was active when its visual terminal coordinates were reset.
            mark.is_closed = true;
            mark.closed_by = Some(TerminalCommandMarkClosedBy::TerminalReset);
            mark.output_confidence = TerminalCommandMarkConfidence::Unknown;
            mark.end_line = Some(fallback_end_line.max(mark.start_line));
            mark.finished_at = Some(now);
            mark.duration_ms = Some(now.saturating_sub(mark.started_at));
            mark.stale = true;
            self.command_fact_ledger.close_from_mark(mark);
        }
        self.command_marks_render_cache_dirty = true;
    }

    pub(crate) fn clear_visual_command_marks(&mut self) {
        // Saved-history clear starts a new terminal line-coordinate epoch.
        self.clear_visual_command_mark_ranges();
        self.command_mark_id_aliases.clear();
    }

    fn clear_visual_command_mark_ranges(&mut self) {
        self.command_marks.clear();
        self.command_marks_render_cache_dirty = true;
        self.selected_command_mark_id = None;
        self.hovered_command_mark_id = None;
        Arc::make_mut(&mut self.collapsed_command_ids).clear();
        self.command_fold_projection = CommandFoldProjection::default();
        self.fold_visual_display_offset = 0;
        self.fold_bottom_padding_rows = 0;
        self.fold_anchor_bottom_padding_rows = 0;
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
        let removed_hovered = self
            .hovered_command_mark_id
            .as_ref()
            .is_some_and(|hovered| {
                self.command_marks
                    .iter()
                    .take(remove_count)
                    .any(|mark| &mark.command_id == hovered)
            });
        self.command_marks.drain(..remove_count);
        let collapsed_count = self.collapsed_command_ids.len();
        Arc::make_mut(&mut self.collapsed_command_ids).retain(|command_id| {
            self.command_marks
                .iter()
                .any(|mark| mark.command_id == *command_id)
        });
        if self.collapsed_command_ids.len() != collapsed_count {
            self.rebuild_command_fold_projection();
        }
        self.command_marks_render_cache_dirty = true;
        if removed_selected {
            self.selected_command_mark_id = None;
        }
        if removed_hovered {
            self.hovered_command_mark_id = None;
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
                    if !self.value.trim().is_empty() {
                        completed_command = Some(self.value.clone());
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

fn command_mark_allows_cwd_fallback(exit_code: Option<i32>) -> bool {
    // Some shells do not expose exit status through shell integration or user
    // input observation. Treat an absent status as an unknown-but-complete
    // command boundary, while still rejecting explicit failures.
    matches!(exit_code, None | Some(0))
}

pub(super) fn cwd_after_simple_cd_command(
    command: &str,
    current_cwd: Option<&str>,
) -> Option<String> {
    let words = split_simple_cd_words(command.trim())?;
    let [program, args @ ..] = words.as_slice() else {
        return None;
    };
    if program.value != "cd" || program.quoted {
        return None;
    }
    let target = match args {
        [] => SimpleCdWord::unquoted("~"),
        [target] => target.clone(),
        [flag, target] if flag.value == "--" && !flag.quoted => target.clone(),
        _ => return None,
    };
    resolve_cd_target(current_cwd, &target.value, !target.quoted)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SimpleCdWord {
    value: String,
    quoted: bool,
}

impl SimpleCdWord {
    fn unquoted(value: &str) -> Self {
        Self {
            value: value.to_string(),
            quoted: false,
        }
    }
}

fn split_simple_cd_words(command: &str) -> Option<Vec<SimpleCdWord>> {
    if command.is_empty() {
        return None;
    }

    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;
    let mut started = false;
    let mut quoted_word = false;
    for ch in command.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            started = true;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            started = true;
            continue;
        }
        if let Some(active_quote) = quote {
            if ch == active_quote {
                quote = None;
                started = true;
            } else if active_quote == '"' && matches!(ch, '$' | '`') {
                return None;
            } else {
                current.push(ch);
                started = true;
            }
            continue;
        }
        if matches!(ch, '\'' | '"') {
            quote = Some(ch);
            started = true;
            quoted_word = true;
            continue;
        }
        if ch.is_whitespace() {
            if started {
                push_simple_cd_word(&mut words, &mut current, quoted_word)?;
                started = false;
                quoted_word = false;
            }
            continue;
        }
        if is_simple_cd_shell_meta(ch) || ch.is_control() {
            return None;
        }
        current.push(ch);
        started = true;
    }
    if escaped || quote.is_some() {
        return None;
    }
    if started {
        push_simple_cd_word(&mut words, &mut current, quoted_word)?;
    }
    Some(words)
}

fn push_simple_cd_word(
    words: &mut Vec<SimpleCdWord>,
    current: &mut String,
    quoted: bool,
) -> Option<()> {
    if current.is_empty() {
        return None;
    }
    words.push(SimpleCdWord {
        value: std::mem::take(current),
        quoted,
    });
    Some(())
}

fn is_simple_cd_shell_meta(ch: char) -> bool {
    // Compound commands, redirections, expansions and globs depend on shell
    // evaluation. Leave those to OSC7/shell integration instead of guessing.
    matches!(
        ch,
        ';' | '|' | '&' | '<' | '>' | '`' | '$' | '(' | ')' | '{' | '}' | '*'
            | '?' | '[' | ']' | '!' | '#'
    )
}

fn resolve_cd_target(
    current_cwd: Option<&str>,
    target: &str,
    tilde_expansion_allowed: bool,
) -> Option<String> {
    let target = target.trim();
    if target.is_empty()
        || target == "-"
        || target == "--"
        || target.chars().any(char::is_control)
        || (target.starts_with('~')
            && (!tilde_expansion_allowed
                || (target != "~" && !target.starts_with("~/"))))
    {
        return None;
    }

    let raw = if matches!(target, "" | "~") || target.starts_with("~/") {
        return None;
    } else if target.starts_with('/') {
        target.to_string()
    } else {
        // Relative cd tracking is only authoritative after a shell-integrated
        // or process-owned absolute cwd exists. Guessing from "~" makes remote
        // Git/SFTP probes look valid while they still lack a real path.
        join_posix_display_path(current_cwd?, target)?
    };
    normalize_posix_display_path(&raw)
}

fn join_posix_display_path(base: &str, relative: &str) -> Option<String> {
    let base = base.trim_end_matches('/');
    if base.is_empty() || !base.starts_with('/') {
        return None;
    }
    let joined = if base == "/" {
        format!("/{relative}")
    } else {
        format!("{base}/{relative}")
    };
    Some(joined)
}

fn normalize_posix_display_path(path: &str) -> Option<String> {
    let (prefix, body) = if path == "~" {
        return Some("~".to_string());
    } else if let Some(rest) = path.strip_prefix("~/") {
        ("~", rest)
    } else if let Some(rest) = path.strip_prefix('/') {
        ("/", rest)
    } else {
        return None;
    };

    let mut parts: Vec<&str> = Vec::new();
    for part in body.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            parts.pop();
        } else {
            parts.push(part);
        }
    }

    if prefix == "~" {
        if parts.is_empty() {
            Some("~".to_string())
        } else {
            Some(format!("~/{}", parts.join("/")))
        }
    } else if parts.is_empty() {
        Some("/".to_string())
    } else {
        Some(format!("/{}", parts.join("/")))
    }
}

#[cfg(test)]
mod input_tracker_tests {
    use super::{TerminalInputTracker, command_mark_allows_cwd_fallback, cwd_after_simple_cd_command};

    #[test]
fn input_tracker_handles_submission_editing_state_and_reset_sequences() {
        let mut tracker = TerminalInputTracker::default();

        assert_eq!(tracker.apply_bytes(b"pwd"), None);
        assert_eq!(tracker.apply_bytes(b"\r"), Some("pwd".to_string()));
        assert_eq!(tracker.apply_bytes(b"\r"), None);

        assert_eq!(tracker.apply_bytes(b"  printf 'a  b'  "), None);
        assert_eq!(
            tracker.apply_bytes(b"\r"),
            Some("  printf 'a  b'  ".to_string())
        );

        let mut tracker = TerminalInputTracker::default();

        assert_eq!(tracker.apply_bytes(b"abc"), None);
        assert_eq!(tracker.apply_bytes(b"\x1b[D"), None);
        assert_eq!(tracker.apply_bytes(b"\x7f"), None);
        assert_eq!(tracker.apply_bytes(b"Z\r"), Some("aZc".to_string()));

        let mut tracker = TerminalInputTracker::default();

        assert_eq!(tracker.apply_bytes(b"abc"), None);
        assert_eq!(tracker.apply_bytes(b"\x1b[D"), None);
        assert_eq!(tracker.apply_bytes(b"Z"), None);

        let state = tracker.state();
        assert_eq!(state.value, "abZc");
        assert_eq!(state.cursor_index, 3);
        assert!(!state.is_cursor_at_end);

        let mut tracker = TerminalInputTracker::default();

        assert_eq!(tracker.apply_bytes(b"rm -rf"), None);
        assert_eq!(tracker.apply_bytes(b"\x03"), None);
        assert_eq!(tracker.apply_bytes(b"\r"), None);

        assert_eq!(tracker.apply_bytes(b"echo before"), None);
        assert_eq!(tracker.apply_bytes(b"\x1b[200~pasted\x1b[201~"), None);
        assert_eq!(tracker.apply_bytes(b"\r"), None);
    }

    #[test]
    fn cd_cwd_action_resolves_literal_paths_and_rejects_shell_dependent_forms() {
        let cases = [
            ("cd /var/log/../tmp", Some("/home/lipsc"), Some("/var/tmp")),
            (
                "cd .oxideterm",
                Some("/home/lipsc"),
                Some("/home/lipsc/.oxideterm"),
            ),
            (
                "cd ..",
                Some("/home/lipsc/OxideTerm"),
                Some("/home/lipsc"),
            ),
            ("cd", Some("/home/lipsc"), None),
            (
                "cd 'dir with spaces'",
                Some("/home/lipsc"),
                Some("/home/lipsc/dir with spaces"),
            ),
            (
                "cd -- 'dir with spaces'",
                Some("/tmp"),
                Some("/tmp/dir with spaces"),
            ),
            ("cd '~/literal space'", Some("/home/lipsc"), None),
            ("cd $HOME/project", Some("/tmp"), None),
            ("cd main && ls", Some("/tmp"), None),
            ("cd -", Some("/tmp"), None),
            ("cd ~root", Some("/tmp"), None),
            ("cd src", None, None),
            ("cd src", Some("~"), None),
            ("cd /tmp/src", None, Some("/tmp/src")),
        ];

        for (command, cwd, expected) in cases {
            assert_eq!(cwd_after_simple_cd_command(command, cwd).as_deref(), expected);
        }
    }

    #[test]
    fn cd_cwd_fallback_accepts_unknown_status_but_rejects_failures() {
        assert!(command_mark_allows_cwd_fallback(None));
        assert!(command_mark_allows_cwd_fallback(Some(0)));
        assert!(!command_mark_allows_cwd_fallback(Some(1)));
        assert!(!command_mark_allows_cwd_fallback(Some(127)));
    }
}
