use super::new_connection::NewConnectionTransport;
use super::*;
use oxideterm_gpui_connections::submission::LocalFormSubmission;
use oxideterm_gpui_settings_view::local_terminal::{
    effective_local_shells, expand_local_terminal_cwd,
};

#[derive(Clone)]
pub(super) struct LocalTerminalInstance {
    pub(super) profile_id: Option<String>,
    pub(super) title: String,
    pub(super) shell: Option<oxideterm_terminal::ShellInfo>,
    pub(super) cwd: Option<std::path::PathBuf>,
    /// Names only, never values: a split must keep the same launch identity.
    pub(super) remove_env: Vec<String>,
}

impl LocalTerminalInstance {
    pub(super) fn new(config: &LocalPtyConfig, title: String) -> Self {
        // Retain launch identity for splits, never a copy of environment secrets.
        Self {
            profile_id: None,
            title,
            shell: config.shell.clone(),
            cwd: config.cwd.clone(),
            remove_env: config.remove_env.clone(),
        }
    }
}

pub(super) fn local_profile_node_id(id: &str) -> NodeId {
    NodeId::new(format!("local-profile-{id}"))
}

impl WorkspaceApp {
    pub(in crate::workspace) fn local_terminal_config(&self) -> LocalPtyConfig {
        let settings = &self.settings_store.settings().local_terminal;
        let effective_shells = effective_local_shells(
            &self.local_shells,
            settings.git_bash_path.as_deref(),
            &settings.shell_paths,
        );
        let shell = settings
            .default_shell_id
            .as_deref()
            .and_then(|id| effective_shells.iter().find(|shell| shell.id == id))
            .cloned();
        let cwd = settings
            .default_cwd
            .as_deref()
            .map(str::trim)
            .filter(|cwd| !cwd.is_empty())
            .map(expand_local_terminal_cwd);
        let env = settings
            .custom_env_vars
            .iter()
            .map(|(key, value)| {
                (
                    key.clone(),
                    value
                        .as_str()
                        .map(ToOwned::to_owned)
                        .unwrap_or_else(|| value.to_string()),
                )
            })
            .collect();

        LocalPtyConfig {
            command: None,
            shell,
            cwd,
            env,
            remove_env: Vec::new(),
            load_profile: settings.load_shell_profile,
            current_directory_shell_integration: self
                .settings_store
                .settings()
                .terminal
                .command_bar
                .current_directory_awareness,
            oh_my_posh_enabled: settings.oh_my_posh_enabled,
            oh_my_posh_theme: settings.oh_my_posh_theme.clone(),
        }
    }

    pub(in crate::workspace) fn local_terminal_tab_title(&self) -> String {
        let settings = &self.settings_store.settings().local_terminal;
        let effective_shells = effective_local_shells(
            &self.local_shells,
            settings.git_bash_path.as_deref(),
            &settings.shell_paths,
        );
        settings
            .default_shell_id
            .as_deref()
            .and_then(|id| effective_shells.iter().find(|shell| shell.id == id))
            .or_else(|| effective_shells.first())
            .map(|shell| shell.label.clone())
            .unwrap_or_else(|| "Local".to_string())
    }


    fn local_profile_config(
        &self,
        shell_id: Option<&str>,
        cwd: Option<&str>,
    ) -> Result<LocalPtyConfig> {
        let mut config = self.local_terminal_config();
        if let Some(id) = shell_id {
            config.shell = Some(
                self.effective_local_shells_for_settings(self.settings_store.settings())
                    .into_iter()
                    .find(|shell| shell.id == id)
                    .ok_or_else(|| {
                        anyhow::anyhow!(self.i18n.t("local_session.shell_unavailable"))
                    })?,
            );
        }
        let wsl = config
            .shell
            .as_ref()
            .is_some_and(|shell| shell.id.starts_with("wsl"));
        if let Some(cwd) = cwd.filter(|cwd| !cwd.trim().is_empty()) {
            config.cwd = Some(if wsl {
                std::path::PathBuf::from(cwd)
            } else {
                settings::expand_local_terminal_cwd(cwd)
            });
        }
        if !wsl && config.cwd.as_ref().is_some_and(|path| !path.is_dir()) {
            anyhow::bail!(self.i18n.t("local_session.directory_unavailable"));
        }
        Ok(config)
    }

    pub(super) fn open_local_terminal_from_form(
        &mut self,
        plan: LocalFormSubmission,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Opening a local profile is a connection attempt: run its local gate
        // off the UI thread before the PTY is created. Saving alone only
        // persists the command.
        let pre_connect_command = plan
            .launch
            .then(|| plan.pre_connect_command().map(str::to_owned))
            .flatten();
        if let Some(command) = pre_connect_command {
            let window_handle = window.window_handle();
            let i18n = self.i18n.clone();
            let (gate, window_close) = self.spawn_window_pre_connect(command, window, cx);
            cx.spawn(async move |workspace, cx| {
                let _window_close = window_close;
                let Ok(result) = gate.await else {
                    return;
                };
                let failure = result.err();
                let _ = cx.update_window(window_handle, move |_, window, cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        if let Some(failure) = failure {
                            this.report_local_pre_connect_failure(failure, &i18n, cx);
                            return;
                        }
                        this.open_local_terminal_from_form_after_gate(plan, window, cx);
                    });
                });
            })
            .detach();
            return;
        }
        self.open_local_terminal_from_form_after_gate(plan, window, cx);
    }

    fn open_local_terminal_from_form_after_gate(
        &mut self,
        plan: LocalFormSubmission,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result = (|| -> Result<()> {
            // Saving a portable profile does not require its shell or directory to exist here.
            let config = plan
                .launch
                .then(|| self.local_profile_config(plan.shell_id(), plan.cwd()))
                .transpose()?;
            let title = plan.title(
                config
                    .as_ref()
                    .and_then(|config| config.shell.as_ref())
                    .map(|shell| shell.label.as_str()),
                || self.local_terminal_tab_title(),
            );
            let profile_id = self.connections.flow.update(cx, |flow, cx| {
                flow.persist_local_form(plan, &mut self.connection_store, cx)
            })?;
            if profile_id.is_some() {
                self.queue_cloud_sync_dirty_refresh(cx);
            }
            if let Some(config) = config {
                let shell_id = config.shell.as_ref().map(|shell| shell.id.clone());
                let (session_id, _) =
                    self.create_local_terminal_tab_with_owned_session(config, title, window, cx)?;
                if let Some(id) = profile_id.as_deref() {
                    self.expanded_ssh_nodes.insert(local_profile_node_id(id));
                }
                self.bind_local_profile(session_id, profile_id.clone(), cx);
                if let Some(shell_id) = shell_id {
                    self.edit_settings(
                        |settings| {
                            let recent = &mut settings.local_terminal.recent_shell_ids;
                            recent.retain(|id| id != &shell_id);
                            recent.insert(0, shell_id);
                            recent.truncate(5);
                        },
                        cx,
                    );
                }
            }
            if let Some(id) = profile_id.as_deref() {
                self.show_local_profile_session(id, cx);
            }
            Ok(())
        })();
        match result {
            Ok(()) => self.close_new_connection_form(window, cx),
            Err(error) => self.connections.flow.update(cx, |flow, cx| {
                flow.set_local_form_error(error.to_string(), cx);
            }),
        }
        cx.notify();
    }

    fn bind_local_profile(
        &mut self,
        session_id: TerminalSessionId,
        profile_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if let Some(id) = &profile_id {
            let _ = self.connection_store.mark_local_terminal_profile_used(id);
        }
        self.tab_host.update(cx, |host, _| {
            if let Some(instance) = host.local_sessions.get_mut(&session_id) {
                instance.profile_id = profile_id;
            }
        });
    }

    fn show_local_profile_session(&mut self, id: &str, cx: &mut Context<Self>) {
        let hidden = &mut self
            .settings_store
            .settings_mut()
            .sidebar_ui
            .hidden_local_terminal_profile_ids;
        let count = hidden.len();
        hidden.retain(|hidden_id| hidden_id != id);
        if hidden.len() != count {
            self.persist_sidebar_settings(cx);
        }
    }

    pub(super) fn remove_local_profile_session(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let session_ids = self
            .tab_host
            .read(cx)
            .local_sessions
            .iter()
            .filter_map(|(session_id, instance)| {
                (instance.profile_id.as_deref() == Some(id)).then_some(*session_id)
            })
            .collect::<Vec<_>>();
        for session_id in session_ids {
            self.close_terminal_session(session_id, window, cx);
        }
        self.expanded_ssh_nodes.remove(&local_profile_node_id(id));
        let hidden = &mut self
            .settings_store
            .settings_mut()
            .sidebar_ui
            .hidden_local_terminal_profile_ids;
        if !hidden.iter().any(|hidden_id| hidden_id == id) {
            hidden.push(id.to_owned());
            self.persist_sidebar_settings(cx);
        }
        cx.notify();
    }

    pub(super) fn open_saved_local_terminal_profile(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(command) = self
            .connection_store
            .local_terminal_profiles()
            .iter()
            .find(|profile| profile.id == id)
            .and_then(|profile| profile.pre_connect_command.clone())
        else {
            self.open_saved_local_terminal_profile_after_gate(id, window, cx);
            return;
        };
        let window_handle = window.window_handle();
        let i18n = self.i18n.clone();
        let profile_id = id.to_string();
        let (gate, window_close) = self.spawn_window_pre_connect(command, window, cx);
        cx.spawn(async move |workspace, cx| {
            let _window_close = window_close;
            let Ok(result) = gate.await else {
                return;
            };
            let failure = result.err();
            let _ = cx.update_window(window_handle, move |_, window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    if let Some(failure) = failure {
                        this.report_local_pre_connect_failure(failure, &i18n, cx);
                        return;
                    }
                    this.open_saved_local_terminal_profile_after_gate(&profile_id, window, cx);
                });
            });
        })
        .detach();
    }

    pub(in crate::workspace) fn spawn_window_pre_connect(
        &self,
        command: String,
        window: &Window,
        cx: &mut App,
    ) -> (
        tokio_util::task::AbortOnDropHandle<Result<(), oxideterm_terminal::LocalCommandFailure>>,
        Subscription,
    ) {
        // GPUI tasks have no Tokio reactor; the window close subscription
        // retires the gate on the workspace runtime before it can open a tab.
        let worker = self.forwarding_runtime.spawn(oxideterm_terminal::run_pre_connect_command_async(
            command,
            oxideterm_terminal::PRE_CONNECT_COMMAND_TIMEOUT,
        ));
        let abort = worker.abort_handle();
        let close = window_shell::observe_window_close(window, cx, move |_, _| abort.abort());
        (tokio_util::task::AbortOnDropHandle::new(worker), close)
    }

    fn report_local_pre_connect_failure(
        &mut self,
        failure: oxideterm_terminal::LocalCommandFailure,
        i18n: &oxideterm_i18n::I18n,
        cx: &mut Context<Self>,
    ) {
        let message = oxideterm_terminal::pre_connect_failure_message(Some(i18n), failure);
        self.connections.session_manager.update(cx, |manager, cx| {
            manager.set_status(Some(message.clone()), cx)
        });
        self.push_workspace_notice(
            TerminalNotice {
                title: message,
                description: None,
                status_text: None,
                progress: None,
                variant: TerminalNoticeVariant::Error,
            },
            cx,
        );
        cx.notify();
    }

    fn open_saved_local_terminal_profile_after_gate(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = self
            .connection_store
            .local_terminal_profiles()
            .iter()
            .find(|p| p.id == id)
            .cloned()
        else {
            return;
        };
        let result = self
            .local_profile_config(profile.shell_id.as_deref(), profile.cwd.as_deref())
            .and_then(|config| {
                self.create_local_terminal_tab_with_owned_session(config, profile.name, window, cx)
            });
        match result {
            Ok((session_id, _)) => {
                self.bind_local_profile(session_id, Some(profile.id.clone()), cx);
                self.show_local_profile_session(&profile.id, cx);
                self.expanded_ssh_nodes
                    .insert(local_profile_node_id(&profile.id));
                self.focus_terminal_session(session_id, window, cx);
            }
            Err(error) => {
                let message = error.to_string();
                self.connections.session_manager.update(cx, |manager, cx| {
                    manager.set_status(Some(message.clone()), cx)
                });
                // Palette and sidebar launches may have no Session Manager surface.
                self.push_workspace_notice(
                    TerminalNotice {
                        title: message,
                        description: None,
                        status_text: None,
                        progress: None,
                        variant: TerminalNoticeVariant::Error,
                    },
                    cx,
                );
            }
        }
        cx.notify();
    }

    pub(super) fn open_saved_local_terminal_profile_editor(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = self
            .connection_store
            .local_terminal_profiles()
            .iter()
            .find(|p| p.id == id)
            .cloned()
        else {
            return;
        };
        self.open_new_connection_form(window, cx);
        let mut form = NewConnectionForm::default();
        form.transport = NewConnectionTransport::LocalTerminal;
        form.local_profile_id = Some(profile.id);
        form.name = profile.name;
        form.icon = profile.icon.unwrap_or_default();
        form.color = profile.color.unwrap_or_default();
        form.icon_background_color = profile.icon_background_color.unwrap_or_default();
        form.local_shell_id = profile.shell_id;
        form.local_cwd = profile.cwd.unwrap_or_default();
        form.group = profile
            .group
            .unwrap_or_else(|| self.i18n.t("ssh.form.ungrouped"));
        self.update_connection_form_state(cx, |state| state.replace_with_new_form(form));
        cx.notify();
    }
}
