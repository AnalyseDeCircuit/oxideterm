use super::*;

pub(in crate::workspace) fn semantic_scheme_label_for_id(
    settings: &PersistedSettings,
    scheme_id: &str,
    i18n: &I18n,
) -> String {
    match scheme_id {
        "balanced" => terminal_semantic_scheme_label(TerminalSemanticScheme::Balanced, i18n),
        "conservative" => {
            terminal_semantic_scheme_label(TerminalSemanticScheme::Conservative, i18n)
        }
        custom_id => settings
            .terminal
            .custom_semantic_schemes
            .iter()
            .find(|scheme| scheme.id == custom_id)
            .map(|scheme| scheme.name.clone())
            .unwrap_or_else(|| custom_id.to_string()),
    }
}

pub(in crate::workspace) fn application_semantic_scheme_label(
    settings: &PersistedSettings,
    i18n: &I18n,
) -> String {
    settings
        .terminal
        .active_custom_semantic_scheme()
        .map(|scheme| scheme.name.clone())
        .unwrap_or_else(|| terminal_semantic_scheme_label(settings.terminal.semantic_scheme, i18n))
}

pub(in crate::workspace) const LOCAL_GIT_BASH_ID: &str = "git-bash";

pub(in crate::workspace) fn local_shell_supports_oh_my_posh(shell_id: Option<&str>) -> bool {
    // The native injector is compiled only for these Windows PowerShell IDs.
    matches!(shell_id, Some("powershell" | "pwsh"))
}

pub(in crate::workspace) fn normalized_local_git_bash_path(path: Option<&str>) -> Option<PathBuf> {
    let path = path?.trim();
    (!path.is_empty()).then(|| PathBuf::from(path))
}

pub(in crate::workspace) fn local_home_path_buf() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

/// Expands home aliases before the configured cwd is passed to the PTY layer.
pub(in crate::workspace) fn expand_local_terminal_cwd(path: &str) -> PathBuf {
    let trimmed = path.trim();
    if trimmed == "~" || trimmed == "$HOME" {
        return local_home_path_buf().unwrap_or_else(|| PathBuf::from(trimmed));
    }

    for prefix in ["~/", "~\\", "$HOME/", "$HOME\\"] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            if let Some(home) = local_home_path_buf() {
                return home.join(rest);
            }
        }
    }

    PathBuf::from(trimmed)
}

pub(in crate::workspace) fn local_git_bash_override(path: Option<&str>) -> Option<ShellInfo> {
    let path = normalized_local_git_bash_path(path)?;
    Some(ShellInfo::new(LOCAL_GIT_BASH_ID, "Git Bash", path).with_args(vec!["--login".to_string()]))
}

pub(in crate::workspace) fn effective_local_shells(
    shells: &[ShellInfo],
    git_bash_path: Option<&str>,
    shell_paths: &std::collections::BTreeMap<String, String>,
) -> Vec<ShellInfo> {
    let mut effective = shells.to_vec();
    if let Some(override_shell) = local_git_bash_override(git_bash_path) {
        if let Some(shell) = effective
            .iter_mut()
            .find(|shell| shell.id == LOCAL_GIT_BASH_ID)
        {
            *shell = override_shell;
        } else {
            effective.push(override_shell);
        }
    }
    for (id, path) in shell_paths {
        if id.starts_with("wsl") || path.trim().is_empty() {
            continue;
        }
        let path = expand_local_terminal_cwd(path);
        let mut found = false;
        for shell in effective.iter_mut().filter(|shell| shell.id == *id) {
            shell.path = path.clone();
            found = true;
        }
        if !found {
            // Keep explicit choices selectable even if discovery no longer finds the executable.
            effective.push(ShellInfo::new(id, id, path));
        }
    }
    effective
}

pub(in crate::workspace) fn resolve_local_shell(
    shells: &[ShellInfo],
    selected_shell_id: Option<&str>,
    default_shell_id: Option<&str>,
) -> Option<ShellInfo> {
    // An explicit one-shot choice wins, then the configured default, then discovery order.
    selected_shell_id
        .and_then(|id| shells.iter().find(|shell| shell.id == id))
        .or_else(|| default_shell_id.and_then(|id| shells.iter().find(|shell| shell.id == id)))
        .or_else(|| shells.first())
        .cloned()
}

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

    pub(in crate::workspace) fn local_shell_select_row(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let effective_shells = self.effective_local_shells_for_settings(settings);
        let default_shell_id = settings
            .local_terminal
            .default_shell_id
            .clone()
            .unwrap_or_else(|| oxideterm_terminal::default_shell().id);
        let value = effective_shells
            .iter()
            .find(|shell| shell.id == default_shell_id)
            .or_else(|| effective_shells.first())
            .map(|shell| shell.label.clone())
            .unwrap_or_else(|| self.i18n.t("settings_view.local_terminal.select_shell"));

        self.select_setting_row(
            "settings_view.local_terminal.default_shell",
            "settings_view.local_terminal.default_shell_hint",
            SettingsSelect::LocalShell,
            value,
            self.tokens.metrics.settings_select_width,
            cx,
        )
    }

    pub(in crate::workspace) fn local_shell_options_rows(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let effective_shells = self.effective_local_shells_for_settings(settings);
        let default_shell_id = settings
            .local_terminal
            .default_shell_id
            .clone()
            .unwrap_or_else(|| oxideterm_terminal::default_shell().id);
        let index = effective_shells
            .iter()
            .position(|shell| shell.id == default_shell_id)
            .unwrap_or(0);
        let Some(shell) = effective_shells.get(index) else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        if !shell.id.starts_with("wsl") {
            let input = SettingsInput::LocalShellPath(index);
            let path = self.current_settings_input_value(input, cx);
            let draft = if self.focused_settings_input == Some(input) {
                self.settings_input_draft.as_str()
            } else {
                path.as_str()
            };
            let resolved = expand_local_terminal_cwd(draft);
            let invalid = !draft.trim().is_empty()
                && (!resolved.is_absolute() || !oxideterm_terminal::is_shell_executable(&resolved));
            let browse_shell = shell.id.clone();
            let reset_shell = shell.id.clone();
            let controls = div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(8.0))
                .child(self.settings_text_input_control(
                    input,
                    path.clone(),
                    shell.path.display().to_string(),
                    self.tokens.metrics.settings_select_width,
                    cx,
                ))
                .child(self.workspace_toolbar_action_button(
                    self.i18n.t("settings_view.local_terminal.browse_shell"),
                    None,
                    ToolbarButtonOptions::default(),
                    cx.listener(move |this, _event, _window, cx| {
                        this.blur_text_inputs(cx);
                        this.pick_local_shell_path(browse_shell.clone(), cx);
                        cx.stop_propagation();
                    }),
                ))
                .when(!path.is_empty(), |controls| {
                    controls.child(self.workspace_toolbar_action_button(
                        self.i18n.t("settings_view.local_terminal.reset_shell_path"),
                        None,
                        ToolbarButtonOptions::default(),
                        cx.listener(move |this, _event, _window, cx| {
                            this.blur_text_inputs(cx);
                            this.set_local_shell_path(&reset_shell, None, cx);
                            cx.stop_propagation();
                        }),
                    ))
                })
                .into_any_element();
            rows.push(self.card_separator());
            rows.push(self.setting_row(
                "settings_view.local_terminal.shell_path",
                "settings_view.local_terminal.shell_path_hint",
                controls,
                cx,
            ));
            if invalid {
                rows.push(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.error))
                        .child(
                            self.i18n
                                .t("settings_view.local_terminal.shell_path_invalid"),
                        )
                        .into_any_element(),
                );
            }
        }
        let scheme_label = settings
            .local_terminal
            .semantic_scheme_for_shell(&shell.id)
            .map(|scheme_id| semantic_scheme_label_for_id(settings, scheme_id, &self.i18n))
            .unwrap_or_else(|| {
                self.i18n
                    .t("ssh.form.terminal_use_application_default")
                    .replace(
                        "{{value}}",
                        &application_semantic_scheme_label(settings, &self.i18n),
                    )
            });
        rows.push(self.card_separator());
        rows.push(self.setting_row(
            "settings_view.local_terminal.shell_scheme",
            "",
            self.settings_select_control(
                SettingsSelect::LocalShellSemanticScheme(index),
                scheme_label,
                false,
                Some(self.tokens.metrics.settings_select_width),
                cx,
            ),
            cx,
        ));
        rows
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
            shell,
            cwd,
            env,
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

#[cfg(test)]
mod local_terminal_tests {
    use super::*;

    #[test]
    fn custom_shell_paths_keep_launch_identity_and_missing_executables() {
        let shells = vec![
            ShellInfo::new("bash", "Bash", "/bin/bash").with_args(vec!["--login".into()]),
            ShellInfo::new("zsh", "Zsh", "/bin/zsh"),
        ];
        let paths = std::collections::BTreeMap::from([
            ("bash".into(), "/custom path/bash".into()),
            ("fish".into(), "/missing/fish".into()),
        ]);
        let effective = effective_local_shells(&shells, None, &paths);
        assert_eq!(
            effective,
            vec![
                ShellInfo::new("bash", "Bash", "/custom path/bash")
                    .with_args(vec!["--login".into()]),
                ShellInfo::new("zsh", "Zsh", "/bin/zsh"),
                ShellInfo::new("fish", "fish", "/missing/fish"),
            ]
        );
        assert_eq!(
            resolve_local_shell(&effective, None, Some("fish"))
                .unwrap()
                .path,
            PathBuf::from("/missing/fish")
        );
        assert_eq!(
            effective_local_shells(&shells, None, &Default::default()),
            shells
        );
    }

    #[test]
    pub(in crate::workspace) fn git_bash_override_replaces_scanned_git_bash_shell() {
        let shells = vec![
            ShellInfo::new("cmd", "Command Prompt", "cmd.exe"),
            ShellInfo::new("git-bash", "Git Bash", r"C:\Program Files\Git\bin\bash.exe"),
        ];

        let effective = effective_local_shells(
            &shells,
            Some(r" D:\PortableGit\bin\bash.exe "),
            &Default::default(),
        );

        assert_eq!(effective.len(), 2);
        let git_bash = effective
            .iter()
            .find(|shell| shell.id == LOCAL_GIT_BASH_ID)
            .expect("git bash override should be present");
        assert_eq!(git_bash.path, PathBuf::from(r"D:\PortableGit\bin\bash.exe"));
        assert_eq!(git_bash.args, vec!["--login"]);
    }

    #[test]
    pub(in crate::workspace) fn blank_git_bash_override_keeps_scanned_shells() {
        let shells = vec![ShellInfo::new("cmd", "Command Prompt", "cmd.exe")];
        assert_eq!(
            effective_local_shells(&shells, Some("  "), &Default::default()),
            shells
        );
    }

    #[test]
    pub(in crate::workspace) fn oh_my_posh_controls_only_match_powershell_shells() {
        assert!(local_shell_supports_oh_my_posh(Some("powershell")));
        assert!(local_shell_supports_oh_my_posh(Some("pwsh")));
        assert!(!local_shell_supports_oh_my_posh(Some("wsl-ubuntu")));
        assert!(!local_shell_supports_oh_my_posh(Some("zsh")));
        assert!(!local_shell_supports_oh_my_posh(None));
    }

    #[test]
    pub(in crate::workspace) fn one_shot_shell_choice_precedes_default_and_fallback() {
        let shells = vec![
            ShellInfo::new("bash", "Bash", "/bin/bash"),
            ShellInfo::new("zsh", "Zsh", "/bin/zsh"),
        ];

        assert_eq!(
            resolve_local_shell(&shells, Some("zsh"), Some("bash")).map(|shell| shell.id),
            Some("zsh".to_string())
        );
        assert_eq!(
            resolve_local_shell(&shells, Some("missing"), Some("zsh")).map(|shell| shell.id),
            Some("zsh".to_string())
        );
        assert_eq!(
            resolve_local_shell(&shells, None, None).map(|shell| shell.id),
            Some("bash".to_string())
        );
    }
}
