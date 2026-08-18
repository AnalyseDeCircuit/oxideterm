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
// Compact shell rows keep stable scan columns without nesting another card surface.
const LOCAL_SHELL_NAME_COLUMN_WIDTH: f32 = 160.0;
const LOCAL_SHELL_PATH_COLUMN_WIDTH: f32 = 240.0;
const LOCAL_SHELL_ROW_VERTICAL_PADDING: f32 = 8.0;

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
) -> Vec<ShellInfo> {
    let Some(override_shell) = local_git_bash_override(git_bash_path) else {
        return shells.to_vec();
    };

    let mut effective = shells
        .iter()
        .filter(|shell| shell.id != LOCAL_GIT_BASH_ID)
        .cloned()
        .collect::<Vec<_>>();
    effective.push(override_shell);
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
        let value = settings
            .local_terminal
            .default_shell_id
            .as_deref()
            .and_then(|id| effective_shells.iter().find(|shell| shell.id == id))
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

    pub(in crate::workspace) fn local_shell_path_hint(
        &self,
        settings: &PersistedSettings,
    ) -> Option<AnyElement> {
        let effective_shells = self.effective_local_shells_for_settings(settings);
        let default_shell = settings
            .local_terminal
            .default_shell_id
            .as_deref()
            .and_then(|id| effective_shells.iter().find(|shell| shell.id == id))
            .or_else(|| effective_shells.first())?
            .clone();

        Some(
            div()
                .w_full()
                .min_w(px(0.0))
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_color(rgb(self.tokens.ui.text_muted))
                                .child(format!(
                                    "{}:",
                                    self.i18n.t("settings_view.local_terminal.path")
                                )),
                        )
                        .child(
                            div()
                                .text_color(rgb(self.tokens.ui.text))
                                .child(default_shell.path.display().to_string()),
                        ),
                )
                .into_any_element(),
        )
    }

    pub(in crate::workspace) fn available_shell_row(
        &self,
        index: usize,
        shell: &ShellInfo,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let default_shell_id = settings.local_terminal.default_shell_id.as_deref();
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
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_wrap()
            .items_center()
            .gap_x(px(16.0))
            .gap_y(px(4.0))
            .py(px(LOCAL_SHELL_ROW_VERTICAL_PADDING))
            .child(
                div()
                    .w(px(LOCAL_SHELL_NAME_COLUMN_WIDTH))
                    .max_w_full()
                    .flex_none()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(shell.label.clone()),
            )
            .child(
                div()
                    .w(px(LOCAL_SHELL_PATH_COLUMN_WIDTH))
                    .max_w_full()
                    .min_w(px(0.0))
                    .flex_1()
                    .truncate()
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(shell.path.display().to_string()),
            )
            .when(default_shell_id == Some(shell.id.as_str()), |row| {
                row.child(self.text_badge(
                    self.i18n.t("settings_view.local_terminal.default"),
                    self.tokens.ui.warning,
                ))
            })
            .child(self.settings_select_control(
                SettingsSelect::LocalShellSemanticScheme(index),
                scheme_label,
                false,
                Some(196.0),
                cx,
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn local_terminal_config(&self) -> LocalPtyConfig {
        let settings = &self.settings_store.settings().local_terminal;
        let effective_shells =
            effective_local_shells(&self.local_shells, settings.git_bash_path.as_deref());
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
        let effective_shells =
            effective_local_shells(&self.local_shells, settings.git_bash_path.as_deref());
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
    pub(in crate::workspace) fn git_bash_override_replaces_scanned_git_bash_shell() {
        let shells = vec![
            ShellInfo::new("cmd", "Command Prompt", "cmd.exe"),
            ShellInfo::new("git-bash", "Git Bash", r"C:\Program Files\Git\bin\bash.exe"),
        ];

        let effective = effective_local_shells(&shells, Some(r" D:\PortableGit\bin\bash.exe "));

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
        assert_eq!(effective_local_shells(&shells, Some("  ")), shells);
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
