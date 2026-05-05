impl WorkspaceApp {
    fn local_shell_select_row(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let value = settings
            .local_terminal
            .default_shell_id
            .as_deref()
            .and_then(|id| self.local_shells.iter().find(|shell| shell.id == id))
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

    fn local_shell_path_hint(&self, settings: &PersistedSettings) -> Option<AnyElement> {
        let default_shell = settings
            .local_terminal
            .default_shell_id
            .as_deref()
            .and_then(|id| self.local_shells.iter().find(|shell| shell.id == id))
            .or_else(|| self.local_shells.first())?;

        Some(
            div()
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .text_color(rgb(self.tokens.ui.text_muted))
                .bg(rgba((self.tokens.ui.bg_panel << 8) | 0x4d))
                .p(px(12.0))
                .rounded(px(self.tokens.radii.sm))
                .border_1()
                .border_color(rgba((self.tokens.ui.border << 8) | 0x80))
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

    fn local_shortcut_row(&self, label_key: &str, shortcut: &'static str) -> AnyElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .py(px(8.0))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .child(
                div()
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t(label_key)),
            )
            .child(self.local_kbd(shortcut))
            .into_any_element()
    }

    fn local_kbd(&self, shortcut: &'static str) -> AnyElement {
        div()
            .px(px(8.0))
            .py(px(4.0))
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_hover))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(shortcut)
            .into_any_element()
    }

    fn available_shell_row(&self, shell: &ShellInfo, default_shell_id: Option<&str>) -> AnyElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .p(px(12.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((self.tokens.ui.border << 8) | 0x80))
            .bg(rgba((self.tokens.ui.bg_panel << 8) | 0x4d))
            .child(
                div().flex().flex_row().items_center().gap(px(12.0)).child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                .text_color(rgb(self.tokens.ui.text))
                                .child(shell.label.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(self.tokens.ui.text_muted))
                                .child(shell.path.display().to_string()),
                        ),
                ),
            )
            .when(default_shell_id == Some(shell.id.as_str()), |row| {
                row.child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.warning))
                        .child(self.i18n.t("settings_view.local_terminal.default")),
                )
            })
            .into_any_element()
    }

    pub(super) fn local_terminal_config(&self) -> LocalPtyConfig {
        let settings = &self.settings_store.settings().local_terminal;
        let shell = settings
            .default_shell_id
            .as_deref()
            .and_then(|id| self.local_shells.iter().find(|shell| shell.id == id))
            .cloned();
        let cwd = settings
            .default_cwd
            .as_deref()
            .map(str::trim)
            .filter(|cwd| !cwd.is_empty())
            .map(PathBuf::from);
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
            oh_my_posh_enabled: settings.oh_my_posh_enabled,
            oh_my_posh_theme: settings.oh_my_posh_theme.clone(),
        }
    }

    pub(super) fn local_terminal_tab_title(&self) -> String {
        let settings = &self.settings_store.settings().local_terminal;
        settings
            .default_shell_id
            .as_deref()
            .and_then(|id| self.local_shells.iter().find(|shell| shell.id == id))
            .or_else(|| self.local_shells.first())
            .map(|shell| shell.label.clone())
            .unwrap_or_else(|| "Local".to_string())
    }
}
