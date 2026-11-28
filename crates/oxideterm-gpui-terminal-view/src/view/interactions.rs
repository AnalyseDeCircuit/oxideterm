use gpui::{
    ClipboardItem, Context, KeyDownEvent, KeyUpEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, ScrollEvent, TouchPhase, Window, point, px,
};
use oxideterm_terminal_emulator::TermMode;
use oxideterm_terminal_unicode::visual_line_for_row_if_bidi;
use zeroize::Zeroizing;

use super::{TerminalView, TerminalViewEvent};
use crate::{
    input::{
        KittyKeyEventType, TerminalKeybindings, TerminalScrollAction, TerminalShortcut, alt_scroll,
        key_escape_sequence, mouse_button_report, mouse_mode, mouse_moved_report,
        mouse_scroll_report, oxideterm_terminal_scroll_action,
    },
    links::{
        TerminalLinkKind, TerminalLinkRange, is_link_interactive_cell,
        persistent_link_ranges_for_rows,
    },
    metrics::TERMINAL_CONTENT_PADDING,
    selection::{
        TerminalPoint, TerminalSelection, TerminalSelectionMode, grid_point_for_viewport_point,
        line_selection_at_point, word_selection_at_point,
    },
};

#[derive(Default)]
pub(super) struct PointerState {
    pub(super) hovered: Option<TerminalLinkRange>,
    pub(super) selecting: bool,
    reported_button: Option<MouseButton>,
    last_report: Option<TerminalPoint>,
    scroll_remainder: Pixels,
}

impl TerminalView {
    pub(super) fn key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = &event.keystroke;
        let m = key.modifiers;
        // Let the platform finish a composition (including Enter/Escape) before
        // interpreting keys as terminal input. AltGr can arrive as Control+Alt.
        if !self.composition.text.is_empty()
            || event.prefer_character_input
            || (m.control && m.alt && key.key_char.is_some())
        {
            return;
        }
        let command = m.platform && !m.control && !m.shift && !m.alt;
        let control = m.control && !m.platform && !m.alt;
        let bindings = cx.try_global::<TerminalKeybindings>();
        let configured = bindings.and_then(|b| b.resolve(key));
        let legacy = bindings.is_none();
        let copy = configured == Some(TerminalShortcut::Copy)
            || (legacy
                && ((key.key == "c" && (command || (control && m.shift)))
                    || (key.key == "insert" && control && !m.shift)));
        let browser_control_paste = cfg!(target_family = "wasm") && !cx.platform_info().is_mac();
        let paste_chord = (key.key == "v"
            && (command || (control && (m.shift || browser_control_paste))))
            || (key.key == "insert" && m.shift && !m.control && !m.platform && !m.alt);
        let browser_paste = browser_control_paste && control && !m.shift && key.key == "v";
        let paste = configured == Some(TerminalShortcut::Paste)
            || (legacy && paste_chord)
            || (configured.is_none() && browser_paste);
        let smart_copy = self.options.smart_copy
            && key.key == "c"
            && control
            && !m.shift
            && !self
                .mode()
                .intersects(TermMode::ALT_SCREEN | TermMode::MOUSE_MODE);
        if (copy || smart_copy)
            && let Some(text) = self.selected_text()
        {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        } else if copy {
            // An explicit Copy shortcut never becomes a control byte.
            window.prevent_default();
            cx.stop_propagation();
            return;
        } else if paste {
            // The browser delivers clipboardData through InputHandler::paste.
            // Cancelling this keydown would also cancel the DOM paste event.
            if cfg!(target_family = "wasm") && paste_chord {
                return;
            }
            self.read_clipboard_for_paste(cx);
        } else if let Some(action) = configured
            .and_then(TerminalShortcut::scroll_action)
            .or_else(|| {
                legacy
                    .then(|| oxideterm_terminal_scroll_action(key))
                    .flatten()
            })
        {
            let delta = match action {
                TerminalScrollAction::LineUp => 1,
                TerminalScrollAction::LineDown => -1,
                TerminalScrollAction::PageUp => self.size.rows as i32,
                TerminalScrollAction::PageDown => -(self.size.rows as i32),
                TerminalScrollAction::Top => self.snapshot.scrollback_lines as i32,
                TerminalScrollAction::Bottom => -(self.snapshot.display_offset as i32),
            };
            self.scroll_grid(delta, cx);
        } else if matches!(
            configured,
            Some(TerminalShortcut::WordBackward | TerminalShortcut::WordForward)
        ) && !self
            .mode()
            .intersects(TermMode::ALT_SCREEN | TermMode::KITTY_KEYBOARD_PROTOCOL)
        {
            let byte = if configured == Some(TerminalShortcut::WordBackward) {
                b'b'
            } else {
                b'f'
            };
            self.send_user_bytes(vec![0x1b, byte], cx);
        } else {
            let mode = self.mode();
            let kind = if event.is_held {
                KittyKeyEventType::Repeat
            } else {
                KittyKeyEventType::Press
            };
            let Some(sequence) = key_escape_sequence(
                key,
                &mode,
                self.alt_sends_escape,
                self.backspace,
                self.delete,
                kind,
            ) else {
                // Printable text is delivered exactly once, by the platform input handler.
                return;
            };
            if mode.intersects(TermMode::WIN32_INPUT | TermMode::REPORT_EVENT_TYPES) {
                self.reported_keys.insert(key.key.clone());
            }
            self.send_user_bytes(sequence.as_bytes().to_vec(), cx);
        }
        window.prevent_default();
        cx.stop_propagation();
    }

    fn read_clipboard_for_paste(&mut self, cx: &mut Context<Self>) {
        let clipboard = cx.read_from_clipboard_async();
        cx.spawn(async move |view, cx| {
            let item = clipboard.await;
            let _ = view.update(cx, |view, cx| {
                if !view.focused {
                    return;
                }
                match item {
                    Ok(Some(item)) => {
                        if let Some(text) = item.text() {
                            view.request_paste(Zeroizing::new(text), cx);
                        }
                    }
                    Ok(None) => {}
                    Err(_) => cx.emit(TerminalViewEvent::ClipboardReadFailed),
                }
            });
        })
        .detach();
    }

    pub(super) fn key_up(
        &mut self,
        event: &KeyUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.reported_keys.remove(&event.keystroke.key)
            && let Some(bytes) = key_escape_sequence(
                &event.keystroke,
                &self.mode(),
                self.alt_sends_escape,
                self.backspace,
                self.delete,
                KittyKeyEventType::Release,
            )
        {
            cx.emit(TerminalViewEvent::SendBytes(bytes.as_bytes().to_vec()));
            window.prevent_default();
            cx.stop_propagation();
        }
    }

    fn point_at(&self, position: Point<Pixels>) -> Option<TerminalPoint> {
        let (bounds, _) = self.viewport?;
        let local = position
            - bounds.origin
            - point(px(TERMINAL_CONTENT_PADDING), px(TERMINAL_CONTENT_PADDING));
        let row = ((local.y / self.metrics.line_height) as usize).min(self.size.rows - 1);
        let visual = ((local.x / self.metrics.cell_width) as usize).min(self.size.cols - 1);
        let col = self
            .snapshot
            .lines
            .get(row)
            .filter(|_| self.options.bidi)
            .and_then(visual_line_for_row_if_bidi)
            .map_or(visual, |line| line.logical_col_for_visual_col(visual));
        Some(TerminalPoint { row, col })
    }

    fn link_at(&self, point: TerminalPoint) -> Option<TerminalLinkRange> {
        let cell = self.snapshot.lines.get(point.row)?.cells.get(point.col)?;
        if cell.hyperlink().is_none() && !is_link_interactive_cell(cell) {
            return None;
        }
        persistent_link_ranges_for_rows(&self.snapshot, point.row..point.row + 1)
            .into_iter()
            .find(|link| {
                link.kind == TerminalLinkKind::Url
                    && (link.start_col..link.end_col).contains(&point.col)
            })
    }

    pub(super) fn mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus, cx);
        let Some(point) = self.point_at(event.position) else {
            return;
        };
        let mode = self.mode();
        let m = event.modifiers;
        let activate_link = if cx.platform_info().is_mac() {
            m.platform
        } else {
            m.control
        };
        let reporting = mouse_mode(mode, m.shift);
        let paste =
            (event.button == MouseButton::Middle && self.options.middle_paste && !reporting)
                || (event.button == MouseButton::Right
                    && self.options.right_paste
                    && !(self.options.respect_mouse_tracking && reporting));
        if paste {
            self.read_clipboard_for_paste(cx);
            window.prevent_default();
            cx.stop_propagation();
        } else if event.button == MouseButton::Left
            && (!self.options.link_modifier || activate_link)
            && let Some(link) = self.link_at(point)
        {
            cx.emit(TerminalViewEvent::OpenUrl(link.target.to_string()));
        } else if mouse_mode(mode, m.shift) {
            if let Some(bytes) = mouse_button_report(point, event.button, m, true, mode) {
                self.pointer.reported_button = Some(event.button);
                self.pointer.last_report = Some(point);
                cx.emit(TerminalViewEvent::SendBytes(bytes));
            }
        } else if event.button == MouseButton::Left
            && (!self.options.selection_requires_shift || m.shift)
        {
            window.request_virtual_keyboard();
            let Some(grid) = grid_point_for_viewport_point(&self.snapshot, point) else {
                return;
            };
            self.pointer.selecting = event.click_count <= 1;
            let selection = match event.click_count {
                0 | 1 if m.shift && self.selection.is_some() => self
                    .selection
                    .map(|s| TerminalSelection { head: grid, ..s }),
                0 | 1 => Some(TerminalSelection {
                    anchor: grid,
                    head: grid,
                    mode: if m.alt {
                        TerminalSelectionMode::Block
                    } else {
                        TerminalSelectionMode::Simple
                    },
                }),
                2 => word_selection_at_point(&self.snapshot, point),
                _ => line_selection_at_point(&self.snapshot, point),
            };
            self.set_selection(selection);
            if event.click_count > 1 {
                self.copy_finished_selection(cx);
            }
            cx.notify();
        }
    }

    fn copy_finished_selection(&self, cx: &mut Context<Self>) {
        if self.options.copy_on_select
            && let Some(text) = self.selected_text()
        {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn drag_to(&mut self, point: TerminalPoint, cx: &mut Context<Self>) {
        if let Some(s) = self.selection
            && let Some(head) = grid_point_for_viewport_point(&self.snapshot, point)
            && s.head != head
        {
            self.set_selection(Some(TerminalSelection { head, ..s }));
            cx.notify();
        }
    }

    pub(super) fn mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(point) = self.point_at(event.position) else {
            return;
        };
        let mode = self.mode();
        if self.pointer.selecting && event.pressed_button == Some(MouseButton::Left) {
            self.drag_to(point, cx);
        } else if mouse_mode(mode, event.modifiers.shift) {
            if self.pointer.last_report != Some(point)
                && let Some(bytes) =
                    mouse_moved_report(point, event.pressed_button, event.modifiers, mode)
            {
                self.pointer.last_report = Some(point);
                cx.emit(TerminalViewEvent::SendBytes(bytes));
            }
        }
        let hovered = if self.pointer.selecting {
            None
        } else {
            self.link_at(point)
        };
        if hovered != self.pointer.hovered {
            self.pointer.hovered = hovered;
            cx.notify();
        }
    }

    pub(super) fn mouse_up(
        &mut self,
        event: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(point) = self.point_at(event.position) else {
            return;
        };
        if self.pointer.reported_button == Some(event.button) {
            self.pointer.reported_button = None;
            self.pointer.last_report = None;
            if let Some(bytes) =
                mouse_button_report(point, event.button, event.modifiers, false, self.mode())
            {
                cx.emit(TerminalViewEvent::SendBytes(bytes));
            }
        } else if event.button == MouseButton::Left && self.pointer.selecting {
            self.drag_to(point, cx);
            self.pointer.selecting = false;
            self.copy_finished_selection(cx);
        }
    }

    pub(super) fn scroll(&mut self, event: &ScrollEvent, _: &mut Window, cx: &mut Context<Self>) {
        match event.touch_phase {
            TouchPhase::Ended => return,
            TouchPhase::Cancelled => {
                self.pointer.scroll_remainder = px(0.0);
                return;
            }
            TouchPhase::Started if !event.delta.precise() => {
                self.pointer.scroll_remainder = px(0.0);
                return;
            }
            TouchPhase::Started | TouchPhase::Moved => {}
        }
        let height = self.metrics.line_height;
        self.pointer.scroll_remainder += event.delta.pixel_delta(height).y;
        let rows = (self.pointer.scroll_remainder / height) as i32;
        if rows == 0 {
            return;
        }
        self.pointer.scroll_remainder -= height * rows as f32;
        let mode = self.mode();
        if mouse_mode(mode, event.modifiers.shift) {
            if let Some(point) = self.point_at(event.position)
                && let Some(bytes) = mouse_scroll_report(point, event, mode)
            {
                cx.emit(TerminalViewEvent::SendBytes(
                    bytes.repeat(rows.unsigned_abs() as usize),
                ));
            }
        } else if mode.contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL)
            && !event.modifiers.shift
        {
            cx.emit(TerminalViewEvent::SendBytes(alt_scroll(rows)));
        } else {
            self.scroll_grid(rows, cx);
        }
        cx.stop_propagation();
    }
}
