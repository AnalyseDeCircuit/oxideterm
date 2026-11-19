use std::{path::PathBuf, rc::Rc};

use gpui::{AnyElement, App, Div, MouseButton, MouseDownEvent, Window, div, prelude::*, px, rgb};
use oxideterm_gpui_ui::button::{
    ButtonOptions, ButtonSize, ButtonVariant, ToolbarButtonOptions, toolbar_button,
};
use oxideterm_i18n::I18n;
use oxideterm_settings::{PersistedSettings, TerminalSemanticScheme};
use oxideterm_settings_model::{SettingsSectionId, reconnect_attempt_label, reconnect_delay_label};
use oxideterm_terminal::ShellInfo;

use crate::{
    SettingsInput, SettingsSelect,
    components::{SettingsCalloutTone, SettingsControls},
    set_load_shell_profile, set_oh_my_posh, set_reconnect_enabled, settings_row,
    surface::nav::render_icon,
    terminal_semantic_scheme_label,
};

const SETTINGS_RECONNECT_FIELD_BASIS: f32 = 240.0;
const SETTINGS_RECONNECT_HINT_LINE_HEIGHT: f32 = 16.0;

#[derive(Clone)]
pub enum LocalTerminalRequest {
    BrowseShell(String),
    ResetShell(String),
    OpenPrivilegeCredentials,
}

pub struct LocalTerminalPage<'a> {
    pub controls: SettingsControls<'a>,
    pub shells: Vec<ShellInfo>,
    pub focused_input: Option<(SettingsInput, &'a str)>,
    pub privilege_credential_count: usize,
    pub dispatch: Rc<dyn Fn(LocalTerminalRequest, &mut Window, &mut App)>,
}

impl<'a> std::ops::Deref for LocalTerminalPage<'a> {
    type Target = SettingsControls<'a>;

    fn deref(&self) -> &Self::Target {
        &self.controls
    }
}

impl LocalTerminalPage<'_> {
    fn action(
        &self,
        action: LocalTerminalRequest,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + use<> {
        let dispatch = self.dispatch.clone();
        move |_, window, cx| {
            dispatch(action.clone(), window, cx);
            cx.stop_propagation();
        }
    }

    fn workspace_toolbar_action_button(
        &self,
        label: String,
        icon: Option<AnyElement>,
        options: ToolbarButtonOptions,
        action: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        toolbar_button(self.tokens, label, icon, options).on_mouse_down(MouseButton::Left, action)
    }

    pub fn local_shell_select_row(&self, settings: &PersistedSettings, cx: &App) -> AnyElement {
        let effective_shells = &self.shells;
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

    pub fn local_shell_options_rows(
        &self,
        settings: &PersistedSettings,
        cx: &App,
    ) -> Vec<AnyElement> {
        let effective_shells = &self.shells;
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
            let path = settings
                .local_terminal
                .shell_paths
                .get(&shell.id)
                .or_else(|| {
                    (shell.id == LOCAL_GIT_BASH_ID)
                        .then_some(&settings.local_terminal.git_bash_path)
                        .and_then(Option::as_ref)
                })
                .cloned()
                .unwrap_or_default();
            let draft = if let Some((focused, draft)) = self.focused_input
                && focused == input
            {
                draft
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
                    self.action(LocalTerminalRequest::BrowseShell(browse_shell)),
                ))
                .when(!path.is_empty(), |controls| {
                    controls.child(self.workspace_toolbar_action_button(
                        self.i18n.t("settings_view.local_terminal.reset_shell_path"),
                        None,
                        ToolbarButtonOptions::default(),
                        self.action(LocalTerminalRequest::ResetShell(reset_shell)),
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

    pub fn settings_local_section(&self, section: SettingsSectionId, cx: &App) -> AnyElement {
        let settings = self.settings;
        match section {
            SettingsSectionId::TerminalLocalShell => {
                let mut shell_rows = vec![self.local_shell_select_row(settings, cx)];
                shell_rows.extend(self.local_shell_options_rows(settings, cx));
                shell_rows.push(self.card_separator());
                shell_rows.push(
                    self.setting_row(
                        "settings_view.local_terminal.default_cwd",
                        "settings_view.local_terminal.default_cwd_hint",
                        self.settings_text_input_control(
                            SettingsInput::LocalDefaultCwd,
                            settings
                                .local_terminal
                                .default_cwd
                                .clone()
                                .unwrap_or_default(),
                            "~".to_string(),
                            self.tokens.metrics.settings_select_width,
                            cx,
                        ),
                        cx,
                    ),
                );
                if cfg!(target_os = "windows")
                    && local_shell_supports_oh_my_posh(
                        settings.local_terminal.default_shell_id.as_deref(),
                    )
                {
                    shell_rows.push(self.card_separator());
                    shell_rows.extend(self.local_oh_my_posh_rows(settings, cx));
                }
                self.settings_card("settings_view.local_terminal.shell", "", shell_rows)
            }
            SettingsSectionId::TerminalLocalShellProfile => self.settings_card(
                "settings_view.local_terminal.shell_profile",
                "",
                vec![self.bool_row(
                    "settings_view.local_terminal.load_shell_profile",
                    "settings_view.local_terminal.load_shell_profile_hint",
                    settings.local_terminal.load_shell_profile,
                    set_load_shell_profile,
                    cx,
                )],
            ),
            SettingsSectionId::TerminalLocalPrivilegeCredentials => {
                self.local_privilege_credentials_card()
            }
            _ => div().into_any_element(),
        }
    }

    fn local_oh_my_posh_rows(&self, settings: &PersistedSettings, cx: &App) -> Vec<AnyElement> {
        let mut rows = vec![
            self.card_subsection_header(
                self.i18n.t("settings_view.local_terminal.oh_my_posh"),
                None,
            ),
            self.bool_row(
                "settings_view.local_terminal.oh_my_posh_enable",
                "settings_view.local_terminal.oh_my_posh_enable_hint",
                settings.local_terminal.oh_my_posh_enabled,
                set_oh_my_posh,
                cx,
            ),
        ];
        if settings.local_terminal.oh_my_posh_enabled {
            rows.push(
                self.settings_callout(
                    SettingsCalloutTone::Info,
                    Some("lucide/info.svg"),
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.info))
                        .child(self.i18n.t("settings_view.local_terminal.oh_my_posh_note"))
                        .into_any_element(),
                ),
            );
            rows.push(self.card_separator());
            rows.push(
                self.setting_row(
                    "settings_view.local_terminal.oh_my_posh_theme",
                    "settings_view.local_terminal.oh_my_posh_theme_hint",
                    self.settings_text_input_control(
                        SettingsInput::LocalOhMyPoshTheme,
                        settings
                            .local_terminal
                            .oh_my_posh_theme
                            .clone()
                            .unwrap_or_default(),
                        self.i18n
                            .t("settings_view.local_terminal.oh_my_posh_theme_placeholder"),
                        300.0,
                        cx,
                    ),
                    cx,
                ),
            );
        }
        rows
    }

    pub fn local_privilege_credentials_card(&self) -> AnyElement {
        let theme = self.tokens.ui;
        let credential_count = self.privilege_credential_count;
        let summary = settings_row(
            &self.tokens,
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(8.0))
                .text_size(px(self.tokens.metrics.ui_text_sm))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgb(theme.text))
                .child(render_icon(
                    "lucide/key-round.svg",
                    16.0,
                    rgb(theme.text_muted),
                ))
                .child(
                    self.i18n
                        .t("settings_view.privilege_credentials.credential_count")
                        .replace("{{count}}", &credential_count.to_string()),
                )
                .into_any_element(),
            Some(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(
                        self.i18n
                            .t("settings_view.privilege_credentials.description"),
                    )
                    .into_any_element(),
            ),
            self.workspace_toolbar_action_button(
                self.i18n.t("terminal.privilege_helper.manage"),
                Some(
                    render_icon("lucide/settings.svg", 14.0, rgb(theme.text_muted))
                        .into_any_element(),
                ),
                ToolbarButtonOptions {
                    button: ButtonOptions {
                        variant: ButtonVariant::Outline,
                        size: ButtonSize::Sm,
                        ..ButtonOptions::default()
                    },
                    ..ToolbarButtonOptions::default()
                },
                self.action(LocalTerminalRequest::OpenPrivilegeCredentials),
            )
            .into_any_element(),
        );

        self.settings_card(
            "settings_view.local_terminal.privilege_credentials",
            "settings_view.local_terminal.privilege_credentials_hint",
            vec![summary.into_any_element()],
        )
    }

    pub fn settings_reconnect_section(&self, section_index: usize, cx: &App) -> AnyElement {
        if section_index != 0 {
            return div().into_any_element();
        }

        let settings = self.settings;
        let reconnect_enabled = settings.reconnect.enabled;
        // Keep reconnect controls in one virtual-list item so the shared card
        // surface cannot be split by list spacing or independent measurement.
        let strategy = div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_card_gap))
            .opacity(if reconnect_enabled { 1.0 } else { 0.4 })
            .child(
                self.card_subsection_header(self.i18n.t("settings_view.reconnect.strategy"), None),
            )
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap(px(32.0))
                    .child(self.reconnect_select_field(
                        "settings_view.reconnect.max_attempts",
                        "settings_view.reconnect.max_attempts_hint",
                        SettingsSelect::ReconnectMaxAttempts,
                        reconnect_attempt_label(settings.reconnect.max_attempts),
                        reconnect_enabled,
                        cx,
                    ))
                    .child(self.reconnect_select_field(
                        "settings_view.reconnect.base_delay",
                        "settings_view.reconnect.base_delay_hint",
                        SettingsSelect::ReconnectBaseDelay,
                        reconnect_delay_label(settings.reconnect.base_delay_ms),
                        reconnect_enabled,
                        cx,
                    ))
                    .child(self.reconnect_select_field(
                        "settings_view.reconnect.max_delay",
                        "settings_view.reconnect.max_delay_hint",
                        SettingsSelect::ReconnectMaxDelay,
                        reconnect_delay_label(settings.reconnect.max_delay_ms),
                        reconnect_enabled,
                        cx,
                    )),
            )
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .pt(px(4.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.reconnect.formula_hint")),
            )
            .into_any_element();

        self.settings_card(
            "settings_view.reconnect.title",
            "settings_view.reconnect.description",
            vec![
                self.reconnect_enabled_row(reconnect_enabled, cx),
                self.card_separator(),
                strategy,
            ],
        )
    }

    pub fn reconnect_enabled_row(&self, checked: bool, cx: &App) -> AnyElement {
        self.bool_row(
            "settings_view.reconnect.enabled",
            "settings_view.reconnect.enabled_hint",
            checked,
            set_reconnect_enabled,
            cx,
        )
    }

    pub fn reconnect_select_field(
        &self,
        label_key: &str,
        hint_key: &str,
        select_id: SettingsSelect,
        value: String,
        enabled: bool,
        cx: &App,
    ) -> AnyElement {
        let control = self.settings_select_control(select_id, value, !enabled, None, cx);

        div()
            .min_w_0()
            .max_w_full()
            .flex_1()
            .flex_basis(px(SETTINGS_RECONNECT_FIELD_BASIS))
            .grid()
            .gap(px(8.0))
            .child(
                div()
                    .grid()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t(label_key)),
                    )
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .whitespace_normal()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .line_height(px(SETTINGS_RECONNECT_HINT_LINE_HEIGHT))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t(hint_key)),
                    ),
            )
            .child(control)
            .when(!enabled, |field| {
                field.on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                    cx.stop_propagation();
                })
            })
            .into_any_element()
    }
}

pub fn semantic_scheme_label_for_id(
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

use crate::select_menu::application_semantic_scheme_label;

pub const LOCAL_GIT_BASH_ID: &str = "git-bash";

pub fn local_shell_supports_oh_my_posh(shell_id: Option<&str>) -> bool {
    // The native injector is compiled only for these Windows PowerShell IDs.
    matches!(shell_id, Some("powershell" | "pwsh"))
}

pub fn normalized_local_git_bash_path(path: Option<&str>) -> Option<PathBuf> {
    let path = path?.trim();
    (!path.is_empty()).then(|| PathBuf::from(path))
}

pub fn local_home_path_buf() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

/// Expands home aliases before the configured cwd is passed to the PTY layer.
pub fn expand_local_terminal_cwd(path: &str) -> PathBuf {
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

pub fn local_git_bash_override(path: Option<&str>) -> Option<ShellInfo> {
    let path = normalized_local_git_bash_path(path)?;
    Some(ShellInfo::new(LOCAL_GIT_BASH_ID, "Git Bash", path).with_args(vec!["--login".to_string()]))
}

pub fn effective_local_shells(
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

pub fn resolve_local_shell(
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
    pub fn git_bash_override_replaces_scanned_git_bash_shell() {
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
    pub fn one_shot_shell_choice_precedes_default_and_fallback() {
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
