use super::new_connection::{NewConnectionSubmitAction, NewConnectionTransport};
use super::*;
use oxideterm_connections::SaveLocalTerminalProfileRequest;

#[derive(Clone)]
pub(super) struct LocalTerminalInstance {
    pub(super) profile_id: Option<String>,
    pub(super) title: String,
    pub(super) shell: Option<oxideterm_terminal::ShellInfo>,
    pub(super) cwd: Option<std::path::PathBuf>,
}

impl LocalTerminalInstance {
    pub(super) fn new(config: &LocalPtyConfig, title: String) -> Self {
        // Retain launch identity for splits, never a copy of environment secrets.
        Self {
            profile_id: None,
            title,
            shell: config.shell.clone(),
            cwd: config.cwd.clone(),
        }
    }
}

impl WorkspaceApp {
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

    pub(super) fn submit_local_terminal_form(
        &mut self,
        action: NewConnectionSubmitAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(form) = self.connection_form_state(cx).form.as_ref() else {
            return;
        };
        let request = SaveLocalTerminalProfileRequest {
            id: form.local_profile_id.clone(),
            name: form.name.trim().to_owned(),
            icon: (!form.icon.is_empty()).then(|| form.icon.clone()),
            color: (!form.color.is_empty()).then(|| form.color.clone()),
            icon_background_color: (!form.icon_background_color.is_empty())
                .then(|| form.icon_background_color.clone()),
            shell_id: form.local_shell_id.clone(),
            cwd: (!form.local_cwd.trim().is_empty()).then(|| form.local_cwd.trim().to_owned()),
            group: (!self.connection_form_group_is_ungrouped(&form.group))
                .then(|| form.group.trim().to_owned()),
        };
        let result = (|| -> Result<()> {
            if action != NewConnectionSubmitAction::Connect && request.name.is_empty() {
                anyhow::bail!(self.i18n.t("local_session.name_required"));
            }
            // Saving a portable profile does not require its shell or directory to exist here.
            let config = if action == NewConnectionSubmitAction::Save {
                None
            } else {
                Some(
                    self.local_profile_config(request.shell_id.as_deref(), request.cwd.as_deref())?,
                )
            };
            let title = if request.name.is_empty() {
                config
                    .as_ref()
                    .and_then(|config| config.shell.as_ref())
                    .map(|shell| shell.label.clone())
                    .unwrap_or_else(|| self.local_terminal_tab_title())
            } else {
                request.name.clone()
            };
            let profile_id = if action != NewConnectionSubmitAction::Connect {
                let profile = self
                    .connection_store
                    .upsert_local_terminal_profile(request)?;
                self.update_connection_form_state(cx, |state| {
                    if let Some(form) = state.form.as_mut() {
                        form.local_profile_id = Some(profile.id.clone());
                    }
                });
                self.queue_cloud_sync_dirty_refresh(cx);
                Some(profile.id)
            } else {
                None
            };
            if let Some(config) = config {
                let shell_id = config.shell.as_ref().map(|shell| shell.id.clone());
                let (session_id, _) =
                    self.create_local_terminal_tab_with_owned_session(config, title, window, cx)?;
                self.bind_local_profile(session_id, profile_id, cx);
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
            Ok(())
        })();
        match result {
            Ok(()) => self.close_new_connection_form(window, cx),
            Err(error) => self.update_connection_form_state(cx, |state| {
                if let Some(form) = state.form.as_mut() {
                    form.error = Some(error.to_string());
                }
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

    pub(super) fn open_saved_local_terminal_profile(
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
            Ok((session_id, _)) => self.bind_local_profile(session_id, Some(profile.id), cx),
            Err(error) => self.session_manager.update(cx, |manager, cx| {
                manager.set_status(Some(error.to_string()), cx)
            }),
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
