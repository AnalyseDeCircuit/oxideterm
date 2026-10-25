use super::*;

fn integration_install_label(
    state: Option<oxideterm_terminal::RemoteShellIntegrationState>,
) -> &'static str {
    use oxideterm_terminal::RemoteShellIntegrationState;
    match state {
        Some(RemoteShellIntegrationState::NeedsUpdate) => {
            "settings_view.connections.shell_integration.update"
        }
        Some(RemoteShellIntegrationState::NeedsRepair | RemoteShellIntegrationState::FilesOnly) => {
            "settings_view.connections.shell_integration.repair"
        }
        Some(RemoteShellIntegrationState::Installed) => {
            "settings_view.connections.shell_integration.reinstall"
        }
        _ => "settings_view.connections.shell_integration.install",
    }
}

pub(in crate::workspace) struct RemoteShellIntegrationRuntimeState {
    node_id: Option<NodeId>,
    status: Option<RemoteShellIntegrationStatus>,
    failure: Option<RemoteShellIntegrationFailure>,
    confirm_node_id: Option<NodeId>,
    confirm_source: Option<RemoteShellIntegrationConfirmSource>,
    terminal_ready_nodes: HashSet<NodeId>,
    terminal_checking_nodes: HashMap<NodeId, (u64, bool)>,
    terminal_prompt_nodes: VecDeque<NodeId>,
    terminal_statuses: HashMap<NodeId, oxideterm_terminal::RemoteShellIntegrationState>,
    deferred: std::collections::BTreeMap<String, u32>,
    suppress_future_terminal_prompts: bool,
    mode: RemoteShellIntegrationMode,
    awareness_enabled: bool,
    next_generation: u64,
    maintenance: Option<(NodeId, u64)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum RemoteShellIntegrationConfirmSource {
    Toolbar,
    TerminalOpen,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum RemoteShellIntegrationAction {
    Inspect,
    Install,
    RemoveReference,
    RemoveAll,
}

#[derive(Clone)]
pub(in crate::workspace) struct RemoteShellIntegrationConfirmSnapshot {
    pub(in crate::workspace) node_id: NodeId,
    pub(in crate::workspace) source: RemoteShellIntegrationConfirmSource,
    pub(in crate::workspace) suppress_future_prompts: bool,
    pub(in crate::workspace) state: Option<oxideterm_terminal::RemoteShellIntegrationState>,
}

#[derive(Clone)]
pub(in crate::workspace) struct RemoteShellIntegrationCardSnapshot {
    pub(in crate::workspace) status: Option<RemoteShellIntegrationStatus>,
    pub(in crate::workspace) failure: Option<RemoteShellIntegrationFailure>,
    pub(in crate::workspace) pending: bool,
}

pub(in crate::workspace) enum RemoteShellIntegrationGateOutcome {
    Applied,
    Configured(NodeId, oxideterm_terminal::RemoteShellIntegrationState),
    RetryInstall(NodeId),
    Failed(RemoteShellIntegrationFailure),
    Stale,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum RemoteShellIntegrationNotice {
    Inspected,
    Configured(NodeId, oxideterm_terminal::RemoteShellIntegrationState),
    ReferenceRemoved,
    AllRemoved,
    CheckFailed(RemoteShellIntegrationFailure),
    Failed(RemoteShellIntegrationFailure),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum RemoteShellIntegrationFailure {
    ConnectionUnavailable,
    SftpUnavailable,
    Integration(oxideterm_terminal::RemoteShellIntegrationError),
    InstallationIncomplete,
}

impl Default for RemoteShellIntegrationRuntimeState {
    fn default() -> Self {
        Self {
            node_id: None,
            status: None,
            failure: None,
            confirm_node_id: None,
            confirm_source: None,
            terminal_ready_nodes: HashSet::new(),
            terminal_checking_nodes: HashMap::new(),
            terminal_prompt_nodes: VecDeque::new(),
            terminal_statuses: HashMap::new(),
            deferred: Default::default(),
            suppress_future_terminal_prompts: false,
            mode: RemoteShellIntegrationMode::Disabled,
            awareness_enabled: false,
            next_generation: 0,
            maintenance: None,
        }
    }
}

impl RemoteShellIntegrationRuntimeState {
    pub(in crate::workspace) fn configure(
        &mut self,
        mode: RemoteShellIntegrationMode,
        awareness_enabled: bool,
    ) {
        self.mode = mode;
        self.awareness_enabled = awareness_enabled;
        if mode == RemoteShellIntegrationMode::Disabled || !awareness_enabled {
            self.terminal_prompt_nodes.clear();
            if self.confirm_source == Some(RemoteShellIntegrationConfirmSource::TerminalOpen) {
                self.confirm_node_id = None;
                self.confirm_source = None;
                self.suppress_future_terminal_prompts = false;
            }
        }
    }

    pub(in crate::workspace) fn pending(&self) -> bool {
        self.maintenance.is_some() || !self.terminal_checking_nodes.is_empty()
    }

    pub(in crate::workspace) fn deployment_mode(&self) -> RemoteShellIntegrationMode {
        self.mode
    }

    pub(in crate::workspace) fn set_deferred(
        &mut self,
        deferred: std::collections::BTreeMap<String, u32>,
    ) {
        self.deferred = deferred;
    }

    pub(in crate::workspace) fn is_deferred(&self, host: &str) -> bool {
        self.mode == RemoteShellIntegrationMode::Ask
            && self.deferred.get(host)
                == Some(&oxideterm_terminal::REMOTE_SHELL_INTEGRATION_VERSION)
    }

    pub(in crate::workspace) fn cancel_terminal_gates(&mut self) {
        self.terminal_checking_nodes.clear();
    }

    pub(in crate::workspace) fn cancel_node(&mut self, node_id: &NodeId) {
        self.terminal_checking_nodes.remove(node_id);
        self.terminal_statuses.remove(node_id);
        self.terminal_ready_nodes.remove(node_id);
        self.terminal_prompt_nodes
            .retain(|queued| queued != node_id);
        if self.confirm_node_id.as_ref() == Some(node_id) {
            self.confirm_node_id = None;
            self.confirm_source = None;
            self.suppress_future_terminal_prompts = false;
            self.advance_terminal_prompt();
        }
        if self.node_id.as_ref() == Some(node_id) {
            self.node_id = None;
            self.status = None;
            self.failure = None;
        }
        if self
            .maintenance
            .as_ref()
            .is_some_and(|(current, _)| current == node_id)
        {
            self.maintenance = None;
        }
    }

    pub(in crate::workspace) fn begin_terminal_gate(
        &mut self,
        node_id: &NodeId,
        force_install: bool,
    ) -> Option<u64> {
        if !self.awareness_enabled
            || self.mode == RemoteShellIntegrationMode::Disabled
            || self.terminal_ready_nodes.contains(node_id)
            || self.terminal_checking_nodes.contains_key(node_id)
        {
            return None;
        }
        let generation = self.next_generation();
        self.terminal_checking_nodes
            .insert(node_id.clone(), (generation, force_install));
        Some(generation)
    }

    pub(in crate::workspace) fn finish_terminal_gate(
        &mut self,
        node_id: NodeId,
        generation: u64,
        result: std::result::Result<
            (
                RemoteShellIntegrationStatus,
                Option<oxideterm_terminal::RemoteShellIntegrationState>,
            ),
            RemoteShellIntegrationFailure,
        >,
    ) -> RemoteShellIntegrationGateOutcome {
        let Some((current_generation, force_install)) =
            self.terminal_checking_nodes.get(&node_id).copied()
        else {
            return RemoteShellIntegrationGateOutcome::Stale;
        };
        if current_generation != generation {
            return RemoteShellIntegrationGateOutcome::Stale;
        }
        self.terminal_checking_nodes.remove(&node_id);
        if let Ok((status, _)) = &result {
            self.terminal_statuses.insert(node_id.clone(), status.state);
        }
        match result {
            Ok((status, previous))
                if matches!(
                    status.state,
                    oxideterm_terminal::RemoteShellIntegrationState::Installed
                        | oxideterm_terminal::RemoteShellIntegrationState::NewerVersion
                ) =>
            {
                self.terminal_ready_nodes.insert(node_id.clone());
                self.terminal_prompt_nodes
                    .retain(|queued| queued != &node_id);
                self.node_id = Some(node_id.clone());
                self.status = Some(status);
                self.failure = None;
                match previous {
                    Some(previous) => {
                        RemoteShellIntegrationGateOutcome::Configured(node_id, previous)
                    }
                    None => RemoteShellIntegrationGateOutcome::Applied,
                }
            }
            Ok((status, _)) if self.mode == RemoteShellIntegrationMode::Ask => {
                self.node_id = Some(node_id.clone());
                self.status = Some(status);
                self.failure = None;
                if !self.terminal_prompt_nodes.contains(&node_id)
                    && self.confirm_node_id.as_ref() != Some(&node_id)
                {
                    self.terminal_prompt_nodes.push_back(node_id);
                }
                self.advance_terminal_prompt();
                RemoteShellIntegrationGateOutcome::Applied
            }
            Ok(_) | Err(_)
                if self.mode == RemoteShellIntegrationMode::Disabled || !self.awareness_enabled =>
            {
                self.failure = None;
                RemoteShellIntegrationGateOutcome::Applied
            }
            Ok((_, None)) if self.mode == RemoteShellIntegrationMode::Enabled => {
                RemoteShellIntegrationGateOutcome::RetryInstall(node_id)
            }
            Ok((status, _)) => {
                self.node_id = Some(node_id);
                self.status = Some(status);
                self.failure = Some(RemoteShellIntegrationFailure::InstallationIncomplete);
                RemoteShellIntegrationGateOutcome::Failed(
                    RemoteShellIntegrationFailure::InstallationIncomplete,
                )
            }
            Err(failure) => {
                self.node_id = Some(node_id);
                self.status = None;
                self.failure = Some(failure);
                if failure.is_unsupported_shell() && !force_install {
                    // Unsupported optional integration stays visible in Settings without a toast.
                    RemoteShellIntegrationGateOutcome::Applied
                } else {
                    RemoteShellIntegrationGateOutcome::Failed(failure)
                }
            }
        }
    }

    pub(in crate::workspace) fn open_toolbar_confirm(&mut self, node_id: Option<NodeId>) {
        self.confirm_node_id = node_id;
        self.confirm_source = Some(RemoteShellIntegrationConfirmSource::Toolbar);
        self.suppress_future_terminal_prompts = false;
    }

    pub(in crate::workspace) fn confirm_snapshot(
        &self,
    ) -> Option<RemoteShellIntegrationConfirmSnapshot> {
        Some(RemoteShellIntegrationConfirmSnapshot {
            node_id: self.confirm_node_id.clone()?,
            source: self.confirm_source?,
            suppress_future_prompts: self.suppress_future_terminal_prompts,
            state: self
                .terminal_statuses
                .get(self.confirm_node_id.as_ref()?)
                .copied(),
        })
    }

    pub(in crate::workspace) fn confirm_open(&self) -> bool {
        self.confirm_node_id.is_some()
    }

    pub(in crate::workspace) fn toggle_prompt_suppression(&mut self) {
        self.suppress_future_terminal_prompts = !self.suppress_future_terminal_prompts;
    }

    pub(in crate::workspace) fn cancel_confirm(&mut self) -> bool {
        let disable_future_prompts = self.confirm_source
            == Some(RemoteShellIntegrationConfirmSource::TerminalOpen)
            && self.suppress_future_terminal_prompts;
        self.confirm_source = None;
        self.confirm_node_id = None;
        self.suppress_future_terminal_prompts = false;
        self.advance_terminal_prompt();
        disable_future_prompts
    }

    pub(in crate::workspace) fn accept_confirm(
        &mut self,
    ) -> Option<(NodeId, RemoteShellIntegrationConfirmSource)> {
        let node_id = self.confirm_node_id.take()?;
        let source = self.confirm_source.take()?;
        self.suppress_future_terminal_prompts = false;
        self.advance_terminal_prompt();
        Some((node_id, source))
    }

    pub(in crate::workspace) fn card_snapshot(
        &self,
        node_id: Option<&NodeId>,
    ) -> RemoteShellIntegrationCardSnapshot {
        let state_matches_node = self.node_id.as_ref() == node_id;
        RemoteShellIntegrationCardSnapshot {
            status: state_matches_node.then(|| self.status.clone()).flatten(),
            failure: state_matches_node.then_some(self.failure).flatten(),
            pending: self.pending(),
        }
    }

    pub(in crate::workspace) fn begin_maintenance(
        &mut self,
        _action: RemoteShellIntegrationAction,
        node_id: NodeId,
    ) -> Option<u64> {
        if self.pending() {
            return None;
        }
        let status = (self.node_id.as_ref() == Some(&node_id))
            .then(|| self.status.clone())
            .flatten();
        let generation = self.next_generation();
        self.node_id = Some(node_id.clone());
        self.status = status;
        self.failure = None;
        self.confirm_node_id = None;
        self.confirm_source = None;
        self.maintenance = Some((node_id, generation));
        Some(generation)
    }

    pub(in crate::workspace) fn finish_maintenance(
        &mut self,
        action: RemoteShellIntegrationAction,
        node_id: NodeId,
        generation: u64,
        result: std::result::Result<RemoteShellIntegrationStatus, RemoteShellIntegrationFailure>,
    ) -> Option<RemoteShellIntegrationNotice> {
        if !self
            .maintenance
            .as_ref()
            .is_some_and(|(current_node_id, current_generation)| {
                current_node_id == &node_id && *current_generation == generation
            })
        {
            return None;
        }
        self.maintenance = None;
        if action == RemoteShellIntegrationAction::Install {
            self.terminal_ready_nodes.remove(&node_id);
        }
        match result {
            Ok(status)
                if action == RemoteShellIntegrationAction::Install
                    && status.state
                        != oxideterm_terminal::RemoteShellIntegrationState::Installed =>
            {
                self.terminal_statuses.insert(node_id, status.state);
                self.status = Some(status);
                self.failure = Some(RemoteShellIntegrationFailure::InstallationIncomplete);
                Some(RemoteShellIntegrationNotice::Failed(
                    RemoteShellIntegrationFailure::InstallationIncomplete,
                ))
            }
            Ok(status) => {
                let previous = self
                    .status
                    .as_ref()
                    .map(|status| status.state)
                    .unwrap_or(oxideterm_terminal::RemoteShellIntegrationState::NotInstalled);
                self.terminal_statuses.insert(node_id.clone(), status.state);
                if action == RemoteShellIntegrationAction::Install {
                    self.terminal_ready_nodes.insert(node_id.clone());
                } else if matches!(
                    action,
                    RemoteShellIntegrationAction::RemoveReference
                        | RemoteShellIntegrationAction::RemoveAll
                ) {
                    self.terminal_ready_nodes.remove(&node_id);
                }
                self.status = Some(status);
                self.failure = None;
                Some(match action {
                    RemoteShellIntegrationAction::Inspect => {
                        RemoteShellIntegrationNotice::Inspected
                    }
                    RemoteShellIntegrationAction::Install => {
                        RemoteShellIntegrationNotice::Configured(node_id, previous)
                    }
                    RemoteShellIntegrationAction::RemoveReference => {
                        RemoteShellIntegrationNotice::ReferenceRemoved
                    }
                    RemoteShellIntegrationAction::RemoveAll => {
                        RemoteShellIntegrationNotice::AllRemoved
                    }
                })
            }
            Err(failure) => {
                self.failure = Some(failure);
                Some(RemoteShellIntegrationNotice::Failed(failure))
            }
        }
    }

    fn advance_terminal_prompt(&mut self) {
        if self.confirm_node_id.is_some() {
            return;
        }
        if let Some(node_id) = self.terminal_prompt_nodes.pop_front() {
            self.confirm_node_id = Some(node_id);
            self.confirm_source = Some(RemoteShellIntegrationConfirmSource::TerminalOpen);
            self.suppress_future_terminal_prompts = false;
        }
    }

    fn next_generation(&mut self) -> u64 {
        self.next_generation = self.next_generation.wrapping_add(1).max(1);
        self.next_generation
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn handle_remote_shell_integration_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self
            .workspace_runtime
            .read(cx)
            .remote_shell_integration_confirm_open()
        {
            return false;
        }
        match self.handle_standard_confirm_key(event, cx) {
            Some(ConfirmKeyboardAction::Cancel) => {
                self.defer_remote_shell_integration_prompt(cx);
                let disable_future_prompts = self.workspace_runtime.update(cx, |runtime, _cx| {
                    runtime.cancel_remote_shell_integration_confirm()
                });
                if disable_future_prompts {
                    self.edit_settings(
                        |settings| {
                            settings.terminal.remote_shell_integration_mode =
                                RemoteShellIntegrationMode::Disabled;
                        },
                        cx,
                    );
                    self.remote_shell_integration_mode_changed(
                        RemoteShellIntegrationMode::Disabled,
                        cx,
                    );
                }
                cx.notify();
                true
            }
            Some(ConfirmKeyboardAction::Confirm) => {
                let accepted = self.workspace_runtime.update(cx, |runtime, _cx| {
                    runtime.accept_remote_shell_integration_confirm()
                });
                if let Some((node_id, source)) = accepted {
                    if source == RemoteShellIntegrationConfirmSource::TerminalOpen {
                        self.start_remote_shell_integration_terminal_gate(node_id, true, cx);
                    } else {
                        self.run_remote_shell_integration_action_for_node(
                            RemoteShellIntegrationAction::Install,
                            node_id,
                            cx,
                        );
                    }
                }
                true
            }
            Some(ConfirmKeyboardAction::Handled) => true,
            None => false,
        }
    }

    pub(in crate::workspace) fn remote_shell_integration_mode_changed(
        &mut self,
        mode: RemoteShellIntegrationMode,
        cx: &mut Context<Self>,
    ) {
        let awareness_enabled = self
            .settings_store
            .settings()
            .terminal
            .command_bar
            .current_directory_awareness;
        let deferred = self
            .settings_store
            .settings()
            .terminal
            .remote_shell_integration_deferred
            .clone();
        self.workspace_runtime.update(cx, |runtime, _cx| {
            runtime.configure_remote_shell_integration(mode, awareness_enabled, deferred);
        });
        cx.notify();
    }

    pub(in crate::workspace) fn remote_shell_integration_pending(&self, cx: &App) -> bool {
        self.workspace_runtime
            .read(cx)
            .remote_shell_integration_pending()
    }

    pub(in crate::workspace) fn active_ssh_terminal_node_id(&self, cx: &App) -> Option<NodeId> {
        let tab = self.active_tab(cx)?;
        let pane_id = tab.active_pane_id?;
        let session_id = tab.root_pane.as_ref()?.session_id_for_pane(pane_id)?;
        self.workspace_runtime
            .read(cx)
            .ssh_terminal_node_id(session_id)
    }

    pub(in crate::workspace) fn open_remote_shell_integration_confirm(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let node_id = self.active_ssh_terminal_node_id(cx);
        self.workspace_runtime.update(cx, |runtime, _cx| {
            runtime.open_remote_shell_integration_toolbar_confirm(node_id);
        });
        cx.notify();
    }

    pub(in crate::workspace) fn start_remote_shell_integration_terminal_gate(
        &mut self,
        node_id: NodeId,
        force_install: bool,
        cx: &mut Context<Self>,
    ) {
        let started = self.workspace_runtime.update(cx, |runtime, _cx| {
            runtime.start_remote_shell_integration_gate(node_id, force_install)
        });
        if started {
            cx.notify();
        }
    }

    fn defer_remote_shell_integration_prompt(&mut self, cx: &mut Context<Self>) {
        let runtime = self.workspace_runtime.read(cx);
        let Some(confirm) = runtime.remote_shell_integration_confirm_snapshot() else {
            return;
        };
        if confirm.source != RemoteShellIntegrationConfirmSource::TerminalOpen {
            return;
        }
        let Some(key) = runtime.remote_shell_integration_host_key(&confirm.node_id) else {
            return;
        };
        self.edit_settings(
            |settings| {
                settings
                    .terminal
                    .remote_shell_integration_deferred
                    .insert(key, oxideterm_terminal::REMOTE_SHELL_INTEGRATION_VERSION);
            },
            cx,
        );
    }

    pub(in crate::workspace) fn render_remote_shell_integration_confirm(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let confirm = self
            .workspace_runtime
            .read(cx)
            .remote_shell_integration_confirm_snapshot()?;
        let node_id = &confirm.node_id;
        let host = self
            .ssh_nodes
            .get(node_id)
            .map(|node| node.title.clone())
            .unwrap_or_else(|| node_id.0.clone());
        let mut description_key =
            if confirm.source == RemoteShellIntegrationConfirmSource::TerminalOpen {
                "settings_view.connections.shell_integration.confirm_description_terminal"
            } else {
                "settings_view.connections.shell_integration.confirm_description"
            };
        let title_key = match confirm.state {
            Some(oxideterm_terminal::RemoteShellIntegrationState::NeedsUpdate) => {
                description_key =
                    "settings_view.connections.shell_integration.confirm_update_description";
                "settings_view.connections.shell_integration.confirm_update_title"
            }
            Some(
                oxideterm_terminal::RemoteShellIntegrationState::NeedsRepair
                | oxideterm_terminal::RemoteShellIntegrationState::FilesOnly,
            ) => {
                description_key =
                    "settings_view.connections.shell_integration.confirm_repair_description";
                "settings_view.connections.shell_integration.confirm_repair_title"
            }
            _ => "settings_view.connections.shell_integration.confirm_title",
        };
        let description = self.i18n.t(description_key).replace("{{host}}", &host);
        let show_suppression = confirm.source == RemoteShellIntegrationConfirmSource::TerminalOpen;
        let suppress_future_prompts = confirm.suppress_future_prompts;
        Some(oxideterm_gpui_ui::confirm::confirm_dialog(
            &self.tokens,
            ConfirmDialogView {
                variant: ConfirmDialogVariant::Default,
                title: div()
                    .child(
                        self.i18n
                            .t(title_key),
                    )
                    .into_any_element(),
                description: Some(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        .child(div().child(description))
                        .when(show_suppression, |description| {
                            description.child(
                                checkbox(
                                    &self.tokens,
                                    self.i18n.t(
                                        "settings_view.connections.shell_integration.dont_ask_again",
                                    ),
                                    suppress_future_prompts,
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.workspace_runtime.update(cx, |runtime, _cx| {
                                            runtime
                                                .toggle_remote_shell_integration_prompt_suppression();
                                        });
                                        cx.stop_propagation();
                                        cx.notify();
                                    }),
                                ),
                            )
                        })
                        .into_any_element(),
                ),
                cancel_label: div()
                    .child(self.i18n.t(if show_suppression {
                        "settings_view.connections.shell_integration.later"
                    } else { "common.actions.cancel" }))
                    .into_any_element(),
                confirm_label: div()
                    .child(
                        self.i18n
                            .t(integration_install_label(confirm.state)),
                    )
                    .into_any_element(),
            },
            cx.listener(|this, _event, _window, cx| {
                this.defer_remote_shell_integration_prompt(cx);
                let disable_future_prompts = this.workspace_runtime.update(cx, |runtime, _cx| {
                    runtime.cancel_remote_shell_integration_confirm()
                });
                if disable_future_prompts {
                    // Reuse the persisted deployment policy so future SSH
                    // terminals skip both inspection prompts and installation.
                    this.edit_settings(
                        |settings| {
                            settings.terminal.remote_shell_integration_mode =
                                RemoteShellIntegrationMode::Disabled;
                        },
                        cx,
                    );
                    this.remote_shell_integration_mode_changed(
                        RemoteShellIntegrationMode::Disabled,
                        cx,
                    );
                }
                cx.stop_propagation();
                cx.notify();
            }),
            cx.listener(|this, _event, _window, cx| {
                let accepted = this.workspace_runtime.update(cx, |runtime, _cx| {
                    runtime.accept_remote_shell_integration_confirm()
                });
                if let Some((node_id, source)) = accepted {
                    if source == RemoteShellIntegrationConfirmSource::TerminalOpen {
                        this.start_remote_shell_integration_terminal_gate(node_id, true, cx);
                    } else {
                        this.run_remote_shell_integration_action_for_node(
                            RemoteShellIntegrationAction::Install,
                            node_id,
                            cx,
                        );
                    }
                }
                cx.stop_propagation();
            }),
        ))
    }

    pub(in crate::workspace) fn remote_shell_integration_card(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let node_id = self.active_ssh_node_id.clone();
        let node_title = node_id
            .as_ref()
            .and_then(|node_id| self.ssh_nodes.get(node_id))
            .map(|node| node.title.clone());
        let state = self
            .workspace_runtime
            .read(cx)
            .remote_shell_integration_card_snapshot(node_id.as_ref());
        let status = state.status;
        let failure = state.failure;
        // The backend owns one operation at a time even if the user selects a
        // different host while the previous operation is still completing.
        let pending = state.pending;

        let mut content = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(self.remote_shell_integration_disclosure());

        if let Some(node_title) = node_title {
            content = content
                .child(self.remote_shell_integration_detail_row(
                    "settings_view.connections.shell_integration.active_host",
                    node_title,
                ))
                .when_some(status.clone(), |content, status| {
                    content
                        .child(self.remote_shell_integration_detail_row(
                            "settings_view.connections.shell_integration.status",
                            self.remote_shell_integration_state_label(status.state),
                        ))
                        .child(self.remote_shell_integration_detail_row(
                            "settings_view.connections.shell_integration.detected_shell",
                            status.shell.display_name().to_string(),
                        ))
                        .child(self.remote_shell_integration_detail_row(
                            "settings_view.connections.shell_integration.directory",
                            status.integration_directory,
                        ))
                        .child(self.remote_shell_integration_detail_row(
                            "settings_view.connections.shell_integration.startup_file",
                            status.startup_files.join("\n"),
                        ))
                })
                .when_some(failure, |content, failure| {
                    let color = if failure.is_unsupported_shell() {
                        self.tokens.ui.text_muted
                    } else {
                        self.tokens.ui.error
                    };
                    content.child(
                        div()
                            .rounded(px(self.tokens.radii.md))
                            .border_1()
                            .border_color(rgb(color))
                            .px(px(12.0))
                            .py(px(10.0))
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(color))
                            .child(self.i18n.t(failure.message_key())),
                    )
                })
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(8.0))
                        .children([
                            self.remote_shell_integration_action_button(
                                "settings_view.connections.shell_integration.inspect",
                                LucideIcon::RefreshCw,
                                ButtonVariant::Outline,
                                pending,
                                RemoteShellIntegrationAction::Inspect,
                                cx,
                            ),
                            self.remote_shell_integration_action_button(
                                integration_install_label(
                                    status.as_ref().map(|status| status.state),
                                ),
                                LucideIcon::Download,
                                ButtonVariant::Secondary,
                                pending,
                                RemoteShellIntegrationAction::Install,
                                cx,
                            ),
                            self.remote_shell_integration_action_button(
                                "settings_view.connections.shell_integration.remove_reference",
                                LucideIcon::Trash2,
                                ButtonVariant::Ghost,
                                pending,
                                RemoteShellIntegrationAction::RemoveReference,
                                cx,
                            ),
                            self.remote_shell_integration_action_button(
                                "settings_view.connections.shell_integration.remove_all",
                                LucideIcon::Trash2,
                                ButtonVariant::Destructive,
                                pending,
                                RemoteShellIntegrationAction::RemoveAll,
                                cx,
                            ),
                        ]),
                )
                .child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(
                            self.i18n
                                .t("settings_view.connections.shell_integration.restart_hint"),
                        ),
                );
        } else {
            content = content.child(
                div()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .px(px(12.0))
                    .py(px(10.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(
                        self.i18n
                            .t("settings_view.connections.shell_integration.no_active_host"),
                    ),
            );
        }

        self.connection_section(
            "settings_view.connections.shell_integration.title",
            "settings_view.connections.shell_integration.description",
            vec![content.into_any_element()],
        )
    }

    fn remote_shell_integration_disclosure(&self) -> AnyElement {
        div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_panel))
            .px(px(12.0))
            .py(px(10.0))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(
                self.i18n
                    .t("settings_view.connections.shell_integration.disclosure"),
            )
            .child(
                div()
                    .font_family("monospace")
                    .text_color(rgb(self.tokens.ui.text))
                    .child("~/.oxideterm/shell-integration/"),
            )
            .into_any_element()
    }

    fn remote_shell_integration_detail_row(&self, label_key: &str, value: String) -> AnyElement {
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_wrap()
            .gap(px(8.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .child(
                div()
                    .w(px(160.0))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(label_key)),
            )
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .font_family("monospace")
                    .text_color(rgb(self.tokens.ui.text))
                    .child(value),
            )
            .into_any_element()
    }

    fn remote_shell_integration_state_label(
        &self,
        state: oxideterm_terminal::RemoteShellIntegrationState,
    ) -> String {
        let key = match state {
            oxideterm_terminal::RemoteShellIntegrationState::NotInstalled => {
                "settings_view.connections.shell_integration.state_not_installed"
            }
            oxideterm_terminal::RemoteShellIntegrationState::FilesOnly => {
                "settings_view.connections.shell_integration.state_files_only"
            }
            oxideterm_terminal::RemoteShellIntegrationState::Installed => {
                "settings_view.connections.shell_integration.state_installed"
            }
            oxideterm_terminal::RemoteShellIntegrationState::NeedsUpdate => {
                "settings_view.connections.shell_integration.state_needs_update"
            }
            oxideterm_terminal::RemoteShellIntegrationState::NeedsRepair => {
                "settings_view.connections.shell_integration.state_needs_repair"
            }
            oxideterm_terminal::RemoteShellIntegrationState::NewerVersion => {
                "settings_view.connections.shell_integration.newer_version"
            }
        };
        self.i18n.t(key)
    }

    fn remote_shell_integration_action_button(
        &self,
        label_key: &str,
        icon: LucideIcon,
        variant: ButtonVariant,
        pending: bool,
        action: RemoteShellIntegrationAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.workspace_toolbar_action_button(
            self.i18n.t(label_key),
            Some(Self::render_lucide_icon(
                icon,
                14.0,
                rgb(self.tokens.ui.text),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: pending,
                },
                icon_position: ToolbarButtonIconPosition::Leading,
                loading: pending,
                ..ToolbarButtonOptions::default()
            },
            cx.listener(move |this, _event, _window, cx| {
                this.run_remote_shell_integration_action(action, cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    fn run_remote_shell_integration_action(
        &mut self,
        action: RemoteShellIntegrationAction,
        cx: &mut Context<Self>,
    ) {
        if self.remote_shell_integration_pending(cx) {
            return;
        }
        let Some(node_id) = self.active_ssh_node_id.clone() else {
            return;
        };
        self.run_remote_shell_integration_action_for_node(action, node_id, cx);
    }

    fn run_remote_shell_integration_action_for_node(
        &mut self,
        action: RemoteShellIntegrationAction,
        node_id: NodeId,
        cx: &mut Context<Self>,
    ) {
        if self.remote_shell_integration_pending(cx) {
            return;
        }
        self.active_ssh_node_id = Some(node_id.clone());
        let started = self.workspace_runtime.update(cx, |runtime, _cx| {
            runtime.start_remote_shell_integration_maintenance(action, node_id)
        });
        if started {
            cx.notify();
        }
    }

    pub(in crate::workspace) fn push_remote_shell_integration_notice(
        &mut self,
        notice: RemoteShellIntegrationNotice,
        cx: &mut Context<Self>,
    ) {
        let mut message = notice.terminal_notice(&self.i18n);
        if let RemoteShellIntegrationNotice::Configured(node_id, _) = notice {
            message.description = self.ssh_nodes.get(&node_id).map(|node| node.title.clone());
            self.apply_workspace_overlay_intent(
                WorkspaceOverlayIntent::ShellIntegrationReady {
                    notice: message,
                    node_id,
                    label: self.i18n.t("sessions.tree.actions.new_terminal"),
                    workspace: cx.entity().downgrade(),
                },
                cx,
            );
        } else {
            self.push_workspace_notice(message, cx);
        }
    }
}

impl RemoteShellIntegrationNotice {
    fn terminal_notice(&self, i18n: &oxideterm_i18n::I18n) -> TerminalNotice {
        if let Self::CheckFailed(failure) | Self::Failed(failure) = self {
            return TerminalNotice {
                title: i18n.t("settings_view.connections.shell_integration.unavailable"),
                description: Some(i18n.t(failure.message_key())),
                status_text: (matches!(self, Self::CheckFailed(_))
                    && *failure != RemoteShellIntegrationFailure::ConnectionUnavailable)
                    .then(|| {
                        i18n.t("settings_view.connections.shell_integration.terminal_unaffected")
                    }),
                progress: None,
                variant: if failure.is_unsupported_shell() {
                    TerminalNoticeVariant::Default
                } else if matches!(self, Self::CheckFailed(_)) {
                    TerminalNoticeVariant::Warning
                } else {
                    TerminalNoticeVariant::Error
                },
            };
        }
        let (message_key, variant) = match self {
            RemoteShellIntegrationNotice::Inspected => (
                "settings_view.connections.shell_integration.inspect_complete",
                TerminalNoticeVariant::Success,
            ),
            RemoteShellIntegrationNotice::Configured(
                _,
                oxideterm_terminal::RemoteShellIntegrationState::NeedsUpdate,
            ) => (
                "settings_view.connections.shell_integration.update_complete",
                TerminalNoticeVariant::Success,
            ),
            RemoteShellIntegrationNotice::Configured(
                _,
                oxideterm_terminal::RemoteShellIntegrationState::NeedsRepair
                | oxideterm_terminal::RemoteShellIntegrationState::FilesOnly,
            ) => (
                "settings_view.connections.shell_integration.repair_complete",
                TerminalNoticeVariant::Success,
            ),
            RemoteShellIntegrationNotice::Configured(_, _) => (
                "settings_view.connections.shell_integration.install_complete",
                TerminalNoticeVariant::Success,
            ),
            RemoteShellIntegrationNotice::ReferenceRemoved => (
                "settings_view.connections.shell_integration.reference_removed",
                TerminalNoticeVariant::Success,
            ),
            RemoteShellIntegrationNotice::AllRemoved => (
                "settings_view.connections.shell_integration.all_removed",
                TerminalNoticeVariant::Success,
            ),
            RemoteShellIntegrationNotice::CheckFailed(_)
            | RemoteShellIntegrationNotice::Failed(_) => unreachable!(),
        };
        TerminalNotice {
            title: i18n.t(message_key),
            description: None,
            status_text: None,
            progress: None,
            variant,
        }
    }
}

impl RemoteShellIntegrationFailure {
    fn is_unsupported_shell(self) -> bool {
        matches!(
            self,
            Self::Integration(oxideterm_terminal::RemoteShellIntegrationError::UnsupportedShell)
        )
    }

    fn message_key(self) -> &'static str {
        use oxideterm_terminal::RemoteShellIntegrationError;
        match self {
            Self::ConnectionUnavailable => {
                "settings_view.connections.shell_integration.connection_unavailable"
            }
            Self::SftpUnavailable => "settings_view.connections.shell_integration.sftp_unavailable",
            Self::Integration(RemoteShellIntegrationError::EnvironmentUnavailable) => {
                "settings_view.connections.shell_integration.environment_unavailable"
            }
            Self::Integration(RemoteShellIntegrationError::HomeUnavailable) => {
                "settings_view.connections.shell_integration.home_unavailable"
            }
            Self::Integration(RemoteShellIntegrationError::UnsupportedShell) => {
                "settings_view.connections.shell_integration.unsupported_shell"
            }
            Self::Integration(RemoteShellIntegrationError::ReadFailed) => {
                "settings_view.connections.shell_integration.read_failed"
            }
            Self::Integration(RemoteShellIntegrationError::InvalidText) => {
                "settings_view.connections.shell_integration.invalid_text"
            }
            Self::Integration(RemoteShellIntegrationError::WriteFailed) => {
                "settings_view.connections.shell_integration.write_failed"
            }
            Self::Integration(RemoteShellIntegrationError::RemoveFailed) => {
                "settings_view.connections.shell_integration.remove_failed"
            }
            Self::Integration(RemoteShellIntegrationError::NewerVersion) => {
                "settings_view.connections.shell_integration.newer_version"
            }
            Self::InstallationIncomplete => {
                "settings_view.connections.shell_integration.installation_incomplete"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_shell_prompts_keep_each_hosts_installation_state() {
        use oxideterm_terminal::{RemoteShellIntegrationState as State, RemoteShellKind};
        let mut runtime = RemoteShellIntegrationRuntimeState::default();
        runtime.configure(RemoteShellIntegrationMode::Ask, true);
        let states = [State::NeedsUpdate, State::NeedsRepair, State::NotInstalled];
        for (index, state) in states.into_iter().enumerate() {
            let node = NodeId(format!("host-{index}"));
            let generation = runtime.begin_terminal_gate(&node, false).unwrap();
            runtime.finish_terminal_gate(
                node,
                generation,
                Ok((
                    RemoteShellIntegrationStatus {
                        shell: RemoteShellKind::Zsh,
                        state,
                        integration_directory: "/home/user/.oxideterm/shell-integration".into(),
                        integration_file: "/home/user/.oxideterm/shell-integration/zsh.zsh".into(),
                        startup_files: vec!["/home/user/.zshrc".into()],
                    },
                    None,
                )),
            );
        }
        for (index, state) in states.into_iter().enumerate() {
            let prompt = runtime.confirm_snapshot().unwrap();
            assert_eq!(
                (prompt.node_id, prompt.state),
                (NodeId(format!("host-{index}")), Some(state))
            );
            runtime.cancel_confirm();
        }
        assert!(runtime.confirm_snapshot().is_none());
    }

    #[test]
    fn remote_shell_deferrals_survive_settings_reload_and_expire_for_new_versions() {
        let key = r#"[["server",22,"alice"]]"#;
        let mut settings = oxideterm_settings::TerminalSettings::default();
        settings.remote_shell_integration_deferred.insert(
            key.into(),
            oxideterm_terminal::REMOTE_SHELL_INTEGRATION_VERSION,
        );
        let saved = serde_json::to_string(&settings).unwrap();
        let restored: oxideterm_settings::TerminalSettings = serde_json::from_str(&saved).unwrap();
        let mut runtime = RemoteShellIntegrationRuntimeState::default();
        runtime.configure(RemoteShellIntegrationMode::Ask, true);
        runtime.set_deferred(restored.remote_shell_integration_deferred);
        assert!(runtime.is_deferred(key));
        assert!(!runtime.is_deferred(r#"[["server",22,"bob"]]"#));
        runtime.set_deferred([(key.into(), 5)].into());
        assert!(!runtime.is_deferred(key));
    }

    #[test]
    fn repair_readback_and_write_failures_remain_retryable_until_verified() {
        use oxideterm_terminal::{
            RemoteShellIntegrationError, RemoteShellIntegrationState as State, RemoteShellKind,
        };
        let status = |state| RemoteShellIntegrationStatus {
            shell: RemoteShellKind::Bash,
            state,
            integration_directory: "/home/fixture/.oxideterm/shell-integration".into(),
            integration_file: "/home/fixture/.oxideterm/shell-integration/bash.sh".into(),
            startup_files: vec!["/home/fixture/.bashrc".into()],
        };
        for previous in [State::NeedsUpdate, State::NeedsRepair] {
            let node = NodeId("repair-host".into());
            let mut runtime = RemoteShellIntegrationRuntimeState::default();
            let generation = runtime
                .begin_maintenance(RemoteShellIntegrationAction::Inspect, node.clone())
                .unwrap();
            runtime.finish_maintenance(
                RemoteShellIntegrationAction::Inspect,
                node.clone(),
                generation,
                Ok(status(previous)),
            );

            let generation = runtime
                .begin_maintenance(RemoteShellIntegrationAction::Install, node.clone())
                .unwrap();
            let notice = runtime.finish_maintenance(
                RemoteShellIntegrationAction::Install,
                node.clone(),
                generation,
                Ok(status(State::NeedsRepair)),
            );
            assert_eq!(
                notice,
                Some(RemoteShellIntegrationNotice::Failed(
                    RemoteShellIntegrationFailure::InstallationIncomplete
                ))
            );
            let card = runtime.card_snapshot(Some(&node));
            assert_eq!(card.status.unwrap().state, State::NeedsRepair);
            assert_eq!(
                card.failure,
                Some(RemoteShellIntegrationFailure::InstallationIncomplete)
            );
            assert!(!card.pending);

            let generation = runtime
                .begin_maintenance(RemoteShellIntegrationAction::Install, node.clone())
                .unwrap();
            let failure = RemoteShellIntegrationFailure::Integration(
                RemoteShellIntegrationError::WriteFailed,
            );
            assert_eq!(
                runtime.finish_maintenance(
                    RemoteShellIntegrationAction::Install,
                    node.clone(),
                    generation,
                    Err(failure)
                ),
                Some(RemoteShellIntegrationNotice::Failed(failure))
            );

            let generation = runtime
                .begin_maintenance(RemoteShellIntegrationAction::Install, node.clone())
                .unwrap();
            assert_eq!(
                runtime.finish_maintenance(
                    RemoteShellIntegrationAction::Install,
                    node.clone(),
                    generation,
                    Ok(status(State::Installed))
                ),
                Some(RemoteShellIntegrationNotice::Configured(
                    node.clone(),
                    State::NeedsRepair
                ))
            );
            let card = runtime.card_snapshot(Some(&node));
            assert_eq!(card.status.unwrap().state, State::Installed);
            assert_eq!(card.failure, None);
            assert!(!card.pending);
        }
    }

    #[test]
    fn remote_shell_success_notice_distinguishes_install_update_and_repair() {
        use oxideterm_terminal::RemoteShellIntegrationState as State;
        let i18n = oxideterm_i18n::I18n::new(oxideterm_i18n::Locale::ZhCn);
        for (state, expected) in [
            (State::NeedsUpdate, "Shell 集成已更新，打开新终端后生效。"),
            (State::NeedsRepair, "Shell 集成已修复，打开新终端后生效。"),
            (
                State::NotInstalled,
                "远程 Shell 集成已安装，请打开新终端启用。",
            ),
        ] {
            let notice = RemoteShellIntegrationNotice::Configured(NodeId("server".into()), state)
                .terminal_notice(&i18n);
            assert_eq!(notice.title, expected);
        }
    }

    #[test]
    fn shell_integration_failure_notice_identifies_the_optional_feature() {
        let i18n = oxideterm_i18n::I18n::new(oxideterm_i18n::Locale::ZhCn);
        let notice = RemoteShellIntegrationNotice::CheckFailed(
            RemoteShellIntegrationFailure::SftpUnavailable,
        )
        .terminal_notice(&i18n);
        assert_eq!(notice.title, "远程 Shell 集成不可用");
        assert_eq!(
            notice.description.as_deref(),
            Some("无法打开 SFTP，无法检查或管理远端集成文件。请确认服务器允许 SFTP。")
        );
        assert_eq!(
            notice.status_text.as_deref(),
            Some("SSH 终端仍可正常使用。")
        );
        assert_eq!(notice.variant, TerminalNoticeVariant::Warning);
    }

    #[test]
    fn shell_integration_gate_keeps_unsupported_shell_silent_until_requested() {
        use oxideterm_terminal::RemoteShellIntegrationError;
        let i18n = oxideterm_i18n::I18n::new(oxideterm_i18n::Locale::ZhCn);
        for (error, description, variant) in [
            (
                RemoteShellIntegrationError::EnvironmentUnavailable,
                "未能识别远端 Shell 环境。请重新连接后重试。",
                TerminalNoticeVariant::Warning,
            ),
            (
                RemoteShellIntegrationError::UnsupportedShell,
                "当前远端 Shell 不支持 OxideTerm 集成，无法启用目录跟踪。",
                TerminalNoticeVariant::Default,
            ),
            (
                RemoteShellIntegrationError::WriteFailed,
                "无法写入集成文件或 Shell 启动文件。请检查远端权限和剩余空间。",
                TerminalNoticeVariant::Warning,
            ),
        ] {
            for force_install in [false, true] {
                let mut state = RemoteShellIntegrationRuntimeState::default();
                state.configure(RemoteShellIntegrationMode::Ask, true);
                let node_id = NodeId("shared-node".to_string());
                let generation = state.begin_terminal_gate(&node_id, force_install).unwrap();
                let failure = RemoteShellIntegrationFailure::Integration(error);
                let outcome = state.finish_terminal_gate(node_id.clone(), generation, Err(failure));
                if error == RemoteShellIntegrationError::UnsupportedShell && !force_install {
                    assert!(matches!(
                        outcome,
                        RemoteShellIntegrationGateOutcome::Applied
                    ));
                } else {
                    let RemoteShellIntegrationGateOutcome::Failed(actual) = outcome else {
                        panic!("requested operations and actual failures must report their reason");
                    };
                    assert_eq!(actual, failure);
                    let notice =
                        RemoteShellIntegrationNotice::CheckFailed(actual).terminal_notice(&i18n);
                    assert_eq!(notice.description.as_deref(), Some(description));
                    assert_eq!(notice.variant, variant);
                }
                assert_eq!(state.card_snapshot(Some(&node_id)).failure, Some(failure));
            }
        }
        let mut state = RemoteShellIntegrationRuntimeState::default();
        let node_id = NodeId("manual-inspection".into());
        let action = RemoteShellIntegrationAction::Inspect;
        let generation = state.begin_maintenance(action, node_id.clone()).unwrap();
        let failure = RemoteShellIntegrationFailure::Integration(
            RemoteShellIntegrationError::UnsupportedShell,
        );
        assert!(matches!(
            state.finish_maintenance(action, node_id.clone(), generation, Err(failure)),
            Some(RemoteShellIntegrationNotice::Failed(actual)) if actual == failure
        ));
        assert_eq!(state.card_snapshot(Some(&node_id)).failure, Some(failure));
        let notice = RemoteShellIntegrationNotice::Failed(
            RemoteShellIntegrationFailure::Integration(RemoteShellIntegrationError::WriteFailed),
        )
        .terminal_notice(&i18n);
        assert_eq!(notice.variant, TerminalNoticeVariant::Error);
        assert_eq!(
            notice.description.as_deref(),
            Some("无法写入集成文件或 Shell 启动文件。请检查远端权限和剩余空间。")
        );
        assert_eq!(notice.status_text, None);
        let notice = RemoteShellIntegrationNotice::CheckFailed(
            RemoteShellIntegrationFailure::ConnectionUnavailable,
        )
        .terminal_notice(&i18n);
        assert_eq!(notice.status_text, None);
    }

    #[test]
    fn only_suppressed_terminal_prompt_disables_future_questions() {
        let mut state = RemoteShellIntegrationRuntimeState::default();
        state.confirm_source = Some(RemoteShellIntegrationConfirmSource::TerminalOpen);
        state.confirm_node_id = Some(NodeId("terminal-node".to_string()));
        state.suppress_future_terminal_prompts = true;
        assert!(state.cancel_confirm());

        state.open_toolbar_confirm(Some(NodeId("toolbar-node".to_string())));
        state.toggle_prompt_suppression();
        assert!(!state.cancel_confirm());
    }

    #[test]
    fn terminal_gate_cancellation_rejects_old_results_and_allows_another_owner() {
        let mut state = RemoteShellIntegrationRuntimeState::default();
        state.configure(RemoteShellIntegrationMode::Ask, true);
        let node_id = NodeId("shared-node".to_string());

        let gate_generation = state
            .begin_terminal_gate(&node_id, false)
            .expect("the first gate should start");
        assert!(state.begin_terminal_gate(&node_id, false).is_none());
        state.cancel_node(&node_id);

        assert!(matches!(
            state.finish_terminal_gate(
                node_id.clone(),
                gate_generation,
                Err(RemoteShellIntegrationFailure::ConnectionUnavailable)
            ),
            RemoteShellIntegrationGateOutcome::Stale
        ));
        assert!(
            state.card_snapshot(Some(&node_id)).failure.is_none(),
            "a cancelled completion must not reintroduce an error projection"
        );
        assert!(state.begin_terminal_gate(&node_id, false).is_some());
    }
}
