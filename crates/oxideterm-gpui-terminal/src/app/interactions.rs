use std::{
    env,
    time::{Duration, Instant},
};

use gpui::{
    ClipboardItem, Context, KeyDownEvent, KeyUpEvent, Modifiers, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, ScrollWheelEvent, TouchPhase, px,
};
use oxideterm_terminal::{
    TermMode, TerminalEditorApplication, TerminalEditorClipboardOperation, TerminalRow,
    TerminalSearchMatch, TerminalSnapshot,
};
use oxideterm_terminal_unicode::visual_line_for_row;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::{
    FreeTypeDragAction, FreeTypeDragState, PendingTerminalEditorClipboard, ScrollbarDrag,
    ScrollbarGeometry, SmoothScrollAnimation, TerminalContextMenu, TerminalPane, TerminalPaneEvent,
    command_mark_ui_available,
};
use crate::command_facts::TerminalAutosuggestInputState;
use crate::terminal_ui::*;
use crate::terminal_view::*;

const TERMINAL_SELECTION_AUTOSCROLL_INTERVAL_MS: u64 = 16;
const TERMINAL_SELECTION_AUTOSCROLL_MAX_ROWS: i32 = 4;
const TERMINAL_FREE_TYPE_MAX_CURSOR_STEPS: usize = 4096;
const TERMINAL_FREE_TYPE_DRAG_THRESHOLD_PX: f32 = 5.0;
const PRIVILEGE_PROMPT_DEBUG_ENV: &str = "OXIDETERM_PRIVILEGE_DEBUG";
const FREE_TYPE_DEBUG_ENV: &str = "OXIDETERM_FREE_TYPE_DEBUG";

fn log_privilege_prompt_terminal(args: std::fmt::Arguments<'_>) {
    if env::var_os(PRIVILEGE_PROMPT_DEBUG_ENV).is_some() {
        eprintln!("[oxideterm:privilege] {args}");
    }
}

fn log_free_type_terminal(args: std::fmt::Arguments<'_>) {
    if env::var_os(FREE_TYPE_DEBUG_ENV).is_some() {
        eprintln!("[oxideterm:free-type] {args}");
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

        if self.context_menu.is_some() {
            self.dismiss_terminal_context_menu(cx);
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
            cx.emit(TerminalPaneEvent::PrivilegePromptSubmitRequested);
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
        if self.handle_editor_free_type_clipboard_shortcut(event, mode, cx) {
            return true;
        }
        if self.handle_free_type_clipboard_shortcut(event, mode, cx) {
            return true;
        }
        if is_legacy_terminal_copy_shortcut(key, modifiers) {
            // Preserve the long-standing terminal convention without consuming plain Insert.
            self.copy_current_selection_or_snapshot(cx);
            return true;
        }
        if is_legacy_terminal_paste_shortcut(key, modifiers) {
            // Clipboard paste must be handled before Insert is encoded as a terminal sequence.
            self.paste_from_clipboard(cx);
            return true;
        }
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

        if free_type_delete_key_requests_selection_delete(key, modifiers) {
            if self.delete_editor_free_type_selection_if_active(mode, cx)
                || self.delete_free_type_selection_if_active(mode, cx)
            {
                return true;
            }
        }

        let key_event_type = if event.is_held {
            KittyKeyEventType::Repeat
        } else {
            KittyKeyEventType::Press
        };
        if let Some(sequence) = configurable_key_escape_sequence(
            &event.keystroke,
            &mode,
            false,
            self.settings.backspace_sequence,
            self.settings.delete_sequence,
            key_event_type,
        ) {
            self.send_user_protocol_bytes(sequence.as_bytes(), cx);
            return true;
        }

        false
    }

    pub fn handle_unfocused_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        if self.focused {
            // Focused printable input is committed by the platform input
            // handler after keydown; forwarding it here would duplicate text.
            return false;
        }

        // Workspace can temporarily own focus while the terminal pane remains
        // the visible active shell. Reuse the pane encoder so Tab, Backspace,
        // and other protocol keys keep the same behavior as focused input.
        if self.handle_key(event, cx) {
            return true;
        }

        let modifiers = event.keystroke.modifiers;
        if modifiers.platform || modifiers.control || modifiers.alt {
            return false;
        }

        // Printable text normally arrives through the focused platform input
        // handler. Forward its key text explicitly when Workspace temporarily
        // owns focus, while leaving shortcuts and IME composition untouched.
        if let Some(text) = event
            .keystroke
            .key_char
            .as_deref()
            .filter(|text| !text.is_empty() && !text.chars().any(char::is_control))
        {
            self.commit_text(text, cx);
            return true;
        }

        // Some platforms omit key_char for an unmodified Space key.
        if matches!(event.keystroke.key.as_str(), "space" | " ") {
            self.commit_text(" ", cx);
            return true;
        }

        false
    }

    pub(crate) fn handle_key_up(&mut self, event: &KeyUpEvent, cx: &mut Context<Self>) {
        let mode = self.terminal.lock().mode();
        if let Some(sequence) = configurable_key_escape_sequence(
            &event.keystroke,
            &mode,
            false,
            self.settings.backspace_sequence,
            self.settings.delete_sequence,
            KittyKeyEventType::Release,
        ) {
            self.send_protocol_bytes(sequence.as_bytes(), cx);
        }
    }

    pub(crate) fn handle_scroll(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        // Terminal menu payloads include row-local command marks and target
        // points; any scroll makes that semantic snapshot stale.
        if self.context_menu.take().is_some() {
            self.context_menu_presence.reopen();
            cx.notify();
        }
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

        if scroll_delta.rows == 0 {
            let clamped_remainder = self.clamp_smooth_scroll_remainder_to_bounds();
            if scroll_delta.repaint || clamped_remainder {
                cx.notify();
            }
            return;
        }

        let previous_offset = self.snapshot.display_offset;
        let snapshot_was_dirty = self.snapshot_dirty;
        let snapshot_started = self.preferences.show_performance_overlay.then(Instant::now);
        let snapshot = {
            let mut terminal = self.terminal.lock();
            let delta = terminal_scroll_delta(scroll_delta.rows);
            if self.snapshot_dirty {
                terminal.scroll_lines(delta);
                terminal.snapshot()
            } else {
                terminal.scroll_lines_snapshot_incremental(delta, &self.snapshot)
            }
        };
        if let Some(snapshot_started) = snapshot_started {
            // This includes the synchronous viewport snapshot used by both precise touchpads and
            // discrete wheels; overscan animation snapshots update the same diagnostic counter.
            self.render_stats.scroll_snapshot_micros = snapshot_started
                .elapsed()
                .as_micros()
                .min(u128::from(u64::MAX))
                as u64;
            self.render_stats.scroll_snapshot_count =
                self.render_stats.scroll_snapshot_count.saturating_add(1);
        }
        if snapshot.display_offset == previous_offset {
            if snapshot_was_dirty {
                self.snapshot = self.stamp_snapshot(snapshot);
                self.snapshot_dirty = false;
            }
            let had_remainder = self.clear_smooth_scroll_remainder();
            if snapshot_was_dirty || had_remainder {
                cx.notify();
            }
            return;
        }
        let applied_rows = if snapshot.display_offset >= previous_offset {
            (snapshot.display_offset - previous_offset) as f32
        } else {
            -((previous_offset - snapshot.display_offset) as f32)
        };
        self.snapshot_dirty = false;
        self.snapshot = self.stamp_snapshot(snapshot);
        if scroll_delta.animate_rows {
            self.start_smooth_scroll_row_animation(applied_rows);
        }
        let clamped_remainder = self.clamp_smooth_scroll_remainder_to_bounds();
        if scroll_delta.repaint || clamped_remainder || applied_rows.abs() > f32::EPSILON {
            cx.notify();
        }
    }

    pub(super) fn clear_smooth_scroll_remainder(&mut self) -> bool {
        let had_remainder = f32::from(self.scroll_input_remainder_px).abs() > f32::EPSILON
            || f32::from(self.smooth_scroll_offset_px).abs() > f32::EPSILON
            || self.smooth_scroll_animation.is_some();
        self.scroll_input_remainder_px = px(0.0);
        self.smooth_scroll_offset_px = px(0.0);
        self.smooth_scroll_animation = None;
        had_remainder
    }

    fn start_smooth_scroll_row_animation(&mut self, applied_rows: f32) {
        let now = Instant::now();
        let _ = self.advance_smooth_scroll_animation(now);
        // The backend owns the integer target while the pane keeps the current visual position.
        // Repeated wheel events move that target without snapping an in-flight animation.
        self.smooth_scroll_offset_px -= px(applied_rows * self.metrics.line_height_f32());
        self.smooth_scroll_animation = Some(SmoothScrollAnimation {
            started_at: now,
            start_offset_px: self.smooth_scroll_offset_px,
        });
        self.wake_terminal_scheduler();
    }

    pub(super) fn clamp_smooth_scroll_remainder_to_bounds(&mut self) -> bool {
        let remainder = f32::from(self.smooth_scroll_offset_px);
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
            TouchPhase::Started => Some(TerminalWheelScrollDelta {
                rows: 0,
                repaint: self.clear_smooth_scroll_remainder(),
                animate_rows: false,
            }),
            TouchPhase::Moved => {
                let precise = event.delta.precise();
                if precise && self.smooth_scroll_animation.is_some() {
                    let _ = self.advance_smooth_scroll_animation(Instant::now());
                    // Preserve the current visual position when a touchpad takes over a wheel
                    // animation, then normalize it through the same row accumulator below.
                    self.scroll_input_remainder_px = self.smooth_scroll_offset_px;
                    self.smooth_scroll_animation = None;
                }
                let line_height = self.metrics.line_height;
                let previous_visual_offset = self.smooth_scroll_offset_px;
                self.scroll_input_remainder_px +=
                    event.delta.pixel_delta(line_height).y * scroll_multiplier;
                let rows = (self.scroll_input_remainder_px / line_height) as i32;
                if rows != 0 {
                    self.scroll_input_remainder_px -=
                        px(rows as f32 * self.metrics.line_height_f32());
                }
                let smooth_scroll = self.settings.smooth_scroll;
                if smooth_scroll && precise {
                    self.smooth_scroll_offset_px = self.scroll_input_remainder_px;
                } else if !smooth_scroll {
                    self.smooth_scroll_offset_px = px(0.0);
                    self.smooth_scroll_animation = None;
                }
                Some(TerminalWheelScrollDelta {
                    rows,
                    repaint: smooth_scroll
                        && precise
                        && self.smooth_scroll_offset_px != previous_visual_offset,
                    animate_rows: smooth_scroll && rows != 0 && !precise,
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
        let mode = self.terminal.lock().mode();
        if self.copy_free_type_selection_to_clipboard_if_active(mode, cx) {
            return;
        }

        if cfg!(target_os = "macos") {
            self.copy_current_selection_or_snapshot(cx);
            return;
        }

        if self.settings.smart_copy
            && smart_copy_selection_is_owned_by_terminal_ui(mode)
            && self.copy_selection_to_clipboard_if_present(cx)
        {
            return;
        }

        self.send_user_protocol_bytes(&[0x03], cx);
    }

    fn handle_free_type_clipboard_shortcut(
        &mut self,
        event: &KeyDownEvent,
        mode: TermMode,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(shortcut) = free_type_clipboard_shortcut(
            event.keystroke.key.as_str(),
            event.keystroke.modifiers,
            cfg!(target_os = "macos"),
        ) else {
            return false;
        };

        match shortcut {
            FreeTypeClipboardShortcut::Copy => {
                self.copy_free_type_selection_to_clipboard_if_active(mode, cx)
            }
            FreeTypeClipboardShortcut::Cut => self.cut_free_type_selection_to_clipboard(mode, cx),
            FreeTypeClipboardShortcut::Paste => {
                if !self.free_type_active_command_accepts_clipboard_paste(mode) {
                    return false;
                }
                // Clipboard paste keeps the existing protection and bracketed-paste path.
                self.paste_from_clipboard(cx);
                true
            }
        }
    }

    fn handle_editor_free_type_clipboard_shortcut(
        &mut self,
        event: &KeyDownEvent,
        mode: TermMode,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(shortcut) = free_type_clipboard_shortcut(
            event.keystroke.key.as_str(),
            event.keystroke.modifiers,
            cfg!(target_os = "macos"),
        ) else {
            return false;
        };
        let Some(editor) = self.active_editor_integration(mode) else {
            return false;
        };

        match shortcut {
            FreeTypeClipboardShortcut::Copy | FreeTypeClipboardShortcut::Cut => {
                if !editor.capabilities.clipboard || !editor.selection.is_active() {
                    return false;
                }
                let operation = if shortcut == FreeTypeClipboardShortcut::Copy {
                    TerminalEditorClipboardOperation::Copy
                } else {
                    TerminalEditorClipboardOperation::Cut
                };
                let edit_operation = if shortcut == FreeTypeClipboardShortcut::Copy {
                    TerminalEditorEditOperation::Copy
                } else {
                    TerminalEditorEditOperation::Cut
                };
                self.pending_editor_clipboard = Some(PendingTerminalEditorClipboard {
                    application: editor.application,
                    operation,
                    requested_at: std::time::Instant::now(),
                });
                self.send_user_protocol_bytes(
                    editor_operation_bytes(editor.application, edit_operation),
                    cx,
                );
                true
            }
            FreeTypeClipboardShortcut::Paste => {
                if !editor.capabilities.edit {
                    return false;
                }
                let prefix = editor_operation_bytes(
                    editor.application,
                    TerminalEditorEditOperation::PreparePaste,
                );
                self.paste_from_clipboard_after(prefix, cx);
                true
            }
        }
    }

    fn delete_editor_free_type_selection_if_active(
        &mut self,
        mode: TermMode,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(editor) = self.active_editor_integration(mode) else {
            return false;
        };
        if !editor.capabilities.edit || !editor.selection.is_active() {
            return false;
        }
        self.send_user_protocol_bytes(
            editor_operation_bytes(
                editor.application,
                TerminalEditorEditOperation::DeleteSelection,
            ),
            cx,
        );
        true
    }

    fn copy_free_type_selection_to_clipboard_if_active(
        &mut self,
        mode: TermMode,
        cx: &mut Context<Self>,
    ) -> bool {
        if !free_type_mode_allows_command_edit(
            self.settings.free_type_mode,
            mode,
            Modifiers::default(),
        ) {
            return false;
        }
        let Some(selection) = self.selection else {
            return false;
        };
        let input_state = self.input_tracker.state();
        let Some(text) = free_type_selected_command_text(&self.snapshot, selection, &input_state)
        else {
            return false;
        };

        cx.write_to_clipboard(ClipboardItem::new_string(text));
        if !self.settings.keep_selection_on_copy {
            self.selection = None;
            cx.notify();
        }
        true
    }

    pub fn cut_to_clipboard(&mut self, cx: &mut Context<Self>) -> bool {
        let mode = self.terminal.lock().mode();
        self.cut_free_type_selection_to_clipboard(mode, cx)
    }

    fn cut_free_type_selection_to_clipboard(
        &mut self,
        mode: TermMode,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.terminal_accepts_input()
            || !free_type_mode_allows_command_edit(
                self.settings.free_type_mode,
                mode,
                Modifiers::default(),
            )
        {
            return false;
        }
        let Some(selection) = self.selection else {
            return false;
        };
        let input_state = self.input_tracker.state();
        let Some((text, bytes)) =
            free_type_selection_cut_payload(&self.snapshot, selection, &input_state, mode)
        else {
            return false;
        };

        // The clipboard receives the exact editable command slice, while the
        // remote line editor remains responsible for applying the deletion.
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.selection = None;
        self.selecting = false;
        self.selection_autoscroll_position = None;
        self.send_user_protocol_bytes(&bytes, cx);
        true
    }

    fn free_type_active_command_accepts_clipboard_paste(&self, mode: TermMode) -> bool {
        if !self.terminal_accepts_input()
            || !free_type_mode_allows_command_edit(
                self.settings.free_type_mode,
                mode,
                Modifiers::default(),
            )
        {
            return false;
        }
        let input_state = self.input_tracker.state();
        let cursor_row_is_active = self
            .snapshot
            .lines
            .get(self.snapshot.cursor_row)
            .is_some_and(|row| row.active_input);
        cursor_row_is_active
            && input_state.cursor_index <= input_state.value.len()
            && input_state.value.is_char_boundary(input_state.cursor_index)
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
            cx.background_executor()
                .timer(Duration::from_millis(120))
                .await;
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
        let selection = self.selection?;
        let Some(request) = snapshot_request_for_selection(&self.snapshot, selection) else {
            return selected_text_for_selection(&self.snapshot, selection);
        };

        // Cross-page selections outlive any individual viewport snapshot. Materialize only their
        // grid range at copy time so normal rendering and in-view copies keep their current cost.
        let snapshot = self
            .terminal
            .lock()
            .snapshot_with_display_offset(request.display_offset, request.rows);
        selected_text_for_selection(&snapshot, selection)
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

    pub(super) fn terminal_point_for_position(
        &self,
        position: gpui::Point<Pixels>,
    ) -> TerminalPoint {
        let origin = self.content_origin();
        let col = ((f32::from(position.x - origin.x) - self.terminal_content_padding_x())
            / self.metrics.cell_width_f32())
        .floor()
        .max(0.0) as usize;
        let smooth_scroll_y_offset = if self.settings.smooth_scroll {
            f32::from(self.smooth_scroll_offset_px)
        } else {
            0.0
        };
        let row = terminal_viewport_row_for_position(
            f32::from(position.y - origin.y),
            smooth_scroll_y_offset,
            self.metrics.line_height_f32(),
            self.snapshot.rows,
        );
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

        // Pointer hover needs links only for the row under the cursor. Scanning the full
        // viewport here repeats URL and path detection for every mouse-move event.
        display_link_ranges_for_rows_with_path_detection(
            &self.snapshot,
            point.row..point.row.saturating_add(1),
            self.settings.detect_file_paths_as_links,
        )
        .into_iter()
        .find(|link| {
            link.row == point.row
                && point.col >= link.start_col
                && point.col < link.end_col
                && (cell.hyperlink().is_some() || is_link_stylable_cell(cell))
        })
    }

    fn scrollbar_geometry(&self) -> Option<ScrollbarGeometry> {
        terminal_scrollbar_for_viewport_display_offset(
            &self.snapshot,
            &self.metrics,
            self.snapshot.rows,
            self.smooth_scroll_display_offset(),
        )
        .map(|scrollbar| {
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

    fn select_matching_pair(
        &mut self,
        position: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        let point = self.terminal_point_for_position(position);
        let Some(selection) = matching_pair_selection_at_point(&self.snapshot, point) else {
            return false;
        };

        self.selection = Some(selection);
        self.selecting = false;
        cx.notify();
        true
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
            cx.background_executor()
                .timer(Duration::from_millis(
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
        let selected = matches[self.selected_search_match.unwrap()].clone();
        self.scroll_to_search_match(&selected, cx);
    }

    pub(super) fn scroll_to_selected_search_match(&mut self, cx: &mut Context<Self>) {
        let matches = self.current_search_matches();
        let Some(index) = self
            .selected_search_match
            .filter(|index| *index < matches.len())
        else {
            return;
        };
        let selected = matches[index].clone();
        self.scroll_to_search_match(&selected, cx);
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
        if self.context_menu.is_some() {
            self.dismiss_terminal_context_menu(cx);
        }

        if event.button == MouseButton::Left
            && terminal_link_activation_allowed(
                event.modifiers,
                self.settings.open_links_with_modifier,
            )
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
        if event.button == MouseButton::Right
            && right_click_paste_requested(self.settings.right_click_paste, mode, event.modifiers)
        {
            return;
        }

        if self.start_free_type_drag_candidate(event, mode) {
            cx.notify();
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
                2 => {
                    let matching_pair_selected = free_type_mode_allows_command_edit(
                        self.settings.free_type_mode,
                        mode,
                        event.modifiers,
                    ) && self.select_matching_pair(event.position, cx);
                    if !matching_pair_selected {
                        self.select_word(event.position, cx);
                    }
                }
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

        let reference_line = self.absolute_line_for_position(event.position);
        let terminal_mode = self.terminal.lock().mode();
        let command_mark_ui_visible =
            command_mark_ui_available(self.settings.command_marks_enabled, terminal_mode);
        let command_mark_id = command_mark_ui_visible
            .then(|| self.command_mark_id_at_absolute_line(reference_line))
            .flatten();
        let navigation_line = command_mark_id
            .as_deref()
            .and_then(|id| self.command_mark_start_line(id))
            .unwrap_or(reference_line);

        // The Tauri/Web terminal now owns a copy/paste context menu instead of
        // exposing the WebView menu. Store pane-local coordinates so the GPUI
        // overlay tracks the same terminal surface without affecting TUI mouse mode.
        self.context_menu_presence.reopen();
        self.context_menu = Some(TerminalContextMenu {
            x: f32::from(event.position.x - bounds.origin.x),
            y: f32::from(event.position.y - bounds.origin.y),
            modem_submenu_open: false,
            target: self.terminal_point_for_position(event.position),
            has_selection: self.selected_text_snapshot().is_some(),
            reference_line: navigation_line,
            command_mark_id,
            has_previous_command: command_mark_ui_visible
                && self
                    .previous_command_mark_id_before_line(navigation_line)
                    .is_some(),
            has_next_command: command_mark_ui_visible
                && self
                    .next_command_mark_id_after_line(navigation_line)
                    .is_some(),
        });
        self.selecting = false;
        cx.notify();
    }

    pub(crate) fn handle_mouse_move(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let mode = self.terminal.lock().mode();
        let can_hover_terminal_content = !self.selecting
            && self.scrollbar_drag.is_none()
            && !mouse_mode(mode, event.modifiers.shift);
        let hovered_link = can_hover_terminal_content
            .then(|| self.link_at_position(event.position))
            .flatten();
        let can_hover_command_marks = can_hover_terminal_content
            && command_mark_ui_available(self.settings.command_marks_enabled, mode);
        let hovered_command_mark_id = can_hover_command_marks
            .then(|| {
                let absolute_line = self.absolute_line_for_position(event.position);
                self.command_mark_id_at_absolute_line(absolute_line)
            })
            .flatten();
        let hover_changed = hovered_link != self.hovered_link
            || hovered_command_mark_id != self.hovered_command_mark_id;
        if hover_changed {
            self.hovered_link = hovered_link;
            self.hovered_command_mark_id = hovered_command_mark_id;
            cx.notify();
        }

        if let Some(drag) = self.scrollbar_drag
            && event.pressed_button == Some(MouseButton::Left)
        {
            self.set_scrollbar_position(event.position, drag.thumb_offset_y, cx);
            return;
        }

        if self.update_free_type_drag(event, cx) {
            return;
        }

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

        if self.finish_free_type_drag(event.position, event.modifiers, cx) {
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
        if event.button == MouseButton::Right
            && right_click_paste_requested(self.settings.right_click_paste, mode, event.modifiers)
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
                if !self.move_cursor_to_free_type_click(event.position, event.modifiers, mode, cx) {
                    self.select_command_mark_at_position(event.position, mode, cx);
                }
            }
            self.last_mouse_report_point = None;
        } else {
            if event.button == MouseButton::Left {
                self.move_cursor_to_free_type_click(event.position, event.modifiers, mode, cx);
            }
            self.selecting = false;
            self.selection_autoscroll_position = None;
            self.last_mouse_report_point = None;
        }
    }

    fn start_free_type_drag_candidate(&mut self, event: &MouseDownEvent, mode: TermMode) -> bool {
        if !free_type_drag_candidate_allowed(self.settings.free_type_mode, mode, event.modifiers) {
            return false;
        }
        if event.button != MouseButton::Left || event.click_count > 1 {
            return false;
        }

        let Some(selection) = self.selection.filter(|selection| !selection.is_empty()) else {
            return false;
        };
        let input_state = self.input_tracker.state();
        let command_text = free_type_selected_command_text(&self.snapshot, selection, &input_state);
        let Some(text) = command_text
            .clone()
            .or_else(|| self.selected_text_snapshot())
        else {
            return false;
        };
        if !free_type_selected_text_can_be_command_input(&text) {
            return false;
        }

        let target = self.terminal_point_for_position(event.position);
        let Some(point) = grid_point_for_viewport_point(&self.snapshot, target) else {
            return false;
        };
        if !selection_contains_grid_point(selection, point) {
            return false;
        }

        let source_selection = command_text.is_some().then_some(selection);
        let Some(action) = free_type_drag_action(event.modifiers, source_selection.is_some())
        else {
            return false;
        };
        self.free_type_drag = Some(FreeTypeDragState {
            start_position: event.position,
            text,
            source_selection,
            action,
            active: false,
        });
        self.selecting = false;
        self.selection_autoscroll_position = None;
        true
    }

    fn update_free_type_drag(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) -> bool {
        let Some(drag) = self.free_type_drag.as_mut() else {
            return false;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            self.free_type_drag = None;
            cx.notify();
            return true;
        }

        if !drag.active && free_type_drag_distance_exceeded(drag.start_position, event.position) {
            drag.active = true;
            cx.notify();
        }
        if let Some(action) =
            free_type_drag_action(event.modifiers, drag.source_selection.is_some())
        {
            drag.action = action;
        }
        true
    }

    fn finish_free_type_drag(
        &mut self,
        position: gpui::Point<Pixels>,
        modifiers: Modifiers,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(mut drag) = self.free_type_drag.take() else {
            return false;
        };
        if !drag.active {
            cx.notify();
            return true;
        }
        let Some(action) = free_type_drag_action(modifiers, drag.source_selection.is_some()) else {
            log_free_type_terminal(format_args!("drag drop rejected: modifier conflict"));
            cx.notify();
            return true;
        };
        drag.action = action;

        let target = self.terminal_point_for_position(position);
        let accepted = match (drag.action, drag.source_selection) {
            (FreeTypeDragAction::MoveSelection, Some(selection)) => {
                self.send_free_type_selection_move(target, selection, cx)
            }
            (FreeTypeDragAction::CopySelection, _) | (FreeTypeDragAction::MoveSelection, None) => {
                self.send_free_type_command_edit_text(target, &drag.text, false, cx)
            }
            (FreeTypeDragAction::ReplaceCommand, _) => {
                self.send_free_type_command_edit_text(target, &drag.text, true, cx)
            }
        };
        if accepted {
            log_free_type_terminal(format_args!("drag drop accepted: action={:?}", drag.action));
        } else {
            log_free_type_terminal(format_args!("drag drop rejected"));
            cx.notify();
        }
        true
    }

    fn send_free_type_selection_move(
        &mut self,
        target: TerminalPoint,
        selection: TerminalSelection,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.terminal_accepts_input() {
            return false;
        }
        let mode = self.terminal.lock().mode();
        if !free_type_mode_allows_command_edit(
            self.settings.free_type_mode,
            mode,
            Modifiers::default(),
        ) {
            return false;
        }
        let input_state = self.input_tracker.state();
        let Some(bytes) =
            free_type_selection_move_bytes(&self.snapshot, selection, target, &input_state, mode)
        else {
            return false;
        };

        // The remote line editor applies the move. An empty payload means that
        // the drop stayed inside the source selection and is already complete.
        self.selection = None;
        self.selecting = false;
        self.selection_autoscroll_position = None;
        if bytes.is_empty() {
            cx.notify();
        } else {
            self.send_user_protocol_bytes(&bytes, cx);
        }
        true
    }

    fn move_cursor_to_free_type_click(
        &mut self,
        position: gpui::Point<Pixels>,
        modifiers: Modifiers,
        mode: TermMode,
        cx: &mut Context<Self>,
    ) -> bool {
        if !free_type_mode_allows_command_edit(self.settings.free_type_mode, mode, modifiers) {
            log_free_type_terminal(format_args!(
                "click rejected: {}",
                free_type_mode_command_edit_rejection_reason(
                    self.settings.free_type_mode,
                    mode,
                    modifiers,
                )
                .unwrap_or("unknown")
            ));
            return false;
        }

        let target = self.terminal_point_for_position(position);
        let input_state = self.input_tracker.state();
        let Some(cursor_move) =
            active_input_cursor_move(&self.snapshot, target, Some(&input_state))
        else {
            log_free_type_terminal(format_args!(
                "click rejected: target outside active input row={} col={}",
                target.row, target.col
            ));
            return false;
        };
        let Some(bytes) = free_type_cursor_move_bytes(cursor_move, mode) else {
            log_free_type_terminal(format_args!(
                "click accepted: already at target row={} col={}",
                target.row, target.col
            ));
            return true;
        };

        // The remote shell is still the source of truth. Send regular cursor
        // keys so readline, zsh, and other line editors can apply their own
        // boundaries instead of letting the client mutate terminal state.
        self.selection = None;
        self.selecting = false;
        self.selection_autoscroll_position = None;
        log_free_type_terminal(format_args!(
            "click accepted: cursor_delta={}",
            cursor_move.delta
        ));
        self.send_user_protocol_bytes(&bytes, cx);
        true
    }

    pub(super) fn delete_free_type_selection_if_active(
        &mut self,
        mode: TermMode,
        cx: &mut Context<Self>,
    ) -> bool {
        if !free_type_mode_allows_command_edit(
            self.settings.free_type_mode,
            mode,
            Modifiers::default(),
        ) {
            log_free_type_terminal(format_args!(
                "selection delete rejected: {}",
                free_type_mode_command_edit_rejection_reason(
                    self.settings.free_type_mode,
                    mode,
                    Modifiers::default(),
                )
                .unwrap_or("unknown")
            ));
            return false;
        }

        let Some(selection) = self.selection else {
            log_free_type_terminal(format_args!("selection delete rejected: no selection"));
            return false;
        };
        let input_state = self.input_tracker.state();
        let Some(bytes) =
            free_type_selection_delete_bytes(&self.snapshot, selection, &input_state, mode)
        else {
            log_free_type_terminal(format_args!(
                "selection delete rejected: selection outside active input"
            ));
            return false;
        };

        self.selection = None;
        self.selecting = false;
        self.selection_autoscroll_position = None;
        log_free_type_terminal(format_args!(
            "selection delete accepted: protocol_bytes={}",
            bytes.len()
        ));
        self.send_user_protocol_bytes(&bytes, cx);
        true
    }

    pub(crate) fn free_type_context_insert_selection_available(
        &self,
        menu: &TerminalContextMenu,
    ) -> bool {
        self.free_type_context_command_edit_bytes(menu.target, false)
            .is_some()
    }

    pub(crate) fn free_type_context_replace_command_available(
        &self,
        menu: &TerminalContextMenu,
    ) -> bool {
        self.free_type_context_command_edit_bytes(menu.target, true)
            .is_some()
    }

    fn free_type_context_command_edit_bytes(
        &self,
        target: TerminalPoint,
        replace_current_command: bool,
    ) -> Option<Vec<u8>> {
        let text = self.selected_text_snapshot()?;
        self.free_type_command_edit_bytes_for_text(target, &text, replace_current_command)
    }

    fn free_type_command_edit_bytes_for_text(
        &self,
        target: TerminalPoint,
        text: &str,
        replace_current_command: bool,
    ) -> Option<Vec<u8>> {
        if !self.terminal_accepts_input() {
            return None;
        }

        let mode = self.terminal.lock().mode();
        if !free_type_mode_allows_command_edit(
            self.settings.free_type_mode,
            mode,
            Modifiers::default(),
        ) {
            return None;
        }

        let input_state = self.input_tracker.state();
        free_type_command_edit_bytes(
            &self.snapshot,
            target,
            &input_state,
            &text,
            replace_current_command,
            mode,
        )
    }

    fn send_free_type_command_edit_text(
        &mut self,
        target: TerminalPoint,
        text: &str,
        replace_current_command: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(bytes) =
            self.free_type_command_edit_bytes_for_text(target, text, replace_current_command)
        else {
            return false;
        };

        // This is a terminal editing intent, not a local buffer mutation. Clear
        // the visual selection and let the remote shell echo the final command.
        self.selection = None;
        self.selecting = false;
        self.selection_autoscroll_position = None;
        self.send_user_protocol_bytes(&bytes, cx);
        true
    }

    pub(crate) fn insert_selection_into_free_type_command_from_context_menu(
        &mut self,
        target: TerminalPoint,
        replace_current_command: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(text) = self.selected_text_snapshot() else {
            return;
        };
        self.send_free_type_command_edit_text(target, &text, replace_current_command, cx);
    }

    fn selection_allowed(&self, shift: bool) -> bool {
        !self.settings.selection_requires_shift || shift
    }

    fn select_command_mark_at_position(
        &mut self,
        position: gpui::Point<Pixels>,
        mode: TermMode,
        cx: &mut Context<Self>,
    ) {
        if !command_mark_ui_available(self.settings.command_marks_enabled, mode) {
            return;
        }
        let absolute_line = self.absolute_line_for_position(position);
        let selected = self.command_mark_id_at_absolute_line(absolute_line);
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

    pub(crate) fn right_click_paste_requested(&self, mode: TermMode, modifiers: Modifiers) -> bool {
        right_click_paste_requested(self.settings.right_click_paste, mode, modifiers)
    }
}

fn mouse_tracking_active(mode: TermMode) -> bool {
    mode.intersects(TermMode::MOUSE_MODE)
}

fn right_click_paste_requested(enabled: bool, mode: TermMode, modifiers: Modifiers) -> bool {
    // Shift remains the local escape hatch for the context menu. Applications
    // that requested mouse tracking continue to own unmodified right clicks.
    enabled && !modifiers.shift && !mouse_tracking_active(mode)
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

fn terminal_viewport_row_for_position(
    position_y_from_origin: f32,
    smooth_scroll_y_offset: f32,
    line_height: f32,
    viewport_rows: usize,
) -> usize {
    // Mouse hit testing must follow the same fractional-row translation as the paint layer.
    let translated_y = position_y_from_origin - TERMINAL_CONTENT_PADDING - smooth_scroll_y_offset;
    ((translated_y / line_height).floor().max(0.0) as usize).min(viewport_rows.saturating_sub(1))
}

fn is_legacy_terminal_copy_shortcut(key: &str, modifiers: Modifiers) -> bool {
    key == "insert"
        && modifiers.control
        && !modifiers.platform
        && !modifiers.alt
        && !modifiers.shift
}

fn is_legacy_terminal_paste_shortcut(key: &str, modifiers: Modifiers) -> bool {
    key == "insert"
        && modifiers.shift
        && !modifiers.platform
        && !modifiers.alt
        && !modifiers.control
}

fn terminal_link_activation_allowed(modifiers: Modifiers, open_links_with_modifier: bool) -> bool {
    if !open_links_with_modifier {
        return true;
    }

    if cfg!(target_os = "macos") {
        modifiers.platform
    } else {
        modifiers.control
    }
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

fn free_type_mode_allows_command_edit(enabled: bool, mode: TermMode, modifiers: Modifiers) -> bool {
    free_type_mode_command_edit_rejection_reason(enabled, mode, modifiers).is_none()
}

fn free_type_mode_command_edit_rejection_reason(
    enabled: bool,
    mode: TermMode,
    modifiers: Modifiers,
) -> Option<&'static str> {
    if !enabled {
        return Some("disabled");
    }
    if mode.contains(TermMode::ALT_SCREEN) {
        return Some("alternate_screen");
    }
    if mouse_tracking_active(mode) {
        return Some("mouse_tracking");
    }
    if modifiers.shift {
        return Some("shift_modifier");
    }
    if modifiers.alt {
        return Some("alt_modifier");
    }
    if modifiers.control {
        return Some("control_modifier");
    }
    if modifiers.platform {
        return Some("platform_modifier");
    }
    None
}

fn free_type_drag_candidate_allowed(enabled: bool, mode: TermMode, modifiers: Modifiers) -> bool {
    if !enabled || mode.contains(TermMode::ALT_SCREEN) || mouse_tracking_active(mode) {
        return false;
    }

    free_type_drag_action(modifiers, true).is_some()
}

fn free_type_drag_action(
    modifiers: Modifiers,
    source_is_active_command_selection: bool,
) -> Option<FreeTypeDragAction> {
    if modifiers.shift || modifiers.platform || (modifiers.alt && modifiers.control) {
        return None;
    }
    if modifiers.alt {
        return Some(FreeTypeDragAction::ReplaceCommand);
    }
    if modifiers.control {
        return Some(FreeTypeDragAction::CopySelection);
    }
    Some(if source_is_active_command_selection {
        FreeTypeDragAction::MoveSelection
    } else {
        // Historical output cannot be deleted, so its legacy drag behavior
        // remains a safe insertion into the verified active command.
        FreeTypeDragAction::CopySelection
    })
}

fn free_type_delete_key_requests_selection_delete(key: &str, modifiers: Modifiers) -> bool {
    matches!(key, "backspace" | "back" | "delete")
        && !modifiers.platform
        && !modifiers.control
        && !modifiers.alt
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FreeTypeClipboardShortcut {
    Copy,
    Cut,
    Paste,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerminalEditorEditOperation {
    Copy,
    Cut,
    PreparePaste,
    DeleteSelection,
}

fn editor_operation_bytes(
    application: TerminalEditorApplication,
    operation: TerminalEditorEditOperation,
) -> &'static [u8] {
    match (application, operation) {
        (
            TerminalEditorApplication::Vim | TerminalEditorApplication::Neovim,
            TerminalEditorEditOperation::Copy,
        ) => b"\x1b[99;1~",
        (
            TerminalEditorApplication::Vim | TerminalEditorApplication::Neovim,
            TerminalEditorEditOperation::Cut,
        ) => b"\x1b[99;2~",
        (
            TerminalEditorApplication::Vim | TerminalEditorApplication::Neovim,
            TerminalEditorEditOperation::PreparePaste,
        ) => b"\x1b[99;3~",
        (
            TerminalEditorApplication::Vim | TerminalEditorApplication::Neovim,
            TerminalEditorEditOperation::DeleteSelection,
        ) => b"\x1b[99;4~",
        (TerminalEditorApplication::Emacs, TerminalEditorEditOperation::Copy) => b"\x1b[99;5~",
        (TerminalEditorApplication::Emacs, TerminalEditorEditOperation::Cut) => b"\x1b[99;6~",
        (TerminalEditorApplication::Emacs, TerminalEditorEditOperation::PreparePaste) => {
            b"\x1b[99;7~"
        }
        (TerminalEditorApplication::Emacs, TerminalEditorEditOperation::DeleteSelection) => {
            b"\x1b[99;8~"
        }
    }
}

fn free_type_clipboard_shortcut(
    key: &str,
    modifiers: Modifiers,
    uses_platform_modifier: bool,
) -> Option<FreeTypeClipboardShortcut> {
    let has_edit_modifier = if uses_platform_modifier {
        modifiers.platform && !modifiers.control
    } else {
        modifiers.control && !modifiers.platform
    };
    if !has_edit_modifier || modifiers.shift || modifiers.alt {
        return None;
    }

    match key.to_ascii_lowercase().as_str() {
        "c" => Some(FreeTypeClipboardShortcut::Copy),
        "x" => Some(FreeTypeClipboardShortcut::Cut),
        "v" => Some(FreeTypeClipboardShortcut::Paste),
        _ => None,
    }
}

fn free_type_selected_text_can_be_command_input(text: &str) -> bool {
    !text.is_empty() && !text.contains(['\r', '\n'])
}

fn free_type_drag_distance_exceeded(
    start: gpui::Point<Pixels>,
    current: gpui::Point<Pixels>,
) -> bool {
    let dx = f32::from(current.x - start.x);
    let dy = f32::from(current.y - start.y);
    dx.hypot(dy) >= TERMINAL_FREE_TYPE_DRAG_THRESHOLD_PX
}

fn selection_contains_grid_point(selection: TerminalSelection, point: TerminalGridPoint) -> bool {
    let (start, end) = selection.normalized();
    match selection.mode {
        TerminalSelectionMode::Block => {
            let line_start = start.line.min(end.line);
            let line_end = start.line.max(end.line);
            let col_start = start.col.min(end.col);
            let col_end = start.col.max(end.col);
            point.line >= line_start
                && point.line <= line_end
                && point.col >= col_start
                && point.col <= col_end
        }
        TerminalSelectionMode::Lines => point.line >= start.line && point.line <= end.line,
        TerminalSelectionMode::Simple | TerminalSelectionMode::Semantic => {
            if point.line < start.line || point.line > end.line {
                return false;
            }
            if start.line == end.line {
                point.col >= start.col && point.col <= end.col
            } else if point.line == start.line {
                point.col >= start.col
            } else if point.line == end.line {
                point.col <= end.col
            } else {
                true
            }
        }
    }
}

#[cfg(test)]
fn active_input_cursor_delta(
    snapshot: &TerminalSnapshot,
    target: TerminalPoint,
    input_state: Option<&TerminalAutosuggestInputState>,
) -> Option<isize> {
    active_input_cursor_move(snapshot, target, input_state).map(|cursor_move| cursor_move.delta)
}

fn active_input_cursor_move(
    snapshot: &TerminalSnapshot,
    target: TerminalPoint,
    input_state: Option<&TerminalAutosuggestInputState>,
) -> Option<FreeTypeCursorMove> {
    let cursor_row = snapshot.lines.get(snapshot.cursor_row)?;
    let target_row = snapshot.lines.get(target.row)?;
    if !cursor_row.active_input {
        return None;
    }

    let width = snapshot.cols.max(1);
    let cursor_col = snapshot.cursor_col.min(width.saturating_sub(1));
    let target_col = target.col.min(width.saturating_sub(1));
    if let Some(state) = input_state
        && let Some(target_index) = active_input_command_target_index(snapshot, target, state)
    {
        return command_cursor_move_to_index(state, target_index);
    }

    if !target_row.active_input {
        return None;
    }

    let (start, end) = active_input_block_bounds(snapshot)?;
    if !(start..=end).contains(&target.row) {
        return None;
    }

    let cursor_offset = grid_offset_from_block_start(snapshot.cursor_row, cursor_col, start, width);
    let raw_target_offset = grid_offset_from_block_start(target.row, target_col, start, width);
    if let Some(state) = input_state
        && let Some(range) =
            active_command_visible_range(Some(state), cursor_offset, start, end, width)
    {
        let target_offset = raw_target_offset.clamp(range.start, range.end);
        return tracked_command_cursor_move(state, target_offset, range.start);
    }

    Some(FreeTypeCursorMove::new(
        raw_target_offset as isize - cursor_offset as isize,
        FreeTypeCursorBoundary::None,
    ))
}

fn active_input_command_target_index(
    snapshot: &TerminalSnapshot,
    target: TerminalPoint,
    input_state: &TerminalAutosuggestInputState,
) -> Option<usize> {
    let cursor_row = snapshot.lines.get(snapshot.cursor_row)?;
    if !cursor_row.active_input {
        return None;
    }
    let width = snapshot.cols.max(1);
    let cursor_col = snapshot.cursor_col.min(width.saturating_sub(1));
    let target_col = target.col.min(width.saturating_sub(1));
    if let Some(range) =
        active_command_visible_range_from_viewport_cursor(snapshot, input_state, cursor_col, width)
        && viewport_offset_is_inside_range_row(target.row, range, width)
    {
        let raw_target_offset = viewport_grid_offset(target.row, target_col, width);
        let target_offset = raw_target_offset.clamp(range.start, range.end);
        return Some(command_cursor_index_for_cell(
            &input_state.value,
            target_offset.saturating_sub(range.start),
        ));
    }

    let (block_start, block_end) = active_input_block_bounds(snapshot)?;
    if !(block_start..=block_end).contains(&target.row) {
        return None;
    }
    let cursor_offset =
        grid_offset_from_block_start(snapshot.cursor_row, cursor_col, block_start, width);
    let range = active_command_visible_range(
        Some(input_state),
        cursor_offset,
        block_start,
        block_end,
        width,
    )?;
    let raw_target_offset =
        grid_offset_from_block_start(target.row, target_col, block_start, width);
    let target_offset = raw_target_offset.clamp(range.start, range.end);
    Some(command_cursor_index_for_cell(
        &input_state.value,
        target_offset.saturating_sub(range.start),
    ))
}

fn active_input_block_bounds(snapshot: &TerminalSnapshot) -> Option<(usize, usize)> {
    let cursor_row = snapshot.lines.get(snapshot.cursor_row)?;
    if !cursor_row.active_input {
        return None;
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

    Some((start, end))
}

fn active_command_visible_range_from_viewport_cursor(
    snapshot: &TerminalSnapshot,
    input_state: &TerminalAutosuggestInputState,
    cursor_col: usize,
    width: usize,
) -> Option<ActiveCommandVisibleRange> {
    // The input tracker can prove the command span even when the terminal
    // snapshot only marks the cursor row as active input.
    if input_state.cursor_index > input_state.value.len()
        || !input_state.value.is_char_boundary(input_state.cursor_index)
    {
        return None;
    }

    let cursor_offset = viewport_grid_offset(snapshot.cursor_row, cursor_col, width);
    let prefix_width = terminal_text_display_width(&input_state.value[..input_state.cursor_index]);
    let command_width = terminal_text_display_width(&input_state.value);
    let start = cursor_offset.checked_sub(prefix_width)?;
    let end = start.saturating_add(command_width);
    let viewport_cell_count = snapshot.rows.saturating_mul(width);
    if end > viewport_cell_count {
        return None;
    }

    Some(ActiveCommandVisibleRange { start, end })
}

fn viewport_grid_offset(row: usize, col: usize, width: usize) -> usize {
    row.saturating_mul(width).saturating_add(col)
}

fn viewport_offset_is_inside_range_row(
    row: usize,
    range: ActiveCommandVisibleRange,
    width: usize,
) -> bool {
    let row_start = row.saturating_mul(width);
    let row_end = row_start.saturating_add(width.saturating_sub(1));
    row_start <= range.end && row_end >= range.start
}

fn grid_offset_from_block_start(row: usize, col: usize, start: usize, width: usize) -> usize {
    row.saturating_sub(start)
        .saturating_mul(width)
        .saturating_add(col)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ActiveCommandVisibleRange {
    start: usize,
    end: usize,
}

fn active_command_visible_range(
    input_state: Option<&TerminalAutosuggestInputState>,
    cursor_offset: usize,
    block_start: usize,
    block_end: usize,
    width: usize,
) -> Option<ActiveCommandVisibleRange> {
    let state = input_state?;
    if state.cursor_index > state.value.len() || !state.value.is_char_boundary(state.cursor_index) {
        return None;
    }

    let prefix_width = terminal_text_display_width(&state.value[..state.cursor_index]);
    let command_width = terminal_text_display_width(&state.value);
    let start = cursor_offset.checked_sub(prefix_width)?;
    let end = start.saturating_add(command_width);
    let block_cell_count = block_end
        .saturating_sub(block_start)
        .saturating_add(1)
        .saturating_mul(width);
    if end > block_cell_count {
        return None;
    }

    Some(ActiveCommandVisibleRange { start, end })
}

fn terminal_text_display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FreeTypeCursorMove {
    delta: isize,
    boundary: FreeTypeCursorBoundary,
}

impl FreeTypeCursorMove {
    fn new(delta: isize, boundary: FreeTypeCursorBoundary) -> Self {
        Self { delta, boundary }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FreeTypeCursorBoundary {
    None,
    CommandStart,
    CommandEnd,
}

fn tracked_command_cursor_move(
    state: &TerminalAutosuggestInputState,
    target_offset: usize,
    command_start_offset: usize,
) -> Option<FreeTypeCursorMove> {
    // Arrow keys move through the remote line editor by logical characters, not
    // terminal cells. Convert the clicked cell back to a command string boundary
    // before deciding how many key presses to send.
    let target_cell = target_offset.saturating_sub(command_start_offset);
    let target_index = command_cursor_index_for_cell(&state.value, target_cell);
    command_cursor_move_to_index(state, target_index)
}

fn command_cursor_move_to_index(
    input_state: &TerminalAutosuggestInputState,
    target_index: usize,
) -> Option<FreeTypeCursorMove> {
    let delta = command_cursor_delta_between(input_state, target_index)?;
    let boundary = if target_index == 0 && input_state.cursor_index != 0 {
        FreeTypeCursorBoundary::CommandStart
    } else if target_index == input_state.value.len()
        && input_state.cursor_index != input_state.value.len()
    {
        FreeTypeCursorBoundary::CommandEnd
    } else {
        FreeTypeCursorBoundary::None
    };
    Some(FreeTypeCursorMove::new(delta, boundary))
}

fn command_cursor_index_for_cell(text: &str, target_cell: usize) -> usize {
    let mut cell_cursor = 0usize;
    for (byte_index, grapheme) in text.grapheme_indices(true) {
        if target_cell <= cell_cursor {
            return byte_index;
        }

        let width = UnicodeWidthStr::width(grapheme);
        let next_cell_cursor = cell_cursor.saturating_add(width);
        if target_cell < next_cell_cursor {
            // Wide cells and composed graphemes do not have a legal cursor stop
            // in the middle. Pick the nearest boundary so a click inside them
            // still moves by one remote editor step.
            let before_distance = target_cell.saturating_sub(cell_cursor);
            let after_distance = next_cell_cursor.saturating_sub(target_cell);
            return if before_distance >= after_distance {
                byte_index + grapheme.len()
            } else {
                byte_index
            };
        }
        cell_cursor = next_cell_cursor;
    }
    text.len()
}

fn command_cursor_step_count(text: &str) -> usize {
    text.graphemes(true).count()
}

fn free_type_selection_delete_bytes(
    snapshot: &TerminalSnapshot,
    selection: TerminalSelection,
    input_state: &TerminalAutosuggestInputState,
    mode: TermMode,
) -> Option<Vec<u8>> {
    let (start_index, end_index) =
        free_type_selection_command_range(snapshot, selection, input_state)?;
    if start_index >= end_index {
        return None;
    }

    let delete_count = command_cursor_step_count(&input_state.value[start_index..end_index]);
    if delete_count == 0 || delete_count > TERMINAL_FREE_TYPE_MAX_CURSOR_STEPS {
        return None;
    }

    let cursor_move = command_cursor_move_to_index(input_state, end_index)?;
    let mut bytes = free_type_cursor_move_bytes(cursor_move, mode).unwrap_or_default();
    for _ in 0..delete_count {
        // Ctrl+H is the portable backward-delete editing command across Bash
        // Readline, Zsh ZLE, and Fish insertion keymaps. CSI 3~ may be unbound
        // by ZLE, while physical Backspace/DEL remains user-configurable.
        bytes.push(0x08);
    }
    Some(bytes)
}

fn free_type_selected_command_text(
    snapshot: &TerminalSnapshot,
    selection: TerminalSelection,
    input_state: &TerminalAutosuggestInputState,
) -> Option<String> {
    let (start_index, end_index) =
        free_type_selection_command_range(snapshot, selection, input_state)?;
    (start_index < end_index).then(|| input_state.value[start_index..end_index].to_string())
}

fn free_type_selection_cut_payload(
    snapshot: &TerminalSnapshot,
    selection: TerminalSelection,
    input_state: &TerminalAutosuggestInputState,
    mode: TermMode,
) -> Option<(String, Vec<u8>)> {
    let text = free_type_selected_command_text(snapshot, selection, input_state)?;
    let bytes = free_type_selection_delete_bytes(snapshot, selection, input_state, mode)?;
    Some((text, bytes))
}

fn free_type_selection_move_bytes(
    snapshot: &TerminalSnapshot,
    selection: TerminalSelection,
    target: TerminalPoint,
    input_state: &TerminalAutosuggestInputState,
    mode: TermMode,
) -> Option<Vec<u8>> {
    let (source_start, source_end) =
        free_type_selection_command_range(snapshot, selection, input_state)?;
    let target_index = active_input_command_target_index(snapshot, target, input_state)?;
    if source_start <= target_index && target_index <= source_end {
        return Some(Vec::new());
    }

    let selected_text = input_state.value.get(source_start..source_end)?;
    let delete_count = command_cursor_step_count(selected_text);
    if delete_count == 0 || delete_count > TERMINAL_FREE_TYPE_MAX_CURSOR_STEPS {
        return None;
    }

    if target_index > source_end {
        // Insert first when moving right. Vi insertion keymaps cannot always
        // move their cursor past the final character, while inserting at the
        // current end and then returning to the source is portable.
        let mut bytes = free_type_cursor_move_bytes(
            command_cursor_move_to_index(input_state, target_index)?,
            mode,
        )
        .unwrap_or_default();
        bytes.extend_from_slice(selected_text.as_bytes());

        let mut expanded_value = input_state.value.clone();
        expanded_value.insert_str(target_index, selected_text);
        let expanded_state = TerminalAutosuggestInputState {
            value: expanded_value,
            cursor_index: target_index + selected_text.len(),
            is_cursor_at_end: target_index == input_state.value.len(),
        };
        let source_move = command_cursor_move_to_index(&expanded_state, source_end)?;
        bytes
            .extend_from_slice(&free_type_cursor_move_bytes(source_move, mode).unwrap_or_default());
        for _ in 0..delete_count {
            bytes.push(0x08);
        }
        return Some(bytes);
    }

    let mut bytes =
        free_type_cursor_move_bytes(command_cursor_move_to_index(input_state, source_end)?, mode)
            .unwrap_or_default();
    for _ in 0..delete_count {
        bytes.push(0x08);
    }

    let mut remaining_value = input_state.value.clone();
    remaining_value.replace_range(source_start..source_end, "");
    let remaining_state = TerminalAutosuggestInputState {
        value: remaining_value,
        cursor_index: source_start,
        is_cursor_at_end: source_start == input_state.value.len() - (source_end - source_start),
    };
    let post_delete_move = command_cursor_move_to_index(&remaining_state, target_index)?;
    bytes.extend_from_slice(
        &free_type_cursor_move_bytes(post_delete_move, mode).unwrap_or_default(),
    );
    bytes.extend_from_slice(selected_text.as_bytes());
    Some(bytes)
}

fn free_type_current_command_delete_bytes(
    input_state: &TerminalAutosuggestInputState,
    mode: TermMode,
) -> Option<Vec<u8>> {
    let delete_count = command_cursor_step_count(&input_state.value);
    if delete_count > TERMINAL_FREE_TYPE_MAX_CURSOR_STEPS {
        return None;
    }

    let mut bytes = free_type_cursor_move_bytes(
        command_cursor_move_to_index(input_state, input_state.value.len())?,
        mode,
    )
    .unwrap_or_default();
    for _ in 0..delete_count {
        bytes.push(0x08);
    }
    Some(bytes)
}

fn free_type_command_edit_bytes(
    snapshot: &TerminalSnapshot,
    target: TerminalPoint,
    input_state: &TerminalAutosuggestInputState,
    text: &str,
    replace_current_command: bool,
    mode: TermMode,
) -> Option<Vec<u8>> {
    // Build editor keystrokes only; the remote line editor remains authoritative.
    if !free_type_selected_text_can_be_command_input(text) {
        return None;
    }

    let cursor_move = active_input_cursor_move(snapshot, target, Some(input_state))?;
    let mut bytes = if replace_current_command {
        free_type_current_command_delete_bytes(input_state, mode)?
    } else {
        free_type_cursor_move_bytes(cursor_move, mode).unwrap_or_default()
    };
    bytes.extend_from_slice(text.as_bytes());
    Some(bytes)
}

fn free_type_selection_command_range(
    snapshot: &TerminalSnapshot,
    selection: TerminalSelection,
    input_state: &TerminalAutosuggestInputState,
) -> Option<(usize, usize)> {
    if !matches!(
        selection.mode,
        TerminalSelectionMode::Simple
            | TerminalSelectionMode::Semantic
            | TerminalSelectionMode::Lines
    ) || selection.is_empty()
    {
        return None;
    }

    let (block_start, block_end) = active_input_block_bounds(snapshot)?;
    let width = snapshot.cols.max(1);
    let cursor_col = snapshot.cursor_col.min(width.saturating_sub(1));
    let cursor_offset =
        grid_offset_from_block_start(snapshot.cursor_row, cursor_col, block_start, width);
    let command_range = active_command_visible_range(
        Some(input_state),
        cursor_offset,
        block_start,
        block_end,
        width,
    )?;
    let (selection_start, selection_end) = selection.normalized();
    let start_offset = selection_point_offset(snapshot, selection_start, block_start, width)?;
    let end_offset =
        selection_point_offset(snapshot, selection_end, block_start, width)?.saturating_add(1);
    let start_offset = start_offset.clamp(command_range.start, command_range.end);
    let end_offset = end_offset.clamp(command_range.start, command_range.end);
    if start_offset >= end_offset {
        return None;
    }

    let start_index =
        command_cursor_index_for_cell(&input_state.value, start_offset - command_range.start);
    let end_index =
        command_cursor_index_for_cell(&input_state.value, end_offset - command_range.start);
    Some((start_index.min(end_index), start_index.max(end_index)))
}

fn selection_point_offset(
    snapshot: &TerminalSnapshot,
    point: TerminalGridPoint,
    block_start: usize,
    width: usize,
) -> Option<usize> {
    let row = viewport_row_for_selection_line(snapshot, point.line)?;
    snapshot
        .lines
        .get(row)
        .is_some_and(|row| row.active_input)
        .then_some(())?;
    Some(grid_offset_from_block_start(
        row,
        point.col.min(width.saturating_sub(1)),
        block_start,
        width,
    ))
}

fn viewport_row_for_selection_line(snapshot: &TerminalSnapshot, line: i32) -> Option<usize> {
    let row = line + snapshot.display_offset as i32;
    usize::try_from(row).ok().filter(|row| *row < snapshot.rows)
}

fn command_cursor_delta_between(
    input_state: &TerminalAutosuggestInputState,
    target_index: usize,
) -> Option<isize> {
    if target_index > input_state.value.len()
        || !input_state.value.is_char_boundary(target_index)
        || input_state.cursor_index > input_state.value.len()
        || !input_state.value.is_char_boundary(input_state.cursor_index)
    {
        return None;
    }

    if target_index > input_state.cursor_index {
        Some(
            command_cursor_step_count(&input_state.value[input_state.cursor_index..target_index])
                as isize,
        )
    } else {
        Some(
            -(command_cursor_step_count(&input_state.value[target_index..input_state.cursor_index])
                as isize),
        )
    }
}

fn free_type_cursor_move_bytes(cursor_move: FreeTypeCursorMove, mode: TermMode) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    if cursor_move.delta.unsigned_abs() > 1 {
        match cursor_move.boundary {
            FreeTypeCursorBoundary::CommandStart => bytes.extend_from_slice(home_key_bytes(mode)),
            FreeTypeCursorBoundary::CommandEnd => bytes.extend_from_slice(end_key_bytes(mode)),
            FreeTypeCursorBoundary::None => {}
        }
    }
    // Boundary keys let Vi insertion modes reach the position after the final
    // character. Repeated arrows are a fallback for ZLE keymaps that leave the
    // xterm Home/End sequences unbound.
    if let Some(motion) = cursor_motion_bytes(cursor_move.delta, mode) {
        bytes.extend_from_slice(&motion);
    }
    (!bytes.is_empty()).then_some(bytes)
}

fn home_key_bytes(mode: TermMode) -> &'static [u8] {
    if mode.contains(TermMode::APP_CURSOR) {
        b"\x1bOH"
    } else {
        b"\x1b[H"
    }
}

fn end_key_bytes(mode: TermMode) -> &'static [u8] {
    if mode.contains(TermMode::APP_CURSOR) {
        b"\x1bOF"
    } else {
        b"\x1b[F"
    }
}

fn cursor_motion_bytes(delta: isize, mode: TermMode) -> Option<Vec<u8>> {
    if delta == 0 {
        return None;
    }

    let app_cursor = mode.contains(TermMode::APP_CURSOR);
    let sequence = match (delta.is_positive(), app_cursor) {
        (true, true) => b"\x1bOC".as_slice(),
        (true, false) => b"\x1b[C".as_slice(),
        (false, true) => b"\x1bOD".as_slice(),
        (false, false) => b"\x1b[D".as_slice(),
    };
    let steps = delta.unsigned_abs();
    if steps > TERMINAL_FREE_TYPE_MAX_CURSOR_STEPS {
        return None;
    }

    let mut bytes = Vec::with_capacity(sequence.len() * steps);
    for _ in 0..steps {
        bytes.extend_from_slice(sequence);
    }
    Some(bytes)
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

    #[cfg(unix)]
    use oxideterm_terminal::{
        GraphicsOptions, LocalPtyConfig, ShellInfo, TerminalEncoding, TerminalEvent,
        TerminalSession, VIM_FREE_TYPE_INTEGRATION_SOURCE,
    };
    use oxideterm_terminal::{TerminalAttrs, TerminalCell, TerminalColor, TerminalCursorShape};
    use oxideterm_terminal::{
        TerminalEditorApplication, TerminalEditorCapabilities, TerminalEditorClipboardOperation,
        TerminalEditorIntegrationEvent, TerminalEditorMode, TerminalEditorSelection,
    };

    fn test_cell(ch: char) -> TerminalCell {
        TerminalCell {
            ch,
            wide: false,
            fg: TerminalColor::rgb(0xe6, 0xe8, 0xeb),
            bg: TerminalColor::rgb(0x0d, 0x0f, 0x12),
            style_origin: Default::default(),
            attrs: TerminalAttrs::default(),
            extra: None,
            cursor: false,
        }
    }

    #[test]
    fn right_click_paste_preserves_context_menu_and_remote_mouse_ownership() {
        assert!(right_click_paste_requested(
            true,
            TermMode::NONE,
            Modifiers::default()
        ));
        assert!(!right_click_paste_requested(
            false,
            TermMode::NONE,
            Modifiers::default()
        ));
        assert!(!right_click_paste_requested(
            true,
            TermMode::NONE,
            Modifiers {
                shift: true,
                ..Modifiers::default()
            }
        ));
        assert!(!right_click_paste_requested(
            true,
            TermMode::MOUSE_MODE,
            Modifiers::default()
        ));
    }

    fn test_row(text: &str, active_input: bool) -> TerminalRow {
        let mut cells = text.chars().map(test_cell).collect::<Vec<_>>();
        if cells.is_empty() {
            cells.push(test_cell(' '));
        }
        let mut row = TerminalRow {
            line_id: 0,
            source_id: 0,
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
        test_snapshot_with_cursor(lines, cursor_row, 0, 120)
    }

    fn test_snapshot_with_cursor(
        lines: Vec<TerminalRow>,
        cursor_row: usize,
        cursor_col: usize,
        cols: usize,
    ) -> TerminalSnapshot {
        TerminalSnapshot {
            generation: 1,
            cols,
            rows: lines.len(),
            cursor_col,
            cursor_row,
            cursor_shape: TerminalCursorShape::Block,
            display_offset: 0,
            scrollback_lines: 0,
            lines,
            images: Vec::new(),
        }
    }

    #[test]
    fn editor_adapter_gate_requires_alt_screen_fresh_state_and_matching_process() {
        let integration = TerminalEditorIntegrationEvent {
            application: TerminalEditorApplication::Vim,
            mode: TerminalEditorMode::Visual,
            selection: TerminalEditorSelection::Character,
            capabilities: TerminalEditorCapabilities {
                mouse: true,
                clipboard: true,
                edit: true,
            },
            active: true,
        };
        assert!(crate::app::editor_integration_is_usable(
            true,
            TermMode::ALT_SCREEN | TermMode::MOUSE_MODE,
            integration,
            Duration::from_millis(100),
            Some("/usr/bin/vim"),
        ));
        assert!(crate::app::editor_integration_is_usable(
            true,
            TermMode::ALT_SCREEN,
            integration,
            Duration::from_millis(100),
            None,
        ));
        assert!(!crate::app::editor_integration_is_usable(
            true,
            TermMode::NONE,
            integration,
            Duration::from_millis(100),
            Some("vim"),
        ));
        assert!(!crate::app::editor_integration_is_usable(
            true,
            TermMode::ALT_SCREEN,
            integration,
            crate::app::EDITOR_INTEGRATION_HEARTBEAT_TIMEOUT + Duration::from_millis(1),
            Some("vim"),
        ));
        assert!(!crate::app::editor_integration_is_usable(
            true,
            TermMode::ALT_SCREEN,
            integration,
            Duration::from_millis(100),
            Some("tmux"),
        ));
    }

    #[test]
    fn editor_operation_sequences_are_namespaced_per_adapter_family() {
        for application in [
            TerminalEditorApplication::Vim,
            TerminalEditorApplication::Neovim,
        ] {
            assert_eq!(
                editor_operation_bytes(application, TerminalEditorEditOperation::Copy),
                b"\x1b[99;1~"
            );
            assert_eq!(
                editor_operation_bytes(application, TerminalEditorEditOperation::DeleteSelection),
                b"\x1b[99;4~"
            );
        }
        assert_eq!(
            editor_operation_bytes(
                TerminalEditorApplication::Emacs,
                TerminalEditorEditOperation::Copy
            ),
            b"\x1b[99;5~"
        );
        assert_eq!(
            editor_operation_bytes(
                TerminalEditorApplication::Emacs,
                TerminalEditorEditOperation::DeleteSelection
            ),
            b"\x1b[99;8~"
        );
    }

    #[cfg(unix)]
    #[derive(Clone, Copy)]
    struct RealPtyShellModeCase {
        shell_id: &'static str,
        mode_name: &'static str,
        binding_command: &'static str,
    }

    #[cfg(unix)]
    #[test]
    fn free_type_sequences_edit_real_shell_line_modes() {
        let cases = [
            RealPtyShellModeCase {
                shell_id: "bash",
                mode_name: "default",
                binding_command: "",
            },
            RealPtyShellModeCase {
                shell_id: "bash",
                mode_name: "emacs",
                binding_command: "set -o emacs; ",
            },
            RealPtyShellModeCase {
                shell_id: "bash",
                mode_name: "vi",
                binding_command: "set -o vi; ",
            },
            RealPtyShellModeCase {
                shell_id: "zsh",
                mode_name: "default",
                binding_command: "",
            },
            RealPtyShellModeCase {
                shell_id: "zsh",
                mode_name: "emacs",
                binding_command: "bindkey -e; ",
            },
            RealPtyShellModeCase {
                shell_id: "zsh",
                mode_name: "vi",
                binding_command: "bindkey -v; KEYTIMEOUT=1; ",
            },
            RealPtyShellModeCase {
                shell_id: "fish",
                mode_name: "default",
                binding_command: "",
            },
            RealPtyShellModeCase {
                shell_id: "fish",
                mode_name: "emacs",
                binding_command: "fish_default_key_bindings; ",
            },
            RealPtyShellModeCase {
                shell_id: "fish",
                mode_name: "vi",
                binding_command: "fish_vi_key_bindings; ",
            },
        ];
        let mut exercised = Vec::new();

        for case in cases {
            let Some(shell_path) = find_real_pty_shell(case.shell_id) else {
                continue;
            };
            if let Err(error) = validate_free_type_in_real_pty(case, shell_path) {
                panic!(
                    "Free Type PTY validation failed for {} {} mode: {error}",
                    case.shell_id, case.mode_name
                );
            }
            exercised.push(format!("{}:{}", case.shell_id, case.mode_name));
        }

        assert!(
            !exercised.is_empty(),
            "at least one supported interactive shell must be available"
        );
    }

    #[cfg(unix)]
    #[test]
    fn free_type_adapter_edits_real_full_screen_vim_session() {
        let Some(vim_path) = find_real_pty_shell("vim") else {
            return;
        };
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock after Unix epoch")
            .as_nanos();
        let fixture_dir = std::env::temp_dir().join(format!(
            "oxideterm-free-type-vim-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&fixture_dir).expect("create Vim fixture directory");
        let integration_path = fixture_dir.join("oxideterm-free-type.vim");
        let document_path = fixture_dir.join("document.txt");
        std::fs::write(&integration_path, VIM_FREE_TYPE_INTEGRATION_SOURCE)
            .expect("write bundled Vim integration");
        std::fs::write(&document_path, "alpha beta\n").expect("write Vim fixture document");

        let config = LocalPtyConfig {
            shell: Some(ShellInfo::new("vim", "Vim", vim_path).with_args(vec![
                "-Nu".to_string(),
                "NONE".to_string(),
                "-n".to_string(),
                "-i".to_string(),
                "NONE".to_string(),
                "-S".to_string(),
                integration_path.to_string_lossy().into_owned(),
                document_path.to_string_lossy().into_owned(),
            ])),
            load_profile: false,
            ..LocalPtyConfig::default()
        };
        let mut session = TerminalSession::local_with_config_graphics_and_encoding(
            100,
            30,
            config,
            GraphicsOptions::default(),
            TerminalEncoding::Utf8,
            200,
        )
        .expect("spawn real Vim PTY");

        // The test owns the editor process and fixture directory even when an
        // adapter assertion fails.
        let result = validate_free_type_in_running_vim(&mut session, &document_path);
        session.shutdown();
        let _ = std::fs::remove_dir_all(&fixture_dir);
        if let Err(error) = result {
            panic!("Free Type full-screen Vim validation failed: {error}");
        }
    }

    #[cfg(unix)]
    fn validate_free_type_in_running_vim(
        session: &mut TerminalSession,
        document_path: &std::path::Path,
    ) -> Result<(), String> {
        wait_for_vim_editor_state(
            session,
            TerminalEditorMode::Normal,
            TerminalEditorSelection::None,
        )?;
        let mode = session.mode();
        if !mode.contains(TermMode::ALT_SCREEN) || !mode.intersects(TermMode::MOUSE_MODE) {
            return Err(format!(
                "Vim adapter did not retain alternate-screen mouse ownership: {mode:?}"
            ));
        }

        session
            .write_protocol_bytes(b"vllll")
            .map_err(|error| format!("visual selection failed: {error}"))?;
        wait_for_vim_editor_state(
            session,
            TerminalEditorMode::Visual,
            TerminalEditorSelection::Character,
        )?;

        session
            .write_protocol_bytes(editor_operation_bytes(
                TerminalEditorApplication::Vim,
                TerminalEditorEditOperation::Copy,
            ))
            .map_err(|error| format!("copy operation failed: {error}"))?;
        wait_for_vim_clipboard(session, TerminalEditorClipboardOperation::Copy, "alpha")?;

        session
            .write_protocol_bytes(editor_operation_bytes(
                TerminalEditorApplication::Vim,
                TerminalEditorEditOperation::Cut,
            ))
            .map_err(|error| format!("cut operation failed: {error}"))?;
        wait_for_vim_clipboard(session, TerminalEditorClipboardOperation::Cut, "alpha")?;

        session
            .write_protocol_bytes(editor_operation_bytes(
                TerminalEditorApplication::Vim,
                TerminalEditorEditOperation::PreparePaste,
            ))
            .map_err(|error| format!("paste preparation failed: {error}"))?;
        session
            .write_text("alpha")
            .map_err(|error| format!("paste text failed: {error}"))?;
        session
            .write_protocol_bytes(b"\x1b:wq\r")
            .map_err(|error| format!("Vim save failed: {error}"))?;
        wait_for_real_pty_exit(session)?;

        let document = std::fs::read_to_string(document_path)
            .map_err(|error| format!("read saved Vim document failed: {error}"))?;
        if document != "alpha beta\n" {
            return Err(format!("unexpected saved Vim document: {document:?}"));
        }
        Ok(())
    }

    #[cfg(unix)]
    fn wait_for_vim_editor_state(
        session: &mut TerminalSession,
        expected_mode: TerminalEditorMode,
        expected_selection: TerminalEditorSelection,
    ) -> Result<(), String> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            session.read_pending();
            for event in session.take_events() {
                if let TerminalEvent::EditorIntegration(editor) = event
                    && editor.active
                    && editor.application == TerminalEditorApplication::Vim
                    && editor.mode == expected_mode
                    && editor.selection == expected_selection
                {
                    return Ok(());
                }
            }
            if !session.lifecycle().is_running() {
                return Err("Vim exited before reporting editor state".to_string());
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        Err(format!(
            "timed out waiting for Vim {expected_mode:?}/{expected_selection:?}"
        ))
    }

    #[cfg(unix)]
    fn wait_for_vim_clipboard(
        session: &mut TerminalSession,
        expected_operation: TerminalEditorClipboardOperation,
        expected_text: &str,
    ) -> Result<(), String> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            session.read_pending();
            for event in session.take_events() {
                if let TerminalEvent::EditorClipboard(clipboard) = event
                    && clipboard.application == TerminalEditorApplication::Vim
                    && clipboard.operation == expected_operation
                {
                    return (clipboard.text.as_str() == expected_text)
                        .then_some(())
                        .ok_or_else(|| {
                            format!(
                                "unexpected Vim clipboard text length: {}",
                                clipboard.text.len()
                            )
                        });
                }
            }
            if !session.lifecycle().is_running() {
                return Err("Vim exited before reporting clipboard text".to_string());
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        Err(format!(
            "timed out waiting for Vim {expected_operation:?} clipboard event; screen={:?}",
            session.buffer_text()
        ))
    }

    #[cfg(unix)]
    fn wait_for_real_pty_exit(session: &mut TerminalSession) -> Result<(), String> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            session.read_pending();
            if !session.lifecycle().is_running() {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        Err("timed out waiting for real PTY process exit".to_string())
    }

    #[cfg(unix)]
    fn validate_free_type_in_real_pty(
        case: RealPtyShellModeCase,
        shell_path: std::path::PathBuf,
    ) -> Result<(), String> {
        let config = LocalPtyConfig {
            shell: Some(ShellInfo::new(case.shell_id, case.shell_id, shell_path)),
            env: std::collections::HashMap::from([
                ("INPUTRC".to_string(), "/dev/null".to_string()),
                ("EDITOR".to_string(), String::new()),
                ("VISUAL".to_string(), String::new()),
            ]),
            load_profile: false,
            ..LocalPtyConfig::default()
        };
        let mut session = TerminalSession::local_with_config_graphics_and_encoding(
            160,
            24,
            config,
            GraphicsOptions::default(),
            TerminalEncoding::Utf8,
            200,
        )
        .map_err(|error| format!("spawn failed: {error}"))?;

        // Every real PTY test owns its shell and always shuts it down before
        // returning, including validation failures.
        let result = validate_free_type_in_running_pty(&mut session, case);
        session.shutdown();
        result
    }

    #[cfg(unix)]
    fn validate_free_type_in_running_pty(
        session: &mut TerminalSession,
        case: RealPtyShellModeCase,
    ) -> Result<(), String> {
        let prompt_setup = match case.shell_id {
            "fish" => "function fish_prompt; printf 'OT> '; end; function fish_right_prompt; end; ",
            "zsh" => "PROMPT='OT> '; RPROMPT=''; ",
            _ => "PS1='OT> '; PS2='OT2> '; ",
        };
        let setup_command = format!(
            "{}{}printf 'OT_SETUP_%s\\n' READY",
            case.binding_command, prompt_setup
        );
        submit_real_pty_command(session, &setup_command, &[])?;
        wait_for_real_pty_text(session, "OT_SETUP_READY")?;

        let insert_command = "printf 'OT_RESULT:%s\\n' abef";
        let insert_target = insert_command
            .rfind("abef")
            .map(|index| index + 2)
            .ok_or_else(|| "insert fixture is missing its argument".to_string())?;
        let insert_snapshot = test_snapshot_with_cursor(
            vec![test_row(insert_command, true)],
            0,
            insert_command.len(),
            160,
        );
        let insert_state = TerminalAutosuggestInputState {
            value: insert_command.to_string(),
            cursor_index: insert_command.len(),
            is_cursor_at_end: true,
        };
        let insert_bytes = free_type_command_edit_bytes(
            &insert_snapshot,
            TerminalPoint {
                row: 0,
                col: insert_target,
            },
            &insert_state,
            "cd",
            false,
            session.mode(),
        )
        .ok_or_else(|| "Free Type insertion bytes were not generated".to_string())?;
        submit_real_pty_command(session, insert_command, &insert_bytes)?;
        wait_for_real_pty_text(session, "OT_RESULT:abcdef")?;

        let move_command = "printf 'OT_MOVE:%s\\n' cdefab";
        let source_start = move_command
            .rfind("cdefab")
            .ok_or_else(|| "move fixture is missing its argument".to_string())?;
        let move_snapshot = test_snapshot_with_cursor(
            vec![test_row(move_command, true)],
            0,
            move_command.len(),
            160,
        );
        let move_selection = TerminalSelection {
            anchor: TerminalGridPoint {
                line: 0,
                col: source_start,
            },
            head: TerminalGridPoint {
                line: 0,
                col: source_start + 1,
            },
            mode: TerminalSelectionMode::Simple,
        };
        let move_state = TerminalAutosuggestInputState {
            value: move_command.to_string(),
            cursor_index: move_command.len(),
            is_cursor_at_end: true,
        };
        let move_bytes = free_type_selection_move_bytes(
            &move_snapshot,
            move_selection,
            TerminalPoint {
                row: 0,
                col: move_command.len(),
            },
            &move_state,
            session.mode(),
        )
        .ok_or_else(|| "Free Type move bytes were not generated".to_string())?;
        submit_real_pty_command(session, move_command, &move_bytes)?;
        wait_for_real_pty_text(session, "OT_MOVE:efabcd")?;
        Ok(())
    }

    #[cfg(unix)]
    fn submit_real_pty_command(
        session: &mut TerminalSession,
        command: &str,
        edit_bytes: &[u8],
    ) -> Result<(), String> {
        session
            .write_text(command)
            .map_err(|error| format!("command input failed: {error}"))?;
        session
            .write_protocol_bytes(edit_bytes)
            .map_err(|error| format!("Free Type edit input failed: {error}"))?;
        session
            .write_protocol_bytes(b"\r")
            .map_err(|error| format!("command submit failed: {error}"))
    }

    #[cfg(unix)]
    fn wait_for_real_pty_text(session: &mut TerminalSession, expected: &str) -> Result<(), String> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut screen = String::new();
        while std::time::Instant::now() < deadline {
            session.read_pending();
            screen = session.buffer_text();
            if screen.contains(expected) {
                return Ok(());
            }
            if !session.lifecycle().is_running() {
                return Err(format!(
                    "shell exited before producing {expected:?}; screen={screen:?}"
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        Err(format!(
            "timed out waiting for {expected:?}; screen={screen:?}"
        ))
    }

    #[cfg(unix)]
    fn find_real_pty_shell(name: &str) -> Option<std::path::PathBuf> {
        std::env::var_os("PATH")
            .into_iter()
            .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
            .map(|directory| directory.join(name))
            .find(|candidate| candidate.is_file())
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
    fn linux_insert_shortcuts_map_to_clipboard_actions() {
        assert!(is_legacy_terminal_copy_shortcut(
            "insert",
            Modifiers {
                control: true,
                ..Modifiers::default()
            }
        ));
        assert!(is_legacy_terminal_paste_shortcut(
            "insert",
            Modifiers {
                shift: true,
                ..Modifiers::default()
            }
        ));
        assert!(!is_legacy_terminal_copy_shortcut(
            "insert",
            Modifiers {
                control: true,
                shift: true,
                ..Modifiers::default()
            }
        ));
    }

    #[test]
    fn free_type_clipboard_shortcuts_follow_platform_edit_modifiers() {
        let mac_modifiers = Modifiers {
            platform: true,
            ..Modifiers::default()
        };
        let other_modifiers = Modifiers {
            control: true,
            ..Modifiers::default()
        };

        assert_eq!(
            free_type_clipboard_shortcut("c", mac_modifiers, true),
            Some(FreeTypeClipboardShortcut::Copy)
        );
        assert_eq!(
            free_type_clipboard_shortcut("x", other_modifiers, false),
            Some(FreeTypeClipboardShortcut::Cut)
        );
        assert_eq!(
            free_type_clipboard_shortcut("v", other_modifiers, false),
            Some(FreeTypeClipboardShortcut::Paste)
        );
        assert_eq!(
            free_type_clipboard_shortcut("c", other_modifiers, true),
            None
        );
        assert_eq!(
            free_type_clipboard_shortcut(
                "v",
                Modifiers {
                    control: true,
                    shift: true,
                    ..Modifiers::default()
                },
                false
            ),
            None
        );
    }

    #[test]
    fn terminal_link_activation_follows_modifier_preference() {
        assert!(terminal_link_activation_allowed(
            Modifiers::default(),
            false
        ));
        assert!(!terminal_link_activation_allowed(
            Modifiers::default(),
            true
        ));
        let link_modifier = if cfg!(target_os = "macos") {
            Modifiers {
                platform: true,
                ..Modifiers::default()
            }
        } else {
            Modifiers {
                control: true,
                ..Modifiers::default()
            }
        };
        assert!(terminal_link_activation_allowed(link_modifier, true));
    }

    #[test]
    fn privilege_prompt_enter_submit_rules_preserve_confirmation_boundaries() {
        for (modifiers, has_inline_hint, expected) in [
            (Modifiers::default(), false, false),
            (Modifiers::default(), true, true),
            (
                Modifiers {
                    shift: true,
                    ..Modifiers::default()
                },
                true,
                false,
            ),
        ] {
            assert_eq!(
                privilege_prompt_enter_requests_submit("enter", modifiers, has_inline_hint),
                expected
            );
        }
    }

    #[test]
    fn free_type_mode_respects_command_edit_conflict_guards() {
        assert!(free_type_mode_allows_command_edit(
            true,
            TermMode::default(),
            Modifiers::default()
        ));
        assert!(!free_type_mode_allows_command_edit(
            false,
            TermMode::default(),
            Modifiers::default()
        ));
        assert!(!free_type_mode_allows_command_edit(
            true,
            TermMode::ALT_SCREEN,
            Modifiers::default()
        ));
        assert!(!free_type_mode_allows_command_edit(
            true,
            TermMode::MOUSE_REPORT_CLICK,
            Modifiers::default()
        ));
        assert!(free_type_mode_allows_command_edit(
            true,
            TermMode::BRACKETED_PASTE,
            Modifiers::default()
        ));
        assert!(free_type_mode_allows_command_edit(
            true,
            TermMode::KITTY_KEYBOARD_PROTOCOL,
            Modifiers::default()
        ));
        assert!(!free_type_mode_allows_command_edit(
            true,
            TermMode::default(),
            Modifiers {
                shift: true,
                ..Modifiers::default()
            }
        ));
        assert!(!free_type_mode_allows_command_edit(
            true,
            TermMode::default(),
            Modifiers {
                platform: true,
                ..Modifiers::default()
            }
        ));
    }

    #[test]
    fn free_type_drag_candidate_allows_move_copy_and_replace_but_rejects_conflicts() {
        assert!(free_type_drag_candidate_allowed(
            true,
            TermMode::default(),
            Modifiers::default()
        ));
        assert!(free_type_drag_candidate_allowed(
            true,
            TermMode::default(),
            Modifiers {
                alt: true,
                ..Modifiers::default()
            }
        ));
        assert!(free_type_drag_candidate_allowed(
            true,
            TermMode::default(),
            Modifiers {
                control: true,
                ..Modifiers::default()
            }
        ));
        assert!(!free_type_drag_candidate_allowed(
            false,
            TermMode::default(),
            Modifiers::default()
        ));
        assert!(!free_type_drag_candidate_allowed(
            true,
            TermMode::ALT_SCREEN,
            Modifiers::default()
        ));
        assert!(!free_type_drag_candidate_allowed(
            true,
            TermMode::MOUSE_REPORT_CLICK,
            Modifiers::default()
        ));
        assert!(!free_type_drag_candidate_allowed(
            true,
            TermMode::default(),
            Modifiers {
                shift: true,
                ..Modifiers::default()
            }
        ));
        assert!(!free_type_drag_candidate_allowed(
            true,
            TermMode::default(),
            Modifiers {
                control: true,
                alt: true,
                ..Modifiers::default()
            }
        ));
        assert_eq!(
            free_type_drag_action(Modifiers::default(), true),
            Some(FreeTypeDragAction::MoveSelection)
        );
        assert_eq!(
            free_type_drag_action(
                Modifiers {
                    control: true,
                    ..Modifiers::default()
                },
                true,
            ),
            Some(FreeTypeDragAction::CopySelection)
        );
        assert_eq!(
            free_type_drag_action(
                Modifiers {
                    alt: true,
                    ..Modifiers::default()
                },
                true,
            ),
            Some(FreeTypeDragAction::ReplaceCommand)
        );
        assert_eq!(
            free_type_drag_action(Modifiers::default(), false),
            Some(FreeTypeDragAction::CopySelection)
        );
    }

    #[test]
    fn free_type_selected_text_command_input_rejects_multiline_text() {
        assert!(free_type_selected_text_can_be_command_input("plain-text"));
        assert!(!free_type_selected_text_can_be_command_input(""));
        assert!(!free_type_selected_text_can_be_command_input(
            "echo one\necho two"
        ));
        assert!(!free_type_selected_text_can_be_command_input(
            "echo one\recho two"
        ));
    }

    #[test]
    fn free_type_cursor_delta_stays_inside_active_input_block() {
        let snapshot = test_snapshot_with_cursor(
            vec![
                test_row("output", false),
                test_row("prompt command", true),
                test_row("wrapped continuation", true),
            ],
            1,
            7,
            20,
        );

        assert_eq!(
            active_input_cursor_delta(&snapshot, TerminalPoint { row: 1, col: 12 }, None),
            Some(5)
        );
        assert_eq!(
            active_input_cursor_delta(&snapshot, TerminalPoint { row: 2, col: 2 }, None),
            Some(15)
        );
        assert_eq!(
            active_input_cursor_delta(&snapshot, TerminalPoint { row: 0, col: 2 }, None),
            None
        );
    }

    #[test]
    fn free_type_cursor_delta_uses_tracked_command_range_when_target_row_is_unmarked() {
        let snapshot = test_snapshot_with_cursor(
            vec![test_row("$ abc", false), test_row("def", true)],
            1,
            2,
            6,
        );
        let input_state = TerminalAutosuggestInputState {
            value: "abcdef".to_string(),
            cursor_index: 6,
            is_cursor_at_end: true,
        };

        assert_eq!(
            active_input_cursor_delta(
                &snapshot,
                TerminalPoint { row: 0, col: 3 },
                Some(&input_state)
            ),
            Some(-5)
        );
    }

    #[test]
    fn free_type_cursor_delta_moves_by_characters_for_wide_cells() {
        let snapshot = test_snapshot_with_cursor(vec![test_row("$ 你a", true)], 0, 5, 20);
        let input_state = TerminalAutosuggestInputState {
            value: "你a".to_string(),
            cursor_index: "你a".len(),
            is_cursor_at_end: true,
        };

        assert_eq!(
            active_input_cursor_delta(
                &snapshot,
                TerminalPoint { row: 0, col: 4 },
                Some(&input_state)
            ),
            Some(-1)
        );
        assert_eq!(
            active_input_cursor_delta(
                &snapshot,
                TerminalPoint { row: 0, col: 3 },
                Some(&input_state)
            ),
            Some(-1)
        );
        assert_eq!(
            active_input_cursor_delta(
                &snapshot,
                TerminalPoint { row: 0, col: 2 },
                Some(&input_state)
            ),
            Some(-2)
        );
    }

    #[test]
    fn free_type_cursor_delta_keeps_grapheme_clusters_together() {
        let combining = "e\u{301}x";
        let snapshot = test_snapshot_with_cursor(vec![test_row("$ e\u{301}x", true)], 0, 4, 20);
        let input_state = TerminalAutosuggestInputState {
            value: combining.to_string(),
            cursor_index: combining.len(),
            is_cursor_at_end: true,
        };

        assert_eq!(
            command_cursor_index_for_cell(combining, 1),
            "e\u{301}".len()
        );
        assert_eq!(
            active_input_cursor_delta(
                &snapshot,
                TerminalPoint { row: 0, col: 3 },
                Some(&input_state)
            ),
            Some(-1)
        );

        let family = "👨\u{200d}👩\u{200d}👧\u{200d}👦x";
        assert_eq!(
            command_cursor_index_for_cell(family, 1),
            "👨\u{200d}👩\u{200d}👧\u{200d}👦".len()
        );
    }

    #[test]
    fn free_type_cursor_delta_maps_zero_width_characters_without_extra_cells() {
        let text = "a\u{200b}b";

        assert_eq!(command_cursor_index_for_cell(text, 1), "a".len());
        assert_eq!(command_cursor_index_for_cell(text, 2), text.len());

        let input_state = TerminalAutosuggestInputState {
            value: text.to_string(),
            cursor_index: text.len(),
            is_cursor_at_end: true,
        };
        assert_eq!(
            command_cursor_delta_between(&input_state, "a".len()),
            Some(-2)
        );
    }

    #[test]
    fn free_type_cursor_delta_maps_tracked_command_across_wrapped_rows() {
        let snapshot = test_snapshot_with_cursor(
            vec![test_row("$ ab", true), test_row("cdef", true)],
            1,
            4,
            5,
        );
        let input_state = TerminalAutosuggestInputState {
            value: "abcdef".to_string(),
            cursor_index: 6,
            is_cursor_at_end: true,
        };

        assert_eq!(
            active_input_cursor_delta(
                &snapshot,
                TerminalPoint { row: 0, col: 2 },
                Some(&input_state)
            ),
            Some(-6)
        );
        assert_eq!(
            active_input_cursor_delta(
                &snapshot,
                TerminalPoint { row: 0, col: 4 },
                Some(&input_state)
            ),
            Some(-5)
        );
        assert_eq!(
            active_input_cursor_delta(
                &snapshot,
                TerminalPoint { row: 1, col: 2 },
                Some(&input_state)
            ),
            Some(-2)
        );
    }

    #[test]
    fn free_type_cursor_move_combines_boundary_keys_with_arrow_fallbacks() {
        let input_state = TerminalAutosuggestInputState {
            value: "abcdef".to_string(),
            cursor_index: 6,
            is_cursor_at_end: true,
        };

        let start_move = command_cursor_move_to_index(&input_state, 0).unwrap();
        assert_eq!(
            free_type_cursor_move_bytes(start_move, TermMode::default()).as_deref(),
            Some(b"\x1b[H\x1b[D\x1b[D\x1b[D\x1b[D\x1b[D\x1b[D".as_slice())
        );
        assert_eq!(
            free_type_cursor_move_bytes(start_move, TermMode::APP_CURSOR).as_deref(),
            Some(b"\x1bOH\x1bOD\x1bOD\x1bOD\x1bOD\x1bOD\x1bOD".as_slice())
        );

        let input_state = TerminalAutosuggestInputState {
            value: "abcdef".to_string(),
            cursor_index: 0,
            is_cursor_at_end: false,
        };
        let end_move = command_cursor_move_to_index(&input_state, input_state.value.len()).unwrap();
        assert_eq!(
            free_type_cursor_move_bytes(end_move, TermMode::default()).as_deref(),
            Some(b"\x1b[F\x1b[C\x1b[C\x1b[C\x1b[C\x1b[C\x1b[C".as_slice())
        );
        assert_eq!(
            free_type_cursor_move_bytes(end_move, TermMode::APP_CURSOR).as_deref(),
            Some(b"\x1bOF\x1bOC\x1bOC\x1bOC\x1bOC\x1bOC\x1bOC".as_slice())
        );
    }

    #[test]
    fn free_type_command_edit_bytes_insert_and_replace_command() {
        let snapshot = test_snapshot_with_cursor(vec![test_row("$ abc", true)], 0, 5, 20);
        let input_state = TerminalAutosuggestInputState {
            value: "abc".to_string(),
            cursor_index: 3,
            is_cursor_at_end: true,
        };

        assert_eq!(
            free_type_command_edit_bytes(
                &snapshot,
                TerminalPoint { row: 0, col: 3 },
                &input_state,
                "XYZ",
                false,
                TermMode::default(),
            )
            .as_deref(),
            Some(b"\x1b[D\x1b[DXYZ".as_slice())
        );
        assert_eq!(
            free_type_command_edit_bytes(
                &snapshot,
                TerminalPoint { row: 0, col: 3 },
                &input_state,
                "XYZ",
                true,
                TermMode::default(),
            )
            .as_deref(),
            Some(b"\x08\x08\x08XYZ".as_slice())
        );
    }

    #[test]
    fn free_type_selection_delete_bytes_targets_command_selection() {
        let snapshot = test_snapshot_with_cursor(vec![test_row("$ abc", true)], 0, 5, 20);
        let selection = TerminalSelection {
            anchor: TerminalGridPoint { line: 0, col: 3 },
            head: TerminalGridPoint { line: 0, col: 3 },
            mode: TerminalSelectionMode::Semantic,
        };
        let input_state = TerminalAutosuggestInputState {
            value: "abc".to_string(),
            cursor_index: 3,
            is_cursor_at_end: true,
        };

        assert_eq!(
            free_type_selection_delete_bytes(
                &snapshot,
                selection,
                &input_state,
                TermMode::default()
            )
            .as_deref(),
            Some(b"\x1b[D\x08".as_slice())
        );
    }

    #[test]
    fn free_type_cut_payload_copies_only_the_editable_command_slice() {
        let snapshot = test_snapshot_with_cursor(vec![test_row("$ 你好abc", true)], 0, 9, 20);
        let selection = TerminalSelection {
            anchor: TerminalGridPoint { line: 0, col: 2 },
            head: TerminalGridPoint { line: 0, col: 5 },
            mode: TerminalSelectionMode::Simple,
        };
        let input_state = TerminalAutosuggestInputState {
            value: "你好abc".to_string(),
            cursor_index: "你好abc".len(),
            is_cursor_at_end: true,
        };

        let (text, bytes) = free_type_selection_cut_payload(
            &snapshot,
            selection,
            &input_state,
            TermMode::default(),
        )
        .expect("command selection should be cuttable");

        assert_eq!(text, "你好");
        assert_eq!(bytes, b"\x1b[D\x1b[D\x1b[D\x08\x08");
    }

    #[test]
    fn free_type_move_selection_handles_targets_before_after_and_inside_source() {
        let snapshot = test_snapshot_with_cursor(vec![test_row("$ abcdef", true)], 0, 8, 20);
        let selection = TerminalSelection {
            anchor: TerminalGridPoint { line: 0, col: 4 },
            head: TerminalGridPoint { line: 0, col: 5 },
            mode: TerminalSelectionMode::Simple,
        };
        let input_state = TerminalAutosuggestInputState {
            value: "abcdef".to_string(),
            cursor_index: "abcdef".len(),
            is_cursor_at_end: true,
        };

        assert_eq!(
            free_type_selection_move_bytes(
                &snapshot,
                selection,
                TerminalPoint { row: 0, col: 2 },
                &input_state,
                TermMode::default(),
            )
            .as_deref(),
            Some(b"\x1b[D\x1b[D\x08\x08\x1b[H\x1b[D\x1b[Dcd".as_slice())
        );
        assert_eq!(
            free_type_selection_move_bytes(
                &snapshot,
                selection,
                TerminalPoint { row: 0, col: 8 },
                &input_state,
                TermMode::default(),
            )
            .as_deref(),
            Some(b"cd\x1b[D\x1b[D\x1b[D\x1b[D\x08\x08".as_slice())
        );
        assert_eq!(
            free_type_selection_move_bytes(
                &snapshot,
                selection,
                TerminalPoint { row: 0, col: 5 },
                &input_state,
                TermMode::default(),
            ),
            Some(Vec::new())
        );
    }

    #[test]
    fn free_type_move_selection_keeps_emoji_graphemes_together() {
        let command = "a👩‍💻你b";
        let command_width = terminal_text_display_width(command);
        let snapshot = test_snapshot_with_cursor(
            vec![test_row(&format!("$ {command}"), true)],
            0,
            command_width + 2,
            20,
        );
        let selection = TerminalSelection {
            anchor: TerminalGridPoint { line: 0, col: 3 },
            head: TerminalGridPoint { line: 0, col: 4 },
            mode: TerminalSelectionMode::Simple,
        };
        let input_state = TerminalAutosuggestInputState {
            value: command.to_string(),
            cursor_index: command.len(),
            is_cursor_at_end: true,
        };

        let bytes = free_type_selection_move_bytes(
            &snapshot,
            selection,
            TerminalPoint {
                row: 0,
                col: command_width + 2,
            },
            &input_state,
            TermMode::default(),
        )
        .expect("emoji selection should move");

        assert_eq!(
            bytes,
            ["👩‍💻".as_bytes(), b"\x1b[D\x1b[D\x1b[D\x08".as_slice()].concat()
        );
    }

    #[test]
    fn free_type_selection_delete_bytes_clamps_line_selection_to_command() {
        let snapshot = test_snapshot_with_cursor(vec![test_row("$ abc", true)], 0, 5, 20);
        let selection = TerminalSelection {
            anchor: TerminalGridPoint { line: 0, col: 0 },
            head: TerminalGridPoint { line: 0, col: 5 },
            mode: TerminalSelectionMode::Lines,
        };
        let input_state = TerminalAutosuggestInputState {
            value: "abc".to_string(),
            cursor_index: 3,
            is_cursor_at_end: true,
        };

        assert_eq!(
            free_type_selection_delete_bytes(
                &snapshot,
                selection,
                &input_state,
                TermMode::default()
            )
            .as_deref(),
            Some(b"\x08\x08\x08".as_slice())
        );
    }

    #[test]
    fn privilege_prompt_snapshot_uses_only_the_current_cursor_input_block() {
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
