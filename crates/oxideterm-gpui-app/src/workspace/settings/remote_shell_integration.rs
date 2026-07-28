use super::*;

pub(in crate::workspace) struct RemoteShellIntegrationRuntimeState {
    node_id: Option<NodeId>,
    status: Option<RemoteShellIntegrationStatus>,
    error: bool,
    confirm_node_id: Option<NodeId>,
    confirm_source: Option<RemoteShellIntegrationConfirmSource>,
    terminal_ready_nodes: HashSet<NodeId>,
    terminal_checking_nodes: HashMap<NodeId, u64>,
    terminal_prompt_nodes: VecDeque<NodeId>,
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
}

#[derive(Clone)]
pub(in crate::workspace) struct RemoteShellIntegrationCardSnapshot {
    pub(in crate::workspace) status: Option<RemoteShellIntegrationStatus>,
    pub(in crate::workspace) error: bool,
    pub(in crate::workspace) pending: bool,
}

pub(in crate::workspace) enum RemoteShellIntegrationGateOutcome {
    Applied,
    RetryInstall(NodeId),
    Failed,
    Stale,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum RemoteShellIntegrationNotice {
    Inspected,
    Installed,
    ReferenceRemoved,
    AllRemoved,
    Failed,
}

impl Default for RemoteShellIntegrationRuntimeState {
    fn default() -> Self {
        Self {
            node_id: None,
            status: None,
            error: false,
            confirm_node_id: None,
            confirm_source: None,
            terminal_ready_nodes: HashSet::new(),
            terminal_checking_nodes: HashMap::new(),
            terminal_prompt_nodes: VecDeque::new(),
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

    pub(in crate::workspace) fn cancel_terminal_gates(&mut self) {
        self.terminal_checking_nodes.clear();
    }

    pub(in crate::workspace) fn cancel_node(&mut self, node_id: &NodeId) {
        self.terminal_checking_nodes.remove(node_id);
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
            self.error = false;
        }
        if self
            .maintenance
            .as_ref()
            .is_some_and(|(current, _)| current == node_id)
        {
            self.maintenance = None;
        }
    }

    pub(in crate::workspace) fn begin_terminal_gate(&mut self, node_id: &NodeId) -> Option<u64> {
        if !self.awareness_enabled
            || self.mode == RemoteShellIntegrationMode::Disabled
            || self.terminal_ready_nodes.contains(node_id)
            || self.terminal_checking_nodes.contains_key(node_id)
        {
            return None;
        }
        let generation = self.next_generation();
        self.terminal_checking_nodes
            .insert(node_id.clone(), generation);
        Some(generation)
    }

    pub(in crate::workspace) fn finish_terminal_gate(
        &mut self,
        node_id: NodeId,
        generation: u64,
        result: std::result::Result<(RemoteShellIntegrationStatus, bool), ()>,
    ) -> RemoteShellIntegrationGateOutcome {
        if self.terminal_checking_nodes.get(&node_id) != Some(&generation) {
            return RemoteShellIntegrationGateOutcome::Stale;
        }
        self.terminal_checking_nodes.remove(&node_id);
        match result {
            Ok((status, _))
                if status.state == oxideterm_terminal::RemoteShellIntegrationState::Installed =>
            {
                self.terminal_ready_nodes.insert(node_id.clone());
                self.terminal_prompt_nodes
                    .retain(|queued| queued != &node_id);
                self.node_id = Some(node_id);
                self.status = Some(status);
                self.error = false;
                RemoteShellIntegrationGateOutcome::Applied
            }
            Ok((status, _)) if self.mode == RemoteShellIntegrationMode::Ask => {
                self.node_id = Some(node_id.clone());
                self.status = Some(status);
                self.error = false;
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
                self.error = false;
                RemoteShellIntegrationGateOutcome::Applied
            }
            Ok((_, false)) if self.mode == RemoteShellIntegrationMode::Enabled => {
                RemoteShellIntegrationGateOutcome::RetryInstall(node_id)
            }
            Ok((status, _)) => {
                self.node_id = Some(node_id);
                self.status = Some(status);
                self.error = true;
                RemoteShellIntegrationGateOutcome::Failed
            }
            Err(_) => {
                self.node_id = Some(node_id);
                self.error = true;
                RemoteShellIntegrationGateOutcome::Failed
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
        })
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
            error: state_matches_node && self.error,
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
        self.error = false;
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
        result: std::result::Result<RemoteShellIntegrationStatus, ()>,
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
        match result {
            Ok(status) => {
                if action == RemoteShellIntegrationAction::Install {
                    self.terminal_ready_nodes.insert(node_id);
                } else if matches!(
                    action,
                    RemoteShellIntegrationAction::RemoveReference
                        | RemoteShellIntegrationAction::RemoveAll
                ) {
                    self.terminal_ready_nodes.remove(&node_id);
                }
                self.status = Some(status);
                self.error = false;
                Some(match action {
                    RemoteShellIntegrationAction::Inspect => {
                        RemoteShellIntegrationNotice::Inspected
                    }
                    RemoteShellIntegrationAction::Install => {
                        RemoteShellIntegrationNotice::Installed
                    }
                    RemoteShellIntegrationAction::RemoveReference => {
                        RemoteShellIntegrationNotice::ReferenceRemoved
                    }
                    RemoteShellIntegrationAction::RemoveAll => {
                        RemoteShellIntegrationNotice::AllRemoved
                    }
                })
            }
            Err(_) => {
                self.error = true;
                Some(RemoteShellIntegrationNotice::Failed)
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
        self.workspace_runtime.update(cx, |runtime, _cx| {
            runtime.configure_remote_shell_integration(mode, awareness_enabled);
        });
        cx.notify();
    }

    pub(in crate::workspace) fn remote_shell_integration_pending(&self, cx: &App) -> bool {
        self.workspace_runtime
            .read(cx)
            .remote_shell_integration_pending()
    }

    pub(in crate::workspace) fn active_ssh_terminal_node_id(&self, cx: &App) -> Option<NodeId> {
        let tab = self.active_tab()?;
        if tab.kind != TabKind::SshTerminal {
            return None;
        }
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
        let description_key = if confirm.source == RemoteShellIntegrationConfirmSource::TerminalOpen
        {
            "settings_view.connections.shell_integration.confirm_description_terminal"
        } else {
            "settings_view.connections.shell_integration.confirm_description"
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
                            .t("settings_view.connections.shell_integration.confirm_title"),
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
                    .child(self.i18n.t("common.actions.cancel"))
                    .into_any_element(),
                confirm_label: div()
                    .child(
                        self.i18n
                            .t("settings_view.connections.shell_integration.install"),
                    )
                    .into_any_element(),
            },
            cx.listener(|this, _event, _window, cx| {
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
        let error = state.error.then(|| {
            format!(
                "{}: {}",
                self.i18n
                    .t("settings_view.connections.shell_integration.status"),
                self.i18n.t("common.status.error")
            )
        });
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
                            status.startup_file,
                        ))
                })
                .when_some(error, |content, error| {
                    content.child(
                        div()
                            .rounded(px(self.tokens.radii.md))
                            .border_1()
                            .border_color(rgb(self.tokens.ui.error))
                            .px(px(12.0))
                            .py(px(10.0))
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.error))
                            .child(error),
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
                                if status.as_ref().is_some_and(|status| {
                                    status.state
                                        == oxideterm_terminal::RemoteShellIntegrationState::Installed
                                }) {
                                    "settings_view.connections.shell_integration.reinstall"
                                } else {
                                    "settings_view.connections.shell_integration.install"
                                },
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
        let (message_key, variant) = match notice {
            RemoteShellIntegrationNotice::Inspected => (
                "settings_view.connections.shell_integration.inspect_complete",
                TerminalNoticeVariant::Success,
            ),
            RemoteShellIntegrationNotice::Installed => (
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
            RemoteShellIntegrationNotice::Failed => {
                ("common.status.error", TerminalNoticeVariant::Error)
            }
        };
        // Runtime errors intentionally collapse to a localized category at the UI boundary.
        self.push_ai_settings_toast(self.i18n.t(message_key), variant, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn terminal_gate_has_one_owner_per_node() {
        let mut state = RemoteShellIntegrationRuntimeState::default();
        state.configure(RemoteShellIntegrationMode::Ask, true);
        let node_id = NodeId("shared-node".to_string());

        assert!(state.begin_terminal_gate(&node_id).is_some());
        assert!(state.begin_terminal_gate(&node_id).is_none());
        state.cancel_node(&node_id);
        assert!(state.begin_terminal_gate(&node_id).is_some());
    }

    #[test]
    fn cancelled_node_rejects_late_content_free_failures() {
        let mut state = RemoteShellIntegrationRuntimeState::default();
        state.configure(RemoteShellIntegrationMode::Ask, true);
        let node_id = NodeId("cancelled-node".to_string());
        let gate_generation = state
            .begin_terminal_gate(&node_id)
            .expect("the first gate should start");
        state.cancel_node(&node_id);

        assert!(matches!(
            state.finish_terminal_gate(node_id.clone(), gate_generation, Err(())),
            RemoteShellIntegrationGateOutcome::Stale
        ));
        assert!(
            !state.card_snapshot(Some(&node_id)).error,
            "a cancelled completion must not reintroduce an error projection"
        );
    }
}
