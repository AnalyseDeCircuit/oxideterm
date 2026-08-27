use std::{collections::HashSet, ops::RangeInclusive, sync::Arc};

use oxideterm_terminal::{TerminalCommandMark, TerminalCommandMarkConfidence, TerminalSnapshot};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct HiddenCommandOutput {
    pub(crate) start: usize,
    pub(crate) end: usize,
    visible_start: usize,
    hidden_through: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct FoldViewportPosition {
    pub(crate) display_offset: usize,
    pub(crate) bottom_padding_rows: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct CommandFoldProjection {
    hidden_outputs: Vec<HiddenCommandOutput>,
    total_physical_lines: usize,
    viewport_rows: usize,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct CommandMarkRenderIndex {
    marks: Vec<TerminalCommandMark>,
    leaf_count: usize,
    maximum_end_lines: Vec<usize>,
}

impl CommandMarkRenderIndex {
    pub(crate) fn rebuild(&mut self, marks: &[TerminalCommandMark]) {
        self.marks.clear();
        self.marks.extend_from_slice(marks);
        self.marks.sort_by_key(|mark| mark.start_line);

        self.leaf_count = self.marks.len().next_power_of_two().max(1);
        self.maximum_end_lines.clear();
        self.maximum_end_lines
            .resize(self.leaf_count.saturating_mul(2), 0);
        for (index, mark) in self.marks.iter().enumerate() {
            self.maximum_end_lines[self.leaf_count + index] =
                mark.end_line.unwrap_or(usize::MAX).max(mark.start_line);
        }
        for node in (1..self.leaf_count).rev() {
            self.maximum_end_lines[node] =
                self.maximum_end_lines[node * 2].max(self.maximum_end_lines[node * 2 + 1]);
        }
    }

    pub(crate) fn visible_marks(
        &self,
        physical_ranges: &[RangeInclusive<usize>],
    ) -> Arc<[TerminalCommandMark]> {
        if self.marks.is_empty() || physical_ranges.is_empty() {
            return Arc::from([]);
        }

        let mut matching_indices = Vec::new();
        for range in physical_ranges {
            self.collect_overlapping_indices(
                1,
                0,
                self.leaf_count,
                *range.start(),
                *range.end(),
                &mut matching_indices,
            );
        }
        matching_indices.sort_unstable();
        matching_indices.dedup();
        matching_indices
            .into_iter()
            .map(|index| self.marks[index].clone())
            .collect::<Vec<_>>()
            .into()
    }

    fn collect_overlapping_indices(
        &self,
        node: usize,
        start_index: usize,
        end_index: usize,
        query_start: usize,
        query_end: usize,
        matches: &mut Vec<usize>,
    ) {
        if start_index >= self.marks.len()
            || self.maximum_end_lines[node] < query_start
            || self.marks[start_index].start_line > query_end
        {
            return;
        }
        if end_index - start_index == 1 {
            matches.push(start_index);
            return;
        }

        let middle = start_index + (end_index - start_index) / 2;
        self.collect_overlapping_indices(
            node * 2,
            start_index,
            middle,
            query_start,
            query_end,
            matches,
        );
        self.collect_overlapping_indices(
            node * 2 + 1,
            middle,
            end_index,
            query_start,
            query_end,
            matches,
        );
    }
}

pub(crate) fn visible_physical_ranges(snapshot: &TerminalSnapshot) -> Vec<RangeInclusive<usize>> {
    let total_physical_lines = snapshot.scrollback_lines.saturating_add(snapshot.rows);
    let mut ranges = Vec::new();
    let mut current_start = None;
    let mut previous_line = None;

    for row in &snapshot.lines {
        let physical_line = snapshot.scrollback_lines as i64 + row.absolute_line;
        let Ok(physical_line) = usize::try_from(physical_line) else {
            continue;
        };
        if physical_line >= total_physical_lines {
            continue;
        }
        if previous_line.is_some_and(|previous| physical_line == previous + 1) {
            previous_line = Some(physical_line);
            continue;
        }
        if let (Some(start), Some(end)) = (current_start.take(), previous_line.take()) {
            ranges.push(start..=end);
        }
        current_start = Some(physical_line);
        previous_line = Some(physical_line);
    }
    if let (Some(start), Some(end)) = (current_start, previous_line) {
        ranges.push(start..=end);
    }
    ranges
}

impl CommandFoldProjection {
    pub(crate) fn new(
        marks: &[TerminalCommandMark],
        collapsed_command_ids: &HashSet<String>,
        total_physical_lines: usize,
        viewport_rows: usize,
    ) -> Self {
        let mut hidden_outputs = marks
            .iter()
            .filter(|mark| collapsed_command_ids.contains(&mark.command_id))
            .filter_map(|mark| foldable_output_range(mark, total_physical_lines))
            .map(|range| HiddenCommandOutput {
                start: *range.start(),
                end: *range.end(),
                visible_start: 0,
                hidden_through: 0,
            })
            .collect::<Vec<_>>();
        hidden_outputs.sort_unstable_by_key(|range| (range.start, range.end));

        // Overlapping command metadata must never hide the same physical row twice.
        let mut merged: Vec<HiddenCommandOutput> = Vec::with_capacity(hidden_outputs.len());
        for range in hidden_outputs {
            if let Some(previous) = merged.last_mut()
                && range.start <= previous.end.saturating_add(1)
            {
                previous.end = previous.end.max(range.end);
            } else {
                merged.push(range);
            }
        }

        let mut hidden_before = 0usize;
        for range in &mut merged {
            range.visible_start = range.start.saturating_sub(hidden_before);
            hidden_before = hidden_before
                .saturating_add(range.end.saturating_sub(range.start).saturating_add(1));
            range.hidden_through = hidden_before;
        }

        Self {
            hidden_outputs: merged,
            total_physical_lines,
            viewport_rows,
        }
    }

    pub(crate) fn is_active(&self) -> bool {
        !self.hidden_outputs.is_empty()
    }

    pub(crate) fn visual_history_rows(&self) -> usize {
        self.visible_line_count().saturating_sub(self.viewport_rows)
    }

    pub(crate) fn clamp_display_offset(&self, display_offset: usize) -> usize {
        display_offset.min(self.visual_history_rows())
    }

    pub(crate) fn grid_lines_for_viewport(
        &self,
        display_offset: usize,
        bottom_padding_rows: usize,
    ) -> Vec<i32> {
        let display_offset = self.clamp_display_offset(display_offset);
        let bottom_padding_rows = bottom_padding_rows.min(self.viewport_rows.saturating_sub(1));
        let visible_lines = self.visible_line_count();
        let content_rows = self.viewport_rows.saturating_sub(bottom_padding_rows);
        let start_visual_line =
            visible_lines.saturating_sub(content_rows.saturating_add(display_offset));
        let end_visual_line = start_visual_line
            .saturating_add(content_rows)
            .min(visible_lines);
        let scrollback_lines = self.total_physical_lines.saturating_sub(self.viewport_rows);
        let mut grid_lines = Vec::with_capacity(self.viewport_rows);

        if start_visual_line < end_visual_line {
            let mut physical_line = self.physical_line_for_visual_index(start_visual_line);
            let mut hidden_index = self
                .hidden_outputs
                .partition_point(|range| range.end < physical_line);
            for _ in start_visual_line..end_visual_line {
                grid_lines.push(relative_grid_line(physical_line, scrollback_lines));
                physical_line = physical_line.saturating_add(1);
                while let Some(range) = self.hidden_outputs.get(hidden_index)
                    && physical_line >= range.start
                {
                    physical_line = physical_line.max(range.end.saturating_add(1));
                    hidden_index += 1;
                }
            }
        }

        // A newly created terminal can have fewer document rows after folding than its viewport.
        // Keep GPUI layout height stable by padding below the projected document with blank rows.
        let screen_rows = self.viewport_rows;
        while grid_lines.len() < self.viewport_rows {
            let padding_row = screen_rows.saturating_add(grid_lines.len());
            grid_lines.push(padding_row.min(i32::MAX as usize) as i32);
        }
        grid_lines
    }

    pub(crate) fn viewport_position_for_anchor(
        &self,
        physical_line: usize,
        viewport_row: usize,
    ) -> FoldViewportPosition {
        let Some(visual_line) = self.visual_index_for_physical_line(physical_line) else {
            return FoldViewportPosition {
                display_offset: self.visual_history_rows(),
                bottom_padding_rows: 0,
            };
        };
        let viewport_start = visual_line.saturating_sub(viewport_row);
        let natural_bottom_start = self.visible_line_count().saturating_sub(self.viewport_rows);
        if viewport_start >= natural_bottom_start {
            FoldViewportPosition {
                display_offset: 0,
                bottom_padding_rows: viewport_start
                    .saturating_sub(natural_bottom_start)
                    .min(self.viewport_rows.saturating_sub(1)),
            }
        } else {
            FoldViewportPosition {
                display_offset: natural_bottom_start.saturating_sub(viewport_start),
                bottom_padding_rows: 0,
            }
        }
    }

    fn visible_line_count(&self) -> usize {
        let hidden_lines = self
            .hidden_outputs
            .last()
            .map_or(0, |range| range.hidden_through);
        self.total_physical_lines.saturating_sub(hidden_lines)
    }

    fn physical_line_for_visual_index(&self, visual_line: usize) -> usize {
        let hidden_index = self
            .hidden_outputs
            .partition_point(|range| range.visible_start <= visual_line);
        let hidden_before = hidden_index
            .checked_sub(1)
            .and_then(|index| self.hidden_outputs.get(index))
            .map_or(0, |range| range.hidden_through);
        visual_line
            .saturating_add(hidden_before)
            .min(self.total_physical_lines)
    }

    fn visual_index_for_physical_line(&self, physical_line: usize) -> Option<usize> {
        if physical_line >= self.total_physical_lines {
            return None;
        }
        let hidden_index = self
            .hidden_outputs
            .partition_point(|range| range.start <= physical_line);
        if let Some(range) = hidden_index
            .checked_sub(1)
            .and_then(|index| self.hidden_outputs.get(index))
        {
            if physical_line <= range.end {
                return None;
            }
            return Some(physical_line.saturating_sub(range.hidden_through));
        }
        Some(physical_line)
    }

    pub(crate) fn update_viewport(&mut self, total_physical_lines: usize, viewport_rows: usize) {
        self.total_physical_lines = total_physical_lines;
        self.viewport_rows = viewport_rows;
    }
}

pub(crate) fn foldable_output_range(
    mark: &TerminalCommandMark,
    total_physical_lines: usize,
) -> Option<RangeInclusive<usize>> {
    if !mark.is_closed || mark.output_confidence != TerminalCommandMarkConfidence::High {
        return None;
    }
    let start = mark.output_start_line?;
    let end = mark.end_line?.min(total_physical_lines.saturating_sub(1));
    (start <= end && start > mark.command_line).then_some(start..=end)
}

fn relative_grid_line(physical_line: usize, scrollback_lines: usize) -> i32 {
    let relative = physical_line as i64 - scrollback_lines as i64;
    relative.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxideterm_terminal::{TerminalCommandMarkClosedBy, TerminalCommandMarkDetectionSource};

    fn closed_mark(
        command_id: &str,
        command_line: usize,
        output: RangeInclusive<usize>,
    ) -> TerminalCommandMark {
        TerminalCommandMark {
            command_id: command_id.to_string(),
            command: Some("printf test".to_string()),
            start_line: command_line,
            command_line,
            output_start_line: Some(*output.start()),
            end_line: Some(*output.end()),
            is_closed: true,
            closed_by: Some(TerminalCommandMarkClosedBy::ShellIntegration),
            exit_code: Some(0),
            duration_ms: Some(1),
            detection_source: TerminalCommandMarkDetectionSource::ShellIntegration,
            submitted_by: None,
            confidence: TerminalCommandMarkConfidence::High,
            output_confidence: TerminalCommandMarkConfidence::High,
            stale: false,
            started_at: 1,
            finished_at: Some(2),
        }
    }

    #[test]
    fn collapsed_output_is_removed_without_removing_command_header() {
        let marks = [closed_mark("one", 3, 4..=7)];
        let collapsed = HashSet::from(["one".to_string()]);
        let projection = CommandFoldProjection::new(&marks, &collapsed, 12, 6);

        assert_eq!(projection.visual_history_rows(), 2);
        assert_eq!(
            projection.grid_lines_for_viewport(2, 0),
            vec![-6, -5, -4, -3, 2, 3]
        );
        assert_eq!(
            projection.grid_lines_for_viewport(0, 0),
            vec![-4, -3, 2, 3, 4, 5]
        );
    }

    #[test]
    fn overlapping_metadata_hides_each_output_row_once() {
        let marks = [closed_mark("one", 2, 3..=6), closed_mark("two", 4, 5..=8)];
        let collapsed = HashSet::from(["one".to_string(), "two".to_string()]);
        let projection = CommandFoldProjection::new(&marks, &collapsed, 12, 4);

        assert_eq!(projection.visual_history_rows(), 2);
        assert_eq!(projection.grid_lines_for_viewport(0, 0), vec![-6, 1, 2, 3]);
    }

    #[test]
    fn render_index_finds_visible_marks_from_unordered_overlapping_metadata() {
        let marks = [
            closed_mark("later", 20, 21..=25),
            closed_mark("overlap", 8, 9..=14),
            closed_mark("earlier", 3, 4..=10),
        ];
        let mut index = CommandMarkRenderIndex::default();
        index.rebuild(&marks);

        let visible = index.visible_marks(&[9..=9, 22..=23]);
        let ids = visible
            .iter()
            .map(|mark| mark.command_id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(ids, vec!["earlier", "overlap", "later"]);
    }

    #[test]
    fn anchor_offset_keeps_header_at_requested_viewport_row() {
        let marks = [closed_mark("one", 5, 6..=12)];
        let collapsed = HashSet::from(["one".to_string()]);
        let projection = CommandFoldProjection::new(&marks, &collapsed, 20, 8);
        let position = projection.viewport_position_for_anchor(5, 2);

        assert_eq!(
            projection
                .grid_lines_for_viewport(position.display_offset, position.bottom_padding_rows)[2],
            -7
        );
    }

    #[test]
    fn anchor_near_bottom_uses_padding_instead_of_pulling_old_history_into_view() {
        let marks = [closed_mark("one", 15, 16..=18)];
        let collapsed = HashSet::from(["one".to_string()]);
        let projection = CommandFoldProjection::new(&marks, &collapsed, 20, 10);
        let position = projection.viewport_position_for_anchor(15, 5);

        assert_eq!(position.display_offset, 0);
        assert_eq!(position.bottom_padding_rows, 3);
        assert_eq!(
            projection.grid_lines_for_viewport(0, position.bottom_padding_rows)[5],
            5
        );
    }
}
