use super::*;
pub(in crate::workspace) use oxideterm_gpui_settings_view::local_terminal::expand_local_terminal_cwd;
use oxideterm_gpui_settings_view::local_terminal::{
    LOCAL_GIT_BASH_ID, LocalTerminalRequest, effective_local_shells,
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

    pub(super) fn handle_local_terminal_settings_request(
        &mut self,
        request: LocalTerminalRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match request {
            LocalTerminalRequest::BrowseShell(id) => {
                self.blur_text_inputs(cx);
                self.pick_local_shell_path(id, cx);
            }
            LocalTerminalRequest::ResetShell(id) => {
                self.blur_text_inputs(cx);
                self.set_local_shell_path(&id, None, cx);
            }
            LocalTerminalRequest::OpenPrivilegeCredentials => self
                .open_privilege_credentials_settings(
                    Some(LOCAL_SHELL_PRIVILEGE_CONNECTION_ID.to_string()),
                    window,
                    cx,
                ),
        }
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
        let selection = oxideterm_gpui_settings_view::entity::picker::pick_path(
            cx,
            self.i18n.t("settings_view.local_terminal.browse_shell"),
            false,
        );
        cx.spawn(async move |workspace, cx| {
            let Some(path) = selection.await else {
                return;
            };
            let _ = workspace.update(cx, |this, cx| {
                this.set_local_shell_path(&shell_id, Some(path.to_string_lossy().into_owned()), cx);
            });
        })
        .detach();
    }
}
