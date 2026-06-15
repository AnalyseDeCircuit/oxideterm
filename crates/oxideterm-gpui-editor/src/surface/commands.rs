// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::{ClipboardItem, Context, Modifiers, Window};
use oxideterm_editor_core::{BufferOffset, LineCol};

use super::{EditorSaveStatus, TextEditorView, coords::floor_char_boundary, input};

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
        let ranges = self
            .all_selections()
            .into_iter()
            .filter_map(|selection| (!selection.is_caret()).then_some(selection.range()))
            .collect::<Vec<_>>();
        if ranges.is_empty() || !self.copy_selection_to_clipboard(cx) {
            return false;
        }
        self.replace_ranges_with_caret(ranges, "", cx);
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

        if matches_tauri_plain_mod_key(key, modifiers, "s") {
            self.save(window, cx);
            return;
        }
        if matches_tauri_plain_mod_key(key, modifiers, "c") {
            self.copy_selection_to_clipboard(cx);
            return;
        }
        if matches_tauri_plain_mod_key(key, modifiers, "x") {
            self.cut_selection_to_clipboard(cx);
            return;
        }
        if matches_tauri_plain_mod_key(key, modifiers, "v") {
            self.paste_from_clipboard(cx);
            return;
        }
        if matches_tauri_plain_mod_key(key, modifiers, "a") {
            self.select_all(cx);
            return;
        }
        if matches_tauri_mod_key(key, modifiers, "z") {
            if modifiers.shift {
                self.redo(cx);
            } else {
                self.undo(cx);
            }
            return;
        }
        if matches_tauri_plain_mod_key(key, modifiers, "y") {
            self.redo(cx);
            return;
        }
        if matches_tauri_plain_mod_key(key, modifiers, "d") {
            self.add_next_find_match_as_cursor(cx);
            return;
        }
        if input::keystroke_commits_platform_text(&event.keystroke) {
            return;
        }

        match key {
            "left" | "arrowleft" => {
                self.cursor.move_left(&self.buffer, modifiers.shift);
                cx.notify();
            }
            "right" | "arrowright" => {
                self.cursor.move_right(&self.buffer, modifiers.shift);
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
            .all_selections()
            .into_iter()
            .filter_map(|selection| {
                (!selection.is_caret())
                    .then(|| self.buffer.slice(selection.range()).ok())
                    .flatten()
            })
            .collect::<Vec<_>>();
        (!parts.is_empty()).then(|| parts.join("\n"))
    }

    fn move_to_line_boundary(&mut self, to_end: bool, extend: bool, cx: &mut Context<Self>) {
        let Ok(position) = self.buffer.offset_to_line_col(self.cursor.selection().head) else {
            return;
        };
        let offset = if to_end {
            self.buffer.line_end_offset(position.line)
        } else {
            self.buffer.line_start_offset(position.line)
        };
        if let Some(offset) = offset {
            self.cursor.move_to(offset, extend);
            self.marked_text = None;
            cx.notify();
        }
    }

    fn move_vertically(&mut self, row_delta: isize, extend: bool, cx: &mut Context<Self>) {
        let Some((current_index, _current_row, current_screen_column)) =
            self.current_display_row_and_screen_column()
        else {
            return;
        };
        let rows = self.display_rows();
        let max_index = rows.len().saturating_sub(1);
        let target_index = current_index
            .saturating_add_signed(row_delta)
            .min(max_index);
        let Some(target_row) = rows.get(target_index).copied() else {
            return;
        };
        let preferred_column = self.cursor.preferred_column_or(current_screen_column);
        let target_visual_column = target_row.start_col.saturating_add(preferred_column);
        let Some(offset) =
            self.offset_for_line_visual_column(target_row.line, target_visual_column)
        else {
            return;
        };
        self.cursor
            .move_to_with_preferred_column(offset, extend, preferred_column);
        self.secondary_selections.clear();
        self.marked_text = None;
        self.viewport.reveal_line(
            target_index,
            self.document_row_count(),
            self.metrics.line_height,
        );
        cx.notify();
    }

    fn current_display_row_and_screen_column(&self) -> Option<(usize, super::DisplayRow, usize)> {
        let position = self
            .buffer
            .offset_to_line_col(self.cursor.selection().head)
            .ok()?;
        let line_text = self.buffer.line_text(position.line).unwrap_or_default();
        let visual_column =
            super::coords::visual_column_for_byte_column(&line_text, position.column);
        let rows = self.display_rows();
        let index = rows
            .iter()
            .enumerate()
            .rfind(|(_, row)| {
                row.line == position.line
                    && visual_column >= row.start_col
                    && visual_column <= row.end_col
            })
            .map(|(index, _)| index)
            .or_else(|| {
                rows.iter()
                    .enumerate()
                    .rfind(|(_, row)| row.line == position.line)
                    .map(|(index, _)| index)
            })?;
        let row = rows.get(index).copied()?;
        Some((index, row, visual_column.saturating_sub(row.start_col)))
    }

    fn offset_for_line_visual_column(
        &self,
        line: usize,
        visual_column: usize,
    ) -> Option<BufferOffset> {
        let line_text = self.buffer.line_text(line).unwrap_or_default();
        let byte_column = super::coords::byte_column_for_visual_column(&line_text, visual_column);
        self.buffer
            .line_col_to_offset(LineCol::new(line, byte_column))
            .ok()
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
            self.after_history_change(cx);
        }
    }

    pub(super) fn redo(&mut self, cx: &mut Context<Self>) {
        if self.buffer.redo().ok() == Some(true) {
            self.after_history_change(cx);
        }
    }

    fn after_history_change(&mut self, cx: &mut Context<Self>) {
        if let Some(syntax) = self.syntax.as_mut() {
            let _ = self.buffer.with_text(|text| syntax.reparse(text));
        }
        self.refresh_highlights();
        self.clear_folds_after_buffer_change();
        self.refresh_find_matches();
        self.secondary_selections.clear();
        self.save_status = if self.buffer.is_dirty() {
            EditorSaveStatus::Dirty
        } else {
            EditorSaveStatus::Clean
        };
        self.viewport
            .clamp(self.document_row_count(), self.metrics.line_height);
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

fn matches_tauri_plain_mod_key(key: &str, modifiers: Modifiers, expected_key: &str) -> bool {
    matches_tauri_mod_key(key, modifiers, expected_key) && !modifiers.shift
}

fn matches_tauri_mod_key(key: &str, modifiers: Modifiers, expected_key: &str) -> bool {
    // CodeMirror's `Mod-*` maps to Command on macOS and Control on
    // Windows/Linux; GPUI exposes that same intent as the secondary modifier.
    modifiers.secondary() && !modifiers.alt && key.eq_ignore_ascii_case(expected_key)
}

#[cfg(test)]
mod tests {
    use gpui::Modifiers;

    use super::{matches_tauri_mod_key, matches_tauri_plain_mod_key};

    #[test]
    fn plain_mod_shortcuts_match_codemirror_mod_key() {
        // The IDE source of truth is CodeMirror `Mod-*`, not GPUI's raw
        // platform key, which is Super/Windows on non-macOS platforms.
        assert!(matches_tauri_plain_mod_key(
            "s",
            Modifiers::secondary_key(),
            "s"
        ));
        assert!(matches_tauri_plain_mod_key(
            "V",
            Modifiers::secondary_key(),
            "v"
        ));
        assert!(matches_tauri_plain_mod_key(
            "c",
            Modifiers::secondary_key(),
            "c"
        ));
        assert!(matches_tauri_plain_mod_key(
            "x",
            Modifiers::secondary_key(),
            "x"
        ));
        assert!(matches_tauri_plain_mod_key(
            "a",
            Modifiers::secondary_key(),
            "a"
        ));
        assert!(matches_tauri_plain_mod_key(
            "d",
            Modifiers::secondary_key(),
            "d"
        ));
    }

    #[test]
    fn plain_mod_shortcuts_reject_modified_variants() {
        // Native save currently mirrors Tauri's plain Mod-s binding only.
        let mut alt_save = Modifiers::secondary_key();
        alt_save.alt = true;
        assert!(!matches_tauri_plain_mod_key("s", alt_save, "s"));

        let mut shifted_save = Modifiers::secondary_key();
        shifted_save.shift = true;
        assert!(!matches_tauri_plain_mod_key("s", shifted_save, "s"));
        assert!(!matches_tauri_plain_mod_key(
            "x",
            Modifiers::secondary_key(),
            "s"
        ));
    }

    #[test]
    fn shifted_mod_z_matches_redo_binding() {
        let mut redo = Modifiers::secondary_key();
        redo.shift = true;
        assert!(matches_tauri_mod_key("z", redo, "z"));
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn mod_shortcuts_do_not_treat_super_as_control_on_non_macos() {
        // GPUI's platform modifier is Super/Windows here, while CodeMirror
        // `Mod-*` expects Control.
        assert!(!matches_tauri_plain_mod_key(
            "s",
            Modifiers {
                platform: true,
                ..Modifiers::none()
            },
            "s"
        ));
    }
}
