use std::{env, time::Duration};

use gpui::{
    ClipboardItem, Context, KeyDownEvent, KeyUpEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, ScrollWheelEvent, Timer, TouchPhase, px,
};
use oxideterm_terminal::{TermMode, TerminalSearchMatch};
use oxideterm_terminal_unicode::visual_line_for_row;

use super::{ScrollbarDrag, ScrollbarGeometry, TerminalPane};
use crate::terminal_ui::*;
use crate::terminal_view::*;

impl TerminalPane {
    pub(crate) fn handle_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;

        if self.pending_paste.is_some() && !modifiers.platform && !modifiers.control {
            match key {
                "enter" => {
                    self.confirm_pending_paste(cx);
                    return;
                }
                "escape" => {
                    self.cancel_pending_paste(cx);
                    return;
                }
                _ => {}
            }
        }

        if modifiers.platform && modifiers.shift && key.eq_ignore_ascii_case("k") {
            let result = if modifiers.alt {
                self.terminal.lock().kill_active_task()
            } else {
                self.terminal.lock().terminate_active_task()
            };
            if result.is_ok() {
                cx.notify();
            }
            return;
        }

        if key == "end" && modifiers.platform {
            self.terminal.lock().scroll_to_bottom();
            self.snapshot = self.terminal.lock().snapshot();
            cx.notify();
            return;
        }

        if key == "home" && modifiers.platform {
            self.terminal.lock().scroll_to_top();
            self.snapshot = self.terminal.lock().snapshot();
            cx.notify();
            return;
        }

        if self.settings.smart_copy
            && is_smart_copy_shortcut(event)
            && self.copy_selection_to_clipboard_if_present(cx)
        {
            return;
        }

        if let Some(action) = oxideterm_terminal_scroll_action(&event.keystroke) {
            self.apply_scroll_action(action, cx);
            return;
        }

        let mode = self.terminal.lock().mode();
        let key_event_type = if event.is_held {
            KittyKeyEventType::Repeat
        } else {
            KittyKeyEventType::Press
        };
        if let Some(sequence) =
            oxideterm_key_escape_sequence(&event.keystroke, &mode, false, key_event_type)
        {
            self.send_user_protocol_bytes(sequence.as_bytes(), cx);
        }
    }

    pub(crate) fn handle_key_up(&mut self, event: &KeyUpEvent, cx: &mut Context<Self>) {
        let mode = self.terminal.lock().mode();
        if let Some(sequence) = oxideterm_key_escape_sequence(
            &event.keystroke,
            &mode,
            false,
            KittyKeyEventType::Release,
        ) {
            self.send_protocol_bytes(sequence.as_bytes(), cx);
        }
    }

    pub(crate) fn handle_scroll(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let mode = self.terminal.lock().mode();
        let scroll_multiplier = if mouse_mode(mode, event.modifiers.shift) {
            1.0
        } else {
            TERMINAL_SCROLL_MULTIPLIER
        };
        let Some(rows) = self.determine_scroll_lines(event, scroll_multiplier) else {
            return;
        };

        if mouse_mode(mode, event.modifiers.shift) {
            let point = self.terminal_point_for_position(event.position);
            let report_count = rows.unsigned_abs().max(1);
            if let Some(report) = mouse_scroll_report(point, event, mode) {
                for _ in 0..report_count {
                    self.send_protocol_bytes(&report, cx);
                }
            }
            return;
        }

        if mode.contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL)
            && !event.modifiers.shift
        {
            let bytes = alt_scroll(rows);
            self.send_protocol_bytes(&bytes, cx);
            return;
        }

        self.terminal
            .lock()
            .scroll_lines(terminal_scroll_delta(rows));
        self.snapshot = self.terminal.lock().snapshot();
        cx.notify();
    }

    fn determine_scroll_lines(
        &mut self,
        event: &ScrollWheelEvent,
        scroll_multiplier: f32,
    ) -> Option<i32> {
        match event.touch_phase {
            TouchPhase::Started => {
                self.scroll_px = px(0.0);
                None
            }
            TouchPhase::Moved => {
                let line_height = self.metrics.line_height;
                let old_offset = (self.scroll_px / line_height) as i32;
                self.scroll_px += event.delta.pixel_delta(line_height).y * scroll_multiplier;
                let new_offset = (self.scroll_px / line_height) as i32;
                self.scroll_px %=
                    px((self.snapshot.rows.max(1) as f32) * self.metrics.line_height_f32());
                Some(new_offset - old_offset).filter(|rows| *rows != 0)
            }
            TouchPhase::Ended => None,
        }
    }

    fn snapshot_text(&self) -> String {
        self.snapshot
            .lines
            .iter()
            .map(|row| row.text().trim_end().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn visible_text_snapshot(&self) -> String {
        self.snapshot_text()
    }

    fn copy_text(&self) -> String {
        self.selected_text()
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| self.snapshot_text())
    }

    pub(super) fn copy_current_selection_or_snapshot(&mut self, cx: &mut Context<Self>) {
        let had_selection = self
            .selection
            .is_some_and(|selection| !selection.is_empty());
        cx.write_to_clipboard(ClipboardItem::new_string(self.copy_text()));
        if had_selection && !self.settings.keep_selection_on_copy {
            self.selection = None;
            cx.notify();
        }
    }

    pub(super) fn copy_from_platform_shortcut(&mut self, cx: &mut Context<Self>) {
        if cfg!(target_os = "macos") {
            self.copy_current_selection_or_snapshot(cx);
            return;
        }

        if self.settings.smart_copy && self.copy_selection_to_clipboard_if_present(cx) {
            return;
        }

        self.send_user_protocol_bytes(&[0x03], cx);
    }

    fn copy_selection_after_select_if_configured(&mut self, cx: &mut Context<Self>) {
        if !self.settings.copy_on_select {
            return;
        }
        let Some(_) = self.selected_text().filter(|text| !text.is_empty()) else {
            return;
        };

        self.copy_on_select_generation = self.copy_on_select_generation.wrapping_add(1);
        let generation = self.copy_on_select_generation;
        cx.spawn(async move |weak, cx| {
            Timer::after(Duration::from_millis(120)).await;
            let _ = weak.update(cx, |this, cx| {
                if this.copy_on_select_generation != generation || !this.settings.copy_on_select {
                    return;
                }
                let Some(current_text) = this.selected_text().filter(|text| !text.is_empty())
                else {
                    return;
                };
                cx.write_to_clipboard(ClipboardItem::new_string(current_text));
                if !this.settings.keep_selection_on_copy {
                    this.selection = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn selected_text(&self) -> Option<String> {
        selected_text_for_selection(&self.snapshot, self.selection?)
    }

    pub fn selected_text_snapshot(&self) -> Option<String> {
        self.selected_text().filter(|text| !text.is_empty())
    }

    fn copy_selection_to_clipboard_if_present(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(text) = self.selected_text().filter(|text| !text.is_empty()) else {
            return false;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        if !self.settings.keep_selection_on_copy {
            self.selection = None;
            cx.notify();
        }
        true
    }

    fn terminal_point_for_position(&self, position: gpui::Point<Pixels>) -> TerminalPoint {
        let origin = self.content_origin();
        let col = ((f32::from(position.x - origin.x) - TERMINAL_CONTENT_PADDING)
            / self.metrics.cell_width_f32())
        .floor()
        .max(0.0) as usize;
        let row = ((f32::from(position.y - origin.y) - TERMINAL_CONTENT_PADDING)
            / self.metrics.line_height_f32())
        .floor()
        .max(0.0) as usize;

        let row = row.min(self.snapshot.rows.saturating_sub(1));
        let logical_col = self
            .snapshot
            .lines
            .get(row)
            .map(visual_line_for_row)
            .filter(|line| line.has_bidi)
            .map(|line| line.logical_col_for_visual_col(col))
            .unwrap_or(col);

        TerminalPoint {
            row,
            col: logical_col.min(self.snapshot.cols.saturating_sub(1)),
        }
    }

    fn link_at_position(&self, position: gpui::Point<Pixels>) -> Option<TerminalLinkRange> {
        let point = self.terminal_point_for_position(position);
        let cell = self
            .snapshot
            .lines
            .get(point.row)
            .and_then(|row| row.cells.get(point.col))?;

        display_link_ranges(&self.snapshot)
            .into_iter()
            .find(|link| {
                link.row == point.row
                    && point.col >= link.start_col
                    && point.col < link.end_col
                    && (cell.hyperlink.is_some() || is_link_stylable_cell(cell))
            })
    }

    fn scrollbar_geometry(&self) -> Option<ScrollbarGeometry> {
        terminal_scrollbar(&self.snapshot, &self.metrics).map(|scrollbar| {
            let origin = self.content_origin();
            let x = px(TERMINAL_CONTENT_PADDING
                + self.snapshot.cols as f32 * self.metrics.cell_width_f32()
                + SCROLLBAR_GAP);
            ScrollbarGeometry {
                x: origin.x + x,
                y: origin.y + px(TERMINAL_CONTENT_PADDING),
                top: px(scrollbar.top),
                height: px(scrollbar.height),
                track_height: px(self.snapshot.rows as f32 * self.metrics.line_height_f32()),
            }
        })
    }

    fn set_scrollbar_position(
        &mut self,
        position: gpui::Point<Pixels>,
        thumb_offset_y: Pixels,
        cx: &mut Context<Self>,
    ) {
        let Some(geometry) = self.scrollbar_geometry() else {
            return;
        };

        let available = (geometry.track_height - geometry.height).max(px(1.0));
        let y = (position.y - geometry.y - thumb_offset_y).clamp(px(0.0), available);
        let scroll_fraction = f32::from(y / available);
        let history = self.snapshot.scrollback_lines;
        let offset = ((1.0 - scroll_fraction) * history as f32).round() as usize;
        self.terminal.lock().scroll_to_display_offset(offset);
        self.snapshot = self.terminal.lock().snapshot();
        cx.notify();
    }

    fn start_selection(
        &mut self,
        position: gpui::Point<Pixels>,
        mode: TerminalSelectionMode,
        cx: &mut Context<Self>,
    ) {
        let point = self.terminal_point_for_position(position);
        let Some(point) = grid_point_for_viewport_point(&self.snapshot, point) else {
            return;
        };
        self.selection = Some(TerminalSelection {
            anchor: point,
            head: point,
            mode,
        });
        self.selecting = true;
        cx.notify();
    }

    fn select_word(&mut self, position: gpui::Point<Pixels>, cx: &mut Context<Self>) {
        let point = self.terminal_point_for_position(position);
        if let Some(selection) = word_selection_at_point(&self.snapshot, point) {
            self.selection = Some(selection);
            self.selecting = false;
            cx.notify();
        } else {
            self.start_selection(position, TerminalSelectionMode::Simple, cx);
        }
    }

    fn select_line(&mut self, position: gpui::Point<Pixels>, cx: &mut Context<Self>) {
        let point = self.terminal_point_for_position(position);
        if let Some(selection) = line_selection_at_point(&self.snapshot, point) {
            self.selection = Some(selection);
            self.selecting = false;
            cx.notify();
        } else {
            self.start_selection(position, TerminalSelectionMode::Simple, cx);
        }
    }

    fn update_selection(&mut self, position: gpui::Point<Pixels>, cx: &mut Context<Self>) {
        if !self.selecting {
            return;
        }

        let point = self.terminal_point_for_position(position);
        if let Some(selection) = &mut self.selection {
            if let Some(point) = grid_point_for_viewport_point(&self.snapshot, point) {
                selection.head = point;
            }
        }
        cx.notify();
    }

    fn finish_selection(&mut self, position: gpui::Point<Pixels>, cx: &mut Context<Self>) {
        self.update_selection(position, cx);
        self.selecting = false;
        self.copy_selection_after_select_if_configured(cx);
    }

    fn apply_scroll_action(&mut self, action: TerminalScrollAction, cx: &mut Context<Self>) {
        {
            let mut terminal = self.terminal.lock();
            match action {
                TerminalScrollAction::PageUp => terminal.page_up(),
                TerminalScrollAction::PageDown => terminal.page_down(),
                TerminalScrollAction::LineUp => terminal.scroll_lines(1),
                TerminalScrollAction::LineDown => terminal.scroll_lines(-1),
                TerminalScrollAction::Top => terminal.scroll_to_top(),
                TerminalScrollAction::Bottom => terminal.scroll_to_bottom(),
            }
        }
        self.snapshot = self.terminal.lock().snapshot();
        cx.notify();
    }

    fn current_search_matches(&self) -> Vec<TerminalSearchMatch> {
        self.search_query
            .as_deref()
            .map(|query| self.terminal.lock().search_matches(query))
            .unwrap_or_default()
    }

    pub(super) fn select_next_search_match(&mut self, forward: bool, cx: &mut Context<Self>) {
        let matches = self.current_search_matches();
        if matches.is_empty() {
            self.selected_search_match = None;
            cx.notify();
            return;
        }

        let current = self
            .selected_search_match
            .unwrap_or(0)
            .min(matches.len() - 1);
        self.selected_search_match = Some(if forward {
            (current + 1) % matches.len()
        } else if current == 0 {
            matches.len() - 1
        } else {
            current - 1
        });
        self.scroll_to_search_match(&matches[self.selected_search_match.unwrap()], cx);
    }

    pub(super) fn scroll_to_selected_search_match(&mut self, cx: &mut Context<Self>) {
        let matches = self.current_search_matches();
        let Some(index) = self
            .selected_search_match
            .filter(|index| *index < matches.len())
        else {
            return;
        };
        self.scroll_to_search_match(&matches[index], cx);
    }

    fn scroll_to_search_match(
        &mut self,
        search_match: &TerminalSearchMatch,
        cx: &mut Context<Self>,
    ) {
        let desired_row = (self.snapshot.rows / 3).max(1) as i32;
        let target_offset = desired_row.saturating_sub(search_match.line).max(0) as usize;
        self.terminal.lock().scroll_to_display_offset(target_offset);
        self.snapshot = self.terminal.lock().snapshot();
        cx.notify();
    }

    pub(crate) fn handle_mouse_down(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        if event.button == MouseButton::Left
            && event.modifiers.platform
            && let Some(link) = self.link_at_position(event.position)
        {
            match link.kind {
                TerminalLinkKind::Url => cx.open_url(&link.target),
                TerminalLinkKind::Path => {
                    if let Ok(base_dir) = env::current_dir()
                        && let Some(url) = path_link_to_file_url(&link.target, &base_dir)
                    {
                        cx.open_url(&url);
                    }
                }
            }
            return;
        }

        if event.button == MouseButton::Left
            && let Some(geometry) = self.scrollbar_geometry()
            && geometry.contains_track(event.position)
        {
            let thumb_offset_y = if geometry.contains_thumb(event.position) {
                event.position.y - geometry.y - geometry.top
            } else {
                geometry.height / 2.0
            };
            self.scrollbar_drag = Some(ScrollbarDrag { thumb_offset_y });
            self.set_scrollbar_position(event.position, thumb_offset_y, cx);
            return;
        }

        let mode = self.terminal.lock().mode();
        if event.button == MouseButton::Middle
            && self.settings.middle_click_paste
            && !mouse_tracking_active(mode)
        {
            return;
        }

        if mouse_mode(mode, event.modifiers.shift) {
            let point = self.terminal_point_for_position(event.position);
            self.last_mouse_report_point = Some(point);
            if let Some(report) =
                mouse_button_report(point, event.button, event.modifiers, true, mode)
            {
                self.send_protocol_bytes(&report, cx);
            }
        } else if self.selection_allowed(event.modifiers.shift) {
            match event.click_count {
                0 | 1 => self.start_selection(
                    event.position,
                    if event.modifiers.alt {
                        TerminalSelectionMode::Block
                    } else {
                        TerminalSelectionMode::Simple
                    },
                    cx,
                ),
                2 => self.select_word(event.position, cx),
                _ => self.select_line(event.position, cx),
            }
        } else {
            self.selecting = false;
            self.selection = None;
        }
    }

    pub(crate) fn handle_mouse_move(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let hovered_link = (!self.selecting && self.scrollbar_drag.is_none())
            .then(|| self.link_at_position(event.position))
            .flatten();
        if hovered_link != self.hovered_link {
            self.hovered_link = hovered_link;
            cx.notify();
        }

        if let Some(drag) = self.scrollbar_drag
            && event.pressed_button == Some(MouseButton::Left)
        {
            self.set_scrollbar_position(event.position, drag.thumb_offset_y, cx);
            return;
        }

        let mode = self.terminal.lock().mode();
        if mouse_mode(mode, event.modifiers.shift) {
            let point = self.terminal_point_for_position(event.position);
            if self.last_mouse_report_point == Some(point) {
                return;
            }
            self.last_mouse_report_point = Some(point);
            if let Some(report) =
                mouse_moved_report(point, event.pressed_button, event.modifiers, mode)
            {
                self.send_protocol_bytes(&report, cx);
            }
        } else if event.pressed_button == Some(MouseButton::Left)
            && self.selection_allowed(event.modifiers.shift)
        {
            self.update_selection(event.position, cx);
        }
    }

    pub(crate) fn handle_mouse_up(&mut self, event: &MouseUpEvent, cx: &mut Context<Self>) {
        if self.scrollbar_drag.take().is_some() {
            cx.notify();
            return;
        }

        let mode = self.terminal.lock().mode();
        if event.button == MouseButton::Middle
            && self.settings.middle_click_paste
            && !mouse_tracking_active(mode)
        {
            self.last_mouse_report_point = None;
            self.paste_from_clipboard(cx);
            return;
        }

        if mouse_mode(mode, event.modifiers.shift) {
            let point = self.terminal_point_for_position(event.position);
            self.last_mouse_report_point = None;
            if let Some(report) =
                mouse_button_report(point, event.button, event.modifiers, false, mode)
            {
                self.send_protocol_bytes(&report, cx);
            }
        } else if self.selection_allowed(event.modifiers.shift) {
            self.finish_selection(event.position, cx);
            if event.button == MouseButton::Left
                && self.selection.is_some_and(|selection| selection.is_empty())
            {
                self.select_command_mark_at_position(event.position, cx);
            }
            self.last_mouse_report_point = None;
        } else {
            self.selecting = false;
            self.last_mouse_report_point = None;
        }
    }

    fn selection_allowed(&self, shift: bool) -> bool {
        !self.settings.selection_requires_shift || shift
    }

    fn select_command_mark_at_position(
        &mut self,
        position: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if !self.settings.command_marks_enabled {
            return;
        }
        let point = self.terminal_point_for_position(position);
        let absolute_line = self
            .snapshot
            .scrollback_lines
            .saturating_add(point.row)
            .saturating_sub(self.snapshot.display_offset);
        let selected = self
            .command_marks
            .iter()
            .rev()
            .find(|mark| {
                let end_line = self.selectable_command_mark_end_line(mark);
                absolute_line >= mark.start_line && absolute_line <= end_line
            })
            .map(|mark| mark.command_id.clone());
        if self.selected_command_mark_id != selected {
            self.selected_command_mark_id = selected;
            cx.notify();
        }
    }
}

fn mouse_tracking_active(mode: TermMode) -> bool {
    mode.intersects(TermMode::MOUSE_MODE)
}

fn is_smart_copy_shortcut(event: &KeyDownEvent) -> bool {
    if cfg!(target_os = "macos") {
        return false;
    }
    let modifiers = event.keystroke.modifiers;
    modifiers.control
        && !modifiers.platform
        && !modifiers.alt
        && !modifiers.shift
        && event.keystroke.key.eq_ignore_ascii_case("c")
}
