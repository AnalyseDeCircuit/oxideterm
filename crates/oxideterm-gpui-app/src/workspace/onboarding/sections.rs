use super::*;
use oxideterm_gpui_ui::{button::ButtonVariant, checkbox};

impl WorkspaceApp {
    pub(in crate::workspace) fn render_onboarding_welcome(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .px(px(32.0))
            .pt(px(32.0))
            .pb(px(24.0))
            .flex()
            .flex_col()
            .gap(px(20.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_size(px(30.0))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(rgb(self.tokens.ui.text_heading))
                                    .child(self.i18n.t("onboarding.welcome")),
                            )
                            .child(
                                div()
                                    .w(px(3.0))
                                    .h(px(21.0))
                                    .rounded(px(2.0))
                                    .bg(rgba((self.tokens.ui.text << 8) | 0x66)),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("onboarding.subtitle")),
                    ),
            )
            .child(self.onboarding_info_card(None, "onboarding.project_intro", None, false, cx))
            .child(div().grid().grid_cols(2).gap(px(8.0)).children([
                self.onboarding_feature_tile(LucideIcon::Zap, "highlight_performance", cx),
                self.onboarding_feature_tile(LucideIcon::Lock, "highlight_security_arch", cx),
                self.onboarding_feature_tile(LucideIcon::Cpu, "highlight_crossplatform", cx),
                self.onboarding_feature_tile(LucideIcon::Puzzle, "highlight_extensible", cx),
            ]))
            .child(self.onboarding_language_picker(cx))
            .into_any_element()
    }

    pub(in crate::workspace) fn onboarding_language_picker(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self.settings_store.settings().general.language;
        let mut grid = div().grid().grid_cols(4).gap(px(6.0));
        for (language, label) in ONBOARDING_LANGUAGES {
            let is_selected = language == selected;
            grid = grid.child(
                div()
                    .px(px(12.0))
                    .py(px(8.0))
                    .rounded(px(self.tokens.radii.sm))
                    .border_1()
                    .border_color(if is_selected {
                        rgb(self.tokens.ui.accent)
                    } else {
                        rgb(self.tokens.ui.border)
                    })
                    .bg(if is_selected {
                        rgb(self.tokens.ui.accent)
                    } else {
                        rgb(self.tokens.ui.bg_card)
                    })
                    .text_color(if is_selected {
                        rgb(self.tokens.ui.accent_text)
                    } else {
                        rgb(self.tokens.ui.text)
                    })
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .cursor(CursorStyle::PointingHand)
                    .child(label)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.edit_settings(|settings| settings.general.language = language, cx);
                            cx.stop_propagation();
                        }),
                    ),
            );
        }
        self.onboarding_section(
            LucideIcon::Home,
            "onboarding.select_language",
            None,
            grid.into_any_element(),
        )
    }

    pub(in crate::workspace) fn render_onboarding_disclaimer(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .px(px(32.0))
            .pt(px(24.0))
            .pb(px(24.0))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(self.onboarding_step_heading(
                LucideIcon::FileText,
                "onboarding.disclaimer_title",
                "onboarding.disclaimer_desc",
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .child(self.onboarding_info_card(
                        Some((LucideIcon::AlertTriangle, self.tokens.ui.warning)),
                        "onboarding.disclaimer_no_warranty",
                        Some("onboarding.disclaimer_no_warranty_text"),
                        false,
                        cx,
                    ))
                    .child(self.onboarding_info_card(
                        Some((LucideIcon::Shield, self.tokens.ui.info)),
                        "onboarding.disclaimer_data_security",
                        Some("onboarding.disclaimer_data_security_text"),
                        false,
                        cx,
                    ))
                    .child(self.onboarding_info_card(
                        Some((LucideIcon::Brain, self.tokens.ui.accent_secondary)),
                        "onboarding.disclaimer_ai",
                        Some("onboarding.disclaimer_ai_text"),
                        false,
                        cx,
                    )),
            )
            .child(self.onboarding_clickable_card(
                LucideIcon::ExternalLink,
                self.i18n.t("onboarding.disclaimer_gpl_note"),
                |this, cx| this.open_onboarding_disclaimer(cx),
                cx,
            ))
            .child(
                div()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .bg(rgb(self.tokens.ui.bg_card))
                    .p(px(12.0))
                    .cursor(CursorStyle::PointingHand)
                    .child(checkbox::checkbox(
                        &self.tokens,
                        self.i18n.t("onboarding.disclaimer_accept"),
                        self.onboarding.disclaimer_accepted,
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.onboarding.disclaimer_accepted =
                                !this.onboarding.disclaimer_accepted;
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_onboarding_appearance(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.settings_store.settings();
        div()
            .px(px(32.0))
            .pt(px(24.0))
            .pb(px(24.0))
            .flex()
            .flex_col()
            .gap(px(20.0))
            .child(self.onboarding_step_heading(
                LucideIcon::Monitor,
                "onboarding.appearance_title",
                "onboarding.appearance_desc",
            ))
            .child(self.onboarding_theme_picker(cx))
            .child(div().h(px(1.0)).bg(rgb(self.tokens.ui.border)))
            .child(self.onboarding_font_picker(cx))
            .child(self.onboarding_tip(
                "onboarding.tip_settings",
                &[("shortcut", platform_cmd(", "))],
            ))
            .child(
                div()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .bg(rgb(theme_by_id(&settings.terminal.theme)
                        .terminal
                        .background))
                    .p(px(16.0))
                    .font_family(SharedString::from(
                        settings
                            .terminal
                            .font_family
                            .terminal_family_name(&settings.terminal.custom_font_family),
                    ))
                    .text_size(px(settings.terminal.font_size as f32))
                    .text_color(rgb(theme_by_id(&settings.terminal.theme)
                        .terminal
                        .foreground))
                    .child("ABCDEFG abcdefg 0123456789")
                    .child(
                        div()
                            .text_color(rgb(theme_by_id(&settings.terminal.theme).terminal.green))
                            .child("天地玄黄 The quick brown fox"),
                    ),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn onboarding_theme_picker(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected_theme = self.settings_store.settings().terminal.theme.clone();
        let mut grid = div().grid().grid_cols(4).gap(px(8.0));
        for theme_id in ONBOARDING_THEME_IDS {
            let selected = selected_theme == theme_id;
            let terminal_theme = theme_by_id(theme_id).terminal;
            let card_radius = self.tokens.radii.md;
            grid = grid.child(
                div()
                    .w_full()
                    .h(px(ONBOARDING_THEME_CARD_HEIGHT))
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .rounded(px(self.tokens.radii.md))
                    .border_2()
                    .border_color(if selected {
                        rgb(self.tokens.ui.accent)
                    } else {
                        rgb(self.tokens.ui.border)
                    })
                    .cursor(CursorStyle::PointingHand)
                    .child(
                        // GPUI does not always clip child backgrounds to the parent radius, so
                        // each painted segment carries the matching Tauri card corner radius.
                        div()
                            .h(px(ONBOARDING_THEME_PREVIEW_HEIGHT))
                            .w_full()
                            .flex()
                            .flex_col()
                            .rounded_t(px(card_radius))
                            .p(px(10.0))
                            .bg(rgb(terminal_theme.background))
                            .child(div().flex().gap(px(6.0)).mb(px(6.0)).children([
                                traffic_dot(terminal_theme.red),
                                traffic_dot(terminal_theme.yellow),
                                traffic_dot(terminal_theme.green),
                            ]))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(3.0))
                                    .font_family(SharedString::from(
                                        FontFamily::Jetbrains.terminal_family_name(""),
                                    ))
                                    .text_size(px(10.0))
                                    .text_color(rgb(terminal_theme.foreground))
                                    .child(
                                        div()
                                            .text_color(rgb(terminal_theme.green))
                                            .child("$ echo \"hi\""),
                                    )
                                    .child(
                                        div()
                                            .w(px(46.0))
                                            .h(px(2.0))
                                            .rounded(px(1.0))
                                            .bg(rgb(terminal_theme.blue)),
                                    )
                                    .child(
                                        div()
                                            .w(px(64.0))
                                            .h(px(2.0))
                                            .rounded(px(1.0))
                                            .bg(rgb(terminal_theme.bright_black)),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .h(px(32.0))
                            .w_full()
                            .flex()
                            .items_center()
                            .px(px(10.0))
                            .border_t_1()
                            .border_color(rgb(self.tokens.ui.border))
                            .rounded_b(px(card_radius))
                            .bg(rgb(self.tokens.ui.bg_card))
                            .text_size(px(11.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(format_theme_label(theme_id)),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.edit_settings(
                                |settings| settings.terminal.theme = theme_id.to_string(),
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ),
            );
        }
        self.onboarding_section(
            LucideIcon::Monitor,
            "onboarding.select_theme",
            Some("onboarding.theme_hint"),
            grid.into_any_element(),
        )
    }

    pub(in crate::workspace) fn onboarding_font_picker(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.settings_store.settings();
        let mut grid = div().grid().grid_cols(2).gap(px(8.0));
        for (family, label, bundled) in ONBOARDING_FONT_OPTIONS {
            let selected = settings.terminal.font_family == family;
            grid = grid.child(
                div()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(if selected {
                        rgb(self.tokens.ui.accent)
                    } else {
                        rgb(self.tokens.ui.border)
                    })
                    .bg(if selected {
                        rgba((self.tokens.ui.accent << 8) | ONBOARDING_ACCENT_SUBTLE_ALPHA)
                    } else {
                        rgb(self.tokens.ui.bg_card)
                    })
                    .px(px(12.0))
                    .py(px(10.0))
                    .flex()
                    .justify_between()
                    .items_center()
                    .cursor(CursorStyle::PointingHand)
                    .child(
                        div()
                            .font_family(SharedString::from(family.terminal_family_name("")))
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(self.tokens.ui.text))
                            .child(label),
                    )
                    .when(bundled, |row| {
                        row.child(
                            div()
                                .text_size(px(10.0))
                                .text_color(rgb(self.tokens.ui.success))
                                .child("✓"),
                        )
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.edit_settings(
                                |settings| settings.terminal.font_family = family,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ),
            );
        }
        let font_size = settings.terminal.font_size;
        self.onboarding_section(
            LucideIcon::Terminal,
            "onboarding.select_font",
            Some("onboarding.font_hint"),
            div()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(grid)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                .text_color(rgb(self.tokens.ui.text))
                                .child(self.i18n.t("onboarding.font_size")),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(self.onboarding_font_size_button("-", -1, cx))
                                .child(
                                    div()
                                        .min_w(px(48.0))
                                        .text_align(gpui::TextAlign::Center)
                                        .font_family(SharedString::from(
                                            FontFamily::Jetbrains.terminal_family_name(""),
                                        ))
                                        .text_color(rgb(self.tokens.ui.text_muted))
                                        .child(format!("{font_size}px")),
                                )
                                .child(self.onboarding_font_size_button("+", 1, cx)),
                        ),
                )
                .into_any_element(),
        )
    }

    pub(in crate::workspace) fn onboarding_font_size_button(
        &self,
        label: &'static str,
        delta: i64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .size(px(28.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_card))
            .cursor(CursorStyle::PointingHand)
            .child(label)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.edit_settings(
                        move |settings| {
                            settings.terminal.font_size =
                                (settings.terminal.font_size + delta).clamp(8, 32);
                        },
                        cx,
                    );
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_onboarding_workflow(
        &self,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        let workflows = [
            (LucideIcon::Server, "workflow_connect"),
            (LucideIcon::Terminal, "workflow_terminal"),
            (LucideIcon::FolderOpen, "workflow_sftp"),
            (LucideIcon::Network, "workflow_forwarding"),
            (LucideIcon::FileCode, "workflow_ide"),
        ];
        let mut list = div().flex().flex_col();
        for (index, (icon, key)) in workflows.into_iter().enumerate() {
            list = list.child(self.onboarding_timeline_item(index + 1, icon, key, index < 4));
        }
        div()
            .px(px(24.0))
            .pt(px(24.0))
            .pb(px(24.0))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(self.onboarding_step_heading(
                LucideIcon::Network,
                "onboarding.workflow_title",
                "onboarding.workflow_desc",
            ))
            .child(list)
            .child(self.onboarding_tip("onboarding.tip_multiplexing", &[]))
            .into_any_element()
    }

    pub(in crate::workspace) fn render_onboarding_features(
        &self,
        _window: &Window,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        let mod_key = if cfg!(target_os = "macos") {
            "⌘K"
        } else {
            "Ctrl+K"
        };
        let features = [
            (LucideIcon::Terminal, "cmd_palette", Some(mod_key), true),
            (LucideIcon::Bot, "ai_chat", None, false),
            (LucideIcon::FolderOpen, "sftp", None, false),
            (LucideIcon::HardDrive, "local_file_manager", None, false),
            (LucideIcon::Network, "port_forwarding", None, false),
            (LucideIcon::RefreshCw, "reconnect", None, false),
            (LucideIcon::Puzzle, "plugin_system", None, false),
            (LucideIcon::FileCode, "custom_themes", None, false),
            (LucideIcon::Rocket, "launchpad", Some("macOS"), false),
            (LucideIcon::Monitor, "wsl_graphics", Some("Windows"), false),
            (LucideIcon::ArrowUpDown, "multiplexing", None, false),
            (LucideIcon::Shield, "security", None, false),
        ];
        let mut grid = div().grid().grid_cols(2).gap(px(10.0));
        for (icon, key, badge, highlight) in features {
            grid = grid.child(self.onboarding_feature_card(icon, key, badge, highlight));
        }
        div()
            .px(px(32.0))
            .pt(px(24.0))
            .pb(px(24.0))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(self.onboarding_step_heading(
                LucideIcon::Shield,
                "onboarding.features",
                "onboarding.features_desc",
            ))
            .child(grid)
            .into_any_element()
    }

    pub(in crate::workspace) fn render_onboarding_ai_intro(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .px(px(32.0))
            .pt(px(24.0))
            .pb(px(24.0))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(self.onboarding_step_heading(
                LucideIcon::Sparkles,
                "onboarding.ai_tools_title",
                "onboarding.ai_tools_desc",
            ))
            .child(self.onboarding_info_card(
                Some((LucideIcon::Bot, self.tokens.ui.accent_secondary)),
                "onboarding.ai_tools_oxidesens",
                Some("onboarding.ai_tools_oxidesens_desc"),
                false,
                cx,
            ))
            .child(self.onboarding_capability_grid(
                "onboarding.ai_tools_capabilities",
                &[
                    (LucideIcon::Terminal, "ai_tools_cap_sidebar"),
                    (LucideIcon::Zap, "ai_tools_cap_inline"),
                    (LucideIcon::Bot, "ai_tools_cap_agent"),
                    (LucideIcon::Key, "ai_tools_cap_byok"),
                ],
            ))
            .child(self.onboarding_capability_list(
                "onboarding.ai_tools_privacy",
                &[
                    (LucideIcon::Lock, "ai_tools_privacy_local"),
                    (LucideIcon::Key, "ai_tools_privacy_keys"),
                    (LucideIcon::Shield, "ai_tools_privacy_context"),
                ],
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn render_onboarding_ai_setup(
        &self,
        _window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let shortcut = if cfg!(target_os = "macos") {
            "⌘K"
        } else {
            "Ctrl+K"
        };
        div()
            .px(px(32.0))
            .pt(px(24.0))
            .pb(px(24.0))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(self.onboarding_step_heading(
                LucideIcon::Settings,
                "onboarding.ai_setup_title",
                "onboarding.ai_setup_desc",
            ))
            .child(self.onboarding_toggle_card(
                "onboarding.ai_tools_enable",
                "onboarding.ai_tools_enable_hint",
                self.onboarding.ai_opt_in,
                true,
                |this, cx| {
                    this.onboarding.ai_opt_in = !this.onboarding.ai_opt_in;
                    if !this.onboarding.ai_opt_in {
                        this.onboarding.tool_use_opt_in = false;
                    }
                    cx.notify();
                },
                cx,
            ))
            .child(self.onboarding_toggle_card(
                "onboarding.ai_tools_enable_tools",
                "onboarding.ai_tools_enable_tools_hint",
                self.onboarding.tool_use_opt_in,
                self.onboarding.ai_opt_in,
                |this, cx| {
                    if this.onboarding.ai_opt_in {
                        this.onboarding.tool_use_opt_in = !this.onboarding.tool_use_opt_in;
                        cx.notify();
                    }
                },
                cx,
            ))
            .child(self.onboarding_info_card_with_text(
                Some((LucideIcon::Terminal, self.tokens.ui.accent)),
                self.i18n.t("onboarding.ai_tools_cmd_palette"),
                self.onboarding_i18n_with(
                    "onboarding.ai_tools_cmd_palette_desc",
                    &[("shortcut", shortcut.to_string())],
                ),
                true,
            ))
            .child(self.onboarding_tip("onboarding.ai_tools_later_hint", &[]))
            .into_any_element()
    }

    pub(in crate::workspace) fn render_onboarding_cli_companion(
        &self,
        _window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let status = self.settings_page.cli_companion_status.as_ref();
        let installed = status.is_some_and(|status| status.installed);
        let bundled = status.is_some_and(|status| status.bundled);
        let needs_reinstall = status.is_some_and(|status| status.needs_reinstall);
        let status_label = if self.settings_page.cli_companion_loading {
            self.i18n.t("settings_view.general.cli_checking")
        } else if installed && needs_reinstall {
            self.i18n.t("onboarding.cli_step_reinstall_required")
        } else if installed {
            self.i18n.t("onboarding.cli_step_installed")
        } else if !bundled && status.is_some() {
            self.i18n.t("onboarding.cli_step_not_bundled")
        } else {
            self.i18n.t("settings_view.general.cli_not_installed")
        };
        let status_color = if installed && !needs_reinstall {
            self.tokens.ui.success
        } else if self.settings_page.cli_companion_loading || needs_reinstall {
            self.tokens.ui.warning
        } else {
            self.tokens.ui.text_muted
        };
        let path = status
            .and_then(|status| status.install_path.clone())
            .unwrap_or_default();

        div()
            .px(px(32.0))
            .pt(px(24.0))
            .pb(px(24.0))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(self.onboarding_step_heading(
                LucideIcon::Terminal,
                "onboarding.cli_step_title",
                "onboarding.cli_step_desc",
            ))
            .child(self.onboarding_info_card(
                Some((LucideIcon::Terminal, self.tokens.ui.accent)),
                "onboarding.cli_step_what",
                Some("onboarding.cli_step_what_text"),
                false,
                cx,
            ))
            .child(self.onboarding_capability_grid(
                "onboarding.cli_step_capabilities",
                &[
                    (LucideIcon::Server, "cli_step_cap_sessions"),
                    (LucideIcon::Network, "cli_step_cap_forward"),
                    (LucideIcon::Bot, "cli_step_cap_ai"),
                    (LucideIcon::Cpu, "cli_step_cap_status"),
                ],
            ))
            .child(
                div()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .bg(rgb(self.tokens.ui.bg_card))
                    .p(px(14.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(Self::render_lucide_icon(
                                        if installed {
                                            LucideIcon::CheckCircle
                                        } else {
                                            LucideIcon::AlertTriangle
                                        },
                                        16.0,
                                        rgb(status_color),
                                    ))
                                    .child(
                                        div()
                                            .text_size(px(self.tokens.metrics.ui_text_xs))
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(rgb(status_color))
                                            .child(status_label),
                                    ),
                            )
                            .when(!path.is_empty(), |column| {
                                column.child(
                                    div()
                                        .text_size(px(11.0))
                                        .font_family(SharedString::from(
                                            FontFamily::Jetbrains.terminal_family_name(""),
                                        ))
                                        .text_color(rgb(self.tokens.ui.text_muted))
                                        .truncate()
                                        .child(self.onboarding_i18n_with(
                                            "onboarding.cli_step_installed_at",
                                            &[("path", path.clone())],
                                        )),
                                )
                            }),
                    )
                    .child(self.onboarding_button(
                        self.i18n.t("settings_view.general.cli_companion"),
                        Some(LucideIcon::Settings),
                        ButtonVariant::Outline,
                        false,
                        |this, window, cx| this.onboarding_open_cli_settings(window, cx),
                        cx,
                    )),
            )
            .child(self.onboarding_tip("onboarding.cli_step_skip_hint", &[]))
            .when(self.settings_page.cli_companion_status.is_none(), |body| {
                body.child(self.onboarding_clickable_card(
                    LucideIcon::RefreshCw,
                    self.i18n.t("settings_view.help.retry"),
                    |this, cx| this.refresh_cli_companion_status(cx),
                    cx,
                ))
            })
            .into_any_element()
    }

    pub(in crate::workspace) fn render_onboarding_quick_start(
        &self,
        _window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_mac = cfg!(target_os = "macos");
        let mod_key = if is_mac { "⌘" } else { "Ctrl" };
        div()
            .px(px(32.0))
            .pt(px(24.0))
            .pb(px(24.0))
            .flex()
            .flex_col()
            .gap(px(20.0))
            .child(self.onboarding_step_heading(
                LucideIcon::Sparkles,
                "onboarding.quick_start",
                "onboarding.quick_start_desc",
            ))
            .child(
                div()
                    .grid()
                    .grid_cols(3)
                    .gap(px(12.0))
                    .child(self.onboarding_action_card(
                        LucideIcon::Terminal,
                        "onboarding.open_terminal",
                        "onboarding.open_terminal_desc",
                        |this, window, cx| this.onboarding_open_terminal(window, cx),
                        cx,
                    ))
                    .child(self.onboarding_action_card(
                        LucideIcon::Plus,
                        "onboarding.new_connection",
                        "onboarding.new_connection_desc",
                        |this, window, cx| this.onboarding_open_new_connection(window, cx),
                        cx,
                    ))
                    .child(self.onboarding_import_card(cx)),
            )
            .child(div().h(px(1.0)).bg(rgb(self.tokens.ui.border)))
            .child(self.onboarding_shortcut_grid(mod_key, is_mac))
            .child(self.onboarding_tip(
                "onboarding.tip_shortcuts",
                &[("shortcut", if is_mac { "⌘/" } else { "Ctrl+/" }.to_string())],
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn onboarding_shortcut_grid(
        &self,
        mod_key: &str,
        is_mac: bool,
    ) -> AnyElement {
        let copy = if is_mac { "⌘⇧C" } else { "Ctrl+Shift+C" };
        let split_right = if is_mac { "⌘E" } else { "Ctrl+E" };
        let split_down = if is_mac { "⌘D" } else { "Ctrl+D" };
        let close_tab = if is_mac { "⌘W" } else { "Ctrl+W" };
        let groups: [(&str, [(&str, &str); 4]); 3] = [
            (
                "shortcuts_navigation",
                [
                    ("shortcut_command_palette", "K"),
                    ("shortcut_new_connection", "N"),
                    ("shortcut_new_tab", "T"),
                    ("shortcut_search", "F"),
                ],
            ),
            (
                "shortcuts_terminal",
                [
                    (
                        "shortcut_ai_chat",
                        if is_mac { "⌘⇧A" } else { "Ctrl+Shift+A" },
                    ),
                    ("shortcut_copy", copy),
                    ("shortcut_split_right", split_right),
                    ("shortcut_split_down", split_down),
                ],
            ),
            (
                "shortcuts_window",
                [
                    ("shortcut_close_tab", close_tab),
                    (
                        "shortcut_zoom",
                        if is_mac { "⌘=/⌘-" } else { "Ctrl+=/Ctrl+-" },
                    ),
                    ("shortcut_command_palette", "K"),
                    ("shortcut_new_connection", "N"),
                ],
            ),
        ];
        let mut grid = div().grid().grid_cols(3).gap(px(16.0));
        for (title, rows) in groups {
            let mut column = div().flex().flex_col().gap(px(8.0)).child(
                div()
                    .text_size(px(10.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(&format!("onboarding.{title}")).to_uppercase()),
            );
            for (desc, key) in rows {
                let key_text = if key.len() == 1 {
                    format!("{mod_key}{key}")
                } else {
                    key.to_string()
                };
                column = column.child(self.onboarding_shortcut_row(&key_text, desc));
            }
            grid = grid.child(column);
        }
        self.onboarding_section(
            LucideIcon::Keyboard,
            "onboarding.shortcuts_title",
            Some("onboarding.shortcuts_hint"),
            grid.into_any_element(),
        )
    }

    pub(in crate::workspace) fn onboarding_shortcut_row(
        &self,
        key: &str,
        desc_key: &str,
    ) -> AnyElement {
        div()
            .flex()
            .items_start()
            .gap(px(8.0))
            .child(
                div()
                    .rounded(px(self.tokens.radii.xs))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .bg(rgb(self.tokens.ui.bg))
                    .px(px(6.0))
                    .py(px(2.0))
                    .font_family(SharedString::from(
                        FontFamily::Jetbrains.terminal_family_name(""),
                    ))
                    .text_size(px(10.0))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(key.to_string()),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t(&format!("onboarding.{desc_key}"))),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn onboarding_import_card(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (icon, text, disabled) = match self.onboarding.import_state {
            OnboardingImportState::Done => (
                LucideIcon::Check,
                self.onboarding_i18n_with(
                    "onboarding.import_ssh_done",
                    &[("count", self.onboarding.imported_count.to_string())],
                ),
                true,
            ),
            OnboardingImportState::Loading => (
                LucideIcon::LoaderCircle,
                self.i18n.t("onboarding.importing"),
                true,
            ),
            OnboardingImportState::Idle => match self.onboarding.host_count {
                Some(0) => (
                    LucideIcon::Download,
                    self.i18n.t("onboarding.import_ssh_none"),
                    true,
                ),
                Some(count) => (
                    LucideIcon::Download,
                    self.onboarding_i18n_with(
                        "onboarding.import_ssh_desc",
                        &[("count", count.to_string())],
                    ),
                    false,
                ),
                None => (
                    LucideIcon::LoaderCircle,
                    self.i18n.t("onboarding.importing"),
                    true,
                ),
            },
        };
        self.onboarding_action_card_with_detail(
            icon,
            self.i18n.t("onboarding.import_ssh"),
            text,
            disabled,
            |this, _window, cx| this.import_onboarding_ssh_hosts(cx),
            cx,
        )
    }

    pub(in crate::workspace) fn onboarding_action_card(
        &self,
        icon: LucideIcon,
        title_key: &str,
        desc_key: &str,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.onboarding_action_card_with_detail(
            icon,
            self.i18n.t(title_key),
            self.i18n.t(desc_key),
            false,
            action,
            cx,
        )
    }

    pub(in crate::workspace) fn onboarding_action_card_with_detail(
        &self,
        icon: LucideIcon,
        title: String,
        detail: String,
        disabled: bool,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .min_h(px(132.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_card))
            .px(px(16.0))
            .py(px(20.0))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(12.0))
            .opacity(if disabled {
                ONBOARDING_DISABLED_OPACITY
            } else {
                1.0
            })
            .cursor(if disabled {
                CursorStyle::OperationNotAllowed
            } else {
                CursorStyle::PointingHand
            })
            .child(
                div()
                    .h(px(32.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Self::render_lucide_icon(
                        icon,
                        24.0,
                        rgb(self.tokens.ui.text_muted),
                    )),
            )
            .child(
                div()
                    .text_align(gpui::TextAlign::Center)
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(detail),
                    ),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    if !disabled {
                        action(this, window, cx);
                    }
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }
}
