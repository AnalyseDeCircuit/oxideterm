// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::{Context, Window};

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

    pub(super) fn paste_from_clipboard(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        self.insert_text(text, cx);
    }

    pub(super) fn handle_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;

        if modifiers.platform && key.eq_ignore_ascii_case("s") {
            self.save(window, cx);
            return;
        }
        if modifiers.platform && key.eq_ignore_ascii_case("v") {
            self.paste_from_clipboard(cx);
            return;
        }
        if modifiers.platform && key.eq_ignore_ascii_case("a") {
            self.select_all(cx);
            return;
        }
        if modifiers.platform && key.eq_ignore_ascii_case("z") {
            if modifiers.shift {
                self.redo(cx);
            } else {
                self.undo(cx);
            }
            return;
        }
        if modifiers.platform && key.eq_ignore_ascii_case("y") {
            self.redo(cx);
            return;
        }
        if modifiers.platform && key.eq_ignore_ascii_case("f") {
            self.select_current_word_for_find(cx);
            return;
        }
        if modifiers.platform && key.eq_ignore_ascii_case("d") {
            self.add_next_find_match_as_cursor(cx);
            return;
        }
        if input::keystroke_commits_platform_text(&event.keystroke) {
            return;
        }

        match key {
            "left" => {
                self.cursor.move_left(&self.buffer, modifiers.shift);
                cx.notify();
            }
            "right" => {
                self.cursor.move_right(&self.buffer, modifiers.shift);
                cx.notify();
            }
            "backspace" => self.delete_backward(cx),
            "delete" => self.delete_forward(cx),
            "enter" => self.insert_text(self.indented_newline(), cx),
            "tab" => self.insert_text(self.settings.indentation_unit(), cx),
            _ => {}
        }
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
