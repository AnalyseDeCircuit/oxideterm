use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EditorScrollOrigin {
    User,
    Synchronization,
    #[default]
    Layout,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EditorScrollAnchor {
    pub version: u64,
    pub source_position: f64,
    pub viewport_y: f32,
    pub at_start: bool,
    pub at_end: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct EditorViewportChanged {
    pub anchor: EditorScrollAnchor,
    pub origin: EditorScrollOrigin,
}

impl gpui::EventEmitter<EditorViewportChanged> for TextEditorView {}

impl TextEditorView {
    pub fn track_edit_changes(&mut self) {
        self.buffer.track_changes(true);
    }
    pub fn take_edit_changes(&mut self) -> Vec<oxideterm_editor_core::BufferChange> {
        self.buffer.take_changes()
    }

    pub fn scroll_anchor(&self) -> EditorScrollAnchor {
        let rows = self.display_rows();
        let y = self.viewport.scroll_y_px;
        let index = (y / self.metrics.line_height).floor().max(0.0) as usize;
        let position = rows
            .get(index.min(rows.len().saturating_sub(1)))
            .map(|row| {
                let line = self.buffer.line_text(row.line).unwrap_or_default();
                let start = self
                    .buffer
                    .line_start_offset(row.line)
                    .map_or(0, |offset| offset.0);
                let a = start + byte_column_for_visual_column(&line, row.start_col);
                let fraction = (y / self.metrics.line_height).fract() as f64;
                if fraction == 0.0 {
                    return a as f64;
                }
                let b = start + byte_column_for_visual_column(&line, row.end_col);
                a as f64 + fraction * b.saturating_sub(a).max(1) as f64
            })
            .unwrap_or(0.0);
        let maximum =
            (rows.len() as f32 * self.metrics.line_height - self.viewport.height_px).max(0.0);
        EditorScrollAnchor {
            version: self.buffer.version(),
            source_position: position,
            viewport_y: 0.0,
            at_start: y <= 0.5,
            at_end: maximum > 0.5 && y >= maximum - 0.5,
        }
    }

    pub fn scroll_to_source(&mut self, anchor: EditorScrollAnchor, cx: &mut Context<Self>) {
        if anchor.version != self.buffer.version() {
            return;
        }
        self.scroll_origin = EditorScrollOrigin::Synchronization;
        self.restore_scroll_anchor(anchor);
        cx.notify();
    }

    pub(super) fn restore_scroll_anchor(&mut self, anchor: EditorScrollAnchor) {
        let rows = self.display_rows();
        let maximum =
            (rows.len() as f32 * self.metrics.line_height - self.viewport.height_px).max(0.0);
        let position = anchor.source_position.clamp(0.0, self.buffer.len() as f64);
        let mut target = maximum;
        for (index, row) in rows.iter().enumerate() {
            let line = self.buffer.line_text(row.line).unwrap_or_default();
            let start = self
                .buffer
                .line_start_offset(row.line)
                .map_or(0, |offset| offset.0);
            let a = start + byte_column_for_visual_column(&line, row.start_col);
            if position <= a as f64 {
                target = index as f32 * self.metrics.line_height - anchor.viewport_y;
                break;
            }
            let b = start + byte_column_for_visual_column(&line, row.end_col);
            if row.is_folded_header {
                let next = rows
                    .get(index + 1)
                    .and_then(|row| self.buffer.line_start_offset(row.line))
                    .map_or(self.buffer.len(), |offset| offset.0);
                if position >= a as f64 && position < next as f64 {
                    target = index as f32 * self.metrics.line_height - anchor.viewport_y;
                    break;
                }
            }
            if position < b as f64 {
                let fraction =
                    ((position - a as f64) / b.saturating_sub(a).max(1) as f64).clamp(0.0, 1.0);
                target =
                    (index as f32 + fraction as f32) * self.metrics.line_height - anchor.viewport_y;
                break;
            }
        }
        self.viewport.scroll_y_px = if anchor.at_start {
            0.0
        } else if anchor.at_end {
            maximum
        } else {
            target.clamp(0.0, maximum)
        };
    }

    pub(super) fn publish_viewport(&mut self, cx: &mut Context<Self>) {
        let current = (
            self.buffer.version(),
            self.viewport.scroll_y_px,
            self.viewport.width_px,
            self.viewport.height_px,
        );
        if self.last_published_scroll == Some(current) {
            return;
        }
        self.last_published_scroll = Some(current);
        cx.emit(EditorViewportChanged {
            anchor: self.scroll_anchor(),
            origin: self.scroll_origin,
        });
        self.scroll_origin = EditorScrollOrigin::Layout;
    }
}
