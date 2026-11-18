use crate::SettingsInput;
use gpui::px;
use oxideterm_terminal_triggers::{
    LocalProcessSpec, TerminalTrigger, TerminalTriggerAction, TerminalTriggerDispatch,
    TerminalTriggerMatch, TerminalTriggerMatchMode, TerminalTriggerScope, TerminalTriggerTiming,
    TerminalTriggersSnapshot, default_snapshot, load_snapshot, new_trigger_id, now_ms,
    save_snapshot, validate_snapshot,
};
use oxideterm_workspace::PaneId;
use std::path::{Path, PathBuf};

const TERMINAL_TRIGGER_DEFAULT_COOLDOWN_MS: u64 = 1_000;
pub fn editable_trigger_action(
    action: &TerminalTriggerAction,
    index: usize,
) -> &TerminalTriggerAction {
    match action {
        TerminalTriggerAction::CustomLink { action } => action,
        TerminalTriggerAction::CustomMenu { items } if !items.is_empty() => {
            &items[index.min(items.len() - 1)].action
        }
        _ => action,
    }
}

pub fn editable_trigger_action_mut(
    action: &mut TerminalTriggerAction,
    index: usize,
) -> &mut TerminalTriggerAction {
    match action {
        TerminalTriggerAction::CustomLink { action } => action,
        TerminalTriggerAction::CustomMenu { items } => {
            let index = index.min(items.len() - 1);
            &mut items[index].action
        }
        _ => action,
    }
}

pub fn is_terminal_trigger_input(input: SettingsInput) -> bool {
    matches!(
        input,
        SettingsInput::TerminalTriggerName
            | SettingsInput::TerminalTriggerDescription
            | SettingsInput::TerminalTriggerPattern
            | SettingsInput::TerminalTriggerActionValue
            | SettingsInput::TerminalTriggerMenuLabel
            | SettingsInput::TerminalTriggerTooltipText
            | SettingsInput::TerminalTriggerExecutable
            | SettingsInput::TerminalTriggerArguments
            | SettingsInput::TerminalTriggerWorkingDirectory
            | SettingsInput::TerminalTriggerDelayMs
            | SettingsInput::TerminalTriggerCooldownMs
            | SettingsInput::TerminalTriggerConnectionSearch
    )
}

pub struct TerminalTriggersSettingsState {
    settings_path: PathBuf,
    pub pane: Option<PaneId>,
    pub shell_confirmation_pending: bool,
    pub modal_scroll: gpui::ScrollHandle,
    pub snapshot: TerminalTriggersSnapshot,
    pub editor: Option<TerminalTrigger>,
    pub editor_open: bool,
    pub menu_item: usize,
    pub scope_picker_open: bool,
    pub scope_picker_query: String,
    pub scope_picker_scroll: gpui::ScrollHandle,
    pub error_key: Option<&'static str>,
}

impl Default for TerminalTriggersSettingsState {
    fn default() -> Self {
        Self {
            settings_path: PathBuf::new(),
            pane: None,
            shell_confirmation_pending: false,
            modal_scroll: gpui::ScrollHandle::new(),
            snapshot: default_snapshot(),
            editor: None,
            editor_open: false,
            menu_item: 0,
            scope_picker_open: false,
            scope_picker_query: String::new(),
            scope_picker_scroll: gpui::ScrollHandle::new(),
            error_key: None,
        }
    }
}

impl TerminalTriggersSettingsState {
    // Settings dropdowns update only the active trigger draft; persistence still happens on Save.
    pub fn terminal_trigger_draft(&self) -> Option<&TerminalTrigger> {
        self.editor.as_ref()
    }

    pub fn terminal_trigger_draft_action(&self) -> Option<&TerminalTriggerAction> {
        self.terminal_trigger_draft()
            .map(|trigger| editable_trigger_action(&trigger.action, self.menu_item))
    }

    pub fn select_terminal_trigger_click_action(&mut self, action: TerminalTriggerAction) {
        if let Some(trigger) = self.editor.as_mut() {
            let current = editable_trigger_action_mut(&mut trigger.action, self.menu_item);
            if std::mem::discriminant(current) != std::mem::discriminant(&action) {
                *current = action;
            }
        }
    }

    pub fn select_terminal_trigger_match_mode(&mut self, mode: TerminalTriggerMatchMode) {
        if let Some(trigger) = self.editor.as_mut() {
            trigger.matcher.mode = mode;
        }
    }

    pub fn select_terminal_trigger_action(&mut self, action: TerminalTriggerAction) {
        let Some(trigger) = self.editor.as_mut() else {
            return;
        };
        let same_kind = std::mem::discriminant(&trigger.action) == std::mem::discriminant(&action);
        if !same_kind {
            trigger.action = action;
            self.menu_item = 0;
        }
    }

    pub fn select_terminal_trigger_process_mode(&mut self, shell: bool) {
        let current_shell = self.terminal_trigger_draft_action().is_some_and(|action| {
            matches!(
                action,
                TerminalTriggerAction::LaunchLocalProcess {
                    process: LocalProcessSpec::ExplicitShell { .. }
                }
            )
        });
        if current_shell != shell {
            self.toggle_process_mode();
        }
    }

    pub fn select_terminal_trigger_quick_command(&mut self, quick_command_id: String) {
        if let Some(trigger) = self.editor.as_mut()
            && let TerminalTriggerAction::RunQuickCommand {
                quick_command_id: selected_id,
            } = editable_trigger_action_mut(&mut trigger.action, self.menu_item)
        {
            *selected_id = quick_command_id;
        }
    }

    pub fn select_terminal_trigger_timing(&mut self, dispatch: TerminalTriggerDispatch) {
        if let Some(trigger) = self.editor.as_mut() {
            trigger.timing.dispatch = dispatch;
        }
    }

    pub fn select_terminal_trigger_scope(&mut self, scope: TerminalTriggerScope) {
        let open_picker = matches!(scope, TerminalTriggerScope::SavedConnections { .. });
        if let Some(trigger) = self.editor.as_mut() {
            trigger.scope = scope;
            self.scope_picker_open = open_picker;
            self.scope_picker_query.clear();
            self.scope_picker_scroll = gpui::ScrollHandle::new();
        }
    }

    pub fn load(settings_path: &Path) -> Self {
        match load_snapshot(settings_path) {
            Ok(snapshot) => {
                let editor = snapshot.triggers.first().cloned();
                Self {
                    settings_path: settings_path.to_path_buf(),
                    snapshot,
                    editor,
                    ..Self::default()
                }
            }
            Err(_error) => Self {
                settings_path: settings_path.to_path_buf(),
                error_key: Some("settings_view.terminal.triggers.load_failed"),
                ..Self::default()
            },
        }
    }

    pub fn new_rule(&mut self) {
        self.menu_item = 0;
        let timestamp = now_ms();
        self.editor = Some(TerminalTrigger {
            id: new_trigger_id(),
            name: String::new(),
            description: None,
            enabled: true,
            matcher: TerminalTriggerMatch {
                pattern: String::new(),
                mode: TerminalTriggerMatchMode::Literal,
                case_sensitive: false,
                whole_word: false,
            },
            action: TerminalTriggerAction::SendText {
                text: String::new(),
                append_enter: false,
            },
            timing: TerminalTriggerTiming {
                dispatch: TerminalTriggerDispatch::Immediate,
                delay_ms: 0,
                cooldown_ms: TERMINAL_TRIGGER_DEFAULT_COOLDOWN_MS,
            },
            scope: TerminalTriggerScope::AllTerminals,
            created_at: timestamp,
            updated_at: timestamp,
        });
        self.editor_open = true;
        self.scope_picker_open = false;
        self.error_key = None;
    }

    pub fn edit_rule(&mut self, trigger_id: &str) {
        self.menu_item = 0;
        self.editor = self
            .snapshot
            .triggers
            .iter()
            .find(|trigger| trigger.id == trigger_id)
            .cloned();
        self.editor_open = self.editor.is_some();
        self.scope_picker_open = false;
        self.error_key = None;
    }

    pub fn duplicate_rule(&mut self, trigger_id: &str, copy_suffix: &str) {
        self.menu_item = 0;
        let Some(mut trigger) = self
            .snapshot
            .triggers
            .iter()
            .find(|trigger| trigger.id == trigger_id)
            .cloned()
        else {
            return;
        };
        let timestamp = now_ms();
        trigger.id = new_trigger_id();
        trigger.name = format!("{} ({copy_suffix})", trigger.name);
        trigger.created_at = timestamp;
        trigger.updated_at = timestamp;
        self.editor = Some(trigger);
        self.editor_open = true;
        self.scope_picker_open = false;
        self.error_key = None;
    }

    pub fn cancel_edit(&mut self) {
        self.menu_item = 0;
        self.editor = self
            .editor
            .as_ref()
            .and_then(|editor| {
                self.snapshot
                    .triggers
                    .iter()
                    .find(|trigger| trigger.id == editor.id)
            })
            .cloned()
            .or_else(|| self.snapshot.triggers.first().cloned());
        self.editor_open = false;
        self.scope_picker_open = false;
        self.error_key = None;
    }

    fn persist_snapshot(&mut self, snapshot: TerminalTriggersSnapshot) -> bool {
        if validate_snapshot(&snapshot).is_err() {
            self.error_key = Some("settings_view.terminal.triggers.validation_failed");
            return false;
        }
        match save_snapshot(&self.settings_path, &snapshot) {
            Ok(()) => {
                self.snapshot = snapshot;
                self.error_key = None;
                true
            }
            Err(_error) => {
                self.error_key = Some("settings_view.terminal.triggers.save_failed");
                false
            }
        }
    }

    pub fn save_editor(&mut self, explicit_shell_enabled: bool) -> bool {
        let Some(mut trigger) = self.editor.clone() else {
            return false;
        };
        if trigger.action.uses_explicit_shell() && !explicit_shell_enabled {
            self.error_key = Some("settings_view.terminal.triggers.shell_execution_hint");
            return false;
        }

        let timestamp = now_ms();
        trigger.updated_at = timestamp;
        let trigger_id = trigger.id.clone();
        let mut snapshot = self.snapshot.clone();
        snapshot.updated_at = timestamp;
        if let Some(existing) = snapshot
            .triggers
            .iter_mut()
            .find(|candidate| candidate.id == trigger.id)
        {
            *existing = trigger;
        } else {
            snapshot.triggers.push(trigger);
        }
        if self.persist_snapshot(snapshot) {
            self.editor = self
                .snapshot
                .triggers
                .iter()
                .find(|trigger| trigger.id == trigger_id)
                .cloned();
            self.editor_open = false;
            self.scope_picker_open = false;
            true
        } else {
            false
        }
    }

    pub fn delete_rule(&mut self, trigger_id: &str) -> bool {
        let mut snapshot = self.snapshot.clone();
        snapshot.triggers.retain(|trigger| trigger.id != trigger_id);
        snapshot.updated_at = now_ms();
        let changed = self.persist_snapshot(snapshot);
        if changed {
            if self
                .editor
                .as_ref()
                .is_some_and(|editor| editor.id == trigger_id)
            {
                self.editor_open = false;
            }
            self.editor = self.snapshot.triggers.first().cloned();
            self.scope_picker_open = false;
        }
        changed
    }

    pub fn toggle_rule(&mut self, trigger_id: &str) -> bool {
        let mut snapshot = self.snapshot.clone();
        let Some(trigger) = snapshot
            .triggers
            .iter_mut()
            .find(|trigger| trigger.id == trigger_id)
        else {
            return false;
        };
        trigger.enabled = !trigger.enabled;
        trigger.updated_at = now_ms();
        snapshot.updated_at = trigger.updated_at;
        let changed = self.persist_snapshot(snapshot);
        if changed
            && self
                .editor
                .as_ref()
                .is_some_and(|editor| editor.id == trigger_id)
        {
            let enabled = self
                .snapshot
                .triggers
                .iter()
                .find(|trigger| trigger.id == trigger_id)
                .map(|trigger| trigger.enabled);
            if let (Some(editor), Some(enabled)) = (self.editor.as_mut(), enabled) {
                // Keep unsaved field edits while applying the independently persisted toggle.
                editor.enabled = enabled;
            }
        }
        changed
    }

    pub fn editor_input_value(&self, input: SettingsInput) -> Option<String> {
        if input == SettingsInput::TerminalTriggerConnectionSearch {
            return Some(self.scope_picker_query.clone());
        }
        let trigger = self.editor.as_ref()?;
        let action = editable_trigger_action(&trigger.action, self.menu_item);
        match input {
            SettingsInput::TerminalTriggerMenuLabel => match &trigger.action {
                TerminalTriggerAction::CustomMenu { items } => {
                    items.get(self.menu_item).map(|item| item.label.clone())
                }
                _ => None,
            },
            SettingsInput::TerminalTriggerName => Some(trigger.name.clone()),
            SettingsInput::TerminalTriggerDescription => {
                Some(trigger.description.clone().unwrap_or_default())
            }
            SettingsInput::TerminalTriggerPattern => Some(trigger.matcher.pattern.clone()),
            SettingsInput::TerminalTriggerActionValue
            | SettingsInput::TerminalTriggerTooltipText => match action {
                TerminalTriggerAction::SendText { text, .. } => Some(text.clone()),
                TerminalTriggerAction::ShowTooltip { text } => Some(text.clone()),
                TerminalTriggerAction::OpenUrl { url } => Some(url.clone()),
                TerminalTriggerAction::RunQuickCommand { quick_command_id } => {
                    Some(quick_command_id.clone())
                }
                _ => Some(String::new()),
            },
            SettingsInput::TerminalTriggerExecutable => match action {
                TerminalTriggerAction::LaunchLocalProcess { process } => match process {
                    LocalProcessSpec::DirectProgram { executable, .. } => Some(executable.clone()),
                    LocalProcessSpec::ExplicitShell {
                        shell_executable, ..
                    } => Some(shell_executable.clone()),
                },
                _ => Some(String::new()),
            },
            SettingsInput::TerminalTriggerArguments => match action {
                TerminalTriggerAction::LaunchLocalProcess { process } => match process {
                    LocalProcessSpec::DirectProgram { arguments, .. }
                    | LocalProcessSpec::ExplicitShell { arguments, .. } => {
                        Some(arguments.join("\n"))
                    }
                },
                _ => Some(String::new()),
            },
            SettingsInput::TerminalTriggerWorkingDirectory => match action {
                TerminalTriggerAction::LaunchLocalProcess { process } => match process {
                    LocalProcessSpec::DirectProgram {
                        working_directory, ..
                    }
                    | LocalProcessSpec::ExplicitShell {
                        working_directory, ..
                    } => Some(working_directory.clone().unwrap_or_default()),
                },
                _ => Some(String::new()),
            },
            SettingsInput::TerminalTriggerDelayMs => Some(trigger.timing.delay_ms.to_string()),
            SettingsInput::TerminalTriggerCooldownMs => {
                Some(trigger.timing.cooldown_ms.to_string())
            }
            _ => None,
        }
    }

    pub fn apply_editor_input(&mut self, input: SettingsInput, value: &str) -> bool {
        if input == SettingsInput::TerminalTriggerConnectionSearch {
            if self.scope_picker_query != value {
                self.scope_picker_query = value.to_string();
                self.scope_picker_scroll
                    .set_offset(gpui::point(px(0.0), px(0.0)));
            }
            return true;
        }
        let Some(trigger) = self.editor.as_mut() else {
            return false;
        };
        if input == SettingsInput::TerminalTriggerMenuLabel {
            if let TerminalTriggerAction::CustomMenu { items } = &mut trigger.action
                && let Some(item) = items.get_mut(self.menu_item)
            {
                item.label = value.to_string();
                return true;
            }
            return false;
        }
        let action = editable_trigger_action_mut(&mut trigger.action, self.menu_item);
        match input {
            SettingsInput::TerminalTriggerName => trigger.name = value.to_string(),
            SettingsInput::TerminalTriggerDescription => {
                trigger.description = (!value.is_empty()).then(|| value.to_string());
            }
            SettingsInput::TerminalTriggerPattern => trigger.matcher.pattern = value.to_string(),
            SettingsInput::TerminalTriggerActionValue
            | SettingsInput::TerminalTriggerTooltipText => match action {
                TerminalTriggerAction::SendText { text, .. } => *text = value.to_string(),
                TerminalTriggerAction::ShowTooltip { text } => *text = value.to_string(),
                TerminalTriggerAction::OpenUrl { url } => *url = value.to_string(),
                TerminalTriggerAction::RunQuickCommand { quick_command_id } => {
                    *quick_command_id = value.to_string();
                }
                _ => return false,
            },
            SettingsInput::TerminalTriggerExecutable => match action {
                TerminalTriggerAction::LaunchLocalProcess { process } => match process {
                    LocalProcessSpec::DirectProgram { executable, .. } => {
                        *executable = value.to_string()
                    }
                    LocalProcessSpec::ExplicitShell {
                        shell_executable, ..
                    } => *shell_executable = value.to_string(),
                },
                _ => return false,
            },
            SettingsInput::TerminalTriggerArguments => match action {
                TerminalTriggerAction::LaunchLocalProcess { process } => {
                    let arguments = value.lines().map(str::to_string).collect();
                    match process {
                        LocalProcessSpec::DirectProgram {
                            arguments: current, ..
                        }
                        | LocalProcessSpec::ExplicitShell {
                            arguments: current, ..
                        } => *current = arguments,
                    }
                }
                _ => return false,
            },
            SettingsInput::TerminalTriggerWorkingDirectory => match action {
                TerminalTriggerAction::LaunchLocalProcess { process } => {
                    let directory = (!value.trim().is_empty()).then(|| value.to_string());
                    match process {
                        LocalProcessSpec::DirectProgram {
                            working_directory, ..
                        }
                        | LocalProcessSpec::ExplicitShell {
                            working_directory, ..
                        } => *working_directory = directory,
                    }
                }
                _ => return false,
            },
            SettingsInput::TerminalTriggerDelayMs => {
                let Ok(delay_ms) = value.parse() else {
                    return false;
                };
                trigger.timing.delay_ms = delay_ms;
            }
            SettingsInput::TerminalTriggerCooldownMs => {
                let Ok(cooldown_ms) = value.parse() else {
                    return false;
                };
                trigger.timing.cooldown_ms = cooldown_ms;
            }
            _ => return false,
        }
        true
    }
    pub fn toggle_process_mode(&mut self) {
        let Some(trigger) = self.editor.as_mut() else {
            return;
        };
        let TerminalTriggerAction::LaunchLocalProcess { process } =
            editable_trigger_action_mut(&mut trigger.action, self.menu_item)
        else {
            return;
        };
        let replacement = match &mut *process {
            LocalProcessSpec::DirectProgram {
                executable,
                arguments,
                working_directory,
            } => LocalProcessSpec::ExplicitShell {
                shell_executable: std::mem::take(executable),
                arguments: std::mem::take(arguments),
                working_directory: working_directory.take(),
            },
            LocalProcessSpec::ExplicitShell {
                shell_executable,
                arguments,
                working_directory,
            } => LocalProcessSpec::DirectProgram {
                executable: std::mem::take(shell_executable),
                arguments: std::mem::take(arguments),
                working_directory: working_directory.take(),
            },
        };
        *process = replacement;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state() -> (tempfile::TempDir, TerminalTriggersSettingsState) {
        let directory = tempfile::tempdir().expect("trigger settings directory");
        let settings_path = directory.path().join("settings.json");
        let state = TerminalTriggersSettingsState::load(&settings_path);
        (directory, state)
    }

    #[test]
    fn editor_saves_only_after_validation_succeeds() {
        let (_directory, mut state) = state();
        state.new_rule();
        let trigger = state.editor.as_mut().expect("trigger editor");
        trigger.name = "Prompt ready".to_string();
        trigger.matcher.pattern = "ready> ".to_string();
        trigger.action = TerminalTriggerAction::SendText {
            text: "status".to_string(),
            append_enter: true,
        };

        assert!(state.save_editor(false));
        assert_eq!(state.snapshot.triggers.len(), 1);
        assert_eq!(
            state.editor.as_ref().map(|trigger| trigger.name.as_str()),
            Some("Prompt ready")
        );
        let saved = load_snapshot(&state.settings_path).expect("saved trigger snapshot");
        assert_eq!(saved.triggers.len(), 1);
        assert_eq!(saved.triggers[0].name, "Prompt ready");
        assert_eq!(saved.triggers[0].matcher.pattern, "ready> ");
        assert_eq!(
            saved.triggers[0].action,
            TerminalTriggerAction::SendText {
                text: "status".to_string(),
                append_enter: true,
            }
        );
    }

    #[test]
    fn explicit_shell_editor_requires_the_separate_trust_setting() {
        let (_directory, mut state) = state();
        state.new_rule();
        let trigger = state.editor.as_mut().expect("trigger editor");
        trigger.name = "Trusted shell".to_string();
        trigger.matcher.pattern = "deploy".to_string();
        trigger.action = TerminalTriggerAction::LaunchLocalProcess {
            process: LocalProcessSpec::ExplicitShell {
                shell_executable: "/bin/sh".to_string(),
                arguments: vec!["-c".to_string(), "echo ok".to_string()],
                working_directory: None,
            },
        };

        assert!(!state.save_editor(false));
        assert!(state.snapshot.triggers.is_empty());
        assert_eq!(
            state.error_key,
            Some("settings_view.terminal.triggers.shell_execution_hint")
        );
    }

    #[test]
    fn modal_state_opens_closes_and_closes_after_valid_save() {
        let (_directory, mut state) = state();
        assert!(!state.editor_open);

        state.new_rule();
        assert!(state.editor_open);

        assert!(!state.save_editor(false));
        assert!(state.editor_open);

        let trigger = state.editor.as_mut().expect("trigger editor");
        trigger.name = "Alert".to_string();
        trigger.matcher.pattern = "ERROR".to_string();
        trigger.action = TerminalTriggerAction::SendText {
            text: "status".to_string(),
            append_enter: true,
        };
        assert!(state.save_editor(false));
        assert!(!state.editor_open);

        let saved_id = state.snapshot.triggers[0].id.clone();
        state.edit_rule(&saved_id);
        assert!(state.editor_open);
        state.cancel_edit();
        assert!(!state.editor_open);

        state.duplicate_rule(&saved_id, "copy");
        assert!(state.editor_open);
        assert!(state.save_editor(false));
        assert!(!state.editor_open);
        assert_eq!(state.snapshot.triggers.len(), 2);

        state.edit_rule(&saved_id);
        assert!(state.editor_open);
        assert!(state.delete_rule(&saved_id));
        assert!(!state.editor_open);
        assert_eq!(state.snapshot.triggers.len(), 1);
    }
}
