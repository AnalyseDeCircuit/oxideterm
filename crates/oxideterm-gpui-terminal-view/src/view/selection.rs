use gpui::Context;
use oxideterm_terminal_emulator::TerminalSelectionRange;

use super::TerminalView;
use crate::selection::{
    TerminalGridPoint, TerminalSelection, TerminalSelectionMode, selected_text_for_selection,
    snapshot_request_for_selection,
};

impl TerminalView {
    pub fn selected_text(&self) -> Option<String> {
        let selection = self.selection?;
        let Some(request) = snapshot_request_for_selection(&self.snapshot, selection) else {
            return selected_text_for_selection(&self.snapshot, selection)
                .filter(|s| !s.is_empty());
        };

        // The emulator exposes viewport snapshots. Page only the requested range on
        // copy instead of maintaining a second scrollback buffer as output arrives.
        let mut emulator = self.emulator.borrow_mut();
        let original = self.snapshot.display_offset;
        let mut offset = request.display_offset;
        // scroll_display emits only a cursor wakeup here: feed/resize have already
        // drained peer events, and this synchronous read never publishes its pages.
        drop(emulator.scroll_display(offset as i32 - original as i32));
        let mut excerpt = emulator.snapshot();
        for start in (excerpt.lines.len()..request.rows).step_by(self.size.rows) {
            let line = start as i32 - request.display_offset as i32;
            let next_offset = (-line).max(0) as usize;
            drop(emulator.scroll_display(next_offset as i32 - offset as i32));
            offset = next_offset;
            let page = emulator.snapshot();
            let first = (line + offset as i32) as usize;
            excerpt.lines.extend(
                page.lines
                    .into_iter()
                    .skip(first)
                    .take(request.rows - start),
            );
        }
        drop(emulator.scroll_display(original as i32 - offset as i32));
        excerpt.rows = excerpt.lines.len();
        selected_text_for_selection(&excerpt, selection).filter(|s| !s.is_empty())
    }

    pub fn clear_selection(&mut self, cx: &mut Context<Self>) {
        if self.selection.is_some() {
            self.set_selection(None);
            self.pointer.selecting = false;
            cx.notify();
        }
    }

    pub(super) fn set_selection(&mut self, selection: Option<TerminalSelection>) {
        self.selection = selection;
        self.emulator.get_mut().set_selection(selection.map(|s| {
            let (a, b) = s.normalized();
            let block = s.mode == TerminalSelectionMode::Block;
            TerminalSelectionRange {
                start_line: a.line,
                end_line: b.line,
                start_col: if block { a.col.min(b.col) } else { a.col },
                end_col: if block { a.col.max(b.col) } else { b.col },
                is_block: block,
            }
        }));
    }

    pub(super) fn sync_selection(&mut self) {
        let Some(s) = self.selection else { return };
        let Some(range) = self.emulator.get_mut().selection() else {
            self.selection = None;
            self.pointer.selecting = false;
            return;
        };
        // The grid tracks scroll and erase, but normalizes the ends. Restore the
        // user's drag direction (independently on each axis for rectangular selection).
        let (mut a, mut b) = (
            TerminalGridPoint {
                line: range.start_line,
                col: range.start_col,
            },
            TerminalGridPoint {
                line: range.end_line,
                col: range.end_col,
            },
        );
        if (s.anchor.line, s.anchor.col) > (s.head.line, s.head.col) {
            std::mem::swap(&mut a, &mut b);
        }
        if range.is_block && (a.col > b.col) != (s.anchor.col > s.head.col) {
            std::mem::swap(&mut a.col, &mut b.col);
        }
        self.selection = Some(TerminalSelection {
            anchor: a,
            head: b,
            mode: s.mode,
        });
    }
}
