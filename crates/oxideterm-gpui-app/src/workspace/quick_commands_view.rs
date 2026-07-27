use std::{
    collections::{HashMap, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    sync::Arc,
};

use gpui::{
    AnyElement, App, Context, CursorStyle, KeyDownEvent, MouseButton, div, prelude::*, px, rgb,
    rgba,
};
use oxideterm_editor_core::utf16::replace_utf16;
use oxideterm_gpui_ui::{
    CommandPanelOptions, StatusPillOptions, StatusTone, SurfacePadding, command_panel,
    modal::rounded_shell_child_radius,
    select::SelectAnchorId,
    status_pill,
    text_input::{TextInputView, text_input, text_input_anchor_probe},
};
use oxideterm_quick_commands::{
    QuickCommandRisk, classify_command_risk, quick_command_category_draft_can_save,
    quick_command_draft_can_save,
};
use zeroize::Zeroizing;

use super::super::ime::WorkspaceImeTarget;
use super::super::{
    QUICK_COMMAND_LIST_ESTIMATED_HEIGHT, QUICK_COMMAND_LIST_OVERSCAN, SelectableTextRole,
    TauriVirtualListSpec, WorkspaceApp, settings_mono_font_family,
    sync_tauri_variable_list_state_by_signatures, tauri_virtual_list,
};
use super::{
    QuickCommand, QuickCommandCategory, QuickCommandCategoryDraft, QuickCommandDraft,
    QuickCommandIcon, QuickCommandInput, TerminalQuickCommandsState,
    default_quick_command_categories, quick_command_icon_source_id,
};
use crate::assets::LucideIcon;

fn quick_command_lucide_icon(icon: QuickCommandIcon) -> LucideIcon {
    match icon {
        QuickCommandIcon::Server => LucideIcon::Server,
        QuickCommandIcon::Folder => LucideIcon::Folder,
        QuickCommandIcon::Docker => LucideIcon::Server,
        QuickCommandIcon::Zap => LucideIcon::Zap,
        QuickCommandIcon::Terminal => LucideIcon::Monitor,
    }
}

const QUICK_COMMANDS_POPOVER_MAX_WIDTH: f32 = 860.0;
const QUICK_COMMANDS_POPOVER_HORIZONTAL_MARGIN: f32 = 12.0;
const QUICK_COMMANDS_LIST_MAX_HEIGHT: f32 = 360.0;
const QUICK_COMMANDS_CONTENT_MIN_HEIGHT: f32 = 300.0;
const QUICK_COMMANDS_BODY_HEADER_HEIGHT: f32 = 49.0;

fn quick_command_icon_label_key(icon: QuickCommandIcon) -> String {
    format!(
        "terminal.quick_commands.icon_{}",
        quick_command_icon_source_id(icon)
    )
}

#[cfg(test)]
fn close_terminal_quick_commands_popover_state(
    open: &mut bool,
    pinned: &mut bool,
    pending_command: &mut Option<String>,
    focused_input: &mut Option<QuickCommandInput>,
    highlighted_command: &mut Option<String>,
) {
    *open = false;
    *pinned = false;
    *pending_command = None;
    *focused_input = None;
    *highlighted_command = None;
}

#[cfg(test)]
fn insert_quick_command_into_command_bar_state(
    draft: &mut String,
    command: &str,
    keep_open: bool,
    command_bar_focused: &mut bool,
    open: &mut bool,
    pinned: &mut bool,
    pending_command: &mut Option<String>,
    focused_input: &mut Option<QuickCommandInput>,
    highlighted_command: &mut Option<String>,
) {
    *draft = command.to_string();
    *command_bar_focused = true;
    if keep_open {
        // Row click inserts into the command bar. In pin mode the palette is a
        // repeatable picker, so keep it visible while moving keyboard ownership
        // back to the command draft.
        *open = true;
        *pinned = true;
        *pending_command = None;
        *focused_input = None;
        *highlighted_command = None;
    } else {
        close_terminal_quick_commands_popover_state(
            open,
            pinned,
            pending_command,
            focused_input,
            highlighted_command,
        );
    }
}

#[cfg(test)]
fn finish_quick_command_execution_state(
    open: &mut bool,
    pinned: bool,
    pending_command: &mut Option<String>,
) {
    // Pending confirmation state is scoped to the command that just ran.
    *pending_command = None;
    if pinned {
        // Pin mode makes execution repeatable from the same palette. The
        // palette itself stays after each command.
        *open = true;
    } else {
        *open = false;
    }
}

fn quick_commands_popover_width_for_bar(command_bar_width: f32) -> f32 {
    let available_width = command_bar_width - QUICK_COMMANDS_POPOVER_HORIZONTAL_MARGIN * 2.0;
    available_width
        .max(0.0)
        .min(QUICK_COMMANDS_POPOVER_MAX_WIDTH)
}

fn quick_command_list_height(row_count: usize) -> f32 {
    (row_count.max(1) as f32 * QUICK_COMMAND_LIST_ESTIMATED_HEIGHT)
        .min(QUICK_COMMANDS_LIST_MAX_HEIGHT)
}

fn quick_commands_content_height(row_count: usize) -> f32 {
    (QUICK_COMMANDS_BODY_HEADER_HEIGHT + quick_command_list_height(row_count))
        .max(QUICK_COMMANDS_CONTENT_MIN_HEIGHT)
}

fn select_quick_command_category_state(
    active_category: &mut String,
    command_editor: &mut Option<QuickCommandDraft>,
    category_editor: &mut Option<QuickCommandCategoryDraft>,
    focused_input: &mut Option<QuickCommandInput>,
    highlighted_command: &mut Option<String>,
    category_id: &str,
) {
    *active_category = category_id.to_string();
    *command_editor = None;
    *category_editor = None;
    *focused_input = None;
    *highlighted_command = None;
}

fn quick_command_editor_tab_target(
    input: QuickCommandInput,
    forward: bool,
) -> Option<QuickCommandInput> {
    // Tauri quick command editors use ordinary DOM focus, so Tab/Shift+Tab
    // walks editable fields in source order. GPUI currently only models the
    // text-field focus targets here, so cycle that subset instead of letting
    // the root focused-input capture swallow Tab at the editor edges.
    const COMMAND_EDITOR_FIELDS: &[QuickCommandInput] = &[
        QuickCommandInput::CommandName,
        QuickCommandInput::CommandText,
        QuickCommandInput::CommandDescription,
        QuickCommandInput::CommandHostPattern,
    ];
    let index = COMMAND_EDITOR_FIELDS
        .iter()
        .position(|candidate| *candidate == input)?;
    if forward {
        COMMAND_EDITOR_FIELDS
            .get(index + 1)
            .copied()
            .or_else(|| COMMAND_EDITOR_FIELDS.first().copied())
    } else {
        index
            .checked_sub(1)
            .and_then(|previous| COMMAND_EDITOR_FIELDS.get(previous).copied())
            .or_else(|| COMMAND_EDITOR_FIELDS.last().copied())
    }
}

fn quick_command_space_inserts_literal(platform: bool, control: bool, alt: bool) -> bool {
    !platform && !control && !alt
}

fn quick_command_risk_tone(risk: QuickCommandRisk) -> StatusTone {
    // Risk colors are presentation policy and remain owned by the GPUI layer.
    match risk {
        QuickCommandRisk::High => StatusTone::Error,
        QuickCommandRisk::Medium => StatusTone::Warning,
    }
}

fn quick_command_risk_label(risk: QuickCommandRisk) -> &'static str {
    // Keep display labels in the UI adapter so localization can remain separate
    // from domain classification while preserving the current English badges.
    match risk {
        QuickCommandRisk::High => "high",
        QuickCommandRisk::Medium => "medium",
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum QuickCommandKeyDirection {
    Next,
    Previous,
}

fn quick_command_highlighted_index(
    visible_commands: &[QuickCommand],
    highlighted_command: Option<&str>,
) -> Option<usize> {
    highlighted_command.and_then(|id| {
        visible_commands
            .iter()
            .position(|command| command.id.as_str() == id)
    })
}

fn quick_command_keyboard_highlight(
    visible_commands: &[QuickCommand],
    highlighted_command: Option<&str>,
    direction: QuickCommandKeyDirection,
) -> Option<String> {
    if visible_commands.is_empty() {
        return None;
    }
    let current = quick_command_highlighted_index(visible_commands, highlighted_command);
    let next = match direction {
        QuickCommandKeyDirection::Next => current
            .map(|index| (index + 1).min(visible_commands.len() - 1))
            .unwrap_or(0),
        QuickCommandKeyDirection::Previous => current
            .map(|index| index.saturating_sub(1))
            .unwrap_or(visible_commands.len() - 1),
    };
    Some(visible_commands[next].id.clone())
}

fn quick_command_highlight_at(visible_commands: &[QuickCommand], index: usize) -> Option<String> {
    visible_commands
        .get(index.min(visible_commands.len().saturating_sub(1)))
        .map(|command| command.id.clone())
}

fn quick_command_row_signature(command: &QuickCommand) -> u64 {
    let mut hasher = DefaultHasher::new();
    // The command id is the row key; text fields and edit timestamps affect
    // visible row content, so include them when syncing GPUI ListState.
    command.id.hash(&mut hasher);
    command.name.hash(&mut hasher);
    command.command.hash(&mut hasher);
    command.category.hash(&mut hasher);
    command.description.hash(&mut hasher);
    command.host_pattern.hash(&mut hasher);
    command.updated_at.hash(&mut hasher);
    hasher.finish()
}

#[derive(Clone)]
struct QuickCommandsRenderSnapshot {
    categories: Vec<QuickCommandCategory>,
    category_counts: HashMap<String, usize>,
    active_category: String,
    query: String,
    focused_input: Option<QuickCommandInput>,
    highlighted_command: Option<String>,
    command_editor: Option<QuickCommandDraft>,
    category_editor: Option<QuickCommandCategoryDraft>,
    last_persist_error: Option<String>,
    visible_commands: Arc<Vec<QuickCommand>>,
    pinned: bool,
    list_state: gpui::ListState,
}

impl TerminalQuickCommandsState {
    fn visible_commands_for_targets(&self, target_fields: &[String]) -> Vec<QuickCommand> {
        self.store.visible_commands_for_targets(target_fields)
    }

    pub(in crate::workspace) fn is_open(&self) -> bool {
        self.open
    }

    pub(in crate::workspace) fn has_open_or_pending(&self) -> bool {
        self.open || self.pending_command.is_some()
    }

    pub(in crate::workspace) fn focused_input(&self) -> Option<QuickCommandInput> {
        self.store.focused_input
    }

    pub(in crate::workspace) fn close(&mut self) -> bool {
        let changed = self.has_open_or_pending()
            || self.pinned
            || self.store.focused_input.is_some()
            || self.store.highlighted_command.is_some();
        self.open = false;
        self.pinned = false;
        self.pending_command = None;
        self.store.focused_input = None;
        self.store.highlighted_command = None;
        changed
    }

    pub(in crate::workspace) fn toggle_open(&mut self) -> bool {
        if self.open {
            self.close();
            false
        } else {
            self.open = true;
            true
        }
    }

    pub(in crate::workspace) fn finish_execution(&mut self) {
        // Confirmation text may contain shell parameters, so dropping the
        // zeroizing owner is part of every completion path.
        self.pending_command = None;
        self.open = self.pinned;
    }

    pub(in crate::workspace) fn request_confirmation(&mut self, command: String) {
        self.pending_command = Some(Zeroizing::new(command));
        self.open = true;
    }

    fn prepare_insertion(&mut self, command: String, keep_open: bool) -> String {
        if keep_open {
            self.open = true;
            self.pinned = true;
            self.pending_command = None;
            self.store.focused_input = None;
            self.store.highlighted_command = None;
        } else {
            self.close();
        }
        command
    }

    fn toggle_pinned(&mut self) {
        self.pinned = !self.pinned;
    }

    fn select_category(&mut self, category_id: &str) {
        select_quick_command_category_state(
            &mut self.store.active_category,
            &mut self.store.command_editor,
            &mut self.store.category_editor,
            &mut self.store.focused_input,
            &mut self.store.highlighted_command,
            category_id,
        );
    }

    fn delete_category(&mut self, category_id: &str) {
        self.store.delete_category(category_id);
        self.store.highlighted_command = None;
    }

    pub(in crate::workspace) fn delete_command(&mut self, command_id: &str) {
        self.store.delete_command(command_id);
        self.store.highlighted_command = None;
    }

    fn set_highlighted_command(&mut self, command_id: String) -> bool {
        if self.store.highlighted_command.as_deref() == Some(command_id.as_str()) {
            return false;
        }
        self.store.highlighted_command = Some(command_id);
        true
    }

    fn set_focused_input(&mut self, input: QuickCommandInput) {
        self.store.focused_input = Some(input);
    }

    pub(in crate::workspace) fn input_value(&self, input: QuickCommandInput) -> Option<&str> {
        match input {
            QuickCommandInput::Search => Some(self.store.query.as_str()),
            QuickCommandInput::CommandName => self
                .store
                .command_editor
                .as_ref()
                .map(|draft| draft.name.as_str()),
            QuickCommandInput::CommandText => self
                .store
                .command_editor
                .as_ref()
                .map(|draft| draft.command.as_str()),
            QuickCommandInput::CommandDescription => self
                .store
                .command_editor
                .as_ref()
                .map(|draft| draft.description.as_str()),
            QuickCommandInput::CommandHostPattern => self
                .store
                .command_editor
                .as_ref()
                .map(|draft| draft.host_pattern.as_str()),
            QuickCommandInput::CategoryName => self
                .store
                .category_editor
                .as_ref()
                .map(|draft| draft.name.as_str()),
        }
    }

    fn input_value_mut(&mut self, input: QuickCommandInput) -> Option<&mut String> {
        match input {
            QuickCommandInput::Search => Some(&mut self.store.query),
            QuickCommandInput::CommandName => self
                .store
                .command_editor
                .as_mut()
                .map(|draft| &mut draft.name),
            QuickCommandInput::CommandText => self
                .store
                .command_editor
                .as_mut()
                .map(|draft| &mut draft.command),
            QuickCommandInput::CommandDescription => self
                .store
                .command_editor
                .as_mut()
                .map(|draft| &mut draft.description),
            QuickCommandInput::CommandHostPattern => self
                .store
                .command_editor
                .as_mut()
                .map(|draft| &mut draft.host_pattern),
            QuickCommandInput::CategoryName => self
                .store
                .category_editor
                .as_mut()
                .map(|draft| &mut draft.name),
        }
    }

    pub(in crate::workspace) fn replace_input(
        &mut self,
        input: QuickCommandInput,
        replacement_range: Option<std::ops::Range<usize>>,
        text: &str,
    ) -> bool {
        if self.store.focused_input != Some(input) {
            return false;
        }
        let Some(value) = self.input_value_mut(input) else {
            return false;
        };
        replace_utf16(value, replacement_range, text);
        if input == QuickCommandInput::Search {
            self.store.highlighted_command = None;
        }
        true
    }

    fn pop_input(&mut self, input: QuickCommandInput) -> bool {
        let Some(value) = self.input_value_mut(input) else {
            return false;
        };
        if value.pop().is_none() {
            return false;
        }
        if input == QuickCommandInput::Search {
            self.store.highlighted_command = None;
        }
        true
    }

    fn move_highlight(&mut self, target_fields: &[String], direction: QuickCommandKeyDirection) {
        let visible_commands = self.visible_commands_for_targets(target_fields);
        self.store.highlighted_command = quick_command_keyboard_highlight(
            &visible_commands,
            self.store.highlighted_command.as_deref(),
            direction,
        );
    }

    fn highlight_edge(&mut self, target_fields: &[String], end: bool) {
        let visible_commands = self.visible_commands_for_targets(target_fields);
        self.store.highlighted_command = if end {
            visible_commands.last().map(|command| command.id.clone())
        } else {
            quick_command_highlight_at(&visible_commands, 0)
        };
    }

    fn prepare_highlighted_insertion(&mut self, target_fields: &[String]) -> Option<String> {
        let visible_commands = self.visible_commands_for_targets(target_fields);
        let selected_index = quick_command_highlighted_index(
            &visible_commands,
            self.store.highlighted_command.as_deref(),
        )
        .unwrap_or(0);
        let command = visible_commands.get(selected_index)?.command.clone();
        Some(self.prepare_insertion(command, self.pinned))
    }

    fn cycle_editor_focus(&mut self, input: QuickCommandInput, forward: bool) -> bool {
        if self.store.command_editor.is_none() {
            return false;
        }
        let Some(next_input) = quick_command_editor_tab_target(input, forward) else {
            return false;
        };
        self.store.focused_input = Some(next_input);
        true
    }

    fn blur_input(&mut self) -> bool {
        let was_focused = self.store.focused_input.take().is_some();
        was_focused
    }

    fn start_command_create(&mut self) {
        self.store.category_editor = None;
        self.store.command_editor = Some(QuickCommandDraft {
            id: None,
            name: String::new(),
            command: String::new(),
            category: self.store.active_category.clone(),
            description: String::new(),
            host_pattern: String::new(),
        });
        self.store.focused_input = Some(QuickCommandInput::CommandName);
        self.store.highlighted_command = None;
    }

    fn start_command_edit(&mut self, command: QuickCommand) {
        self.store.category_editor = None;
        self.store.command_editor = Some(QuickCommandDraft {
            id: Some(command.id),
            name: command.name,
            command: command.command,
            category: command.category,
            description: command.description.unwrap_or_default(),
            host_pattern: command.host_pattern.unwrap_or_default(),
        });
        self.store.focused_input = Some(QuickCommandInput::CommandName);
        self.store.highlighted_command = None;
    }

    fn start_category_create(&mut self) {
        self.store.command_editor = None;
        self.store.category_editor = Some(QuickCommandCategoryDraft {
            id: None,
            name: String::new(),
            icon: QuickCommandIcon::Zap,
        });
        self.store.focused_input = Some(QuickCommandInput::CategoryName);
        self.store.highlighted_command = None;
    }

    fn start_category_edit(&mut self, category: QuickCommandCategory) {
        self.store.command_editor = None;
        self.store.category_editor = Some(QuickCommandCategoryDraft {
            id: Some(category.id),
            name: category.name,
            icon: category.icon,
        });
        self.store.focused_input = Some(QuickCommandInput::CategoryName);
        self.store.highlighted_command = None;
    }

    fn set_category_icon(&mut self, icon: QuickCommandIcon) {
        if let Some(draft) = self.store.category_editor.as_mut() {
            draft.icon = icon;
        }
    }

    fn set_command_category(&mut self, category_id: String) {
        if let Some(draft) = self.store.command_editor.as_mut() {
            draft.category = category_id;
        }
    }

    fn cancel_editor(&mut self) {
        self.store.command_editor = None;
        self.store.category_editor = None;
        self.store.focused_input = None;
        self.store.highlighted_command = None;
    }

    fn save_command_editor(&mut self) -> bool {
        let Some(draft) = self.store.command_editor.as_ref() else {
            return false;
        };
        if !quick_command_draft_can_save(draft) {
            return false;
        }
        let Some(draft) = self.store.command_editor.take() else {
            return false;
        };
        self.store.upsert_command(draft);
        self.store.focused_input = None;
        self.store.highlighted_command = None;
        true
    }

    fn save_category_editor(&mut self) -> bool {
        let Some(draft) = self.store.category_editor.as_ref() else {
            return false;
        };
        if !quick_command_category_draft_can_save(draft) {
            return false;
        }
        let Some(draft) = self.store.category_editor.take() else {
            return false;
        };
        self.store.upsert_category(draft);
        self.store.focused_input = None;
        self.store.highlighted_command = None;
        true
    }

    fn render_snapshot(&self, target_fields: &[String]) -> QuickCommandsRenderSnapshot {
        let visible_commands = Arc::new(self.visible_commands_for_targets(target_fields));
        let mut category_counts = HashMap::new();
        for command in &self.store.commands {
            *category_counts.entry(command.category.clone()).or_insert(0) += 1;
        }
        let signatures = visible_commands
            .iter()
            .map(quick_command_row_signature)
            .collect::<Vec<_>>();
        let list_spec = TauriVirtualListSpec::new(
            px(QUICK_COMMAND_LIST_ESTIMATED_HEIGHT),
            QUICK_COMMAND_LIST_OVERSCAN,
        );
        sync_tauri_variable_list_state_by_signatures(
            &self.list_state,
            &mut self.list_cache.borrow_mut(),
            "terminal-quick-commands",
            &signatures,
            list_spec,
        );
        QuickCommandsRenderSnapshot {
            categories: self.store.categories.clone(),
            category_counts,
            active_category: self.store.active_category.clone(),
            query: self.store.query.clone(),
            focused_input: self.store.focused_input,
            highlighted_command: self.store.highlighted_command.clone(),
            command_editor: self.store.command_editor.clone(),
            category_editor: self.store.category_editor.clone(),
            last_persist_error: self.store.last_persist_error.clone(),
            visible_commands,
            pinned: self.pinned,
            list_state: self.list_state.clone(),
        }
    }
}

impl WorkspaceApp {
    fn quick_commands_render_snapshot(
        &self,
        cx: &mut Context<Self>,
    ) -> QuickCommandsRenderSnapshot {
        let active_label = self
            .active_tab()
            .map(|tab| self.tab_display_title(tab))
            .unwrap_or_default();
        self.terminal
            .read(cx)
            .quick_commands
            .render_snapshot(&[active_label])
    }

    pub(in crate::workspace) fn close_terminal_quick_commands_popover(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        self.terminal
            .update(cx, |terminal, _cx| terminal.quick_commands.close())
    }

    pub(in crate::workspace) fn finish_terminal_quick_command_execution(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.terminal.update(cx, |terminal, _cx| {
            terminal.quick_commands.finish_execution()
        });
    }

    fn insert_quick_command_into_command_bar(
        &mut self,
        command: String,
        keep_open: bool,
        cx: &mut Context<Self>,
    ) {
        let command = self.terminal.update(cx, |terminal, _cx| {
            terminal
                .quick_commands
                .prepare_insertion(command, keep_open)
        });
        // The command draft remains a window/IME adapter until the command-bar
        // state moves as the next terminal slice.
        self.terminal_command_bar_draft = command;
        self.terminal_command_bar_focused = true;
    }

    pub(in crate::workspace) fn handle_quick_commands_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(input) = self.terminal.read(cx).quick_commands.focused_input() else {
            return;
        };
        let target_fields = [self
            .active_tab()
            .map(|tab| self.tab_display_title(tab))
            .unwrap_or_default()];
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        if input == QuickCommandInput::Search {
            match key {
                "escape" if !modifiers.platform && !modifiers.control => {
                    // Tauri keeps Escape as the browser-like popover dismissal
                    // path for the Command Bar quick commands surface.
                    self.close_terminal_quick_commands_popover(cx);
                    self.terminal_command_bar_focused = true;
                    self.ime_marked_text = None;
                    cx.notify();
                    return;
                }
                "arrowdown" | "down" if !modifiers.platform && !modifiers.control => {
                    self.terminal.update(cx, |terminal, _cx| {
                        terminal
                            .quick_commands
                            .move_highlight(&target_fields, QuickCommandKeyDirection::Next)
                    });
                    cx.notify();
                    return;
                }
                "arrowup" | "up" if !modifiers.platform && !modifiers.control => {
                    self.terminal.update(cx, |terminal, _cx| {
                        terminal
                            .quick_commands
                            .move_highlight(&target_fields, QuickCommandKeyDirection::Previous)
                    });
                    cx.notify();
                    return;
                }
                "home" if !modifiers.platform && !modifiers.control => {
                    self.terminal.update(cx, |terminal, _cx| {
                        terminal
                            .quick_commands
                            .highlight_edge(&target_fields, false)
                    });
                    cx.notify();
                    return;
                }
                "end" if !modifiers.platform && !modifiers.control => {
                    self.terminal.update(cx, |terminal, _cx| {
                        terminal.quick_commands.highlight_edge(&target_fields, true)
                    });
                    cx.notify();
                    return;
                }
                "enter" if !modifiers.platform && !modifiers.control => {
                    let command = self.terminal.update(cx, |terminal, _cx| {
                        terminal
                            .quick_commands
                            .prepare_highlighted_insertion(&target_fields)
                    });
                    if let Some(command) = command {
                        self.terminal_command_bar_draft = command;
                        self.terminal_command_bar_focused = true;
                        cx.notify();
                    }
                    return;
                }
                _ => {}
            }
        }
        match key {
            "tab" if !modifiers.platform && !modifiers.control => {
                if self.terminal.update(cx, |terminal, _cx| {
                    terminal
                        .quick_commands
                        .cycle_editor_focus(input, !modifiers.shift)
                }) {
                    self.clear_ime_selection();
                    self.ime_marked_text = None;
                    cx.notify();
                }
            }
            "escape" => {
                if self
                    .terminal
                    .update(cx, |terminal, _cx| terminal.quick_commands.blur_input())
                {
                    self.ime_marked_text = None;
                    cx.notify();
                }
            }
            "enter" if input == QuickCommandInput::CategoryName => {
                if self.terminal.update(cx, |terminal, _cx| {
                    terminal.quick_commands.save_category_editor()
                }) {
                    cx.notify();
                }
            }
            "enter"
                if matches!(
                    input,
                    QuickCommandInput::CommandName
                        | QuickCommandInput::CommandText
                        | QuickCommandInput::CommandDescription
                        | QuickCommandInput::CommandHostPattern
                ) =>
            {
                if self.terminal.update(cx, |terminal, _cx| {
                    terminal.quick_commands.save_command_editor()
                }) {
                    cx.notify();
                }
            }
            "backspace" if !modifiers.platform && !modifiers.control => {
                if self
                    .terminal
                    .update(cx, |terminal, _cx| terminal.quick_commands.pop_input(input))
                {
                    // Empty Backspace does not change the active field or the
                    // filtered command list, so skip a redundant repaint.
                    cx.notify();
                }
            }
            "space" | " "
                if quick_command_space_inserts_literal(
                    modifiers.platform,
                    modifiers.control,
                    modifiers.alt,
                ) =>
            {
                // Some GPUI platforms deliver Space without key_char, so the
                // platform text owner never commits it. Route that fallback
                // through the same IME replacement path as ordinary text.
                let target = WorkspaceImeTarget::QuickCommand(input);
                let replacement_range = self.ime_selection_range_for_target(target, cx);
                let caret = replacement_range
                    .as_ref()
                    .map(|range| range.start + " ".encode_utf16().count());
                self.clear_ime_selection();
                self.replace_ime_target_text(target, replacement_range, " ", cx);
                if let Some(caret) = caret {
                    self.set_ime_selection_from_anchor(target, caret, caret);
                }
            }
            _ => {}
        }
    }

    pub(in crate::workspace) fn quick_command_input_value(
        &self,
        input: QuickCommandInput,
        cx: &App,
    ) -> Option<String> {
        self.terminal
            .read(cx)
            .quick_commands
            .input_value(input)
            .map(str::to_string)
    }

    pub(in crate::workspace) fn render_quick_commands_popover(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let snapshot = self.quick_commands_render_snapshot(cx);
        let popover_width = self
            .select_anchors
            .get(&SelectAnchorId::TerminalCommandBar)
            .map(|anchor| quick_commands_popover_width_for_bar(f32::from(anchor.bounds.size.width)))
            .unwrap_or(QUICK_COMMANDS_POPOVER_MAX_WIDTH);
        let mut popover = command_panel(
            &self.tokens,
            CommandPanelOptions::new()
                .width(popover_width)
                .max_height(520.0)
                .padding(SurfacePadding::None)
                .terminal_owned(),
        )
        .absolute()
        .bottom(px(56.0))
        .right(px(QUICK_COMMANDS_POPOVER_HORIZONTAL_MARGIN))
        // The popover sits inside an occluding outside-dismiss backdrop.
        // Mark the panel itself as occluding too, so category-row clicks
        // are hit-tested against this event island instead of the backdrop.
        .occlude()
        // Tauri uses `w-[min(860px,calc(100%-1.5rem))]` on a child of
        // TerminalCommandBar. Compute against the cached command-bar
        // bounds so AI sidebar and window-width changes shrink the panel
        // instead of clipping its left edge.
        .max_w(px(QUICK_COMMANDS_POPOVER_MAX_WIDTH))
        .text_size(px(12.0))
        .font_family(settings_mono_font_family(self.settings_store.settings()))
        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_mouse_down(MouseButton::Right, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_scroll_wheel(|_, _, cx| {
            // Match Tauri's popover scroll boundary: wheel input inside
            // the quick command surface must not close the overlay or leak
            // to the terminal behind it.
            cx.stop_propagation();
        });

        let content_height = quick_commands_content_height(snapshot.visible_commands.len());
        let sidebar = self.render_quick_command_category_sidebar(&snapshot, cx);
        let body = self.render_quick_command_body(&snapshot, cx);
        popover = popover.child(
            div()
                // command_panel is column-oriented; quick commands need their
                // sidebar and body to share one explicit row-height owner.
                .h(px(content_height))
                .min_h(px(0.0))
                .flex()
                .child(sidebar)
                .child(body),
        );
        popover.into_any_element()
    }

    fn render_quick_command_category_sidebar(
        &self,
        snapshot: &QuickCommandsRenderSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mut sidebar = div()
            .w(px(160.0))
            .h_full()
            .flex_none()
            .overflow_hidden()
            .rounded_l(px(rounded_shell_child_radius(self.tokens.radii.lg)))
            .border_r_1()
            .border_color(rgba((theme.border << 8) | 0x99))
            .bg(rgba((theme.bg << 8) | 0x73))
            .p(px(8.0))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .mb(px(2.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text_muted))
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::PlainDocument,
                                "quick-commands",
                                "title",
                                self.i18n.t("terminal.quick_commands.title").to_uppercase(),
                                theme.text_muted,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .child(self.quick_command_pin_button(
                                snapshot.pinned,
                                |this, _event, _window, cx| {
                                    this.terminal.update(cx, |terminal, _cx| {
                                        terminal.quick_commands.toggle_pinned()
                                    });
                                    cx.stop_propagation();
                                    cx.notify();
                                },
                                cx,
                            ))
                            .child(self.quick_command_icon_button(
                                LucideIcon::Plus,
                                |this, _event, _window, cx| {
                                    this.start_quick_command_category_create(cx);
                                    cx.stop_propagation();
                                },
                                cx,
                            ))
                            .child(self.quick_command_icon_button(
                                LucideIcon::X,
                                |this, _event, _window, cx| {
                                    this.close_terminal_quick_commands_popover(cx);
                                    cx.stop_propagation();
                                    cx.notify();
                                },
                                cx,
                            )),
                    ),
            );

        for category in &snapshot.categories {
            let category_id = category.id.clone();
            let active = category.id == snapshot.active_category;
            let count = snapshot
                .category_counts
                .get(category.id.as_str())
                .copied()
                .unwrap_or_default();
            let can_delete = !default_quick_command_categories()
                .iter()
                .any(|default| default.id == category.id)
                && count == 0;
            sidebar = sidebar.child(
                div()
                    .group("quick-command-category")
                    .cursor_pointer()
                    .rounded(px(self.tokens.radii.md))
                    .px(px(8.0))
                    .py(px(6.0))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .bg(if active {
                        rgba((theme.accent << 8) | 0x1f)
                    } else {
                        rgba(0x00000000)
                    })
                    .text_color(if active {
                        rgb(theme.accent)
                    } else {
                        rgb(theme.text_muted)
                    })
                    .hover(move |style| style.bg(rgb(theme.bg_hover)).text_color(rgb(theme.text)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener({
                            let category_id = category_id.clone();
                            move |this, _event, _window, cx| {
                                this.terminal.update(cx, |terminal, _cx| {
                                    terminal.quick_commands.select_category(&category_id)
                                });
                                cx.stop_propagation();
                                cx.notify();
                            }
                        }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(Self::render_lucide_icon(
                                quick_command_lucide_icon(category.icon),
                                14.0,
                                if active {
                                    rgb(theme.accent)
                                } else {
                                    rgb(theme.text_muted)
                                },
                            ))
                            .child(div().flex_1().truncate().child(
                                // Tauri renders category labels as plain spans inside
                                // a button. Do not attach selectable-text mouse
                                // handlers here; category clicks must stay inside
                                // the popover instead of reaching outside-dismiss.
                                self.render_display_text_with_role(
                                    SelectableTextRole::NonSelectable,
                                    "quick-command-category-cell",
                                    ("name", category.id.as_str()),
                                    category.name.clone(),
                                    if active {
                                        theme.accent
                                    } else {
                                        theme.text_muted
                                    },
                                    cx,
                                ),
                            ))
                            .child(status_pill(
                                &self.tokens,
                                count.to_string(),
                                StatusPillOptions::new(StatusTone::Neutral).compact(),
                            )),
                    )
                    .child(self.quick_command_mini_button(
                        LucideIcon::Pencil,
                        {
                            let category = category.clone();
                            move |this, _event, _window, cx| {
                                this.start_quick_command_category_edit(category.clone(), cx);
                                cx.stop_propagation();
                            }
                        },
                        cx,
                    ))
                    .when(can_delete, |row| {
                        row.child(self.quick_command_mini_button(
                            LucideIcon::Trash2,
                            {
                                let category_id = category_id.clone();
                                move |this, _event, _window, cx| {
                                    this.terminal.update(cx, |terminal, _cx| {
                                        terminal.quick_commands.delete_category(&category_id)
                                    });
                                    cx.stop_propagation();
                                    cx.notify();
                                }
                            },
                            cx,
                        ))
                    }),
            );
        }

        sidebar
            .child(div().flex_1())
            .when_some(snapshot.last_persist_error.as_ref(), |sidebar, error| {
                sidebar.child(
                    div()
                        .rounded(px(self.tokens.radii.md))
                        .border_1()
                        .border_color(rgba(0xef444480))
                        .bg(rgba(0xef44441a))
                        .p(px(6.0))
                        .text_size(px(10.0))
                        .text_color(rgba(0xfca5a5ff))
                        .child(error.clone()),
                )
            })
            .into_any_element()
    }

    fn render_quick_command_body(
        &self,
        snapshot: &QuickCommandsRenderSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex_1()
            .h_full()
            .min_w(px(0.0))
            .overflow_hidden()
            .rounded_r(px(rounded_shell_child_radius(self.tokens.radii.lg)))
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .border_b_1()
                    .border_color(rgba((theme.border << 8) | 0x99))
                    .p(px(8.0))
                    .child(div().flex_1().min_w(px(0.0)).child(
                        self.render_quick_command_text_input(
                            QuickCommandInput::Search,
                            snapshot.query.clone(),
                            snapshot.focused_input,
                            self.i18n.t("terminal.quick_commands.search_placeholder"),
                            cx,
                        ),
                    ))
                    .child(
                        div()
                            .h(px(32.0))
                            .px(px(8.0))
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .rounded(px(self.tokens.radii.md))
                            .border_1()
                            .border_color(rgba((theme.border << 8) | 0x99))
                            .cursor_pointer()
                            .text_color(rgb(theme.text_muted))
                            .hover(move |style| {
                                style.bg(rgb(theme.bg_hover)).text_color(rgb(theme.text))
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.start_quick_command_create(cx);
                                    cx.stop_propagation();
                                }),
                            )
                            .child(Self::render_lucide_icon(
                                LucideIcon::Plus,
                                14.0,
                                rgb(theme.text_muted),
                            ))
                            // Tauri treats this as a select-none control label; selection must not steal the button click.
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::NonSelectable,
                                "quick-command-add-button",
                                "label",
                                self.i18n.t("terminal.quick_commands.add"),
                                theme.text_muted,
                                cx,
                            )),
                    ),
            )
            .when_some(snapshot.category_editor.as_ref(), |body, _| {
                body.child(self.render_quick_command_category_editor(snapshot, cx))
            })
            .when_some(snapshot.command_editor.as_ref(), |body, _| {
                body.child(self.render_quick_command_editor(snapshot, cx))
            })
            .child(self.render_quick_command_rows(snapshot, cx))
            .into_any_element()
    }

    fn render_quick_command_rows(
        &self,
        snapshot: &QuickCommandsRenderSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        if snapshot.visible_commands.is_empty() {
            return div()
                .h(px(180.0))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(8.0))
                .text_color(rgb(theme.text_muted))
                .child(Self::render_lucide_icon(
                    LucideIcon::Zap,
                    20.0,
                    rgb(theme.text_muted),
                ))
                .child(self.render_display_text_with_role(
                    SelectableTextRole::PlainDocument,
                    "quick-commands-empty",
                    snapshot.query.as_str(),
                    if snapshot.query.trim().is_empty() {
                        self.i18n.t("terminal.quick_commands.empty_category")
                    } else {
                        self.i18n.t("terminal.quick_commands.empty_search")
                    },
                    theme.text_muted,
                    cx,
                ))
                .into_any_element();
        }

        let state = snapshot.list_state.clone();
        let spec = TauriVirtualListSpec::new(
            px(QUICK_COMMAND_LIST_ESTIMATED_HEIGHT),
            QUICK_COMMAND_LIST_OVERSCAN,
        );
        let workspace = cx.entity();
        let visible_commands = snapshot.visible_commands.clone();
        let pinned = snapshot.pinned;
        let highlighted_command = snapshot.highlighted_command.clone();
        div()
            .flex_1()
            .min_h(px(0.0))
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(tauri_virtual_list(
                state,
                spec,
                move |index, _window, cx| {
                    let visible_commands = visible_commands.clone();
                    let highlighted_command = highlighted_command.clone();
                    workspace.update(cx, |this, cx| {
                        this.render_quick_command_list_item(
                            index,
                            &visible_commands,
                            pinned,
                            highlighted_command.as_deref(),
                            cx,
                        )
                    })
                },
            ))
            .into_any_element()
    }

    fn render_quick_command_list_item(
        &self,
        index: usize,
        visible_commands: &[QuickCommand],
        pinned: bool,
        highlighted_command: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let total = visible_commands.len();
        let Some(command) = visible_commands.get(index).cloned() else {
            return div().into_any_element();
        };
        div()
            .px(px(8.0))
            .when(index == 0, |item| item.pt(px(8.0)))
            .pb(px(if index + 1 == total { 8.0 } else { 4.0 }))
            .child(self.render_quick_command_row(command, pinned, highlighted_command, cx))
            .into_any_element()
    }

    fn render_quick_command_row(
        &self,
        command: QuickCommand,
        pinned: bool,
        highlighted_command: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let risk = classify_command_risk(&command.command);
        let command_for_insert = command.command.clone();
        let command_for_run = command.command.clone();
        let command_for_edit = command.clone();
        let command_id = command.id.clone();
        let command_id_for_hover = command.id.clone();
        let keep_open_for_insert = pinned;
        let highlighted = highlighted_command == Some(command.id.as_str());
        let selection_group_id = crate::workspace::selectable_text::selectable_text_id(
            "quick-command-row",
            command.id.as_str(),
        );
        div()
            .rounded(px(self.tokens.radii.md))
            .px(px(8.0))
            .py(px(8.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_color(rgb(theme.text_muted))
            .bg(if highlighted {
                rgba((theme.bg_hover << 8) | 0xb3)
            } else {
                rgba(0x00000000)
            })
            .hover(move |style| {
                style
                    .bg(rgba((theme.bg_hover << 8) | 0xb3))
                    .text_color(rgb(theme.text))
            })
            .on_mouse_move(
                cx.listener(move |this, _event: &gpui::MouseMoveEvent, _window, cx| {
                    // Mouse hover and ArrowUp/ArrowDown share the same active
                    // row state, matching browser menu focus without changing
                    // row-safe selectable click bubbling.
                    if this.terminal.update(cx, |terminal, _cx| {
                        terminal
                            .quick_commands
                            .set_highlighted_command(command_id_for_hover.clone())
                    }) {
                        cx.notify();
                    }
                }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, window, cx| {
                            this.insert_quick_command_into_command_bar(
                                command_for_insert.clone(),
                                keep_open_for_insert,
                                cx,
                            );
window.focus(&this.focus_handle, cx);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .truncate()
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text))
                                    .child(self.render_row_safe_selectable_display_text_in_group(
                                        selection_group_id,
                                        "quick-command-row-cell",
                                        ("name", command.id.as_str()),
                                        0,
                                        command.name.clone(),
                                        theme.text,
                                        None,
                                        cx,
                                    )),
                            )
                            .when_some(risk, |row, risk| {
                                row.child(
                                    status_pill(
                                        &self.tokens,
                                        quick_command_risk_label(risk).to_uppercase(),
                                        StatusPillOptions::new(quick_command_risk_tone(risk))
                                            .compact()
                                            .strong(),
                                    ),
                                )
                            })
                            .when_some(command.host_pattern.as_ref(), |row, pattern| {
                                row.child(
                                    status_pill(
                                        &self.tokens,
                                        pattern.clone(),
                                        StatusPillOptions::new(StatusTone::Neutral).compact(),
                                    ),
                                )
                            }),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(12.0))
                            .text_color(rgba((theme.accent << 8) | 0xd9))
                            .child(self.render_row_safe_selectable_display_text_in_group_with_alpha(
                                selection_group_id,
                                "quick-command-row-cell",
                                ("command", command.id.as_str()),
                                1,
                                command.command.clone(),
                                theme.accent,
                                0xd9 as f32 / 255.0,
                                None,
                                cx,
                            )),
                    )
                    .when_some(command.description.as_ref(), |row, description| {
                        row.child(
                            div()
                                .truncate()
                                .text_size(px(11.0))
                                .text_color(rgba((theme.text_muted << 8) | 0xb3))
                                .child(self.render_row_safe_selectable_display_text_in_group_with_alpha(
                                    selection_group_id,
                                    "quick-command-row-cell",
                                    ("description", command.id.as_str()),
                                    2,
                                    description.clone(),
                                    theme.text_muted,
                                    0xb3 as f32 / 255.0,
                                    None,
                                    cx,
                                )),
                        )
                    }),
            )
            .child(self.quick_command_action_button(
                LucideIcon::Play,
                move |this, _event, window, cx| {
                    this.run_quick_command(&command_for_run, window, cx);
                    cx.stop_propagation();
                },
                cx,
            ))
            .child(self.quick_command_action_button(
                LucideIcon::Pencil,
                move |this, _event, _window, cx| {
                    this.start_quick_command_edit(command_for_edit.clone(), cx);
                    cx.stop_propagation();
                },
                cx,
            ))
            .child(self.quick_command_action_button(
                LucideIcon::Trash2,
                move |this, _event, _window, cx| {
                    this.terminal.update(cx, |terminal, _cx| {
                        terminal.quick_commands.delete_command(&command_id)
                    });
                    cx.stop_propagation();
                    cx.notify();
                },
                cx,
            ))
            .into_any_element()
    }

    fn render_quick_command_category_editor(
        &self,
        snapshot: &QuickCommandsRenderSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let Some(draft) = snapshot.category_editor.as_ref() else {
            return div().into_any_element();
        };
        let can_save = quick_command_category_draft_can_save(draft);
        let mut icon_options = div().flex().items_center().gap(px(4.0));
        for icon in [
            QuickCommandIcon::Terminal,
            QuickCommandIcon::Server,
            QuickCommandIcon::Folder,
            QuickCommandIcon::Docker,
            QuickCommandIcon::Zap,
        ] {
            let active = draft.icon == icon;
            icon_options = icon_options.child(
                div()
                    .h(px(30.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(if active {
                        rgb(theme.accent)
                    } else {
                        rgba((theme.border << 8) | 0x80)
                    })
                    .bg(if active {
                        rgba((theme.accent << 8) | 0x1a)
                    } else {
                        rgba(0x00000000)
                    })
                    .cursor_pointer()
                    .text_color(if active {
                        rgb(theme.accent)
                    } else {
                        rgb(theme.text_muted)
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.terminal.update(cx, |terminal, _cx| {
                                terminal.quick_commands.set_category_icon(icon)
                            });
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .child(Self::render_lucide_icon(
                        quick_command_lucide_icon(icon),
                        13.0,
                        if active {
                            rgb(theme.accent)
                        } else {
                            rgb(theme.text_muted)
                        },
                    ))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "quick-command-icon-option",
                        quick_command_icon_source_id(icon),
                        self.i18n.t(&quick_command_icon_label_key(icon)),
                        if active {
                            theme.accent
                        } else {
                            theme.text_muted
                        },
                        cx,
                    )),
            );
        }

        div()
            .border_b_1()
            .border_color(rgba((theme.border << 8) | 0x99))
            .bg(rgba((theme.bg << 8) | 0x59))
            .p(px(8.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .grid()
                    .gap(px(8.0))
                    .child(
                        self.render_quick_command_text_input(
                            QuickCommandInput::CategoryName,
                            draft.name.clone(),
                            snapshot.focused_input,
                            self.i18n
                                .t("terminal.quick_commands.group_name_placeholder"),
                            cx,
                        ),
                    )
                    .child(icon_options),
            )
            .child(self.render_quick_editor_buttons(
                can_save,
                "terminal.quick_commands.save_group",
                |this, cx| this.save_quick_command_category_editor(cx),
                cx,
            ))
            .into_any_element()
    }

    fn render_quick_command_editor(
        &self,
        snapshot: &QuickCommandsRenderSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let Some(draft) = snapshot.command_editor.as_ref() else {
            return div().into_any_element();
        };
        let can_save = quick_command_draft_can_save(draft);
        let mut categories = div().flex().items_center().gap(px(4.0)).flex_wrap();
        for category in &snapshot.categories {
            let category_id = category.id.clone();
            let active = draft.category == category.id;
            categories = categories.child(
                div()
                    .h(px(28.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(if active {
                        rgb(theme.accent)
                    } else {
                        rgba((theme.border << 8) | 0x80)
                    })
                    .text_color(if active {
                        rgb(theme.accent)
                    } else {
                        rgb(theme.text_muted)
                    })
                    .bg(if active {
                        rgba((theme.accent << 8) | 0x1a)
                    } else {
                        rgba(0x00000000)
                    })
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.terminal.update(cx, |terminal, _cx| {
                                terminal
                                    .quick_commands
                                    .set_command_category(category_id.clone())
                            });
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "quick-command-editor-category",
                        category.id.as_str(),
                        category.name.clone(),
                        if active {
                            theme.accent
                        } else {
                            theme.text_muted
                        },
                        cx,
                    )),
            );
        }

        div()
            .border_b_1()
            .border_color(rgba((theme.border << 8) | 0x99))
            .bg(rgba((theme.bg << 8) | 0x59))
            .p(px(8.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .grid()
                    .gap(px(8.0))
                    .child(self.render_quick_command_text_input(
                        QuickCommandInput::CommandName,
                        draft.name.clone(),
                        snapshot.focused_input,
                        self.i18n.t("terminal.quick_commands.name_placeholder"),
                        cx,
                    ))
                    .child(self.render_quick_command_text_input(
                        QuickCommandInput::CommandText,
                        draft.command.clone(),
                        snapshot.focused_input,
                        self.i18n.t("terminal.quick_commands.command_placeholder"),
                        cx,
                    ))
                    .child(
                        self.render_quick_command_text_input(
                            QuickCommandInput::CommandDescription,
                            draft.description.clone(),
                            snapshot.focused_input,
                            self.i18n
                                .t("terminal.quick_commands.description_placeholder"),
                            cx,
                        ),
                    )
                    .child(
                        self.render_quick_command_text_input(
                            QuickCommandInput::CommandHostPattern,
                            draft.host_pattern.clone(),
                            snapshot.focused_input,
                            self.i18n
                                .t("terminal.quick_commands.host_pattern_placeholder"),
                            cx,
                        ),
                    )
                    .child(categories),
            )
            .child(self.render_quick_editor_buttons(
                can_save,
                "terminal.quick_commands.save",
                |this, cx| this.save_quick_command_editor(cx),
                cx,
            ))
            .into_any_element()
    }

    fn render_quick_editor_buttons(
        &self,
        can_save: bool,
        save_key: &'static str,
        save: fn(&mut WorkspaceApp, &mut Context<WorkspaceApp>),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex()
            .justify_end()
            .gap(px(8.0))
            .child(self.quick_command_text_button(
                self.i18n.t("terminal.quick_commands.cancel"),
                true,
                cx.listener(|this, _event, _window, cx| {
                    this.terminal
                        .update(cx, |terminal, _cx| terminal.quick_commands.cancel_editor());
                    cx.stop_propagation();
                    cx.notify();
                }),
            ))
            .child(
                self.quick_command_text_button(
                    self.i18n.t(save_key),
                    can_save,
                    cx.listener(move |this, _event, _window, cx| {
                        save(this, cx);
                        cx.stop_propagation();
                    }),
                )
                .bg(if can_save {
                    rgba((theme.accent << 8) | 0x26)
                } else {
                    rgba(0x00000000)
                }),
            )
            .into_any_element()
    }

    fn render_quick_command_text_input(
        &self,
        input: QuickCommandInput,
        value: String,
        focused_input: Option<QuickCommandInput>,
        placeholder: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = focused_input == Some(input);
        let target = WorkspaceImeTarget::QuickCommand(input);
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value: &value,
                    placeholder,
                    focused,
                    caret_visible: self.new_connection_caret_visible,
                    secret: false,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target, cx),
                    marked_text: self.marked_text_for_target(target, cx),
                },
            )
            .h(px(32.0))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    this.terminal.update(cx, |terminal, _cx| {
                        terminal.quick_commands.set_focused_input(input)
                    });
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle, cx);
                    this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(cx.listener(
                |this, event: &gpui::MouseMoveEvent, window, cx| {
                    this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                },
            )),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    fn start_quick_command_create(&mut self, cx: &mut Context<Self>) {
        self.terminal.update(cx, |terminal, _cx| {
            terminal.quick_commands.start_command_create()
        });
        cx.notify();
    }

    fn start_quick_command_edit(&mut self, command: QuickCommand, cx: &mut Context<Self>) {
        self.terminal.update(cx, |terminal, _cx| {
            terminal.quick_commands.start_command_edit(command)
        });
        cx.notify();
    }

    fn start_quick_command_category_create(&mut self, cx: &mut Context<Self>) {
        self.terminal.update(cx, |terminal, _cx| {
            terminal.quick_commands.start_category_create()
        });
        cx.notify();
    }

    fn start_quick_command_category_edit(
        &mut self,
        category: QuickCommandCategory,
        cx: &mut Context<Self>,
    ) {
        self.terminal.update(cx, |terminal, _cx| {
            terminal.quick_commands.start_category_edit(category)
        });
        cx.notify();
    }

    fn save_quick_command_editor(&mut self, cx: &mut Context<Self>) {
        if self.terminal.update(cx, |terminal, _cx| {
            terminal.quick_commands.save_command_editor()
        }) {
            cx.notify();
        }
    }

    fn save_quick_command_category_editor(&mut self, cx: &mut Context<Self>) {
        if self.terminal.update(cx, |terminal, _cx| {
            terminal.quick_commands.save_category_editor()
        }) {
            cx.notify();
        }
    }
}

#[cfg(test)]
mod terminal_command_bar_quick_command_tests {
    use super::{
        QuickCommand, QuickCommandCategoryDraft, QuickCommandDraft, QuickCommandIcon,
        QuickCommandInput, QuickCommandKeyDirection, QuickCommandRisk, StatusTone,
        close_terminal_quick_commands_popover_state, finish_quick_command_execution_state,
        insert_quick_command_into_command_bar_state, quick_command_category_draft_can_save,
        quick_command_draft_can_save, quick_command_editor_tab_target,
        quick_command_keyboard_highlight, quick_command_risk_label, quick_command_risk_tone,
        quick_command_space_inserts_literal, quick_commands_popover_width_for_bar,
        select_quick_command_category_state,
    };

    #[test]
    fn quick_command_popover_outside_click_closes_without_blurring_command_bar() {
        let mut open = true;
        let mut pinned = true;
        let mut pending_command = Some("rm -rf /tmp/example".to_string());
        let mut focused_input = Some(QuickCommandInput::Search);
        let mut highlighted_command = Some("qc-risky".to_string());
        let command_bar_focused = true;

        close_terminal_quick_commands_popover_state(
            &mut open,
            &mut pinned,
            &mut pending_command,
            &mut focused_input,
            &mut highlighted_command,
        );

        assert!(!open);
        assert!(!pinned);
        assert_eq!(pending_command, None);
        assert_eq!(focused_input, None);
        assert_eq!(highlighted_command, None);
        assert!(command_bar_focused);
    }

    #[test]
    fn quick_command_unpinned_row_click_inserts_command_and_closes_popover() {
        let mut draft = String::new();
        let mut command_bar_focused = false;
        let mut open = true;
        let mut pinned = false;
        let mut pending_command = Some("docker system prune".to_string());
        let mut focused_input = Some(QuickCommandInput::Search);
        let mut highlighted_command = Some("qc-docker".to_string());

        insert_quick_command_into_command_bar_state(
            &mut draft,
            "git status",
            false,
            &mut command_bar_focused,
            &mut open,
            &mut pinned,
            &mut pending_command,
            &mut focused_input,
            &mut highlighted_command,
        );

        assert_eq!(draft, "git status");
        assert!(command_bar_focused);
        assert!(!open);
        assert!(!pinned);
        assert_eq!(pending_command, None);
        assert_eq!(focused_input, None);
        assert_eq!(highlighted_command, None);
    }

    #[test]
    fn quick_command_pinned_row_click_inserts_command_and_keeps_popover_open() {
        let mut draft = String::new();
        let mut command_bar_focused = false;
        let mut open = true;
        let mut pinned = false;
        let mut pending_command = Some("docker system prune".to_string());
        let mut focused_input = Some(QuickCommandInput::Search);
        let mut highlighted_command = Some("qc-docker".to_string());

        insert_quick_command_into_command_bar_state(
            &mut draft,
            "git status",
            true,
            &mut command_bar_focused,
            &mut open,
            &mut pinned,
            &mut pending_command,
            &mut focused_input,
            &mut highlighted_command,
        );

        assert_eq!(draft, "git status");
        assert!(command_bar_focused);
        assert!(open);
        assert!(pinned);
        assert_eq!(pending_command, None);
        assert_eq!(focused_input, None);
        assert_eq!(highlighted_command, None);
    }

    #[test]
    fn quick_command_pinned_execution_keeps_popover_open() {
        let mut open = true;
        let pinned = true;
        let mut pending_command = Some("apt update".to_string());

        finish_quick_command_execution_state(&mut open, pinned, &mut pending_command);

        assert!(open);
        assert_eq!(pending_command, None);
    }

    #[test]
    fn quick_command_unpinned_execution_closes_popover() {
        let mut open = true;
        let pinned = false;
        let mut pending_command = Some("apt update".to_string());

        finish_quick_command_execution_state(&mut open, pinned, &mut pending_command);

        assert!(!open);
        assert_eq!(pending_command, None);
    }

    #[test]
    fn quick_command_keyboard_highlight_clamps_like_browser_menu_focus() {
        let commands = vec![
            QuickCommand {
                id: "first".to_string(),
                name: "First".to_string(),
                command: "pwd".to_string(),
                category: "system".to_string(),
                description: None,
                host_pattern: None,
                created_at: 0,
                updated_at: 0,
            },
            QuickCommand {
                id: "second".to_string(),
                name: "Second".to_string(),
                command: "ls".to_string(),
                category: "system".to_string(),
                description: None,
                host_pattern: None,
                created_at: 0,
                updated_at: 0,
            },
        ];

        assert_eq!(
            quick_command_keyboard_highlight(&commands, None, QuickCommandKeyDirection::Next),
            Some("first".to_string())
        );
        assert_eq!(
            quick_command_keyboard_highlight(
                &commands,
                Some("first"),
                QuickCommandKeyDirection::Next
            ),
            Some("second".to_string())
        );
        assert_eq!(
            quick_command_keyboard_highlight(
                &commands,
                Some("second"),
                QuickCommandKeyDirection::Next
            ),
            Some("second".to_string())
        );
        assert_eq!(
            quick_command_keyboard_highlight(&commands, None, QuickCommandKeyDirection::Previous),
            Some("second".to_string())
        );
        assert_eq!(
            quick_command_keyboard_highlight(
                &commands,
                Some("missing"),
                QuickCommandKeyDirection::Next
            ),
            Some("first".to_string())
        );
        assert_eq!(
            quick_command_keyboard_highlight(&[], None, QuickCommandKeyDirection::Next),
            None
        );
    }

    #[test]
    fn quick_command_editor_tab_cycles_text_fields_without_swallowing_focus() {
        assert_eq!(
            quick_command_editor_tab_target(QuickCommandInput::CommandName, true),
            Some(QuickCommandInput::CommandText)
        );
        assert_eq!(
            quick_command_editor_tab_target(QuickCommandInput::CommandHostPattern, true),
            Some(QuickCommandInput::CommandName)
        );
        assert_eq!(
            quick_command_editor_tab_target(QuickCommandInput::CommandName, false),
            Some(QuickCommandInput::CommandHostPattern)
        );
        assert_eq!(
            quick_command_editor_tab_target(QuickCommandInput::Search, true),
            None
        );
    }

    #[test]
    fn quick_command_plain_space_is_literal_text() {
        assert!(quick_command_space_inserts_literal(false, false, false));
        assert!(!quick_command_space_inserts_literal(true, false, false));
        assert!(!quick_command_space_inserts_literal(false, true, false));
        assert!(!quick_command_space_inserts_literal(false, false, true));
    }

    #[test]
    fn quick_command_risk_maps_domain_values_to_ui_labels_and_tones() {
        assert_eq!(
            quick_command_risk_tone(QuickCommandRisk::High),
            StatusTone::Error
        );
        assert_eq!(
            quick_command_risk_tone(QuickCommandRisk::Medium),
            StatusTone::Warning
        );
        assert_eq!(quick_command_risk_label(QuickCommandRisk::High), "high");
        assert_eq!(quick_command_risk_label(QuickCommandRisk::Medium), "medium");
    }

    #[test]
    fn quick_command_popover_width_matches_tauri_min_calc() {
        assert_eq!(quick_commands_popover_width_for_bar(1200.0), 860.0);
        assert_eq!(quick_commands_popover_width_for_bar(600.0), 576.0);
        assert_eq!(quick_commands_popover_width_for_bar(240.0), 216.0);
    }

    #[test]
    fn quick_command_category_switch_keeps_popover_open() {
        let open = true;
        let mut active_category = "files".to_string();
        let mut command_editor = Some(QuickCommandDraft {
            id: Some("command".to_string()),
            name: "List".to_string(),
            command: "ls".to_string(),
            category: "files".to_string(),
            description: String::new(),
            host_pattern: String::new(),
        });
        let mut category_editor = Some(QuickCommandCategoryDraft {
            id: Some("files".to_string()),
            name: "Files".to_string(),
            icon: QuickCommandIcon::Folder,
        });
        let mut focused_input = Some(QuickCommandInput::CommandName);
        let mut highlighted_command = Some("list".to_string());

        select_quick_command_category_state(
            &mut active_category,
            &mut command_editor,
            &mut category_editor,
            &mut focused_input,
            &mut highlighted_command,
            "docker",
        );

        assert!(open);
        assert_eq!(active_category, "docker");
        assert!(command_editor.is_none());
        assert!(category_editor.is_none());
        assert!(focused_input.is_none());
        assert!(highlighted_command.is_none());
    }

    #[test]
    fn quick_command_editor_save_gate_matches_tauri_disabled_button() {
        assert!(!quick_command_draft_can_save(&QuickCommandDraft {
            id: None,
            name: String::new(),
            command: "git status".to_string(),
            category: "system".to_string(),
            description: String::new(),
            host_pattern: String::new(),
        }));
        assert!(!quick_command_draft_can_save(&QuickCommandDraft {
            id: None,
            name: "Status".to_string(),
            command: "   ".to_string(),
            category: "system".to_string(),
            description: String::new(),
            host_pattern: String::new(),
        }));
        assert!(quick_command_draft_can_save(&QuickCommandDraft {
            id: None,
            name: "Status".to_string(),
            command: "git status".to_string(),
            category: "system".to_string(),
            description: String::new(),
            host_pattern: String::new(),
        }));
    }

    #[test]
    fn quick_command_category_editor_save_gate_matches_tauri_disabled_button() {
        assert!(!quick_command_category_draft_can_save(
            &QuickCommandCategoryDraft {
                id: None,
                name: "   ".to_string(),
                icon: QuickCommandIcon::Zap,
            }
        ));
        assert!(quick_command_category_draft_can_save(
            &QuickCommandCategoryDraft {
                id: None,
                name: "Ops".to_string(),
                icon: QuickCommandIcon::Zap,
            }
        ));
    }
}
