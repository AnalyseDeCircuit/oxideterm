use super::*;
pub(in crate::workspace) use oxideterm_gpui_settings_view::local_terminal::expand_local_terminal_cwd;
use oxideterm_gpui_settings_view::local_terminal::{
    LOCAL_GIT_BASH_ID, LocalTerminalPage, LocalTerminalRequest, effective_local_shells,
    resolve_local_shell,
};

impl WorkspaceApp {
    pub(in crate::workspace) fn effective_local_shells_for_settings(
        &self,
        settings: &PersistedSettings,
    ) -> Vec<ShellInfo> {
        effective_local_shells(
            &self.local_shells,
            settings.local_terminal.git_bash_path.as_deref(),
            &settings.local_terminal.shell_paths,
        )
    }

    pub(in crate::workspace) fn resolved_local_shell(
        &self,
        selected_shell_id: Option<&str>,
    ) -> Option<ShellInfo> {
        let settings = self.settings_store.settings();
        let effective_shells = self.effective_local_shells_for_settings(settings);
        resolve_local_shell(
            &effective_shells,
            selected_shell_id,
            settings.local_terminal.default_shell_id.as_deref(),
        )
    }

    fn with_local_terminal_page<R>(
        &self,
        cx: &Context<Self>,
        render: impl FnOnce(LocalTerminalPage<'_>, &App) -> R,
    ) -> R {
        let owner = cx.entity().downgrade();
        let dispatch = Rc::new(move |request, window: &mut Window, cx: &mut App| {
            let _ = owner.update(cx, |app, cx| match request {
                LocalTerminalRequest::BrowseShell(id) => {
                    app.blur_text_inputs(cx);
                    app.pick_local_shell_path(id, cx);
                }
                LocalTerminalRequest::ResetShell(id) => {
                    app.blur_text_inputs(cx);
                    app.set_local_shell_path(&id, None, cx);
                }
                LocalTerminalRequest::OpenPrivilegeCredentials => app
                    .open_privilege_credentials_settings(
                        Some(LOCAL_SHELL_PRIVILEGE_CONNECTION_ID.to_string()),
                        window,
                        cx,
                    ),
            });
        });
        self.with_settings_controls(cx, |controls, cx| {
            render(
                LocalTerminalPage {
                    controls,
                    shells: self
                        .effective_local_shells_for_settings(self.settings_store.settings()),
                    focused_input: self
                        .focused_settings_input
                        .map(|input| (input, self.settings_input_draft.as_str())),
                    privilege_credential_count: self
                        .connection_store
                        .list_privilege_credentials(LOCAL_SHELL_PRIVILEGE_CONNECTION_ID)
                        .map(|credentials| credentials.len())
                        .unwrap_or_default(),
                    dispatch,
                },
                cx,
            )
        })
    }

    pub(in crate::workspace) fn settings_local_section(
        &self,
        section: SettingsSectionId,
        cx: &Context<Self>,
    ) -> AnyElement {
        self.with_local_terminal_page(cx, |page, cx| page.settings_local_section(section, cx))
    }

    pub(in crate::workspace) fn settings_reconnect_section(
        &self,
        section: usize,
        cx: &Context<Self>,
    ) -> AnyElement {
        self.with_local_terminal_page(cx, |page, cx| page.settings_reconnect_section(section, cx))
    }

    pub(in crate::workspace) fn set_local_shell_path(
        &mut self,
        shell_id: &str,
        path: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.edit_settings(
            |settings| {
                if let Some(path) = path {
                    if settings.local_terminal.default_shell_id.is_none() {
                        settings.local_terminal.default_shell_id = Some(shell_id.to_string());
                    }
                    settings
                        .local_terminal
                        .shell_paths
                        .insert(shell_id.to_string(), path);
                } else {
                    settings.local_terminal.shell_paths.remove(shell_id);
                }
                if shell_id == LOCAL_GIT_BASH_ID {
                    settings.local_terminal.git_bash_path = None;
                }
            },
            cx,
        );
    }

    fn pick_local_shell_path(&mut self, shell_id: String, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(
                self.i18n
                    .t("settings_view.local_terminal.browse_shell")
                    .into(),
            ),
        });
        cx.spawn(async move |workspace, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = workspace.update(cx, |this, cx| {
                this.set_local_shell_path(&shell_id, Some(path.to_string_lossy().into_owned()), cx);
            });
        })
        .detach();
    }

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
}
