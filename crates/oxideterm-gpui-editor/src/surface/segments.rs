// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::HashMap, sync::Arc};

use gpui::{Font, WindowTextSystem};
use oxideterm_editor_core::{BufferOffset, TextRange};
use unicode_segmentation::UnicodeSegmentation;

use super::{TextEditorView, coords::grapheme_visual_width};

const SEGMENT_BYTES: usize = 2048;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SegmentOrigin {
    pub byte: usize,
    pub column: usize,
    pub x: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px, size};
    use oxideterm_editor_core::Selection;

    #[gpui::test]
    fn horizontal_segments_preserve_caret_hit_testing_search_and_history(
        cx: &mut gpui::TestAppContext,
    ) {
        let source = format!("{}中文e\u{301}👨‍👩‍👧‍👦tail", "0123456789 ".repeat(16_384));
        let byte = 12_345;
        let (editor, cx) = cx.add_window_view(|_, cx| {
            let mut editor =
                TextEditorView::new(source.clone(), &oxideterm_theme::default_tokens(), cx);
            // Integral headless advances avoid the no-op shaper's long-line accumulation drift.
            editor.apply_runtime_settings(
                &oxideterm_theme::default_tokens(),
                "monospace".into(),
                10.0,
                1.2,
                false,
                false,
                cx,
            );
            editor
        });
        cx.simulate_resize(size(px(800.0), px(400.0)));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        cx.update(|window, app| {
            editor.update(app, |editor, cx| {
                let segments = editor
                    .line_segments(0)
                    .expect("headless glyphs permit segmentation");
                let index = segments.at_byte(byte);
                assert!(index > 0);
                let expected_x = f32::from(
                    editor
                        .shape_coordinate_line(
                            &source[..byte],
                            &WindowTextSystem::new(editor.text_system.clone()),
                        )
                        .width(),
                );
                editor.viewport.scroll_x_px = expected_x - 100.0;
                let bounds = editor.content_bounds.unwrap();
                let caret = editor.bounds_for_byte_offset(BufferOffset(byte), bounds, window);
                assert!(
                    (f32::from(caret.left() - bounds.left())
                        - editor.visible_gutter_width()
                        - editor.visible_content_padding_x()
                        - 100.0)
                        .abs()
                        < 0.01
                );
                assert_eq!(
                    editor.offset_for_window_point(
                        point(caret.left(), caret.top() + px(1.0)),
                        window
                    ),
                    Some(BufferOffset(byte))
                );
                editor.set_find_case_sensitive(true, cx);
                editor.set_find_query("中文e\u{301}👨‍👩‍👧‍👦", cx);
                editor
                    .cursor
                    .set_selection(Selection::caret(BufferOffset(byte)));
                editor.insert_text("中", cx);
                editor.undo(cx);
                assert_eq!(editor.buffer.text(), source);
            })
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        editor.read_with(cx, |editor, _| {
            let hit = editor.find_matches.get(0).unwrap();
            assert_eq!(hit.range.start.0..hit.range.end.0, 180224..source.len() - 4);
            assert!(
                editor.highlight_chunk_cache.borrow().retained_bytes
                    < super::super::HighlightChunkCache::BYTE_BUDGET
            );
        });
    }
}

pub(super) struct LineSegments {
    pub starts: Vec<SegmentOrigin>,
    pub len: usize,
}

impl LineSegments {
    pub fn at_byte(&self, byte: usize) -> usize {
        self.starts
            .partition_point(|part| part.byte <= byte)
            .saturating_sub(1)
    }

    pub fn at_x(&self, x: f32) -> usize {
        self.starts
            .partition_point(|part| part.x <= x)
            .saturating_sub(1)
    }

    pub fn end(&self, index: usize) -> usize {
        self.starts
            .get(index + 1)
            .map_or(self.len, |part| part.byte)
    }
}

#[derive(Default)]
pub(super) struct SegmentCache {
    version: u64,
    font: Option<(Font, f32, usize)>,
    pub lines: HashMap<usize, Option<Arc<LineSegments>>>,
}

impl TextEditorView {
    pub(super) fn line_segments(&self, line: usize) -> Option<Arc<LineSegments>> {
        let start = self.buffer.line_start_offset(line)?.0;
        let end = self.buffer.line_end_offset(line)?.0;
        // Preedit inserts display-only text. The composing row's existing
        // layout owns those shifted coordinates until the composition commits.
        if self
            .marked_text
            .as_ref()
            .is_some_and(|marked| marked.range.start.0 >= start && marked.range.start.0 <= end)
        {
            return None;
        }
        if end - start <= SEGMENT_BYTES * 2 {
            return None;
        }
        let font = (
            crate::metrics::editor_code_font(
                &self.appearance.font_family,
                self.appearance.font_fallback_family.as_deref(),
                self.appearance.font_weight,
            ),
            self.metrics.font_size,
            self.text_system.font_generation(),
        );
        let mut cache = self.segment_cache.borrow_mut();
        if cache.version != self.buffer.version() || cache.font.as_ref() != Some(&font) {
            cache.lines.clear();
            cache.version = self.buffer.version();
            cache.font = Some(font);
        }
        cache
            .lines
            .entry(line)
            .or_insert_with(|| {
                self.buffer
                    .with_line_text(line, |text| self.measure_segments(text).map(Arc::new))
                    .flatten()
            })
            .clone()
    }

    pub(super) fn segment_text(&self, line: usize, start: usize, end: usize) -> String {
        let base = self.buffer.line_start_offset(line).unwrap().0;
        self.buffer
            .slice(TextRange::new(
                BufferOffset(base + start),
                BufferOffset(base + end),
            ))
            .unwrap()
    }

    fn measure_segments(&self, text: &str) -> Option<LineSegments> {
        // Control characters can be rendered as replacement markers or tabs;
        // their display geometry is not represented by the raw text layout.
        if text.chars().any(char::is_control) {
            return None;
        }
        // Establish exact native geometry once. Only compact boundaries survive;
        // neither this text system nor the full glyph array enters the window cache.
        let system = WindowTextSystem::new(self.text_system.clone());
        let whole = self.shape_coordinate_line(text, &system);
        if !whole
            .runs
            .iter()
            .flat_map(|run| run.glyphs.iter().map(|glyph| glyph.index))
            .is_sorted()
        {
            return None;
        }
        let mut ends = Vec::new();
        let mut start = 0;
        let mut columns = 0;
        for (byte, grapheme) in text.grapheme_indices(true) {
            if byte - start >= SEGMENT_BYTES {
                ends.push((byte, columns));
                start = byte;
            }
            columns += grapheme_visual_width(grapheme);
        }
        ends.push((text.len(), columns));
        let mut original = whole
            .runs
            .iter()
            .flat_map(|run| run.glyphs.iter().map(move |glyph| (run.font_id, glyph)))
            .peekable();
        let mut starts = Vec::with_capacity(ends.len());
        let mut origin = SegmentOrigin::default();
        for (end, column_end) in ends {
            let system = WindowTextSystem::new(self.text_system.clone());
            let part = self.shape_coordinate_line(&text[origin.byte..end], &system);
            // Contextual shaping and ligatures may cross a grapheme boundary.
            // Segment only when the actual font produces the same glyphs and positions.
            for run in &part.runs {
                for glyph in &run.glyphs {
                    let (font, expected) = original.next()?;
                    if font != run.font_id
                        || expected.id != glyph.id
                        || expected.index != origin.byte + glyph.index
                        || !same_position(
                            f32::from(expected.position.x),
                            origin.x + f32::from(glyph.position.x),
                        )
                        || expected.position.y != glyph.position.y
                        || expected.is_emoji != glyph.is_emoji
                    {
                        return None;
                    }
                }
            }
            let next_x = original
                .peek()
                .map_or(f32::from(whole.width()), |(_, glyph)| {
                    f32::from(glyph.position.x)
                });
            if next_x < origin.x || !same_position(next_x, origin.x + f32::from(part.width())) {
                return None;
            }
            starts.push(origin);
            origin = SegmentOrigin {
                byte: end,
                column: column_end,
                x: next_x,
            };
        }
        if original.next().is_some() {
            return None;
        }
        Some(LineSegments {
            starts,
            len: text.len(),
        })
    }
}

fn same_position(left: f32, right: f32) -> bool {
    // Global pixel coordinates lose fractional precision far from the origin.
    // Permit only arithmetic rounding, not a font-dependent shaping difference.
    (left - right).abs() <= (left.abs().max(right.abs()) * f32::EPSILON * 2.0).max(0.001)
}
