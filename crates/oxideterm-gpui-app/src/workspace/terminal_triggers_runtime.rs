use std::{collections::HashMap, path::Path, process::Stdio, time::Duration};

use gpui::{AnyElement, App, Context, EntityId, Task, Timer, WeakEntity, div, prelude::*};
use oxideterm_gpui_terminal::TerminalPane;
use oxideterm_quick_commands::{QuickCommandRisk, prepare_quick_command};
use oxideterm_terminal::TerminalSessionKind;
use oxideterm_terminal_triggers::{
    ExpandedLocalProcessSpec, ExpandedTriggerAction, SavedConnectionKind, SavedConnectionRef,
    TerminalTrigger, TerminalTriggerScope, TerminalTriggersSnapshot, TriggerMatched,
    compile_active, expand_template,
};
use tokio::process::Command;
use zeroize::Zeroizing;

use super::{
    ConfirmDialogVariant, ConfirmDialogView, PaneId, TerminalNotice, TerminalNoticeVariant,
    TerminalSessionId, WorkspaceApp,
};

#[cfg(windows)]
const TERMINAL_TRIGGER_PROCESS_CREATE_NO_WINDOW: u32 = 0x08000000;
const MAX_PENDING_TRIGGER_ACTIONS: usize = 128;

pub(super) struct TerminalTriggerRuntimeState {
    generation: u64,
    next_action_id: u64,
    session_overrides: HashMap<TerminalSessionId, HashMap<String, bool>>,
    delayed_tasks: HashMap<u64, Task<()>>,
    process_tasks: HashMap<u64, LocalTriggerProcessTask>,
    pending_quick_command: Option<PendingTriggerQuickCommand>,
}

impl Default for TerminalTriggerRuntimeState {
    fn default() -> Self {
        Self {
            generation: 0,
            next_action_id: 1,
            session_overrides: HashMap::new(),
            delayed_tasks: HashMap::new(),
            process_tasks: HashMap::new(),
            pending_quick_command: None,
        }
    }
}

#[derive(Clone)]
struct TerminalTriggerActionTarget {
    pane_id: PaneId,
    session_id: TerminalSessionId,
    pane: WeakEntity<TerminalPane>,
}

impl TerminalTriggerActionTarget {
    fn entity_id(&self) -> Option<EntityId> {
        self.pane.upgrade().map(|pane| pane.entity_id())
    }
}

struct PendingTriggerQuickCommand {
    target: TerminalTriggerActionTarget,
    trigger_id: String,
    generation: u64,
    quick_command_id: String,
    quick_command_updated_at: u64,
    command: Zeroizing<String>,
    rule_name: String,
    quick_command_name: String,
    risk: Option<QuickCommandRisk>,
}

struct LocalTriggerProcessTask {
    _completion: Task<()>,
    abort_handle: tokio::task::AbortHandle,
}

impl Drop for LocalTriggerProcessTask {
    fn drop(&mut self) {
        // Workspace shutdown owns cancellation of every process it launched.
        self.abort_handle.abort();
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn terminal_trigger_quick_command_pending(&self) -> bool {
        self.terminal_trigger_runtime
            .pending_quick_command
            .is_some()
    }

    pub(in crate::workspace) fn handle_terminal_trigger_matches(
        &mut self,
        pane_id: PaneId,
        session_id: TerminalSessionId,
        pane: WeakEntity<TerminalPane>,
        matches: Vec<TriggerMatched>,
        cx: &mut Context<Self>,
    ) {
        let target = TerminalTriggerActionTarget {
            pane_id,
            session_id,
            pane,
        };
        for matched in matches {
            if matched.generation() != self.terminal_trigger_runtime.generation {
                continue;
            }
            if matched.delay_ms() == 0 {
                self.execute_terminal_trigger_match(target.clone(), matched, cx);
                continue;
            }
            if self.terminal_trigger_runtime.delayed_tasks.len() >= MAX_PENDING_TRIGGER_ACTIONS {
                continue;
            }
            let action_id = self.next_terminal_trigger_action_id();
            let delay = Duration::from_millis(matched.delay_ms());
            let target = target.clone();
            let task = cx.spawn(async move |weak, cx| {
                Timer::after(delay).await;
                let _ = weak.update(cx, |workspace, cx| {
                    workspace.execute_terminal_trigger_match(target, matched, cx);
                    workspace
                        .terminal_trigger_runtime
                        .delayed_tasks
                        .remove(&action_id);
                });
            });
            self.terminal_trigger_runtime
                .delayed_tasks
                .insert(action_id, task);
        }
    }

    fn next_terminal_trigger_action_id(&mut self) -> u64 {
        let action_id = self.terminal_trigger_runtime.next_action_id;
        self.terminal_trigger_runtime.next_action_id = action_id.wrapping_add(1).max(1);
        action_id
    }

    fn execute_terminal_trigger_match(
        &mut self,
        target: TerminalTriggerActionTarget,
        matched: TriggerMatched,
        cx: &mut Context<Self>,
    ) {
        if !self.terminal_trigger_target_is_live(&target, cx)
            || matched.generation() != self.terminal_trigger_runtime.generation
        {
            return;
        }
        let Some(trigger) = self
            .terminal_triggers
            .snapshot
            .triggers
            .iter()
            .find(|trigger| trigger.id == matched.trigger_id())
            .filter(|trigger| trigger.enabled)
            .cloned()
        else {
            return;
        };
        if !self.terminal_trigger_enabled_for_session(target.session_id, &trigger.id)
            || !self.terminal_trigger_applies_to_pane(target.pane_id, &trigger, cx)
        {
            return;
        }
        let Ok(action) = trigger.action.expand(&matched) else {
            return;
        };
        match action {
            ExpandedTriggerAction::SendText { text, append_enter } => {
                let _ = target.pane.update(cx, |pane, cx| {
                    pane.send_trigger_text(&text, append_enter, cx)
                });
            }
            ExpandedTriggerAction::RunQuickCommand { quick_command_id } => {
                self.run_terminal_trigger_quick_command(
                    target,
                    trigger.id,
                    trigger.name,
                    quick_command_id,
                    &matched,
                    cx,
                );
            }
            ExpandedTriggerAction::LaunchLocalProcess { process } => {
                self.launch_terminal_trigger_process(process, cx);
            }
        }
    }

    fn terminal_trigger_target_is_live(
        &self,
        target: &TerminalTriggerActionTarget,
        cx: &App,
    ) -> bool {
        let Some(target_entity_id) = target.entity_id() else {
            return false;
        };
        let tab_host = self.tab_host.read(cx);
        let registered_target_matches = tab_host
            .panes()
            .get(&target.pane_id)
            .is_some_and(|pane| pane.entity_id() == target_entity_id);
        registered_target_matches
            && tab_host
                .terminal_location(target.session_id)
                .is_some_and(|location| location.pane_id == target.pane_id)
    }

    fn terminal_trigger_enabled_for_session(
        &self,
        session_id: TerminalSessionId,
        trigger_id: &str,
    ) -> bool {
        self.terminal_trigger_runtime
            .session_overrides
            .get(&session_id)
            .and_then(|overrides| overrides.get(trigger_id))
            .copied()
            .unwrap_or(true)
    }

    pub(in crate::workspace) fn set_terminal_trigger_session_override(
        &mut self,
        session_id: TerminalSessionId,
        trigger_id: String,
        enabled: bool,
    ) {
        self.terminal_trigger_runtime
            .session_overrides
            .entry(session_id)
            .or_default()
            .insert(trigger_id, enabled);
    }

    pub(in crate::workspace) fn clear_terminal_trigger_session_overrides(
        &mut self,
        session_id: TerminalSessionId,
    ) {
        self.terminal_trigger_runtime
            .session_overrides
            .remove(&session_id);
    }

    pub(in crate::workspace) fn terminal_trigger_effective_enabled_for_pane(
        &self,
        pane_id: PaneId,
        trigger_id: &str,
        cx: &App,
    ) -> bool {
        let globally_enabled = self
            .terminal_triggers
            .snapshot
            .triggers
            .iter()
            .find(|trigger| trigger.id == trigger_id)
            .is_some_and(|trigger| trigger.enabled);
        if !globally_enabled {
            return false;
        }
        let Some(session_id) = self.session_id_for_pane(pane_id, cx) else {
            return false;
        };
        self.terminal_trigger_enabled_for_session(session_id, trigger_id)
    }

    pub(in crate::workspace) fn terminal_trigger_applies_to_pane(
        &self,
        pane_id: PaneId,
        trigger: &TerminalTrigger,
        cx: &App,
    ) -> bool {
        let is_local = self
            .tab_host
            .read(cx)
            .panes()
            .get(&pane_id)
            .is_some_and(|pane| pane.read(cx).session_kind() == TerminalSessionKind::LocalPty);
        let saved_connection = self
            .session_id_for_pane(pane_id, cx)
            .and_then(|session_id| self.terminal_saved_connection_refs.get(&session_id));
        terminal_trigger_scope_applies(&trigger.scope, is_local, saved_connection)
    }

    pub(in crate::workspace) fn register_terminal_saved_connection(
        &mut self,
        session_id: TerminalSessionId,
        kind: SavedConnectionKind,
        id: String,
        cx: &mut Context<Self>,
    ) {
        self.terminal_saved_connection_refs
            .insert(session_id, SavedConnectionRef { kind, id });
        if let Some(location) = self.tab_host.read(cx).terminal_location(session_id) {
            self.refresh_terminal_trigger_pane(location.pane_id, cx);
        }
    }

    pub(in crate::workspace) fn refresh_terminal_trigger_runtime(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.terminal_trigger_runtime.generation = self
            .terminal_trigger_runtime
            .generation
            .wrapping_add(1)
            .max(1);
        self.terminal_trigger_runtime.delayed_tasks.clear();
        self.terminal_trigger_runtime.pending_quick_command = None;
        let pane_ids = self
            .tab_host
            .read(cx)
            .panes()
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for pane_id in pane_ids {
            self.refresh_terminal_trigger_pane(pane_id, cx);
        }
    }

    pub(in crate::workspace) fn shutdown_terminal_trigger_runtime(&mut self) {
        // Workspace release cancels timers and aborts every still-running local child.
        self.terminal_trigger_runtime.delayed_tasks.clear();
        self.terminal_trigger_runtime.pending_quick_command = None;
        self.terminal_trigger_runtime.process_tasks.clear();
    }

    pub(in crate::workspace) fn refresh_terminal_trigger_pane(
        &mut self,
        pane_id: PaneId,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.tab_host.read(cx).panes().get(&pane_id).cloned() else {
            return;
        };
        let Some(session_id) = self.session_id_for_pane(pane_id, cx) else {
            return;
        };
        let mut snapshot = TerminalTriggersSnapshot {
            version: self.terminal_triggers.snapshot.version,
            triggers: self
                .terminal_triggers
                .snapshot
                .triggers
                .iter()
                .filter(|trigger| {
                    trigger.enabled
                        && self.terminal_trigger_enabled_for_session(session_id, &trigger.id)
                        && self.terminal_trigger_applies_to_pane(pane_id, trigger, cx)
                })
                .cloned()
                .collect(),
            updated_at: self.terminal_triggers.snapshot.updated_at,
        };
        // Session overrides only filter validated global rules; they never mutate persistence.
        for trigger in &mut snapshot.triggers {
            trigger.enabled = true;
        }
        let rules = compile_active(&snapshot, self.terminal_trigger_runtime.generation)
            .ok()
            .flatten();
        pane.update(cx, |pane, _cx| pane.set_trigger_rules(rules));
    }

    pub(in crate::workspace) fn toggle_terminal_trigger_for_pane(
        &mut self,
        pane_id: PaneId,
        trigger_id: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(session_id) = self.session_id_for_pane(pane_id, cx) else {
            return;
        };
        let enabled = !self.terminal_trigger_effective_enabled_for_pane(pane_id, trigger_id, cx);
        self.set_terminal_trigger_session_override(session_id, trigger_id.to_string(), enabled);
        self.refresh_terminal_trigger_pane(pane_id, cx);
        cx.notify();
    }

    fn run_terminal_trigger_quick_command(
        &mut self,
        target: TerminalTriggerActionTarget,
        trigger_id: String,
        rule_name: String,
        quick_command_id: String,
        matched: &TriggerMatched,
        cx: &mut Context<Self>,
    ) {
        let Some(mut quick_command) = self
            .terminal
            .read(cx)
            .quick_commands
            .store
            .commands
            .iter()
            .find(|command| command.id == quick_command_id)
            .cloned()
        else {
            return;
        };
        let Ok(command_template) = expand_template(&quick_command.command, matched) else {
            return;
        };
        quick_command.command = command_template.to_string();
        let parameter_values = quick_command
            .parameters
            .iter()
            .filter_map(|parameter| {
                parameter
                    .default_value
                    .clone()
                    .map(|value| (parameter.name.clone(), zeroize::Zeroizing::new(value)))
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        let Some(context) = self.quick_command_context_for_pane(target.pane_id, cx) else {
            return;
        };
        let Ok(prepared) = prepare_quick_command(&quick_command, &[context], &parameter_values)
        else {
            self.push_workspace_notice(
                TerminalNotice {
                    title: self.i18n.t("terminal.triggers.quick_command_data_missing"),
                    description: None,
                    status_text: None,
                    progress: None,
                    variant: TerminalNoticeVariant::Error,
                },
                cx,
            );
            return;
        };
        let template_requires_confirmation = prepared.confirmation_required;
        let Some(prepared_target) = prepared.targets.into_iter().next() else {
            return;
        };
        let command = prepared_target.command;
        let risk = prepared_target.risk;
        let requires_confirmation = template_requires_confirmation
            || self
                .settings_store
                .settings()
                .terminal
                .command_bar
                .quick_commands_confirm_before_run;
        if requires_confirmation {
            if self
                .terminal_trigger_runtime
                .pending_quick_command
                .is_some()
            {
                return;
            }
            self.terminal_trigger_runtime.pending_quick_command =
                Some(PendingTriggerQuickCommand {
                    target,
                    trigger_id,
                    generation: matched.generation(),
                    quick_command_id,
                    quick_command_updated_at: quick_command.updated_at,
                    command,
                    rule_name,
                    quick_command_name: quick_command.name,
                    risk,
                });
            self.reset_standard_confirm_focus();
            cx.notify();
            return;
        }
        let _ = target
            .pane
            .update(cx, |pane, cx| pane.send_trigger_text(&command, true, cx));
    }

    pub(in crate::workspace) fn cancel_terminal_trigger_quick_command(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.terminal_trigger_runtime.pending_quick_command = None;
        self.clear_standard_confirm_focus();
        cx.notify();
    }

    pub(in crate::workspace) fn handle_terminal_trigger_quick_command_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        match self.handle_standard_confirm_key(event, cx) {
            Some(super::ConfirmKeyboardAction::Cancel) => {
                self.cancel_terminal_trigger_quick_command(cx);
                true
            }
            Some(super::ConfirmKeyboardAction::Confirm) => {
                self.confirm_terminal_trigger_quick_command(cx);
                true
            }
            Some(super::ConfirmKeyboardAction::Handled) => true,
            None => false,
        }
    }

    pub(in crate::workspace) fn confirm_terminal_trigger_quick_command(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(pending) = self.terminal_trigger_runtime.pending_quick_command.take() else {
            return;
        };
        self.clear_standard_confirm_focus();
        let quick_command_is_current = self
            .terminal
            .read(cx)
            .quick_commands
            .store
            .commands
            .iter()
            .any(|command| {
                command.id == pending.quick_command_id
                    && command.updated_at == pending.quick_command_updated_at
            });
        let current_trigger = self
            .terminal_triggers
            .snapshot
            .triggers
            .iter()
            .find(|trigger| trigger.id == pending.trigger_id && trigger.enabled)
            .cloned();
        let trigger_is_current = pending.generation == self.terminal_trigger_runtime.generation
            && current_trigger.as_ref().is_some_and(|trigger| {
                self.terminal_trigger_applies_to_pane(pending.target.pane_id, trigger, cx)
            });
        if quick_command_is_current
            && trigger_is_current
            && self.terminal_trigger_enabled_for_session(
                pending.target.session_id,
                &pending.trigger_id,
            )
            && self.terminal_trigger_target_is_live(&pending.target, cx)
        {
            let _ = pending.target.pane.update(cx, |pane, cx| {
                pane.send_trigger_text(&pending.command, true, cx)
            });
        }
        cx.notify();
    }

    pub(in crate::workspace) fn render_terminal_trigger_quick_command_confirm(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let pending = self
            .terminal_trigger_runtime
            .pending_quick_command
            .as_ref()?;
        let risk_key = match pending.risk {
            Some(QuickCommandRisk::High) => "terminal.triggers.risk_high",
            Some(QuickCommandRisk::Medium) => "terminal.triggers.risk_medium",
            None => "terminal.triggers.risk_configured",
        };
        let description = self
            .i18n
            .t("terminal.triggers.risky_quick_command_description")
            .replace("{{trigger}}", &pending.rule_name)
            .replace("{{command}}", &pending.quick_command_name)
            .replace("{{risk}}", &self.i18n.t(risk_key));
        Some(
            oxideterm_gpui_ui::confirm::confirm_dialog_with_focus(
                &self.tokens,
                ConfirmDialogView {
                    variant: ConfirmDialogVariant::Danger,
                    title: div()
                        .child(self.i18n.t("terminal.triggers.risky_quick_command_title"))
                        .into_any_element(),
                    description: Some(div().child(description).into_any_element()),
                    cancel_label: div()
                        .child(self.i18n.t("terminal.triggers.cancel"))
                        .into_any_element(),
                    confirm_label: div()
                        .child(self.i18n.t("terminal.triggers.run"))
                        .into_any_element(),
                },
                self.standard_confirm_focus(),
                cx.listener(|workspace, _event, _window, cx| {
                    workspace.cancel_terminal_trigger_quick_command(cx);
                    cx.stop_propagation();
                }),
                cx.listener(|workspace, _event, _window, cx| {
                    workspace.confirm_terminal_trigger_quick_command(cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element(),
        )
    }

    fn launch_terminal_trigger_process(
        &mut self,
        process: ExpandedLocalProcessSpec,
        cx: &mut Context<Self>,
    ) {
        if self.terminal_trigger_runtime.process_tasks.len() >= MAX_PENDING_TRIGGER_ACTIONS {
            return;
        }
        let (launch, explicit_shell) = LocalTriggerProcessLaunch::from_expanded(process);
        if explicit_shell
            && !self
                .settings_store
                .settings()
                .terminal
                .triggers
                .explicit_shell_enabled
        {
            return;
        }
        let action_id = self.next_terminal_trigger_action_id();
        let worker = self
            .forwarding_runtime
            .spawn(run_local_trigger_process(launch));
        let abort_handle = worker.abort_handle();
        let completion = cx.spawn(async move |weak, cx| {
            let result = worker.await;
            let _ = weak.update(cx, |workspace, cx| {
                workspace
                    .terminal_trigger_runtime
                    .process_tasks
                    .remove(&action_id);
                if !matches!(result, Ok(Ok(()))) {
                    workspace.push_workspace_notice(
                        TerminalNotice {
                            title: workspace.i18n.t("terminal.triggers.launch_failed"),
                            description: None,
                            status_text: None,
                            progress: None,
                            variant: TerminalNoticeVariant::Error,
                        },
                        cx,
                    );
                }
            });
        });
        self.terminal_trigger_runtime.process_tasks.insert(
            action_id,
            LocalTriggerProcessTask {
                _completion: completion,
                abort_handle,
            },
        );
    }
}

fn terminal_trigger_scope_applies(
    scope: &TerminalTriggerScope,
    is_local: bool,
    saved_connection: Option<&SavedConnectionRef>,
) -> bool {
    match scope {
        TerminalTriggerScope::AllTerminals => true,
        TerminalTriggerScope::LocalTerminals => is_local,
        TerminalTriggerScope::SavedConnections { connections } => {
            saved_connection.is_some_and(|source| connections.contains(source))
        }
    }
}

struct LocalTriggerProcessLaunch {
    executable: Zeroizing<String>,
    arguments: Vec<Zeroizing<String>>,
    working_directory: Option<Zeroizing<String>>,
}

impl LocalTriggerProcessLaunch {
    fn from_expanded(process: ExpandedLocalProcessSpec) -> (Self, bool) {
        match process {
            ExpandedLocalProcessSpec::DirectProgram {
                executable,
                arguments,
                working_directory,
            } => (
                Self {
                    executable,
                    arguments,
                    working_directory,
                },
                false,
            ),
            ExpandedLocalProcessSpec::ExplicitShell {
                shell_executable,
                arguments,
                working_directory,
            } => (
                Self {
                    executable: shell_executable,
                    arguments,
                    working_directory,
                },
                true,
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LocalTriggerProcessError {
    SpawnFailed,
    WaitFailed,
}

async fn run_local_trigger_process(
    launch: LocalTriggerProcessLaunch,
) -> Result<(), LocalTriggerProcessError> {
    let mut command = build_local_trigger_process_command(&launch);
    drop(launch);
    let mut child = command
        .spawn()
        .map_err(|_| LocalTriggerProcessError::SpawnFailed)?;
    // The builder owns unavoidable OS argument copies; release them immediately after spawn.
    drop(command);
    child
        .wait()
        .await
        .map_err(|_| LocalTriggerProcessError::WaitFailed)?;
    Ok(())
}

fn build_local_trigger_process_command(launch: &LocalTriggerProcessLaunch) -> Command {
    let mut command = Command::new(launch.executable.as_str());
    command
        .args(launch.arguments.iter().map(|argument| argument.as_str()))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    if let Some(working_directory) = launch
        .working_directory
        .as_deref()
        .filter(|working_directory| !working_directory.trim().is_empty())
    {
        command.current_dir(Path::new(working_directory));
    }
    configure_local_trigger_process(&mut command);
    command
}

fn configure_local_trigger_process(command: &mut Command) {
    #[cfg(windows)]
    {
        // Trigger processes run without a console owned by the GUI application.
        command.creation_flags(TERMINAL_TRIGGER_PROCESS_CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = command;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn launch(arguments: &[&str]) -> LocalTriggerProcessLaunch {
        LocalTriggerProcessLaunch {
            executable: Zeroizing::new("program".to_string()),
            arguments: arguments
                .iter()
                .map(|argument| Zeroizing::new((*argument).to_string()))
                .collect(),
            working_directory: None,
        }
    }

    #[test]
    fn direct_process_arguments_are_not_shell_split() {
        let launch = launch(&["value with spaces", "; touch unexpected", "$(whoami)"]);
        let command = build_local_trigger_process_command(&launch);
        let arguments = command
            .as_std()
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(
            arguments,
            ["value with spaces", "; touch unexpected", "$(whoami)"]
        );
    }

    #[test]
    fn empty_working_directory_is_not_applied() {
        let mut launch = launch(&[]);
        launch.working_directory = Some(Zeroizing::new("   ".to_string()));

        assert!(
            build_local_trigger_process_command(&launch)
                .as_std()
                .get_current_dir()
                .is_none()
        );
    }

    #[test]
    fn explicit_shell_mode_remains_distinct_from_direct_program_mode() {
        let direct = ExpandedLocalProcessSpec::DirectProgram {
            executable: Zeroizing::new("program".to_string()),
            arguments: Vec::new(),
            working_directory: None,
        };
        let shell = ExpandedLocalProcessSpec::ExplicitShell {
            shell_executable: Zeroizing::new("shell".to_string()),
            arguments: Vec::new(),
            working_directory: None,
        };

        assert!(!LocalTriggerProcessLaunch::from_expanded(direct).1);
        assert!(LocalTriggerProcessLaunch::from_expanded(shell).1);
    }

    #[test]
    fn local_process_errors_do_not_include_action_content() {
        let rendered = format!("{:?}", LocalTriggerProcessError::SpawnFailed);

        assert_eq!(rendered, "SpawnFailed");
        assert!(!rendered.contains("secret-value"));
    }

    #[test]
    fn trigger_scope_uses_protocol_qualified_saved_connection_identity() {
        let ssh = SavedConnectionRef {
            kind: SavedConnectionKind::Ssh,
            id: "shared-id".to_string(),
        };
        let telnet = SavedConnectionRef {
            kind: SavedConnectionKind::Telnet,
            id: "shared-id".to_string(),
        };
        let scope = TerminalTriggerScope::SavedConnections {
            connections: vec![ssh.clone()],
        };

        assert!(terminal_trigger_scope_applies(&scope, false, Some(&ssh)));
        assert!(!terminal_trigger_scope_applies(
            &scope,
            false,
            Some(&telnet)
        ));
        assert!(!terminal_trigger_scope_applies(&scope, true, None));
        assert!(terminal_trigger_scope_applies(
            &TerminalTriggerScope::LocalTerminals,
            true,
            None
        ));
    }
}
