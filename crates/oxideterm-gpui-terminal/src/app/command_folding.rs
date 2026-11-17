use super::*;
use gpui::{AnyElement, MouseButton, div, prelude::*, rgb, rgba};
use oxideterm_gpui_ui::motion::{
    MotionDuration, duration, ease_out_cubic, exit_settle_duration, fade, fade_in,
};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::time::{Duration, Instant};

#[derive(Clone, PartialEq, Eq)]
struct FoldedOutput {
    command_id: String,
    lines: Range<usize>,
}

struct ExpandFoldHighlight {
    command_id: String,
    generation: u64,
}

#[derive(Clone, Copy)]
struct FoldMarkerRotation {
    started_at: Instant,
    from: f32,
    to: f32,
    duration: Duration,
}

impl FoldMarkerRotation {
    fn progress_at(self, now: Instant) -> f32 {
        let total = self.duration.as_secs_f32();
        if total <= 0.0 {
            return self.to;
        }
        let elapsed = now.saturating_duration_since(self.started_at).as_secs_f32();
        let t = ease_out_cubic((elapsed / total).clamp(0.0, 1.0));
        self.from + (self.to - self.from) * t
    }

    fn is_finished(self, now: Instant) -> bool {
        self.duration.is_zero() || now.saturating_duration_since(self.started_at) >= self.duration
    }
}

#[derive(Default)]
pub(super) struct CommandFolding {
    // Pane-local presentation state; the emulator, recording and copied output stay intact.
    collapsed: HashSet<String>,
    ranges: Vec<FoldedOutput>,
    snapshot: Option<TerminalSnapshot>,
    fold_epochs: HashMap<String, u64>,
    next_epoch: u64,
    expand_highlight: Option<ExpandFoldHighlight>,
    marker_rotations: HashMap<String, FoldMarkerRotation>,
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

    pub(super) fn advance_command_fold_animations(&mut self, now: Instant) -> bool {
        self.command_folding
            .marker_rotations
            .retain(|_, rotation| !rotation.is_finished(now));
        !self.command_folding.marker_rotations.is_empty()
    }

    pub(super) fn command_fold_marker_rotations(&self, now: Instant) -> HashMap<String, f32> {
        self.command_folding
            .marker_rotations
            .iter()
            .map(|(id, rotation)| (id.clone(), rotation.progress_at(now)))
            .collect()
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
        {
            return None;
        }
        crate::terminal_view::command_mark_foldable_range(
            mark,
            self.snapshot.scrollback_lines + self.snapshot.rows,
        )
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
        let collapsed = &self.command_folding.collapsed;
        self.command_folding
            .fold_epochs
            .retain(|id, _| collapsed.contains(id));
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
        result.logical_context.clear();
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
                    self.terminal.lock().snapshot_with_display_offset(
                        display_offset,
                        skip + count,
                        Some(&self.snapshot),
                    )
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
            for context in &segment.logical_context {
                let rows = context
                    .rows
                    .iter()
                    .filter(|mapped| {
                        segment
                            .lines
                            .iter()
                            .skip(skip)
                            .take(count)
                            .any(|row| row.source_id == mapped.source_id)
                    })
                    .map(|row| oxideterm_terminal::TerminalLogicalContextRow {
                        source_id: row.source_id,
                        columns: row.columns.clone(),
                    })
                    .collect::<Vec<_>>();
                if !rows.is_empty() {
                    result.logical_context.push(Arc::new(
                        oxideterm_terminal::TerminalLogicalContext {
                            text: context.text.clone(),
                            signature: context.signature,
                            cols: context.cols,
                            members: context.members.clone(),
                            start_line: map.visible_line(context.start_line),
                            end_line: map.visible_line(context.end_line),
                            active_input: context.active_input,
                            start_at_history_boundary: context.start_at_history_boundary,
                            rows,
                        },
                    ));
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
        let now = Instant::now();
        let was_collapsed = self.command_folding.collapsed.contains(id);
        let current_rotation = self
            .command_folding
            .marker_rotations
            .get(id)
            .map(|rotation| rotation.progress_at(now))
            .unwrap_or(if was_collapsed { 1.0 } else { 0.0 });
        self.command_folding.next_epoch = self.command_folding.next_epoch.wrapping_add(1);
        let epoch = self.command_folding.next_epoch;
        let collapsed_now = !was_collapsed;
        if collapsed_now {
            self.command_folding.collapsed.insert(id.to_owned());
            self.command_folding
                .fold_epochs
                .insert(id.to_owned(), epoch);
            if self
                .command_folding
                .expand_highlight
                .as_ref()
                .is_some_and(|highlight| highlight.command_id == id)
            {
                self.command_folding.expand_highlight = None;
            }
        } else {
            self.command_folding.collapsed.remove(id);
            self.command_folding.fold_epochs.remove(id);
            if self.theme.tokens.motion.enabled {
                self.command_folding.expand_highlight = Some(ExpandFoldHighlight {
                    command_id: id.to_owned(),
                    generation: epoch,
                });
                let delay = exit_settle_duration(&self.theme.tokens, MotionDuration::Control);
                cx.spawn(async move |weak, cx| {
                    cx.background_executor().timer(delay).await;
                    let _ = weak.update(cx, |this, cx| {
                        if this
                            .command_folding
                            .expand_highlight
                            .as_ref()
                            .is_some_and(|highlight| highlight.generation == epoch)
                        {
                            this.command_folding.expand_highlight = None;
                            cx.notify();
                        }
                    });
                })
                .detach();
            } else {
                self.command_folding.expand_highlight = None;
            }
        }
        if self.theme.tokens.motion.enabled && self.theme.tokens.motion.spatial_enabled {
            let marker_duration = duration(&self.theme.tokens, MotionDuration::Micro);
            if !marker_duration.is_zero() {
                self.command_folding.marker_rotations.insert(
                    id.to_owned(),
                    FoldMarkerRotation {
                        started_at: now,
                        from: current_rotation,
                        to: if collapsed_now { 1.0 } else { 0.0 },
                        duration: marker_duration,
                    },
                );
            } else {
                self.command_folding.marker_rotations.remove(id);
            }
        } else {
            self.command_folding.marker_rotations.remove(id);
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
        let collapsed = &self.command_folding.collapsed;
        self.command_folding
            .fold_epochs
            .retain(|id, _| collapsed.contains(id));
        self.refresh_command_folding();
    }

    pub(super) fn render_fold_summaries(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let snapshot = self.displayed_snapshot();
        let offset = self.smooth_scroll_display_offset();
        let base_offset = offset.ceil() as usize;
        let top = snapshot.scrollback_lines.saturating_sub(base_offset);
        let y_offset = (offset - base_offset as f32) * self.metrics.line_height_f32();
        let line_height = self.metrics.line_height_f32();
        let tokens = &self.theme.tokens;
        let left_px =
            self.terminal_content_padding_x() - f32::from(self.horizontal_scroll_offset_px);
        let mut elements: Vec<AnyElement> = self
            .command_folding
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
                let epoch = self
                    .command_folding
                    .fold_epochs
                    .get(&id)
                    .copied()
                    .unwrap_or(0);
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
                let text_color = rgb(if failure.is_some() {
                    tokens.ui.error
                } else {
                    tokens.ui.text_muted
                });
                let hover_color = rgb(if failure.is_some() {
                    tokens.ui.error
                } else {
                    tokens.ui.text_secondary
                });
                let summary_id = SharedString::from(format!("terminal-fold-summary-{epoch}-{id}"));
                let summary = div()
                    .h(self.metrics.line_height)
                    .line_height(self.metrics.line_height)
                    .text_size(self.metrics.font_size)
                    .text_color(text_color)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .cursor_pointer()
                    .hover(move |style| style.text_color(hover_color))
                    .child(label)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            this.toggle_command_fold(&id, cx);
                        }),
                    );
                let y = y_offset + row as f32 * line_height;
                Some(
                    div()
                        .absolute()
                        .left(px(left_px))
                        .right_0()
                        .top(px(y))
                        .h(self.metrics.line_height)
                        .child(fade_in(tokens, summary_id, summary, MotionDuration::Micro))
                        .into_any_element(),
                )
            })
            .collect();

        if let Some(highlight) = &self.command_folding.expand_highlight
            && let Some(mark) = self
                .command_marks
                .iter()
                .find(|mark| mark.command_id == highlight.command_id)
            && let Some(range) = self.foldable_output(mark)
        {
            let visible_start = self.command_folding.visible_line(range.start);
            let visible_end = self.command_folding.visible_line(range.end - 1);
            let viewport_end = top.saturating_add(snapshot.rows.saturating_sub(1));
            if visible_end >= top && visible_start <= viewport_end {
                let start_row = visible_start.max(top) - top;
                let end_row = visible_end.min(viewport_end) - top;
                let band_top = y_offset + start_row as f32 * line_height;
                let band_height = (end_row - start_row + 1) as f32 * line_height;
                let band = div()
                    .size_full()
                    .rounded(px(tokens.radii.sm.max(2.0)))
                    .bg(rgba((tokens.ui.accent << 8) | 0x16));
                elements.push(
                    div()
                        .absolute()
                        .left(px(left_px))
                        .right_0()
                        .top(px(band_top))
                        .h(px(band_height))
                        .child(fade(
                            tokens,
                            SharedString::from(format!(
                                "terminal-fold-expand-{}",
                                highlight.generation
                            )),
                            band,
                            MotionDuration::Control,
                            false,
                        ))
                        .into_any_element(),
                );
            }
        }

        elements
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    use oxideterm_terminal::{
        TerminalCommandMarkClosedBy, TerminalCommandMarkConfidence,
        TerminalCommandMarkDetectionSource, TerminalCommandPosition,
    };

    fn foldable_mark(id: &str) -> TerminalCommandMark {
        TerminalCommandMark {
            command_id: id.to_string(),
            command: Some("ls -la".to_string()),
            start_line: 0,
            command_line: 0,
            command_line_clipped: false,
            output_start: Some(TerminalCommandPosition { line: 1, column: 0 }),
            output_end: Some(TerminalCommandPosition { line: 4, column: 0 }),
            end_line: Some(4),
            is_closed: true,
            closed_by: Some(TerminalCommandMarkClosedBy::ShellIntegration),
            exit_code: Some(0),
            duration_ms: Some(12),
            detection_source: TerminalCommandMarkDetectionSource::ShellIntegration,
            submitted_by: None,
            confidence: TerminalCommandMarkConfidence::High,
            output_confidence: TerminalCommandMarkConfidence::High,
            stale: false,
            started_at: 1,
            finished_at: Some(2),
        }
    }

    #[gpui::test]
    fn toggle_command_fold_tracks_epochs_and_motion_gated_expand_highlight(
        cx: &mut TestAppContext,
    ) {
        let (pane, cx) = cx.add_window_view(|window, cx| {
            TerminalPane::new_recording_playback(
                80,
                10,
                TerminalUiPreferences {
                    command_folding_enabled: true,
                    ..Default::default()
                },
                window,
                cx,
            )
            .expect("test terminal pane")
        });

        pane.update(cx, |pane, cx| {
            pane.command_marks.push(foldable_mark("cmd-1"));
            pane.theme.tokens.motion.enabled = true;

            pane.toggle_command_fold("cmd-1", cx);
            assert!(pane.command_folding.collapsed.contains("cmd-1"));
            assert_eq!(pane.command_folding.fold_epochs.get("cmd-1"), Some(&1));
            assert!(pane.command_folding.expand_highlight.is_none());
            assert!(pane.command_folding.marker_rotations.contains_key("cmd-1"));
            assert_eq!(
                pane.command_folding.marker_rotations["cmd-1"].to, 1.0,
                "collapsing rotates square 0 -> 45deg"
            );

            pane.toggle_command_fold("cmd-1", cx);
            assert!(!pane.command_folding.collapsed.contains("cmd-1"));
            assert!(!pane.command_folding.fold_epochs.contains_key("cmd-1"));
            assert_eq!(
                pane.command_folding
                    .expand_highlight
                    .as_ref()
                    .map(|highlight| highlight.command_id.as_str()),
                Some("cmd-1")
            );
            assert_eq!(
                pane.command_folding.marker_rotations["cmd-1"].to, 0.0,
                "expanding rotates square back 45deg -> 0"
            );
        });

        cx.executor()
            .advance_clock(std::time::Duration::from_secs(1));
        cx.run_until_parked();
        pane.update(cx, |pane, _cx| {
            assert!(
                pane.command_folding.expand_highlight.is_none(),
                "expand highlight should clear after its settle timer"
            );
            assert!(
                !pane.advance_command_fold_animations(
                    Instant::now() + std::time::Duration::from_secs(1)
                ),
                "completed marker rotation animation must prune cleanly"
            );
        });

        pane.update(cx, |pane, cx| {
            pane.theme.tokens.motion.enabled = false;
            pane.toggle_command_fold("cmd-1", cx);
            pane.toggle_command_fold("cmd-1", cx);
            assert!(
                pane.command_folding.expand_highlight.is_none(),
                "disabled motion must not schedule an expand reveal wash"
            );
            assert!(
                pane.command_folding.marker_rotations.is_empty(),
                "disabled motion must snap marker rotation directly"
            );
        });
    }
}

#[cfg(test)]
mod semantic_context_tests {
    use super::*;
    use gpui::TestAppContext;

    #[gpui::test]
    fn folded_output_preserves_physical_wrapped_context_and_maps_its_coordinates(
        cx: &mut TestAppContext,
    ) {
        let (pane, cx) = cx.add_window_view(|window, cx| {
            TerminalPane::new_recording_playback(10, 8, Default::default(), window, cx).unwrap()
        });
        pane.update(cx, |pane, _| {
            {
                let mut terminal = pane.terminal.lock();
                terminal
                    .feed_recording_output(b"hide1\r\nhide2\r\n'abcdefghijklmnopqrstuvwx'\r\nDONE");
                pane.snapshot = terminal.snapshot();
            }
            pane.command_folding.ranges = vec![FoldedOutput {
                command_id: "hidden-output".into(),
                lines: 0..2,
            }];
            let folded = pane.folded_snapshot(0, 8).unwrap();
            assert!(folded.logical_context_for_row(0).is_none());
            assert_eq!(folded.lines[1].text(), "'abcdefghi");
            let context = folded.logical_context_for_row(1).unwrap();
            assert_eq!(context.text.trim_end(), "'abcdefghijklmnopqrstuvwx'");
            assert_eq!((context.start_line, context.end_line), (1, 3));
            assert_eq!(context.rows[0].columns[0], (0, 0));
            assert_eq!(context.rows[1].columns[0], (10, 0));
            assert_eq!(context.rows[2].columns[0], (20, 0));
        });
    }
}
