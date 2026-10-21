use super::*;
use gpui::{AnyElement, MouseButton, div, prelude::*, rgb};
use std::collections::HashSet;
use std::ops::Range;

#[derive(Clone, PartialEq, Eq)]
struct FoldedOutput {
    command_id: String,
    lines: Range<usize>,
}

#[derive(Default)]
pub(super) struct CommandFolding {
    // Pane-local presentation state; the emulator, recording and copied output stay intact.
    collapsed: HashSet<String>,
    ranges: Vec<FoldedOutput>,
    snapshot: Option<TerminalSnapshot>,
}

impl CommandFolding {
    pub(super) fn visible_line(&self, physical: usize) -> usize {
        let mut removed = 0;
        for fold in &self.ranges {
            if physical <= fold.lines.start {
                break;
            }
            removed += physical.min(fold.lines.end - 1) - fold.lines.start;
        }
        physical.saturating_sub(removed)
    }

    fn physical_line(&self, visible: usize) -> usize {
        let mut physical = visible;
        for fold in &self.ranges {
            if physical <= fold.lines.start {
                break;
            }
            physical += fold.lines.len() - 1;
        }
        physical
    }

    fn history(&self, snapshot: &TerminalSnapshot) -> usize {
        self.visible_line(snapshot.scrollback_lines + snapshot.rows)
            .saturating_sub(snapshot.rows)
    }

    pub(super) fn offset(&self, snapshot: &TerminalSnapshot) -> usize {
        self.history(snapshot).saturating_sub(
            self.visible_line(
                snapshot
                    .scrollback_lines
                    .saturating_sub(snapshot.display_offset),
            ),
        )
    }
}

impl TerminalPane {
    pub(super) fn collapsed_command_ids(&self) -> HashSet<String> {
        self.command_folding.collapsed.clone()
    }

    pub(super) fn command_gutter_contains(&self, position: Point<Pixels>) -> bool {
        let Some(bounds) = self.bounds.filter(|bounds| bounds.contains(&position)) else {
            return false;
        };
        let left = self.timestamp_gutter_width() - f32::from(self.horizontal_scroll_offset_px);
        let x = f32::from(position.x - bounds.origin.x) - TERMINAL_CONTENT_PADDING;
        let y = f32::from(position.y - bounds.origin.y) - TERMINAL_CONTENT_PADDING;
        x >= left
            && x < left + self.command_mark_gutter_width()
            && y >= 0.0
            && y < self.snapshot.rows as f32 * self.metrics.line_height_f32()
    }

    pub(super) fn render_command_gutter_controls(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        if !self.preferences.command_folding_enabled
            || !self.command_actions_available(self.render_mode)
        {
            return Vec::new();
        }
        let offset = self.smooth_scroll_display_offset();
        let base = offset.ceil() as usize;
        let top = self.visible_scroll_history().saturating_sub(base);
        let y_offset = (offset - base as f32) * self.metrics.line_height_f32();
        self.command_marks
            .iter()
            .filter_map(|mark| {
                self.foldable_output(mark)?;
                let row = self
                    .command_folding
                    .visible_line(mark.start_line)
                    .checked_sub(top)?;
                if row > self.snapshot.rows {
                    return None;
                }
                let id = mark.command_id.clone();
                Some(
                    div()
                        .absolute()
                        .left(px(TERMINAL_CONTENT_PADDING + self.timestamp_gutter_width()
                            - f32::from(self.horizontal_scroll_offset_px)))
                        .top(px(y_offset + row as f32 * self.metrics.line_height_f32()))
                        .w(self.metrics.cell_width)
                        .h(self.metrics.line_height)
                        .cursor_pointer()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, window, cx| {
                                window.prevent_default();
                                cx.stop_propagation();
                                window.focus(&this.focus_handle, cx);
                                this.toggle_command_fold(&id, cx);
                            }),
                        )
                        .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .into_any_element(),
                )
            })
            .collect()
    }

    pub(super) fn command_actions_available(&self, mode: TermMode) -> bool {
        !self.command_reflow_pending
            && self.last_pty_resize.is_none_or(|(cols, rows, _, _)| {
                (self.snapshot.cols, self.snapshot.rows) == (cols, rows)
            })
            && command_mark_ui_available(true, mode)
    }

    fn foldable_output(&self, mark: &TerminalCommandMark) -> Option<Range<usize>> {
        if !self.preferences.command_folding_enabled
            || !self.command_actions_available(self.render_mode)
            || mark.stale
            || mark.command_line_clipped
            || !mark.is_closed
            || mark.output_confidence != TerminalCommandMarkConfidence::High
            || mark.closed_by != Some(TerminalCommandMarkClosedBy::ShellIntegration)
        {
            return None;
        }
        let start = mark.output_start?;
        let end = mark.output_end?;
        // Partial boundary rows may contain the input or the following prompt.
        // Keep them visible rather than hiding cells outside the output range.
        let first =
            (start.line + usize::from(start.column != 0)).max(mark.command_line.saturating_add(1));
        let last = end.line;
        (last > first && last <= self.snapshot.scrollback_lines + self.snapshot.rows)
            .then_some(first..last)
    }

    pub(super) fn command_fold_action(&self, id: &str) -> Option<String> {
        let mark = self
            .command_marks
            .iter()
            .find(|mark| mark.command_id == id)?;
        self.foldable_output(mark)?;
        let labels = &self.preferences.command_selection_labels;
        Some(if self.command_folding.collapsed.contains(id) {
            labels.expand_output.clone()
        } else {
            labels.collapse_output.clone()
        })
    }

    pub(super) fn refresh_command_folding(&mut self) {
        if !self.preferences.command_folding_enabled {
            return;
        }
        if self.command_reflow_pending {
            self.command_folding.ranges.clear();
            self.command_folding.snapshot = None;
            return;
        }
        if self.command_folding.collapsed.is_empty() {
            self.command_folding.ranges.clear();
            self.command_folding.snapshot = None;
            return;
        }
        let mut ranges = self
            .command_marks
            .iter()
            .filter_map(|mark| {
                self.command_folding
                    .collapsed
                    .contains(&mark.command_id)
                    .then(|| {
                        self.foldable_output(mark).map(|lines| FoldedOutput {
                            command_id: mark.command_id.clone(),
                            lines,
                        })
                    })
                    .flatten()
            })
            .collect::<Vec<_>>();
        ranges.sort_by_key(|fold| (fold.lines.start, std::cmp::Reverse(fold.lines.end)));
        let mut end = 0;
        ranges.retain(|fold| {
            if fold.lines.start < end {
                return false;
            }
            end = fold.lines.end;
            true
        });
        self.command_folding.collapsed.retain(|id| {
            self.command_marks
                .iter()
                .any(|mark| &mark.command_id == id && !mark.stale && !mark.command_line_clipped)
        });
        if self.command_folding.ranges == ranges
            && self
                .command_folding
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.generation == self.snapshot.generation)
        {
            return;
        }
        self.command_folding.ranges = ranges;
        self.command_folding.snapshot = if self.command_folding.ranges.is_empty() {
            None
        } else {
            self.folded_snapshot(
                self.command_folding.offset(&self.snapshot),
                self.snapshot.rows,
            )
        };
        if !self.command_folding.ranges.is_empty() && self.command_folding.snapshot.is_none() {
            // A background parser moved the buffer during segment reads. Render the
            // coherent original frame and refresh, rather than hiding unrelated rows.
            self.command_folding.ranges.clear();
            self.snapshot_dirty = true;
        }
    }

    pub(super) fn displayed_snapshot(&self) -> &TerminalSnapshot {
        self.command_folding
            .snapshot
            .as_ref()
            .unwrap_or(&self.snapshot)
    }

    pub(super) fn folding_active(&self) -> bool {
        !self.command_folding.ranges.is_empty()
    }

    pub(super) fn visible_scroll_history(&self) -> usize {
        self.command_folding.history(&self.snapshot)
    }

    pub(super) fn visible_scroll_offset(&self) -> usize {
        self.command_folding.offset(&self.snapshot)
    }

    pub(super) fn physical_scroll_offset(&self, visible_offset: usize) -> usize {
        if visible_offset == 0 {
            return 0;
        }
        let top = self.visible_scroll_history().saturating_sub(visible_offset);
        self.snapshot
            .scrollback_lines
            .saturating_sub(self.command_folding.physical_line(top))
    }

    pub(super) fn folded_snapshot(&self, offset: usize, rows: usize) -> Option<TerminalSnapshot> {
        let map = &self.command_folding;
        let history = map.history(&self.snapshot);
        let offset = offset.min(history);
        let top = history - offset;
        let total = self.snapshot.scrollback_lines + self.snapshot.rows;
        let mut result = self.snapshot.clone();
        result.lines.clear();
        result.images.clear();
        result.scrollback_lines = history;
        result.display_offset = offset;
        let cursor = self.absolute_cursor_line();
        result.cursor_row = map
            .visible_line(cursor)
            .checked_sub(top)
            .unwrap_or(usize::MAX);
        let mut visible = top;
        while result.lines.len() < rows {
            let physical = map.physical_line(visible);
            if physical >= total {
                break;
            }
            if map.ranges.iter().any(|fold| fold.lines.start == physical) {
                let mut row = self
                    .snapshot
                    .lines
                    .first()
                    .cloned()
                    .expect("terminal has rows");
                row.cells = Arc::new(Vec::new());
                row.line_id = 0;
                row.source_id = 0;
                row.wrapped = false;
                row.active_input = false;
                row.absolute_line = visible as i64 - history as i64;
                row.refresh_signature();
                result.lines.push(row);
                visible += 1;
                continue;
            }
            let next_fold = map
                .ranges
                .iter()
                .find(|fold| fold.lines.start > physical)
                .map_or(total, |fold| fold.lines.start);
            let count = (next_fold - physical).min(rows - result.lines.len());
            // Read contiguous visible segments only, never materialize hidden scrollback.
            let raw_top = self
                .snapshot
                .scrollback_lines
                .saturating_sub(self.snapshot.display_offset);
            let segment =
                if physical >= raw_top && physical + count <= raw_top + self.snapshot.lines.len() {
                    self.snapshot.clone()
                } else {
                    let display_offset = self.snapshot.scrollback_lines.saturating_sub(physical);
                    let skip = physical.saturating_sub(self.snapshot.scrollback_lines);
                    self.terminal
                        .lock()
                        .snapshot_with_display_offset(display_offset, skip + count)
                };
            let segment_top = segment
                .scrollback_lines
                .saturating_sub(segment.display_offset);
            if segment.scrollback_lines != self.snapshot.scrollback_lines
                || segment.cols != self.snapshot.cols
            {
                return None;
            }
            let skip = physical.saturating_sub(segment_top);
            for image in segment.images {
                let first = segment_top + image.row;
                let last = first + image.rows;
                if first >= physical
                    && first < physical + count
                    && !map
                        .ranges
                        .iter()
                        .any(|fold| first < fold.lines.end && last > fold.lines.start)
                {
                    let mut image = image;
                    image.row = map.visible_line(first).saturating_sub(top);
                    result.images.push(image);
                }
            }
            let before = result.lines.len();
            result
                .lines
                .extend(segment.lines.into_iter().skip(skip).take(count));
            let added = result.lines.len() - before;
            if added == 0 {
                break;
            }
            visible += added;
        }
        Some(result)
    }

    pub(super) fn displayed_absolute_line(&self, row: usize) -> usize {
        let snapshot = self.displayed_snapshot();
        self.command_folding.physical_line(
            snapshot
                .scrollback_lines
                .saturating_sub(snapshot.display_offset)
                + row,
        )
    }

    pub(super) fn physical_grid_point(&self, point: TerminalPoint) -> Option<TerminalGridPoint> {
        (point.row
            < self
                .displayed_snapshot()
                .lines
                .len()
                .min(self.snapshot.rows))
        .then(|| TerminalGridPoint {
            line: self.displayed_absolute_line(point.row) as i32
                - self.snapshot.scrollback_lines as i32,
            col: point.col,
        })
    }

    pub(super) fn physical_terminal_point(&self, point: TerminalPoint) -> Option<TerminalPoint> {
        let grid = self.physical_grid_point(point)?;
        let row = grid.line as i64 + self.snapshot.display_offset as i64;
        Some(TerminalPoint {
            row: usize::try_from(row).ok()?,
            col: point.col,
        })
    }

    pub(super) fn physical_selection(&self, mut selection: TerminalSelection) -> TerminalSelection {
        let history = self.visible_scroll_history() as i64;
        for point in [&mut selection.anchor, &mut selection.head] {
            let visible = (history + i64::from(point.line)).max(0) as usize;
            point.line = self.command_folding.physical_line(visible) as i32
                - self.snapshot.scrollback_lines as i32;
        }
        selection
    }

    pub(super) fn displayed_selection(&self) -> Option<TerminalSelection> {
        let mut selection = self.selection?;
        for point in [&mut selection.anchor, &mut selection.head] {
            point.line = self.displayed_grid_line(point.line);
        }
        Some(selection)
    }

    fn displayed_grid_line(&self, line: i32) -> i32 {
        let physical = (self.snapshot.scrollback_lines as i64 + i64::from(line)).max(0) as usize;
        self.command_folding.visible_line(physical) as i32 - self.visible_scroll_history() as i32
    }

    pub(super) fn displayed_search_matches(
        &self,
        matches: Arc<[TerminalSearchMatch]>,
    ) -> Arc<[TerminalSearchMatch]> {
        if !self.folding_active() {
            return matches;
        }
        matches
            .iter()
            .cloned()
            .map(|mut found| {
                found.ranges.retain(|range| {
                    let physical = (self.snapshot.scrollback_lines as i64 + i64::from(range.line))
                        .max(0) as usize;
                    !self
                        .command_folding
                        .ranges
                        .iter()
                        .any(|fold| fold.lines.contains(&physical))
                });
                for range in &mut found.ranges {
                    range.line = self.displayed_grid_line(range.line);
                }
                found.line = self.displayed_grid_line(found.line);
                found
            })
            .collect()
    }

    pub(super) fn displayed_command_mark(
        &self,
        mut mark: TerminalCommandMark,
    ) -> TerminalCommandMark {
        let map = &self.command_folding;
        mark.start_line = map.visible_line(mark.start_line);
        mark.command_line = map.visible_line(mark.command_line);
        mark.end_line = mark.end_line.map(|line| map.visible_line(line));
        for position in [&mut mark.output_start, &mut mark.output_end]
            .into_iter()
            .flatten()
        {
            position.line = map.visible_line(position.line);
        }
        mark
    }

    pub(super) fn toggle_command_fold(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.command_fold_action(id).is_none() {
            return;
        }
        let anchor = self.displayed_absolute_line(0);
        if !self.command_folding.collapsed.remove(id) {
            self.command_folding.collapsed.insert(id.to_owned());
        }
        self.clear_smooth_scroll_remainder();
        self.clear_terminal_interactions(cx);
        self.set_selection(None);
        self.refresh_command_folding();
        let top = self
            .command_folding
            .visible_line(anchor)
            .min(self.visible_scroll_history());
        let offset = self.physical_scroll_offset(self.visible_scroll_history() - top);
        let snapshot = {
            let mut terminal = self.terminal.lock();
            terminal.scroll_to_display_offset(offset);
            terminal.snapshot()
        };
        self.snapshot = self.stamp_snapshot(snapshot);
        self.refresh_command_folding();
        cx.notify();
    }

    pub(super) fn expand_folds_for_search(&mut self, found: &TerminalSearchMatch) {
        let history = self.snapshot.scrollback_lines as i64;
        self.command_folding.collapsed.retain(|id| {
            !self.command_folding.ranges.iter().any(|fold| {
                &fold.command_id == id && {
                    let line = (history + i64::from(found.line)).max(0) as usize;
                    fold.lines.contains(&line)
                        || found.ranges.iter().any(|range| {
                            fold.lines
                                .contains(&((history + i64::from(range.line)).max(0) as usize))
                        })
                }
            })
        });
        self.refresh_command_folding();
    }

    pub(super) fn render_fold_summaries(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let snapshot = self.displayed_snapshot();
        let offset = self.smooth_scroll_display_offset();
        let base_offset = offset.ceil() as usize;
        let top = snapshot.scrollback_lines.saturating_sub(base_offset);
        let y_offset = (offset - base_offset as f32) * self.metrics.line_height_f32();
        self.command_folding
            .ranges
            .iter()
            .filter_map(|fold| {
                let row = self
                    .command_folding
                    .visible_line(fold.lines.start)
                    .checked_sub(top)?;
                if row > snapshot.rows {
                    return None;
                }
                let id = fold.command_id.clone();
                let labels = &self.preferences.command_selection_labels;
                let mut label = labels
                    .folded_output
                    .replace("{{count}}", &fold.lines.len().to_string());
                let failure = self
                    .command_marks
                    .iter()
                    .find(|mark| mark.command_id == id)
                    .and_then(|mark| mark.exit_code)
                    .filter(|code| *code != 0);
                if let Some(code) = failure {
                    label.push_str(" · ");
                    label.push_str(
                        &labels
                            .folded_exit_code
                            .replace("{{code}}", &code.to_string()),
                    );
                }
                let y = y_offset + row as f32 * self.metrics.line_height_f32();
                Some(
                    div()
                        .absolute()
                        .left(px(self.terminal_content_padding_x()
                            - f32::from(self.horizontal_scroll_offset_px)))
                        .right(px(SCROLLBAR_RESERVED_WIDTH))
                        .top(px(y))
                        .h(self.metrics.line_height)
                        .line_height(self.metrics.line_height)
                        .text_size(self.metrics.font_size)
                        .text_color(rgb(if failure.is_some() {
                            self.theme.tokens.ui.error
                        } else {
                            self.theme.tokens.ui.text_muted
                        }))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .cursor_pointer()
                        .child(label)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, window, cx| {
                                window.prevent_default();
                                cx.stop_propagation();
                                this.toggle_command_fold(&id, cx);
                            }),
                        )
                        .into_any_element(),
                )
            })
            .collect()
    }
}
