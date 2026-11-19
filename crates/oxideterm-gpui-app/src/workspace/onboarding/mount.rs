use super::*;
use crate::workspace::settings::{APPEARANCE_BORDER_RADIUS_MAX, APPEARANCE_BORDER_RADIUS_MIN};
use crate::workspace::tabs::effective_shortcut_label;
use oxideterm_gpui_onboarding::{
    OnboardingCliSnapshot, OnboardingFrame, OnboardingImportStatus, OnboardingRequest,
    OnboardingShortcutItem, OnboardingThemeTarget, OnboardingView, OnboardingViewBindings,
};
use oxideterm_gpui_settings_view::theme_picker::appearance_theme_palette;
use oxideterm_gpui_settings_view::{animation_label, animation_options};
use oxideterm_settings_model::{
    ThemeTarget, assign_current_appearance, custom_theme_display_name, theme_ui_colors,
};

const ONBOARDING_BRAND_ICON_SIZE: f32 = 44.0;

impl WorkspaceApp {
    pub(in crate::workspace) fn onboarding_view_bindings(
        cx: &Context<Self>,
    ) -> OnboardingViewBindings {
        let owner = cx.entity().downgrade();
        OnboardingViewBindings {
            frame: Rc::new({
                let owner = owner.clone();
                move |cx| {
                    let app = owner.upgrade()?;
                    Some(app.read(cx).onboarding_frame(cx))
                }
            }),
            dispatch: Rc::new({
                let owner = owner.clone();
                move |request, window, cx| {
                    let _ = owner.update(cx, |app, cx| {
                        app.handle_onboarding_request(request, window, cx);
                    });
                }
            }),
            language_select: Rc::new({
                let owner = owner.clone();
                move |_tokens, label, cx| {
                    let Some(owner) = owner.upgrade() else {
                        return div().into_any_element();
                    };
                    let label = label.to_owned();
                    owner.update(cx, |app, cx| {
                        app.settings_select_control(
                            SettingsSelect::Language,
                            label,
                            false,
                            Some(156.0),
                            cx,
                        )
                    })
                }
            }),
            theme_select: Rc::new({
                let owner = owner.clone();
                move |target, settings, cx| {
                    let Some(owner) = owner.upgrade() else {
                        return div().into_any_element();
                    };
                    let settings = settings.clone();
                    owner.update(cx, |app, cx| {
                        let edit_dark = app
                            .settings_workspace
                            .read(cx)
                            .appearance_edit_dark
                            .unwrap_or(app.system_dark);
                        let (model_target, select_id) =
                            match (target, app.settings_workspace.read(cx).appearance_edit_dark) {
                                (OnboardingThemeTarget::Application, Some(false)) => (
                                    ThemeTarget::Application,
                                    SettingsSelect::AppearanceThemeLight,
                                ),
                                (OnboardingThemeTarget::Application, Some(true)) => (
                                    ThemeTarget::Application,
                                    SettingsSelect::AppearanceThemeDark,
                                ),
                                (OnboardingThemeTarget::Application, None) => {
                                    (ThemeTarget::Application, SettingsSelect::AppearanceTheme)
                                }
                                (OnboardingThemeTarget::Terminal, Some(false)) => (
                                    ThemeTarget::Terminal,
                                    SettingsSelect::AppearanceTerminalThemeLight,
                                ),
                                (OnboardingThemeTarget::Terminal, Some(true)) => (
                                    ThemeTarget::Terminal,
                                    SettingsSelect::AppearanceTerminalThemeDark,
                                ),
                                (OnboardingThemeTarget::Terminal, None) => (
                                    ThemeTarget::Terminal,
                                    SettingsSelect::AppearanceTerminalTheme,
                                ),
                            };
                        let selected_id = model_target.scheme_id(&settings, edit_dark);
                        app.settings_select_control(
                            select_id,
                            custom_theme_display_name(&settings, selected_id),
                            false,
                            Some(196.0),
                            cx,
                        )
                    })
                }
            }),
            custom_font_input: Rc::new({
                let owner = owner.clone();
                move |settings, cx| {
                    let Some(owner) = owner.upgrade() else {
                        return div().into_any_element();
                    };
                    let value = settings.terminal.custom_font_family.clone();
                    owner.update(cx, |app, cx| {
                        app.settings_text_input_control_fill(
                            SettingsInput::TerminalCustomFontFamily,
                            value,
                            "'Sarasa Fixed SC', 'Fira Code', monospace".to_string(),
                            cx,
                        )
                    })
                }
            }),
            radius_slider: Rc::new({
                let owner = owner.clone();
                move |radius, cx| {
                    let Some(owner) = owner.upgrade() else {
                        return div().into_any_element();
                    };
                    owner.update(cx, |app, cx| {
                        app.appearance_slider_control(
                            SettingsSlider::OnboardingBorderRadius,
                            SelectAnchorId::OnboardingBorderRadiusSlider,
                            APPEARANCE_BORDER_RADIUS_MIN,
                            APPEARANCE_BORDER_RADIUS_MAX,
                            radius as f32,
                            cx,
                        )
                    })
                }
            }),
        }
    }

    pub(in crate::workspace) fn notify_onboarding_view(&self, cx: &mut App) {
        if self.onboarding.open {
            gpui::App::notify(cx, self.onboarding_view.entity_id());
        }
    }

    pub(in crate::workspace) fn render_onboarding_tab(&self) -> AnyElement {
        self.onboarding_view.clone().into_any_element()
    }

    pub(in crate::workspace) fn lock_workspace_chrome_when_onboarding_locked(
        &self,
        element: AnyElement,
        locked: bool,
    ) -> AnyElement {
        if !locked {
            return element;
        }
        div()
            .relative()
            .h_full()
            .flex_none()
            .child(div().h_full().opacity(0.42).child(element))
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .cursor(CursorStyle::OperationNotAllowed)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                    .on_mouse_down(MouseButton::Middle, |_, _, cx| cx.stop_propagation())
                    .on_scroll(|_, _, cx| cx.stop_propagation()),
            )
            .into_any_element()
    }

    fn onboarding_frame(&self, cx: &App) -> OnboardingFrame {
        let settings = self.settings_store.settings().clone();
        let edit_scheme = self.settings_workspace.read(cx).appearance_edit_dark;
        let preview_target = self
            .open_settings_select
            .and_then(SettingsSelect::theme_target);
        let preview_dark = self
            .open_settings_select
            .and_then(SettingsSelect::theme_system_dark)
            .unwrap_or_else(|| edit_scheme.unwrap_or(self.system_dark));
        let hovered_theme = self.settings_views.theme_picker.read(cx).preview();
        let preview_theme = preview_target.and(hovered_theme);
        let resolve_id = |target: ThemeTarget| {
            if preview_target == Some(target) {
                preview_theme
                    .map(str::to_owned)
                    .unwrap_or_else(|| target.scheme_id(&settings, preview_dark).to_owned())
            } else {
                target.scheme_id(&settings, preview_dark).to_owned()
            }
        };
        let app_id = resolve_id(ThemeTarget::Application);
        let term_id = resolve_id(ThemeTarget::Terminal);
        let mut preview_tokens = ThemeTokens {
            ui: theme_ui_colors(&settings, &app_id),
            terminal: appearance_theme_palette(&settings, &term_id),
            ..self.tokens
        };
        preview_tokens.refresh_palette_metrics();

        let cli = self.settings_workspace.read(cx).cli_companion_snapshot();
        let cli_snapshot = OnboardingCliSnapshot {
            loading: cli.loading,
            installed: cli.status.as_ref().is_some_and(|s| s.installed),
            needs_reinstall: cli.status.as_ref().is_some_and(|s| s.needs_reinstall),
            bundled: cli.status.as_ref().is_some_and(|s| s.bundled),
            has_status: cli.status.is_some(),
            install_path: cli.status.and_then(|s| s.install_path),
        };

        let overrides = &settings.keybindings.overrides;
        const SHORTCUTS: [(&str, &str); 6] = [
            ("app.commandPalette", "onboarding.shortcut_command_palette"),
            ("app.newConnection", "onboarding.shortcut_new_connection"),
            ("app.newTerminal", "onboarding.shortcut_new_tab"),
            ("app.toggleAiSidebar", "onboarding.shortcut_ai_chat"),
            ("app.splitRight", "onboarding.shortcut_split_right"),
            ("app.splitDown", "onboarding.shortcut_split_down"),
        ];
        let shortcuts = SHORTCUTS
            .into_iter()
            .filter_map(|(action_id, label_key)| {
                effective_shortcut_label(action_id, overrides).map(|key_label| {
                    OnboardingShortcutItem {
                        label_key,
                        key_label,
                    }
                })
            })
            .collect();

        let sidebar_occupied_width = if self.sidebar_collapsed {
            self.tokens.metrics.activity_bar_width
        } else {
            self.sidebar_width
        };

        OnboardingFrame {
            tokens: self.tokens,
            preview_tokens,
            preview_theme_name: custom_theme_display_name(&settings, &app_id),
            i18n: self.i18n.clone(),
            ui_font_family: settings_ui_font_family(&settings.appearance.ui_font_family),
            mono_font_family: settings_mono_font_family(&settings),
            language_label: self.language_label(settings.general.language),
            animation_options: animation_options()
                .iter()
                .map(|&speed| (speed, animation_label(speed, &self.i18n)))
                .collect(),
            app_icon_source: crate::app_icon::app_icon_trimmed_source_sized(
                settings.appearance.app_icon,
                ONBOARDING_BRAND_ICON_SIZE,
            ),
            sidebar_occupied_width,
            has_background: self.window_background_preferences().is_some(),
            system_dark: self.system_dark,
            edit_scheme,
            open_settings_select_active: self.open_settings_select.is_some(),
            step: self.onboarding.step,
            disclaimer_accepted: self.onboarding.disclaimer_accepted,
            save_failed: self.onboarding.save_failed,
            ai_opt_in: self.onboarding.ai_opt_in,
            tool_use_opt_in: self.onboarding.tool_use_opt_in,
            import_status: match self.onboarding.import_state {
                OnboardingImportState::Idle => OnboardingImportStatus::Idle,
                OnboardingImportState::Loading => OnboardingImportStatus::Loading,
                OnboardingImportState::Done => OnboardingImportStatus::Done,
            },
            imported_count: self.onboarding.imported_count,
            host_count: self.onboarding.host_count,
            cli: cli_snapshot,
            shortcuts,
            settings,
        }
    }

    fn handle_onboarding_request(
        &mut self,
        request: OnboardingRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match request {
            OnboardingRequest::ToggleDisclaimer => self.toggle_onboarding_disclaimer_acceptance(cx),
            OnboardingRequest::OpenLegalNotice => self.open_help_legal_notice(cx),
            OnboardingRequest::GoToStep(step) => self.onboarding_go_to_step(step, cx),
            OnboardingRequest::Next => self.onboarding_next(cx),
            OnboardingRequest::Back => self.onboarding_back(cx),
            OnboardingRequest::SkipToLaunch => self.onboarding_skip_to_quick_start(cx),
            OnboardingRequest::Complete => self.complete_onboarding(cx),
            OnboardingRequest::CloseOpenSelect => {
                if self.open_settings_select.is_some() {
                    self.close_settings_select();
                    self.clear_settings_select_anchors();
                    cx.notify();
                }
            }
            OnboardingRequest::SetAppearanceMode(scheme) => {
                self.close_settings_select();
                self.blur_text_inputs(cx);
                self.finish_settings_slider_drag(cx);
                self.settings_workspace
                    .update(cx, |settings, cx| settings.finish_background_blur_drag(cx));
                self.change_appearance_mode(scheme, cx);
            }
            OnboardingRequest::SetEditScheme(dark) => {
                self.blur_text_inputs(cx);
                self.finish_settings_slider_drag(cx);
                self.settings_workspace.update(cx, |settings, cx| {
                    settings.appearance_edit_dark = Some(dark);
                    cx.notify();
                });
            }
            OnboardingRequest::AssignCurrentScheme(selected) => {
                let scheme = oxideterm_settings::AppearanceScheme::from_dark(selected);
                let system_dark = self.system_dark;
                self.edit_settings(
                    |settings| assign_current_appearance(settings, scheme, system_dark),
                    cx,
                );
            }
            OnboardingRequest::SetFontFamily(family) => {
                self.edit_settings(|settings| settings.terminal.font_family = family, cx);
            }
            OnboardingRequest::AdjustFontSize(delta) => {
                self.edit_settings(
                    |settings| {
                        settings.terminal.font_size =
                            (settings.terminal.font_size + delta).clamp(8, 32);
                    },
                    cx,
                );
            }
            OnboardingRequest::SetCornerStyle(style) => {
                self.edit_settings(|settings| settings.appearance.corner_style = style, cx);
            }
            OnboardingRequest::SetAnimationSpeed(speed) => {
                self.edit_settings(|settings| settings.appearance.animation_speed = speed, cx);
            }
            OnboardingRequest::ToggleAiOptIn => {
                self.onboarding.ai_opt_in = !self.onboarding.ai_opt_in;
                if !self.onboarding.ai_opt_in {
                    self.onboarding.tool_use_opt_in = false;
                }
                self.notify_onboarding_view(cx);
            }
            OnboardingRequest::ToggleToolUseOptIn => {
                if self.onboarding.ai_opt_in {
                    self.onboarding.tool_use_opt_in = !self.onboarding.tool_use_opt_in;
                    self.notify_onboarding_view(cx);
                }
            }
            OnboardingRequest::ImportSshHosts => self.import_onboarding_ssh_hosts(cx),
            OnboardingRequest::RefreshCliStatus => self.refresh_cli_companion_status(cx),
            OnboardingRequest::OpenTerminal => self.onboarding_open_terminal(window, cx),
            OnboardingRequest::OpenNewConnection => self.onboarding_open_new_connection(window, cx),
            OnboardingRequest::OpenConnectionImporters => {
                self.onboarding_open_connection_importers(window, cx);
            }
            OnboardingRequest::OpenCliSettings => self.onboarding_open_cli_settings(window, cx),
        }
    }
}

pub(in crate::workspace) fn create_onboarding_view(
    settings_workspace: &Entity<crate::workspace::settings::SettingsWorkspaceEntity>,
    theme_picker: &Entity<oxideterm_gpui_settings_view::theme_picker::SettingsThemePicker>,
    cx: &mut Context<WorkspaceApp>,
) -> Entity<OnboardingView> {
    let bindings = WorkspaceApp::onboarding_view_bindings(cx);
    let weak_owner = cx.entity().downgrade();
    cx.new(|cx| {
        let mut view = OnboardingView::new(bindings);
        view.retain_subscription(cx.observe(settings_workspace, {
            let weak_owner = weak_owner.clone();
            move |_, _, cx| {
                if weak_owner
                    .upgrade()
                    .is_some_and(|app| app.read(cx).onboarding.open)
                {
                    cx.notify();
                }
            }
        }));
        view.retain_subscription(cx.observe(theme_picker, {
            let weak_owner = weak_owner.clone();
            move |_, _, cx| {
                if weak_owner
                    .upgrade()
                    .is_some_and(|app| app.read(cx).onboarding.open)
                {
                    cx.notify();
                }
            }
        }));
        view
    })
}
