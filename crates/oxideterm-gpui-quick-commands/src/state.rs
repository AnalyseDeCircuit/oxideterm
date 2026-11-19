pub(crate) use oxideterm_quick_commands::{
    QUICK_COMMANDS_SCHEMA_VERSION, QuickCommand, QuickCommandAvailability, QuickCommandCategory,
    QuickCommandCategoryDraft, QuickCommandConfirmationPolicy, QuickCommandIcon,
    QuickCommandImportResult, QuickCommandImportStrategy, QuickCommandMatchField,
    QuickCommandParameter, QuickCommandParameterKind, QuickCommandTargetProtocol,
    QuickCommandsSnapshot, default_quick_command_categories, default_quick_commands, now_ms,
};
use std::sync::Arc;
use std::{
    cell::RefCell,
    collections::{HashMap, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};

use gpui::{Entity, ListAlignment, ListState, ScrollHandle, Subscription, px};
use oxideterm_editor_core::utf16::replace_utf16;
use oxideterm_gpui_editor::TextEditorView;
use oxideterm_gpui_ui::text_input::TextInputViewport;
use oxideterm_quick_commands::{
    FALLBACK_QUICK_COMMAND_CATEGORY_ID, QuickCommandTargetContext,
    quick_command_category_draft_can_save, validate_quick_command_template,
};
use zeroize::Zeroizing;

use crate::{QuickCommandsViews, panel::QuickCommandsPanelState};
use oxideterm_gpui_ui::virtual_list::{
    TauriVirtualListSpec, VirtualListSignatureCache, sync_tauri_variable_list_state_by_signatures,
};

pub(crate) const QUICK_COMMAND_LIST_INITIAL_ITEM_COUNT: usize = 0;
pub(crate) const QUICK_COMMAND_LIST_ESTIMATED_HEIGHT: f32 = 56.0;
pub(crate) const QUICK_COMMAND_LIST_OVERSCAN: usize = 6;

pub const QUICK_COMMAND_TEXTAREA_LINE_HEIGHT: f32 = 22.0;
pub(crate) const QUICK_COMMAND_TEXTAREA_MIN_HEIGHT: f32 = 126.0;
pub const QUICK_COMMAND_TEXTAREA_VERTICAL_PADDING: f32 = 8.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum QuickCommandInput {
    Search,
    CommandName,
    CommandText,
    CommandDescription,
    CommandHostPattern,
    ParameterName(usize),
    ParameterLabel(usize),
    ParameterDefault(usize),
    ParameterChoices(usize),
    CategoryName,
    CategoryColor,
}

impl QuickCommandInput {
    pub fn anchor_key(self) -> u64 {
        match self {
            Self::Search => 1,
            Self::CommandName => 2,
            Self::CommandText => 3,
            Self::CommandDescription => 4,
            Self::CommandHostPattern => 5,
            Self::CategoryName => 6,
            Self::CategoryColor => 7,
            Self::ParameterName(index) => 100 + index as u64 * 4,
            Self::ParameterLabel(index) => 101 + index as u64 * 4,
            Self::ParameterDefault(index) => 102 + index as u64 * 4,
            Self::ParameterChoices(index) => 103 + index as u64 * 4,
        }
    }
}

#[derive(Clone)]
pub(crate) struct QuickCommandParameterEditorDraft {
    pub name: String,
    pub label: String,
    pub kind: QuickCommandParameterKind,
    pub default_value: String,
    pub choices: String,
    pub required: bool,
}

#[derive(Clone)]
pub(crate) struct QuickCommandEditorDraft {
    pub id: Option<String>,
    pub name: String,
    pub command: String,
    pub category: String,
    pub description: String,
    pub host_patterns: String,
    pub match_field: QuickCommandMatchField,
    pub parameters: Vec<QuickCommandParameterEditorDraft>,
    pub protocols: Vec<QuickCommandTargetProtocol>,
    pub confirmation: QuickCommandConfirmationPolicy,
    pub created_at: u64,
    pub sort_order: i64,
}

#[derive(Clone)]
pub struct QuickCommandExecutionDraft {
    pub command: QuickCommand,
    // Runtime substitutions may contain credentials, so dialog ownership is
    // also the zeroization boundary for every entered value.
    pub parameter_values: Vec<Zeroizing<String>>,
}

#[derive(Clone)]
pub(crate) struct QuickCommandCategoryDeletePrompt {
    pub id: String,
    pub name: String,
}

pub(crate) fn quick_command_icon_source_id(icon: QuickCommandIcon) -> &'static str {
    match icon {
        QuickCommandIcon::Terminal => "terminal",
        QuickCommandIcon::Server => "server",
        QuickCommandIcon::Folder => "folder",
        QuickCommandIcon::Docker => "docker",
        QuickCommandIcon::Zap => "zap",
    }
}

#[derive(Clone)]
pub struct QuickCommandsState {
    pub(crate) settings_path: PathBuf,
    pub categories: Vec<QuickCommandCategory>,
    pub commands: Vec<QuickCommand>,
    pub active_category: String,
    pub query: String,
    pub focused_input: Option<QuickCommandInput>,
    // Browser popovers keep one active option for keyboard navigation without
    // stealing the row click target; store the stable command id instead of a
    // transient index so filtering and category changes cannot select a stale row.
    pub highlighted_command: Option<String>,
    pub(crate) command_editor: Option<QuickCommandEditorDraft>,
    pub category_editor: Option<QuickCommandCategoryDraft>,
    pub last_persist_error: Option<String>,
}

/// Owns the quick-command store and its complete terminal-surface lifecycle.
pub struct TerminalQuickCommandsState {
    pub store: QuickCommandsState,
    pub views: Option<QuickCommandsViews>,
    pub(crate) open: bool,
    pub(crate) manager_open: bool,
    pub(crate) pinned: bool,
    pub panel: QuickCommandsPanelState,
    pub pending_execution: Option<QuickCommandExecutionDraft>,
    pub(crate) pending_category_delete: Option<QuickCommandCategoryDeletePrompt>,
    pub(crate) list_state: ListState,
    pub(crate) list_cache: RefCell<VirtualListSignatureCache>,
    pub(crate) input_viewports: RefCell<HashMap<QuickCommandInput, TextInputViewport>>,
    pub command_input: Option<Entity<TextEditorView>>,
    pub(crate) command_input_subscription: Option<Subscription>,
    pub(crate) editor_scroll: ScrollHandle,
}

impl TerminalQuickCommandsState {
    pub fn load(settings_path: &Path) -> Self {
        Self {
            store: QuickCommandsState::load(settings_path),
            views: None,
            open: false,
            manager_open: false,
            pinned: false,
            panel: QuickCommandsPanelState::default(),
            pending_execution: None,
            pending_category_delete: None,
            // User-defined command sets are unbounded, so the surface owns a
            // virtual list instead of rebuilding every row on root repaint.
            list_state: ListState::new(
                QUICK_COMMAND_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(QUICK_COMMAND_LIST_ESTIMATED_HEIGHT),
                    QUICK_COMMAND_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            list_cache: RefCell::new(VirtualListSignatureCache::default()),
            // Each editor field keeps browser-like horizontal position across redraws.
            input_viewports: RefCell::new(HashMap::new()),
            command_input: None,
            command_input_subscription: None,
            editor_scroll: ScrollHandle::new(),
        }
    }

    pub fn input_viewport(&self, input: QuickCommandInput) -> TextInputViewport {
        self.input_viewports
            .borrow_mut()
            .entry(input)
            .or_default()
            .clone()
    }
}

fn select_quick_command_category_state(
    active_category: &mut String,
    command_editor: &mut Option<QuickCommandEditorDraft>,
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

pub(crate) fn quick_command_editor_can_save(draft: &QuickCommandEditorDraft) -> bool {
    if draft.name.trim().is_empty() || draft.command.trim().is_empty() {
        return false;
    }
    let mut names = std::collections::HashSet::new();
    let mut parameters = Vec::with_capacity(draft.parameters.len());
    for parameter in &draft.parameters {
        let name = parameter.name.trim();
        let mut characters = name.chars();
        let valid_name = characters
            .next()
            .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
            && characters.all(|character| character.is_ascii_alphanumeric() || character == '_');
        let choices = parameter
            .choices
            .split([',', '\n'])
            .map(str::trim)
            .filter(|choice| !choice.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        let default_value = parameter.default_value.trim();
        if !valid_name
            || !names.insert(name.to_string())
            || parameter.label.trim().is_empty()
            || (parameter.kind == QuickCommandParameterKind::Choice && choices.is_empty())
            || (parameter.kind == QuickCommandParameterKind::Choice
                && !default_value.is_empty()
                && !choices.iter().any(|choice| choice == default_value))
            || (parameter.kind == QuickCommandParameterKind::Secret
                && (!default_value.is_empty() || !choices.is_empty()))
        {
            return false;
        }
        parameters.push(QuickCommandParameter {
            name: name.to_string(),
            label: parameter.label.trim().to_string(),
            kind: parameter.kind,
            default_value: (parameter.kind != QuickCommandParameterKind::Secret
                && !default_value.is_empty())
            .then(|| default_value.to_string()),
            choices: if parameter.kind == QuickCommandParameterKind::Secret {
                Vec::new()
            } else {
                choices
            },
            required: parameter.required,
        });
    }
    // A command cannot enter persisted state with misspelled or malformed template tokens.
    validate_quick_command_template(&draft.command, &parameters).is_ok()
}

pub fn quick_command_space_inserts_literal(platform: bool, control: bool, alt: bool) -> bool {
    !platform && !control && !alt
}

pub fn quick_command_input_uses_monospace(input: QuickCommandInput) -> bool {
    matches!(
        input,
        QuickCommandInput::CommandText
            | QuickCommandInput::CategoryColor
            | QuickCommandInput::CommandHostPattern
            | QuickCommandInput::ParameterName(_)
            | QuickCommandInput::ParameterDefault(_)
            | QuickCommandInput::ParameterChoices(_)
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuickCommandKeyDirection {
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
    command.parameters.hash(&mut hasher);
    command.availability.hash(&mut hasher);
    command.confirmation.hash(&mut hasher);
    command.sort_order.hash(&mut hasher);
    command.updated_at.hash(&mut hasher);
    hasher.finish()
}

#[derive(Clone)]
pub(crate) struct QuickCommandsRenderSnapshot {
    pub(crate) categories: Vec<QuickCommandCategory>,
    pub(crate) category_counts: HashMap<String, usize>,
    pub(crate) active_category: String,
    pub(crate) query: String,
    pub(crate) focused_input: Option<QuickCommandInput>,
    pub(crate) highlighted_command: Option<String>,
    pub(crate) command_editor: Option<QuickCommandEditorDraft>,
    pub(crate) category_editor: Option<QuickCommandCategoryDraft>,
    pub(crate) pending_execution: Option<QuickCommandExecutionDraft>,
    pub(crate) pending_category_delete: Option<QuickCommandCategoryDeletePrompt>,
    pub(crate) last_persist_error: Option<String>,
    pub(crate) visible_commands: Arc<Vec<QuickCommand>>,
    pub(crate) pinned: bool,
    pub(crate) managing: bool,
    pub(crate) list_state: gpui::ListState,
}

impl TerminalQuickCommandsState {
    pub(crate) fn visible_commands_for_targets(
        &self,
        target: Option<&QuickCommandTargetContext>,
    ) -> Vec<QuickCommand> {
        self.store.visible_commands_for_targets(target)
    }

    pub fn quick_bar_snapshot(
        &self,
        target: Option<&QuickCommandTargetContext>,
        category_id: Option<&str>,
    ) -> (Vec<QuickCommandCategory>, Vec<QuickCommand>) {
        // QuickBar is a read-only projection of the existing persisted store.
        // Preserve category and command order instead of creating a second model.
        let category_id =
            category_id.filter(|id| self.store.categories.iter().any(|c| c.id == *id));
        (
            self.store.categories.clone(),
            self.store
                .commands
                .iter()
                .filter(|command| {
                    category_id.is_none_or(|id| command.category == id)
                        && target.map_or_else(
                            || {
                                command.availability.protocols.is_empty()
                                    && command.availability.host_patterns.is_empty()
                            },
                            |target| {
                                oxideterm_quick_commands::quick_command_available_for_target(
                                    command, target,
                                )
                            },
                        )
                })
                .cloned()
                .collect(),
        )
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn manager_open(&self) -> bool {
        self.manager_open
    }

    pub fn has_open_or_pending(&self) -> bool {
        self.open || self.pending_execution.is_some()
    }

    pub fn focused_input(&self) -> Option<QuickCommandInput> {
        self.store.focused_input
    }

    pub fn close(&mut self) -> bool {
        let changed = self.has_open_or_pending()
            || self.pinned
            || self.store.focused_input.is_some()
            || self.store.highlighted_command.is_some();
        self.open = false;
        self.panel.finish_resize();
        self.pinned = false;
        self.pending_execution = None;
        self.store.focused_input = None;
        self.store.highlighted_command = None;
        changed
    }

    pub fn toggle_open(&mut self) -> bool {
        if self.open {
            self.close();
            false
        } else {
            self.open = true;
            true
        }
    }

    pub fn finish_execution(&mut self) {
        // Confirmation text may contain shell parameters, so dropping the
        // zeroizing owner is part of every completion path.
        self.pending_execution = None;
        self.open = self.pinned;
        self.store.focused_input = None;
        if !self.open {
            self.panel.finish_resize();
        }
    }

    pub fn request_execution(&mut self, command: QuickCommand) {
        self.editor_scroll = gpui::ScrollHandle::new();
        self.store.focused_input = None;
        let parameter_values = command
            .parameters
            .iter()
            .map(|parameter| Zeroizing::new(parameter.default_value.clone().unwrap_or_default()))
            .collect();
        self.pending_execution = Some(QuickCommandExecutionDraft {
            command,
            parameter_values,
        });
        self.open = true;
    }

    pub(crate) fn cancel_execution(&mut self) -> bool {
        self.store.focused_input = None;
        self.pending_execution.take().is_some()
    }

    pub(crate) fn prepare_insertion(&mut self, command: String, keep_open: bool) -> String {
        if keep_open {
            self.open = true;
            self.pinned = true;
            self.store.focused_input = None;
            self.store.highlighted_command = None;
        } else {
            self.close();
        }
        command
    }

    pub(crate) fn toggle_pinned(&mut self) {
        self.pinned = !self.pinned;
    }

    pub fn open_manager(&mut self) {
        // The manager is a workspace modal with an independent lifecycle, so
        // opening it must release the dock and its pending execution first.
        self.open = false;
        self.panel.finish_resize();
        self.pinned = false;
        self.pending_execution = None;
        self.pending_category_delete = None;
        self.manager_open = true;
        self.store.command_editor = None;
        self.command_input = None;
        self.command_input_subscription = None;
        self.store.category_editor = None;
        self.store.focused_input = Some(QuickCommandInput::Search);
        self.store.highlighted_command = None;
    }

    pub fn close_manager(&mut self) -> bool {
        let changed = self.manager_open
            || self.store.command_editor.is_some()
            || self.store.category_editor.is_some()
            || self.store.focused_input.is_some()
            || self.pending_category_delete.is_some();
        self.manager_open = false;
        self.pending_category_delete = None;
        self.store.command_editor = None;
        self.command_input = None;
        self.command_input_subscription = None;
        self.store.category_editor = None;
        self.store.focused_input = None;
        self.store.highlighted_command = None;
        changed
    }

    pub(crate) fn select_category(&mut self, category_id: &str) {
        self.command_input = None;
        self.command_input_subscription = None;
        select_quick_command_category_state(
            &mut self.store.active_category,
            &mut self.store.command_editor,
            &mut self.store.category_editor,
            &mut self.store.focused_input,
            &mut self.store.highlighted_command,
            category_id,
        );
    }

    pub(crate) fn request_category_delete(&mut self, category: QuickCommandCategory) {
        if category.id == FALLBACK_QUICK_COMMAND_CATEGORY_ID {
            return;
        }
        self.pending_category_delete = Some(QuickCommandCategoryDeletePrompt {
            id: category.id,
            name: category.name,
        });
        self.store.focused_input = None;
        self.store.highlighted_command = None;
    }

    pub fn category_delete_pending(&self) -> bool {
        self.pending_category_delete.is_some()
    }

    pub fn cancel_category_delete(&mut self) -> bool {
        self.pending_category_delete.take().is_some()
    }

    pub fn confirm_category_delete(&mut self) -> bool {
        let Some(prompt) = self.pending_category_delete.take() else {
            return false;
        };
        self.store.highlighted_command = None;
        self.store.delete_category(&prompt.id)
    }

    pub fn delete_command(&mut self, command_id: &str) {
        self.store.delete_command(command_id);
        self.store.highlighted_command = None;
    }

    pub(crate) fn set_highlighted_command(&mut self, command_id: String) -> bool {
        if self.store.highlighted_command.as_deref() == Some(command_id.as_str()) {
            return false;
        }
        self.store.highlighted_command = Some(command_id);
        true
    }

    pub(crate) fn set_focused_input(&mut self, input: QuickCommandInput) {
        self.store.focused_input = Some(input);
    }

    pub fn input_value(&self, input: QuickCommandInput) -> Option<&str> {
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
                .map(|draft| draft.host_patterns.as_str()),
            QuickCommandInput::ParameterName(index) => self
                .store
                .command_editor
                .as_ref()?
                .parameters
                .get(index)
                .map(|parameter| parameter.name.as_str()),
            QuickCommandInput::ParameterLabel(index) => self
                .store
                .command_editor
                .as_ref()?
                .parameters
                .get(index)
                .map(|parameter| parameter.label.as_str()),
            QuickCommandInput::ParameterDefault(index) => {
                if let Some(execution) = self.pending_execution.as_ref() {
                    execution
                        .parameter_values
                        .get(index)
                        .map(|value| value.as_str())
                } else {
                    self.store
                        .command_editor
                        .as_ref()?
                        .parameters
                        .get(index)
                        .map(|parameter| parameter.default_value.as_str())
                }
            }
            QuickCommandInput::ParameterChoices(index) => self
                .store
                .command_editor
                .as_ref()?
                .parameters
                .get(index)
                .map(|parameter| parameter.choices.as_str()),
            QuickCommandInput::CategoryName => self
                .store
                .category_editor
                .as_ref()
                .map(|draft| draft.name.as_str()),
            QuickCommandInput::CategoryColor => self
                .store
                .category_editor
                .as_ref()
                .map(|draft| draft.icon_color.as_str()),
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
                .map(|draft| &mut draft.host_patterns),
            QuickCommandInput::ParameterName(index) => self
                .store
                .command_editor
                .as_mut()?
                .parameters
                .get_mut(index)
                .map(|parameter| &mut parameter.name),
            QuickCommandInput::ParameterLabel(index) => self
                .store
                .command_editor
                .as_mut()?
                .parameters
                .get_mut(index)
                .map(|parameter| &mut parameter.label),
            QuickCommandInput::ParameterDefault(index) => {
                if let Some(execution) = self.pending_execution.as_mut() {
                    execution
                        .parameter_values
                        .get_mut(index)
                        .map(|value| &mut **value)
                } else {
                    self.store
                        .command_editor
                        .as_mut()?
                        .parameters
                        .get_mut(index)
                        .map(|parameter| &mut parameter.default_value)
                }
            }
            QuickCommandInput::ParameterChoices(index) => self
                .store
                .command_editor
                .as_mut()?
                .parameters
                .get_mut(index)
                .map(|parameter| &mut parameter.choices),
            QuickCommandInput::CategoryName => self
                .store
                .category_editor
                .as_mut()
                .map(|draft| &mut draft.name),
            QuickCommandInput::CategoryColor => self
                .store
                .category_editor
                .as_mut()
                .map(|draft| &mut draft.icon_color),
        }
    }

    pub fn replace_input(
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

    pub fn move_highlight(
        &mut self,
        target: Option<&QuickCommandTargetContext>,
        direction: QuickCommandKeyDirection,
    ) {
        let visible_commands = self.visible_commands_for_targets(target);
        self.store.highlighted_command = quick_command_keyboard_highlight(
            &visible_commands,
            self.store.highlighted_command.as_deref(),
            direction,
        );
    }

    pub fn highlight_edge(&mut self, target: Option<&QuickCommandTargetContext>, end: bool) {
        let visible_commands = self.visible_commands_for_targets(target);
        self.store.highlighted_command = if end {
            visible_commands.last().map(|command| command.id.clone())
        } else {
            quick_command_highlight_at(&visible_commands, 0)
        };
    }

    pub fn prepare_highlighted_insertion(
        &mut self,
        target: Option<&QuickCommandTargetContext>,
    ) -> Option<String> {
        let visible_commands = self.visible_commands_for_targets(target);
        let selected_index = quick_command_highlighted_index(
            &visible_commands,
            self.store.highlighted_command.as_deref(),
        )
        .unwrap_or(0);
        let command = visible_commands.get(selected_index)?.command.clone();
        Some(self.prepare_insertion(command, self.pinned))
    }

    pub fn cycle_editor_focus(&mut self, input: QuickCommandInput, forward: bool) -> bool {
        let mut fields = Vec::new();
        if self.store.category_editor.is_some() {
            fields.push(QuickCommandInput::CategoryName);
            fields.push(QuickCommandInput::CategoryColor);
        } else if let Some(draft) = self.store.command_editor.as_ref() {
            fields.extend([
                QuickCommandInput::CommandName,
                QuickCommandInput::CommandText,
                QuickCommandInput::CommandDescription,
                QuickCommandInput::CommandHostPattern,
            ]);
            for (index, parameter) in draft.parameters.iter().enumerate() {
                fields.extend([
                    QuickCommandInput::ParameterName(index),
                    QuickCommandInput::ParameterLabel(index),
                ]);
                if parameter.kind != QuickCommandParameterKind::Secret {
                    fields.push(QuickCommandInput::ParameterDefault(index));
                }
                if parameter.kind == QuickCommandParameterKind::Choice {
                    fields.push(QuickCommandInput::ParameterChoices(index));
                }
            }
        }
        let Some(index) = fields.iter().position(|candidate| *candidate == input) else {
            return false;
        };
        let next_index = if forward {
            (index + 1) % fields.len()
        } else if index == 0 {
            fields.len() - 1
        } else {
            index - 1
        };
        let Some(next_input) = fields.get(next_index).copied() else {
            return false;
        };
        self.store.focused_input = Some(next_input);
        true
    }

    pub fn blur_input(&mut self) -> bool {
        let was_focused = self.store.focused_input.take().is_some();
        was_focused
    }

    pub(crate) fn start_command_create(&mut self) {
        self.store.category_editor = None;
        self.manager_open = true;
        self.store.command_editor = Some(QuickCommandEditorDraft {
            id: None,
            name: String::new(),
            command: String::new(),
            category: self.store.active_category.clone(),
            description: String::new(),
            host_patterns: String::new(),
            match_field: QuickCommandMatchField::Host,
            parameters: Vec::new(),
            protocols: Vec::new(),
            confirmation: QuickCommandConfirmationPolicy::Inherit,
            created_at: now_ms(),
            sort_order: self
                .store
                .commands
                .iter()
                .map(|command| command.sort_order)
                .max()
                .unwrap_or(-1)
                .saturating_add(1),
        });
        self.store.focused_input = Some(QuickCommandInput::CommandName);
        self.store.highlighted_command = None;
    }

    pub(crate) fn start_command_edit(&mut self, command: QuickCommand) {
        self.store.category_editor = None;
        self.manager_open = true;
        self.store.command_editor = Some(QuickCommandEditorDraft {
            id: Some(command.id),
            name: command.name,
            command: command.command,
            category: command.category,
            description: command.description.unwrap_or_default(),
            host_patterns: command.availability.host_patterns.join(", "),
            match_field: command.availability.match_field,
            parameters: command
                .parameters
                .into_iter()
                .map(|parameter| QuickCommandParameterEditorDraft {
                    name: parameter.name,
                    label: parameter.label,
                    kind: parameter.kind,
                    default_value: parameter.default_value.unwrap_or_default(),
                    choices: parameter.choices.join(", "),
                    required: parameter.required,
                })
                .collect(),
            protocols: command.availability.protocols,
            confirmation: command.confirmation,
            created_at: command.created_at,
            sort_order: command.sort_order,
        });
        self.store.focused_input = Some(QuickCommandInput::CommandName);
        self.store.highlighted_command = None;
    }

    pub(crate) fn start_category_create(&mut self) {
        self.store.command_editor = None;
        self.command_input = None;
        self.command_input_subscription = None;
        self.store.category_editor = Some(QuickCommandCategoryDraft {
            id: None,
            name: String::new(),
            icon: QuickCommandIcon::Zap,
            icon_color: String::new(),
        });
        self.store.focused_input = Some(QuickCommandInput::CategoryName);
        self.store.highlighted_command = None;
    }

    pub(crate) fn start_category_edit(&mut self, category: QuickCommandCategory) {
        self.store.command_editor = None;
        self.command_input = None;
        self.command_input_subscription = None;
        self.store.category_editor = Some(QuickCommandCategoryDraft {
            id: Some(category.id),
            name: category.name,
            icon: category.icon,
            icon_color: category.icon_color.unwrap_or_default(),
        });
        self.store.focused_input = Some(QuickCommandInput::CategoryName);
        self.store.highlighted_command = None;
    }

    pub(crate) fn set_category_icon(&mut self, icon: QuickCommandIcon) {
        if let Some(draft) = self.store.category_editor.as_mut() {
            draft.icon = icon;
        }
    }

    pub(crate) fn set_command_category(&mut self, category_id: String) {
        if let Some(draft) = self.store.command_editor.as_mut() {
            draft.category = category_id;
        }
    }

    pub(crate) fn add_command_parameter(&mut self) {
        let Some(draft) = self.store.command_editor.as_mut() else {
            return;
        };
        draft.parameters.push(QuickCommandParameterEditorDraft {
            name: String::new(),
            label: String::new(),
            kind: QuickCommandParameterKind::Text,
            default_value: String::new(),
            choices: String::new(),
            required: false,
        });
        self.store.focused_input = Some(QuickCommandInput::ParameterName(
            draft.parameters.len().saturating_sub(1),
        ));
    }

    pub(crate) fn remove_command_parameter(&mut self, index: usize) {
        if let Some(draft) = self.store.command_editor.as_mut()
            && index < draft.parameters.len()
        {
            draft.parameters.remove(index);
            self.store.focused_input = None;
        }
    }

    pub(crate) fn set_command_parameter_kind(
        &mut self,
        index: usize,
        kind: QuickCommandParameterKind,
    ) {
        if let Some(parameter) = self
            .store
            .command_editor
            .as_mut()
            .and_then(|draft| draft.parameters.get_mut(index))
        {
            parameter.kind = kind;
            if kind == QuickCommandParameterKind::Secret {
                // Secret values exist only in the execution dialog and never in persisted drafts.
                parameter.default_value.clear();
                parameter.choices.clear();
            }
        }
    }

    pub(crate) fn toggle_command_parameter_required(&mut self, index: usize) {
        if let Some(parameter) = self
            .store
            .command_editor
            .as_mut()
            .and_then(|draft| draft.parameters.get_mut(index))
        {
            parameter.required = !parameter.required;
        }
    }

    pub(crate) fn toggle_command_protocol(&mut self, protocol: QuickCommandTargetProtocol) {
        let Some(draft) = self.store.command_editor.as_mut() else {
            return;
        };
        if let Some(index) = draft
            .protocols
            .iter()
            .position(|candidate| *candidate == protocol)
        {
            draft.protocols.remove(index);
        } else {
            draft.protocols.push(protocol);
        }
    }

    pub(crate) fn toggle_command_confirmation(&mut self) {
        if let Some(draft) = self.store.command_editor.as_mut() {
            draft.confirmation = match draft.confirmation {
                QuickCommandConfirmationPolicy::Inherit => QuickCommandConfirmationPolicy::Always,
                QuickCommandConfirmationPolicy::Always => QuickCommandConfirmationPolicy::Inherit,
            };
        }
    }

    pub(crate) fn set_execution_parameter_value(&mut self, index: usize, value: String) {
        if let Some(value_slot) = self
            .pending_execution
            .as_mut()
            .and_then(|execution| execution.parameter_values.get_mut(index))
        {
            *value_slot = Zeroizing::new(value);
        }
    }

    pub(crate) fn cancel_editor(&mut self) {
        self.store.command_editor = None;
        self.command_input = None;
        self.command_input_subscription = None;
        self.store.category_editor = None;
        self.store.focused_input = None;
        self.store.highlighted_command = None;
    }

    pub(crate) fn save_command_editor(&mut self) -> bool {
        let Some(draft) = self.store.command_editor.as_ref() else {
            return false;
        };
        if !quick_command_editor_can_save(draft) {
            return false;
        }
        let Some(draft) = self.store.command_editor.take() else {
            return false;
        };
        if !self.store.upsert_editor_command(draft.clone()) {
            self.store.command_editor = Some(draft);
            return false;
        }
        self.command_input = None;
        self.command_input_subscription = None;
        self.store.focused_input = None;
        self.store.highlighted_command = None;
        true
    }

    pub fn save_category_editor(&mut self) -> bool {
        let Some(draft) = self.store.category_editor.as_ref() else {
            return false;
        };
        if !quick_command_category_draft_can_save(draft) {
            return false;
        }
        let Some(draft) = self.store.category_editor.take() else {
            return false;
        };
        self.store.upsert_category(draft.clone());
        if self.store.last_persist_error.is_some() {
            self.store.category_editor = Some(draft);
            return false;
        }
        self.store.focused_input = None;
        self.store.highlighted_command = None;
        true
    }

    pub(crate) fn render_snapshot(
        &self,
        target: Option<&QuickCommandTargetContext>,
    ) -> QuickCommandsRenderSnapshot {
        let visible_commands = Arc::new(if self.manager_open {
            self.store.visible_commands_for_management()
        } else {
            self.visible_commands_for_targets(target)
        });
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
            pending_execution: self.pending_execution.clone(),
            pending_category_delete: self.pending_category_delete.clone(),
            last_persist_error: self.store.last_persist_error.clone(),
            visible_commands,
            pinned: self.pinned,
            managing: self.manager_open,
            list_state: self.list_state.clone(),
        }
    }
}
