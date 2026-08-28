use std::{
    collections::{HashMap, hash_map::DefaultHasher},
    fs,
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::Arc,
};

use gpui::{
    AnyElement, App, Context, KeyDownEvent, MouseButton, PathPromptOptions, SharedString, div,
    prelude::*, px, rgb, rgba,
};
use oxideterm_editor_core::utf16::replace_utf16;
use oxideterm_gpui_ui::{
    CommandPanelOptions, StatusPillOptions, StatusTone, SurfacePadding, command_panel,
    modal::{dismissible_dialog_backdrop, overlay_content_boundary, rounded_shell_child_radius},
    scroll::ScrollableElement,
    select::SelectAnchorId,
    status_pill,
    text_input::{TextInputView, text_input_with_viewport},
};
use oxideterm_i18n::I18n;
use oxideterm_quick_commands::{
    QuickCommandRisk, QuickCommandTemplateError, classify_command_risk,
    quick_command_category_draft_can_save, quick_command_has_runtime_substitutions,
    validate_quick_command_template,
};
use zeroize::Zeroizing;

use super::super::ime::WorkspaceImeTarget;
use super::super::{
    QUICK_COMMAND_LIST_ESTIMATED_HEIGHT, QUICK_COMMAND_LIST_OVERSCAN, SelectableTextRole,
    TauriVirtualListSpec, TerminalNotice, TerminalNoticeVariant, WorkspaceApp,
    settings_mono_font_family, settings_ui_font_family,
    sync_tauri_variable_list_state_by_signatures, tauri_virtual_list,
};
use super::{
    QuickCommand, QuickCommandCategory, QuickCommandCategoryDraft, QuickCommandConfirmationPolicy,
    QuickCommandEditorDraft, QuickCommandExecutionDraft, QuickCommandIcon,
    QuickCommandImportStrategy, QuickCommandInput, QuickCommandParameter,
    QuickCommandParameterEditorDraft, QuickCommandParameterKind, QuickCommandTargetProtocol,
    TerminalQuickCommandsState, default_quick_command_categories, now_ms,
    quick_command_icon_source_id,
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

const QUICK_COMMANDS_POPOVER_MAX_WIDTH: f32 = 680.0;
const QUICK_COMMANDS_POPOVER_HORIZONTAL_MARGIN: f32 = 12.0;
const QUICK_COMMANDS_MANAGER_WIDTH: f32 = 1120.0;
const QUICK_COMMANDS_MANAGER_HEIGHT: f32 = 720.0;
const QUICK_COMMANDS_MANAGER_COMMAND_LIST_WIDTH: f32 = 360.0;
const QUICK_COMMANDS_LIST_MAX_HEIGHT: f32 = 360.0;
const QUICK_COMMANDS_CONTENT_MIN_HEIGHT: f32 = 300.0;
const QUICK_COMMANDS_BODY_HEADER_HEIGHT: f32 = 49.0;
const QUICK_COMMAND_CATEGORY_PICKER_MAX_HEIGHT: f32 = 72.0;

fn quick_command_icon_label_key(icon: QuickCommandIcon) -> String {
    format!(
        "terminal.quick_commands.icon_{}",
        quick_command_icon_source_id(icon)
    )
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

fn quick_commands_export_directory() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
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

fn quick_command_editor_can_save(draft: &QuickCommandEditorDraft) -> bool {
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

fn quick_command_space_inserts_literal(platform: bool, control: bool, alt: bool) -> bool {
    !platform && !control && !alt
}

pub(in crate::workspace) fn quick_command_input_uses_monospace(input: QuickCommandInput) -> bool {
    matches!(
        input,
        QuickCommandInput::CommandText
            | QuickCommandInput::CommandHostPattern
            | QuickCommandInput::ParameterName(_)
            | QuickCommandInput::ParameterDefault(_)
            | QuickCommandInput::ParameterChoices(_)
    )
}

fn quick_command_risk_tone(risk: QuickCommandRisk) -> StatusTone {
    // Risk colors are presentation policy and remain owned by the GPUI layer.
    match risk {
        QuickCommandRisk::High => StatusTone::Error,
        QuickCommandRisk::Medium => StatusTone::Warning,
    }
}

fn quick_command_risk_label(i18n: &I18n, risk: QuickCommandRisk) -> String {
    match risk {
        QuickCommandRisk::High => i18n.t("terminal.quick_commands.risk_high"),
        QuickCommandRisk::Medium => i18n.t("terminal.quick_commands.risk_medium"),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum QuickCommandRiskBadge {
    Classified(QuickCommandRisk),
    Dynamic,
}

fn quick_command_risk_badge(template: &str) -> Option<QuickCommandRiskBadge> {
    classify_command_risk(template)
        .map(QuickCommandRiskBadge::Classified)
        .or_else(|| {
            quick_command_has_runtime_substitutions(template)
                .then_some(QuickCommandRiskBadge::Dynamic)
        })
}

fn quick_command_risk_badge_label(i18n: &I18n, badge: QuickCommandRiskBadge) -> String {
    match badge {
        QuickCommandRiskBadge::Classified(risk) => quick_command_risk_label(i18n, risk),
        QuickCommandRiskBadge::Dynamic => i18n.t("terminal.quick_commands.risk_dynamic"),
    }
}

fn quick_command_risk_badge_tone(badge: QuickCommandRiskBadge) -> StatusTone {
    match badge {
        QuickCommandRiskBadge::Classified(risk) => quick_command_risk_tone(risk),
        QuickCommandRiskBadge::Dynamic => StatusTone::Warning,
    }
}

fn quick_command_template_error_label(i18n: &I18n, error: &QuickCommandTemplateError) -> String {
    let (key, replacements) = match error {
        QuickCommandTemplateError::UnterminatedToken => (
            "terminal.quick_commands.error_unterminated_token",
            Vec::new(),
        ),
        QuickCommandTemplateError::UnknownToken(token) => (
            "terminal.quick_commands.error_unknown_token",
            vec![("{{token}}", token.as_str())],
        ),
        QuickCommandTemplateError::UnknownModifier(modifier) => (
            "terminal.quick_commands.error_unknown_modifier",
            vec![("{{modifier}}", modifier.as_str())],
        ),
        QuickCommandTemplateError::UnknownParameter(parameter) => (
            "terminal.quick_commands.error_unknown_parameter",
            vec![("{{parameter}}", parameter.as_str())],
        ),
        QuickCommandTemplateError::TooManyParameterValues => (
            "terminal.quick_commands.error_too_many_parameter_values",
            Vec::new(),
        ),
        QuickCommandTemplateError::ParameterValueTooLong(parameter) => (
            "terminal.quick_commands.error_parameter_value_too_long",
            vec![("{{parameter}}", parameter.as_str())],
        ),
        QuickCommandTemplateError::ExpandedCommandTooLong { target } => (
            "terminal.quick_commands.error_expanded_command_too_long",
            vec![("{{target}}", target.as_str())],
        ),
        QuickCommandTemplateError::MissingParameter(parameter) => (
            "terminal.quick_commands.error_missing_parameter",
            vec![("{{parameter}}", parameter.as_str())],
        ),
        QuickCommandTemplateError::InvalidChoice { parameter } => (
            "terminal.quick_commands.error_invalid_choice",
            vec![("{{parameter}}", parameter.as_str())],
        ),
        QuickCommandTemplateError::MissingContext { target, field } => (
            "terminal.quick_commands.error_missing_context",
            vec![
                ("{{target}}", target.as_str()),
                ("{{field}}", field.as_str()),
            ],
        ),
    };
    replacements
        .into_iter()
        .fold(i18n.t(key), |label, (placeholder, value)| {
            label.replace(placeholder, value)
        })
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
    command.parameters.hash(&mut hasher);
    command.availability.hash(&mut hasher);
    command.confirmation.hash(&mut hasher);
    command.sort_order.hash(&mut hasher);
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
    command_editor: Option<QuickCommandEditorDraft>,
    category_editor: Option<QuickCommandCategoryDraft>,
    pending_execution: Option<QuickCommandExecutionDraft>,
    last_persist_error: Option<String>,
    visible_commands: Arc<Vec<QuickCommand>>,
    pinned: bool,
    managing: bool,
    list_state: gpui::ListState,
}

impl TerminalQuickCommandsState {
    fn visible_commands_for_targets(
        &self,
        target_fields: &[String],
        protocol: Option<QuickCommandTargetProtocol>,
    ) -> Vec<QuickCommand> {
        self.store
            .visible_commands_for_targets(target_fields)
            .into_iter()
            .filter(|command| {
                command.availability.protocols.is_empty()
                    || protocol
                        .is_some_and(|protocol| command.availability.protocols.contains(&protocol))
            })
            .collect()
    }

    pub(in crate::workspace) fn quick_bar_snapshot(
        &self,
        target_fields: &[String],
        protocol: Option<QuickCommandTargetProtocol>,
    ) -> (Vec<QuickCommandCategory>, Vec<QuickCommand>) {
        // QuickBar is a read-only projection of the existing persisted store.
        // Preserve category and command order instead of creating a second model.
        (
            self.store.categories.clone(),
            self.store
                .commands
                .iter()
                .filter(|command| {
                    oxideterm_quick_commands::match_quick_command_host_patterns(
                        &command.availability.host_patterns,
                        target_fields,
                    ) && (command.availability.protocols.is_empty()
                        || protocol.is_some_and(|protocol| {
                            command.availability.protocols.contains(&protocol)
                        }))
                })
                .cloned()
                .collect(),
        )
    }

    pub(in crate::workspace) fn is_open(&self) -> bool {
        self.open
    }

    pub(in crate::workspace) fn manager_open(&self) -> bool {
        self.manager_open
    }

    pub(in crate::workspace) fn has_open_or_pending(&self) -> bool {
        self.open || self.pending_execution.is_some()
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
        self.pending_execution = None;
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
        self.pending_execution = None;
        self.open = self.pinned;
    }

    pub(in crate::workspace) fn request_execution(&mut self, command: QuickCommand) {
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

    fn cancel_execution(&mut self) -> bool {
        self.pending_execution.take().is_some()
    }

    fn prepare_insertion(&mut self, command: String, keep_open: bool) -> String {
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

    fn toggle_pinned(&mut self) {
        self.pinned = !self.pinned;
    }

    fn open_manager(&mut self) {
        // The manager is a workspace modal with an independent lifecycle, so
        // opening it must release every command-bar popover state first.
        self.open = false;
        self.pinned = false;
        self.pending_execution = None;
        self.manager_open = true;
        self.store.command_editor = None;
        self.store.category_editor = None;
        self.store.focused_input = Some(QuickCommandInput::Search);
        self.store.highlighted_command = None;
    }

    fn close_manager(&mut self) -> bool {
        let changed = self.manager_open
            || self.store.command_editor.is_some()
            || self.store.category_editor.is_some()
            || self.store.focused_input.is_some();
        self.manager_open = false;
        self.store.command_editor = None;
        self.store.category_editor = None;
        self.store.focused_input = None;
        self.store.highlighted_command = None;
        changed
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

    fn move_highlight(
        &mut self,
        target_fields: &[String],
        protocol: Option<QuickCommandTargetProtocol>,
        direction: QuickCommandKeyDirection,
    ) {
        let visible_commands = self.visible_commands_for_targets(target_fields, protocol);
        self.store.highlighted_command = quick_command_keyboard_highlight(
            &visible_commands,
            self.store.highlighted_command.as_deref(),
            direction,
        );
    }

    fn highlight_edge(
        &mut self,
        target_fields: &[String],
        protocol: Option<QuickCommandTargetProtocol>,
        end: bool,
    ) {
        let visible_commands = self.visible_commands_for_targets(target_fields, protocol);
        self.store.highlighted_command = if end {
            visible_commands.last().map(|command| command.id.clone())
        } else {
            quick_command_highlight_at(&visible_commands, 0)
        };
    }

    fn prepare_highlighted_insertion(
        &mut self,
        target_fields: &[String],
        protocol: Option<QuickCommandTargetProtocol>,
    ) -> Option<String> {
        let visible_commands = self.visible_commands_for_targets(target_fields, protocol);
        let selected_index = quick_command_highlighted_index(
            &visible_commands,
            self.store.highlighted_command.as_deref(),
        )
        .unwrap_or(0);
        let command = visible_commands.get(selected_index)?.command.clone();
        Some(self.prepare_insertion(command, self.pinned))
    }

    fn cycle_editor_focus(&mut self, input: QuickCommandInput, forward: bool) -> bool {
        let mut fields = Vec::new();
        if self.store.category_editor.is_some() {
            fields.push(QuickCommandInput::CategoryName);
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

    fn blur_input(&mut self) -> bool {
        let was_focused = self.store.focused_input.take().is_some();
        was_focused
    }

    fn start_command_create(&mut self) {
        self.store.category_editor = None;
        self.manager_open = true;
        self.store.command_editor = Some(QuickCommandEditorDraft {
            id: None,
            name: String::new(),
            command: String::new(),
            category: self.store.active_category.clone(),
            description: String::new(),
            host_patterns: String::new(),
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

    fn start_command_edit(&mut self, command: QuickCommand) {
        self.store.category_editor = None;
        self.manager_open = true;
        self.store.command_editor = Some(QuickCommandEditorDraft {
            id: Some(command.id),
            name: command.name,
            command: command.command,
            category: command.category,
            description: command.description.unwrap_or_default(),
            host_patterns: command.availability.host_patterns.join(", "),
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

    fn add_command_parameter(&mut self) {
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

    fn remove_command_parameter(&mut self, index: usize) {
        if let Some(draft) = self.store.command_editor.as_mut()
            && index < draft.parameters.len()
        {
            draft.parameters.remove(index);
            self.store.focused_input = None;
        }
    }

    fn set_command_parameter_kind(&mut self, index: usize, kind: QuickCommandParameterKind) {
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

    fn toggle_command_parameter_required(&mut self, index: usize) {
        if let Some(parameter) = self
            .store
            .command_editor
            .as_mut()
            .and_then(|draft| draft.parameters.get_mut(index))
        {
            parameter.required = !parameter.required;
        }
    }

    fn toggle_command_protocol(&mut self, protocol: QuickCommandTargetProtocol) {
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

    fn toggle_command_confirmation(&mut self) {
        if let Some(draft) = self.store.command_editor.as_mut() {
            draft.confirmation = match draft.confirmation {
                QuickCommandConfirmationPolicy::Inherit => QuickCommandConfirmationPolicy::Always,
                QuickCommandConfirmationPolicy::Always => QuickCommandConfirmationPolicy::Inherit,
            };
        }
    }

    fn set_execution_parameter_value(&mut self, index: usize, value: String) {
        if let Some(value_slot) = self
            .pending_execution
            .as_mut()
            .and_then(|execution| execution.parameter_values.get_mut(index))
        {
            *value_slot = Zeroizing::new(value);
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

    fn render_snapshot(
        &self,
        target_fields: &[String],
        protocol: Option<QuickCommandTargetProtocol>,
    ) -> QuickCommandsRenderSnapshot {
        let visible_commands = Arc::new(if self.manager_open {
            self.store.visible_commands_for_management()
        } else {
            self.visible_commands_for_targets(target_fields, protocol)
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
            last_persist_error: self.store.last_persist_error.clone(),
            visible_commands,
            pinned: self.pinned,
            managing: self.manager_open,
            list_state: self.list_state.clone(),
        }
    }
}

impl WorkspaceApp {
    fn quick_commands_render_snapshot(
        &self,
        cx: &mut Context<Self>,
    ) -> QuickCommandsRenderSnapshot {
        let target_fields = self.terminal_command_context(cx).target_fields();
        let protocol = self
            .active_pane_id(cx)
            .and_then(|pane_id| self.quick_command_context_for_pane(pane_id, cx))
            .map(|context| context.protocol);
        self.terminal
            .read(cx)
            .quick_commands
            .render_snapshot(&target_fields, protocol)
    }

    pub(in crate::workspace) fn close_terminal_quick_commands_popover(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        self.terminal
            .update(cx, |terminal, _cx| terminal.quick_commands.close())
    }

    pub(in crate::workspace) fn open_quick_commands_manager(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        self.prepare_modal_interaction_boundary(cx);
        self.terminal
            .update(cx, |terminal, _cx| terminal.quick_commands.open_manager());
        self.ime_marked_text = None;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn close_quick_commands_manager(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let changed = self
            .terminal
            .update(cx, |terminal, _cx| terminal.quick_commands.close_manager());
        if changed {
            self.ime_marked_text = None;
            self.clear_ime_selection();
            cx.notify();
        }
        changed
    }

    pub(in crate::workspace) fn finish_terminal_quick_command_execution(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.terminal.update(cx, |terminal, _cx| {
            terminal.quick_commands.finish_execution()
        });
    }

    fn cancel_terminal_quick_command_execution(&mut self, cx: &mut Context<Self>) {
        if self.terminal.update(cx, |terminal, _cx| {
            terminal.quick_commands.cancel_execution()
        }) {
            cx.notify();
        }
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
        self.replace_terminal_command_sender_text(command, cx);
    }

    pub(in crate::workspace) fn handle_quick_commands_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(input) = self.terminal.read(cx).quick_commands.focused_input() else {
            return;
        };
        let target_fields = self.terminal_command_context(cx).target_fields();
        let protocol = self
            .active_pane_id(cx)
            .and_then(|pane_id| self.quick_command_context_for_pane(pane_id, cx))
            .map(|context| context.protocol);
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        if input == QuickCommandInput::Search {
            match key {
                "escape" if !modifiers.platform && !modifiers.control => {
                    if self.terminal.read(cx).quick_commands.manager_open() {
                        self.terminal
                            .update(cx, |terminal, _cx| terminal.quick_commands.blur_input());
                    } else {
                        // The compact launcher follows browser popover dismissal,
                        // while the workspace manager keeps its own modal lifecycle.
                        self.close_terminal_quick_commands_popover(cx);
                    }
                    self.ime_marked_text = None;
                    cx.notify();
                    return;
                }
                "arrowdown" | "down" if !modifiers.platform && !modifiers.control => {
                    self.terminal.update(cx, |terminal, _cx| {
                        terminal.quick_commands.move_highlight(
                            &target_fields,
                            protocol,
                            QuickCommandKeyDirection::Next,
                        )
                    });
                    cx.notify();
                    return;
                }
                "arrowup" | "up" if !modifiers.platform && !modifiers.control => {
                    self.terminal.update(cx, |terminal, _cx| {
                        terminal.quick_commands.move_highlight(
                            &target_fields,
                            protocol,
                            QuickCommandKeyDirection::Previous,
                        )
                    });
                    cx.notify();
                    return;
                }
                "home" if !modifiers.platform && !modifiers.control => {
                    self.terminal.update(cx, |terminal, _cx| {
                        terminal
                            .quick_commands
                            .highlight_edge(&target_fields, protocol, false)
                    });
                    cx.notify();
                    return;
                }
                "end" if !modifiers.platform && !modifiers.control => {
                    self.terminal.update(cx, |terminal, _cx| {
                        terminal
                            .quick_commands
                            .highlight_edge(&target_fields, protocol, true)
                    });
                    cx.notify();
                    return;
                }
                "enter" if !modifiers.platform && !modifiers.control => {
                    let command = self.terminal.update(cx, |terminal, _cx| {
                        terminal
                            .quick_commands
                            .prepare_highlighted_insertion(&target_fields, protocol)
                    });
                    if let Some(command) = command {
                        self.replace_terminal_command_sender_text(command, cx);
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
        let max_width = QUICK_COMMANDS_POPOVER_MAX_WIDTH;
        let popover_width = self
            .select_anchors
            .get(&SelectAnchorId::TerminalCommandBar)
            .map(|anchor| quick_commands_popover_width_for_bar(f32::from(anchor.bounds.size.width)))
            .unwrap_or(max_width)
            .min(max_width);
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
        .max_w(px(max_width))
        .text_size(px(self.tokens.metrics.ui_text_sm))
        .font_family(settings_ui_font_family(
            &self.settings_store.settings().appearance.ui_font_family,
        ))
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

    pub(in crate::workspace) fn render_quick_commands_manager_modal(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let snapshot = self.quick_commands_render_snapshot(cx);
        let theme = self.tokens.ui;
        let manager = oxideterm_gpui_ui::modal_container(&self.tokens)
            .w(px(QUICK_COMMANDS_MANAGER_WIDTH))
            .max_w_full()
            .h(px(QUICK_COMMANDS_MANAGER_HEIGHT))
            .max_h_full()
            .shadow(oxideterm_gpui_ui::theme_overlay_shadow(&self.tokens))
            .flex()
            .flex_col()
            .font_family(settings_ui_font_family(
                &self.settings_store.settings().appearance.ui_font_family,
            ))
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .child(
                div()
                    .h(px(54.0))
                    .flex_none()
                    .px(px(16.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(Self::render_lucide_icon(
                                LucideIcon::Zap,
                                16.0,
                                rgb(theme.accent),
                            ))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_base))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(theme.text))
                                    .child(self.i18n.t("terminal.quick_commands.title")),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .child(self.quick_command_tooltip_icon_button(
                                LucideIcon::Upload,
                                "quick-commands-import",
                                self.i18n.t("terminal.quick_commands.import"),
                                |this, _event, _window, cx| {
                                    this.import_quick_commands(cx);
                                    cx.stop_propagation();
                                },
                                cx,
                            ))
                            .child(self.quick_command_tooltip_icon_button(
                                LucideIcon::Download,
                                "quick-commands-export",
                                self.i18n.t("terminal.quick_commands.export"),
                                |this, _event, _window, cx| {
                                    this.export_quick_commands(cx);
                                    cx.stop_propagation();
                                },
                                cx,
                            ))
                            .child(self.quick_command_icon_button(
                                LucideIcon::X,
                                |this, _event, _window, cx| {
                                    this.close_quick_commands_manager(cx);
                                    cx.stop_propagation();
                                },
                                cx,
                            )),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .overflow_hidden()
                    .border_t_1()
                    .border_color(rgba((theme.border << 8) | 0x99))
                    .child(self.render_quick_command_category_sidebar(&snapshot, cx))
                    .child(self.render_quick_command_manager_list(&snapshot, cx))
                    .child(self.render_quick_command_manager_inspector(&snapshot, cx)),
            );

        dismissible_dialog_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.close_quick_commands_manager(cx);
                    cx.stop_propagation();
                }),
            )
            .child(overlay_content_boundary(manager))
            .into_any_element()
    }

    fn export_quick_commands(&mut self, cx: &mut Context<Self>) {
        let export = self
            .terminal
            .read(cx)
            .quick_commands
            .store
            .export_snapshot_json();
        let Ok(snapshot_json) = export else {
            self.push_workspace_notice(
                TerminalNotice {
                    title: self.i18n.t("terminal.quick_commands.export_failed"),
                    description: None,
                    status_text: None,
                    progress: None,
                    variant: TerminalNoticeVariant::Error,
                },
                cx,
            );
            return;
        };
        // Command bodies may contain user-entered sensitive literals during explicit export.
        let snapshot_json = Zeroizing::new(snapshot_json);
        let receiver = cx.prompt_for_new_path(
            &quick_commands_export_directory(),
            Some("oxideterm-quick-commands.json"),
        );
        cx.spawn(async move |weak, cx| {
            let result = match receiver.await {
                Ok(Ok(Some(path))) => {
                    oxideterm_atomic_file::durable_write(&path, snapshot_json.as_bytes())
                        .map(|()| Some(path))
                        .map_err(|_| ())
                }
                Ok(Ok(None)) => Ok(None),
                Ok(Err(_)) | Err(_) => Err(()),
            };
            let _ = weak.update(cx, |this, cx| match result {
                Ok(Some(path)) => this.push_workspace_notice(
                    TerminalNotice {
                        title: this.i18n.t("terminal.quick_commands.export_success"),
                        description: Some(path.to_string_lossy().to_string()),
                        status_text: None,
                        progress: None,
                        variant: TerminalNoticeVariant::Success,
                    },
                    cx,
                ),
                Ok(None) => {}
                Err(()) => this.push_workspace_notice(
                    TerminalNotice {
                        title: this.i18n.t("terminal.quick_commands.export_failed"),
                        description: None,
                        status_text: None,
                        progress: None,
                        variant: TerminalNoticeVariant::Error,
                    },
                    cx,
                ),
            });
        })
        .detach();
    }

    fn import_quick_commands(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(SharedString::from(
                self.i18n.t("terminal.quick_commands.import"),
            )),
        });
        cx.spawn(async move |weak, cx| {
            let selected_path = match receiver.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) => None,
                Ok(Err(_)) | Err(_) => {
                    let _ = weak.update(cx, |this, cx| {
                        this.push_workspace_notice(
                            TerminalNotice {
                                title: this.i18n.t("terminal.quick_commands.import_failed"),
                                description: None,
                                status_text: None,
                                progress: None,
                                variant: TerminalNoticeVariant::Error,
                            },
                            cx,
                        );
                    });
                    return;
                }
            };
            let Some(path) = selected_path else {
                return;
            };
            let snapshot_json = fs::metadata(&path)
                .map_err(|_| ())
                .and_then(|metadata| {
                    (metadata.len() <= oxideterm_quick_commands::MAX_QUICK_COMMANDS_FILE_BYTES)
                        .then_some(())
                        .ok_or(())
                })
                .and_then(|()| {
                    fs::read_to_string(&path)
                        .map(Zeroizing::new)
                        .map_err(|_| ())
                });
            let _ = weak.update(cx, |this, cx| {
                let Ok(snapshot_json) = snapshot_json else {
                    this.push_workspace_notice(
                        TerminalNotice {
                            title: this.i18n.t("terminal.quick_commands.import_failed"),
                            description: None,
                            status_text: None,
                            progress: None,
                            variant: TerminalNoticeVariant::Error,
                        },
                        cx,
                    );
                    return;
                };
                let result = this.terminal.update(cx, |terminal, _cx| {
                    // Desktop import keeps every existing record and renames conflicts.
                    terminal
                        .quick_commands
                        .store
                        .apply_snapshot_json(&snapshot_json, QuickCommandImportStrategy::Rename)
                });
                if result.errors.is_empty() {
                    let title = this
                        .i18n
                        .t("terminal.quick_commands.import_success")
                        .replace("{{count}}", &result.imported.to_string());
                    this.push_workspace_notice(
                        TerminalNotice {
                            title,
                            description: None,
                            status_text: None,
                            progress: None,
                            variant: TerminalNoticeVariant::Success,
                        },
                        cx,
                    );
                    cx.notify();
                } else {
                    this.push_workspace_notice(
                        TerminalNotice {
                            title: this.i18n.t("terminal.quick_commands.import_failed"),
                            description: None,
                            status_text: None,
                            progress: None,
                            variant: TerminalNoticeVariant::Error,
                        },
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    fn render_quick_command_manager_list(
        &self,
        snapshot: &QuickCommandsRenderSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .w(px(QUICK_COMMANDS_MANAGER_COMMAND_LIST_WIDTH))
            .h_full()
            .flex_none()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(rgba((theme.border << 8) | 0x99))
            .child(self.render_quick_command_toolbar(snapshot, cx))
            .child(self.render_quick_command_rows(snapshot, cx))
            .into_any_element()
    }

    fn render_quick_command_manager_inspector(
        &self,
        snapshot: &QuickCommandsRenderSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let content = if snapshot.category_editor.is_some() {
            self.render_quick_command_category_editor(snapshot, cx)
        } else if snapshot.command_editor.is_some() {
            self.render_quick_command_editor(snapshot, cx)
        } else {
            div()
                .h_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(10.0))
                .text_color(rgb(theme.text_muted))
                .child(Self::render_lucide_icon(
                    LucideIcon::Pencil,
                    22.0,
                    rgb(theme.text_muted),
                ))
                .child(
                    div()
                        .max_w(px(280.0))
                        .text_center()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .child(self.i18n.t("terminal.quick_commands.manager_empty")),
                )
                .into_any_element()
        };
        div()
            .flex_1()
            .h_full()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .bg(rgba((theme.bg << 8) | 0x40))
            .child(content)
            .into_any_element()
    }

    fn render_quick_command_category_sidebar(
        &self,
        snapshot: &QuickCommandsRenderSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let sidebar = div()
            .w(px(if snapshot.managing { 220.0 } else { 160.0 }))
            .h_full()
            .flex_none()
            .overflow_hidden()
            .when(!snapshot.managing, |sidebar| {
                sidebar.rounded_l(px(rounded_shell_child_radius(self.tokens.radii.lg)))
            })
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
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text_muted))
                            .child(
                                self.render_display_text_with_role(
                                    SelectableTextRole::PlainDocument,
                                    "quick-commands",
                                    "title",
                                    self.i18n
                                        .t(if snapshot.managing {
                                            "terminal.quick_commands.groups"
                                        } else {
                                            "terminal.quick_commands.title"
                                        })
                                        .to_uppercase(),
                                    theme.text_muted,
                                    cx,
                                ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .when(!snapshot.managing, |actions| {
                                actions.child(self.quick_command_pin_button(
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
                            })
                            .when(snapshot.managing, |actions| {
                                actions.child(self.quick_command_icon_button(
                                    LucideIcon::Plus,
                                    |this, _event, _window, cx| {
                                        this.start_quick_command_category_create(cx);
                                        cx.stop_propagation();
                                    },
                                    cx,
                                ))
                            })
                            .when(!snapshot.managing, |actions| {
                                actions.child(self.quick_command_icon_button(
                                    LucideIcon::X,
                                    |this, _event, _window, cx| {
                                        this.close_terminal_quick_commands_popover(cx);
                                        cx.stop_propagation();
                                        cx.notify();
                                    },
                                    cx,
                                ))
                            }),
                    ),
            );

        let mut category_list = div().flex().flex_col().gap(px(6.0));
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
            category_list = category_list.child(
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
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
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
                    .when(snapshot.managing, |row| {
                        row.child(self.quick_command_mini_button(
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
                    })
                    .when(snapshot.managing && can_delete, |row| {
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
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scrollbar()
                    .child(category_list),
            )
            .when(snapshot.last_persist_error.is_some(), |sidebar| {
                sidebar.child(
                    div()
                        .rounded(px(self.tokens.radii.md))
                        .border_1()
                        .border_color(rgba(0xef444480))
                        .bg(rgba(0xef44441a))
                        .p(px(6.0))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgba(0xfca5a5ff))
                        .child(self.i18n.t("terminal.quick_commands.persist_failed")),
                )
            })
            .into_any_element()
    }

    fn render_quick_command_body(
        &self,
        snapshot: &QuickCommandsRenderSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let body = div()
            .flex_1()
            .h_full()
            .min_w(px(0.0))
            .overflow_hidden()
            .when(!snapshot.managing, |body| {
                body.rounded_r(px(rounded_shell_child_radius(self.tokens.radii.lg)))
            })
            .flex()
            .flex_col()
            .child(self.render_quick_command_toolbar(snapshot, cx));
        if let Some(execution) = snapshot.pending_execution.as_ref() {
            return body
                .child(self.render_quick_command_execution(execution, snapshot, cx))
                .into_any_element();
        }

        body.when_some(snapshot.category_editor.as_ref(), |body, _| {
            body.child(self.render_quick_command_category_editor(snapshot, cx))
        })
        .when_some(snapshot.command_editor.as_ref(), |body, _| {
            body.child(self.render_quick_command_editor(snapshot, cx))
        })
        .child(self.render_quick_command_rows(snapshot, cx))
        .into_any_element()
    }

    fn render_quick_command_toolbar(
        &self,
        snapshot: &QuickCommandsRenderSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .border_b_1()
            .border_color(rgba((theme.border << 8) | 0x99))
            .p(px(8.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(self.render_quick_command_text_input(
                        QuickCommandInput::Search,
                        snapshot.query.clone(),
                        snapshot.focused_input,
                        self.i18n.t("terminal.quick_commands.search_placeholder"),
                        cx,
                    )),
            )
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
                    .hover(move |style| style.bg(rgb(theme.bg_hover)).text_color(rgb(theme.text)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, window, cx| {
                            if this.terminal.read(cx).quick_commands.manager_open() {
                                this.start_quick_command_create(cx);
                            } else {
                                this.open_quick_commands_manager(window, cx);
                            }
                            cx.stop_propagation();
                        }),
                    )
                    .child(Self::render_lucide_icon(
                        if snapshot.managing {
                            LucideIcon::Plus
                        } else {
                            LucideIcon::Settings
                        },
                        14.0,
                        rgb(theme.text_muted),
                    ))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "quick-command-add-button",
                        "label",
                        self.i18n.t(if snapshot.managing {
                            "terminal.quick_commands.add"
                        } else {
                            "terminal.quick_commands.manage"
                        }),
                        theme.text_muted,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn render_quick_command_execution(
        &self,
        execution: &QuickCommandExecutionDraft,
        snapshot: &QuickCommandsRenderSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let parameter_values = execution
            .command
            .parameters
            .iter()
            .zip(&execution.parameter_values)
            .map(|(parameter, value)| (parameter.name.clone(), value.clone()))
            .collect::<std::collections::BTreeMap<_, _>>();
        let target_contexts = self.quick_command_target_contexts(cx);
        let contexts = target_contexts
            .iter()
            .map(|(_, context)| context.clone())
            .collect::<Vec<_>>();
        let prepared = oxideterm_quick_commands::prepare_quick_command(
            &execution.command,
            &contexts,
            &parameter_values,
        );
        let can_run = prepared
            .as_ref()
            .is_ok_and(|prepared| !prepared.targets.is_empty());
        let mut parameters = div().flex().flex_col().gap(px(8.0));
        for (index, parameter) in execution.command.parameters.iter().enumerate() {
            let value = execution
                .parameter_values
                .get(index)
                .cloned()
                .unwrap_or_default();
            let input = if parameter.kind == QuickCommandParameterKind::Secret {
                self.render_quick_command_secret_input(
                    QuickCommandInput::ParameterDefault(index),
                    &value,
                    snapshot.focused_input,
                    parameter.label.clone(),
                    cx,
                )
            } else {
                self.render_quick_command_text_input(
                    QuickCommandInput::ParameterDefault(index),
                    value.to_string(),
                    snapshot.focused_input,
                    parameter.label.clone(),
                    cx,
                )
            };
            let mut field = div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(theme.text_muted))
                        .child(parameter.label.clone()),
                )
                .child(input);
            if parameter.kind == QuickCommandParameterKind::Choice {
                let mut choices = div().flex().items_center().gap(px(4.0)).flex_wrap();
                for choice in &parameter.choices {
                    let choice_for_click = choice.clone();
                    let selected = choice == value.as_str();
                    choices = choices.child(
                        self.quick_command_text_button(
                            choice.clone(),
                            true,
                            cx.listener(move |this, _event, _window, cx| {
                                this.terminal.update(cx, |terminal, _cx| {
                                    terminal.quick_commands.set_execution_parameter_value(
                                        index,
                                        choice_for_click.clone(),
                                    )
                                });
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )
                        .border_color(if selected {
                            rgb(theme.accent)
                        } else {
                            rgba((theme.border << 8) | 0x99)
                        }),
                    );
                }
                field = field.child(choices);
            }
            parameters = parameters.child(field);
        }
        let contains_secret_values = execution
            .command
            .parameters
            .iter()
            .zip(&execution.parameter_values)
            .any(|(parameter, value)| {
                parameter.kind == QuickCommandParameterKind::Secret && !value.is_empty()
            });
        let preview = match &prepared {
            Ok(prepared) => {
                let mut rows = div().flex().flex_col().gap(px(6.0));
                for target in &prepared.targets {
                    let risk = target.risk;
                    rows = rows.child(
                        div()
                            .rounded(px(self.tokens.radii.md))
                            .border_1()
                            .border_color(rgba((theme.border << 8) | 0x99))
                            .p(px(7.0))
                            .flex()
                            .flex_col()
                            .gap(px(3.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(
                                        div()
                                            .text_size(px(self.tokens.metrics.ui_text_sm))
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(rgb(theme.text))
                                            .child(target.label.clone()),
                                    )
                                    .when_some(risk, |row, risk| {
                                        row.child(status_pill(
                                            &self.tokens,
                                            quick_command_risk_label(&self.i18n, risk)
                                                .to_uppercase(),
                                            StatusPillOptions::new(quick_command_risk_tone(risk))
                                                .compact(),
                                        ))
                                    }),
                            )
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .font_family(settings_mono_font_family(
                                        self.settings_store.settings(),
                                    ))
                                    .text_color(rgb(theme.accent))
                                    .child(if contains_secret_values {
                                        self.i18n
                                            .t("terminal.quick_commands.preview_contains_secrets")
                                    } else {
                                        // GPUI owns one frame-local display copy; the runtime
                                        // source remains zeroizing and is cleared on close.
                                        target.command.to_string()
                                    }),
                            ),
                    );
                }
                if !prepared.unavailable_targets.is_empty() {
                    rows = rows.child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.warning))
                            .child(
                                self.i18n
                                    .t("terminal.quick_commands.unavailable_targets")
                                    .replace(
                                        "{{targets}}",
                                        &prepared.unavailable_targets.join(", "),
                                    ),
                            ),
                    );
                }
                rows.into_any_element()
            }
            Err(errors) => div()
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(rgba((theme.error << 8) | 0x99))
                .p(px(7.0))
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .text_color(rgb(theme.error))
                .child(
                    errors
                        .iter()
                        .map(|error| quick_command_template_error_label(&self.i18n, error))
                        .collect::<Vec<_>>()
                        .join("; "),
                )
                .into_any_element(),
        };
        div()
            .flex_1()
            .min_h(px(0.0))
            .p(px(12.0))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(theme.text))
                    .child(
                        self.i18n
                            .t("terminal.quick_commands.execution_title")
                            .replace("{{name}}", &execution.command.name),
                    ),
            )
            .when(!execution.command.parameters.is_empty(), |body| {
                body.child(parameters)
            })
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t("terminal.quick_commands.preview")),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scrollbar()
                    .child(preview),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(self.quick_command_text_button(
                        self.i18n.t("terminal.quick_commands.cancel"),
                        true,
                        cx.listener(|this, _event, _window, cx| {
                            this.cancel_terminal_quick_command_execution(cx);
                            cx.stop_propagation();
                        }),
                    ))
                    .child(
                        self.quick_command_text_button(
                            self.i18n.t("terminal.quick_commands.run"),
                            can_run,
                            cx.listener(|this, _event, window, cx| {
                                this.confirm_quick_command_execution(window, cx);
                                cx.stop_propagation();
                            }),
                        )
                        .bg(if can_run {
                            rgba((theme.accent << 8) | 0x26)
                        } else {
                            rgba(0x00000000)
                        }),
                    ),
            )
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
        let managing = snapshot.managing;
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
                            managing,
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
        managing: bool,
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
            .child(self.render_quick_command_row(
                command,
                pinned,
                managing,
                highlighted_command,
                cx,
            ))
            .into_any_element()
    }

    fn render_quick_command_row(
        &self,
        command: QuickCommand,
        pinned: bool,
        managing: bool,
        highlighted_command: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let risk = quick_command_risk_badge(&command.command);
        let command_for_insert = command.command.clone();
        let command_for_run = command.clone();
        let command_for_primary_action = command.clone();
        let command_id_for_move_up = command.id.clone();
        let command_id_for_move_down = command.id.clone();
        let command_id_for_delete = command.id.clone();
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
                            if managing {
                                this.start_quick_command_edit(
                                    command_for_primary_action.clone(),
                                    cx,
                                );
                            } else {
                                this.insert_quick_command_into_command_bar(
                                    command_for_insert.clone(),
                                    keep_open_for_insert,
                                    cx,
                                );
                            }
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
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
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
                                        quick_command_risk_badge_label(&self.i18n, risk)
                                            .to_uppercase(),
                                        StatusPillOptions::new(quick_command_risk_badge_tone(risk))
                                            .compact()
                                            .strong(),
                                    ),
                                )
                            })
                            .when_some(
                                (!command.availability.host_patterns.is_empty())
                                    .then(|| command.availability.host_patterns.join(", ")),
                                |row, pattern| {
                                row.child(
                                    status_pill(
                                        &self.tokens,
                                        pattern,
                                        StatusPillOptions::new(StatusTone::Neutral).compact(),
                                    )
                                    .font_family(settings_mono_font_family(
                                        self.settings_store.settings(),
                                    )),
                                )
                            }),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .font_family(settings_mono_font_family(
                                self.settings_store.settings(),
                            ))
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
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgba((theme.text_muted << 8) | 0xcc))
                                .child(self.render_row_safe_selectable_display_text_in_group_with_alpha(
                                    selection_group_id,
                                    "quick-command-row-cell",
                                    ("description", command.id.as_str()),
                                    2,
                                    description.clone(),
                                    theme.text_muted,
                                    0xcc as f32 / 255.0,
                                    None,
                                    cx,
                                )),
                        )
                    }),
            )
            .child(self.quick_command_action_button(
                LucideIcon::Play,
                move |this, _event, window, cx| {
                    this.run_quick_command_model(&command_for_run, window, cx);
                    cx.stop_propagation();
                },
                cx,
            ))
            .when(managing, |row| {
                row.child(self.quick_command_action_button(
                    LucideIcon::ArrowUp,
                    move |this, _event, _window, cx| {
                        this.terminal.update(cx, |terminal, _cx| {
                            terminal
                                .quick_commands
                                .store
                                .move_command(&command_id_for_move_up, -1)
                        });
                        cx.stop_propagation();
                        cx.notify();
                    },
                    cx,
                ))
                .child(self.quick_command_action_button(
                    LucideIcon::ArrowDown,
                    move |this, _event, _window, cx| {
                        this.terminal.update(cx, |terminal, _cx| {
                            terminal
                                .quick_commands
                                .store
                                .move_command(&command_id_for_move_down, 1)
                        });
                        cx.stop_propagation();
                        cx.notify();
                    },
                    cx,
                ))
                .child(self.quick_command_action_button(
                    LucideIcon::Trash2,
                    move |this, _event, _window, cx| {
                        this.terminal.update(cx, |terminal, _cx| {
                            terminal.quick_commands.delete_command(&command_id_for_delete)
                        });
                        cx.stop_propagation();
                        cx.notify();
                    },
                    cx,
                ))
            })
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
            .bg(rgba((theme.bg << 8) | 0x59))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .when(snapshot.managing, |editor| {
                editor.h_full().min_h(px(0.0)).p(px(16.0))
            })
            .when(!snapshot.managing, |editor| {
                editor
                    .border_b_1()
                    .border_color(rgba((theme.border << 8) | 0x99))
                    .p(px(8.0))
            })
            .overflow_y_scrollbar()
            .when(snapshot.managing, |editor| {
                editor.child(
                    div()
                        .mb(px(4.0))
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(theme.text))
                        .child(self.i18n.t(if draft.id.is_some() {
                            "terminal.quick_commands.edit_group"
                        } else {
                            "terminal.quick_commands.add_group"
                        })),
                )
            })
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
        let can_save = quick_command_editor_can_save(draft);
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

        let mut protocols = div().flex().items_center().gap(px(4.0)).flex_wrap();
        for (protocol, label_key) in [
            (
                QuickCommandTargetProtocol::Local,
                "terminal.quick_commands.protocol_local",
            ),
            (
                QuickCommandTargetProtocol::Ssh,
                "terminal.quick_commands.protocol_ssh",
            ),
            (
                QuickCommandTargetProtocol::Mosh,
                "terminal.quick_commands.protocol_mosh",
            ),
            (
                QuickCommandTargetProtocol::Telnet,
                "terminal.quick_commands.protocol_telnet",
            ),
            (
                QuickCommandTargetProtocol::Serial,
                "terminal.quick_commands.protocol_serial",
            ),
            (
                QuickCommandTargetProtocol::Tmux,
                "terminal.quick_commands.protocol_tmux",
            ),
        ] {
            let active = draft.protocols.contains(&protocol);
            protocols = protocols.child(
                self.quick_command_text_button(
                    self.i18n.t(label_key),
                    true,
                    cx.listener(move |this, _event, _window, cx| {
                        this.terminal.update(cx, |terminal, _cx| {
                            terminal.quick_commands.toggle_command_protocol(protocol)
                        });
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .border_color(if active {
                    rgb(theme.accent)
                } else {
                    rgba((theme.border << 8) | 0x99)
                })
                .bg(if active {
                    rgba((theme.accent << 8) | 0x1a)
                } else {
                    rgba(0x00000000)
                }),
            );
        }
        let parameters = self.render_quick_command_parameter_editor(draft, snapshot, cx);
        let confirmation_always = draft.confirmation == QuickCommandConfirmationPolicy::Always;

        div()
            .bg(rgba((theme.bg << 8) | 0x59))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .when(snapshot.managing, |editor| {
                editor.h_full().min_h(px(0.0)).p(px(16.0))
            })
            .when(!snapshot.managing, |editor| {
                editor
                    .border_b_1()
                    .border_color(rgba((theme.border << 8) | 0x99))
                    .p(px(8.0))
                    .max_h(px(420.0))
            })
            .overflow_y_scrollbar()
            .when(snapshot.managing, |editor| {
                editor.child(
                    div()
                        .mb(px(4.0))
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(theme.text))
                        .child(self.i18n.t(if draft.id.is_some() {
                            "terminal.quick_commands.edit_command"
                        } else {
                            "terminal.quick_commands.add_command"
                        })),
                )
            })
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
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("terminal.quick_commands.template_hint")),
                    )
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
                            draft.host_patterns.clone(),
                            snapshot.focused_input,
                            self.i18n
                                .t("terminal.quick_commands.host_pattern_placeholder"),
                            cx,
                        ),
                    )
                    .child(
                        div()
                            .w_full()
                            .max_h(px(QUICK_COMMAND_CATEGORY_PICKER_MAX_HEIGHT))
                            .overflow_y_scrollbar()
                            .child(categories),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(5.0))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(theme.text_muted))
                                    .child(self.i18n.t("terminal.quick_commands.protocols")),
                            )
                            .child(protocols),
                    )
                    .child(parameters)
                    .child(
                        self.quick_command_text_button(
                            self.i18n.t(if confirmation_always {
                                "terminal.quick_commands.confirmation_always"
                            } else {
                                "terminal.quick_commands.confirmation_inherit"
                            }),
                            true,
                            cx.listener(|this, _event, _window, cx| {
                                this.terminal.update(cx, |terminal, _cx| {
                                    terminal.quick_commands.toggle_command_confirmation()
                                });
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )
                        .border_color(if confirmation_always {
                            rgb(theme.warning)
                        } else {
                            rgba((theme.border << 8) | 0x99)
                        }),
                    ),
            )
            .child(self.render_quick_editor_buttons(
                can_save,
                "terminal.quick_commands.save",
                |this, cx| this.save_quick_command_editor(cx),
                cx,
            ))
            .into_any_element()
    }

    fn render_quick_command_parameter_editor(
        &self,
        draft: &QuickCommandEditorDraft,
        snapshot: &QuickCommandsRenderSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mut rows = div().flex().flex_col().gap(px(6.0));
        for (index, parameter) in draft.parameters.iter().enumerate() {
            let kind = parameter.kind;
            let required = parameter.required;
            rows = rows.child(
                div()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgba((theme.border << 8) | 0x99))
                    .p(px(6.0))
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .flex()
                            .gap(px(6.0))
                            .child(div().flex_1().child(self.render_quick_command_text_input(
                                QuickCommandInput::ParameterName(index),
                                parameter.name.clone(),
                                snapshot.focused_input,
                                self.i18n.t("terminal.quick_commands.parameter_name"),
                                cx,
                            )))
                            .child(div().flex_1().child(self.render_quick_command_text_input(
                                QuickCommandInput::ParameterLabel(index),
                                parameter.label.clone(),
                                snapshot.focused_input,
                                self.i18n.t("terminal.quick_commands.parameter_label"),
                                cx,
                            ))),
                    )
                    .when(kind != QuickCommandParameterKind::Secret, |row| {
                        row.child(self.render_quick_command_text_input(
                            QuickCommandInput::ParameterDefault(index),
                            parameter.default_value.clone(),
                            snapshot.focused_input,
                            self.i18n.t("terminal.quick_commands.parameter_default"),
                            cx,
                        ))
                    })
                    .when(kind == QuickCommandParameterKind::Choice, |row| {
                        row.child(self.render_quick_command_text_input(
                            QuickCommandInput::ParameterChoices(index),
                            parameter.choices.clone(),
                            snapshot.focused_input,
                            self.i18n.t("terminal.quick_commands.parameter_choices"),
                            cx,
                        ))
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(self.quick_command_text_button(
                                self.i18n.t(match kind {
                                    QuickCommandParameterKind::Text => {
                                        "terminal.quick_commands.parameter_text"
                                    }
                                    QuickCommandParameterKind::Choice => {
                                        "terminal.quick_commands.parameter_choice"
                                    }
                                    QuickCommandParameterKind::Secret => {
                                        "terminal.quick_commands.parameter_secret"
                                    }
                                }),
                                true,
                                cx.listener(move |this, _event, _window, cx| {
                                    let next = match kind {
                                        QuickCommandParameterKind::Text => {
                                            QuickCommandParameterKind::Choice
                                        }
                                        QuickCommandParameterKind::Choice => {
                                            QuickCommandParameterKind::Secret
                                        }
                                        QuickCommandParameterKind::Secret => {
                                            QuickCommandParameterKind::Text
                                        }
                                    };
                                    this.terminal.update(cx, |terminal, _cx| {
                                        terminal
                                            .quick_commands
                                            .set_command_parameter_kind(index, next)
                                    });
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ))
                            .child(self.quick_command_text_button(
                                self.i18n.t(if required {
                                    "terminal.quick_commands.parameter_required"
                                } else {
                                    "terminal.quick_commands.parameter_optional"
                                }),
                                true,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.terminal.update(cx, |terminal, _cx| {
                                        terminal
                                            .quick_commands
                                            .toggle_command_parameter_required(index)
                                    });
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ))
                            .child(self.quick_command_icon_button(
                                LucideIcon::Trash2,
                                move |this, _event, _window, cx| {
                                    this.terminal.update(cx, |terminal, _cx| {
                                        terminal.quick_commands.remove_command_parameter(index)
                                    });
                                    cx.stop_propagation();
                                    cx.notify();
                                },
                                cx,
                            )),
                    ),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("terminal.quick_commands.parameters")),
                    )
                    .child(self.quick_command_text_button(
                        self.i18n.t("terminal.quick_commands.add_parameter"),
                        true,
                        cx.listener(|this, _event, _window, cx| {
                            this.terminal.update(cx, |terminal, _cx| {
                                terminal.quick_commands.add_command_parameter()
                            });
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )),
            )
            .child(rows)
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
        self.render_quick_command_input(input, &value, focused_input, placeholder, false, cx)
    }

    fn render_quick_command_secret_input(
        &self,
        input: QuickCommandInput,
        value: &Zeroizing<String>,
        focused_input: Option<QuickCommandInput>,
        placeholder: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_quick_command_input(input, value, focused_input, placeholder, true, cx)
    }

    fn render_quick_command_input(
        &self,
        input: QuickCommandInput,
        value: &str,
        focused_input: Option<QuickCommandInput>,
        placeholder: String,
        secret: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = focused_input == Some(input);
        let target = WorkspaceImeTarget::QuickCommand(input);
        let viewport = self.terminal.read(cx).quick_commands.input_viewport(input);
        let active_offset = self.ime_active_offset_for_target(target, cx);
        self.text_input_with_workspace_ime(
            target,
            text_input_with_viewport(
                &self.tokens,
                TextInputView {
                    value,
                    placeholder,
                    focused,
                    caret_visible: self.input_caret.visible(),
                    secret,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target, cx),
                    marked_text: self.marked_text_for_target(target, cx),
                },
                &viewport,
                active_offset,
            )
            .h(px(32.0))
            .when(quick_command_input_uses_monospace(input), |field| {
                field.font_family(settings_mono_font_family(self.settings_store.settings()))
            }),
            move |this, cx| {
                this.terminal.update(cx, |terminal, _cx| {
                    terminal.quick_commands.set_focused_input(input)
                });
            },
            cx,
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
        QuickCommandConfirmationPolicy, QuickCommandEditorDraft, QuickCommandParameterEditorDraft,
        QuickCommandParameterKind, QuickCommandRiskBadge, quick_command_editor_can_save,
        quick_command_risk_badge, quick_command_space_inserts_literal,
    };

    #[test]
    fn quick_command_plain_space_is_literal_text() {
        assert!(quick_command_space_inserts_literal(false, false, false));
        assert!(!quick_command_space_inserts_literal(true, false, false));
        assert!(!quick_command_space_inserts_literal(false, true, false));
        assert!(!quick_command_space_inserts_literal(false, false, true));
    }

    #[test]
    fn quick_command_editor_rejects_unknown_template_parameter() {
        let mut draft = QuickCommandEditorDraft {
            id: None,
            name: "Deploy".to_string(),
            command: "deploy {{param.sevrice}}".to_string(),
            category: "custom".to_string(),
            description: String::new(),
            host_patterns: String::new(),
            parameters: vec![QuickCommandParameterEditorDraft {
                name: "service".to_string(),
                label: "Service".to_string(),
                kind: QuickCommandParameterKind::Text,
                default_value: String::new(),
                choices: String::new(),
                required: true,
            }],
            protocols: Vec::new(),
            confirmation: QuickCommandConfirmationPolicy::Inherit,
            created_at: 1,
            sort_order: 0,
        };

        assert!(!quick_command_editor_can_save(&draft));
        draft.command = "deploy {{param.service|sh}}".to_string();
        assert!(quick_command_editor_can_save(&draft));
    }

    #[test]
    fn quick_command_editor_rejects_secret_defaults() {
        let draft = QuickCommandEditorDraft {
            id: None,
            name: "Login".to_string(),
            command: "login {{param.password}}".to_string(),
            category: "custom".to_string(),
            description: String::new(),
            host_patterns: String::new(),
            parameters: vec![QuickCommandParameterEditorDraft {
                name: "password".to_string(),
                label: "Password".to_string(),
                kind: QuickCommandParameterKind::Secret,
                default_value: "must-not-persist".to_string(),
                choices: String::new(),
                required: true,
            }],
            protocols: Vec::new(),
            confirmation: QuickCommandConfirmationPolicy::Inherit,
            created_at: 1,
            sort_order: 0,
        };

        assert!(!quick_command_editor_can_save(&draft));
    }

    #[test]
    fn parameterized_commands_show_dynamic_risk_until_expansion() {
        assert_eq!(
            quick_command_risk_badge("echo {{param.value}}"),
            Some(QuickCommandRiskBadge::Dynamic)
        );
    }
}
