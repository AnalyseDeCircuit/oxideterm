use std::{env, time::Duration};

use gpui::{
    ClipboardItem, Context, KeyDownEvent, KeyUpEvent, Modifiers, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, ScrollWheelEvent, Timer, TouchPhase, px,
};
use oxideterm_terminal::{TermMode, TerminalRow, TerminalSearchMatch, TerminalSnapshot};
use oxideterm_terminal_unicode::visual_line_for_row;

use super::{ScrollbarDrag, ScrollbarGeometry, TerminalContextMenu, TerminalPane};
use crate::terminal_ui::*;
use crate::terminal_view::*;

const TERMINAL_SELECTION_AUTOSCROLL_INTERVAL_MS: u64 = 16;
const TERMINAL_SELECTION_AUTOSCROLL_MAX_ROWS: i32 = 4;
const PRIVILEGE_PROMPT_DEBUG_ENV: &str = "OXIDETERM_PRIVILEGE_DEBUG";

fn log_privilege_prompt_terminal(args: std::fmt::Arguments<'_>) {
    if env::var_os(PRIVILEGE_PROMPT_DEBUG_ENV).is_some() {
        eprintln!("[oxideterm:privilege] {args}");
    }
}

#[derive(Clone, Copy)]
struct TerminalWheelScrollDelta {
    rows: i32,
    repaint: bool,
    animate_rows: bool,
}

impl TerminalPane {
    pub(crate) fn handle_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;

        if self.context_menu.take().is_some() {
            cx.notify();
            if key == "escape" {
                return true;
            }
        }

        if self.pending_paste.is_some() && !modifiers.platform && !modifiers.control {
            match key {
                "enter" => {
                    self.confirm_pending_paste(cx);
                    return true;
                }
                "escape" => {
                    self.cancel_pending_paste(cx);
                    return true;
                }
                _ => {}
            }
        }

        let has_privilege_prompt_inline_hint = self.privilege_prompt_inline_hint.is_some();
        let privilege_prompt_submit = privilege_prompt_enter_requests_submit(
            key,
            modifiers,
            has_privilege_prompt_inline_hint,
        );
        if key == "enter" && !modifiers.platform && !modifiers.control && !modifiers.alt {
            log_privilege_prompt_terminal(format_args!(
                "pane enter: shift={} has_inline_hint={} submit_request={}",
                modifiers.shift, has_privilege_prompt_inline_hint, privilege_prompt_submit
            ));
        }
        if privilege_prompt_submit {
            // The workspace owns secret lookup and PTY writes. The terminal
            // captures Enter before it becomes a normal newline, but only
            // after Workspace confirms the active scope has one fillable
            // credential and mirrors that as the visible inline hint.
            self.privilege_prompt_submit_requested = true;
            cx.notify();
            return true;
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
            return true;
        }

        if key == "end" && modifiers.platform {
            let snapshot = {
                let mut terminal = self.terminal.lock();
                terminal.scroll_to_bottom();
                terminal.snapshot()
            };
            self.clear_smooth_scroll_remainder();
            self.snapshot = self.stamp_snapshot(snapshot);
            cx.notify();
            return true;
        }

        if key == "home" && modifiers.platform {
            let snapshot = {
                let mut terminal = self.terminal.lock();
                terminal.scroll_to_top();
                terminal.snapshot()
            };
            self.clear_smooth_scroll_remainder();
            self.snapshot = self.stamp_snapshot(snapshot);
            cx.notify();
            return true;
        }

        let mode = self.terminal.lock().mode();
        if is_platform_copy_shortcut(event) {
            // macOS terminals reserve Cmd+C for copy; Ctrl+C remains the
            // protocol interrupt path below.
            self.copy_current_selection_or_snapshot(cx);
            return true;
        }

        if self.settings.smart_copy
            && is_smart_copy_shortcut(event)
            && smart_copy_selection_is_owned_by_terminal_ui(mode)
            && self.copy_selection_to_clipboard_if_present(cx)
        {
            return true;
        }

        if let Some(action) = oxideterm_terminal_scroll_action(&event.keystroke) {
            self.apply_scroll_action(action, cx);
            return true;
        }

        let key_event_type = if event.is_held {
            KittyKeyEventType::Repeat
        } else {
            KittyKeyEventType::Press
        };
        if let Some(sequence) =
            oxideterm_key_escape_sequence(&event.keystroke, &mode, false, key_event_type)
        {
            self.send_user_protocol_bytes(sequence.as_bytes(), cx);
            return true;
        }

        false
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
        let Some(scroll_delta) = self.determine_scroll_delta(event, scroll_multiplier) else {
            return;
        };

        if mouse_mode(mode, event.modifiers.shift) {
            self.clear_smooth_scroll_remainder();
            let rows = scroll_delta.rows;
            if rows == 0 {
                return;
            }
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
            self.clear_smooth_scroll_remainder();
            if scroll_delta.rows == 0 {
                return;
            }
            let bytes = alt_scroll(scroll_delta.rows);
            self.send_protocol_bytes(&bytes, cx);
            return;
        }

        let previous_offset = self.snapshot.display_offset;
        let snapshot = if scroll_delta.rows == 0 {
            self.snapshot.clone()
        } else {
            let mut terminal = self.terminal.lock();
            terminal.scroll_lines(terminal_scroll_delta(scroll_delta.rows));
            terminal.snapshot()
        };
        if scroll_delta.rows != 0 && snapshot.display_offset == previous_offset {
            let had_remainder = self.clear_smooth_scroll_remainder();
            if had_remainder {
                cx.notify();
            }
            return;
        }
        if scroll_delta.rows != 0 {
            self.snapshot = self.stamp_snapshot(snapshot);
            if scroll_delta.animate_rows {
                self.start_smooth_scroll_row_animation(scroll_delta.rows);
            }
        }
        let clamped_remainder = self.clamp_smooth_scroll_remainder_to_bounds();
        if scroll_delta.rows != 0 || scroll_delta.repaint || clamped_remainder {
            cx.notify();
        }
    }

    pub(super) fn clear_smooth_scroll_remainder(&mut self) -> bool {
        let had_remainder = f32::from(self.scroll_remainder_px).abs() > f32::EPSILON
            || self.smooth_scroll_animation_active;
        self.scroll_remainder_px = px(0.0);
        self.smooth_scroll_animation_active = false;
        had_remainder
    }

    fn start_smooth_scroll_row_animation(&mut self, rows: i32) {
        // Line-based wheel events arrive as whole terminal rows. Keep the PTY
        // state line-based, then animate the newly snapped snapshot back into
        // place so text can be partially clipped during the transition.
        self.scroll_remainder_px = if rows > 0 {
            -self.metrics.line_height
        } else {
            self.metrics.line_height
        };
        self.smooth_scroll_animation_active = true;
    }

    pub(super) fn clamp_smooth_scroll_remainder_to_bounds(&mut self) -> bool {
        let remainder = f32::from(self.scroll_remainder_px);
        let at_bottom = self.snapshot.display_offset == 0;
        let at_top = self.snapshot.display_offset >= self.snapshot.scrollback_lines;
        if (at_bottom && remainder < 0.0) || (at_top && remainder > 0.0) {
            return self.clear_smooth_scroll_remainder();
        }
        false
    }

    fn determine_scroll_delta(
        &mut self,
        event: &ScrollWheelEvent,
        scroll_multiplier: f32,
    ) -> Option<TerminalWheelScrollDelta> {
        match event.touch_phase {
            TouchPhase::Started => {
                self.scroll_remainder_px = px(0.0);
                self.smooth_scroll_animation_active = false;
                None
            }
            TouchPhase::Moved => {
                if self.smooth_scroll_animation_active {
                    self.scroll_remainder_px = px(0.0);
                    self.smooth_scroll_animation_active = false;
                }
                let line_height = self.metrics.line_height;
                let previous_remainder = self.scroll_remainder_px;
                self.scroll_remainder_px +=
                    event.delta.pixel_delta(line_height).y * scroll_multiplier;
                let rows = (self.scroll_remainder_px / line_height) as i32;
                if rows != 0 {
                    self.scroll_remainder_px -= px(rows as f32 * self.metrics.line_height_f32());
                }
                let smooth_scroll = self.settings.smooth_scroll;
                Some(TerminalWheelScrollDelta {
                    rows,
                    repaint: smooth_scroll
                        && rows == 0
                        && self.scroll_remainder_px != previous_remainder,
                    animate_rows: smooth_scroll && rows != 0 && !event.delta.precise(),
                })
            }
            TouchPhase::Ended => None,
        }
    }

    fn snapshot_text(&self) -> String {
        snapshot_text_from_rows(&self.snapshot.lines)
    }

    pub fn visible_text_snapshot(&self) -> String {
        self.snapshot_text()
    }

    pub fn privilege_prompt_text_snapshot(&self) -> String {
        privilege_prompt_text_from_snapshot(&self.snapshot)
    }

    pub fn ai_buffer_snapshot(&self) -> String {
        // Match Tauri's terminal registry buffer getter for AI tools: this
        // includes recent scrollback instead of only the visible viewport.
        self.terminal.lock().buffer_text()
    }

    pub fn ai_screen_snapshot(&self) -> oxideterm_terminal::TerminalSnapshot {
        // AI tool observation mirrors Tauri's terminal registry screen reader:
        // expose a read-only viewport snapshot without letting GPUI types leak
        // into the orchestrator tool payload.
        self.snapshot.clone()
    }

    pub fn ai_screen_is_alternate_buffer(&self) -> bool {
        // Tauri's readScreen reports whether xterm is currently using the
        // alternate buffer, which is important for TUI-oriented AI actions.
        self.terminal.lock().mode().contains(TermMode::ALT_SCREEN)
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

        let mode = self.terminal.lock().mode();
        if self.settings.smart_copy
            && smart_copy_selection_is_owned_by_terminal_ui(mode)
            && self.copy_selection_to_clipboard_if_present(cx)
        {
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

    pub(super) fn copy_selection_to_clipboard_if_present(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
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
        let col = ((f32::from(position.x - origin.x) - self.terminal_content_padding_x())
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
            let viewport_width = self
                .bounds
                .map(|bounds| bounds.size.width)
                .unwrap_or_else(|| px(0.0));
            let x = terminal_scrollbar_x_for_viewport(viewport_width);
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
        let snapshot = {
            let mut terminal = self.terminal.lock();
            terminal.scroll_to_display_offset(offset);
            terminal.snapshot()
        };
        self.clear_smooth_scroll_remainder();
        self.snapshot = self.stamp_snapshot(snapshot);
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
        self.selection_autoscroll_position = Some(position);
        self.schedule_selection_autoscroll(cx);
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
        self.selection_autoscroll_position = None;
        self.copy_selection_after_select_if_configured(cx);
    }

    fn update_selection_with_autoscroll(
        &mut self,
        position: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.selection_autoscroll_position = Some(position);
        self.update_selection(position, cx);
        self.schedule_selection_autoscroll(cx);
    }

    fn schedule_selection_autoscroll(&mut self, cx: &mut Context<Self>) {
        if self.selection_autoscroll_scheduled {
            return;
        }
        // Browser terminals keep extending a drag selection after the pointer
        // leaves the viewport; GPUI needs an explicit scroll tick for that.
        self.selection_autoscroll_scheduled = true;
        cx.spawn(async move |weak, cx| {
            Timer::after(Duration::from_millis(
                TERMINAL_SELECTION_AUTOSCROLL_INTERVAL_MS,
            ))
            .await;
            let _ = weak.update(cx, |this, cx| {
                this.selection_autoscroll_scheduled = false;
                this.run_selection_autoscroll_tick(cx);
            });
        })
        .detach();
    }

    fn run_selection_autoscroll_tick(&mut self, cx: &mut Context<Self>) {
        let Some(position) = self.selection_autoscroll_position else {
            return;
        };
        if !self.selecting {
            self.selection_autoscroll_position = None;
            return;
        }

        let delta_rows = self.selection_autoscroll_delta_rows(position);
        if delta_rows == 0 {
            return;
        }

        let current_offset = self.snapshot.display_offset;
        let target_offset = if delta_rows > 0 {
            current_offset.saturating_add(delta_rows as usize)
        } else {
            current_offset.saturating_sub(delta_rows.unsigned_abs() as usize)
        }
        .min(self.snapshot.scrollback_lines);

        if target_offset == current_offset {
            return;
        }

        let snapshot = {
            let mut terminal = self.terminal.lock();
            terminal.scroll_to_display_offset(target_offset);
            terminal.snapshot()
        };
        self.clear_smooth_scroll_remainder();
        self.snapshot = self.stamp_snapshot(snapshot);
        self.update_selection(position, cx);
        self.schedule_selection_autoscroll(cx);
    }

    fn selection_autoscroll_delta_rows(&self, position: gpui::Point<Pixels>) -> i32 {
        let origin = self.content_origin();
        let top = origin.y + px(TERMINAL_CONTENT_PADDING);
        let bottom = top + px(self.snapshot.rows.max(1) as f32 * self.metrics.line_height_f32());
        terminal_selection_autoscroll_delta_rows(position.y, top, bottom, self.metrics.line_height)
    }

    fn apply_scroll_action(&mut self, action: TerminalScrollAction, cx: &mut Context<Self>) {
        let snapshot = {
            let mut terminal = self.terminal.lock();
            match action {
                TerminalScrollAction::PageUp => terminal.page_up(),
                TerminalScrollAction::PageDown => terminal.page_down(),
                TerminalScrollAction::LineUp => terminal.scroll_lines(1),
                TerminalScrollAction::LineDown => terminal.scroll_lines(-1),
                TerminalScrollAction::Top => terminal.scroll_to_top(),
                TerminalScrollAction::Bottom => terminal.scroll_to_bottom(),
            }
            terminal.snapshot()
        };
        self.clear_smooth_scroll_remainder();
        self.snapshot = self.stamp_snapshot(snapshot);
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
        let snapshot = {
            let mut terminal = self.terminal.lock();
            terminal.scroll_to_display_offset(target_offset);
            terminal.snapshot()
        };
        self.clear_smooth_scroll_remainder();
        self.snapshot = self.stamp_snapshot(snapshot);
        cx.notify();
    }

    pub(crate) fn handle_mouse_down(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        if self.context_menu.take().is_some() {
            cx.notify();
        }

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
            self.selection_autoscroll_position = None;
            self.selection = None;
        }
    }

    pub(crate) fn open_terminal_context_menu(
        &mut self,
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(bounds) = self.bounds else {
            return;
        };

        // The Tauri/Web terminal now owns a copy/paste context menu instead of
        // exposing the WebView menu. Store pane-local coordinates so the GPUI
        // overlay tracks the same terminal surface without affecting TUI mouse mode.
        self.context_menu = Some(TerminalContextMenu {
            x: f32::from(event.position.x - bounds.origin.x),
            y: f32::from(event.position.y - bounds.origin.y),
            can_copy: self.selected_text_snapshot().is_some(),
        });
        self.selecting = false;
        cx.notify();
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
            self.update_selection_with_autoscroll(event.position, cx);
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
            self.selection_autoscroll_position = None;
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
        if self.selected_command_mark_id == selected {
            if selected.is_some() {
                self.selected_command_mark_id = None;
                cx.notify();
            }
            return;
        }

        if self.selected_command_mark_id != selected {
            self.selected_command_mark_id = selected;
            cx.notify();
        }
    }
}

fn mouse_tracking_active(mode: TermMode) -> bool {
    mode.intersects(TermMode::MOUSE_MODE)
}

fn terminal_selection_autoscroll_delta_rows(
    position_y: Pixels,
    top: Pixels,
    bottom: Pixels,
    line_height: Pixels,
) -> i32 {
    let distance = if position_y < top {
        f32::from(top - position_y)
    } else if position_y > bottom {
        -f32::from(position_y - bottom)
    } else {
        return 0;
    };
    let line_height = f32::from(line_height).max(1.0);
    let rows = (distance.abs() / line_height)
        .ceil()
        .max(1.0)
        .min(TERMINAL_SELECTION_AUTOSCROLL_MAX_ROWS as f32) as i32;
    if distance > 0.0 { rows } else { -rows }
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

fn is_platform_copy_shortcut(event: &KeyDownEvent) -> bool {
    if !cfg!(target_os = "macos") {
        return false;
    }
    let modifiers = event.keystroke.modifiers;
    modifiers.platform
        && !modifiers.control
        && !modifiers.alt
        && !modifiers.shift
        && event.keystroke.key.eq_ignore_ascii_case("c")
}

fn smart_copy_selection_is_owned_by_terminal_ui(mode: TermMode) -> bool {
    // In TUI-owned modes Ctrl+C must remain application input even if native
    // still has a stale visual selection from the normal scrollback buffer.
    !mode.contains(TermMode::ALT_SCREEN) && !mouse_tracking_active(mode)
}

fn privilege_prompt_enter_requests_submit(
    key: &str,
    modifiers: Modifiers,
    has_inline_hint: bool,
) -> bool {
    if key != "enter" || modifiers.platform || modifiers.control || modifiers.alt || modifiers.shift
    {
        return false;
    }
    has_inline_hint
}

fn snapshot_text_from_rows(rows: &[TerminalRow]) -> String {
    rows.iter()
        .map(|row| row.text().trim_end().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

fn privilege_prompt_text_from_snapshot(snapshot: &TerminalSnapshot) -> String {
    let Some(cursor_row) = snapshot.lines.get(snapshot.cursor_row) else {
        return snapshot_text_from_rows(&snapshot.lines);
    };

    if !cursor_row.active_input {
        return cursor_row.text().trim_end().to_string();
    }

    let mut start = snapshot.cursor_row;
    while start > 0
        && snapshot
            .lines
            .get(start - 1)
            .is_some_and(|row| row.active_input)
    {
        start -= 1;
    }

    let mut end = snapshot.cursor_row;
    while end + 1 < snapshot.lines.len()
        && snapshot
            .lines
            .get(end + 1)
            .is_some_and(|row| row.active_input)
    {
        end += 1;
    }

    // Privilege prompts should be detected from the live input area, not the
    // whole viewport. Full-screen scans can either miss SSH prompts when chrome
    // rows trail the cursor or, worse, match stale sudo prompts in scrollback.
    snapshot_text_from_rows(&snapshot.lines[start..=end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use oxideterm_terminal::{TerminalAttrs, TerminalCell, TerminalColor, TerminalCursorShape};

    fn test_cell(ch: char) -> TerminalCell {
        TerminalCell {
            ch,
            zerowidth: String::new(),
            wide: false,
            fg: TerminalColor::rgb(0xe6, 0xe8, 0xeb),
            bg: TerminalColor::rgb(0x0d, 0x0f, 0x12),
            attrs: TerminalAttrs::default(),
            hyperlink: None,
            cursor: false,
        }
    }

    fn test_row(text: &str, active_input: bool) -> TerminalRow {
        let mut cells = text.chars().map(test_cell).collect::<Vec<_>>();
        if cells.is_empty() {
            cells.push(test_cell(' '));
        }
        let mut row = TerminalRow {
            absolute_line: 0,
            cells: Arc::new(cells),
            wrapped: false,
            active_input,
            signature: 0,
        };
        row.refresh_signature();
        row
    }

    fn test_snapshot(lines: Vec<TerminalRow>, cursor_row: usize) -> TerminalSnapshot {
        TerminalSnapshot {
            generation: 1,
            cols: 120,
            rows: lines.len(),
            cursor_col: 0,
            cursor_row,
            cursor_shape: TerminalCursorShape::Block,
            display_offset: 0,
            scrollback_lines: 0,
            lines,
            images: Vec::new(),
        }
    }

    #[test]
    fn smart_copy_yields_ctrl_c_to_tui_modes() {
        assert!(smart_copy_selection_is_owned_by_terminal_ui(
            TermMode::default()
        ));
        assert!(!smart_copy_selection_is_owned_by_terminal_ui(
            TermMode::ALT_SCREEN
        ));
        assert!(!smart_copy_selection_is_owned_by_terminal_ui(
            TermMode::MOUSE_REPORT_CLICK
        ));
    }

    #[test]
    fn selection_autoscroll_matches_display_offset_direction() {
        assert_eq!(
            terminal_selection_autoscroll_delta_rows(px(89.0), px(100.0), px(200.0), px(10.0)),
            2
        );
        assert_eq!(
            terminal_selection_autoscroll_delta_rows(px(211.0), px(100.0), px(200.0), px(10.0)),
            -2
        );
        assert_eq!(
            terminal_selection_autoscroll_delta_rows(px(150.0), px(100.0), px(200.0), px(10.0)),
            0
        );
        assert_eq!(
            terminal_selection_autoscroll_delta_rows(px(250.0), px(100.0), px(200.0), px(10.0)),
            -TERMINAL_SELECTION_AUTOSCROLL_MAX_ROWS
        );
    }

    #[test]
    fn privilege_prompt_enter_requires_inline_hint() {
        assert!(!privilege_prompt_enter_requests_submit(
            "enter",
            Modifiers::default(),
            false
        ));
    }

    #[test]
    fn privilege_prompt_enter_requests_submit_for_inline_hint() {
        assert!(privilege_prompt_enter_requests_submit(
            "enter",
            Modifiers::default(),
            true
        ));
    }

    #[test]
    fn privilege_prompt_modified_enter_does_not_request_submit() {
        assert!(!privilege_prompt_enter_requests_submit(
            "enter",
            Modifiers {
                shift: true,
                ..Modifiers::default()
            },
            true
        ));
    }

    #[test]
    fn privilege_prompt_snapshot_uses_cursor_input_block() {
        let snapshot = test_snapshot(
            vec![
                test_row("old sudo command", false),
                test_row("[sudo] old 的密码:", false),
                test_row("❯ sudo yazi", false),
                test_row("[sudo] lipsc 的密码:", true),
                test_row("status text after cursor", false),
            ],
            3,
        );

        assert_eq!(
            privilege_prompt_text_from_snapshot(&snapshot),
            "[sudo] lipsc 的密码:"
        );
    }

    #[test]
    fn privilege_prompt_snapshot_does_not_match_stale_scrollback_prompt() {
        let snapshot = test_snapshot(
            vec![
                test_row("❯ sudo yazi", false),
                test_row("[sudo] lipsc 的密码:", false),
                test_row("", true),
            ],
            2,
        );

        assert_eq!(privilege_prompt_text_from_snapshot(&snapshot), "");
    }
}
