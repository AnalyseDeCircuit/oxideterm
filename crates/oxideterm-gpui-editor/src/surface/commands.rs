// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::{ClipboardItem, Context, Modifiers, Window};
use oxideterm_editor_core::{BufferOffset, LineCol};

use super::{EditorSaveStatus, TextEditorView, coords::floor_char_boundary, input};

/// Host-resolved shortcuts share the app's persisted keybindings without
/// coupling this reusable editor to the application settings crate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditorShortcut {
    Save,
    Copy,
    Cut,
    Paste,
    SelectAll,
    Undo,
    Redo,
    AddNextMatch,
    Find,
}

impl EditorShortcut {
    pub const BINDINGS: &'static [(&'static str, Self)] = &[
        ("editor.save", Self::Save),
        ("editor.copy", Self::Copy),
        ("editor.cut", Self::Cut),
        ("editor.paste", Self::Paste),
        ("editor.selectAll", Self::SelectAll),
        ("editor.undo", Self::Undo),
        ("editor.redo", Self::Redo),
        ("editor.addNextMatch", Self::AddNextMatch),
        ("editor.find", Self::Find),
    ];
}

pub struct EditorKeybindings {
    pub bindings: Vec<(gpui::KeyBinding, EditorShortcut)>,
    pub normalize: fn(&gpui::Keystroke) -> Option<gpui::Keystroke>,
}
impl gpui::Global for EditorKeybindings {}

impl EditorKeybindings {
    pub fn resolve(&self, key: &gpui::Keystroke) -> Option<EditorShortcut> {
        let key = (self.normalize)(key)?;
        self.bindings
            .iter()
            .find(|(binding, _)| {
                binding
                    .keystrokes()
                    .first()
                    .is_some_and(|binding| key.should_match(binding))
            })
            .map(|(_, action)| *action)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EditorCommand {
    Save,
    Undo,
    Redo,
    SelectAll,
    InsertText(String),
    DeleteBackward,
    DeleteForward,
    Find(String),
    FindNext,
    FindPrevious,
    ReplaceCurrent(String),
    ReplaceAll(String),
    AddNextFindMatchCursor,
    ClearSecondaryCursors,
    ToggleSoftWrap,
}

impl TextEditorView {
    pub fn execute_command(
        &mut self,
        command: EditorCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match command {
            EditorCommand::Save => self.save(window, cx),
            EditorCommand::Undo => self.undo(cx),
            EditorCommand::Redo => self.redo(cx),
            EditorCommand::SelectAll => self.select_all(cx),
            EditorCommand::InsertText(text) => self.insert_text(text, cx),
            EditorCommand::DeleteBackward => self.delete_backward(cx),
            EditorCommand::DeleteForward => self.delete_forward(cx),
            EditorCommand::Find(query) => self.set_find_query(query, cx),
            EditorCommand::FindNext => self.select_next_find_match(cx),
            EditorCommand::FindPrevious => self.select_previous_find_match(cx),
            EditorCommand::ReplaceCurrent(replacement) => {
                self.replace_current_find_match(replacement, cx)
            }
            EditorCommand::ReplaceAll(replacement) => {
                self.replace_all_find_matches(replacement, cx)
            }
            EditorCommand::AddNextFindMatchCursor => self.add_next_find_match_as_cursor(cx),
            EditorCommand::ClearSecondaryCursors => self.clear_secondary_cursors(cx),
            EditorCommand::ToggleSoftWrap => {
                let mut settings = self.settings.clone();
                settings.soft_wrap = !settings.soft_wrap;
                self.set_settings(settings, cx);
            }
        }
    }

    pub fn paste_from_clipboard(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        self.insert_text(text, cx);
    }

    pub fn copy_selection_to_clipboard(&self, cx: &mut Context<Self>) -> bool {
        let Some(text) = self.selected_text() else {
            return false;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        true
    }

    pub fn cut_selection_to_clipboard(&mut self, cx: &mut Context<Self>) -> bool {
        if self.read_only {
            return self.copy_selection_to_clipboard(cx);
        }
        if !self.copy_selection_to_clipboard(cx) {
            return false;
        }
        self.replace_selection_ranges(self.cursor.selection().range(), "", cx);
        true
    }

    pub(super) fn handle_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        if key == "escape" && self.close_search(window, cx) {
            cx.stop_propagation();
            return;
        }
        let commits_platform_text = input::keystroke_commits_platform_text(&event.keystroke);
        if !commits_platform_text {
            // Navigation and command keys do not pass through the platform
            // text handler, so they restart caret blinking here.
            self.activate_caret_blink(cx);
        }

        if let Some(bindings) = cx.try_global::<EditorKeybindings>() {
            if let Some(shortcut) = bindings.resolve(&event.keystroke) {
                match shortcut {
                    EditorShortcut::Save => self.save(window, cx),
                    EditorShortcut::Copy => {
                        self.copy_selection_to_clipboard(cx);
                    }
                    EditorShortcut::Cut => {
                        self.cut_selection_to_clipboard(cx);
                    }
                    EditorShortcut::Paste => self.paste_from_clipboard(cx),
                    EditorShortcut::SelectAll => self.select_all(cx),
                    EditorShortcut::Undo => self.undo(cx),
                    EditorShortcut::Redo => self.redo(cx),
                    EditorShortcut::AddNextMatch => self.add_next_find_match_as_cursor(cx),
                    EditorShortcut::Find => {
                        if !self.open_search(cx) {
                            return;
                        }
                    }
                }
                cx.stop_propagation();
                return;
            }
        } else {
            if matches_tauri_plain_mod_key(key, modifiers, "f", cx) && self.open_search(cx) {
                cx.stop_propagation();
                return;
            }
            if matches_tauri_plain_mod_key(key, modifiers, "s", cx) {
                self.save(window, cx);
                return;
            }
            if matches_tauri_plain_mod_key(key, modifiers, "c", cx) {
                self.copy_selection_to_clipboard(cx);
                return;
            }
            if matches_tauri_plain_mod_key(key, modifiers, "x", cx) {
                self.cut_selection_to_clipboard(cx);
                return;
            }
            if matches_tauri_plain_mod_key(key, modifiers, "v", cx) {
                self.paste_from_clipboard(cx);
                return;
            }
            if matches_tauri_plain_mod_key(key, modifiers, "a", cx) {
                self.select_all(cx);
                return;
            }
            if matches_tauri_mod_key(key, modifiers, "z", cx) {
                if modifiers.shift {
                    self.redo(cx);
                } else {
                    self.undo(cx);
                }
                return;
            }
            if matches_tauri_plain_mod_key(key, modifiers, "y", cx) {
                self.redo(cx);
                return;
            }
            if matches_tauri_plain_mod_key(key, modifiers, "d", cx) {
                self.add_next_find_match_as_cursor(cx);
                return;
            }
        }
        if commits_platform_text {
            return;
        }

        let mac = cx.platform_info().is_mac();
        let word_modifier = if mac {
            modifiers.alt && !modifiers.platform
        } else {
            modifiers.control && !modifiers.alt
        };
        let forward = matches!(key, "right" | "arrowright" | "delete");
        if word_modifier
            && matches!(
                key,
                "left" | "arrowleft" | "right" | "arrowright" | "backspace" | "delete"
            )
        {
            if matches!(key, "backspace" | "delete") {
                if !self.read_only {
                    let ranges = self
                        .selections_primary_first()
                        .map(|selection| {
                            if selection.is_caret() {
                                super::TextRange::new(
                                    selection.head,
                                    self.buffer.word_boundary(selection.head, forward),
                                )
                            } else {
                                selection.range()
                            }
                        })
                        .collect();
                    self.replace_ranges_with_caret(ranges, "", cx);
                }
            } else {
                self.move_horizontal(forward, true, modifiers.shift);
                cx.notify();
            }
            cx.stop_propagation();
            return;
        }
        if mac && modifiers.platform && matches!(key, "left" | "right" | "arrowleft" | "arrowright")
        {
            self.move_to_line_boundary(forward, modifiers.shift, cx);
            cx.stop_propagation();
            return;
        }
        match key {
            "escape" if !self.secondary_cursors.is_empty() => {
                self.clear_secondary_cursors(cx);
                cx.stop_propagation();
            }
            "left" | "arrowleft" => {
                self.move_horizontal(false, false, modifiers.shift);
                cx.notify();
            }
            "right" | "arrowright" => {
                self.move_horizontal(true, false, modifiers.shift);
                cx.notify();
            }
            "up" | "arrowup" => self.move_vertically(-1, modifiers.shift, cx),
            "down" | "arrowdown" => self.move_vertically(1, modifiers.shift, cx),
            "pageup" => {
                self.move_vertically(-(self.visible_page_rows() as isize), modifiers.shift, cx)
            }
            "pagedown" => {
                self.move_vertically(self.visible_page_rows() as isize, modifiers.shift, cx)
            }
            "home" => self.move_to_line_boundary(false, modifiers.shift, cx),
            "end" => self.move_to_line_boundary(true, modifiers.shift, cx),
            "backspace" => self.delete_backward(cx),
            "delete" => self.delete_forward(cx),
            "enter" => self.insert_text(self.indented_newline(), cx),
            "tab" => self.insert_text(self.settings.indentation_unit(), cx),
            _ => {}
        }
    }

    fn selected_text(&self) -> Option<String> {
        let parts = self
            .active_selections()
            .into_iter()
            .filter_map(|selection| self.buffer.slice(selection.range()).ok())
            .collect::<Vec<_>>();
        (!parts.is_empty()).then(|| parts.join("\n"))
    }

    fn move_horizontal(&mut self, forward: bool, by_word: bool, extend: bool) {
        let move_cursor = |cursor: &mut super::Cursor| {
            if by_word {
                cursor.move_to(
                    self.buffer.word_boundary(cursor.selection().head, forward),
                    extend,
                );
            } else if forward {
                cursor.move_right(&self.buffer, extend);
            } else {
                cursor.move_left(&self.buffer, extend);
            }
        };
        for cursor in std::iter::once(&mut self.cursor).chain(&mut self.secondary_cursors) {
            move_cursor(cursor);
        }
    }

    fn move_to_line_boundary(&mut self, to_end: bool, extend: bool, cx: &mut Context<Self>) {
        let boundary = |head| {
            let line = self.buffer.offset_to_line_col(head).ok()?.line;
            if to_end {
                self.buffer.line_end_offset(line)
            } else {
                self.buffer.line_start_offset(line)
            }
        };
        for cursor in std::iter::once(&mut self.cursor).chain(&mut self.secondary_cursors) {
            if let Some(offset) = boundary(cursor.selection().head) {
                cursor.move_to(offset, extend);
            }
        }
        self.marked_text = None;
        cx.notify();
    }

    fn move_vertically(&mut self, row_delta: isize, extend: bool, cx: &mut Context<Self>) {
        let rows = self.display_rows();
        let move_cursor = |cursor: &mut super::Cursor| {
            let position = self
                .buffer
                .offset_to_line_col(cursor.selection().head)
                .ok()?;
            let text = self.buffer.line_text(position.line)?;
            let visual = super::coords::visual_column_for_byte_column(&text, position.column);
            let (index, _, column) =
                super::wrap::display_row_for_visual_column(&rows, position.line, visual)?;
            let index = index
                .saturating_add_signed(row_delta)
                .min(rows.len().saturating_sub(1));
            let row = rows.get(index)?;
            let column = cursor.preferred_column().unwrap_or(column);
            let text = self.buffer.line_text(row.line)?;
            let byte_column = super::coords::byte_column_for_visual_column(
                &text,
                (row.start_col + column).min(row.end_col),
            );
            let offset = self
                .buffer
                .line_col_to_offset(LineCol::new(row.line, byte_column))
                .ok()?;
            cursor.move_to_with_preferred_column(offset, extend, column);
            Some(index)
        };
        let Some(index) = move_cursor(&mut self.cursor) else {
            return;
        };
        for cursor in &mut self.secondary_cursors {
            move_cursor(cursor);
        }
        self.marked_text = None;
        self.reveal_display_row(index);
        cx.notify();
    }

    fn visible_page_rows(&self) -> usize {
        if self.metrics.line_height <= 0.0 {
            return 1;
        }
        (self.viewport.height_px / self.metrics.line_height)
            .floor()
            .max(1.0) as usize
    }

    pub(super) fn undo(&mut self, cx: &mut Context<Self>) {
        if self.buffer.undo().ok() == Some(true) {
            self.after_history_change(self.buffer.selections_after_undo().map(<[_]>::to_vec), cx);
        }
    }

    pub(super) fn redo(&mut self, cx: &mut Context<Self>) {
        if self.buffer.redo().ok() == Some(true) {
            self.after_history_change(self.buffer.selections_after_redo().map(<[_]>::to_vec), cx);
        }
    }

    fn after_history_change(
        &mut self,
        selections: Option<Vec<super::Selection>>,
        cx: &mut Context<Self>,
    ) {
        self.request_syntax(None, true, cx);
        self.clear_folds_after_buffer_change();
        self.refresh_find_matches(cx);
        self.secondary_cursors.clear();
        if let Some((primary, rest)) = selections.as_deref().and_then(|s| s.split_first()) {
            self.cursor.set_selection(*primary);
            self.secondary_cursors
                .extend(rest.iter().copied().map(super::Cursor::from));
        } else if self
            .buffer
            .offset_to_line_col(self.cursor.selection().head)
            .is_err()
            || self
                .buffer
                .offset_to_line_col(self.cursor.selection().anchor)
                .is_err()
        {
            let mut head = BufferOffset(self.cursor.selection().head.0.min(self.buffer.len()));
            while self.buffer.offset_to_line_col(head).is_err() {
                head.0 -= 1;
            }
            self.cursor.move_to(head, false);
        }
        self.save_status = if self.buffer.is_dirty() {
            EditorSaveStatus::Dirty
        } else {
            EditorSaveStatus::Clean
        };
        if self.render_cache_active {
            self.viewport
                .clamp(self.document_row_count(), self.metrics.line_height);
        }
        cx.notify();
    }

    fn indented_newline(&self) -> String {
        let selection = self.cursor.selection();
        let Ok(position) = self.buffer.offset_to_line_col(selection.range().start) else {
            return "\n".to_string();
        };
        let line = self.buffer.line_text(position.line).unwrap_or_default();
        let mut indent = line
            .chars()
            .take_while(|ch| matches!(ch, ' ' | '\t'))
            .collect::<String>();
        let before_cursor = &line[..floor_char_boundary(&line, position.column)];
        if before_cursor.trim_end().ends_with(['{', '[', '(']) {
            indent.push_str(&self.settings.indentation_unit());
        }
        format!("\n{indent}")
    }
}

fn matches_tauri_plain_mod_key(
    key: &str,
    modifiers: Modifiers,
    expected_key: &str,
    cx: &gpui::App,
) -> bool {
    matches_tauri_mod_key(key, modifiers, expected_key, cx) && !modifiers.shift
}

fn matches_tauri_mod_key(
    key: &str,
    modifiers: Modifiers,
    expected_key: &str,
    cx: &gpui::App,
) -> bool {
    // CodeMirror's `Mod-*` maps to Command on macOS and Control on
    // Windows/Linux; GPUI exposes that same intent as the secondary modifier.
    modifiers.secondary(cx) && !modifiers.alt && key.eq_ignore_ascii_case(expected_key)
}
