// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::{Keystroke, Window};

use super::*;

#[derive(Clone, Copy)]
pub enum SftpKeyAction {
    SelectAll,
    EditPath,
    RevealTerminalDirectory,
    Open,
    Preview,
    Upload,
    Download,
    Delete,
    Rename,
    TogglePreviewSource,
    SaveEditor,
}

pub enum SftpKeyResult {
    Ignored,
    Handled,
    ResetIme,
    CloseDialog,
    AcceptDialog,
    CommitPath(SftpPane),
    AcceptCompletion(SftpPane, usize),
    OpenFile(SftpPane, SftpFileEntry),
    Transfer(SftpPane, SftpTransferDirection),
    RevealTerminalDirectory,
}

impl SftpWorkspaceEntity {
    pub fn handle_key(
        &mut self,
        keystroke: &Keystroke,
        action: Option<SftpKeyAction>,
        tree_mode: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> SftpKeyResult {
        use SftpKeyResult::*;

        let key = keystroke.key.as_str();
        if matches!(self.dialog, Some(SftpDialog::Editor { .. })) {
            if matches!(action, Some(SftpKeyAction::SaveEditor)) {
                self.save_editor(cx);
                return Handled;
            }
            return if key == "escape" && self.confirm_or_close_editor(cx) {
                CloseDialog
            } else if key == "escape" {
                Handled
            } else {
                Ignored
            };
        }
        if self.dialog.is_some() && self.focused_input.is_none() {
            if matches!(action, Some(SftpKeyAction::TogglePreviewSource))
                && matches!(self.dialog, Some(SftpDialog::Preview { .. }))
                && self.preview_is_markdown()
            {
                self.toggle_preview_source(cx);
                return Handled;
            }
            return match (key, self.dialog.clone()) {
                ("escape", Some(SftpDialog::EditorCloseConfirm { name })) => {
                    self.restore_editor(name, window, cx);
                    Handled
                }
                ("escape", _) | ("enter", Some(SftpDialog::EditorCloseConfirm { .. })) => {
                    CloseDialog
                }
                ("enter", _) => AcceptDialog,
                _ => Ignored,
            };
        }
        if let Some(input) = self.focused_input {
            let pane = match input {
                SftpInput::LocalPath => Some(SftpPane::Local),
                SftpInput::RemotePath => Some(SftpPane::Remote),
                _ => None,
            };
            if let Some(pane) = pane
                && let Some(result) = self.handle_completion_key(pane, keystroke, cx)
            {
                return result;
            }
            return match key {
                "escape" | "tab"
                    if key == "escape"
                        || (!keystroke.modifiers.platform && !keystroke.modifiers.control) =>
                {
                    if let Some(pane) = pane {
                        self.cancel_path_edit(pane, cx);
                        cx.notify();
                    } else {
                        self.clear_input_focus(cx);
                    }
                    ResetIme
                }
                "enter" => match (pane, input) {
                    (Some(pane), _) => CommitPath(pane),
                    (_, SftpInput::DialogValue) => AcceptDialog,
                    _ => Handled,
                },
                _ => Ignored,
            };
        }

        let pane = self.active_pane;
        let tree = pane == SftpPane::Remote && tree_mode;
        match action {
            Some(SftpKeyAction::RevealTerminalDirectory) => return RevealTerminalDirectory,
            Some(SftpKeyAction::SelectAll) => {
                // Tree selection refers to a single visible row, never to a hidden flat selection.
                if !tree {
                    self.select_all_files(pane);
                    cx.notify();
                }
                self.dismiss_context_menu(cx);
                return Handled;
            }
            Some(SftpKeyAction::EditPath) => {
                self.start_path_edit(pane, cx);
                self.dismiss_context_menu(cx);
                cx.notify();
                return Handled;
            }
            Some(SftpKeyAction::Open) => {
                // Enter opens directories; file quick look belongs to the Preview binding.
                if let Some(file) = self.keyboard_selected_file(pane, tree)
                    && file.file_type == SftpFileType::Directory
                {
                    return if tree {
                        self.toggle_tree_directory(file.path, cx);
                        Handled
                    } else {
                        OpenFile(pane, file)
                    };
                }
                return Ignored;
            }
            Some(SftpKeyAction::Preview) => {
                if pane == SftpPane::Remote
                    && let Some(file) = self.keyboard_selected_file(pane, tree)
                    && file.file_type != SftpFileType::Directory
                {
                    return OpenFile(pane, file);
                }
                return Ignored;
            }
            Some(SftpKeyAction::Upload) => {
                return if pane == SftpPane::Local && !self.local_selected.is_empty() {
                    Transfer(pane, SftpTransferDirection::Upload)
                } else {
                    Ignored
                };
            }
            Some(SftpKeyAction::Download) => {
                return if pane == SftpPane::Remote && !self.visible_selection(pane, tree).is_empty()
                {
                    Transfer(pane, SftpTransferDirection::Download)
                } else {
                    Ignored
                };
            }
            Some(SftpKeyAction::Delete) => {
                let files = self.visible_selection(pane, tree);
                if files.is_empty() {
                    return Ignored;
                }
                let paths = if tree {
                    self.selected_tree_file()
                        .map(|file| vec![file.path])
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
                self.set_dialog(SftpDialog::Delete { pane, files, paths });
                cx.notify();
                return Handled;
            }
            Some(SftpKeyAction::Rename) => {
                if let Some(file) = self.keyboard_selected_file(pane, tree) {
                    self.open_rename_dialog(pane, file.name, tree.then_some(file.path), cx);
                    return Handled;
                }
                return Ignored;
            }
            _ => {}
        }

        if keystroke.modifiers.platform || keystroke.modifiers.control || keystroke.modifiers.alt {
            return Ignored;
        }
        match key {
            "escape" => {
                self.dismiss_context_menu(cx);
                self.clear_input_focus(cx);
                Handled
            }
            "up" | "arrowup" | "down" | "arrowdown" => {
                let delta = if matches!(key, "up" | "arrowup") {
                    -1
                } else {
                    1
                };
                let changed = if tree {
                    self.move_tree_selection(delta)
                } else {
                    self.move_selection(pane, delta)
                };
                if changed {
                    cx.notify();
                }
                Handled
            }
            _ => Ignored,
        }
    }

    pub fn tree_mode(&self, enabled: bool) -> bool {
        enabled
            && self.current_surface_id == Some(SftpSurfaceId::Sidebar)
            && self.remote_filter.trim().is_empty()
    }

    pub fn visible_selection(&self, pane: SftpPane, tree_mode: bool) -> Vec<String> {
        if pane == SftpPane::Remote && tree_mode {
            self.selected_tree_file()
                .map(|file| vec![file.name])
                .unwrap_or_default()
        } else {
            self.selected_names(pane)
        }
    }

    fn keyboard_selected_file(&self, pane: SftpPane, tree_mode: bool) -> Option<SftpFileEntry> {
        if tree_mode {
            self.selected_tree_file()
        } else {
            self.single_selected_file(pane)
        }
    }

    fn handle_completion_key(
        &mut self,
        pane: SftpPane,
        keystroke: &Keystroke,
        cx: &mut Context<Self>,
    ) -> Option<SftpKeyResult> {
        if keystroke.modifiers.platform || keystroke.modifiers.control || keystroke.modifiers.alt {
            return None;
        }
        let completion = match pane {
            SftpPane::Local => &mut self.local_path_completion,
            SftpPane::Remote => &mut self.remote_path_completion,
        };
        if !completion.is_visible() {
            return None;
        }
        let key = keystroke.key.as_str();
        match key {
            "up" | "arrowup" | "down" | "arrowdown" => {
                let delta = if matches!(key, "up" | "arrowup") {
                    -1
                } else {
                    1
                };
                if !completion.move_selection(delta) {
                    return None;
                }
                cx.notify();
                Some(SftpKeyResult::Handled)
            }
            "enter" | "tab" => completion
                .acceptance_index(key)
                .map(|index| SftpKeyResult::AcceptCompletion(pane, index)),
            "escape" => {
                completion.dismiss();
                cx.notify();
                Some(SftpKeyResult::Handled)
            }
            _ => None,
        }
    }
}
