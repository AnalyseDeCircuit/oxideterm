use super::*;
use crate::workspace::runtime_entity::RemoteShellIntegrationGateRequest;

#[derive(Clone, Debug, Default)]
pub(in crate::workspace) struct RemoteShellIntegrationUiState {
    node_id: Option<NodeId>,
    status: Option<RemoteShellIntegrationStatus>,
    pending: bool,
    maintenance_pending: bool,
    error: Option<String>,
    confirm_node_id: Option<NodeId>,
    confirm_source: Option<RemoteShellIntegrationConfirmSource>,
    terminal_ready_nodes: HashSet<NodeId>,
    terminal_checking_nodes: HashSet<NodeId>,
    terminal_prompt_nodes: VecDeque<NodeId>,
    suppress_future_terminal_prompts: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RemoteShellIntegrationConfirmSource {
    Toolbar,
    TerminalOpen,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RemoteShellIntegrationAction {
    Inspect,
    Install,
    RemoveReference,
    RemoveAll,
}

fn should_disable_remote_shell_integration_after_cancel(
    source: Option<RemoteShellIntegrationConfirmSource>,
    suppress_future_prompts: bool,
) -> bool {
    source == Some(RemoteShellIntegrationConfirmSource::TerminalOpen) && suppress_future_prompts
}

impl WorkspaceApp {
    pub(in crate::workspace) fn remote_shell_integration_mode_changed(
        &mut self,
        mode: RemoteShellIntegrationMode,
        cx: &mut Context<Self>,
    ) {
        if mode == RemoteShellIntegrationMode::Disabled {
            self.remote_shell_integration.terminal_prompt_nodes.clear();
            if self.remote_shell_integration.confirm_source
                == Some(RemoteShellIntegrationConfirmSource::TerminalOpen)
            {
                self.remote_shell_integration.confirm_node_id = None;
                self.remote_shell_integration.confirm_source = None;
            }
        }
        cx.notify();
    }

    fn advance_remote_shell_integration_terminal_prompt(&mut self) {
        if self.remote_shell_integration.confirm_node_id.is_some() {
            return;
        }
        if let Some(node_id) = self
            .remote_shell_integration
            .terminal_prompt_nodes
            .pop_front()
        {
            self.remote_shell_integration.confirm_node_id = Some(node_id);
            self.remote_shell_integration.confirm_source =
                Some(RemoteShellIntegrationConfirmSource::TerminalOpen);
            self.remote_shell_integration
                .suppress_future_terminal_prompts = false;
        }
    }

    fn refresh_remote_shell_integration_pending(&mut self) {
        self.remote_shell_integration.pending = self.remote_shell_integration.maintenance_pending
            || !self
                .remote_shell_integration
                .terminal_checking_nodes
                .is_empty();
    }

    pub(in crate::workspace) fn remote_shell_integration_pending(&self) -> bool {
        self.remote_shell_integration.pending
    }

    pub(in crate::workspace) fn active_ssh_terminal_node_id(&self) -> Option<NodeId> {
        let tab = self.active_tab()?;
        if tab.kind != TabKind::SshTerminal {
            return None;
        }
        let pane_id = tab.active_pane_id?;
        let session_id = tab.root_pane.as_ref()?.session_id_for_pane(pane_id)?;
        self.terminal_ssh_nodes.get(&session_id).cloned()
    }

    pub(in crate::workspace) fn open_remote_shell_integration_confirm(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.remote_shell_integration.confirm_node_id = self.active_ssh_terminal_node_id();
        self.remote_shell_integration.confirm_source =
            Some(RemoteShellIntegrationConfirmSource::Toolbar);
        self.remote_shell_integration
            .suppress_future_terminal_prompts = false;
        cx.notify();
    }

    pub(in crate::workspace) fn start_remote_shell_integration_terminal_gate(
        &mut self,
        node_id: NodeId,
        force_install: bool,
        cx: &mut Context<Self>,
    ) {
        let terminal_settings = &self.settings_store.settings().terminal;
        let mode = terminal_settings.remote_shell_integration_mode;
        if !terminal_settings.command_bar.current_directory_awareness
            || mode == RemoteShellIntegrationMode::Disabled
            || self
                .remote_shell_integration
                .terminal_ready_nodes
                .contains(&node_id)
            || !self
                .remote_shell_integration
                .terminal_checking_nodes
                .insert(node_id.clone())
        {
            return;
        }
        self.refresh_remote_shell_integration_pending();
        self.workspace_runtime
            .read(cx)
            .start_remote_shell_integration_gate(RemoteShellIntegrationGateRequest {
                node_id,
                mode,
                force_install,
            });
        cx.notify();
    }

    pub(in crate::workspace) fn finish_remote_shell_integration_terminal_gate(
        &mut self,
        node_id: NodeId,
        result: std::result::Result<(RemoteShellIntegrationStatus, bool), String>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.remote_shell_integration
            .terminal_checking_nodes
            .remove(&node_id);
        self.refresh_remote_shell_integration_pending();
        let mode = self
            .settings_store
            .settings()
            .terminal
            .remote_shell_integration_mode;
        let awareness_enabled = self
            .settings_store
            .settings()
            .terminal
            .command_bar
            .current_directory_awareness;
        match result {
            Ok((status, _))
                if status.state == oxideterm_terminal::RemoteShellIntegrationState::Installed =>
            {
                self.remote_shell_integration
                    .terminal_ready_nodes
                    .insert(node_id.clone());
                self.remote_shell_integration
                    .terminal_prompt_nodes
                    .retain(|queued| queued != &node_id);
                self.remote_shell_integration.node_id = Some(node_id);
                self.remote_shell_integration.status = Some(status);
                self.remote_shell_integration.error = None;
            }
            Ok((status, _)) if mode == RemoteShellIntegrationMode::Ask => {
                self.remote_shell_integration.node_id = Some(node_id.clone());
                self.remote_shell_integration.status = Some(status);
                self.remote_shell_integration.error = None;
                if !self
                    .remote_shell_integration
                    .terminal_prompt_nodes
                    .contains(&node_id)
                    && self.remote_shell_integration.confirm_node_id.as_ref() != Some(&node_id)
                {
                    self.remote_shell_integration
                        .terminal_prompt_nodes
                        .push_back(node_id);
                }
                self.advance_remote_shell_integration_terminal_prompt();
            }
            Ok(_) | Err(_)
                if mode == RemoteShellIntegrationMode::Disabled || !awareness_enabled =>
            {
                self.remote_shell_integration.error = None;
            }
            Ok((_, false)) if mode == RemoteShellIntegrationMode::Enabled => {
                self.start_remote_shell_integration_terminal_gate(node_id, true, cx);
            }
            Ok((status, _)) => {
                let error = format!(
                    "{}: {}",
                    self.i18n
                        .t("settings_view.connections.shell_integration.status"),
                    self.remote_shell_integration_state_label(status.state)
                );
                self.remote_shell_integration.error = Some(error.clone());
                self.push_ai_settings_toast(error, TerminalNoticeVariant::Error, cx);
            }
            Err(error) => {
                self.remote_shell_integration.error = Some(error.clone());
                self.push_ai_settings_toast(error, TerminalNoticeVariant::Error, cx);
            }
        }
        cx.notify();
    }

    pub(in crate::workspace) fn render_remote_shell_integration_confirm(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let node_id = self.remote_shell_integration.confirm_node_id.as_ref()?;
        let host = self
            .ssh_nodes
            .get(node_id)
            .map(|node| node.title.clone())
            .unwrap_or_else(|| node_id.0.clone());
        let description_key = if self.remote_shell_integration.confirm_source
            == Some(RemoteShellIntegrationConfirmSource::TerminalOpen)
        {
            "settings_view.connections.shell_integration.confirm_description_terminal"
        } else {
            "settings_view.connections.shell_integration.confirm_description"
        };
        let description = self.i18n.t(description_key).replace("{{host}}", &host);
        let show_suppression = self.remote_shell_integration.confirm_source
            == Some(RemoteShellIntegrationConfirmSource::TerminalOpen);
        let suppress_future_prompts = self
            .remote_shell_integration
            .suppress_future_terminal_prompts;
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
                                        this.remote_shell_integration
                                            .suppress_future_terminal_prompts = !this
                                            .remote_shell_integration
                                            .suppress_future_terminal_prompts;
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
                let source = this.remote_shell_integration.confirm_source;
                let suppress_future_prompts = this
                    .remote_shell_integration
                    .suppress_future_terminal_prompts;
                this.remote_shell_integration.confirm_source = None;
                this.remote_shell_integration.confirm_node_id = None;
                this.remote_shell_integration.suppress_future_terminal_prompts = false;
                if should_disable_remote_shell_integration_after_cancel(
                    source,
                    suppress_future_prompts,
                ) {
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
                } else {
                    this.advance_remote_shell_integration_terminal_prompt();
                }
                cx.stop_propagation();
                cx.notify();
            }),
            cx.listener(|this, _event, _window, cx| {
                let node_id = this.remote_shell_integration.confirm_node_id.take();
                let source = this.remote_shell_integration.confirm_source.take();
                this.remote_shell_integration.suppress_future_terminal_prompts = false;
                if let Some(node_id) = node_id {
                    if source == Some(RemoteShellIntegrationConfirmSource::TerminalOpen) {
                        this.start_remote_shell_integration_terminal_gate(node_id, true, cx);
                    } else {
                        this.run_remote_shell_integration_action_for_node(
                            RemoteShellIntegrationAction::Install,
                            node_id,
                            cx,
                        );
                    }
                }
                this.advance_remote_shell_integration_terminal_prompt();
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
        let state_matches_node = self.remote_shell_integration.node_id == node_id;
        let status = state_matches_node
            .then(|| self.remote_shell_integration.status.clone())
            .flatten();
        let error = state_matches_node
            .then(|| self.remote_shell_integration.error.clone())
            .flatten();
        // The backend owns one operation at a time even if the user selects a
        // different host while the previous operation is still completing.
        let pending = self.remote_shell_integration.pending;

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
        if self.remote_shell_integration.pending {
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
        if self.remote_shell_integration.pending {
            return;
        }
        let status = (self.remote_shell_integration.node_id.as_ref() == Some(&node_id))
            .then(|| self.remote_shell_integration.status.clone())
            .flatten();
        self.active_ssh_node_id = Some(node_id.clone());
        self.remote_shell_integration.node_id = Some(node_id.clone());
        self.remote_shell_integration.status = status;
        self.remote_shell_integration.maintenance_pending = true;
        self.refresh_remote_shell_integration_pending();
        self.remote_shell_integration.error = None;
        self.remote_shell_integration.confirm_node_id = None;
        self.remote_shell_integration.confirm_source = None;
        let router = self.node_router.clone();
        let runtime = self.forwarding_runtime.clone();
        let success_message = self.i18n.t(match action {
            RemoteShellIntegrationAction::Inspect => {
                "settings_view.connections.shell_integration.inspect_complete"
            }
            RemoteShellIntegrationAction::Install => {
                "settings_view.connections.shell_integration.install_complete"
            }
            RemoteShellIntegrationAction::RemoveReference => {
                "settings_view.connections.shell_integration.reference_removed"
            }
            RemoteShellIntegrationAction::RemoveAll => {
                "settings_view.connections.shell_integration.all_removed"
            }
        });
        cx.spawn(async move |weak, cx| {
            let action_node_id = node_id.clone();
            let result = runtime
                .spawn(async move {
                    let resolved = router
                        .resolve_connection(&action_node_id)
                        .await
                        .map_err(|error| error.to_string())?;
                    let remote_env = resolved.handle.remote_env();
                    let sftp = router
                        .acquire_sftp(&action_node_id)
                        .await
                        .map_err(|error| error.to_string())?;
                    let sftp = sftp.lock().await;
                    match action {
                        RemoteShellIntegrationAction::Inspect => {
                            oxideterm_terminal::inspect_remote_shell_integration(
                                &sftp,
                                remote_env.as_ref(),
                            )
                            .await
                        }
                        RemoteShellIntegrationAction::Install => {
                            oxideterm_terminal::install_remote_shell_integration(
                                &sftp,
                                remote_env.as_ref(),
                            )
                            .await
                        }
                        RemoteShellIntegrationAction::RemoveReference => {
                            oxideterm_terminal::remove_remote_shell_integration(
                                &sftp,
                                remote_env.as_ref(),
                                false,
                            )
                            .await
                        }
                        RemoteShellIntegrationAction::RemoveAll => {
                            oxideterm_terminal::remove_remote_shell_integration(
                                &sftp,
                                remote_env.as_ref(),
                                true,
                            )
                            .await
                        }
                    }
                })
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result);
            let _ = weak.update(cx, |this, cx| {
                this.remote_shell_integration.maintenance_pending = false;
                this.refresh_remote_shell_integration_pending();
                match result {
                    Ok(status) => {
                        if action == RemoteShellIntegrationAction::Install {
                            this.remote_shell_integration
                                .terminal_ready_nodes
                                .insert(node_id.clone());
                        } else if matches!(
                            action,
                            RemoteShellIntegrationAction::RemoveReference
                                | RemoteShellIntegrationAction::RemoveAll
                        ) {
                            this.remote_shell_integration
                                .terminal_ready_nodes
                                .remove(&node_id);
                        }
                        this.remote_shell_integration.status = Some(status);
                        this.remote_shell_integration.error = None;
                        this.push_ai_settings_toast(
                            success_message,
                            TerminalNoticeVariant::Success,
                            cx,
                        );
                    }
                    Err(error) => {
                        this.remote_shell_integration.error = Some(error.clone());
                        this.push_ai_settings_toast(error, TerminalNoticeVariant::Error, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_suppressed_terminal_prompt_disables_future_questions() {
        assert!(should_disable_remote_shell_integration_after_cancel(
            Some(RemoteShellIntegrationConfirmSource::TerminalOpen),
            true,
        ));
        assert!(!should_disable_remote_shell_integration_after_cancel(
            Some(RemoteShellIntegrationConfirmSource::TerminalOpen),
            false,
        ));
        assert!(!should_disable_remote_shell_integration_after_cancel(
            Some(RemoteShellIntegrationConfirmSource::Toolbar),
            true,
        ));
    }
}
