use super::*;
use oxideterm_gpui_settings_view::remote_shell::{RemoteShellPage, RemoteShellRequest};

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
                self.resolve_remote_shell_integration_prompt(false, cx);
                true
            }
            Some(ConfirmKeyboardAction::Confirm) => {
                self.resolve_remote_shell_integration_prompt(true, cx);
                true
            }
            Some(ConfirmKeyboardAction::Handled) => true,
            None => false,
        }
    }

    fn resolve_remote_shell_integration_prompt(&mut self, confirm: bool, cx: &mut Context<Self>) {
        if confirm {
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
        } else {
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

    fn with_remote_shell_page<R>(
        &self,
        cx: &Context<Self>,
        render: impl FnOnce(RemoteShellPage<'_>, &App) -> R,
    ) -> R {
        let owner = cx.entity().downgrade();
        let dispatch = Rc::new(move |request, cx: &mut App| {
            let _ = owner.update(cx, |app, cx| app.handle_remote_shell_request(request, cx));
        });
        render(
            RemoteShellPage {
                components: self.settings_components(),
                dispatch,
            },
            cx,
        )
    }

    pub(super) fn handle_remote_shell_request(&mut self, request: RemoteShellRequest, cx: &mut Context<Self>) {
        match request {
                RemoteShellRequest::ToggleSuppression => {
                    self.workspace_runtime.update(cx, |runtime, _cx| {
                        runtime.toggle_remote_shell_integration_prompt_suppression();
                    });
                    cx.notify();
                }
                RemoteShellRequest::Defer => self.resolve_remote_shell_integration_prompt(false, cx),
                RemoteShellRequest::Confirm => {
                    self.resolve_remote_shell_integration_prompt(true, cx)
                }
                RemoteShellRequest::Run(action) => {
                    self.run_remote_shell_integration_action(action, cx)
                }
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
        let host = self
            .ssh_nodes
            .get(&confirm.node_id)
            .map(|node| node.title.clone())
            .unwrap_or_else(|| confirm.node_id.0.clone());
        Some(self.with_remote_shell_page(cx, |page, _| page.confirmation(confirm, host)))
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
