use std::rc::Rc;

use gpui::{
    AnyElement, App, Div, Entity, MouseButton, MouseDownEvent, Window, div, prelude::*, px, rgb,
    rgba,
};
use oxideterm_gpui_ui::button::{
    ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonIconPosition,
    ToolbarButtonOptions, toolbar_button,
};
#[cfg(not(target_os = "macos"))]
use oxideterm_gpui_ui::switch::{SwitchOptions, switch_with};
use oxideterm_settings::DataDirectoryInfo;
use oxideterm_settings_model::SettingsSectionId;

use crate::{
    cli_companion::{
        CLI_COMPANION_COMMAND_NAME, LEGACY_CLI_COMPANION_COMMAND_NAME, cli_install_path,
    },
    components::{SettingsCalloutTone, SettingsControls, settings_mono_font_family},
    entity::{CliCompanionOperation, LaunchAtLoginError, SettingsWorkspaceEntity},
    select_menu::language_label,
    surface::nav::render_icon,
    *,
};

mod device;
pub use device::render_data_directory_confirm;

const TERMINAL_COMMAND_SPECS_ACTION_ICON_SIZE: f32 = 12.0;

#[derive(Clone, Copy)]
pub enum GeneralRequest {
    Cli(CliCompanionOperation),
    OpenLoginItems,
    SetLaunchAtLogin(bool),
    PickDataDirectory,
    ResetDataDirectory,
    ClearCommandHistory,
    OpenCommandSpecs,
}

pub struct GeneralPage<'a> {
    pub controls: SettingsControls<'a>,
    pub state: &'a Entity<SettingsWorkspaceEntity>,
    pub data_dir: Option<DataDirectoryInfo>,
    pub history_failed: bool,
    pub focus_handoff: Option<AnyElement>,
    pub command_specs_path: String,
    pub command_specs_summary: String,
    pub dispatch: Rc<dyn Fn(GeneralRequest, &mut Window, &mut App)>,
}

impl<'a> std::ops::Deref for GeneralPage<'a> {
    type Target = SettingsControls<'a>;

    fn deref(&self) -> &Self::Target {
        &self.controls
    }
}

pub fn render_focus_handoff_commands(
    components: &crate::components::SettingsComponents<'_>,
    mono_font: gpui::SharedString,
    control: AnyElement,
    toggle: Rc<dyn Fn(String, &mut App)>,
) -> AnyElement {
    let settings = components.settings;
    let mut presets = div().w_full().flex().flex_row().flex_wrap().gap(px(8.0));
    for command in oxideterm_settings::RECOMMENDED_FOCUS_HANDOFF_COMMANDS {
        let enabled = settings
            .terminal
            .command_bar
            .focus_handoff_commands
            .iter()
            .any(|candidate| candidate == command);
        let command_name = (*command).to_string();
        let theme = components.tokens.ui;
        let chip = div()
            .h(px(32.0))
            .px(px(12.0))
            .rounded(px(components.tokens.radii.md))
            .border_1()
            .border_color(if enabled {
                rgba((theme.accent << 8) | 0x99)
            } else {
                rgba((theme.border << 8) | 0x99)
            })
            .bg(if enabled {
                rgba((theme.accent << 8) | 0x20)
            } else {
                rgba(0x00000000)
            })
            .flex()
            .items_center()
            .gap(px(6.0))
            .cursor_pointer()
            .text_size(px(components.tokens.metrics.ui_text_sm))
            .font_family(mono_font.clone())
            .text_color(if enabled {
                rgb(theme.accent)
            } else {
                rgb(theme.text_muted)
            })
            .hover(move |style| {
                style
                    .border_color(rgba((theme.accent << 8) | 0x88))
                    .bg(rgb(theme.bg_hover))
                    .text_color(rgb(theme.text))
            })
            .when(enabled, |chip| chip.child("✓"))
            .child(command_name.clone())
            .on_mouse_down(MouseButton::Left, {
                let toggle = toggle.clone();
                move |_, _, cx| {
                    toggle(command_name.clone(), cx);
                    cx.stop_propagation();
                }
            });
        presets = presets.child(chip);
    }

    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            components.card_subsection_header(
                components
                    .i18n
                    .t("settings_view.terminal.command_bar_focus_handoff_presets"),
                Some(
                    components
                        .i18n
                        .t("settings_view.terminal.command_bar_focus_handoff_presets_hint"),
                ),
            ),
        )
        .child(presets)
        .child(components.card_separator())
        .child(
            components.card_subsection_header(
                components
                    .i18n
                    .t("settings_view.terminal.command_bar_focus_handoff_advanced"),
                Some(
                    components
                        .i18n
                        .t("settings_view.terminal.command_bar_focus_handoff_advanced_hint"),
                ),
            ),
        )
        .child(control)
        .into_any_element()
}

impl GeneralPage<'_> {
    fn action(
        &self,
        request: GeneralRequest,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + use<> {
        let dispatch = self.dispatch.clone();
        move |_, window, cx| {
            dispatch(request, window, cx);
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
        let enabled = !(options.button.disabled || options.loading);
        toolbar_button(self.tokens, label, icon, options).on_mouse_down(
            MouseButton::Left,
            move |event, window, cx| {
                if enabled {
                    action(event, window, cx);
                }
                cx.stop_propagation();
            },
        )
    }

    fn cli_companion_action_button(
        &self,
        label: String,
        icon: &'static str,
        variant: ButtonVariant,
        loading: bool,
        request: GeneralRequest,
    ) -> AnyElement {
        self.workspace_toolbar_action_button(
            label,
            Some(render_icon(icon, 14.0, rgb(self.tokens.ui.text))),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                icon_position: ToolbarButtonIconPosition::Leading,
                loading,
                ..ToolbarButtonOptions::default()
            },
            self.action(request),
        )
        .into_any_element()
    }

    pub fn settings_general_section(&self, section: SettingsSectionId, cx: &App) -> AnyElement {
        let settings = self.settings;
        match section {
            SettingsSectionId::GeneralLanguage => self.settings_card(
                "settings_view.general.language",
                "",
                vec![self.select_setting_row(
                    "settings_view.general.language",
                    "settings_view.general.language_hint",
                    SettingsSelect::Language,
                    language_label(settings.general.language),
                    self.tokens.metrics.settings_select_width,
                    cx,
                )],
            ),
            SettingsSectionId::GeneralExternalConnectionUris => self.settings_card(
                "settings_view.general.connection_uri_integration",
                "settings_view.general.connection_uri_integration_hint",
                vec![self.bool_row(
                    "settings_view.general.external_connection_uris",
                    "settings_view.general.external_connection_uris_hint",
                    settings.general.external_connection_uris_enabled,
                    |settings, enabled| settings.general.external_connection_uris_enabled = enabled,
                    cx,
                )],
            ),
            SettingsSectionId::GeneralDataDirectory => {
                let Some(data_dir_info) = self.data_dir.as_ref() else {
                    return div().into_any_element();
                };
                let data_dir = data_dir_info.path.display().to_string();
                self.settings_card(
                    "settings_view.general.data_directory",
                    "settings_view.general.data_directory_hint",
                    vec![
                        div()
                            .w_full()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(16.0))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .text_size(px(self.tokens.metrics.ui_text_base))
                                    .text_color(rgb(self.tokens.ui.text))
                                    .font_family(settings_mono_font_family(self.settings))
                                    .truncate()
                                    .child(data_dir),
                            )
                            .when(data_dir_info.can_change, |row| {
                                row.child(self.settings_data_directory_change_button())
                            })
                            .when(data_dir_info.can_change && data_dir_info.is_custom, |row| {
                                row.child(self.settings_data_directory_reset_button())
                            })
                            .into_any_element(),
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.warning))
                            .child(
                                self.i18n
                                    .t("settings_view.general.data_directory_restart_notice"),
                            )
                            .into_any_element(),
                    ],
                )
            }
            SettingsSectionId::GeneralCliCompanion => {
                let cli = self.state.read(cx).cli_companion_snapshot();
                let cli_status = cli.status.as_ref();
                let cli_loading = cli.loading;
                let cli_installed = cli_status.is_some_and(|status| status.installed);
                let cli_bundled = cli_status.is_some_and(|status| status.bundled);
                let cli_needs_reinstall = cli_status.is_some_and(|status| status.needs_reinstall);
                let legacy_cli_installed = cli_status.is_some_and(|status| status.legacy_installed);
                let legacy_cli_path = cli_status
                    .and_then(|status| status.legacy_install_path.clone())
                    .unwrap_or_default();
                let cli_path = cli_status
                    .and_then(|status| status.install_path.clone())
                    .unwrap_or_else(|| cli_install_path().display().to_string());
                let (badge_label, badge_color) = if cli.error.is_some() {
                    (
                        self.i18n.t("settings_view.general.cli_status_error"),
                        self.tokens.ui.error,
                    )
                } else if cli_loading {
                    (
                        self.i18n.t("settings_view.general.cli_checking"),
                        self.tokens.ui.warning,
                    )
                } else if cli_installed && cli_needs_reinstall {
                    (
                        self.i18n.t("settings_view.general.cli_reinstall_required"),
                        self.tokens.ui.warning,
                    )
                } else if cli_installed {
                    (
                        self.i18n.t("settings_view.general.cli_installed"),
                        self.tokens.ui.success,
                    )
                } else {
                    (
                        self.i18n.t("settings_view.general.cli_not_installed"),
                        self.tokens.ui.text_muted,
                    )
                };
                let reinstall_hint = cli_status
                    .filter(|status| status.installed && status.needs_reinstall)
                    .map(|status| {
                        self.i18n
                            .t("settings_view.general.cli_reinstall_hint")
                            .replace("{{version}}", &status.app_version)
                    });
                let mut rows = vec![
                    self.card_subsection_header(
                        self.i18n.t("settings_view.general.cli_tool"),
                        Some(self.i18n.t("settings_view.general.cli_tool_hint")),
                    ),
                    div()
                        .w_full()
                        .flex()
                        .flex_row()
                        .items_end()
                        .justify_between()
                        .gap(px(16.0))
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .gap(px(10.0))
                                .min_w(px(0.0))
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap(px(10.0))
                                        .child(render_icon(
                                            "lucide/terminal.svg",
                                            16.0,
                                            rgb(self.tokens.ui.text_muted),
                                        ))
                                        .child(
                                            div()
                                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                                .font_family(settings_mono_font_family(
                                                    self.settings,
                                                ))
                                                .text_color(rgb(self.tokens.ui.text))
                                                .child(CLI_COMPANION_COMMAND_NAME),
                                        )
                                        .child(self.text_badge(badge_label, badge_color)),
                                )
                                .child(
                                    div()
                                        .w_full()
                                        .min_w(px(0.0))
                                        .text_size(px(self.tokens.metrics.ui_text_xs))
                                        .font_family(settings_mono_font_family(self.settings))
                                        .text_color(rgb(self.tokens.ui.text_muted))
                                        .truncate()
                                        .child(cli_path),
                                )
                                .when_some(reinstall_hint, |column, hint| {
                                    column.child(
                                        div()
                                            .text_size(px(self.tokens.metrics.ui_text_xs))
                                            .text_color(rgb(self.tokens.ui.warning))
                                            .child(hint),
                                    )
                                })
                                .when_some(cli.error.clone(), |column, error| {
                                    column.child(
                                        div()
                                            .text_size(px(self.tokens.metrics.ui_text_xs))
                                            .text_color(rgb(self.tokens.ui.error))
                                            .child(error),
                                    )
                                })
                                .when(
                                    !cli_loading && cli_status.is_some() && !cli_bundled,
                                    |column| {
                                        column.child(
                                            div()
                                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                                .text_color(rgb(self.tokens.ui.text_muted))
                                                .child(
                                                    self.i18n
                                                        .t("settings_view.general.cli_not_bundled"),
                                                ),
                                        )
                                    },
                                ),
                        )
                        .when(
                            cli_bundled && (!cli_installed || cli_needs_reinstall),
                            |row| {
                                row.child(self.cli_companion_action_button(
                                    if cli_needs_reinstall {
                                        self.i18n.t("settings_view.general.cli_reinstall")
                                    } else {
                                        self.i18n.t("settings_view.general.cli_install")
                                    },
                                    "lucide/download.svg",
                                    ButtonVariant::Outline,
                                    cli_loading,
                                    GeneralRequest::Cli(CliCompanionOperation::Install),
                                ))
                            },
                        )
                        .when(cli_installed, |row| {
                            row.child(self.cli_companion_action_button(
                                self.i18n.t("settings_view.general.cli_uninstall"),
                                "lucide/trash-2.svg",
                                ButtonVariant::Ghost,
                                cli_loading,
                                GeneralRequest::Cli(CliCompanionOperation::Uninstall),
                            ))
                        })
                        .when(!cli_loading && cli.error.is_some(), |row| {
                            row.child(self.cli_companion_action_button(
                                self.i18n.t("settings_view.help.retry"),
                                "lucide/refresh-cw.svg",
                                ButtonVariant::Ghost,
                                false,
                                GeneralRequest::Cli(CliCompanionOperation::Refresh),
                            ))
                        })
                        .into_any_element(),
                ];
                if legacy_cli_installed {
                    rows.push(self.card_separator());
                    rows.push(self.settings_callout(
                        SettingsCalloutTone::Warning,
                        Some("lucide/alert-triangle.svg"),
                        div()
                            .w_full()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .items_center()
                            .justify_between()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(220.0))
                                    .flex()
                                    .flex_col()
                                    .gap(px(4.0))
                                    .child(
                                        div()
                                            .text_size(px(self.tokens.metrics.ui_text_sm))
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(rgb(self.tokens.ui.text))
                                            .child(self.i18n.t("migration.cli_legacy_found")),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(self.tokens.metrics.ui_text_xs))
                                            .text_color(rgb(self.tokens.ui.text_muted))
                                            .child(self.i18n.t("migration.cli_legacy_settings_hint")),
                                    )
                                    .child(
                                        div()
                                            .min_w(px(0.0))
                                            .text_size(px(self.tokens.metrics.ui_text_xs))
                                            .font_family(settings_mono_font_family(
                                                self.settings,
                                            ))
                                            .text_color(rgb(self.tokens.ui.text_muted))
                                            .truncate()
                                            .child(format!(
                                                "{LEGACY_CLI_COMPANION_COMMAND_NAME} · {legacy_cli_path}"
                                            )),
                                    ),
                            )
                            .child(self.cli_companion_action_button(
                                self.i18n.t("migration.cli_uninstall_legacy"),
                                "lucide/trash-2.svg",
                                ButtonVariant::Ghost,
                                cli_loading,
                                GeneralRequest::Cli(CliCompanionOperation::UninstallLegacy),
                            ))
                            .into_any_element(),
                    ));
                }
                self.settings_card("settings_view.general.cli_companion", "", rows)
            }
            SettingsSectionId::GeneralStartup => self.launch_at_login_settings_card(cx),
            SettingsSectionId::GeneralWindowBehavior
                if cfg!(any(target_os = "windows", target_os = "macos")) =>
            {
                let (label_key, hint_key) = close_to_background_label_keys();
                self.settings_card(
                    "settings_view.general.window_behavior",
                    "settings_view.general.window_behavior_hint",
                    vec![self.bool_row(
                        label_key,
                        hint_key,
                        settings.general.minimize_to_tray_on_close,
                        |settings, enabled| settings.general.minimize_to_tray_on_close = enabled,
                        cx,
                    )],
                )
            }
            _ => div().into_any_element(),
        }
    }

    #[cfg(target_os = "macos")]
    pub fn launch_at_login_settings_card(&self, cx: &App) -> AnyElement {
        let launch_at_login = self.state.read(cx).launch_at_login_snapshot();
        // TODO(signing): Once every macOS artifact is Developer ID signed and
        // notarized, replace this system-settings handoff with an in-app
        // SMAppService register/unregister toggle and remove the manual copy.
        let mut description = div().min_w(px(0.0)).flex().flex_col().gap(px(4.0)).child(
            div()
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(
                    self.i18n
                        .t("settings_view.general.launch_at_login_macos_hint"),
                ),
        );
        if let Some(error) = launch_at_login.error {
            let error = match error {
                LaunchAtLoginError::OperationFailed(error) => error,
            };
            description = description.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.error))
                    .child(
                        self.i18n
                            .t("settings_view.general.launch_at_login_failed")
                            .replace("{{error}}", error.as_ref()),
                    ),
            );
        }
        let manage_button = self.workspace_toolbar_action_button(
            self.i18n.t("settings_view.general.manage_login_items"),
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                ..ToolbarButtonOptions::default()
            },
            self.action(GeneralRequest::OpenLoginItems),
        );

        self.settings_card(
            "settings_view.general.startup",
            "settings_view.general.startup_hint",
            vec![settings_row(
                &self.tokens,
                div()
                    .child(self.i18n.t("settings_view.general.launch_at_login"))
                    .into_any_element(),
                Some(description.into_any_element()),
                manage_button.into_any_element(),
            )],
        )
    }

    #[cfg(not(target_os = "macos"))]
    pub fn launch_at_login_settings_card(&self, cx: &App) -> AnyElement {
        let launch_at_login = self.state.read(cx).launch_at_login_snapshot();
        let loading = launch_at_login.pending;
        let enabled = launch_at_login.enabled;
        let error = launch_at_login.error;
        let control = div().flex_none().child(
            switch_with(
                &self.tokens,
                "launch-at-login",
                String::new(),
                enabled,
                SwitchOptions::default().disabled(loading),
            )
            .on_mouse_down(
                MouseButton::Left,
                self.action(GeneralRequest::SetLaunchAtLogin(!enabled)),
            ),
        );
        let mut label = div().min_w(px(0.0)).flex().flex_col().gap(px(4.0)).child(
            div()
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(self.i18n.t("settings_view.general.launch_at_login_hint")),
        );
        if loading {
            label = label.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(
                        self.i18n
                            .t("settings_view.general.launch_at_login_updating"),
                    ),
            );
        }
        if let Some(error) = error {
            let error = match error {
                LaunchAtLoginError::ApprovalRequired => self
                    .i18n
                    .t("settings_view.general.launch_at_login_approval_required"),
                LaunchAtLoginError::OperationFailed(error)
                | LaunchAtLoginError::TaskFailed(error) => error.to_string(),
            };
            label = label.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.error))
                    .child(
                        self.i18n
                            .t("settings_view.general.launch_at_login_failed")
                            .replace("{{error}}", error.as_ref()),
                    ),
            );
        }

        self.settings_card(
            "settings_view.general.startup",
            "settings_view.general.startup_hint",
            vec![settings_row(
                &self.tokens,
                div()
                    .child(self.i18n.t("settings_view.general.launch_at_login"))
                    .into_any_element(),
                Some(label.into_any_element()),
                control.into_any_element(),
            )],
        )
    }

    pub fn settings_data_directory_change_button(&self) -> AnyElement {
        self.workspace_toolbar_action_button(
            self.i18n.t("settings_view.general.change"),
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                ..ToolbarButtonOptions::default()
            },
            self.action(GeneralRequest::PickDataDirectory),
        )
        .into_any_element()
    }

    pub fn settings_data_directory_reset_button(&self) -> AnyElement {
        self.workspace_toolbar_action_button(
            self.i18n.t("settings_view.general.reset_to_default"),
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                ..ToolbarButtonOptions::default()
            },
            self.action(GeneralRequest::ResetDataDirectory),
        )
        .into_any_element()
    }

    pub fn terminal_command_specs_editor_row(&self) -> AnyElement {
        let path = self.command_specs_path.clone();
        let theme = self.tokens.ui;

        div()
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(self.tokens.metrics.modal_section_gap))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(self.tokens.spacing.one))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(self.i18n.t("settings_view.terminal.command_specs")),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("settings_view.terminal.command_specs_hint")),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(self.tokens.metrics.modal_field_gap))
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .font_family(settings_mono_font_family(self.settings))
                                    .truncate()
                                    .child(path),
                            )
                            .child(self.command_specs_summary.clone()),
                    ),
            )
            .child(self.workspace_toolbar_action_button(
                self.i18n.t("settings_view.terminal.command_specs_edit"),
                Some(render_icon(
                    "lucide/pencil.svg",
                    TERMINAL_COMMAND_SPECS_ACTION_ICON_SIZE,
                    rgb(theme.text),
                )),
                ToolbarButtonOptions {
                    button: ButtonOptions {
                        variant: ButtonVariant::Outline,
                        size: ButtonSize::Sm,
                        radius: ButtonRadius::Md,
                        disabled: false,
                    },
                    ..ToolbarButtonOptions::default()
                },
                self.action(GeneralRequest::OpenCommandSpecs),
            ))
            .into_any_element()
    }

    pub fn terminal_awareness_section(
        mut self,
        section: SettingsSectionId,
        cx: &App,
    ) -> AnyElement {
        let settings = self.settings;
        match section {
            SettingsSectionId::TerminalCommandBarControls => {
                let mut rows = vec![
                    self.bool_row(
                        "settings_view.terminal.command_bar",
                        "settings_view.terminal.command_bar_hint",
                        settings.terminal.command_bar.enabled,
                        set_command_bar_enabled,
                        cx,
                    ),
                    self.bool_row(
                        "settings_view.terminal.command_bar_git_status",
                        "settings_view.terminal.command_bar_git_status_hint",
                        settings.terminal.command_bar.git_status,
                        set_command_bar_git_status,
                        cx,
                    ),
                    self.bool_row(
                        "settings_view.terminal.command_bar_project_tasks",
                        "settings_view.terminal.command_bar_project_tasks_hint",
                        settings.terminal.command_bar.project_tasks,
                        set_command_bar_project_tasks,
                        cx,
                    ),
                    self.bool_row(
                        "settings_view.terminal.command_bar_ssh_latency",
                        "settings_view.terminal.command_bar_ssh_latency_hint",
                        settings.terminal.command_bar.ssh_latency,
                        set_command_bar_ssh_latency,
                        cx,
                    ),
                    self.select_setting_row(
                        "settings_view.terminal.ssh_latency_interval",
                        "settings_view.terminal.ssh_latency_interval_hint",
                        SettingsSelect::SshLatencyInterval,
                        settings
                            .terminal
                            .command_bar
                            .ssh_latency_interval_secs
                            .to_string(),
                        self.tokens.metrics.settings_select_narrow_width,
                        cx,
                    ),
                    self.bool_row(
                        "settings_view.terminal.command_bar_current_directory_awareness",
                        "settings_view.terminal.command_bar_current_directory_awareness_hint",
                        settings.terminal.command_bar.show_current_directory,
                        set_command_bar_show_current_directory,
                        cx,
                    ),
                    self.bool_row(
                        "settings_view.terminal.autosuggest_local_history",
                        "settings_view.terminal.autosuggest_local_history_hint",
                        settings.terminal.autosuggest.local_shell_history,
                        set_autosuggest_local_history,
                        cx,
                    ),
                    self.bool_row(
                        "terminal.history.save",
                        "terminal.history.save_hint",
                        settings.terminal.autosuggest.save_history,
                        set_save_command_history,
                        cx,
                    ),
                    self.number_row(
                        "terminal.history.retention",
                        "terminal.history.retention_hint",
                        SettingsInput::CommandHistoryRetention,
                        i64::from(settings.terminal.autosuggest.history_retention_days),
                        cx,
                    ),
                    self.workspace_toolbar_action_button(
                        self.i18n.t("terminal.history.clear_all"),
                        None,
                        ToolbarButtonOptions {
                            button: ButtonOptions {
                                variant: ButtonVariant::Outline,
                                size: ButtonSize::Sm,
                                radius: ButtonRadius::Md,
                                disabled: false,
                            },
                            ..Default::default()
                        },
                        self.action(GeneralRequest::ClearCommandHistory),
                    )
                    .into_any_element(),
                ];
                if self.history_failed {
                    rows.push(
                        self.settings_callout(
                            SettingsCalloutTone::Error,
                            Some("lucide/alert-circle.svg"),
                            div()
                                .child(self.i18n.t("terminal.history.error"))
                                .into_any_element(),
                        ),
                    );
                }
                self.settings_card(
                    "settings_view.terminal.command_bar",
                    "",
                    self.separated_settings_rows(rows),
                )
            }
            SettingsSectionId::TerminalCommandBarFocusHandoff => {
                let focus_handoff = self.focus_handoff.take();
                self.settings_card(
                    "settings_view.terminal.command_bar_focus_handoff",
                    "settings_view.terminal.command_bar_focus_handoff_hint",
                    focus_handoff.into_iter().collect(),
                )
            }
            SettingsSectionId::TerminalCommandBarQuickCommands => self.settings_card(
                "settings_view.terminal.quick_commands",
                "",
                vec![
                    self.bool_row(
                        "settings_view.terminal.quick_commands",
                        "settings_view.terminal.quick_commands_hint",
                        settings.terminal.command_bar.quick_commands_enabled,
                        set_quick_commands_enabled,
                        cx,
                    ),
                    self.card_separator(),
                    self.bool_row(
                        "settings_view.terminal.quick_bar",
                        "settings_view.terminal.quick_bar_hint",
                        settings.terminal.command_bar.quick_bar_enabled,
                        set_quick_bar_enabled,
                        cx,
                    ),
                    self.card_separator(),
                    self.bool_row(
                        "settings_view.terminal.quick_commands_confirm",
                        "settings_view.terminal.quick_commands_confirm_hint",
                        settings
                            .terminal
                            .command_bar
                            .quick_commands_confirm_before_run,
                        set_quick_commands_confirm,
                        cx,
                    ),
                    self.card_separator(),
                    self.bool_row(
                        "settings_view.terminal.quick_commands_toast",
                        "settings_view.terminal.quick_commands_toast_hint",
                        settings.terminal.command_bar.quick_commands_show_toast,
                        set_quick_commands_toast,
                        cx,
                    ),
                    self.card_separator(),
                    self.terminal_command_specs_editor_row(),
                ],
            ),
            SettingsSectionId::TerminalAwarenessControls => self.settings_card(
                "settings_view.terminal.awareness_title",
                "settings_view.terminal.awareness_description",
                vec![
                    self.bool_row(
                        "settings_view.terminal.awareness_enabled",
                        "settings_view.terminal.awareness_enabled_hint",
                        settings.terminal.command_bar.current_directory_awareness,
                        set_command_bar_current_directory_awareness,
                        cx,
                    ),
                    self.card_separator(),
                    self.select_setting_row(
                        "settings_view.connections.shell_integration.mode_label",
                        "settings_view.connections.shell_integration.mode_hint",
                        SettingsSelect::RemoteShellIntegrationMode,
                        remote_shell_integration_mode_label(
                            settings.terminal.remote_shell_integration_mode,
                            &self.i18n,
                        ),
                        self.tokens.metrics.settings_select_width,
                        cx,
                    ),
                ],
            ),
            _ => div().into_any_element(),
        }
    }
}

fn close_to_background_label_keys() -> (&'static str, &'static str) {
    if cfg!(target_os = "macos") {
        (
            "settings_view.general.keep_running_on_close",
            "settings_view.general.keep_running_on_close_hint",
        )
    } else {
        (
            "settings_view.general.minimize_to_tray_on_close",
            "settings_view.general.minimize_to_tray_on_close_hint",
        )
    }
}
