use super::highlight::{semantic_class_label, semantic_context_label};
use super::*;
use oxideterm_terminal_triggers::{
    LocalProcessSpec, TerminalTriggerAction, TerminalTriggerDispatch, TerminalTriggerMatchMode,
    TerminalTriggerScope,
};

pub(in crate::workspace) const SETTINGS_ROW_LABEL_MIN_WIDTH: f32 = 180.0; // Keep localized labels readable before controls wrap.

impl WorkspaceApp {
    pub(in crate::workspace) fn render_settings_select_overlay(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        // Select content follows the logical open state directly. Dropdowns
        // never retain a second render-only state for exit animation.
        let open_select = self.open_settings_select?;
        let anchor = self.select_anchors.get(&open_select.anchor_id()).copied()?;
        let width =
            f32::from(anchor.bounds.size.width).max(self.tokens.metrics.ui_select_min_width);
        let settings = self.settings_store.settings();
        let active_tab = self.settings_workspace.read(cx).route_snapshot().active_tab;

        let popup = match (active_tab, open_select) {
            (SettingsTab::General, SettingsSelect::Language) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for language in language_options() {
                    let label = self.language_label(language);
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, label, language == settings.general.language),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(|settings| settings.general.language = language, cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Help, SettingsSelect::UpdateChannel) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for channel in [UpdateChannel::Stable, UpdateChannel::Beta] {
                    let label = update_channel_label(channel, &self.i18n);
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            label,
                            channel == settings.general.update_channel,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                |settings| settings.general.update_channel = channel,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Network, SettingsSelect::UpdateProxyMode) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                let current = settings.general.update_proxy.mode;
                for mode in [
                    UpdateProxyMode::Direct,
                    UpdateProxyMode::Application,
                    UpdateProxyMode::System,
                    UpdateProxyMode::Custom,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            update_proxy_mode_label(mode, &self.i18n),
                            mode == current,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                |settings| settings.general.update_proxy.mode = mode,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Network, SettingsSelect::UpdateProxyProtocol) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                let current = settings.general.update_proxy.protocol;
                for protocol in [
                    UpdateProxyProtocol::Http,
                    UpdateProxyProtocol::Https,
                    UpdateProxyProtocol::Socks5,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            update_proxy_protocol_label(protocol, &self.i18n),
                            protocol == current,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                |settings| settings.general.update_proxy.protocol = protocol,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Network, SettingsSelect::NetworkApplicationProxyMode) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                let current = settings.network.application_proxy_mode;
                for mode in [
                    SettingsApplicationProxyMode::System,
                    SettingsApplicationProxyMode::Direct,
                    SettingsApplicationProxyMode::Shared,
                ] {
                    // A missing shared profile is shown but cannot become an
                    // active route, keeping the runtime policy fail-closed.
                    let shared_proxy_missing = mode == SettingsApplicationProxyMode::Shared
                        && settings.network.upstream_proxy.is_none();
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            network_application_proxy_mode_label(mode, &self.i18n),
                            mode == current,
                        ),
                        shared_proxy_missing,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            if shared_proxy_missing {
                                return;
                            }
                            this.close_settings_select();
                            this.edit_settings(
                                |settings| settings.network.application_proxy_mode = mode,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Appearance, SettingsSelect::AppearanceTheme) => {
                let mut popup = select_panel_overlay_popup_with_max_height(
                    &self.tokens,
                    width,
                    self.tokens.metrics.settings_theme_select_popup_max_height,
                );

                if !settings.custom_themes.is_empty() {
                    popup = popup.child(select_label(
                        &self.tokens,
                        self.i18n.t("settings_view.appearance.theme_group_custom"),
                    ));
                    let mut custom_theme_ids: Vec<_> =
                        settings.custom_themes.keys().cloned().collect();
                    custom_theme_ids.sort();
                    for theme_id in custom_theme_ids {
                        let label = custom_theme_display_name(settings, &theme_id);
                        let selected = theme_id == settings.terminal.theme;
                        popup = popup.child(select_option_action(
                            select_option(&self.tokens, label, selected),
                            false,
                            false,
                            cx.listener(move |this, _event, _window, cx| {
                                this.close_settings_select();
                                this.edit_settings(
                                    |settings| settings.terminal.theme = theme_id.clone(),
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ));
                    }
                    popup = popup.child(select_separator(&self.tokens));
                }

                popup = popup.child(select_label(
                    &self.tokens,
                    self.i18n.t("settings_view.appearance.theme_group_oxide"),
                ));
                for &theme_id in OXIDE_THEME_IDS {
                    if !built_in_theme_exists(theme_id) {
                        continue;
                    }
                    let next_theme = theme_id.to_string();
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            theme_display_name(theme_id),
                            theme_id == settings.terminal.theme.as_str(),
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                |settings| settings.terminal.theme = next_theme.clone(),
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }

                popup = popup
                    .child(select_separator(&self.tokens))
                    .child(select_label(
                        &self.tokens,
                        self.i18n.t("settings_view.appearance.theme_group_classic"),
                    ));
                let mut classic_themes: Vec<_> = BUILT_IN_THEMES
                    .iter()
                    .filter(|theme| !is_oxide_theme(theme.id))
                    .collect();
                classic_themes.sort_by_key(|theme| theme.id);
                for theme in classic_themes {
                    let theme_id = theme.id.to_string();
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            theme_display_name(theme.id),
                            theme.id == settings.terminal.theme.as_str(),
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                |settings| settings.terminal.theme = theme_id.clone(),
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Appearance, SettingsSelect::CustomThemeDuplicate) => {
                let mut popup = select_panel_overlay_popup_with_max_height(
                    &self.tokens,
                    width,
                    self.tokens.metrics.settings_theme_select_popup_max_height,
                );
                let mut themes: Vec<_> = BUILT_IN_THEMES.iter().collect();
                themes.sort_by_key(|theme| theme.id);
                for theme in themes {
                    let theme_id = theme.id.to_string();
                    let selected = self
                        .settings_workspace
                        .read(cx)
                        .theme_editor()
                        .is_some_and(|editor| editor.duplicate_theme == theme_id);
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, theme_display_name(theme.id), selected),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.settings_workspace.update(cx, |settings, cx| {
                                settings.duplicate_theme_editor_from(&theme_id, cx);
                            });
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Appearance, SettingsSelect::AppearanceDensity) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &density in density_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            density_label(density, &self.i18n),
                            density == settings.appearance.ui_density,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                |settings| settings.appearance.ui_density = density,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Appearance, SettingsSelect::AppearanceAnimation) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &speed in animation_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            animation_label(speed, &self.i18n),
                            speed == settings.appearance.animation_speed,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            // Apply the selected profile before starting the
                            // exit so choosing Off closes in this same event.
                            this.edit_settings(
                                |settings| settings.appearance.animation_speed = speed,
                                cx,
                            );
                            this.close_settings_select();
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Appearance, SettingsSelect::AppearanceRenderProfile) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &profile in render_profile_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            render_profile_label(profile, &self.i18n),
                            profile == settings.appearance.render_profile,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                |settings| settings.appearance.render_profile = profile,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Appearance, SettingsSelect::AppearanceFrostedGlass) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                // The selector lists platform window materials only. Legacy
                // WebView CSS/native values are normalized to the system entry.
                let selected_mode = match settings.appearance.frosted_glass {
                    FrostedGlassMode::Css | FrostedGlassMode::Native => FrostedGlassMode::System,
                    mode => mode,
                };
                for mode in available_modes()
                    .iter()
                    .copied()
                    .map(frosted_glass_mode_from_native)
                {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            frosted_glass_label(mode, &self.i18n),
                            selected_mode == mode,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                |settings| settings.appearance.frosted_glass = mode,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Appearance, SettingsSelect::AppearanceBackgroundFit) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &fit in background_fit_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            background_fit_label(fit, &self.i18n),
                            fit == settings.terminal.background_fit,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                |settings| settings.terminal.background_fit = fit,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalFontFamily) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &family in font_family_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            font_family_label(family),
                            family == settings.terminal.font_family,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                |settings| settings.terminal.font_family = family,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalCjkFontFamily) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                let current_family = settings.terminal.cjk_font_family.trim();
                for &family in terminal_cjk_font_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            terminal_cjk_font_label(family, &self.i18n),
                            family == current_family,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                |settings| settings.terminal.cjk_font_family = family.to_string(),
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalEncoding) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &encoding in terminal_encoding_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            terminal_encoding_label(encoding),
                            encoding == settings.terminal.terminal_encoding,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                |settings| settings.terminal.terminal_encoding = encoding,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalSessionLogFileMode) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &mode in terminal_session_log_file_mode_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            terminal_session_log_file_mode_label(mode, &self.i18n),
                            mode == settings.terminal.session_log.file_mode,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                |settings| settings.terminal.session_log.file_mode = mode,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalBackspaceSequence) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &sequence in terminal_backspace_sequence_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            terminal_backspace_sequence_label(sequence),
                            sequence == settings.terminal.backspace_sequence,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                |settings| settings.terminal.backspace_sequence = sequence,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalDeleteSequence) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &sequence in terminal_delete_sequence_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            terminal_delete_sequence_label(sequence),
                            sequence == settings.terminal.delete_sequence,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                |settings| settings.terminal.delete_sequence = sequence,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalCursorStyle) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &style in cursor_style_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            cursor_style_label(style, &self.i18n),
                            style == settings.terminal.cursor_style,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                |settings| settings.terminal.cursor_style = style,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Ide, SettingsSelect::IdeAgentMode) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for mode in [
                    IdeAgentMode::Ask,
                    IdeAgentMode::Enabled,
                    IdeAgentMode::Disabled,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            ide_agent_label(mode, &self.i18n),
                            mode == settings.ide.agent_mode,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(|settings| settings.ide.agent_mode = mode, cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::RemoteShellIntegrationMode) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for mode in [
                    RemoteShellIntegrationMode::Ask,
                    RemoteShellIntegrationMode::Enabled,
                    RemoteShellIntegrationMode::Disabled,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            remote_shell_integration_mode_label(mode, &self.i18n),
                            mode == settings.terminal.remote_shell_integration_mode,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                |settings| settings.terminal.remote_shell_integration_mode = mode,
                                cx,
                            );
                            this.remote_shell_integration_mode_changed(mode, cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalTriggerMatchMode) => {
                let draft = self.terminal_trigger_draft()?;
                let current = draft.matcher.mode;
                let mut popup = select_overlay_popup(&self.tokens, width);
                for mode in [
                    TerminalTriggerMatchMode::Literal,
                    TerminalTriggerMatchMode::Regex,
                ] {
                    let label = self.i18n.t(match mode {
                        TerminalTriggerMatchMode::Literal => {
                            "settings_view.terminal.triggers.literal"
                        }
                        TerminalTriggerMatchMode::Regex => "settings_view.terminal.triggers.regex",
                    });
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, label, mode == current),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.select_terminal_trigger_match_mode(mode);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalTriggerAction) => {
                let draft = self.terminal_trigger_draft()?;
                let current = match &draft.action {
                    TerminalTriggerAction::SendText { .. } => 0,
                    TerminalTriggerAction::RunQuickCommand { .. } => 1,
                    TerminalTriggerAction::LaunchLocalProcess { .. } => 2,
                };
                let mut popup = select_overlay_popup(&self.tokens, width);
                for action_index in 0..3 {
                    let label_key = match action_index {
                        0 => "settings_view.terminal.triggers.action_send_text",
                        1 => "settings_view.terminal.triggers.action_quick_command",
                        _ => "settings_view.terminal.triggers.action_local_process",
                    };
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            self.i18n.t(label_key),
                            action_index == current,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            let action = match action_index {
                                0 => TerminalTriggerAction::SendText {
                                    text: String::new(),
                                    append_enter: false,
                                },
                                1 => {
                                    let quick_command_id = this
                                        .terminal
                                        .read(cx)
                                        .quick_commands
                                        .store
                                        .commands
                                        .first()
                                        .map(|command| command.id.clone())
                                        .unwrap_or_default();
                                    TerminalTriggerAction::RunQuickCommand { quick_command_id }
                                }
                                _ => TerminalTriggerAction::LaunchLocalProcess {
                                    process: LocalProcessSpec::DirectProgram {
                                        executable: String::new(),
                                        arguments: Vec::new(),
                                        working_directory: None,
                                    },
                                },
                            };
                            this.select_terminal_trigger_action(action);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalTriggerProcessMode) => {
                let draft = self.terminal_trigger_draft()?;
                let current_shell = matches!(
                    &draft.action,
                    TerminalTriggerAction::LaunchLocalProcess {
                        process: LocalProcessSpec::ExplicitShell { .. }
                    }
                );
                let mut popup = select_overlay_popup(&self.tokens, width);
                for shell in [false, true] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            self.i18n.t(if shell {
                                "settings_view.terminal.triggers.explicit_shell"
                            } else {
                                "settings_view.terminal.triggers.direct_program"
                            }),
                            shell == current_shell,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.select_terminal_trigger_process_mode(shell);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalTriggerQuickCommand) => {
                let draft = self.terminal_trigger_draft()?;
                let current = match &draft.action {
                    TerminalTriggerAction::RunQuickCommand { quick_command_id } => {
                        quick_command_id.clone()
                    }
                    _ => String::new(),
                };
                let mut popup = select_panel_overlay_popup_with_max_height(
                    &self.tokens,
                    width,
                    self.tokens.metrics.settings_theme_select_popup_max_height,
                );
                for command in self
                    .terminal
                    .read(cx)
                    .quick_commands
                    .store
                    .commands
                    .iter()
                    .filter(|command| {
                        oxideterm_quick_commands::quick_command_can_run_non_interactively(command)
                    })
                {
                    let id = command.id.clone();
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, command.name.clone(), id == current),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.select_terminal_trigger_quick_command(id.clone());
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalTriggerTiming) => {
                let draft = self.terminal_trigger_draft()?;
                let current = draft.timing.dispatch;
                let mut popup = select_overlay_popup(&self.tokens, width);
                for dispatch in [
                    TerminalTriggerDispatch::Immediate,
                    TerminalTriggerDispatch::AfterNextLineBreak,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            self.i18n.t(match dispatch {
                                TerminalTriggerDispatch::Immediate => {
                                    "settings_view.terminal.triggers.immediate"
                                }
                                TerminalTriggerDispatch::AfterNextLineBreak => {
                                    "settings_view.terminal.triggers.after_next_line_break"
                                }
                            }),
                            dispatch == current,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.select_terminal_trigger_timing(dispatch);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalTriggerScope) => {
                let draft = self.terminal_trigger_draft()?;
                let current = match &draft.scope {
                    TerminalTriggerScope::AllTerminals => 0,
                    TerminalTriggerScope::LocalTerminals => 1,
                    TerminalTriggerScope::SavedConnections { .. } => 2,
                };
                let saved_connections = match &draft.scope {
                    TerminalTriggerScope::SavedConnections { connections } => connections.clone(),
                    _ => Vec::new(),
                };
                let mut popup = select_overlay_popup(&self.tokens, width);
                for scope_index in 0..3 {
                    let label_key = match scope_index {
                        0 => "settings_view.terminal.triggers.all_terminals",
                        1 => "settings_view.terminal.triggers.local_terminals",
                        _ => "settings_view.terminal.triggers.saved_connections",
                    };
                    let existing_connections = saved_connections.clone();
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, self.i18n.t(label_key), scope_index == current),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            let scope = match scope_index {
                                0 => TerminalTriggerScope::AllTerminals,
                                1 => TerminalTriggerScope::LocalTerminals,
                                _ => TerminalTriggerScope::SavedConnections {
                                    connections: existing_connections.clone(),
                                },
                            };
                            this.select_terminal_trigger_scope(scope);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalSemanticScheme) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &scheme in terminal_semantic_scheme_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            terminal_semantic_scheme_label(scheme, &self.i18n),
                            scheme == settings.terminal.semantic_scheme,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                |settings| {
                                    settings.terminal.semantic_scheme = scheme;
                                    settings.terminal.semantic_custom_scheme = None;
                                },
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                if !settings.terminal.custom_semantic_schemes.is_empty() {
                    popup = popup
                        .child(select_separator(&self.tokens))
                        .child(select_label(
                            &self.tokens,
                            self.i18n
                                .t("settings_view.terminal.highlight_rules.semantic_scheme_custom"),
                        ));
                    for custom in &settings.terminal.custom_semantic_schemes {
                        let id = custom.id.clone();
                        let selected = settings.terminal.semantic_custom_scheme.as_deref()
                            == Some(id.as_str());
                        popup = popup.child(select_option_action(
                            select_option(&self.tokens, custom.name.clone(), selected),
                            false,
                            false,
                            cx.listener(move |this, _event, _window, cx| {
                                this.close_settings_select();
                                this.edit_settings(
                                    |settings| {
                                        settings.terminal.semantic_custom_scheme = Some(id.clone());
                                    },
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ));
                    }
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::HighlightRuleSet) => {
                let mut popup =
                    select_overlay_popup(&self.tokens, width).child(select_option_action(
                        select_option(
                            &self.tokens,
                            self.i18n
                                .t("settings_view.terminal.highlight_rules.rule_set_global_base"),
                            settings.terminal.default_highlight_rule_set.is_none(),
                        ),
                        false,
                        false,
                        cx.listener(|this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                |settings| settings.terminal.default_highlight_rule_set = None,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                if !settings.terminal.highlight_rule_sets.is_empty() {
                    popup = popup.child(select_separator(&self.tokens));
                    for rule_set in &settings.terminal.highlight_rule_sets {
                        let id = rule_set.id.clone();
                        let selected = settings.terminal.default_highlight_rule_set.as_deref()
                            == Some(id.as_str());
                        popup = popup.child(select_option_action(
                            select_option(&self.tokens, rule_set.name.clone(), selected),
                            false,
                            false,
                            cx.listener(move |this, _event, _window, cx| {
                                this.close_settings_select();
                                this.edit_settings(
                                    |settings| {
                                        settings.terminal.default_highlight_rule_set =
                                            Some(id.clone());
                                    },
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ));
                    }
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::SemanticSchemeRuleClass(index)) => {
                let selected = settings
                    .terminal
                    .active_custom_semantic_scheme()
                    .and_then(|scheme| scheme.rules.get(index))
                    .map(|rule| rule.class)?;
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &class in SEMANTIC_CLASSES {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            semantic_class_label(class, &self.i18n),
                            class == selected,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                |settings| {
                                    let _ = edit_custom_semantic_scheme(settings, |scheme| {
                                        if let Some(rule) = scheme.rules.get_mut(index) {
                                            rule.class = class;
                                        }
                                    });
                                },
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::SemanticSchemeRuleContext(index)) => {
                let selected = settings
                    .terminal
                    .active_custom_semantic_scheme()
                    .and_then(|scheme| scheme.rules.get(index))
                    .map(|rule| rule.context)?;
                let mut popup = select_overlay_popup(&self.tokens, width);
                for context in [
                    SemanticRuleContext::Any,
                    SemanticRuleContext::Command,
                    SemanticRuleContext::Output,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            semantic_context_label(context, &self.i18n),
                            context == selected,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                |settings| {
                                    let _ = edit_custom_semantic_scheme(settings, |scheme| {
                                        if let Some(rule) = scheme.rules.get_mut(index) {
                                            rule.context = context;
                                        }
                                    });
                                },
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::HighlightPreset) => {
                let mut popup = select_overlay_popup(&self.tokens, width.max(288.0));
                for (group_index, group) in
                    highlight_preset_groups(&self.i18n).into_iter().enumerate()
                {
                    if group_index > 0 {
                        popup = popup.child(select_separator(&self.tokens));
                    }
                    popup = popup.child(select_label(&self.tokens, group.label));
                    for preset in group.items {
                        popup = popup.child(select_option_action(
                            select_option(&self.tokens, preset.label.clone(), false),
                            false,
                            false,
                            cx.listener(move |this, _event, _window, cx| {
                                this.close_settings_select();
                                this.add_highlight_preset(preset.rules.clone(), cx);
                                cx.stop_propagation();
                            }),
                        ));
                    }
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::HighlightRenderMode(index)) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                let selected = settings
                    .terminal
                    .highlight_rules
                    .get(index)
                    .map(|rule| rule.render_mode)
                    .unwrap_or_default();
                for &mode in highlight_render_mode_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            highlight_render_mode_label(mode, &self.i18n),
                            mode == selected,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_highlight_rule(index, |rule| rule.render_mode = mode, cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::HighlightMatchScope(index)) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                let selected = settings
                    .terminal
                    .highlight_rules
                    .get(index)
                    .map(|rule| rule.match_scope)
                    .unwrap_or_default();
                for &scope in highlight_match_scope_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            highlight_match_scope_label(scope, &self.i18n),
                            scope == selected,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_highlight_rule(index, |rule| rule.match_scope = scope, cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::LocalShell) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                let selected = settings.local_terminal.default_shell_id.as_deref();
                for shell in self.effective_local_shells_for_settings(settings) {
                    let shell_id = shell.id.clone();
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            shell.label,
                            selected == Some(shell_id.as_str()),
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                |settings| {
                                    settings.local_terminal.default_shell_id =
                                        Some(shell_id.clone())
                                },
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::LocalShellSemanticScheme(index)) => {
                let shell = self
                    .effective_local_shells_for_settings(settings)
                    .into_iter()
                    .nth(index)?;
                let selected = settings.local_terminal.semantic_scheme_for_shell(&shell.id);
                let shell_id = shell.id.clone();
                let inherited_label = self
                    .i18n
                    .t("ssh.form.terminal_use_application_default")
                    .replace(
                        "{{value}}",
                        &application_semantic_scheme_label(settings, &self.i18n),
                    );
                let mut popup =
                    select_overlay_popup(&self.tokens, width).child(select_option_action(
                        select_option(&self.tokens, inherited_label, selected.is_none()),
                        false,
                        false,
                        cx.listener({
                            let shell_id = shell_id.clone();
                            move |this, _event, _window, cx| {
                                this.close_settings_select();
                                this.edit_settings(
                                    |settings| {
                                        settings
                                            .local_terminal
                                            .semantic_scheme_by_shell
                                            .remove(&shell_id);
                                    },
                                    cx,
                                );
                                cx.stop_propagation();
                            }
                        }),
                    ));
                for (scheme_id, scheme) in [
                    ("balanced", TerminalSemanticScheme::Balanced),
                    ("conservative", TerminalSemanticScheme::Conservative),
                ] {
                    let scheme_id = scheme_id.to_string();
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            terminal_semantic_scheme_label(scheme, &self.i18n),
                            selected == Some(scheme_id.as_str()),
                        ),
                        false,
                        false,
                        cx.listener({
                            let shell_id = shell_id.clone();
                            move |this, _event, _window, cx| {
                                this.close_settings_select();
                                this.edit_settings(
                                    |settings| {
                                        settings
                                            .local_terminal
                                            .semantic_scheme_by_shell
                                            .insert(shell_id.clone(), scheme_id.clone());
                                    },
                                    cx,
                                );
                                cx.stop_propagation();
                            }
                        }),
                    ));
                }
                if !settings.terminal.custom_semantic_schemes.is_empty() {
                    popup = popup
                        .child(select_separator(&self.tokens))
                        .child(select_label(
                            &self.tokens,
                            self.i18n
                                .t("settings_view.terminal.highlight_rules.semantic_scheme_custom"),
                        ));
                    for custom in &settings.terminal.custom_semantic_schemes {
                        let scheme_id = custom.id.clone();
                        popup = popup.child(select_option_action(
                            select_option(
                                &self.tokens,
                                custom.name.clone(),
                                selected == Some(scheme_id.as_str()),
                            ),
                            false,
                            false,
                            cx.listener({
                                let shell_id = shell_id.clone();
                                move |this, _event, _window, cx| {
                                    this.close_settings_select();
                                    this.edit_settings(
                                        |settings| {
                                            settings
                                                .local_terminal
                                                .semantic_scheme_by_shell
                                                .insert(shell_id.clone(), scheme_id.clone());
                                        },
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }
                            }),
                        ));
                    }
                }
                Some(popup)
            }
            (SettingsTab::Privilege, SettingsSelect::LocalPrivilegeKind) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                let current_kind = self.settings_workspace.read(cx).privilege_kind();
                for kind in [
                    PrivilegeCredentialKind::SudoPassword,
                    PrivilegeCredentialKind::SuPassword,
                    PrivilegeCredentialKind::CustomPrompt,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            self.settings_privilege_kind_label(kind),
                            current_kind == kind,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.settings_workspace.update(cx, |settings, cx| {
                                settings.set_privilege_kind(kind, cx);
                            });
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Connections, SettingsSelect::ConnectionIdleTimeout) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for (seconds, label) in connection_idle_timeout_options(&self.i18n) {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            label,
                            seconds == settings.connection_pool.idle_timeout_secs,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                |settings| settings.connection_pool.idle_timeout_secs = seconds,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Connections, SettingsSelect::ConnectionImportSource) => {
                let selected_source = self.settings_workspace.read(cx).connection_import_source();
                let mut popup = select_overlay_popup(&self.tokens, width);
                for source in connection_import_source_options().iter().copied() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            connection_import_source_label(source, &self.i18n),
                            source == selected_source,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.set_connection_import_source(source, cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Connections, SettingsSelect::ConnectionImportDuplicateStrategy) => {
                let selected_strategy = self
                    .settings_workspace
                    .read(cx)
                    .connection_import_duplicate_strategy();
                let mut popup = select_overlay_popup(&self.tokens, width);
                for strategy in [
                    ConnectionImportDuplicateStrategy::Skip,
                    ConnectionImportDuplicateStrategy::Rename,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            connection_import_duplicate_strategy_label(strategy, &self.i18n),
                            strategy == selected_strategy,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.settings_workspace.update(cx, |settings, cx| {
                                settings.set_connection_import_duplicate_strategy(strategy, cx);
                            });
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Connections, SettingsSelect::ReconnectMaxAttempts) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for attempts in reconnect_max_attempt_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            attempts.to_string(),
                            attempts == settings.reconnect.max_attempts,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                |settings| set_reconnect_max_attempts(settings, attempts),
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Connections, SettingsSelect::ReconnectBaseDelay) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for (delay_ms, label) in reconnect_base_delay_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            label,
                            delay_ms == settings.reconnect.base_delay_ms,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                |settings| set_reconnect_base_delay(settings, delay_ms),
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Connections, SettingsSelect::ReconnectMaxDelay) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for (delay_ms, label) in reconnect_max_delay_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            label,
                            delay_ms == settings.reconnect.max_delay_ms,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                |settings| set_reconnect_max_delay(settings, delay_ms),
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Network, SettingsSelect::NetworkProxyProtocol) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                let current = settings
                    .network
                    .upstream_proxy
                    .as_ref()
                    .map(|proxy| proxy.protocol)
                    .unwrap_or(SettingsUpstreamProxyProtocol::Socks5);
                for protocol in [
                    SettingsUpstreamProxyProtocol::Socks5,
                    SettingsUpstreamProxyProtocol::HttpConnect,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            network_proxy_protocol_label(protocol, &self.i18n),
                            protocol == current,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                move |settings| {
                                    if let Some(proxy) = settings.network.upstream_proxy.as_mut() {
                                        proxy.protocol = protocol;
                                    }
                                },
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Network, SettingsSelect::NetworkProxyAuth) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                let current = settings
                    .network
                    .upstream_proxy
                    .as_ref()
                    .map(|proxy| match &proxy.auth {
                        SettingsUpstreamProxyAuth::None => NetworkProxyAuthMode::None,
                        SettingsUpstreamProxyAuth::Password { .. } => {
                            NetworkProxyAuthMode::Password
                        }
                    })
                    .unwrap_or(NetworkProxyAuthMode::None);
                let has_saved_password =
                    settings
                        .network
                        .upstream_proxy
                        .as_ref()
                        .is_some_and(|proxy| {
                            matches!(
                                &proxy.auth,
                                SettingsUpstreamProxyAuth::Password {
                                    keychain_id: Some(_),
                                    ..
                                }
                            )
                        });
                for mode in [NetworkProxyAuthMode::None, NetworkProxyAuthMode::Password] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            network_proxy_auth_label(mode, &self.i18n),
                            mode == current,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.settings_workspace.update(cx, |settings, cx| {
                                settings.finish_network_proxy_password_action(None, cx);
                            });
                            if mode == NetworkProxyAuthMode::None
                                && has_saved_password
                                && let Err(error) = this
                                    .connection_store
                                    .delete_global_upstream_proxy_password()
                            {
                                this.settings_workspace.update(cx, |settings, cx| {
                                    settings.set_network_proxy_password_status(
                                        Some(error.to_string()),
                                        cx,
                                    );
                                });
                                cx.stop_propagation();
                                return;
                            }
                            this.edit_settings(
                                move |settings| {
                                    if let Some(proxy) = settings.network.upstream_proxy.as_mut() {
                                        proxy.auth = match mode {
                                            NetworkProxyAuthMode::None => {
                                                SettingsUpstreamProxyAuth::None
                                            }
                                            NetworkProxyAuthMode::Password => {
                                                SettingsUpstreamProxyAuth::Password {
                                                    username: String::new(),
                                                    keychain_id: None,
                                                }
                                            }
                                        };
                                    }
                                },
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Ai, SettingsSelect::AiProviderTemplate) => {
                let mut popup = select_overlay_popup(&self.tokens, width.max(AI_PROVIDER_SELECT_W));
                for template in AI_PROVIDER_TEMPLATES {
                    let provider_type = template.provider_type;
                    let is_selected =
                        self.ai_entity.read(cx).settings_new_provider_type() == provider_type;
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, self.i18n.t(template.label_key), is_selected),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.ai_entity.update(cx, |ai, cx| {
                                ai.select_settings_provider_type(provider_type, cx);
                            });
                            cx.stop_propagation();
                            // WorkspaceApp owns the surrounding settings render.
                            cx.notify();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Ai, SettingsSelect::AiContextMaxChars) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for value in AI_CONTEXT_MAX_CHAR_OPTIONS {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            self.ai_context_max_chars_label(value),
                            settings.ai.context_max_chars == value,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                move |settings| set_ai_context_max_chars(settings, value),
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Ai, SettingsSelect::AiContextVisibleLines) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for value in AI_CONTEXT_VISIBLE_LINE_OPTIONS {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            self.ai_context_visible_lines_label(value),
                            settings.ai.context_visible_lines == value,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                move |settings| set_ai_context_lines(settings, value),
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Knowledge, SettingsSelect::AiEmbeddingProvider) => {
                let current = settings
                    .ai
                    .embedding_config
                    .as_ref()
                    .and_then(|config| config.get("providerId"))
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                let mut popup = select_panel_overlay_popup_with_max_height(
                    &self.tokens,
                    width.max(AI_PROVIDER_SELECT_W),
                    320.0,
                );
                popup = popup.child(select_option_action(
                    select_option(
                        &self.tokens,
                        self.i18n
                            .t("settings_view.knowledge.auto_embedding_provider"),
                        current.is_none(),
                    ),
                    false,
                    false,
                    cx.listener(move |this, _event, _window, cx| {
                        this.close_settings_select();
                        this.edit_settings(
                            |settings| {
                                let model = settings
                                    .ai
                                    .embedding_config
                                    .as_ref()
                                    .and_then(|config| config.get("model"))
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or_default()
                                    .to_string();
                                settings.ai.embedding_config = Some(serde_json::json!({
                                    "providerId": null,
                                    "model": model
                                }));
                            },
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                ));
                for provider in ai_provider_views(settings) {
                    let provider_id = provider.id.clone();
                    let selected = current.as_deref() == Some(provider.id.as_str());
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, provider.name, selected),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            let provider_id = provider_id.clone();
                            this.close_settings_select();
                            this.edit_settings(
                                move |settings| {
                                    let model = settings
                                        .ai
                                        .embedding_config
                                        .as_ref()
                                        .and_then(|config| config.get("model"))
                                        .and_then(serde_json::Value::as_str)
                                        .unwrap_or_default()
                                        .to_string();
                                    settings.ai.embedding_config = Some(serde_json::json!({
                                        "providerId": provider_id,
                                        "model": model
                                    }));
                                },
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Knowledge, SettingsSelect::KnowledgeCollectionScope) => {
                let popup = select_overlay_popup(&self.tokens, width.max(220.0)).child(
                    select_option_action(
                        select_option(
                            &self.tokens,
                            self.i18n.t("settings_view.knowledge.scope_global"),
                            true,
                        ),
                        false,
                        false,
                        cx.listener(|this, _event, _window, cx| {
                            this.close_settings_select();
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
                );
                Some(popup)
            }
            (SettingsTab::Knowledge, SettingsSelect::KnowledgeDocumentFormat) => {
                let mut popup = select_overlay_popup(&self.tokens, width.max(220.0));
                for (format, label) in [("markdown", "Markdown"), ("plaintext", "Plain Text")] {
                    let selected =
                        self.ai_entity.read(cx).knowledge_new_document_format() == format;
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, label, selected),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.ai_entity.update(cx, |entity, cx| {
                                entity.set_knowledge_document_format(format.to_string());
                                cx.notify();
                            });
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Ai, SettingsSelect::AiMcpTransport) => {
                let current = self
                    .ai_entity
                    .read(cx)
                    .mcp_transport()
                    .unwrap_or(oxideterm_ai::McpTransport::Stdio);
                let mut popup = select_overlay_popup(&self.tokens, width.max(220.0));
                for (transport, label) in [
                    (oxideterm_ai::McpTransport::Stdio, "stdio"),
                    (
                        oxideterm_ai::McpTransport::StreamableHttp,
                        "Streamable HTTP (auto fallback)",
                    ),
                    (oxideterm_ai::McpTransport::LegacySse, "Legacy SSE"),
                ] {
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, label, transport == current),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.ai_entity.update(cx, |ai, cx| {
                                ai.set_mcp_transport(transport, cx);
                            });
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Ai, SettingsSelect::AiMcpAuthMode) => {
                let current = self
                    .ai_entity
                    .read(cx)
                    .mcp_auth_mode()
                    .unwrap_or(oxideterm_ai::McpAuthHeaderMode::Bearer);
                let mut popup = select_overlay_popup(&self.tokens, width.max(220.0));
                for (mode, label) in [
                    (
                        oxideterm_ai::McpAuthHeaderMode::Bearer,
                        self.i18n.t("settings_view.mcp.auth_header_mode_bearer"),
                    ),
                    (
                        oxideterm_ai::McpAuthHeaderMode::Raw,
                        self.i18n.t("settings_view.mcp.auth_header_mode_raw"),
                    ),
                    (
                        oxideterm_ai::McpAuthHeaderMode::None,
                        self.i18n.t("settings_view.mcp.auth_header_mode_none"),
                    ),
                ] {
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, label, mode == current),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.ai_entity.update(cx, |ai, cx| {
                                ai.set_mcp_auth_mode(mode, cx);
                            });
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Sftp, SettingsSelect::SftpPresentation) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for preference in [
                    oxideterm_settings::SftpPresentationPreference::Ask,
                    oxideterm_settings::SftpPresentationPreference::Tab,
                    oxideterm_settings::SftpPresentationPreference::Sidebar,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            sftp_page::sftp_presentation_label(preference, &self.i18n),
                            preference == settings.sftp.presentation,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                |settings| settings.sftp.presentation = preference,
                                cx,
                            );
                            if preference != oxideterm_settings::SftpPresentationPreference::Sidebar
                                && let Some(node_id) = this.embedded_sftp_node_id.clone()
                            {
                                this.close_embedded_sftp_for_node(&node_id, cx);
                            }
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Sftp, SettingsSelect::SftpProtocol) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for protocol in [
                    oxideterm_settings::FileTransferProtocolPreference::Auto,
                    oxideterm_settings::FileTransferProtocolPreference::Sftp,
                    oxideterm_settings::FileTransferProtocolPreference::Scp,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            file_transfer_protocol_label(protocol, &self.i18n),
                            protocol == settings.sftp.transfer_protocol,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                |settings| settings.sftp.transfer_protocol = protocol,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Sftp, SettingsSelect::SftpConcurrent) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &count in sftp_concurrent_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            sftp_transfer_count_label(&self.i18n, count),
                            count == settings.sftp.max_concurrent_transfers,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                |settings| settings.sftp.max_concurrent_transfers = count,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Sftp, SettingsSelect::SftpDirectoryParallelism) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &count in sftp_directory_parallelism_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            sftp_transfer_count_label(&self.i18n, count),
                            count == settings.sftp.directory_parallelism,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                |settings| settings.sftp.directory_parallelism = count,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Sftp, SettingsSelect::SftpConflict) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for action in [
                    oxideterm_settings::ConflictAction::Ask,
                    oxideterm_settings::ConflictAction::Overwrite,
                    oxideterm_settings::ConflictAction::Skip,
                    oxideterm_settings::ConflictAction::Rename,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            conflict_label(action, &self.i18n),
                            action == settings.sftp.conflict_action,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                |settings| settings.sftp.conflict_action = action,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            _ => None,
        }?;
        let popup = overlay_content_boundary(popup).into_any_element();

        Some(
            popover_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, window, cx| {
                        this.dismiss_transient_workspace_overlays_from_outside_pointer(window, cx);
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, _event, window, cx| {
                        this.dismiss_transient_workspace_overlays_from_outside_pointer(window, cx);
                        cx.stop_propagation();
                    }),
                )
                .child(
                    deferred(
                        anchored()
                            .anchor(Corner::TopLeft)
                            .position(anchor.bounds.bottom_left())
                            .offset(point(
                                px(0.0),
                                px(self.tokens.metrics.settings_select_popup_gap),
                            ))
                            .position_mode(AnchoredPositionMode::Window)
                            .child(popup),
                    )
                    .with_priority(oxideterm_gpui_ui::modal::TAURI_POPOVER_LAYER_PRIORITY),
                )
                .into_any_element(),
        )
    }

    pub(in crate::workspace) fn open_settings_select_from_pointer(
        &mut self,
        select_id: SettingsSelect,
        _cx: &mut Context<Self>,
    ) {
        // Browser select triggers opened by pointer do not show a focus-visible
        // ring. Keep the origin and open/toggle rule in one place so settings,
        // AI provider, and knowledge selects do not drift apart.
        self.focused_settings_input = None;
        if self.open_settings_select == Some(select_id) {
            self.close_settings_select();
            return;
        }
        self.open_settings_select = Some(select_id);
        self.settings_select_focus_origin = Some(browser_behavior::BrowserFocusOrigin::Pointer);
    }

    pub(in crate::workspace) fn language_select_row(
        &self,
        selected: Language,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let control_width = self.tokens.metrics.settings_select_width;
        let control = self.settings_select_control(
            SettingsSelect::Language,
            self.language_label(selected),
            false,
            Some(control_width),
            cx,
        );

        self.setting_row(
            "settings_view.general.language",
            "settings_view.general.language_hint",
            control,
            cx,
        )
    }

    pub(in crate::workspace) fn select_setting_row(
        &self,
        label_key: &str,
        hint_key: &str,
        select_id: SettingsSelect,
        value: String,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let control = self.settings_select_control(select_id, value, false, Some(width), cx);

        self.setting_row(label_key, hint_key, control, cx)
    }

    pub(in crate::workspace) fn bool_row(
        &self,
        label_key: &str,
        hint_key: &str,
        checked: bool,
        setter: fn(&mut PersistedSettings, bool),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.setting_row(
            label_key,
            hint_key,
            checkbox(&self.tokens, String::new(), checked)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.edit_settings(|settings| setter(settings, !checked), cx);
                    }),
                )
                .into_any_element(),
            cx,
        )
    }

    pub(in crate::workspace) fn number_row(
        &self,
        label_key: &str,
        hint_key: &str,
        input: SettingsInput,
        value: i64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Numeric rows use the same focus, IME, caret, and draft pipeline as
        // every other settings text field instead of simulating a click stepper.
        let control = self.number_input(input, value.to_string(), 112.0, cx);
        self.setting_row(label_key, hint_key, control, cx)
    }

    pub(in crate::workspace) fn setting_row(
        &self,
        label_key: &str,
        hint_key: &str,
        control: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = self.i18n.t(label_key);
        let hint = self.i18n.t(hint_key);
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(px(self.tokens.metrics.settings_row_gap))
            .child(
                div()
                    .flex_1()
                    .min_w(px(SETTINGS_ROW_LABEL_MIN_WIDTH))
                    .flex_basis(px(SETTINGS_ROW_LABEL_MIN_WIDTH))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.render_selectable_text_scoped(
                                "settings-row-label",
                                label_key,
                                label,
                                self.tokens.ui.text,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.render_selectable_text_scoped(
                                "settings-row-hint",
                                hint_key,
                                hint,
                                self.tokens.ui.text_muted,
                                cx,
                            )),
                    ),
            )
            .child(control)
            .into_any_element()
    }
}
