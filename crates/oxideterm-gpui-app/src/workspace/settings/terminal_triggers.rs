use super::*;
use oxideterm_terminal_triggers::{
    LocalProcessSpec, SavedConnectionKind, SavedConnectionRef, TerminalTrigger,
    TerminalTriggerAction, TerminalTriggerDispatch, TerminalTriggerMatch, TerminalTriggerMatchMode,
    TerminalTriggerScope, TerminalTriggerTiming, TerminalTriggersSnapshot, default_snapshot,
    load_snapshot, new_trigger_id, now_ms, save_snapshot, validate_snapshot,
};

// Terminal settings reserve section zero for the page switcher before page cards.
const TERMINAL_TRIGGER_SETTINGS_SECTION_INDEX_WITH_PAGE_SWITCHER: usize = 3;
const TERMINAL_TRIGGER_DEFAULT_COOLDOWN_MS: u64 = 1_000;
// Trigger forms stay readable on wide settings panes instead of stretching with the window.
const TERMINAL_TRIGGER_EDITOR_MAX_WIDTH: f32 = 1_120.0;
const TERMINAL_TRIGGER_FIELD_BASIS: f32 = 280.0;
const TERMINAL_TRIGGER_COMPACT_FIELD_BASIS: f32 = 180.0;
const TERMINAL_TRIGGER_SIDEBAR_WIDTH: f32 = 260.0;
const TERMINAL_TRIGGER_RULE_LIST_MAX_HEIGHT: f32 = 520.0;

fn is_terminal_trigger_input(input: SettingsInput) -> bool {
    matches!(
        input,
        SettingsInput::TerminalTriggerName
            | SettingsInput::TerminalTriggerDescription
            | SettingsInput::TerminalTriggerPattern
            | SettingsInput::TerminalTriggerActionValue
            | SettingsInput::TerminalTriggerExecutable
            | SettingsInput::TerminalTriggerArguments
            | SettingsInput::TerminalTriggerWorkingDirectory
            | SettingsInput::TerminalTriggerDelayMs
            | SettingsInput::TerminalTriggerCooldownMs
    )
}

pub(in crate::workspace) struct TerminalTriggersSettingsState {
    settings_path: PathBuf,
    pub(in crate::workspace) snapshot: TerminalTriggersSnapshot,
    editor: Option<TerminalTrigger>,
    scope_picker_open: bool,
    error_key: Option<&'static str>,
}

impl TerminalTriggersSettingsState {
    pub(in crate::workspace) fn load(settings_path: &Path) -> Self {
        match load_snapshot(settings_path) {
            Ok(snapshot) => {
                let editor = snapshot.triggers.first().cloned();
                Self {
                    settings_path: settings_path.to_path_buf(),
                    snapshot,
                    editor,
                    scope_picker_open: false,
                    error_key: None,
                }
            }
            Err(_error) => Self {
                settings_path: settings_path.to_path_buf(),
                snapshot: default_snapshot(),
                editor: None,
                scope_picker_open: false,
                error_key: Some("settings_view.terminal.triggers.load_failed"),
            },
        }
    }

    fn new_rule(&mut self) {
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
        self.scope_picker_open = false;
        self.error_key = None;
    }

    fn edit_rule(&mut self, trigger_id: &str) {
        self.editor = self
            .snapshot
            .triggers
            .iter()
            .find(|trigger| trigger.id == trigger_id)
            .cloned();
        self.scope_picker_open = false;
        self.error_key = None;
    }

    fn duplicate_rule(&mut self, trigger_id: &str, copy_suffix: &str) {
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
        self.scope_picker_open = false;
        self.error_key = None;
    }

    pub(in crate::workspace) fn cancel_edit(&mut self) {
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

    fn save_editor(&mut self, explicit_shell_enabled: bool) -> bool {
        let Some(mut trigger) = self.editor.clone() else {
            return false;
        };
        if matches!(
            trigger.action,
            TerminalTriggerAction::LaunchLocalProcess {
                process: LocalProcessSpec::ExplicitShell { .. }
            }
        ) && !explicit_shell_enabled
        {
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
            self.scope_picker_open = false;
            true
        } else {
            false
        }
    }

    fn delete_rule(&mut self, trigger_id: &str) -> bool {
        let mut snapshot = self.snapshot.clone();
        snapshot.triggers.retain(|trigger| trigger.id != trigger_id);
        snapshot.updated_at = now_ms();
        let changed = self.persist_snapshot(snapshot);
        if changed {
            self.editor = self.snapshot.triggers.first().cloned();
            self.scope_picker_open = false;
        }
        changed
    }

    fn toggle_rule(&mut self, trigger_id: &str) -> bool {
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

    fn editor_input_value(&self, input: SettingsInput) -> Option<String> {
        let trigger = self.editor.as_ref()?;
        match input {
            SettingsInput::TerminalTriggerName => Some(trigger.name.clone()),
            SettingsInput::TerminalTriggerDescription => {
                Some(trigger.description.clone().unwrap_or_default())
            }
            SettingsInput::TerminalTriggerPattern => Some(trigger.matcher.pattern.clone()),
            SettingsInput::TerminalTriggerActionValue => match &trigger.action {
                TerminalTriggerAction::SendText { text, .. } => Some(text.clone()),
                TerminalTriggerAction::RunQuickCommand { quick_command_id } => {
                    Some(quick_command_id.clone())
                }
                TerminalTriggerAction::LaunchLocalProcess { .. } => Some(String::new()),
            },
            SettingsInput::TerminalTriggerExecutable => match &trigger.action {
                TerminalTriggerAction::LaunchLocalProcess { process } => match process {
                    LocalProcessSpec::DirectProgram { executable, .. } => Some(executable.clone()),
                    LocalProcessSpec::ExplicitShell {
                        shell_executable, ..
                    } => Some(shell_executable.clone()),
                },
                _ => Some(String::new()),
            },
            SettingsInput::TerminalTriggerArguments => match &trigger.action {
                TerminalTriggerAction::LaunchLocalProcess { process } => match process {
                    LocalProcessSpec::DirectProgram { arguments, .. }
                    | LocalProcessSpec::ExplicitShell { arguments, .. } => {
                        Some(arguments.join("\n"))
                    }
                },
                _ => Some(String::new()),
            },
            SettingsInput::TerminalTriggerWorkingDirectory => match &trigger.action {
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

    fn apply_editor_input(&mut self, input: SettingsInput, value: &str) -> bool {
        let Some(trigger) = self.editor.as_mut() else {
            return false;
        };
        match input {
            SettingsInput::TerminalTriggerName => trigger.name = value.to_string(),
            SettingsInput::TerminalTriggerDescription => {
                trigger.description = (!value.is_empty()).then(|| value.to_string());
            }
            SettingsInput::TerminalTriggerPattern => trigger.matcher.pattern = value.to_string(),
            SettingsInput::TerminalTriggerActionValue => match &mut trigger.action {
                TerminalTriggerAction::SendText { text, .. } => *text = value.to_string(),
                TerminalTriggerAction::RunQuickCommand { quick_command_id } => {
                    *quick_command_id = value.to_string();
                }
                TerminalTriggerAction::LaunchLocalProcess { .. } => return false,
            },
            SettingsInput::TerminalTriggerExecutable => match &mut trigger.action {
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
            SettingsInput::TerminalTriggerArguments => match &mut trigger.action {
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
            SettingsInput::TerminalTriggerWorkingDirectory => match &mut trigger.action {
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
}

impl WorkspaceApp {
    pub(in crate::workspace) fn open_terminal_trigger_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.terminal_trigger_settings_pane = None;
        self.open_terminal_trigger_settings_page(window, cx);
    }

    pub(in crate::workspace) fn open_terminal_trigger_settings_for_pane(
        &mut self,
        pane_id: PaneId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.terminal_trigger_settings_pane = Some(pane_id);
        self.open_terminal_trigger_settings_page(window, cx);
    }

    fn open_terminal_trigger_settings_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.set_active_tab(SettingsTab::Terminal, cx);
            settings.set_terminal_page(TerminalSettingsPage::Awareness, cx);
        });
        self.sync_settings_section_list_state(cx);
        self.settings_section_list_state
            .scroll_to(gpui::ListOffset {
                item_ix: SETTINGS_SECTION_HEADER_ITEM_COUNT
                    + TERMINAL_TRIGGER_SETTINGS_SECTION_INDEX_WITH_PAGE_SWITCHER,
                offset_in_item: px(0.0),
            });
        self.open_settings(window, cx);
    }

    pub(in crate::workspace) fn terminal_trigger_settings_input_value(
        &self,
        input: SettingsInput,
    ) -> Option<String> {
        self.terminal_triggers.editor_input_value(input)
    }

    pub(in crate::workspace) fn apply_terminal_trigger_settings_input(
        &mut self,
        input: SettingsInput,
        value: &str,
    ) -> bool {
        self.terminal_triggers.apply_editor_input(input, value)
    }

    pub(in crate::workspace) fn clear_terminal_trigger_input_focus(&mut self) {
        if let Some(input) = self
            .focused_settings_input
            .filter(|input| is_terminal_trigger_input(*input))
        {
            self.focused_settings_input = None;
            self.clear_settings_input_draft(input);
            self.clear_ime_selection();
        }
    }

    // Settings dropdowns update only the active trigger draft; persistence still happens on Save.
    pub(in crate::workspace) fn terminal_trigger_draft(&self) -> Option<&TerminalTrigger> {
        self.terminal_triggers.editor.as_ref()
    }

    pub(in crate::workspace) fn select_terminal_trigger_match_mode(
        &mut self,
        mode: TerminalTriggerMatchMode,
    ) {
        if let Some(trigger) = self.terminal_triggers.editor.as_mut() {
            trigger.matcher.mode = mode;
        }
    }

    pub(in crate::workspace) fn select_terminal_trigger_action(
        &mut self,
        action: TerminalTriggerAction,
    ) {
        let Some(trigger) = self.terminal_triggers.editor.as_mut() else {
            return;
        };
        let same_kind = matches!(
            (&trigger.action, &action),
            (
                TerminalTriggerAction::SendText { .. },
                TerminalTriggerAction::SendText { .. }
            ) | (
                TerminalTriggerAction::RunQuickCommand { .. },
                TerminalTriggerAction::RunQuickCommand { .. }
            ) | (
                TerminalTriggerAction::LaunchLocalProcess { .. },
                TerminalTriggerAction::LaunchLocalProcess { .. }
            )
        );
        if !same_kind {
            trigger.action = action;
        }
    }

    pub(in crate::workspace) fn select_terminal_trigger_process_mode(&mut self, shell: bool) {
        let current_shell = self
            .terminal_triggers
            .editor
            .as_ref()
            .is_some_and(|trigger| {
                matches!(
                    trigger.action,
                    TerminalTriggerAction::LaunchLocalProcess {
                        process: LocalProcessSpec::ExplicitShell { .. }
                    }
                )
            });
        if current_shell != shell {
            self.toggle_terminal_trigger_process_mode();
        }
    }

    pub(in crate::workspace) fn select_terminal_trigger_quick_command(
        &mut self,
        quick_command_id: String,
    ) {
        if let Some(trigger) = self.terminal_triggers.editor.as_mut()
            && let TerminalTriggerAction::RunQuickCommand {
                quick_command_id: selected_id,
            } = &mut trigger.action
        {
            *selected_id = quick_command_id;
        }
    }

    pub(in crate::workspace) fn select_terminal_trigger_timing(
        &mut self,
        dispatch: TerminalTriggerDispatch,
    ) {
        if let Some(trigger) = self.terminal_triggers.editor.as_mut() {
            trigger.timing.dispatch = dispatch;
        }
    }

    pub(in crate::workspace) fn select_terminal_trigger_scope(
        &mut self,
        scope: TerminalTriggerScope,
    ) {
        let open_picker = matches!(scope, TerminalTriggerScope::SavedConnections { .. });
        if let Some(trigger) = self.terminal_triggers.editor.as_mut() {
            trigger.scope = scope;
            self.terminal_triggers.scope_picker_open = open_picker;
        }
    }

    fn persist_terminal_trigger_change(&mut self, changed: bool, cx: &mut Context<Self>) {
        if changed {
            self.refresh_terminal_trigger_runtime(cx);
        }
        cx.notify();
    }

    fn persist_terminal_trigger_shell_execution(&mut self, enabled: bool, cx: &mut Context<Self>) {
        let previous_settings = self.settings_store.settings().clone();
        let mut next_settings = previous_settings.clone();
        set_terminal_trigger_shell_execution(&mut next_settings, enabled);
        match self.settings_store.replace_and_save(next_settings) {
            Ok(saved) => {
                self.terminal_trigger_shell_confirmation_pending = false;
                self.terminal_triggers.error_key = None;
                self.apply_loaded_settings_to_runtime(&previous_settings, &saved.settings, cx);
                self.settings_workspace.update(cx, |settings, _cx| {
                    settings.acknowledge_external_store_state()
                });
                self.emit_native_plugin_settings_events(&previous_settings, &saved.settings, cx);
                self.refresh_terminal_trigger_runtime(cx);
                self.sync_tab_titles(cx);
            }
            Err(_error) => {
                self.terminal_trigger_shell_confirmation_pending = false;
                self.terminal_triggers.error_key =
                    Some("settings_view.terminal.triggers.save_failed");
            }
        }
        cx.notify();
    }

    pub(in crate::workspace) fn terminal_triggers_settings_card(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(pane_id) = self.terminal_trigger_settings_pane {
            return self.terminal_trigger_session_card(pane_id, cx);
        }
        self.terminal_trigger_global_card(cx)
    }

    fn terminal_trigger_global_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let header = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_row()
            .flex_wrap()
            .items_start()
            .justify_between()
            .gap(px(16.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(220.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(
                                self.i18n
                                    .t("settings_view.terminal.triggers.global_title")
                                    .to_uppercase(),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("settings_view.terminal.triggers.description")),
                    ),
            )
            .child(self.terminal_trigger_button(
                self.i18n.t("settings_view.terminal.triggers.add_rule"),
                ButtonVariant::Default,
                cx.listener(|this, _event, _window, cx| {
                    this.clear_terminal_trigger_input_focus();
                    this.terminal_triggers.new_rule();
                    cx.notify();
                }),
            ))
            .into_any_element();
        let mut rows = vec![
            header,
            self.card_separator(),
            self.terminal_trigger_shell_trust_row(cx),
            self.card_separator(),
            self.terminal_trigger_manager(cx),
        ];
        if let Some(error_key) = self.terminal_triggers.error_key {
            rows.push(self.card_separator());
            rows.push(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.error))
                    .child(self.i18n.t(error_key))
                    .into_any_element(),
            );
        }
        rows.push(self.card_separator());
        rows.push(
            div()
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(
                    self.i18n
                        .t("settings_view.terminal.triggers.secrets_warning"),
                )
                .into_any_element(),
        );
        self.plain_settings_card(rows)
    }

    fn terminal_trigger_manager(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut rule_list = div().w_full().flex().flex_col().gap(px(4.0));
        if self.terminal_triggers.snapshot.triggers.is_empty() {
            rule_list = rule_list.child(
                div()
                    .py(px(16.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.terminal.triggers.no_rules")),
            );
        } else {
            for trigger in &self.terminal_triggers.snapshot.triggers {
                rule_list = rule_list.child(self.terminal_trigger_rule_row(trigger, cx));
            }
        }

        let selected_saved_rule = self.terminal_triggers.editor.as_ref().and_then(|editor| {
            self.terminal_triggers
                .snapshot
                .triggers
                .iter()
                .find(|trigger| trigger.id == editor.id)
        });
        let sidebar = div()
            .w(px(TERMINAL_TRIGGER_SIDEBAR_WIDTH))
            .max_w_full()
            .flex_none()
            .pr(px(16.0))
            .border_r_1()
            .border_color(rgb(self.tokens.ui.border))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                div()
                    .w_full()
                    .max_h(px(TERMINAL_TRIGGER_RULE_LIST_MAX_HEIGHT))
                    .overflow_y_scrollbar()
                    .child(rule_list),
            )
            .when_some(selected_saved_rule, |sidebar, trigger| {
                let duplicate_id = trigger.id.clone();
                let delete_id = trigger.id.clone();
                let copy_suffix = self
                    .i18n
                    .t("settings_view.terminal.triggers.duplicate_rule");
                sidebar.child(
                    div()
                        .w_full()
                        .pt(px(10.0))
                        .border_t_1()
                        .border_color(rgb(self.tokens.ui.border))
                        .flex()
                        .flex_wrap()
                        .gap(px(6.0))
                        .child(
                            self.terminal_trigger_button(
                                self.i18n
                                    .t("settings_view.terminal.triggers.duplicate_rule"),
                                ButtonVariant::Outline,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.clear_terminal_trigger_input_focus();
                                    this.terminal_triggers
                                        .duplicate_rule(&duplicate_id, &copy_suffix);
                                    cx.notify();
                                }),
                            ),
                        )
                        .child(self.terminal_trigger_button(
                            self.i18n.t("settings_view.terminal.triggers.delete_rule"),
                            ButtonVariant::Destructive,
                            cx.listener(move |this, _event, _window, cx| {
                                let changed = this.terminal_triggers.delete_rule(&delete_id);
                                this.persist_terminal_trigger_change(changed, cx);
                            }),
                        )),
                )
            });

        let content = if self.terminal_triggers.scope_picker_open {
            self.terminal_trigger_scope_picker(cx)
        } else if let Some(editor) = self.terminal_triggers.editor.as_ref() {
            self.terminal_trigger_editor(editor, cx)
        } else {
            div()
                .w_full()
                .py(px(24.0))
                .text_size(px(self.tokens.metrics.ui_text_sm))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(self.i18n.t("settings_view.terminal.triggers.no_rules"))
                .into_any_element()
        };

        // WindTerm-style master-detail keeps navigation stable while a rule is edited.
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_row()
            .items_start()
            .gap(px(20.0))
            .child(sidebar)
            .child(div().flex_1().min_w(px(0.0)).child(content))
            .into_any_element()
    }

    fn terminal_trigger_session_card(&self, pane_id: PaneId, cx: &mut Context<Self>) -> AnyElement {
        let pane_exists = self.tab_host.read(cx).panes().contains_key(&pane_id);
        let mut rows = Vec::new();
        if !pane_exists {
            rows.push(
                div()
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(
                        self.i18n
                            .t("settings_view.terminal.triggers.session_closed"),
                    )
                    .into_any_element(),
            );
        } else {
            let applicable_triggers = self
                .terminal_triggers
                .snapshot
                .triggers
                .iter()
                .filter(|trigger| {
                    trigger.enabled && self.terminal_trigger_applies_to_pane(pane_id, trigger, cx)
                })
                .collect::<Vec<_>>();
            if applicable_triggers.is_empty() {
                rows.push(
                    div()
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(
                            self.i18n
                                .t("settings_view.terminal.triggers.no_applicable_rules"),
                        )
                        .into_any_element(),
                );
            } else {
                for trigger in applicable_triggers {
                    rows.push(self.terminal_trigger_session_rule_row(pane_id, trigger, cx));
                    rows.push(self.card_separator());
                }
            }
        }
        rows.push(
            div()
                .w_full()
                .flex()
                .justify_end()
                .child(self.terminal_trigger_button(
                    self.i18n.t("settings_view.terminal.triggers.manage_global"),
                    ButtonVariant::Outline,
                    cx.listener(|this, _event, _window, cx| {
                        this.terminal_trigger_settings_pane = None;
                        cx.notify();
                    }),
                ))
                .into_any_element(),
        );
        self.settings_card(
            "settings_view.terminal.triggers.session_title",
            "settings_view.terminal.triggers.session_description",
            rows,
        )
    }

    fn terminal_trigger_button(
        &self,
        label: String,
        variant: ButtonVariant,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        // Shared button styling keeps primary, secondary, and destructive actions distinct.
        self.workspace_toolbar_action_button(
            label,
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                has_background: true,
                height: Some(30.0),
                padding_x: Some(12.0),
                ..ToolbarButtonOptions::default()
            },
            listener,
        )
    }

    fn terminal_trigger_shell_trust_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let enabled = self
            .settings_store
            .settings()
            .terminal
            .triggers
            .explicit_shell_enabled;
        let mut row = div()
            .w_full()
            .max_w(px(TERMINAL_TRIGGER_EDITOR_MAX_WIDTH))
            .rounded(px(self.tokens.radii.md))
            .bg(self.settings_panel_background(self.tokens.ui.bg_panel))
            .p(px(12.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .gap(px(3.0))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(
                                        self.i18n
                                            .t("settings_view.terminal.triggers.shell_execution"),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .child(
                                        self.i18n.t(
                                            "settings_view.terminal.triggers.shell_execution_hint",
                                        ),
                                    ),
                            ),
                    )
                    .child(
                        checkbox(&self.tokens, String::new(), enabled).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                if enabled {
                                    this.persist_terminal_trigger_shell_execution(false, cx);
                                } else {
                                    this.terminal_trigger_shell_confirmation_pending = true;
                                    cx.notify();
                                }
                            }),
                        ),
                    ),
            );
        if self.terminal_trigger_shell_confirmation_pending && !enabled {
            row = row.child(
                div()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.error))
                    .p(px(10.0))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(self.tokens.ui.error))
                            .child(
                                self.i18n
                                    .t("settings_view.terminal.triggers.shell_confirm_title"),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(
                                self.i18n
                                    .t("settings_view.terminal.triggers.shell_confirm_description"),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.0))
                            .child(self.terminal_trigger_button(
                                self.i18n.t("settings_view.terminal.triggers.cancel"),
                                ButtonVariant::Outline,
                                cx.listener(|this, _event, _window, cx| {
                                    this.terminal_trigger_shell_confirmation_pending = false;
                                    cx.notify();
                                }),
                            ))
                            .child(self.terminal_trigger_button(
                                self.i18n.t("settings_view.terminal.triggers.shell_confirm"),
                                ButtonVariant::Destructive,
                                cx.listener(|this, _event, _window, cx| {
                                    this.persist_terminal_trigger_shell_execution(true, cx);
                                }),
                            )),
                    ),
            );
        }
        row.into_any_element()
    }

    fn terminal_trigger_rule_row(
        &self,
        trigger: &TerminalTrigger,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let edit_id = trigger.id.clone();
        let toggle_id = trigger.id.clone();
        let selected = self
            .terminal_triggers
            .editor
            .as_ref()
            .is_some_and(|editor| editor.id == trigger.id);
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(px(self.tokens.radii.md))
            .p(px(8.0))
            .cursor_pointer()
            .when(selected, |row| {
                row.bg(rgba((self.tokens.ui.accent << 8) | 0x1f))
            })
            .when(!selected, |row| {
                row.hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.clear_terminal_trigger_input_focus();
                    this.terminal_triggers.edit_rule(&edit_id);
                    cx.notify();
                }),
            )
            .child(
                div()
                    .w(px(2.0))
                    .h(px(28.0))
                    .rounded_full()
                    .when(selected, |marker| marker.bg(rgb(self.tokens.ui.accent))),
            )
            .child(
                checkbox(&self.tokens, String::new(), trigger.enabled).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        let changed = this.terminal_triggers.toggle_rule(&toggle_id);
                        this.persist_terminal_trigger_change(changed, cx);
                        cx.stop_propagation();
                    }),
                ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(trigger.name.clone()),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(trigger.matcher.pattern.clone()),
                    ),
            )
            .into_any_element()
    }

    fn terminal_trigger_session_rule_row(
        &self,
        pane_id: PaneId,
        trigger: &TerminalTrigger,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let trigger_id = trigger.id.clone();
        let checked = self.terminal_trigger_effective_enabled_for_pane(pane_id, &trigger.id, cx);
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .child(
                        div()
                            .text_color(rgb(self.tokens.ui.text))
                            .child(trigger.name.clone()),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(trigger.matcher.pattern.clone()),
                    ),
            )
            .child(
                checkbox(&self.tokens, String::new(), checked).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.toggle_terminal_trigger_for_pane(pane_id, &trigger_id, cx);
                        cx.notify();
                    }),
                ),
            )
            .into_any_element()
    }

    fn terminal_trigger_editor(
        &self,
        trigger: &TerminalTrigger,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let action_label = match &trigger.action {
            TerminalTriggerAction::SendText { .. } => {
                "settings_view.terminal.triggers.action_send_text"
            }
            TerminalTriggerAction::RunQuickCommand { .. } => {
                "settings_view.terminal.triggers.action_quick_command"
            }
            TerminalTriggerAction::LaunchLocalProcess { .. } => {
                "settings_view.terminal.triggers.action_local_process"
            }
        };
        let editor_title = if trigger.name.trim().is_empty() {
            self.i18n.t("settings_view.terminal.triggers.add_rule")
        } else {
            trigger.name.clone()
        };
        let details = div()
            .w_full()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(px(12.0))
            .child(self.terminal_trigger_responsive_field(
                TERMINAL_TRIGGER_FIELD_BASIS,
                self.terminal_trigger_labeled_input(
                    "settings_view.terminal.triggers.name",
                    SettingsInput::TerminalTriggerName,
                    trigger.name.clone(),
                    cx,
                ),
            ))
            .child(self.terminal_trigger_responsive_field(
                TERMINAL_TRIGGER_FIELD_BASIS,
                self.terminal_trigger_labeled_input(
                    "settings_view.terminal.triggers.description_label",
                    SettingsInput::TerminalTriggerDescription,
                    trigger.description.clone().unwrap_or_default(),
                    cx,
                ),
            ));

        let match_options = div()
            .w_full()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_end()
            .gap(px(12.0))
            .child(self.terminal_trigger_responsive_field(
                TERMINAL_TRIGGER_COMPACT_FIELD_BASIS,
                self.terminal_trigger_select_field(
                    self.i18n.t("settings_view.terminal.triggers.match_mode"),
                    self.i18n.t(match trigger.matcher.mode {
                        TerminalTriggerMatchMode::Literal => {
                            "settings_view.terminal.triggers.literal"
                        }
                        TerminalTriggerMatchMode::Regex => "settings_view.terminal.triggers.regex",
                    }),
                    SettingsSelect::TerminalTriggerMatchMode,
                    cx,
                ),
            ))
            .child(
                div()
                    .flex_1()
                    .min_w(px(220.0))
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap(px(8.0))
                    .child(self.terminal_trigger_bool_option(
                        "settings_view.terminal.triggers.case_sensitive",
                        trigger.matcher.case_sensitive,
                        cx.listener(|this, _event, _window, cx| {
                            if let Some(trigger) = this.terminal_triggers.editor.as_mut() {
                                trigger.matcher.case_sensitive = !trigger.matcher.case_sensitive;
                            }
                            cx.notify();
                        }),
                    ))
                    .child(self.terminal_trigger_bool_option(
                        "settings_view.terminal.triggers.whole_word",
                        trigger.matcher.whole_word,
                        cx.listener(|this, _event, _window, cx| {
                            if let Some(trigger) = this.terminal_triggers.editor.as_mut() {
                                trigger.matcher.whole_word = !trigger.matcher.whole_word;
                            }
                            cx.notify();
                        }),
                    )),
            );

        let mut action_fields = div().w_full().flex().flex_col().gap(px(10.0)).child(
            self.terminal_trigger_compact_field(
                TERMINAL_TRIGGER_COMPACT_FIELD_BASIS,
                self.terminal_trigger_select_field(
                    self.i18n.t("settings_view.terminal.triggers.action"),
                    self.i18n.t(action_label),
                    SettingsSelect::TerminalTriggerAction,
                    cx,
                ),
            ),
        );

        action_fields = match &trigger.action {
            TerminalTriggerAction::SendText { append_enter, .. } => action_fields
                .child(
                    self.terminal_trigger_labeled_input(
                        "settings_view.terminal.triggers.text",
                        SettingsInput::TerminalTriggerActionValue,
                        self.terminal_triggers
                            .editor_input_value(SettingsInput::TerminalTriggerActionValue)
                            .unwrap_or_default(),
                        cx,
                    ),
                )
                .child(self.terminal_trigger_bool_option(
                    "settings_view.terminal.triggers.append_enter",
                    *append_enter,
                    cx.listener(|this, _event, _window, cx| {
                        if let Some(trigger) = this.terminal_triggers.editor.as_mut()
                            && let TerminalTriggerAction::SendText { append_enter, .. } =
                                &mut trigger.action
                        {
                            *append_enter = !*append_enter;
                        }
                        cx.notify();
                    }),
                )),
            TerminalTriggerAction::RunQuickCommand { .. } => {
                action_fields.child(self.terminal_trigger_quick_command_option(cx))
            }
            TerminalTriggerAction::LaunchLocalProcess { process } => {
                let shell_mode = matches!(process, LocalProcessSpec::ExplicitShell { .. });
                action_fields
                    .child(
                        self.terminal_trigger_compact_field(
                            TERMINAL_TRIGGER_COMPACT_FIELD_BASIS,
                            self.terminal_trigger_select_field(
                                self.i18n
                                    .t("settings_view.terminal.triggers.action_local_process"),
                                self.i18n.t(if shell_mode {
                                    "settings_view.terminal.triggers.explicit_shell"
                                } else {
                                    "settings_view.terminal.triggers.direct_program"
                                }),
                                SettingsSelect::TerminalTriggerProcessMode,
                                cx,
                            ),
                        ),
                    )
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .gap(px(12.0))
                            .child(
                                self.terminal_trigger_responsive_field(
                                    TERMINAL_TRIGGER_FIELD_BASIS,
                                    self.terminal_trigger_labeled_input(
                                        "settings_view.terminal.triggers.executable",
                                        SettingsInput::TerminalTriggerExecutable,
                                        self.terminal_triggers
                                            .editor_input_value(
                                                SettingsInput::TerminalTriggerExecutable,
                                            )
                                            .unwrap_or_default(),
                                        cx,
                                    ),
                                ),
                            )
                            .child(
                                self.terminal_trigger_responsive_field(
                                    TERMINAL_TRIGGER_FIELD_BASIS,
                                    self.terminal_trigger_labeled_input(
                                        "settings_view.terminal.triggers.working_directory",
                                        SettingsInput::TerminalTriggerWorkingDirectory,
                                        self.terminal_triggers
                                            .editor_input_value(
                                                SettingsInput::TerminalTriggerWorkingDirectory,
                                            )
                                            .unwrap_or_default(),
                                        cx,
                                    ),
                                ),
                            ),
                    )
                    .child(
                        self.terminal_trigger_labeled_input(
                            "settings_view.terminal.triggers.arguments",
                            SettingsInput::TerminalTriggerArguments,
                            self.terminal_triggers
                                .editor_input_value(SettingsInput::TerminalTriggerArguments)
                                .unwrap_or_default(),
                            cx,
                        ),
                    )
            }
        };

        let timing_fields = div()
            .w_full()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_end()
            .gap(px(12.0))
            .child(self.terminal_trigger_responsive_field(
                TERMINAL_TRIGGER_FIELD_BASIS,
                self.terminal_trigger_select_field(
                    self.i18n.t("settings_view.terminal.triggers.timing"),
                    self.i18n.t(match trigger.timing.dispatch {
                        TerminalTriggerDispatch::Immediate => {
                            "settings_view.terminal.triggers.immediate"
                        }
                        TerminalTriggerDispatch::AfterNextLineBreak => {
                            "settings_view.terminal.triggers.after_next_line_break"
                        }
                    }),
                    SettingsSelect::TerminalTriggerTiming,
                    cx,
                ),
            ))
            .child(self.terminal_trigger_responsive_field(
                TERMINAL_TRIGGER_COMPACT_FIELD_BASIS,
                self.terminal_trigger_labeled_input(
                    "settings_view.terminal.triggers.delay_ms",
                    SettingsInput::TerminalTriggerDelayMs,
                    trigger.timing.delay_ms.to_string(),
                    cx,
                ),
            ))
            .child(self.terminal_trigger_responsive_field(
                TERMINAL_TRIGGER_COMPACT_FIELD_BASIS,
                self.terminal_trigger_labeled_input(
                    "settings_view.terminal.triggers.cooldown_ms",
                    SettingsInput::TerminalTriggerCooldownMs,
                    trigger.timing.cooldown_ms.to_string(),
                    cx,
                ),
            ));

        div()
            .w_full()
            .max_w(px(TERMINAL_TRIGGER_EDITOR_MAX_WIDTH))
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(14.0))
            .child(
                div().w_full().flex().items_center().child(
                    div()
                        .min_w(px(0.0))
                        .truncate()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(self.tokens.ui.text))
                        .child(editor_title),
                ),
            )
            .child(details)
            .child(self.card_separator())
            .child(self.terminal_trigger_labeled_input(
                "settings_view.terminal.triggers.pattern",
                SettingsInput::TerminalTriggerPattern,
                trigger.matcher.pattern.clone(),
                cx,
            ))
            .child(match_options)
            .child(self.card_separator())
            .child(action_fields)
            .child(self.card_separator())
            .child(timing_fields)
            .child(self.terminal_trigger_scope_editor(trigger, cx))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(self.terminal_trigger_button(
                        self.i18n.t("settings_view.terminal.triggers.cancel"),
                        ButtonVariant::Outline,
                        cx.listener(|this, _event, _window, cx| {
                            this.clear_terminal_trigger_input_focus();
                            this.terminal_triggers.cancel_edit();
                            cx.notify();
                        }),
                    ))
                    .child(self.terminal_trigger_button(
                        self.i18n.t("settings_view.terminal.triggers.save"),
                        ButtonVariant::Default,
                        cx.listener(|this, _event, _window, cx| {
                            if let Some(input) = this
                                .focused_settings_input
                                .filter(|input| is_terminal_trigger_input(*input))
                            {
                                let input_draft = this.settings_input_draft.clone();
                                if !this.apply_terminal_trigger_settings_input(input, &input_draft)
                                {
                                    this.terminal_triggers.error_key =
                                        Some("settings_view.terminal.triggers.validation_failed");
                                    cx.notify();
                                    return;
                                }
                            }
                            let shell_enabled = this
                                .settings_store
                                .settings()
                                .terminal
                                .triggers
                                .explicit_shell_enabled;
                            let changed = this.terminal_triggers.save_editor(shell_enabled);
                            if changed {
                                this.clear_terminal_trigger_input_focus();
                            }
                            this.persist_terminal_trigger_change(changed, cx);
                        }),
                    )),
            )
            .into_any_element()
    }

    fn terminal_trigger_labeled_input(
        &self,
        label_key: &'static str,
        input: SettingsInput,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(5.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(label_key)),
            )
            .child(self.settings_text_input_control_fill(input, value, self.i18n.t(label_key), cx))
            .into_any_element()
    }

    fn terminal_trigger_responsive_field(
        &self,
        preferred_width: f32,
        field: AnyElement,
    ) -> AnyElement {
        // Responsive field slots stay compact on wide panes and wrap cleanly on narrow panes.
        div()
            .max_w_full()
            .min_w(px(0.0))
            .flex_1()
            .flex_basis(px(preferred_width))
            .child(field)
            .into_any_element()
    }

    fn terminal_trigger_compact_field(
        &self,
        preferred_width: f32,
        field: AnyElement,
    ) -> AnyElement {
        // Standalone choice fields keep their labels close to the control on wide panes.
        div()
            .w(px(preferred_width))
            .max_w_full()
            .min_w(px(0.0))
            .child(field)
            .into_any_element()
    }

    fn terminal_trigger_select_field(
        &self,
        label: String,
        value: String,
        select_id: SettingsSelect,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .flex()
            .flex_col()
            .items_start()
            .gap(px(5.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(label),
            )
            .child(self.settings_select_control(select_id, value, false, None, cx))
            .into_any_element()
    }

    fn terminal_trigger_bool_option(
        &self,
        label_key: &'static str,
        checked: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        div()
            .h(px(30.0))
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(7.0))
            .rounded(px(self.tokens.radii.md))
            .bg(self.settings_panel_background(self.tokens.ui.bg_panel))
            .child(
                checkbox(&self.tokens, String::new(), checked)
                    .on_mouse_down(MouseButton::Left, listener),
            )
            .child(
                div()
                    .whitespace_nowrap()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t(label_key)),
            )
            .into_any_element()
    }

    fn toggle_terminal_trigger_process_mode(&mut self) {
        let Some(trigger) = self.terminal_triggers.editor.as_mut() else {
            return;
        };
        let TerminalTriggerAction::LaunchLocalProcess { process } = &mut trigger.action else {
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

    fn terminal_trigger_quick_command_option(&self, cx: &mut Context<Self>) -> AnyElement {
        let quick_commands = &self.terminal.read(cx).quick_commands.store.commands;
        let selected_id = self
            .terminal_triggers
            .editor
            .as_ref()
            .and_then(|trigger| match &trigger.action {
                TerminalTriggerAction::RunQuickCommand { quick_command_id } => {
                    Some(quick_command_id.as_str())
                }
                _ => None,
            })
            .unwrap_or_default();
        let selected_label = quick_commands
            .iter()
            .find(|command| command.id == selected_id)
            .map(|command| command.name.clone())
            .unwrap_or_else(|| self.i18n.t("settings_view.terminal.triggers.quick_command"));
        self.terminal_trigger_compact_field(
            TERMINAL_TRIGGER_FIELD_BASIS,
            self.terminal_trigger_select_field(
                self.i18n.t("settings_view.terminal.triggers.quick_command"),
                selected_label,
                SettingsSelect::TerminalTriggerQuickCommand,
                cx,
            ),
        )
    }

    fn terminal_trigger_scope_editor(
        &self,
        trigger: &TerminalTrigger,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let scope_label = match &trigger.scope {
            TerminalTriggerScope::AllTerminals => "settings_view.terminal.triggers.all_terminals",
            TerminalTriggerScope::LocalTerminals => {
                "settings_view.terminal.triggers.local_terminals"
            }
            TerminalTriggerScope::SavedConnections { .. } => {
                "settings_view.terminal.triggers.saved_connections"
            }
        };
        self.terminal_trigger_compact_field(
            TERMINAL_TRIGGER_FIELD_BASIS,
            self.terminal_trigger_select_field(
                self.i18n.t("settings_view.terminal.triggers.scope"),
                self.i18n.t(scope_label),
                SettingsSelect::TerminalTriggerScope,
                cx,
            ),
        )
    }

    fn terminal_trigger_scope_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let options = self.terminal_trigger_saved_connection_options();
        let total_count = options.len();
        let selected_count = self
            .terminal_triggers
            .editor
            .as_ref()
            .and_then(|trigger| match &trigger.scope {
                TerminalTriggerScope::SavedConnections { connections } => Some(connections.len()),
                _ => None,
            })
            .unwrap_or(0);
        let mut list = div()
            .w_full()
            .max_h(px(TERMINAL_TRIGGER_RULE_LIST_MAX_HEIGHT))
            .overflow_y_scrollbar()
            .flex()
            .flex_col()
            .gap(px(4.0));
        if options.is_empty() {
            list = list.child(
                div()
                    .py(px(20.0))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(
                        self.i18n
                            .t("settings_view.terminal.triggers.no_saved_connections"),
                    ),
            );
        } else {
            for (reference, label) in options {
                let checked = self
                    .terminal_triggers
                    .editor
                    .as_ref()
                    .is_some_and(|trigger| match &trigger.scope {
                        TerminalTriggerScope::SavedConnections { connections } => {
                            connections.contains(&reference)
                        }
                        _ => false,
                    });
                let toggled = reference.clone();
                list = list.child(
                    div()
                        .w_full()
                        .min_w(px(0.0))
                        .p(px(10.0))
                        .rounded(px(self.tokens.radii.md))
                        .bg(self.settings_panel_background(self.tokens.ui.bg_panel))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(12.0))
                        .child(
                            div()
                                .min_w(px(0.0))
                                .truncate()
                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                .text_color(rgb(self.tokens.ui.text))
                                .child(label),
                        )
                        .child(
                            checkbox(&self.tokens, String::new(), checked).on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    let Some(TerminalTriggerScope::SavedConnections {
                                        connections,
                                    }) = this
                                        .terminal_triggers
                                        .editor
                                        .as_mut()
                                        .map(|trigger| &mut trigger.scope)
                                    else {
                                        return;
                                    };
                                    if let Some(index) = connections
                                        .iter()
                                        .position(|candidate| candidate == &toggled)
                                    {
                                        connections.remove(index);
                                    } else {
                                        connections.push(toggled.clone());
                                    }
                                    cx.notify();
                                }),
                            ),
                        ),
                );
            }
        }

        div()
            .w_full()
            .max_w(px(TERMINAL_TRIGGER_EDITOR_MAX_WIDTH))
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(14.0))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(
                                        self.i18n
                                            .t("settings_view.terminal.triggers.saved_connections"),
                                    ),
                            )
                            .child(self.text_badge(
                                format!("{selected_count} / {total_count}"),
                                self.tokens.ui.accent,
                            )),
                    )
                    .child(
                        self.terminal_trigger_button(
                            self.i18n
                                .t("settings_view.terminal.triggers.scope_picker_done"),
                            ButtonVariant::Default,
                            cx.listener(|this, _event, _window, cx| {
                                this.terminal_triggers.scope_picker_open = false;
                                cx.notify();
                            }),
                        ),
                    ),
            )
            .child(self.card_separator())
            .child(list)
            .into_any_element()
    }

    fn terminal_trigger_saved_connection_options(&self) -> Vec<(SavedConnectionRef, String)> {
        let mut options = Vec::new();
        options.extend(
            self.connection_store
                .connections()
                .iter()
                .map(|connection| {
                    (
                        SavedConnectionRef {
                            kind: SavedConnectionKind::Ssh,
                            id: connection.id.clone(),
                        },
                        format!("SSH · {}", connection.name),
                    )
                }),
        );
        options.extend(
            self.connection_store
                .telnet_profiles()
                .iter()
                .map(|profile| {
                    (
                        SavedConnectionRef {
                            kind: SavedConnectionKind::Telnet,
                            id: profile.id.clone(),
                        },
                        format!("Telnet · {}", profile.name),
                    )
                }),
        );
        options.extend(self.connection_store.mosh_profiles().iter().map(|profile| {
            (
                SavedConnectionRef {
                    kind: SavedConnectionKind::Mosh,
                    id: profile.id.clone(),
                },
                format!("Mosh · {}", profile.name),
            )
        }));
        options.extend(
            self.connection_store
                .serial_profiles()
                .iter()
                .map(|profile| {
                    (
                        SavedConnectionRef {
                            kind: SavedConnectionKind::Serial,
                            id: profile.id.clone(),
                        },
                        format!("Serial · {}", profile.name),
                    )
                }),
        );
        options
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
        assert_eq!(
            load_snapshot(&state.settings_path)
                .expect("saved trigger snapshot")
                .triggers
                .len(),
            1
        );
    }

    #[test]
    fn invalid_editor_keeps_the_last_saved_snapshot() {
        let (_directory, mut state) = state();
        state.new_rule();
        let trigger = state.editor.as_mut().expect("trigger editor");
        trigger.name = "Broken regex".to_string();
        trigger.matcher.mode = TerminalTriggerMatchMode::Regex;
        trigger.matcher.pattern = "(".to_string();
        trigger.action = TerminalTriggerAction::SendText {
            text: "ignored".to_string(),
            append_enter: false,
        };

        assert!(!state.save_editor(false));
        assert!(state.snapshot.triggers.is_empty());
        assert!(state.editor.is_some());
        assert_eq!(
            state.error_key,
            Some("settings_view.terminal.triggers.validation_failed")
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
}
